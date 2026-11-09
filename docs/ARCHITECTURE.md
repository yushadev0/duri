# Architecture

```
duri/
├─ src/                         React UI (one bundle, two windows)
│  ├─ main.tsx                  picks the view by window label: "island" or "settings"
│  ├─ island/                   the island shell
│  │  ├─ Island.tsx             view resolution, morphing, hit-rect reporting
│  │  ├─ state.ts               pure state machine (idle → compact → expanded, alerts)
│  │  ├─ geometry.ts            sizes, radii, delays, window size
│  │  ├─ Home.tsx               compact view when nothing is active
│  │  └─ ExpandedChrome.tsx     shared header + app switcher
│  ├─ apps/                     one folder per app + registry.ts + types.ts
│  ├─ settings/                 the Settings window
│  ├─ components/ lib/ design/  shared UI, helpers, tokens and springs
│  └─ i18n/                     en.json, tr.json
└─ src-tauri/src/
   ├─ lib.rs                    plugins, state, command registration
   ├─ island_window.rs          overlay window, click-through hit testing
   ├─ tray.rs                   tray icon/menu, settings window
   ├─ settings.rs  db.rs        settings, SQLite + migrations
   └─ apps/                     backend half of each app
```

## The island window

The island is one transparent, borderless, always-on-top Tauri window (640×320 logical px) at the top center of the primary monitor. It has `WS_EX_NOACTIVATE`, so clicking it never steals focus, and `WS_EX_TOOLWINDOW`, so it stays out of Alt+Tab and the taskbar.

Because the window is much bigger than the visible pill, it ignores cursor events by default. The frontend reports the visible rectangles with `island_set_hit_rects`. A Rust thread checks the cursor (`GetCursorPos`) every 20 ms while it is near the island and every 60 ms elsewhere, and turns cursor events on only while the cursor is over one of those rectangles. The window position and scale are re-read once a second, not every tick. The same thread emits `island://pointer` (true/false). The webview can't get `mouseleave` once events are ignored, so hover is driven entirely by this event.

**Visibility.** The window is shown unless the tray menu hid it or a fullscreen app is in front (`apply_visibility` in `island_window.rs`). A watcher thread checks every 500 ms: Windows' own notification state catches exclusive-fullscreen games and presentation mode, and a foreground window that covers the whole primary monitor catches borderless games, F11 browsers and video players. Browsers report fullscreen as still maximized, so the check is "covers the monitor and has no title bar" rather than "not maximized". The desktop (`Progman`/`WorkerW`) never counts.

## Focus

Hovering never takes focus (`WS_EX_NOACTIVATE`). A click does bring the island to the front (WebView2 activates it), and Duri keeps it that way while the island is open: bouncing focus back right away made the whole screen flicker. When the island rests again (pointer left, Esc, or the person clicked elsewhere), `island_set_focus(false)` hands focus back to `last_external`, the last foreground window that isn't Duri's, which the hit-test thread keeps track of. If another app is already in front by then, it is left alone.

Text fields (Clipboard search, Notes) call `useIsland().setTyping(true)`. That removes `WS_EX_NOACTIVATE`, brings the island to the front, and locks it open so the pointer leaving doesn't collapse it. Windows usually refuses that because the click went to WebView2's process, so `force_activate` briefly attaches to the foreground thread's input (`AttachThreadInput`). Leaving typing mode is debounced by 150 ms: taking focus makes the field blur and refocus, and reacting to that blur would send the next keystrokes to another app. `setTyping(false)`, Esc, or collapsing gives focus back to `last_external`. Pasting (`clip_paste`) does the same and then sends Ctrl+V, so the paste lands where the person was.

## State machine

`src/island/state.ts` is a pure reducer with unit tests:

- `pointer` is the raw signal. `hover` and `leave` are debounced versions of it, dispatched by `Island.tsx`.
- `open(appId)` expands an app. `collapse` returns the island to its resting mode.
- `attention(appId)` holds the island in compact mode (e.g. when a timer ends) until `attentionEnd`.
- `growing` picks the spring: a bouncy morph when growing, a calm settle when shrinking.

