# Contributing to Duri

Thanks for helping. A few things keep the project easy to work on.

## Setup

See "Build from source" in the [README](README.md). Then:

```powershell
pnpm tauri dev   # run the app with hot reload
pnpm typecheck   # TypeScript
pnpm lint        # ESLint (includes React Compiler rules)
pnpm test        # Vitest
cd src-tauri; cargo test; cargo clippy
```

CI runs all of these on every push and pull request.

## Guidelines

- **One app, one folder.** UI in `src/apps/<id>/`, backend in `src-tauri/src/apps/<id>.rs`. Read [docs/ADDING_AN_APP.md](docs/ADDING_AN_APP.md) first.
- **Design follows Apple's HIG**, adapted to Windows. The island is black, and opaque unless the person turns on Transparency. Each app gets one bold identity color. Controls are at least 28 px. Every icon-only button has a label. Motion respects reduced-motion settings.
- **User-facing text goes through i18n.** Add keys to both `src/i18n/en.json` and `src/i18n/tr.json`.
- **Storage:** add SQLite schema changes as new entries at the end of `MIGRATIONS` in `src-tauri/src/db.rs`. Never edit existing entries.
- Keep pull requests focused, and describe how you tested the change on Windows.

## Releasing

1. Bump `version` in `package.json` and `src-tauri/Cargo.toml` (they must match; `tauri.conf.json` reads `package.json`).
2. Commit, then tag and push: `git tag v0.2.0 && git push origin main v0.2.0`.
3. The Release workflow builds the installers and opens a **draft** release with `latest.json`. Check it, write the notes, and publish. Installed copies only see the update once the release is published.

Updates are signed with the updater key. The private key and its password live in the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`; the public key is in `tauri.conf.json`. If the private key is lost, installed copies can't verify new updates and have to be reinstalled by hand, so keep a backup of it.
