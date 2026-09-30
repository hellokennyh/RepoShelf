import { describe, it, expect } from "vitest";
import { formatRepositoryDate, formatRepositoryDateTime } from "../utils/date";

describe("dateUtils - formatRepositoryDate & formatRepositoryDateTime", () => {
  const sampleIso = "2024-02-29T16:30:00Z"; // Leap day, 16:30 UTC

  it("formats date in UTC across leap day", () => {
    const formattedUtc = formatRepositoryDate(sampleIso, "en-US", "UTC");
    expect(formattedUtc).toContain("Feb 29, 2024");
  });

  it("formats date across timezone boundaries (+8 Shanghai -> March 1)", () => {
    const formattedShanghai = formatRepositoryDate(sampleIso, "zh-CN", "Asia/Shanghai");
    // 2024-02-29 16:30 UTC is 2024-03-01 00:30 in Shanghai
    expect(formattedShanghai).toContain("2024");
    expect(formattedShanghai).toContain("3");
    expect(formattedShanghai).toContain("1");
  });

  it("formats date across timezone boundaries (-5 New York -> Feb 29 11:30 AM)", () => {
    const formattedNy = formatRepositoryDate(sampleIso, "en-US", "America/New_York");
    expect(formattedNy).toContain("Feb 29, 2024");
  });

  it("formats date and time accurately with formatRepositoryDateTime", () => {
    const formatted = formatRepositoryDateTime(sampleIso, "en-US", "UTC");
    expect(formatted).toContain("2024");
    expect(formatted).toMatch(/4:30|16:30/);
  });

  it("handles empty or null or undefined gracefully with fallback", () => {
    expect(formatRepositoryDate(null)).toBe("—");
    expect(formatRepositoryDate(undefined)).toBe("—");
    expect(formatRepositoryDate("")).toBe("—");
    expect(formatRepositoryDate("   ")).toBe("—");
    expect(formatRepositoryDate(null, undefined, undefined, "未知")).toBe("未知");
    expect(formatRepositoryDate(undefined, undefined, undefined, "Unknown")).toBe("Unknown");

    expect(formatRepositoryDateTime(null)).toBe("—");
    expect(formatRepositoryDateTime(undefined)).toBe("—");
    expect(formatRepositoryDateTime("")).toBe("—");
    expect(formatRepositoryDateTime("invalid-date-string")).toBe("—");
    expect(formatRepositoryDateTime("invalid-date-string", undefined, undefined, "CustomFallback")).toBe("CustomFallback");
  });

  it("handles malformed string without throwing", () => {
    expect(formatRepositoryDate("not-a-real-date")).toBe("—");
    expect(formatRepositoryDate("2024-99-99T99:99:99Z")).toBe("—");
  });

  it("supports zh-Hans, zh-Hant, and en locales", () => {
    const en = formatRepositoryDate("2023-10-15T00:00:00Z", "en-US", "UTC");
    const zhHans = formatRepositoryDate("2023-10-15T00:00:00Z", "zh-Hans", "UTC");
    const zhHant = formatRepositoryDate("2023-10-15T00:00:00Z", "zh-Hant", "UTC");

    expect(en).toContain("Oct 15, 2023");
    expect(zhHans).toMatch(/2023.*10.*15/);
    expect(zhHant).toMatch(/2023.*10.*15/);
  });
});
