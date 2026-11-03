# Duri TODO

## Next

- [ ] **Tuck away:** drag the island up past the top edge to shrink it to a tiny handle; drag it back down (or hover the handle) to bring it back

## Suggestions (from Claude, to discuss)

- [ ] **Multi-monitor:** choose a monitor or follow the one with the cursor
- [ ] **Global shortcut** to open the island, a quick note, or the clipboard (keyboard-only use)
- [ ] **Approve Claude Code permission prompts** from the island (PermissionRequest hook answering allow/deny)
- [ ] **Verify Codex tracking** against a real Codex install (parser is written from the rollout format, untested)
- [ ] **Code-sign the installer** so SmartScreen doesn't warn on first run
- [ ] **Keyboard navigation** inside the expanded island (Tab between controls, arrows in lists)
- [ ] **Two players from the same app** (e.g. two Chrome windows) share one source id, so the source picker can only reach the first; key sources by session, not app id
- [ ] Optional "Copied" confirmation in the pill when something is copied

## Done

- [x] Sessions app (Claude Code hooks + Codex watcher), Clipboard app, Notes app
- [x] Island style (floating / attached), pill size, app dock at the bottom
- [x] Volume HUD, replacing the Windows volume popup, volume slider in Music
- [x] Music background gradient from the cover color
- [x] Idle info (clock / CPU & RAM), also shown in the middle of activity pills
- [x] Right-click menu, focus that stays on the island after a click
- [x] 12/24-hour clock (follows Windows by default), island transparency, hide in fullscreen
- [x] Releases: tag-triggered GitHub Actions build, in-app updater (v0.1.0 published)
- [x] Idle CPU: stepped pill animations, aligned clocks, cheaper hit test (20% of a core with music down to 2%), benchmark in README
- [x] GitHub page: README with demo GIF and screenshots (demo content only), repo description and topics
