/**
 * Apps in the order they were last opened (most recent first), so the
 * home view can offer the few the person actually uses. Kept per PC in
 * localStorage; losing it only means the dock order is used again.
 */
import { createStore, useStore } from "./store";

const KEY = "duri.recentApps";
/** More than the home view shows, so turning an app off doesn't empty it. */
const KEEP = 12;

function load(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? "[]");
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
  } catch {
    return [];
  }
}

const store = createStore<string[]>(load());

export const useRecentApps = () => useStore(store);

/** Moves `id` to the front of the list. */
export function noteAppOpened(id: string) {
  const next = [id, ...store.get().filter((x) => x !== id)].slice(0, KEEP);
  if (next.join() === store.get().join()) return;
  store.set(next);
  try {
    localStorage.setItem(KEY, JSON.stringify(next));
  } catch {
    // Not remembered; fine.
  }
}

/**
 * The `count` apps to offer: the most recently opened ones that are still
 * available, then the rest in `available` (dock) order.
 */
export function pickRecent<T extends { id: string }>(available: T[], recent: string[], count: number): T[] {
  const byId = new Map(available.map((a) => [a.id, a]));
  const picked = recent.map((id) => byId.get(id)).filter((a): a is T => !!a);
  for (const a of available) if (!picked.includes(a)) picked.push(a);
  return picked.slice(0, count);
}
