import type { Activity, DuriApp } from "./types";

export interface RankedActivity {
  app: DuriApp;
  activity: Activity;
}

/** Orders apps with an activity by priority (ties keep registry order). */
export function rankActivities(
  apps: DuriApp[],
  activities: Record<string, Activity | null | undefined>,
): RankedActivity[] {
  return apps
    .map((app, index) => ({ app, activity: activities[app.id], index }))
    .filter((x): x is { app: DuriApp; activity: Activity; index: number } => !!x.activity)
    .sort((a, b) => b.activity.priority - a.activity.priority || a.index - b.index)
    .map(({ app, activity }) => ({ app, activity }));
}

export function sameActivity(a: Activity | null | undefined, b: Activity | null | undefined) {
  if (!a || !b) return !a && !b;
  return a.priority === b.priority && !!a.alert === !!b.alert && !!a.hud === !!b.hud && a.key === b.key;
}
