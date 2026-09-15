import React, { createContext, useContext, useEffect, useState } from "react";
import { UiLocale } from "../types/settings";
import { Translations } from "./types";
import { en } from "./locales/en";
import { zhHans } from "./locales/zh-Hans";
import { zhHant } from "./locales/zh-Hant";
import { fetchLocaleInitInfo, updateUiLanguage } from "../services/tauri";

const LOCALE_RESOURCES: Record<UiLocale, Translations> = {
  en,
  "zh-Hans": zhHans,
  "zh-Hant": zhHant,
};

export interface I18nContextType {
  locale: UiLocale;
  translations: Translations;
  setLocale: (newLocale: UiLocale) => Promise<boolean>;
  t: (keyPath: string, params?: Record<string, string | number>) => string;
  isUpdating: boolean;
  initWarning: string | null;
  detectedSourceTag: string | null;
}

export const I18nContext = createContext<I18nContextType | null>(null);

function getNestedValue(obj: unknown, path: string): string | undefined {
  const parts = path.split(".");
  let current: unknown = obj;
  for (const part of parts) {
    if (current && typeof current === "object" && part in (current as Record<string, unknown>)) {
      current = (current as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return typeof current === "string" ? current : undefined;
}

export const I18nProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [locale, setLocaleState] = useState<UiLocale>("zh-Hans");
  const [isUpdating, setIsUpdating] = useState(false);
  const [initWarning, setInitWarning] = useState<string | null>(null);
  const [detectedSourceTag, setDetectedSourceTag] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;
    fetchLocaleInitInfo()
      .then((info) => {
        if (!isMounted) return;
        if (info.locale === "en" || info.locale === "zh-Hans" || info.locale === "zh-Hant") {
          setLocaleState(info.locale);
        }
        if (info.warning) setInitWarning(info.warning);
        if (info.detected_source_tag) setDetectedSourceTag(info.detected_source_tag);
      })
      .catch((err) => {
        console.warn("Failed to fetch initial locale info from Tauri:", err);
      });
    return () => {
      isMounted = false;
    };
  }, []);

  const setLocale = async (newLocale: UiLocale): Promise<boolean> => {
    try {
      setIsUpdating(true);
      await updateUiLanguage(newLocale);
      setLocaleState(newLocale);
      return true;
    } catch (err) {
      console.error("Failed to update UI language via Tauri IPC:", err);
      return false;
    } finally {
      setIsUpdating(false);
    }
  };

  const currentTranslations = LOCALE_RESOURCES[locale] || LOCALE_RESOURCES.en;

  const t = (keyPath: string, params?: Record<string, string | number>): string => {
    let template = getNestedValue(currentTranslations, keyPath);
    if (!template) {
      // Fallback to canonical English
      template = getNestedValue(LOCALE_RESOURCES.en, keyPath);
    }
    if (!template) {
      return keyPath;
    }
    if (params) {
      for (const [k, v] of Object.entries(params)) {
        template = template.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
      }
    }
    return template;
  };

  return (
    <I18nContext.Provider
      value={{
        locale,
        translations: currentTranslations,
        setLocale,
        t,
        isUpdating,
        initWarning,
        detectedSourceTag,
      }}
    >
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = (): I18nContextType => {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    // Return safe fallback if outside provider (e.g. some isolated unit tests)
    return {
      locale: "zh-Hans",
      translations: zhHans,
      setLocale: async () => false,
      t: (keyPath: string, params?: Record<string, string | number>) => {
        let template = getNestedValue(zhHans, keyPath) || getNestedValue(en, keyPath) || keyPath;
        if (params) {
          for (const [k, v] of Object.entries(params)) {
            template = template.replace(new RegExp(`\\{${k}\\}`, "g"), String(v));
          }
        }
        return template;
      },
      isUpdating: false,
      initWarning: null,
      detectedSourceTag: null,
    };
  }
  return ctx;
};
