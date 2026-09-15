import { describe, it, expect } from "vitest";
import { en } from "../i18n/locales/en";
import { zhHans } from "../i18n/locales/zh-Hans";
import { zhHant } from "../i18n/locales/zh-Hant";

function getAllKeyPaths(obj: Record<string, unknown>, prefix = ""): string[] {
  let keys: string[] = [];
  for (const [k, v] of Object.entries(obj)) {
    const fullKey = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) {
      keys = keys.concat(getAllKeyPaths(v as Record<string, unknown>, fullKey));
    } else {
      keys.push(fullKey);
    }
  }
  return keys;
}

function getValueByPath(obj: Record<string, unknown>, path: string): unknown {
  const parts = path.split(".");
  let cur: unknown = obj;
  for (const part of parts) {
    if (cur && typeof cur === "object" && part in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[part];
    } else {
      return undefined;
    }
  }
  return cur;
}

function extractPlaceholders(text: string): string[] {
  const matches = text.match(/\{[a-zA-Z0-9_]+\}/g);
  return matches ? matches.sort() : [];
}

describe("S4 I18N Resource Parity and Completeness", () => {
  const enKeyPaths = getAllKeyPaths(en as unknown as Record<string, unknown>);
  const zhHansKeyPaths = getAllKeyPaths(zhHans as unknown as Record<string, unknown>);
  const zhHantKeyPaths = getAllKeyPaths(zhHant as unknown as Record<string, unknown>);

  it("canonical English dictionary contains valid keys", () => {
    expect(enKeyPaths.length).toBeGreaterThan(50);
  });

  it("every canonical English key exists in zh-Hans and is a non-empty string", () => {
    for (const key of enKeyPaths) {
      const val = getValueByPath(zhHans as unknown as Record<string, unknown>, key);
      expect(typeof val, `Missing or non-string key in zh-Hans: ${key}`).toBe("string");
      expect((val as string).trim().length, `Empty string for key in zh-Hans: ${key}`).toBeGreaterThan(0);
    }
  });

  it("every canonical English key exists in zh-Hant and is a non-empty string", () => {
    for (const key of enKeyPaths) {
      const val = getValueByPath(zhHant as unknown as Record<string, unknown>, key);
      expect(typeof val, `Missing or non-string key in zh-Hant: ${key}`).toBe("string");
      expect((val as string).trim().length, `Empty string for key in zh-Hant: ${key}`).toBeGreaterThan(0);
    }
  });

  it("no extraneous keys exist in zh-Hans that are missing in canonical English", () => {
    for (const key of zhHansKeyPaths) {
      const val = getValueByPath(en as unknown as Record<string, unknown>, key);
      expect(typeof val, `Extraneous key in zh-Hans not in en: ${key}`).toBe("string");
    }
  });

  it("no extraneous keys exist in zh-Hant that are missing in canonical English", () => {
    for (const key of zhHantKeyPaths) {
      const val = getValueByPath(en as unknown as Record<string, unknown>, key);
      expect(typeof val, `Extraneous key in zh-Hant not in en: ${key}`).toBe("string");
    }
  });

  it("interpolated parameter placeholders match between canonical English and localized variants", () => {
    for (const key of enKeyPaths) {
      const enVal = getValueByPath(en as unknown as Record<string, unknown>, key) as string;
      const zhHansVal = getValueByPath(zhHans as unknown as Record<string, unknown>, key) as string;
      const zhHantVal = getValueByPath(zhHant as unknown as Record<string, unknown>, key) as string;

      const enPlaceholders = extractPlaceholders(enVal);
      const zhHansPlaceholders = extractPlaceholders(zhHansVal);
      const zhHantPlaceholders = extractPlaceholders(zhHantVal);

      expect(zhHansPlaceholders, `Placeholder mismatch in zh-Hans for key: ${key}`).toEqual(enPlaceholders);
      expect(zhHantPlaceholders, `Placeholder mismatch in zh-Hant for key: ${key}`).toEqual(enPlaceholders);
    }
  });
});
