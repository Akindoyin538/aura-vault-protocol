/**
 * Governance Proposals Routes — Issue #1268
 *
 * GET  /api/v1/governance/proposals          — list non-expired proposals (paginated)
 * GET  /api/v1/governance/proposals/:id      — get single proposal with votes
 * POST /api/v1/governance/proposals/:id/vote — record off-chain vote intent (auth required)
 * GET  /api/v1/governance/proposals/:id/status — real-time on-chain status
 *
 * Closes #1268
 */

import { Router, type Request, type Response } from "express";
import { z } from "zod";
import { authenticate } from "../middleware/authMiddleware.js";
import { validate } from "../validation.js";
import {
  successResponse,
  errorResponse,
  paginatedResponse,
} from "../dto/ApiResponseDto.js";
import { logger } from "../logger.js";
import {
  listProposals,
  getProposalById,
  recordVoteIntent,
  getOnChainProposalStatus,
  type ProposalStatus,
} from "../services/governanceService.js";

export const governanceRouter = Router();

// ── Validation schemas ────────────────────────────────────────────────────────

const VALID_STATUSES: ProposalStatus[] = [
  "active",
  "approved",
  "rejected",
  "executed",
  "expired",
  "cancelled",
];

const listQuerySchema = z.object({
  page: z.coerce.number().int().positive().default(1),
  pageSize: z.coerce.number().int().min(1).max(100).default(20),
  status: z.enum(["active", "approved", "rejected", "executed", "expired", "cancelled"]).optional(),
  includeExpired: z
    .string()
    .optional()
    .transform((v) => v === "true"),
});

const voteBodySchema = z.object({
  support: z.boolean({
    required_error: "support is required",
    invalid_type_error: "support must be a boolean",
  }),
  comment: z
    .string()
    .max(500, "comment must be at most 500 characters")
    .optional(),
});

// ── Helper: validate UUID param ───────────────────────────────────────────────

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isUuid(value: string): boolean {
  return UUID_RE.test(value);
}

// ── GET /api/v1/governance/proposals ─────────────────────────────────────────

/**
 * List all non-expired proposals (default).
 *
 * Query params:
 *   page         - Page number (default 1)
 *   pageSize     - Items per page, 1–100 (default 20)
 *   status       - Filter by lifecycle status
 *   includeExpired - Set to "true" to include expired proposals
 *
 * Response 200: paginated ApiResponseDto
 */
governanceRouter.get(
  "/proposals",
  async (req: Request, res: Response): Promise<void> => {
    const parsed = listQuerySchema.safeParse(req.query);
    if (!parsed.success) {
      const first = parsed.error.issues[0];
      res.status(400).json(
        errorResponse(
          "VALIDATION_ERROR",
          `Invalid query parameter: ${first?.path.join(".") ?? "unknown"} — ${first?.message ?? "invalid"}`,
          parsed.error.issues.map((i) => ({
            field: i.path.join("."),
            message: i.message,
          })),
        ),
      );
      return;
    }

    const { page, pageSize, status, includeExpired } = parsed.data;

    try {
      const result = await listProposals({ page, pageSize, status, includeExpired });
      res
        .status(200)
        .json(paginatedResponse(result.proposals, page, pageSize, result.total));
    } catch (err) {
      logger.error({ err }, "[governance] list proposals failed");
      res
        .status(500)
        .json(errorResponse("INTERNAL_ERROR", "Failed to retrieve governance proposals"));
    }
  },
);

// ── GET /api/v1/governance/proposals/:id ─────────────────────────────────────

/**
 * Get a single proposal with its full vote list.
 *
 * Response 200: { success: true, data: ProposalWithVotes }
 * Response 404: proposal not found
 */
