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
  setUpstreamUpdaterAdapter,
  UpdaterAdapter,
  DownloadProgress,
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
    vi.mocked(tauriService.fetchSettings).mockResolvedValue(mockSettings);
    vi.mocked(tauriService.fetchSecretStatus).mockResolvedValue(mockSecretStatus);
    vi.mocked(tauriService.fetchLocaleInitInfo).mockResolvedValue({
      locale: "zh-Hans",
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

      expect(screen.getByTestId("updater-current-version")).toHaveTextContent("v1.1.3");
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
          version: "1.1.3",
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
          version: "1.1.4",
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

      expect(screen.getByTestId("updater-new-version")).toHaveTextContent("v1.1.4");
      expect(screen.getByTestId("updater-release-date")).toHaveTextContent("2026-11-02");
      expect(screen.getByTestId("updater-release-notes")).toHaveTextContent("Release Notes: NSIS Multilingual");
      expect(screen.getByTestId("download-and-install-btn")).toBeInTheDocument();
      expect(screen.getByTestId("updater-cancel-btn")).toBeInTheDocument();
    });

    it("collapses panel when user clicks cancel / later button", async () => {
      const mockAdapter: UpdaterAdapter = {
        check: vi.fn().mockResolvedValue({
          available: true,
          version: "1.1.4",
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
          version: "1.1.4",
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
          version: "1.1.4",
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
});
