import { Router } from "express";
import {
  getRankedLeaderboard,
  type LeaderboardPeriod,
} from "../services/analyticsCache.js";
import { logger } from "../logger.js";

export const leaderboardRouter = Router();

const periods = new Set<LeaderboardPeriod>(["all", "30d", "7d", "24h"]);

leaderboardRouter.get("/", async (req, res): Promise<void> => {
  try {
    const requestedPeriod = req.query.period;
    const period = typeof requestedPeriod === "string" && periods.has(requestedPeriod as LeaderboardPeriod)
      ? requestedPeriod as LeaderboardPeriod
      : "all";
    if (requestedPeriod !== undefined && (typeof requestedPeriod !== "string" || !periods.has(requestedPeriod as LeaderboardPeriod))) {
      res.status(400).json({ error: "period must be one of all, 30d, 7d, or 24h" });
      return;
    }
    const entries = await getRankedLeaderboard(period);
    res.json({
      leaderboard: entries.map(({ userId, shareBalance, rank, rankChange }) => ({
        userId: `${userId.slice(0, 8)}...`,
        shareBalance,
        rank,
        rankChange,
      })),
      period,
      generatedAt: new Date().toISOString(),
    });
  } catch (err) {
    logger.error({ err }, "Failed to retrieve leaderboard");
    res.status(500).json({ error: "Failed to retrieve leaderboard" });
  }
});
