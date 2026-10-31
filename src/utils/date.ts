/**
 * Date and time formatting utility with explicit timezone and locale support.
 * Conforms to RepoShelf v1.1.2 Technical Spec §6 & §18, and v1.1.3 S1 specification.
 */

/**
 * Formats any ISO-8601, UTC timestamp, or date string into zero-padded YYYY/MM/DD.
 * Empty, null, undefined, or invalid dates return fallback (default "—").
 * If the input is a pure date string (YYYY-MM-DD), calendar day is preserved directly.
 */
export function formatDateYYYYMMDD(
  isoString: string | null | undefined,
  timeZone?: string,
  fallback = "—"
): string {
  if (!isoString || typeof isoString !== "string") {
    return fallback;
  }
  const trimmed = isoString.trim();
  if (trimmed === "") {
    return fallback;
  }

  // Pure calendar date "YYYY-MM-DD"
  if (/^\d{4}-\d{2}-\d{2}$/.test(trimmed)) {
    const [yStr, mStr, dStr] = trimmed.split("-");
    const y = parseInt(yStr, 10);
    const m = parseInt(mStr, 10);
    const d = parseInt(dStr, 10);
    if (m < 1 || m > 12 || d < 1 || d > 31) {
      return fallback;
    }
    const daysInMonth = new Date(Date.UTC(y, m, 0)).getUTCDate();
    if (d > daysInMonth) {
      return fallback;
    }
    return `${yStr}/${mStr}/${dStr}`;
  }

  const date = new Date(trimmed);
  if (isNaN(date.getTime())) {
    return fallback;
  }

  try {
    const dtf = new Intl.DateTimeFormat("en-US", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      ...(timeZone ? { timeZone } : {}),
    });
    const parts = dtf.formatToParts(date);
    const year = parts.find((p) => p.type === "year")?.value;
    const month = parts.find((p) => p.type === "month")?.value;
    const day = parts.find((p) => p.type === "day")?.value;
    if (year && month && day) {
      return `${year}/${month}/${day}`;
    }
  } catch {
    // fallback
  }

  return fallback;
}

export function formatRepositoryDate(
  isoString: string | null | undefined,
  locale?: string,
  timeZone?: string,
  fallback = "—"
): string {
  if (!isoString || typeof isoString !== "string") {
    return fallback;
  }
  const trimmed = isoString.trim();
  if (trimmed === "") {
    return fallback;
  }

  const date = new Date(trimmed);
  if (isNaN(date.getTime())) {
    return fallback;
  }

  try {
    const options: Intl.DateTimeFormatOptions = {
      year: "numeric",
      month: "short",
      day: "numeric",
    };
    if (timeZone) {
      options.timeZone = timeZone;
    }
    return new Intl.DateTimeFormat(locale || undefined, options).format(date);
  } catch {
    return fallback;
  }
}

export function formatRepositoryDateTime(
  isoString: string | null | undefined,
  locale?: string,
  timeZone?: string,
  fallback = "—"
): string {
  if (!isoString || typeof isoString !== "string") {
    return fallback;
  }
  const trimmed = isoString.trim();
  if (trimmed === "") {
    return fallback;
  }

  const date = new Date(trimmed);
  if (isNaN(date.getTime())) {
    return fallback;
  }

  try {
    const options: Intl.DateTimeFormatOptions = {
      year: "numeric",
      month: "numeric",
      day: "numeric",
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    };
    if (timeZone) {
      options.timeZone = timeZone;
    }
    return new Intl.DateTimeFormat(locale || undefined, options).format(date);
  } catch {
    return fallback;
  }
}
