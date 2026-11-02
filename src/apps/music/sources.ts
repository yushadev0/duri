/** Friendly names for the app ids (AUMIDs) Windows reports for media sessions. */
const KNOWN: [RegExp, string][] = [
  [/spotify/i, "Spotify"],
  [/msedge/i, "Edge"],
  [/chrome/i, "Chrome"],
  [/firefox|308046B0AF4A39CB/i, "Firefox"],
  [/brave/i, "Brave"],
  [/opera/i, "Opera"],
  [/ZuneMusic/i, "Media Player"],
  [/ZuneVideo/i, "Movies & TV"],
  [/AppleMusic/i, "Apple Music"],
  [/tidal/i, "TIDAL"],
  [/deezer/i, "Deezer"],
  [/vlc/i, "VLC"],
  [/foobar/i, "foobar2000"],
  [/musicbee/i, "MusicBee"],
];

export function sourceName(id: string): string {
  if (!id) return "";
  for (const [re, name] of KNOWN) if (re.test(id)) return name;
  const last = id.split("!").pop() ?? id;
  const base = last.replace(/\.exe$/i, "").split(/[\\/]/).pop() ?? last;
  return base.split(".").pop() || base;
}
