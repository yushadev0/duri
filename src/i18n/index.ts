import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import { settingsStore, type LanguagePref } from "../lib/settings";
import en from "./en.json";
import tr from "./tr.json";

export type Language = "en" | "tr";

export function resolveLanguage(pref: LanguagePref, system = navigator.language): Language {
  if (pref === "en" || pref === "tr") return pref;
  return system.toLowerCase().startsWith("tr") ? "tr" : "en";
}

i18n.use(initReactI18next).init({
  resources: { en: { translation: en }, tr: { translation: tr } },
  lng: resolveLanguage(settingsStore.get().language),
  fallbackLng: "en",
  interpolation: { escapeValue: false },
});

settingsStore.subscribe(() => {
  const lng = resolveLanguage(settingsStore.get().language);
  if (i18n.language !== lng) i18n.changeLanguage(lng);
});

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng;
});

export default i18n;
