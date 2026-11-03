# Duri TODO

## Next

- [ ] **GitHub page:** a README with screenshots/GIFs of each app, badges, a clear install section, a polished repo description and topics
- [ ] **Demo video** of the island and its apps (for the README and releases)
- [ ] **Releases:** GitHub Actions builds signed installers on a tag, plus the tauri updater. Keep using the durian mark from Settings › About as the app icon (`src-tauri/icons/app-icon.svg`, generated with `pnpm tauri icon`)
- [ ] **Tuck away:** drag the island up past the top edge to shrink it to a tiny handle; drag it back down (or hover the handle) to bring it back

## Suggestions (from Claude, to discuss)

- [ ] **Multi-monitor:** choose a monitor or follow the one with the cursor
- [ ] **Global shortcut** to open the island, a quick note, or the clipboard (keyboard-only use)
- [ ] **Approve Claude Code permission prompts** from the island (PermissionRequest hook answering allow/deny)
- [ ] **Verify Codex tracking** against a real Codex install (parser is written from the rollout format, untested)
- [ ] **Code-sign the installer** so SmartScreen doesn't warn on first run
- [ ] **Idle CPU audit** of the polling threads (hit test 20 ms, volume 100 ms, music 500 ms) and back off when nothing changes
- [ ] **Keyboard navigation** inside the expanded island (Tab between controls, arrows in lists)
- [ ] Optional "Copied" confirmation in the pill when something is copied

## Done

- [x] Sessions app (Claude Code hooks + Codex watcher), Clipboard app, Notes app
- [x] Island style (floating / attached), pill size, app dock at the bottom
- [x] Volume HUD, replacing the Windows volume popup, volume slider in Music
- [x] Music background gradient from the cover color
- [x] Idle info (clock / CPU & RAM), also shown in the middle of activity pills
- [x] Right-click menu, focus that stays on the island after a click
- [x] 12/24-hour clock (follows Windows by default), island transparency, hide in fullscreen
