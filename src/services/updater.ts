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
  id?: string;
  hasUnsavedDrafts: () => boolean;
  isSaving: () => boolean;
  hasActiveTasks: () => boolean;
  getDetails?: () => string | undefined;
}

export interface SafetyCheckResult {
  safe: boolean;
  reason?: "UNSAVED_DRAFTS" | "SAVING_IN_PROGRESS" | "ACTIVE_TASKS" | "UNSUPPORTED_INSTALL_CHANNEL";
  details?: string;
}

export type InstallChannel = "NSIS" | "MSI" | "STANDALONE" | "UNKNOWN";

export interface ChannelCheckResult {
  channel: InstallChannel;
  canAutoUpdate: boolean;
  reason?: "MSI_MANUAL_UPGRADE_REQUIRED" | "STANDALONE_MANUAL_UPGRADE_REQUIRED" | "UNKNOWN_CHANNEL_FAIL_CLOSED";
  details?: string;
}

export interface InstallChannelAdapter {
  detectChannel: () => Promise<ChannelCheckResult>;
}

const registeredGuards = new Map<string, UpdateSafetyGuard>();

export function registerUpdateSafetyGuard(id: string, guard: UpdateSafetyGuard): () => void {
  registeredGuards.set(id, guard);
  return () => {
    registeredGuards.delete(id);
  };
}

export function unregisterUpdateSafetyGuard(id: string): void {
  registeredGuards.delete(id);
}

export function clearUpdateSafetyGuards(): void {
  registeredGuards.clear();
}

export function getRegisteredSafetyGuards(): UpdateSafetyGuard[] {
  return Array.from(registeredGuards.values());
}

let currentChannelAdapter: InstallChannelAdapter = {
  detectChannel: async () => {
    try {
      const { fetchInstallationChannel } = await import("./tauri");
      const info = await fetchInstallationChannel();
      if (info.channel === "NSIS") {
        return {
          channel: "NSIS",
          canAutoUpdate: true,
          details: info.details,
        };
      } else if (info.channel === "MSI") {
        return {
          channel: "MSI",
          canAutoUpdate: false,
          reason: "MSI_MANUAL_UPGRADE_REQUIRED",
          details: info.details,
        };
      } else if (info.channel === "STANDALONE") {
        return {
          channel: "STANDALONE",
          canAutoUpdate: false,
          reason: "STANDALONE_MANUAL_UPGRADE_REQUIRED",
          details: info.details,
        };
      } else {
        return {
          channel: "UNKNOWN",
          canAutoUpdate: false,
          reason: "UNKNOWN_CHANNEL_FAIL_CLOSED",
          details: info.details,
        };
      }
    } catch {
      return {
        channel: "UNKNOWN",
        canAutoUpdate: false,
        reason: "UNKNOWN_CHANNEL_FAIL_CLOSED",
        details: "Channel detection failed; failing closed to prevent unsafe installation",
      };
    }
  },
};

export function setInstallChannelAdapter(adapter: InstallChannelAdapter): void {
  currentChannelAdapter = adapter;
}

export function getInstallChannelAdapter(): InstallChannelAdapter {
  return currentChannelAdapter;
}

export async function detectInstallChannel(): Promise<ChannelCheckResult> {
  return await currentChannelAdapter.detectChannel();
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
 * Aggregates all globally registered guards (e.g. FavoriteDetailModal, session editors)
 * as well as any caller-provided local guard.
 */
export function checkUpdateSafety(guard?: UpdateSafetyGuard): SafetyCheckResult {
  const allGuards: UpdateSafetyGuard[] = [...registeredGuards.values()];
  if (guard) {
    allGuards.push(guard);
  }

  if (allGuards.length === 0) {
    return { safe: true };
  }

  // 1. Check if any guard is saving
  for (const g of allGuards) {
    if (g.isSaving()) {
      return {
        safe: false,
        reason: "SAVING_IN_PROGRESS",
        details: g.getDetails ? g.getDetails() : undefined,
      };
    }
  }

  // 2. Check if any guard has active tasks
  for (const g of allGuards) {
    if (g.hasActiveTasks()) {
      return {
        safe: false,
        reason: "ACTIVE_TASKS",
        details: g.getDetails ? g.getDetails() : undefined,
      };
    }
  }

  // 3. Check if any guard has unsaved drafts
  for (const g of allGuards) {
    if (g.hasUnsavedDrafts()) {
      return {
        safe: false,
        reason: "UNSAVED_DRAFTS",
        details: g.getDetails ? g.getDetails() : undefined,
      };
    }
  }

  return { safe: true };
}

/**
 * Performs comprehensive asynchronous safety check across all frontend guards
 * and backend tasks (e.g. background AI analysis, translations).
 */
export async function checkGlobalUpdateSafety(localGuard?: UpdateSafetyGuard): Promise<SafetyCheckResult> {
  const syncSafety = checkUpdateSafety(localGuard);
  if (!syncSafety.safe) {
    return syncSafety;
  }

  try {
    const { fetchActiveBackendTasks } = await import("./tauri");
    const backendTasks = await fetchActiveBackendTasks();
    if (backendTasks && backendTasks.has_active_tasks) {
      return {
        safe: false,
        reason: "ACTIVE_TASKS",
        details: `Backend tasks running (analysis: ${backendTasks.active_analysis_tasks}, translating: ${backendTasks.is_translating})`,
      };
    }
  } catch {
    // If backend tasks cannot be checked or running in unit test environment, rely on frontend guards
  }

  return { safe: true };
}
