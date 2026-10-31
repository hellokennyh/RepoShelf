import { describe, it, expect } from "vitest";
import { formatDateYYYYMMDD } from "../utils/date";
import { formatCategory } from "../utils/category";
import { ALL_CATEGORIES } from "../types/library";
import { en } from "../i18n/locales/en";
import { zhHans } from "../i18n/locales/zh-Hans";
import { zhHant } from "../i18n/locales/zh-Hant";

describe("S1: formatDateYYYYMMDD Unit Tests", () => {
  it("formats null, undefined, empty, and whitespace as fallback", () => {
    expect(formatDateYYYYMMDD(null)).toBe("—");
    expect(formatDateYYYYMMDD(undefined)).toBe("—");
    expect(formatDateYYYYMMDD("")).toBe("—");
    expect(formatDateYYYYMMDD("   ")).toBe("—");
    expect(formatDateYYYYMMDD(null, undefined, "N/A")).toBe("N/A");
  });

  it("formats invalid or malformed date strings safely as fallback", () => {
    expect(formatDateYYYYMMDD("invalid-date")).toBe("—");
    expect(formatDateYYYYMMDD("2026-99-99")).toBe("—");
    expect(formatDateYYYYMMDD("not-a-timestamp", undefined, "Unknown")).toBe("Unknown");
  });

  it("formats pure calendar dates (YYYY-MM-DD) directly without timezone day drift", () => {
    expect(formatDateYYYYMMDD("2026-07-03")).toBe("2026/07/03");
    expect(formatDateYYYYMMDD("2026-01-09")).toBe("2026/01/09");
    expect(formatDateYYYYMMDD("2026-12-31")).toBe("2026/12/31");
    expect(formatDateYYYYMMDD("2024-02-29")).toBe("2024/02/29"); // leap day
  });

  it("formats UTC timestamps with zero padding in specified timezones", () => {
    const leapTimestamp = "2024-02-29T16:30:00Z";

    // In UTC, this is Feb 29, 2024
    expect(formatDateYYYYMMDD(leapTimestamp, "UTC")).toBe("2024/02/29");

    // In Asia/Shanghai (+8), 16:30 UTC Feb 29 rolls into March 01
    expect(formatDateYYYYMMDD(leapTimestamp, "Asia/Shanghai")).toBe("2024/03/01");

    // In America/New_York (-5), 16:30 UTC is 11:30 AM Feb 29
    expect(formatDateYYYYMMDD(leapTimestamp, "America/New_York")).toBe("2024/02/29");
  });

  it("handles negative offset timezone month boundary correctly", () => {
    const sample = "2024-03-01T02:00:00Z";
    // In UTC: March 1
    expect(formatDateYYYYMMDD(sample, "UTC")).toBe("2024/03/01");
    // In New York (-5): Feb 29 21:00
    expect(formatDateYYYYMMDD(sample, "America/New_York")).toBe("2024/02/29");
  });

  it("S1R-B: validates timezone semantics for 2026-03-01 day boundaries across UTC, Asia/Shanghai, and America/Los_Angeles", () => {
    // 1. Early morning UTC timestamp: 2026-03-01T00:30:00Z
    const tEarly = "2026-03-01T00:30:00Z";
    // UTC: 2026/03/01
    expect(formatDateYYYYMMDD(tEarly, "UTC")).toBe("2026/03/01");
    // Asia/Shanghai (+8: 08:30 on March 1): 2026/03/01
    expect(formatDateYYYYMMDD(tEarly, "Asia/Shanghai")).toBe("2026/03/01");
    // America/Los_Angeles (-8: Feb 28 16:30): 2026/02/28
    expect(formatDateYYYYMMDD(tEarly, "America/Los_Angeles")).toBe("2026/02/28");

    // 2. Late night UTC timestamp: 2026-03-01T23:30:00Z
    const tLate = "2026-03-01T23:30:00Z";
    // UTC: 2026/03/01
    expect(formatDateYYYYMMDD(tLate, "UTC")).toBe("2026/03/01");
    // Asia/Shanghai (+8: March 2 07:30): 2026/03/02
    expect(formatDateYYYYMMDD(tLate, "Asia/Shanghai")).toBe("2026/03/02");
    // America/Los_Angeles (-8: March 1 15:30): 2026/03/01
    expect(formatDateYYYYMMDD(tLate, "America/Los_Angeles")).toBe("2026/03/01");

    // 3. Timestamps with explicit timezone offsets
    const tOffsetPlus8 = "2026-03-01T08:30:00+08:00"; // 00:30 UTC
    expect(formatDateYYYYMMDD(tOffsetPlus8, "Asia/Shanghai")).toBe("2026/03/01");
    expect(formatDateYYYYMMDD(tOffsetPlus8, "America/Los_Angeles")).toBe("2026/02/28");

    const tOffsetMinus8 = "2026-03-01T15:30:00-08:00"; // 23:30 UTC
    expect(formatDateYYYYMMDD(tOffsetMinus8, "America/Los_Angeles")).toBe("2026/03/01");
    expect(formatDateYYYYMMDD(tOffsetMinus8, "Asia/Shanghai")).toBe("2026/03/02");

    // 4. Pure calendar date string (YYYY-MM-DD): preserved directly in any timezone
    expect(formatDateYYYYMMDD("2026-03-01", "UTC")).toBe("2026/03/01");
    expect(formatDateYYYYMMDD("2026-03-01", "Asia/Shanghai")).toBe("2026/03/01");
    expect(formatDateYYYYMMDD("2026-03-01", "America/Los_Angeles")).toBe("2026/03/01");
  });

  it("ensures 4-digit year, 2-digit month, and 2-digit day with slash separator", () => {
    const result = formatDateYYYYMMDD("2025-05-06T00:00:00Z", "UTC");
    expect(result).toBe("2025/05/06");
    expect(result).toMatch(/^\d{4}\/\d{2}\/\d{2}$/);
  });
});

