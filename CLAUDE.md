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
- windows-rs 0.62: wait on WinRT async with `.join()`, not `.get()`.

## Roadmap

Done: shell, settings, tray, Music, Timer, Stopwatch.
Next: Sessions (Claude Code hooks → local HTTP endpoint; Codex `~/.codex/sessions/**/*.jsonl` watcher), Clipboard (text/image/file history), Notes, GitHub Releases + tauri updater.
