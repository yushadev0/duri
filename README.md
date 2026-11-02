# Duri

**A dynamic island for Windows.** Duri sits at the top of your screen as a small black pill. It shows what is going on right now, opens on hover, and expands on click.

The name comes from the durian: spiky outside, soft inside.

## Apps

Duri's features are called **apps**. Each one can be switched off in Settings › Apps.

| App | What it does | Status |
| --- | --- | --- |
| 🎵 Music | Shows what is playing in Spotify, your browser, or any media app, and lets you control it: play/pause, skip, seek, switch players | ✅ |
| ⏳ Timer | Several countdowns at once, with an alert, sound, and notification when one ends | ✅ |
| ⏱️ Stopwatch | Laps, fastest/slowest lap, survives restarts | ✅ |
| 🤖 Sessions | Tracks Claude Code and Codex sessions: working, needs you (permission prompts), done. Connect Claude Code in Settings › Apps | ✅ (Codex: beta) |
| 📋 Clipboard | History of text, images, and files. Search, pin, and click to paste back where you were typing. Skips content that password managers mark private | ✅ |
| 🔊 Volume | Shows the volume in the pill when it changes; can replace the Windows volume popup (Settings › General) | ✅ |
| 📝 Notes | Quick plain-text notes from the island. Saved as you type, searchable, pinnable | ✅ |

## How it behaves

- **Idle:** a pill showing the most important live activity. A second activity appears as a small bubble beside it.
- **Hover:** a compact view of that activity, or the clock and an app launcher.
- **Click:** the full app, with a switcher for the other apps.
- Clicks on empty space around the island pass through to the windows underneath, and the island never takes focus from what you are typing in.
- Tray icon: open Settings, hide or show the island, quit.

## Install

Releases are coming soon. Until then, build from source.

### Build from source

Requirements: Windows 10/11, [Node.js](https://nodejs.org) 20+, [pnpm](https://pnpm.io), [Rust](https://rustup.rs) (MSVC toolchain), and Visual Studio C++ Build Tools.

```powershell
pnpm install
pnpm tauri dev      # run in development
pnpm tauri build    # produce an installer in src-tauri/target/release/bundle
```

## Tech

[Tauri 2](https://tauri.app) (Rust) with a React + TypeScript UI and [Motion](https://motion.dev) for the spring animations. Media control uses Windows' Global System Media Transport Controls. Data is stored in SQLite under `%APPDATA%\Duri`. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Contributing

Ideas and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md) and [docs/ADDING_AN_APP.md](docs/ADDING_AN_APP.md).

## Türkçe

Duri, Windows için bir dinamik ada. Ekranın üstünde küçük siyah bir hap olarak durur. O an olanı gösterir, üzerine gelince açılır, tıklayınca büyür. İçindeki özelliklere "app" denir: Müzik, Zamanlayıcı, Kronometre, ve yolda olanlar: Claude Code/Codex oturum takibi, Pano, Notlar. Arayüz Türkçe ve İngilizce destekler.

## License

[MIT](LICENSE)
