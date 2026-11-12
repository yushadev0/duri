# Duri — project notes for Claude

Windows 11 dynamic-island app. Tauri 2 (Rust) + React/TS + Motion. Long-running open-source project (MIT, github.com/yushadev0/duri). The user speaks Turkish; code, comments, and docs are English.

## Rules

- **Commits/PRs: never mention Claude.** No `Co-Authored-By: Claude`, no "Generated with Claude Code", no Claude author. Commit as the user's git identity.
- UI design follows the `apple-design` skill (Apple HIG adapted to Windows). Load it before designing or reviewing UI.
- Every user-facing string goes in both `src/i18n/en.json` and `src/i18n/tr.json`.
- DB schema changes are appended to `MIGRATIONS` in `src-tauri/src/db.rs`, never edited.
- Keep `WINDOW_WIDTH/HEIGHT` in `src/island/geometry.ts` in sync with `src-tauri/src/island_window.rs`.

## Commands

```
pnpm tauri dev                       # run (Rust may need: $env:Path += ";$HOME\.cargo\bin")
pnpm typecheck; pnpm lint; pnpm test
cd src-tauri; cargo test; cargo clippy
```

## Architecture

Read `docs/ARCHITECTURE.md` and `docs/ADDING_AN_APP.md`. In short:
- `src/island/` is the shell: a pure reducer (`state.ts`), the morphing `Island.tsx`, and hit rects that the Rust hit-test thread uses for click-through. Hover comes from the `island://pointer` event.
- `src/apps/<id>/` exports a `DuriApp` (Minimal/Pill/Compact/Expanded + `useActivity`), listed in `src/apps/registry.ts`.
- `src-tauri/src/apps/<id>.rs` is the backend half: commands + `<id>://<event>` events.
- Taskbar clock (ARCHITECTURE › Taskbar clock): `src-clock/clock.cpp` is a C++/WinRT DLL that runs inside explorer.exe, built by `src-tauri/build.rs`; `src-tauri/src/taskbar_clock/` loads it and sends the text. Never copy code from Windhawk mods (GPL; Duri is MIT). Restarting explorer in a test closes the user's folder windows: note and reopen them.
- windows-rs 0.62: wait on WinRT async with `.join()`, not `.get()`.
- Focus model (see ARCHITECTURE › Focus): the island hands focus back to `last_external` unless an app calls `useIsland().setTyping(true)`. Never show the island with plain activation.
- Debug: `curl http://127.0.0.1:47321/duri/sessions` lists the sessions Duri sees.
- Vite's watcher sometimes misses edits made by scripts; if the UI looks stale, restart `pnpm tauri dev`.
- When testing typing/paste with SendKeys, first put a scratch window in front so stray keys land there, never in the user's apps.

## Roadmap

Done: shell, settings, tray, Music (+ cover gradient), Timer, Stopwatch, Sessions (Claude hooks verified with simulated payloads; Codex parser written from the rollout format but untested on a real install).
Ideas queue lives in TODO.md (do after the planned items).
Done too: Clipboard (search, pin, paste back; verified end to end).
Done too: Notes (autosave, focus verified with a scratch window).
Done too: TODO ideas (island style, pill size, bottom dock, volume HUD + flyout replacement, music volume slider).
Done too (1.1.0): Battery, Brightness, Camera and microphone apps (`apps/power.rs`, `apps/privacy.rs`).
Done too (1.2.0): Taskbar clock customization (opt-in, own Settings pane).
Next: nothing planned; see TODO.md. TODO.md › Suggestions are Claude's ideas: ask before doing them.
