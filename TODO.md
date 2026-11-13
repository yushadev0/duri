# Duri TODO

## Next


## Suggestions (from Claude, to discuss)

- [ ] **Multi-monitor:** choose a monitor or follow the one with the cursor
- [ ] **Verify Codex tracking** against a real Codex install (parser is written from the rollout format, untested)
- [ ] **Code-sign the installer** so SmartScreen doesn't warn on first run
- [ ] **Keyboard navigation** inside the expanded island (Tab between controls, arrows in lists)

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
- [x] Tuck away (drag up / hover to peek / drag down or right-click), answer Claude Code permission prompts from the island (opt-in)
- [x] Notifications app (Windows toasts + Copied confirmation), two players from the same app (Chrome 2)
- [x] Global shortcuts (open island, new note, search clipboard), recorded in Settings
- [x] One expanded size for every app, auto-hide like a taskbar (v0.2.1)
- [x] Bluetooth connect/disconnect notices, Caps Lock / Insert / Scroll Lock indicator, indicators over fullscreen and while hidden, auto-hide opens straight to the panel
- [x] Bluetooth battery levels (rings in the dock app) and a low battery notice; Settings no longer closes at once after a WebView2 update
- [x] GitHub page: README with demo GIF and screenshots (demo content only), repo description and topics
- [x] Island stays centered after display changes, tuck drag no longer gets stuck (v1.1.0)
- [x] Battery (charger notices, low battery), Brightness (indicator + slider), Camera and microphone in use (v1.1.0)
- [x] Taskbar clock: up to 3 lines, alignment, time/date formats, CPU/RAM/GPU/battery, network and disk speed, weather and forecast (Open-Meteo), song in the Windows tray clock; own Settings pane (opt-in, helper DLL inside explorer; v1.2.0)
- [x] Lyrics from LRCLIB (word-synced when available) in Music's expanded view and optionally the pill; fullscreen Idle Mode with a word constellation, scrolling lyrics or cover, a sound-reactive circle in instrumental parts and an optional clock (v1.3.0)