## Apps and activities

Every app implements `DuriApp` (`src/apps/types.ts`). It provides four presentations (Minimal, Pill, Compact, Expanded) and a `useActivity()` hook. Activities are ranked by priority: the top one gets the pill, the second gets the bubble. An activity with `alert: true` triggers attention.

Backend modules own their state and background threads. They expose `#[tauri::command]`s and emit `<app>://<event>` events. The frontend mirrors their state in small stores (`src/lib/store.ts`, read via `useSyncExternalStore`).

Current priorities: timer finished 100 · lock key 96 · volume 95 · copied 93 · session needs you 90 · notification / Bluetooth device 85 · timer 60 · session working 50 · music playing 40 · session just finished 35 · stopwatch running 30 · stopwatch paused 10.

## Time

Timers and the stopwatch store absolute Unix-ms timestamps (`endsAt`, `runningSince`), never counters. They survive restarts without drift, and the UI computes what to show from the same clock as `Date.now()`.

## Music

`apps/music.rs` has one worker thread that owns the WinRT `GlobalSystemMediaTransportControlsSessionManager`. Commands reach it over a channel. Between commands it polls every 500 ms, emits `music://state` when anything changes and `music://artwork` (a data URL) when the track changes. The UI extrapolates the playback position from `positionMs` + `positionUpdatedAt`, and derives an accent color from the cover art.

## Clipboard

`apps/clipboard.rs` listens with a clipboard-format listener (clipboard-win `Monitor`). It reads files (CF_HDROP), then text, then bitmaps, and skips content flagged `ExcludeClipboardContentFromMonitorProcessing` or `CanIncludeInClipboardHistory = 0`. Items live in the `clips` table, deduplicated by content hash: copying something again moves it to the top. Images are saved as PNG under `%APPDATA%\Duri\clips` with a 160 px thumbnail in the row. Up to 300 unpinned items are kept.

## Volume

`apps/volume.rs` owns the default output device's `IAudioEndpointVolume` on one COM thread. Duri's own changes apply at once; a 250 ms poll catches changes made elsewhere (other apps, device switches) and emits `volume://changed`. The UI turns each change into a short-lived, high-priority activity (the volume pill). Volume is a `hidden` app: it never appears in the launcher or dock.

**Replace the Windows volume popup.** Windows 11's flyout can't be hidden reliably, so a `WH_KEYBOARD_LL` hook takes the volume keys, changes the volume itself, and swallows the key, so Windows never opens its popup. Known limitation: while a Duri window (Settings, or the island in typing mode) is in front, Windows still shows its popup, because it does so whenever the foreground app changes the volume.

**Indicators over everything.** Volume and lock key activities set `hud: true`. They show while the island is tucked or auto-hidden, and `island_set_hud` tells the backend to show the window over a fullscreen app until the indicator is gone (350 ms later, so the pill settles). That show uses `SetWindowPos(HWND_TOPMOST, SWP_NOACTIVATE | SWP_SHOWWINDOW)` rather than Tauri's `show`, so a borderless game never loses focus, and is undone with a raw `ShowWindow(SW_HIDE)`, since Tauri didn't see it. Exclusive-fullscreen games and presentation mode stay untouched.

## Lock keys

`apps/keys.rs` has its own `WH_KEYBOARD_LL` hook that only watches Caps Lock, Insert and Scroll Lock (it never swallows them, and ignores auto-repeat). The hook runs before Windows flips the toggle, so another thread reads `GetKeyState` 40 ms later and emits `keys://lock`.

## Bluetooth

`apps/bluetooth.rs` runs a WinRT `DeviceWatcher` over paired Bluetooth association endpoints (classic and LE) with `System.Devices.Aep.IsConnected`, so changes arrive as events and nothing is polled. The first enumeration only records the current state. A dual-mode device has two endpoints that change together, so notices are deduplicated per container id for 4 s. The class of device (major/minor) picks the icon. If Windows stops the watcher (the Bluetooth service restarting), a new one starts.

