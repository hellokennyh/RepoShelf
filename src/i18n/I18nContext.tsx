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
  isInitialized: boolean;
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

import { getTestFallbackLocale, setTestFallbackLocale } from "./localeState";
export { setTestFallbackLocale };

export interface I18nProviderProps {
  children: React.ReactNode;
  initialLocale?: UiLocale;
  initialIsInitialized?: boolean;
}

export const I18nProvider: React.FC<I18nProviderProps> = ({
  children,
  initialLocale,
  initialIsInitialized,
}) => {
  const testLocale = getTestFallbackLocale();
  const [isInitialized, setIsInitialized] = useState<boolean>(
    initialIsInitialized ?? (initialLocale !== undefined || testLocale !== null)
  );
  const [locale, setLocaleState] = useState<UiLocale>(
    initialLocale ?? testLocale ?? "en"
  );
  const [isUpdating, setIsUpdating] = useState(false);
  const [initWarning, setInitWarning] = useState<string | null>(null);
  const [detectedSourceTag, setDetectedSourceTag] = useState<string | null>(null);

  useEffect(() => {
    let isMounted = true;
    fetchLocaleInitInfo()
      .then((info) => {
        if (!isMounted) return;
        if (initialLocale !== undefined) {
          setIsInitialized(true);
          return;
        }
        if (info && (info.locale === "en" || info.locale === "zh-Hans" || info.locale === "zh-Hant")) {
          setLocaleState(info.locale);
        } else {
          setLocaleState(testLocale ?? "en");
        }
        if (info?.warning) setInitWarning(info.warning);
        if (info?.detected_source_tag) setDetectedSourceTag(info.detected_source_tag);
        setIsInitialized(true);
      })
      .catch((err) => {
        console.warn("Failed to fetch initial locale info from Tauri:", err);
        if (!isMounted) return;
        if (initialLocale === undefined) {
          setLocaleState(testLocale ?? "en");
        }
        setIsInitialized(true);
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

  if (!isInitialized) {
    return (
      <div
        className="min-h-screen bg-slate-900 flex items-center justify-center text-slate-400"
        data-testid="i18n-loading-shell"
      >
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-500" />
      </div>
    );
  }

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
        isInitialized,
      }}
    >
      {children}
    </I18nContext.Provider>
  );
};

export const useI18n = (): I18nContextType => {
  const ctx = useContext(I18nContext);
  if (!ctx) {
    const fallback = getTestFallbackLocale() ?? "en";
    const currentTranslations = LOCALE_RESOURCES[fallback] || LOCALE_RESOURCES.en;
    return {
      locale: fallback,
      translations: currentTranslations,
      setLocale: async () => false,
      t: (keyPath: string, params?: Record<string, string | number>) => {
        let template =
          getNestedValue(currentTranslations, keyPath) ||
          getNestedValue(LOCALE_RESOURCES.en, keyPath) ||
          keyPath;
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
      isInitialized: true,
    };
  }
  return ctx;
};
