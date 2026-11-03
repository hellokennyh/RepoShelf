import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, act } from "@testing-library/react";
import { SettingsPage } from "../pages/SettingsPage";
import { I18nProvider } from "../i18n/I18nContext";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import * as tauriService from "../services/tauri";
import {
  compareSemVer,
  parseSemVer,
  checkUpdateSafety,
  checkGlobalUpdateSafety,
  setUpstreamUpdaterAdapter,
  UpdaterAdapter,
  DownloadProgress,
  registerUpdateSafetyGuard,
  clearUpdateSafetyGuards,
  setInstallChannelAdapter,
} from "../services/updater";
import * as fs from "fs";
import * as path from "path";

// Mock Tauri backend services
vi.mock("../services/tauri", () => ({
  fetchSettings: vi.fn(),
  saveSettings: vi.fn(),
  fetchSecretStatus: vi.fn(),
  saveGitHubToken: vi.fn(),
  clearGitHubToken: vi.fn(),
  saveAiApiKey: vi.fn(),
  clearAiApiKey: vi.fn(),
  testGitHubConnection: vi.fn(),
  testAiConnection: vi.fn(),
  discoverOllamaEndpoints: vi.fn(),
  listOllamaModels: vi.fn(),
  openGitHubUrl: vi.fn(),
  fetchLocaleInitInfo: vi.fn(),
  fetchInstallationChannel: vi.fn(),
  fetchActiveBackendTasks: vi.fn(),
}));

const mockSettings = {
  schema_version: 1,
  github: { default_search_limit: 20, default_trending_period: "daily" as const },
  ai: {
    enabled: false,
    provider: "openai_compatible" as const,
    model: "gpt-4o",
    api_endpoint: null,
    base_url: null,
    temperature: 0.7,
    max_tokens: null,
    stream: false,
    repository_analysis_context_tokens: null,
  },
  ui: { locale: "zh-Hans" as const },
};

const mockSecretStatus = {
  github_token_configured: false,
  ai_api_key_configured: false,
};

function renderSettingsPage() {
  return render(
    <I18nProvider>
      <DiscoverySessionProvider>
        <SettingsPage />
      </DiscoverySessionProvider>
    </I18nProvider>
  );
}

