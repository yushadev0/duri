import { bluetoothApp } from "./bluetooth";
import { clipboardApp } from "./clipboard";
import { keysApp } from "./keys";
import { musicApp } from "./music";
import { notesApp } from "./notes";
import { notificationsApp } from "./notifications";
import { sessionsApp } from "./sessions";
import { stopwatchApp } from "./timers/stopwatch";
import { timerApp } from "./timers/timer";
import { volumeApp } from "./volume";
import type { DuriApp } from "./types";

/**
 * Every app Duri ships with, in launcher order. To add an app, build it under
 * src/apps/<id>/ and list it here (see docs/ADDING_AN_APP.md).
 */
export const registry: DuriApp[] = [
  musicApp,
  sessionsApp,
  notificationsApp,
  clipboardApp,
  notesApp,
  timerApp,
  stopwatchApp,
  volumeApp,
  keysApp,
  bluetoothApp,
];
