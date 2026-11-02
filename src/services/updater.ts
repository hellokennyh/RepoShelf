/**
 * RepoShelf v1.1.4 Secure Updater Service
 * Provides update checking, state management, version comparison, download progress,
 * and safety gating against active drafts / background tasks.
 */

export type UpdateStatus =
  | "IDLE"
  | "CHECKING"
  | "UP_TO_DATE"
  | "UPDATE_AVAILABLE"
  | "DOWNLOADING"
  | "INSTALLING"
  | "FAILED"
  | "SERVICE_UNAVAILABLE";

export interface UpdateInfo {
  version: string;
  currentVersion: string;
  date?: string;
  body?: string;
}

export interface DownloadProgress {
  downloadedBytes: number;
  totalBytes?: number;
  percentage?: number;
}

export interface UpdateSafetyGuard {
  hasUnsavedDrafts: () => boolean;
  isSaving: () => boolean;
  hasActiveTasks: () => boolean;
}

export interface SafetyCheckResult {
  safe: boolean;
  reason?: "UNSAVED_DRAFTS" | "SAVING_IN_PROGRESS" | "ACTIVE_TASKS";
}

export interface UpdaterAdapter {
  check: (options?: { timeout?: number }) => Promise<{
    available: boolean;
    version?: string;
    currentVersion?: string;
    date?: string;
    body?: string;
    downloadAndInstall?: (onProgress?: (progress: DownloadProgress) => void) => Promise<void>;
  }>;
  relaunch: () => Promise<void>;
}

/**
 * Robust SemVer parser and comparator.
 * Guarantees no dictionary string comparison and handles pre-releases correctly.
 */
export interface SemVer {
  major: number;
  minor: number;
  patch: number;
  prerelease?: string;
}

export function parseSemVer(versionStr: string): SemVer | null {
  const clean = versionStr.trim().replace(/^v/i, "");
  // Regex supporting: major.minor.patch(-prerelease)?
  const match = clean.match(/^(\d+)\.(\d+)\.(\d+)(?:-([0-9A-Za-z.-]+))?$/);
  if (!match) return null;

  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2], 10),
    patch: parseInt(match[3], 10),
    prerelease: match[4] || undefined,
  };
}

/**
 * Returns:
 *  1 if a > b
 * -1 if a < b
 *  0 if a === b
 * throws if either version is invalid SemVer
 */
export function compareSemVer(v1: string, v2: string): number {
  const p1 = parseSemVer(v1);
  const p2 = parseSemVer(v2);

  if (!p1 || !p2) {
    throw new Error(`Invalid SemVer comparison between '${v1}' and '${v2}'`);
  }

  if (p1.major !== p2.major) return p1.major > p2.major ? 1 : -1;
  if (p1.minor !== p2.minor) return p1.minor > p2.minor ? 1 : -1;
  if (p1.patch !== p2.patch) return p1.patch > p2.patch ? 1 : -1;

  // Prerelease comparison:
  // A version with a prerelease has lower precedence than a normal version (e.g. 1.0.0-alpha < 1.0.0)
  if (p1.prerelease && !p2.prerelease) return -1;
  if (!p1.prerelease && p2.prerelease) return 1;
  if (p1.prerelease && p2.prerelease) {
    if (p1.prerelease === p2.prerelease) return 0;
    return p1.prerelease.localeCompare(p2.prerelease);
  }

  return 0;
}

/**
 * Default Tauri Updater Adapter implementation.
 * Wraps @tauri-apps/plugin-updater and @tauri-apps/plugin-process.
 */
let currentAdapter: UpdaterAdapter = {
  check: async () => {
    // Dynamic import to prevent crashing when running in non-Tauri browser / unit test environment
    const { check } = await import("@tauri-apps/plugin-updater");
    const update = await check();
    if (!update) {
      return { available: false };
    }
    return {
      available: true,
      version: update.version,
      currentVersion: update.currentVersion,
      date: update.date,
      body: update.body,
      downloadAndInstall: async (onProgress) => {
        let downloaded = 0;
        let total = 0;
        await update.downloadAndInstall((event) => {
          if (!onProgress) return;
          if (event.event === "Started") {
            total = event.data.contentLength || 0;
            onProgress({
              downloadedBytes: 0,
              totalBytes: total,
              percentage: total > 0 ? 0 : undefined,
            });
          } else if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            const pct = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : undefined;
            onProgress({
              downloadedBytes: downloaded,
              totalBytes: total,
              percentage: pct,
            });
          } else if (event.event === "Finished") {
            onProgress({
              downloadedBytes: downloaded,
              totalBytes: total,
              percentage: 100,
            });
          }
        });
      },
    };
  },
  relaunch: async () => {
    const { relaunch } = await import("@tauri-apps/plugin-process");
    await relaunch();
  },
};

/**
 * Allows switching the updater adapter (e.g. for mock mode, integration tests).
 */
export function setUpstreamUpdaterAdapter(adapter: UpdaterAdapter) {
  currentAdapter = adapter;
}

export function getUpstreamUpdaterAdapter(): UpdaterAdapter {
  return currentAdapter;
}

/**
 * Evaluates whether it is safe to initiate application update & shutdown.
 */
export function checkUpdateSafety(guard?: UpdateSafetyGuard): SafetyCheckResult {
  if (!guard) return { safe: true };

  if (guard.isSaving()) {
    return { safe: false, reason: "SAVING_IN_PROGRESS" };
  }
  if (guard.hasActiveTasks()) {
    return { safe: false, reason: "ACTIVE_TASKS" };
  }
  if (guard.hasUnsavedDrafts()) {
    return { safe: false, reason: "UNSAVED_DRAFTS" };
  }

  return { safe: true };
}
