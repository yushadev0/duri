import { musicApp } from "./music";
import { stopwatchApp } from "./timers/stopwatch";
import { timerApp } from "./timers/timer";
import type { DuriApp } from "./types";

/**
 * Every app Duri ships with, in launcher order. To add an app, build it under
 * src/apps/<id>/ and list it here (see docs/ADDING_AN_APP.md).
 */
export const registry: DuriApp[] = [musicApp, timerApp, stopwatchApp];