governanceRouter.get(
  "/proposals/:id",
  async (req: Request, res: Response): Promise<void> => {
    const { id } = req.params;

    if (!isUuid(id)) {
      res
        .status(400)
        .json(errorResponse("INVALID_ID", "Proposal ID must be a valid UUID"));
      return;
    }

    try {
      const proposal = await getProposalById(id);

      if (!proposal) {
        res.status(404).json(errorResponse("NOT_FOUND", "Governance proposal not found"));
        return;
      }

      res.status(200).json(successResponse(proposal));
    } catch (err) {
      logger.error({ err, id }, "[governance] get proposal failed");
      res
        .status(500)
        .json(errorResponse("INTERNAL_ERROR", "Failed to retrieve governance proposal"));
    }
  },
);

// ── POST /api/v1/governance/proposals/:id/vote ───────────────────────────────

/**
 * Record an off-chain vote intent for an authenticated user.
 *
 * Requires: Bearer JWT (authenticate middleware)
 *
 * Body: { support: boolean, comment?: string }
 *
 * After recording, the client is guided to sign and broadcast the on-chain
 * governance transaction to finalise the vote.
 *
 * Response 201: { success: true, data: { voteIntent, nextStep } }
 * Response 400: already voted or proposal not active
 * Response 401: not authenticated
 * Response 404: proposal not found
 */
governanceRouter.post(
  "/proposals/:id/vote",
  authenticate,
  validate(voteBodySchema),
  async (req: Request, res: Response): Promise<void> => {
    const { id } = req.params;

    if (!isUuid(id)) {
      res
        .status(400)
        .json(errorResponse("INVALID_ID", "Proposal ID must be a valid UUID"));
      return;
    }

    const { support, comment } = req.body as z.infer<typeof voteBodySchema>;
    const voterAddress = (req as Request & { user: { sub: string } }).user.sub;

    try {
      const voteIntent = await recordVoteIntent({
        proposalId: id,
        voterAddress,
        support,
        comment,
      });

      res.status(201).json(
        successResponse({
          voteIntent,
          nextStep: {
            action: "sign_on_chain",
            message:
              "Your off-chain vote intent has been recorded. " +
              "To finalise, sign and broadcast the governance vote transaction using Freighter.",
            proposalId: id,
            support,
          },
        }),
      );
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code;
      const message = err instanceof Error ? err.message : "Vote failed";

      if (code === "NOT_FOUND") {
        res.status(404).json(errorResponse("NOT_FOUND", message));
        return;
      }
      if (code === "ALREADY_VOTED") {
        res.status(409).json(errorResponse("ALREADY_VOTED", message));
        return;
      }
      if (code === "PROPOSAL_NOT_ACTIVE" || code === "PROPOSAL_EXPIRED") {
        res.status(422).json(errorResponse(code, message));
        return;
      }

      logger.error({ err, id, voterAddress }, "[governance] vote intent failed");
      res.status(500).json(errorResponse("INTERNAL_ERROR", "Failed to record vote intent"));
    }
  },
);

// ── GET /api/v1/governance/proposals/:id/status ──────────────────────────────

/**
 * Query the Soroban contract directly for real-time proposal status.
 *
 * This endpoint does NOT use the database cache — it always reads from
 * the contract so the frontend governance dashboard shows accurate data.
 *
 * Response 200: { success: true, data: OnChainProposalStatus }
 * Response 404: proposal not found in DB
 */
governanceRouter.get(
  "/proposals/:id/status",
  async (req: Request, res: Response): Promise<void> => {
    const { id } = req.params;

    if (!isUuid(id)) {
      res
        .status(400)
        .json(errorResponse("INVALID_ID", "Proposal ID must be a valid UUID"));
      return;
    }

    try {
      const status = await getOnChainProposalStatus(id);
      res.status(200).json(successResponse(status));
    } catch (err) {
      const code = (err as NodeJS.ErrnoException).code;
      if (code === "NOT_FOUND") {
        res.status(404).json(errorResponse("NOT_FOUND", "Governance proposal not found"));
        return;
      }

      logger.error({ err, id }, "[governance] on-chain status query failed");
      res
        .status(500)
        .json(errorResponse("INTERNAL_ERROR", "Failed to query on-chain proposal status"));
    }
  },
);
