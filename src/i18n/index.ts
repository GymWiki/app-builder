import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";
import en from "./en.json";
import nl from "./nl.json";

export const LANGUAGES = ["en", "nl"] as const;
export type Language = (typeof LANGUAGES)[number];

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: { en: { translation: en }, nl: { translation: nl } },
    supportedLngs: LANGUAGES,
    nonExplicitSupportedLngs: true,
    load: "languageOnly",
    fallbackLng: "en",
    interpolation: { escapeValue: false },
    detection: { order: ["localStorage", "navigator"], lookupLocalStorage: "studio-lang", caches: ["localStorage"] },
    returnNull: false,
  });

i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = lng.slice(0, 2);
});
document.documentElement.lang = i18n.language.slice(0, 2);

export default i18n;
