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
   ├─ idle_window.rs            Music › Idle Mode fullscreen window
   ├─ http.rs                   small WinHTTP client (weather, lyrics)
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

Current priorities: timer finished 100 · battery critically low 99 · lock key 96 · volume / brightness 95 · charger plugged in or out 94 · copied 93 · battery low 92 · session needs you 90 · update available 89 · camera or microphone started 87 · notification / Bluetooth device 85 · timer 60 · music with lyrics in the pill 55 · session working 50 · music playing 40 · session just finished 35 · stopwatch running 30 · stopwatch paused 10.

## Time

Timers and the stopwatch store absolute Unix-ms timestamps (`endsAt`, `runningSince`), never counters. They survive restarts without drift, and the UI computes what to show from the same clock as `Date.now()`.

## Music

`apps/music.rs` has one worker thread that owns the WinRT `GlobalSystemMediaTransportControlsSessionManager`. Commands reach it over a channel. Between commands it polls every 500 ms, emits `music://state` when anything changes and `music://artwork` (a data URL) when the track changes. The UI extrapolates the playback position from `positionMs` + `positionUpdatedAt`, and derives an accent color from the cover art.

**Lyrics in the island.** The expanded view shows three lines between the song and the controls: the one before, the one being sung and the next (`ExpandedLyrics` in `lyricsViews.tsx`; a fixed 49 px band, so the controls never move, with a status line for songs without timed lyrics). With Settings › Apps › Music › Show Lyrics in the Pill, the music pill's `PillCenter` puts the line being sung in the middle of the pill on one line, optionally with the time; between lines the usual idle info stays. For songs with timed lyrics `usePillWidth` widens the pill to fit about 7–8 words (steady for the whole song, capped so the bubble still fits in the window), and music's priority rises to 55 so a working Claude Code session takes the bubble instead of covering the lyrics. Lyrics live in one store per window (`useLyrics`), so the pill and the expanded view ask for a song once, and `useLineIndex` re-renders on line changes only (a timer for the next line), keeping the pill's idle cost.

**Open the player.** Clicking the cover in Music's expanded view calls `music_open_player` (`apps/music_player.rs`): it finds a visible top-level window whose process matches the session's app id (exe name, Firefox's install hash, or a packaged app's `WindowsApps` folder), prefers the one whose title contains the song's title (the right browser window), and brings it forward with `island_window::activate_external`, which also makes it the window focus returns to. A packaged app with no window is started through `shell:AppsFolder\<aumid>`.

**Idle Mode.** The button in Music's expanded view calls `music_idle_open` (`idle_window.rs`), which opens a borderless fullscreen window labelled `idle` on the primary monitor; `main.tsx` renders `MusicIdle` in it (`src/apps/music/idle/`). Esc or the close button calls `music_idle_close`. While it is in front, `fullscreen_window_in_front` treats it like any fullscreen player, so the island steps aside. An optional clock sits at the top (`musicIdleClock`). Four looks, saved as `musicIdleStyle` ("auto" = Network, or Lyrics under reduced motion; keys 1–4 switch):

- **Network** (`IdleConstellation.tsx`, `constellation.ts`): each line of the song is a constellation. Its words land one by one at random spots, each joined by a line drawn from the word before (the stroke starts 300 ms early so the word lands on the beat); when the line is over the figure fades over 900 ms while the next one starts on an empty screen. Spots are seeded by the line, so a seek redraws the same figure: each word tries 60 random spots and keeps the one with no overlapping labels, a comfortable step from the previous word and no crossing lines. Untimed lyrics fall back to Lyrics. Around it: faint stars twinkling over seconds, a ripple where each word lands, a spark at the tip of the line being drawn, and finished figures drift up as they fade. Where nothing is sung (intro, breaks, outro, a whole instrumental) a circle breathes with the sound and steps aside 700 ms before the next line; while lyrics are loading or none were found, the circle is all there is. Its outer ring is the last ~1.5 s of levels (newest at the top) plus a faint sway of two drifting waves that grows a little with the level, so it never sits still but stays round. Its level comes from `apps/meter.rs`: the default speakers' peak meter (`IAudioMeterInformation`, one number, no audio captured), read every 33 ms only while the Network or Reactive look is open and sent to the idle window as `audio://level`; `meter.ts` turns it into a VU-like envelope (fast rise, slow fall, scaled to the song's own recent peak) so the ring breathes rather than flickers.
- The canvas loop runs while playing or fading and stops at rest. Glows rise softly and fade over seconds, never flash.
- **Reactive**: only the circle, 1.35× larger, for the whole song; never asks for lyrics. The background swells with it (`drawBackdrop`): an accent light that follows the level plus how far it stands above the last ~2 s, rising over ~0.5 s and fading over ~2 s.
- **Lyrics** (`IdleViews.tsx`): the current line centered and bright, the list gliding to it; a timer set for the next line's start re-renders, not every frame. Clicking a line seeks.
- **Cover**: big artwork and progress; the progress bar seeks (click or drag, arrow keys ±5 s, Home/End). Easter egg (`FlipCover`): clicking the artwork flips it into a vinyl record with the cover as its label, spinning at about 22 rpm while playing; the light on it stays still. A tonearm (`Tonearm`, SVG) lowers onto it 650 ms after the flip when playing, moves from the outer groove toward the label with the song's progress, and swings off when paused. Reduced motion cross-fades instead and nothing spins. Also the Lyrics look's fallback while lyrics are loading or missing; this look never asks for lyrics.

