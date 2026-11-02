import { api } from "./api";

/** Weigh-in days (0 = Sunday … 6 = Saturday) and reminder time, kept in the settings table. */
export interface WeighInPlan {
  days: number[];
  time: string; // "HH:MM"
}

export async function loadWeighInPlan(): Promise<WeighInPlan> {
  const [days, time] = await Promise.all([api.getSetting("weigh_in_days"), api.getSetting("weigh_in_time")]);
  return {
    days: (days ?? "")
      .split(",")
      .filter((d) => d.trim() !== "")
      .map(Number),
    time: time || "09:00",
  };
}

/** Pushes the plan to the Android alarm (also after a restore or reinstall). */
export function syncWeighInReminder(plan: WeighInPlan) {
  const [h, m] = plan.time.split(":").map(Number);
  return api.weighInReminder(plan.days, h, m).catch(() => {});
}

export async function saveWeighInPlan(plan: WeighInPlan) {
  await api.setSetting("weigh_in_days", plan.days.join(","));
  await api.setSetting("weigh_in_time", plan.time);
  await syncWeighInReminder(plan);
}