describe("S2: Updater Engine & Settings UI Acceptance Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    clearUpdateSafetyGuards();
    vi.mocked(tauriService.fetchSettings).mockResolvedValue(mockSettings);
    vi.mocked(tauriService.fetchSecretStatus).mockResolvedValue(mockSecretStatus);
    vi.mocked(tauriService.fetchLocaleInitInfo).mockResolvedValue({
      locale: "zh-Hans",
    });
    vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
      channel: "NSIS",
      can_auto_update: true,
      details: "Mocked NSIS channel",
    });
    vi.mocked(tauriService.fetchActiveBackendTasks).mockResolvedValue({
      active_analysis_tasks: 0,
      is_translating: false,
      has_active_tasks: false,
    });
  });

  describe("1. SemVer Parsing & Strict Comparison (No Lexicographical Bug)", () => {
    it("parses standard semver and prereleases correctly", () => {
      expect(parseSemVer("1.1.4")).toEqual({ major: 1, minor: 1, patch: 4, prerelease: undefined });
      expect(parseSemVer("v1.1.4")).toEqual({ major: 1, minor: 1, patch: 4, prerelease: undefined });
      expect(parseSemVer("1.1.4-beta.1")).toEqual({ major: 1, minor: 1, patch: 4, prerelease: "beta.1" });
      expect(parseSemVer("invalid-version")).toBeNull();
    });

    it("compares higher, lower, and identical versions correctly", () => {
      expect(compareSemVer("1.1.4", "1.1.3")).toBe(1);
      expect(compareSemVer("1.1.3", "1.1.4")).toBe(-1);
      expect(compareSemVer("1.1.3", "1.1.3")).toBe(0);
    });

    it("handles multi-digit numbers without string dictionary sort error (e.g. 1.1.10 > 1.1.9)", () => {
      expect(compareSemVer("1.1.10", "1.1.9")).toBe(1);
      expect(compareSemVer("1.1.9", "1.1.10")).toBe(-1);
      expect(compareSemVer("1.10.0", "1.2.0")).toBe(1);
    });

    it("treats prerelease as lower precedence than stable release", () => {
      expect(compareSemVer("1.1.4-beta.1", "1.1.4")).toBe(-1);
      expect(compareSemVer("1.1.4", "1.1.4-beta.1")).toBe(1);
    });

    it("throws on invalid semver inputs", () => {
      expect(() => compareSemVer("bad", "1.1.3")).toThrow();
    });
  });

  describe("2. Safety Guard: Draft, Saving & Active Task Blocking", () => {
    it("permits update when all conditions are clean", () => {
      const result = checkUpdateSafety({
        hasUnsavedDrafts: () => false,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });
      expect(result.safe).toBe(true);
      expect(result.reason).toBeUndefined();
    });

    it("blocks update when unsaved drafts exist", () => {
      const result = checkUpdateSafety({
        hasUnsavedDrafts: () => true,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });
      expect(result.safe).toBe(false);
      expect(result.reason).toBe("UNSAVED_DRAFTS");
    });

    it("blocks update when saving is in progress", () => {
      const result = checkUpdateSafety({
        hasUnsavedDrafts: () => false,
        isSaving: () => true,
        hasActiveTasks: () => false,
      });
      expect(result.safe).toBe(false);
      expect(result.reason).toBe("SAVING_IN_PROGRESS");
    });

    it("blocks update when active background tasks are running", () => {
      const result = checkUpdateSafety({
        hasUnsavedDrafts: () => false,
        isSaving: () => false,
        hasActiveTasks: () => true,
      });
      expect(result.safe).toBe(false);
      expect(result.reason).toBe("ACTIVE_TASKS");
    });
  });

  describe("3. Settings UI: Check For Updates & Lifecycle States", () => {
    it("renders About & Updates card initially in IDLE state without auto-check", async () => {
      renderSettingsPage();
      await waitFor(() => {
        expect(screen.getByTestId("updater-card")).toBeInTheDocument();
      });

      expect(screen.getByTestId("updater-current-version")).toHaveTextContent("v1.1.4");
      expect(screen.getByTestId("check-update-btn")).toBeEnabled();
      expect(screen.queryByTestId("updater-available-panel")).not.toBeInTheDocument();
      expect(screen.queryByTestId("updater-progress-panel")).not.toBeInTheDocument();
    });

    it("shows 'Up to date' when no new version exists", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({ available: false }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-status")).toHaveTextContent(/已是最新版本|You're up to date/);
      });
      expect(mockAdapter.check).toHaveBeenCalledTimes(1);
    });

    it("shows 'Up to date' when candidate version is identical or lower", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.4",
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-status")).toHaveTextContent(/已是最新版本|You're up to date/);
      });
      expect(screen.queryByTestId("updater-available-panel")).not.toBeInTheDocument();
    });

    it("shows UPDATE_AVAILABLE panel with version, notes, date when newer release is found", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          date: "2026-11-02",
          body: "Release Notes: NSIS Multilingual and Secure Updater.",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-available-panel")).toBeInTheDocument();
      });

      expect(screen.getByTestId("updater-new-version")).toHaveTextContent("v1.1.5");
      expect(screen.getByTestId("updater-release-date")).toHaveTextContent("2026-11-02");
      expect(screen.getByTestId("updater-release-notes")).toHaveTextContent("Release Notes: NSIS Multilingual");
      expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument();
      expect(screen.getByTestId("updater-cancel-btn")).toBeInTheDocument();
    });

    it("collapses panel when user clicks cancel / later button", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("updater-available-panel")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("updater-cancel-btn"));
      expect(screen.queryByTestId("updater-available-panel")).not.toBeInTheDocument();
    });

    it("blocks download and displays warning if unsaved settings exist", async () => {
      const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      // Trigger update available
      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      // Simulate user editing a setting to create unsaved draft
      const searchLimitInput = screen.getByLabelText(/默认搜索结果上限|Default Search Limit/i);
      fireEvent.change(searchLimitInput, { target: { value: "50" } });

      // Click download and install
      fireEvent.click(screen.getByTestId("download-and-install-btn"));

      // Must be blocked!
      await waitFor(() => {
        expect(screen.getByTestId("updater-safety-warning")).toBeInTheDocument();
      });
      expect(downloadAndInstall).not.toHaveBeenCalled();
    });

    it("executes download with progress and initiates relaunch on completion", async () => {
      let progressCallback: ((p: DownloadProgress) => void) | undefined;
      const downloadAndInstall = vi.fn().mockImplementation(async (onProgress) => {
        progressCallback = onProgress;
        if (progressCallback) {
          progressCallback({ downloadedBytes: 500, totalBytes: 1000, percentage: 50 });
        }
      });
      const relaunch = vi.fn().mockResolvedValue(undefined);

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch,
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      await act(async () => {
        fireEvent.click(screen.getByTestId("download-and-install-btn"));
      });

      expect(downloadAndInstall).toHaveBeenCalled();
      expect(relaunch).toHaveBeenCalled();
    });

    it("handles 404 / 403 / network errors as SERVICE_UNAVAILABLE with retry option", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockRejectedValue(new Error("HTTP 404 Not Found")),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-error-panel")).toBeInTheDocument();
      });
      expect(screen.getByTestId("updater-status")).toHaveTextContent(/更新服务暂不可用|Update service unavailable/);
      expect(screen.getByTestId("updater-retry-btn")).toBeInTheDocument();
    });

    it("handles unexpected errors as FAILED with error message and retry option", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockRejectedValue(new Error("Corrupted signature manifest")),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-error-panel")).toBeInTheDocument();
      });
      expect(screen.getByTestId("updater-error-text")).toHaveTextContent("Corrupted signature manifest");
      expect(screen.getByTestId("updater-retry-btn")).toBeInTheDocument();
    });

    it("disables duplicate concurrent clicks during check", async () => {
      let finishCheck: () => void = () => {};
      const pendingPromise = new Promise<{ available: boolean }>((resolve) => {
        finishCheck = () => resolve({ available: false });
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockReturnValue(pendingPromise),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      const checkBtn = screen.getByTestId("check-update-btn");
      fireEvent.click(checkBtn);

      // Button should now be disabled
      expect(checkBtn).toBeDisabled();

      // Second click does nothing
      fireEvent.click(checkBtn);
      expect(mockAdapter.check).toHaveBeenCalledTimes(1);

      // Resolve pending check
      await act(async () => {
        finishCheck();
      });
      await waitFor(() => expect(checkBtn).toBeEnabled());
    });
  });

  describe("4. Tauri Capabilities Verification", () => {
    it("default capability grants updater:default and process:allow-restart", () => {
      const capPath = path.resolve(__dirname, "../../src-tauri/capabilities/default.json");
      const content = fs.readFileSync(capPath, "utf-8");
      const json = JSON.parse(content);

      expect(json.permissions).toContain("updater:default");
      expect(json.permissions).toContain("process:allow-restart");
      expect(json.permissions).not.toContain("shell:allow-execute");
    });
  });

  describe("5. Installation Channel Protection Suite (S5B)", () => {
    it("allows download & install when verified NSIS channel is detected", async () => {
      vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
        channel: "NSIS",
        can_auto_update: true,
        details: "Verified NSIS",
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument();
      });
      expect(screen.queryByTestId("updater-channel-blocked-notice")).not.toBeInTheDocument();
      expect(screen.queryByTestId("manual-download-btn")).not.toBeInTheDocument();
    });

    it("blocks auto-update when legacy MSI channel is detected and shows manual download guidance", async () => {
      vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
        channel: "MSI",
        can_auto_update: false,
        details: "Detected MSI installation",
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-channel-blocked-notice")).toBeInTheDocument();
      });

      // Must NOT render download-and-install button
      expect(screen.queryByTestId("download-and-install-btn")).not.toBeInTheDocument();
      // Must render manual download button
      expect(screen.getByTestId("manual-download-btn")).toBeInTheDocument();
      expect(screen.getByTestId("updater-channel-blocked-notice")).toHaveTextContent(/MSI/i);
    });

    it("blocks auto-update when standalone portable execution is detected", async () => {
      vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
        channel: "STANDALONE",
        can_auto_update: false,
        details: "Running standalone",
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-channel-blocked-notice")).toBeInTheDocument();
      });

      expect(screen.queryByTestId("download-and-install-btn")).not.toBeInTheDocument();
      expect(screen.getByTestId("manual-download-btn")).toBeInTheDocument();
    });

    it("fails closed when install channel is unknown", async () => {
      vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
        channel: "UNKNOWN",
        can_auto_update: false,
        details: "Unknown install source",
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall: vi.fn().mockResolvedValue(undefined),
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-channel-blocked-notice")).toBeInTheDocument();
      });

      expect(screen.queryByTestId("download-and-install-btn")).not.toBeInTheDocument();
      expect(screen.getByTestId("manual-download-btn")).toBeInTheDocument();
    });

    it("clicking manual download button opens GitHub Releases URL", async () => {
      vi.mocked(tauriService.fetchInstallationChannel).mockResolvedValue({
        channel: "MSI",
        can_auto_update: false,
        details: "Detected MSI",
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("manual-download-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("manual-download-btn"));
      expect(tauriService.openGitHubUrl).toHaveBeenCalledWith(
        "https://github.com/hellokennyh/RepoShelf/releases/latest"
      );
    });

    it("supports pluggable channel adapter via setInstallChannelAdapter", async () => {
      const customAdapter = {
        detectChannel: vi.fn().mockResolvedValue({
          channel: "NSIS" as const,
          canAutoUpdate: true,
          details: "Custom adapter NSIS",
        }),
      };
      setInstallChannelAdapter(customAdapter);
      const res = await customAdapter.detectChannel();
      expect(res.canAutoUpdate).toBe(true);
      expect(res.channel).toBe("NSIS");
    });
  });

  describe("6. Global Unsaved Drafts & Background Tasks Protection Suite (S5B)", () => {
    it("blocks update installation when external guard (e.g. FavoriteDetailModal) has unsaved drafts", async () => {
      const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      // Register external guard simulating open FavoriteDetailModal with translation/notes draft
      registerUpdateSafetyGuard("test-favorite-modal", {
        hasUnsavedDrafts: () => true,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("download-and-install-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-safety-warning")).toBeInTheDocument();
      });
      expect(screen.getByTestId("updater-safety-warning")).toHaveTextContent(
        /未保存的内容|Unsaved changes detected/
      );
      expect(downloadAndInstall).not.toHaveBeenCalled();
    });

    it("blocks update installation when external guard has active tasks (e.g. AI analysis running)", async () => {
      const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      registerUpdateSafetyGuard("test-active-analysis", {
        hasUnsavedDrafts: () => false,
        isSaving: () => false,
        hasActiveTasks: () => true,
      });

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("download-and-install-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-safety-warning")).toBeInTheDocument();
      });
      expect(screen.getByTestId("updater-safety-warning")).toHaveTextContent(
        /后台有正在运行的分析或翻译任务|Background tasks are in progress/
      );
      expect(downloadAndInstall).not.toHaveBeenCalled();
    });

    it("blocks update installation when backend reports active analysis tasks", async () => {
      vi.mocked(tauriService.fetchActiveBackendTasks).mockResolvedValue({
        active_analysis_tasks: 1,
        is_translating: false,
        has_active_tasks: true,
      });

      const downloadAndInstall = vi.fn().mockResolvedValue(undefined);
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch: vi.fn().mockResolvedValue(undefined),
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("download-and-install-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("updater-safety-warning")).toBeInTheDocument();
      });
      expect(screen.getByTestId("updater-safety-warning")).toHaveTextContent(
        /后台有正在运行的分析或翻译任务|Background tasks are in progress/
      );
      expect(downloadAndInstall).not.toHaveBeenCalled();
    });

    it("re-checks safety before relaunch and blocks restart if drafts created during download", async () => {
      let draftTriggered = false;
      const relaunch = vi.fn().mockResolvedValue(undefined);
      const downloadAndInstall = vi.fn().mockImplementation(async () => {
        // User starts editing while download is in flight
        draftTriggered = true;
      });

      registerUpdateSafetyGuard("dynamic-draft-during-download", {
        hasUnsavedDrafts: () => draftTriggered,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });

      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.5",
          downloadAndInstall,
        }),
        relaunch,
      };
      setUpstreamUpdaterAdapter(mockAdapter);

      renderSettingsPage();
      await waitFor(() => expect(screen.getByTestId("check-update-btn")).toBeInTheDocument());

      fireEvent.click(screen.getByTestId("check-update-btn"));
      await waitFor(() => expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument());

      await act(async () => {
        fireEvent.click(screen.getByTestId("download-and-install-btn"));
      });

      // Download was executed, but relaunch must be BLOCKED because draft appeared!
      expect(downloadAndInstall).toHaveBeenCalled();
      expect(relaunch).not.toHaveBeenCalled();
      await waitFor(() => {
        expect(screen.getByTestId("updater-safety-warning")).toBeInTheDocument();
      });
    });

    it("checkGlobalUpdateSafety evaluates both synchronous local guards and async backend tasks", async () => {
      vi.mocked(tauriService.fetchActiveBackendTasks).mockResolvedValue({
        active_analysis_tasks: 0,
        is_translating: false,
        has_active_tasks: false,
      });

      const safeResult = await checkGlobalUpdateSafety({
        hasUnsavedDrafts: () => false,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });
      expect(safeResult.safe).toBe(true);

      const unsafeDraft = await checkGlobalUpdateSafety({
        hasUnsavedDrafts: () => true,
        isSaving: () => false,
        hasActiveTasks: () => false,
      });
      expect(unsafeDraft.safe).toBe(false);
      expect(unsafeDraft.reason).toBe("UNSAVED_DRAFTS");
    });
  });

  describe("7. Release Endpoint & Security Baseline in tauri.conf.json (S5B)", () => {
    it("contains official HTTPS release endpoint and zero sensitive tokens", () => {
      const confPath = path.resolve(__dirname, "../../src-tauri/tauri.conf.json");
      const content = fs.readFileSync(confPath, "utf-8");
      const json = JSON.parse(content);

      const endpoints = json.plugins?.updater?.endpoints;
      expect(endpoints).toBeDefined();
      expect(Array.isArray(endpoints)).toBe(true);
      expect(endpoints).toContain(
        "https://github.com/hellokennyh/RepoShelf/releases/latest/download/latest.json"
      );

      // Verify no placeholder domains or embedded tokens
      expect(content).not.toContain("example.com");
      expect(content).not.toContain("ghp_");
      expect(content).not.toContain("github_pat_");

      // Verify createUpdaterArtifacts is false until signing key ceremony
      expect(json.bundle?.createUpdaterArtifacts).toBe(false);

      // Verify pubkey is not filled with fake or placeholder key
      expect(json.plugins?.updater?.pubkey).toBeUndefined();
    });
  });

  describe("8. .gitignore Security Rules Verification (S5B)", () => {
    it("ignores Project Docs, SQLite databases, keys, credentials, and release artifacts", () => {
      const gitignorePath = path.resolve(__dirname, "../../.gitignore");
      const content = fs.readFileSync(gitignorePath, "utf-8");

      expect(content).toContain("Project Docs/");
      expect(content).toContain("*.db");
      expect(content).toContain("*.sqlite");
      expect(content).toContain("*.sqlite3");
      expect(content).toContain("*.key");
      expect(content).toContain("*.pem");
      expect(content).toContain("*.p12");
      expect(content).toContain("*.sig");
      expect(content).toContain("*.msi");
      expect(content).toContain("*.nsis");
      expect(content).toContain("*.exe");
    });
  });
});
