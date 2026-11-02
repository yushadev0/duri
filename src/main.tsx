import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import React from "react";
import ReactDOM from "react-dom/client";
import "./design/tokens.css";
import i18n from "./i18n";
import { Island } from "./island/Island";
import { startSettingsSync } from "./lib/settings";
import { SettingsApp } from "./settings/SettingsApp";

const label = getCurrentWebviewWindow().label;
document.documentElement.dataset.window = label;
startSettingsSync();

if (label === "island") {
  // The tray menu follows the UI language.
  const syncTray = () =>
    invoke("tray_set_labels", {
      labels: {
        settings: i18n.t("tray.settings"),
        hide: i18n.t("tray.hide"),
        show: i18n.t("tray.show"),
        quit: i18n.t("tray.quit"),
      },
    }).catch(() => {});
  syncTray();
  i18n.on("languageChanged", syncTray);
  if (!import.meta.env.DEV) document.addEventListener("contextmenu", (e) => e.preventDefault());
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{label === "settings" ? <SettingsApp /> : <Island />}</React.StrictMode>,
);