Battery levels are not on the endpoints: Windows keeps them as `DEVPKEY_Bluetooth_Battery` (`{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2`) on a PnP node of the same container (the Hands-Free AG node of headphones). A second thread reads them with one `FindAllAsync` over the connected containers: every 60 s, and every 4 s for 40 s after a connection change, since headphones report their level a moment after connecting. Windows sends no change events for this property. The connected list (one entry per container) goes out as `bluetooth://devices` when it changes; `bluetooth_devices` returns the last one. When a reading drops to 20% or below (or a device connects already low), `bluetooth://low` shows a notice once; devices that were already low when Duri started stay quiet. Devices that don't report a level (the DualSense, for one) show "No battery info".

## WebView2 runtime

Duri runs for days while WebView2 updates itself. A new webview would start the newest runtime, which can't share the user data folder with the older browser process the island is on, so it exits and the window closes at once. `webview_runtime::pin_to_running` sets `WEBVIEW2_BROWSER_EXECUTABLE_FOLDER` to the island's runtime before Settings is built; call it before building any new window.

## Storage

SQLite at `%APPDATA%\Duri\duri.db`, WAL mode. Migrations are an append-only list in `db.rs`, tracked by `PRAGMA user_version`. Small state is stored as JSON in the `kv` table.

## Idle cost

The island window is transparent, so anything that changes in it makes WebView2 recompose the whole window. A CSS animation that never ends would do that at the display's refresh rate, all day. Rules that keep Duri near zero while it rests:

- No infinite CSS animations in the pill or bubble. Use `useStep` (`src/lib/ticks.ts`) to step a few times a second (the music equalizer, the working dots). Smooth animations are fine in compact and expanded views, which the person is looking at and which close again.
- Re-render on the boundary that changes what's shown: `useAlignedNow(enabled, 60_000)` for a clock without seconds, `useAlignedNow(enabled, 1000, endsAt)` for a countdown. `useNow` without an interval (every frame) is for precise digits in open views only.
- Measure with `scripts/bench.ps1` (duri.exe plus its WebView2 processes) before and after a change that touches the pill.

## Permission prompts

Optional (Settings › Apps › Integrations). Duri adds a `PermissionRequest` hook to `~/.claude/settings.json` whose curl call POSTs to `/duri/claude-permission` and waits. The server keeps the request open (`Sessions.pending`) and the session shows `permission` in the island with Allow / Deny / Answer in Terminal. `session_permission` answers it with the JSON Claude Code expects (`hookSpecificOutput.decision.behavior`), or with an empty response, which makes Claude Code ask in the terminal as usual. That empty answer is also what happens when nobody answers within 2 minutes or when the island is hidden. Any later hook event for the session clears a prompt that is still shown, since Claude has moved on.

## Notifications

`apps/notifications.rs` owns the WinRT `UserNotificationListener` on one thread. Reading the toast list is expensive (it returns everything in Action Center), so it happens only when Windows' notification database log (`wpndatabase.db-wal`) changed size or write time, checked once a second with one file query, plus a safety read every 30 s. `NotificationChanged` is documented for packaged apps and fails for Duri; folder change notifications don't fire for that file because Windows keeps it open. The worker diffs the toast ids, reads new toasts (app name, logo, text) and emits `notifications://new`; the list (last 30, memory only) goes out as `notifications://changed`. Duri's own toasts are skipped. Access follows Windows Settings › Privacy & security › Notifications. The "Copied" confirmation comes from `clipboard://copied`, which the clipboard monitor sends for new copies but not for Duri's own paste-back.

## Keyboard shortcuts

`shortcuts.rs` registers the accelerators from the settings (`shortcutIsland`, `shortcutNote`, `shortcutClipboard`, e.g. "Ctrl+Alt+Space") with the global-shortcut plugin and re-registers when they change. A press emits `island://shortcut` with the action; the island opens the app, locks itself open and takes focus (`island_set_focus`), so Esc works and typing goes to Notes or the clipboard search. It closes on Esc or when the window loses focus. Only the island has a default: other apps own many global shortcuts, and Ctrl+Alt+letter is AltGr+letter on many keyboard layouts. A shortcut another app already registered is reported as taken in Settings.
