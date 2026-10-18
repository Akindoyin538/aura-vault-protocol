import { Router, type Request, type Response } from "express";
import {
  getPreferences,
  updatePreferences,
  preferencesStore,
  type UserPreferences,
} from "../preferences.js";

export const userPreferencesRouter = Router();

userPreferencesRouter.get("/", getPreferences);
userPreferencesRouter.put("/", updatePreferences);
userPreferencesRouter.patch("/", updatePreferences);

userPreferencesRouter.delete("/alerts/:id", (req: Request, res: Response): void => {
  const userId = ((req as any).user?.sub as string) || "anonymous";
  const { id } = req.params;

  const current: UserPreferences = preferencesStore.get(userId) ?? {
    default_history_range: "30d",
    priceAlerts: [],
  };

  const updatedAlerts = (current.priceAlerts ?? []).filter((a) => a.id !== id);
  const updated: UserPreferences = {
    ...current,
    priceAlerts: updatedAlerts,
  };

  preferencesStore.set(userId, updated);
  res.json(updated);
});
