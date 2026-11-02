# Adding an app

An app has a UI half (`src/apps/<id>/`) and, if it needs Windows APIs, background work or storage, a backend half (`src-tauri/src/apps/<id>.rs`).

## 1. Backend (optional)

1. Create `src-tauri/src/apps/<id>.rs` with an `init(app: &AppHandle, …)` function, its state (`app.manage(...)`), and `#[tauri::command]` functions.
2. Add `pub mod <id>;` and the `init` call to `src-tauri/src/apps/mod.rs`.
3. Register the commands in `generate_handler![…]` in `src-tauri/src/lib.rs`.
4. Push changes to the UI with `app.emit("<id>://<event>", payload)`. Use `#[serde(rename_all = "camelCase")]` on payloads.
5. Persist state with `Db::get_json` / `Db::set_json` (namespace = app id), or add a migration for real tables.
6. Put the logic in plain structs with unit tests. See `timers.rs`.

## 2. UI

1. Create `src/apps/<id>/` with:
   - a store mirroring the backend state (`createStore` + `listen`) and a `start()` that subscribes once;
   - the four presentations: `Minimal` (34×34 bubble), `Pill` (height 34, `pillWidth` wide), `Compact` (hover, default 372×72), `Expanded` (click; the bottom 44 px are the shared app dock);
   - a `useActivity()` hook returning `null` or a **stable** `Activity` object (use constants or `useMemo`).
2. Export a `DuriApp` object and add it to `src/apps/registry.ts`. Set `hidden: true` for system apps that should only appear on their own (like Volume).
3. Add `apps.<id>.name` / `apps.<id>.description` and every string you use to `src/i18n/en.json` and `tr.json`.

## Design checklist

- Black, opaque island. Use one bold identity color (`accent`) for the app's key elements.
- Concentric corners: an inner element's radius = outer radius − its margin (see `CONTENT_PADDING`).
- Controls ≥ 28 px; use `IconButton` / `TextButton` from `src/components/Button.tsx` (they stop clicks from reaching the island).
- Text ≥ 11 px; numbers that change use the `tabular` class.
- Preserve element positions between Compact and Expanded so the morph reads as one object.
- Alerts (`alert: true`) only for things people must not miss.
- Respect reduced motion (CSS animations are already disabled globally under `prefers-reduced-motion`).
