import { describe, it, expect } from "vitest";
import { getErrorMessage } from "../utils/error";

describe("Phase 7 Error Helper — getErrorMessage (P7-GAP-02)", () => {
  it("P7-ERR-01: extracts message from an Error instance", () => {
    const error = new Error("Network failed");
    expect(getErrorMessage(error)).toBe("Network failed");
  });

  it("P7-ERR-02: returns nonblank string as-is", () => {
    expect(getErrorMessage("Network failed")).toBe("Network failed");
    expect(getErrorMessage("   Connection refused   ")).toBe("Connection refused");
  });

  it("P7-ERR-03: extracts message from Tauri IPC error object with message property", () => {
    const tauriError = {
      code: "DATABASE",
      message: "Database unavailable",
    };
    expect(getErrorMessage(tauriError)).toBe("Database unavailable");
  });

  it("P7-ERR-04: returns safe fallback for empty object without message", () => {
    expect(getErrorMessage({})).toBe("操作失败，请重试。");
    expect(getErrorMessage({ code: "UNKNOWN_ERROR" })).toBe("操作失败，请重试。");
  });

  it("P7-ERR-05: returns safe fallback for null or undefined", () => {
    expect(getErrorMessage(null)).toBe("操作失败，请重试。");
    expect(getErrorMessage(undefined)).toBe("操作失败，请重试。");
  });

  it("P7-ERR-06: plain object must NEVER render [object Object]", () => {
    const plainObjects = [
      {},
      { status: 500 },
      { error: "something" },
      { nested: { a: 1 } },
    ];

    for (const obj of plainObjects) {
      const result = getErrorMessage(obj);
      expect(result).not.toContain("[object Object]");
      expect(result).toBe("操作失败，请重试。");
    }
  });

  it("handles empty or whitespace-only messages by returning fallback", () => {
    expect(getErrorMessage(new Error(""))).toBe("操作失败，请重试。");
    expect(getErrorMessage(new Error("   "))).toBe("操作失败，请重试。");
    expect(getErrorMessage("")).toBe("操作失败，请重试。");
    expect(getErrorMessage("   ")).toBe("操作失败，请重试。");
    expect(getErrorMessage({ message: "" })).toBe("操作失败，请重试。");
    expect(getErrorMessage({ message: "   " })).toBe("操作失败，请重试。");
  });

  it("accepts and returns custom fallback if provided", () => {
    expect(getErrorMessage(null, "自定义错误提示")).toBe("自定义错误提示");
    expect(getErrorMessage({}, "自定义错误提示")).toBe("自定义错误提示");
  });

  it("handles non-object primitive types safely without throwing", () => {
    expect(getErrorMessage(123)).toBe("操作失败，请重试。");
    expect(getErrorMessage(true)).toBe("操作失败，请重试。");
    expect(getErrorMessage(Symbol("sym"))).toBe("操作失败，请重试。");
  });
});