Lyrics come from LRCLIB (`apps/lyrics.rs`, no key): the exact `/api/get` (needs the length), then `/api/search` with a cleaned title and artist ("(Official Video)", "- 2011 Remaster", "feat.", "X - Topic") and with "Artist - Title" split out of video titles, keeping records within 4 s of the song's length and preferring word-synced, then line-synced ones. Word-synced records (`hasWordSync`) carry a `lyricsfile`, YAML with every line's start and end and every word's start; `apps/lyricsfile.rs` reads just that shape (no YAML dependency) and the UI joins syllables without a trailing space into one word. Answers are cached in the `lyrics` table, misses for 3 days; network errors are not cached. Otherwise the UI parses the LRC (`lyrics.ts`): `[mm:ss.xx]` lines, repeats, `[offset:]`, enhanced `<mm:ss.xx>` word times when present; otherwise each line's words are spread over the line by length. Plain (untimed) lyrics show as a page. The credit in the corner says whether the lyrics are word-synced, line-synced (word times estimated) or not synced. HTTP goes through `http.rs` (WinHTTP, shared with the weather).

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

## Battery and brightness

`apps/power.rs` polls nothing. A hidden top-level window registers for power setting notifications (`RegisterPowerSettingNotification`): power source, battery percentage, battery saver and `GUID_VIDEO_CURRENT_MONITOR_BRIGHTNESS`. Windows sends each one right after registering (the starting values, which only fill the state) and again on every change. It is a top-level window, not a message-only one, so it also gets the `PBT_APMPOWERSTATUSCHANGE` broadcast when charging starts or stops without a percent change. Battery details come from `GetSystemPowerStatus`. Events: `power://battery` (state), `power://plugged` (charger in or out), `power://low` (once at 20%, once more at 10% with `critical`, reset by plugging in; starting Duri already low stays quiet until the next threshold) and `power://brightness`. The Battery and Brightness apps share one store (`src/apps/power/store.ts`).

Brightness is set with WMI (`WmiMonitorBrightnessMethods.WmiSetBrightness` in `ROOT\WMI`), the same path as the Windows slider for built-in panels. External monitors don't report brightness this way, so on desktops the Brightness app never shows. On most laptops the brightness keys are handled by the firmware and never reach a keyboard hook, so Duri can't take them over the way it does the volume keys: Windows' own brightness popup may appear next to Duri's indicator.

## Camera and microphone

`apps/privacy.rs` reads what drives the taskbar's own camera and microphone icons: `HKCU\...\CapabilityAccessManager\ConsentStore\{webcam,microphone}`, one subkey per packaged app and one per desktop app under `NonPackaged` (the exe path with `#` for `\`). An app is using the device while its `LastUsedTimeStop` is 0. `RegNotifyChangeKeyValue` on both trees wakes the worker, with a safety re-read every 60 s. Desktop apps are named by their exe's FileDescription ("Google Chrome"), packaged ones from their family name ("Windows Camera"). `privacy://changed` carries the names per device. The island draws the badges itself, left of the island (no clicks, so no hit rect), and they stay while the island is tucked or auto-hidden. The app shows a notice when a new app starts. Settings › Apps can turn off the whole app or each device.

## Taskbar clock

Optional and off by default (Settings › Taskbar Clock, `src/settings/TaskbarClockPane.tsx`). It replaces the text of the Windows tray clock, which Windows offers no API for, so it is the one place Duri runs code in another process. Two halves:

- `src-clock/clock.cpp` → `duri_clock.dll`, C++/WinRT, runs inside explorer.exe. `src-tauri/build.rs` compiles it with the MSVC that `cc` finds and `taskbar_clock/inject.rs` embeds it with `include_bytes!`, so there is nothing extra to bundle.
- `src-tauri/src/taskbar_clock/` renders the text and sends it over. All the logic lives here; the DLL only shows what it is told.

