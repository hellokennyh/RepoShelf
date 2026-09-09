/**
 * Shared safe error extraction utility for Tauri IPC calls and UI operations.
 *
 * Requirements:
 * - If input is an Error and has a nonblank message: return message.
 * - If input is a nonblank string: return string.
 * - If input is a non-null object containing a nonblank string property `message`: return that message.
 * - For all other values: return a concise safe fallback (default: "操作失败，请重试。").
 * - Plain objects must NEVER render "[object Object]".
 * - Never JSON.stringify arbitrary unknown error objects for display.
 * - Never expose raw response bodies, headers, stack traces, or secrets.
 */
export function getErrorMessage(
  err: unknown,
  fallback = "操作失败，请重试。"
): string {
  if (err == null) {
    return fallback;
  }

  if (err instanceof Error) {
    const msg = err.message?.trim();
    return msg ? msg : fallback;
  }

  if (typeof err === "string") {
    const trimmed = err.trim();
    return trimmed ? trimmed : fallback;
  }

  if (typeof err === "object") {
    try {
      const candidate = (err as Record<string, unknown>).message;
      if (typeof candidate === "string") {
        const trimmed = candidate.trim();
        if (trimmed.length > 0) {
          return trimmed;
        }
      }
    } catch {
      return fallback;
    }
  }

  return fallback;
}
