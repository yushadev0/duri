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

Current priorities: timer finished 100 · session needs you 90 · timer 60 · session working 50 · music playing 40 · session just finished 35 · stopwatch running 30 · stopwatch paused 10.

## Time

Timers and the stopwatch store absolute Unix-ms timestamps (`endsAt`, `runningSince`), never counters. They survive restarts without drift, and the UI computes what to show from the same clock as `Date.now()`.

## Music

`apps/music.rs` has one worker thread that owns the WinRT `GlobalSystemMediaTransportControlsSessionManager`. Commands reach it over a channel. Between commands it polls every 500 ms, emits `music://state` when anything changes and `music://artwork` (a data URL) when the track changes. The UI extrapolates the playback position from `positionMs` + `positionUpdatedAt`, and derives an accent color from the cover art.

## Clipboard

`apps/clipboard.rs` listens with a clipboard-format listener (clipboard-win `Monitor`). It reads files (CF_HDROP), then text, then bitmaps, and skips content flagged `ExcludeClipboardContentFromMonitorProcessing` or `CanIncludeInClipboardHistory = 0`. Items live in the `clips` table, deduplicated by content hash: copying something again moves it to the top. Images are saved as PNG under `%APPDATA%\Duri\clips` with a 160 px thumbnail in the row. Up to 300 unpinned items are kept.

## Volume

`apps/volume.rs` owns the default output device's `IAudioEndpointVolume` on one COM thread. Duri's own changes apply at once; a 250 ms poll catches changes made elsewhere (other apps, device switches) and emits `volume://changed`. The UI turns each change into a short-lived, high-priority activity (the volume pill). Volume is a `hidden` app: it never appears in the launcher or dock.

**Replace the Windows volume popup.** Windows 11's flyout can't be hidden reliably, so a `WH_KEYBOARD_LL` hook takes the volume keys, changes the volume itself, and swallows the key, so Windows never opens its popup. Known limitation: while a Duri window (Settings, or the island in typing mode) is in front, Windows still shows its popup, because it does so whenever the foreground app changes the volume.

## Storage

SQLite at `%APPDATA%\Duri\duri.db`, WAL mode. Migrations are an append-only list in `db.rs`, tracked by `PRAGMA user_version`. Small state is stored as JSON in the `kv` table.

## Idle cost

The island window is transparent, so anything that changes in it makes WebView2 recompose the whole window. A CSS animation that never ends would do that at the display's refresh rate, all day. Rules that keep Duri near zero while it rests:

- No infinite CSS animations in the pill or bubble. Use `useStep` (`src/lib/ticks.ts`) to step a few times a second (the music equalizer, the working dots). Smooth animations are fine in compact and expanded views, which the person is looking at and which close again.
- Re-render on the boundary that changes what's shown: `useAlignedNow(enabled, 60_000)` for a clock without seconds, `useAlignedNow(enabled, 1000, endsAt)` for a countdown. `useNow` without an interval (every frame) is for precise digits in open views only.
- Measure with `scripts/bench.ps1` (duri.exe plus its WebView2 processes) before and after a change that touches the pill.