**Loading.** `inject.rs` writes the DLL to `%APPDATA%\Duri\clock\duri_clock_<hash>.dll`, sets a `WH_CALLWNDPROC` hook on the thread of `Shell_TrayWnd` (Windows maps the DLL into explorer for that), sends the registered message `DuriClockAttach` and removes the hook. On that message the DLL pins itself, creates a message-only window named `DuriClockHost:<file name>` and connects to XAML Diagnostics (`InitializeXamlDiagnosticsEx`, the API behind Visual Studio's Live Visual Tree) with itself as the TAP DLL. The hash in the file name means an update never has to overwrite a locked file, and Duri only finds the window of the exact build it shipped.

**Changing the clock.** XAML Diagnostics replays the taskbar's visual tree to `Tap::OnVisualTreeChange` and keeps reporting additions (a second taskbar, a rebuilt one). The DLL waits for a `TextBlock` named `TimeInnerTextBlock`; its parent is `DateTimeStackPanel`, inside a `Grid`. To show text it collapses the stack panel and appends its own `TextBlock` to that grid, with the clock's font and a binding to its foreground brush (the taskbar swaps the brush with the theme). The text is not hit-testable, so clicks and the tooltip still belong to the clock button. Collapsing the time and date blocks one by one does not work: Windows shows the date again on the next refresh. No function is hooked and no private symbol is used; if the names change, nothing is found and nothing happens.

**Protocol.** `WM_COPYDATA` to the host window: `dwData` 1 = set text (payload: `SetHeader` = Duri's process id, alignment 0/1/2 and a generation, three u32s; then NUL-terminated UTF-16 with `\n` between lines), 0 = revert. With three lines the DLL drops the font size by 1 and tightens the line height so they fit. Usage numbers change width every second, so our TextBlock's `MinWidth` follows its widest size until the generation changes (the user edited the lines); otherwise the whole tray would twitch. The reply is 1 + the number of clocks on screen, so "delivered but no clock found" (`Status::NoClock`) is told apart from "not delivered". Keep the constants in `clock.cpp` and `inject.rs` in sync.

**Updates.** One thread (`worker`), asleep on a channel while the feature is off. While on, it wakes on each wall-clock second, samples only what the lines use (`values::Snapshot`: CPU and RAM from `system::sample`; GPU, network and disk from PDH counters in `perf.rs`, opened lazily and closed when unused; battery from `GetSystemPowerStatus`; song from `apps::music::now_playing`), renders, and sends only if the text changed. While Settings shows the token reference it asks `taskbar_clock_reference` every second, which samples everything for 3 s so each row has a live example.

**Weather.** `weather.rs`: Open-Meteo forecast (current + 8 days, `timezone=auto`) and geocoding for the city search, over WinHTTP (`http_get`; the WinRT `HttpClient` failed TLS with 0x80072F8F on the user's PC). Cached per place and unit in `WeatherCache`, refreshed every 15 min (1 min after a failure) on a short-lived thread, and only while a weather token is in use. WMO codes map to an emoji and an English or Turkish word following Duri's language. `cargo test -- --ignored live_` checks the real service. If the host window is gone (explorer restarted) it loads the DLL again; right after explorer starts the taskbar has no XAML yet, so the DLL retries its connection once a second.

**Templates.** `template.rs`: `{time}`, `{date}`, `{time:PICTURE}`, `{date:PICTURE}` (Windows format pictures via `GetTimeFormatEx` / `GetDateFormatEx` in the user's locale), `{weekday}`, `{forecast:N}` and the `Key` values. Unknown tokens stay as typed; a missing value (no city, no battery) shows `–`. Blank lines are dropped; all blank reverts to the Windows clock. `REFERENCE` is the list Settings shows; a test checks every key is in it. A new token needs `Key::NAMES`, `REFERENCE` and `settings.taskbarClock.tokens` in both language files.

**Safety nets.**
- The DLL watches Duri's process handle every 2 s and restores the Windows clock if Duri is gone.
- Crash guard (`Guard`, stored in `kv` as `taskbar_clock/guard`): before loading, Duri records the explorer pid; 30 s later, if that explorer is still the taskbar's owner, the record is cleared. A record whose explorer is gone when Duri next wants to load counts as a strike; two strikes switch the setting off (`Status::Blocked`) until the user turns it on again.
- Every entry point in the DLL catches C++ exceptions. Explorer has no console, so the DLL logs the few things worth knowing to `%TEMP%\duri-clock.log`.

**Limits.** The DLL never unloads: XAML holds pointers into it. Reverted, it is inert until explorer restarts, and its file stays locked until then (older files are deleted on the next start). Windows 11 x64 with the stock taskbar only. To iterate on the DLL just rebuild: a changed DLL gets a new hash and loads next to the old one.

When testing: restarting explorer closes the user's folder windows, so note them first (`Shell.Application.Windows()`) and reopen them.

## Updates

`src/lib/updates.ts` runs the Tauri updater against `latest.json` of the latest GitHub release: 30 s after launch and every 12 h (Settings › General › Check for Updates Automatically; never in development unless `VITE_DURI_UPDATE_CHECK=1`). A new version is not a Windows notification (those were easy to miss) but the `update` app (`src/apps/update/`): a `hidden`, `system` app (not in the dock, not in Settings › Apps) whose activity is an alert at priority 89, so the island bounces open in compact form and stays there until answered, the way a permission prompt does. **Download** calls `installUpdate` (progress in the same place; the NSIS installer runs passive and Duri restarts), **Not Now** stores the version in `localStorage` (`duri.update.dismissed`) so that version is not asked about again; Settings › About still offers it. A failed download shows in the prompt with a Close button. To try the prompt locally, lower the version in `package.json`, touch `src-tauri/tauri.conf.json` (otherwise the old binary keeps its version) and run `pnpm tauri dev` with `VITE_DURI_UPDATE_CHECK=1`.

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
