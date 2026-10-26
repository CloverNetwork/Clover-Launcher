/**
 * Translations. English text is its own key, so anything without a translation stays
 * English rather than showing a key. The language follows the system unless someone picks
 * one in Settings.
 */
import { de } from "./locales/de";

export type Language = "system" | "en" | "de";
const LANGUAGE_KEY = "clover.language";

export function storedLanguage(): Language {
  try {
    const value = localStorage.getItem(LANGUAGE_KEY);
    return value === "en" || value === "de" ? value : "system";
  } catch {
    return "system";
  }
}

export function setLanguage(language: Language) {
  try {
    if (language === "system") localStorage.removeItem(LANGUAGE_KEY);
    else localStorage.setItem(LANGUAGE_KEY, language);
  } catch {
    // Without storage, the system language it is.
  }
}

function resolve(): "en" | "de" {
  const chosen = storedLanguage();
  if (chosen !== "system") return chosen;
  const system = typeof navigator === "undefined" ? "en" : navigator.language.toLowerCase();
  return system.startsWith("de") ? "de" : "en";
}

export const language = resolve();
/** For dates and numbers: the picked language's format, or with "system", the system's own. */
export const locale = { system: undefined, en: "en-US", de: "de-DE" }[storedLanguage()];
const table: Record<string, string> = language === "de" ? de : {};

/** Translates `text`, filling `{name}` placeholders from `values`. */
export function t(text: string, values?: Record<string, string | number>): string {
  let out = table[text] ?? text;
  if (values) {
    for (const [key, value] of Object.entries(values)) out = out.replaceAll(`{${key}}`, String(value));
  }
  return out;
}