describe("S1: formatCategory & Category Translations Parity", () => {
  const createMockT = (localeResource: Record<string, unknown>) => {
    return (keyPath: string): string => {
      const parts = keyPath.split(".");
      let cur: unknown = localeResource;
      for (const p of parts) {
        if (cur && typeof cur === "object" && p in (cur as Record<string, unknown>)) {
          cur = (cur as Record<string, unknown>)[p];
        } else {
          return keyPath;
        }
      }
      return typeof cur === "string" ? cur : keyPath;
    };
  };

  const tEn = createMockT(en as unknown as Record<string, unknown>);
  const tHans = createMockT(zhHans as unknown as Record<string, unknown>);
  const tHant = createMockT(zhHant as unknown as Record<string, unknown>);

  it("has exactly 16 standard categories defined in ALL_CATEGORIES", () => {
    expect(ALL_CATEGORIES.length).toBe(16);
  });

  it("translates every one of the 16 standard categories in zh-Hans", () => {
    for (const cat of ALL_CATEGORIES) {
      const translated = formatCategory(tHans, cat);
      expect(translated).not.toBe("");
      expect(translated).not.toBe(`categories.${cat}`);
      // Must not contain untranslated English except allowed proper names like AI, Web, SaaS, DevOps, UI
      expect(typeof translated).toBe("string");
    }

    // Specific check for Automation
    expect(formatCategory(tHans, "Automation")).toBe("自动化");
    expect(formatCategory(tHans, "AI & Agents")).toBe("AI 与智能体");
    expect(formatCategory(tHans, "Developer Tools")).toBe("开发者工具");
    expect(formatCategory(tHans, "Web & SaaS")).toBe("Web 与 SaaS");
    expect(formatCategory(tHans, "Data & Database")).toBe("数据与数据库");
    expect(formatCategory(tHans, "DevOps & Cloud")).toBe("DevOps 与云原生");
    expect(formatCategory(tHans, "Security & Privacy")).toBe("安全与隐私");
    expect(formatCategory(tHans, "Libraries & Frameworks")).toBe("开发库与框架");
  });

  it("translates every one of the 16 standard categories in zh-Hant", () => {
    for (const cat of ALL_CATEGORIES) {
      const translated = formatCategory(tHant, cat);
      expect(translated).not.toBe("");
      expect(translated).not.toBe(`categories.${cat}`);
      expect(typeof translated).toBe("string");
    }

    expect(formatCategory(tHant, "Automation")).toBe("自動化");
    expect(formatCategory(tHant, "AI & Agents")).toBe("AI 與智慧體");
    expect(formatCategory(tHant, "Developer Tools")).toBe("開發者工具");
  });

  it("translates every one of the 16 standard categories in en", () => {
    for (const cat of ALL_CATEGORIES) {
      const translated = formatCategory(tEn, cat);
      expect(translated).toBe(cat);
    }
  });

  it("safely falls back to the raw category name for unknown categories without categories. prefix", () => {
    const unknownCat = "Quantum Computing";
    expect(formatCategory(tHans, unknownCat)).toBe("Quantum Computing");
    expect(formatCategory(tHant, unknownCat)).toBe("Quantum Computing");
    expect(formatCategory(tEn, unknownCat)).toBe("Quantum Computing");
  });

  it("handles null, undefined, empty category strings gracefully", () => {
    expect(formatCategory(tHans, null)).toBe("");
    expect(formatCategory(tHans, undefined)).toBe("");
    expect(formatCategory(tHans, "")).toBe("");
    expect(formatCategory(tHans, "   ")).toBe("");
  });
});
