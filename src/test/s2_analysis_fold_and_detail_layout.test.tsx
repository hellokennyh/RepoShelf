import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { useState } from "react";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import * as tauriService from "../services/tauri";
import { AnalysisEnvelopeV1, FavoriteItem } from "../types/library";
import { I18nProvider } from "../i18n/I18nContext";
import { setTestFallbackLocale } from "../i18n/localeState";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";

vi.mock("../services/tauri", () => ({
  deleteFavorite: vi.fn(),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: true, reason: null }),
  getFavorite: vi.fn(),
  openGitHubUrl: vi.fn(),
  refreshFavorite: vi.fn(),
  suggestFavoriteAiMetadata: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  translateRepositoryDescription: vi.fn(),
  analyzeRepository: vi.fn(),
  cancelRepositoryAnalysis: vi.fn(),
  fetchLocaleInitInfo: vi.fn().mockResolvedValue({
    locale: "zh-Hans",
    warning: null,
    detected_source_tag: "zh-CN",
  }),
  updateUiLanguage: vi.fn().mockResolvedValue({
    schema_version: 1,
    ui_language: "zh-Hans",
  }),
}));

const mockEnvelope: AnalysisEnvelopeV1 = {
  schema_version: 1,
  content: {
    overview: {
      status: "supported",
      items: [{ text: "八维项目概览：高性能仓库管理工具", basis: "documented", evidence_ids: ["E001"] }],
      missing_information: [],
    },
    features: {
      status: "supported",
      items: [{ text: "核心功能：本地搜索与快速书签", basis: "code_observed", evidence_ids: ["E001"] }],
      missing_information: [],
    },
    use_cases: {
      status: "supported",
      items: [{ text: "适用场景：开源项目追踪与技术选型", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    tech_stack: {
      status: "supported",
      items: [{ text: "技术栈：Rust, Tauri 2, React, TypeScript", basis: "code_observed", evidence_ids: [] }],
      missing_information: [],
    },
    setup: {
      status: "supported",
      items: [{ text: "部署与使用：cargo tauri dev", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    target_users: {
      status: "supported",
      items: [{ text: "目标用户：开发者与架构师", basis: "inferred", evidence_ids: [] }],
      missing_information: [],
    },
    maturity: {
      status: "supported",
      items: [{ text: "成熟度与维护：v1.1.2 活跃维护中", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    limitations: {
      status: "partial",
      items: [{ text: "局限与限制：需配置 GitHub 凭据获取高额度 API", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
  },
  target_locale: "zh-Hans",
  generated_at: "2026-10-31T10:00:00Z",
  generator: {
    provider: "ollama",
    model_requested: "qwen2.5-coder:7b",
    model_reported: "qwen2.5-coder:7b",
    prompt_version: "v1.1.2",
    selector_version: "v1.1.2",
    context_budget_tokens: 32768,
    input_tokens: 1500,
    input_tokens_is_estimate: false,
    output_limit_tokens: 4096,
  },
  source: {
    github_repo_id: "1001",
    canonical_url: "https://github.com/test-org/super-repo",
    default_branch: "main",
    commit_sha: "abcdef1234567890abcdef1234567890abcdef12",
    metadata_observed_at: "2026-10-31T10:00:00Z",
    evidence_fingerprint: "demo_fp_sha256",
  },
  evidence: [
    {
      id: "E001",
      kind: "file",
      path: "README.md",
      commit_sha: "abcdef1234567890abcdef1234567890abcdef12",
      blob_sha: null,
      observed_at: "2026-10-31T10:00:00Z",
      included_ranges: [{ start_line: 1, end_line: 50 }],
      included_content_hash: "hash_readme",
      truncated: false,
      public_fact_summary: "Main documentation file",
    },
  ],
  coverage: {
    status: "bounded",
    selected_file_count: 5,
    sent_file_count: 5,
    evidence_count: 1,
    gaps: [],
    executed_code: false,
    verified_deployment: false,
  },
  search_text: "八维项目概览：高性能仓库管理工具",
};

const sampleEmptyFavorite: FavoriteItem = {
  id: 101,
  github_repo_id: 2001,
  github_url: "https://github.com/test-org/empty-repo",
  name: "empty-repo",
  description: "A repository without analysis",
  category: "Developer Tools",
  stars: 1500,
  license: "MIT",
  pushed_at: "2026-03-01T12:00:00Z",
  rating: 4,
  rating_reason: "高质量工具",
  rating_confidence: "high",
  notes: "No analysis yet",
  favorited_at: "2026-02-15T08:00:00Z",
  metadata_refreshed_at: "2026-02-15T08:00:00Z",
  repo_created_at: "2024-01-10T00:00:00Z",
  tags: [],
  ai_analysis: null,
};

const sampleSavedFavorite: FavoriteItem = {
  id: 102,
  github_repo_id: 2002,
  github_url: "https://github.com/test-org/saved-repo",
  name: "saved-repo",
  description: "A repository with saved analysis report",
  category: "Automation",
  stars: 3200,
  license: "Apache-2.0",
  pushed_at: "2026-03-01T12:00:00Z",
  rating: 5,
  rating_reason: "极为优秀的自动化框架",
  rating_confidence: "high",
  notes: "Has saved report",
  favorited_at: "2026-02-15T08:00:00Z",
  metadata_refreshed_at: "2026-02-15T08:00:00Z",
  repo_created_at: "2024-01-10T00:00:00Z",
  tags: [],
  ai_analysis: mockEnvelope,
};

describe("S2: AI Analysis Folding (R03) & Detail GitHub URL Layout (R04)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setTestFallbackLocale("zh-Hans");
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({ ready: true, reason: null });
  });

  const renderModal = (
    fav: FavoriteItem,
    locale: "zh-Hans" | "zh-Hant" | "en" = "zh-Hans",
    onClose = vi.fn(),
    onUpdated = vi.fn()
  ) => {
    setTestFallbackLocale(locale);
    vi.mocked(tauriService.fetchLocaleInitInfo).mockResolvedValue({
      locale,
      warning: null,
      detected_source_tag: null,
    });

    return render(
      <I18nProvider initialLocale={locale} initialIsInitialized={true}>
        <DiscoverySessionProvider>
          <FavoriteDetailModal
            isOpen={true}
            favorite={fav}
            onClose={onClose}
            onUpdated={onUpdated}
            onDeleted={vi.fn()}
          />
        </DiscoverySessionProvider>
      </I18nProvider>
    );
  };

  // =========================================================================
  // R04 Detail GitHub URL Layout Tests
  // =========================================================================
  describe("R04: Detail GitHub URL in Top Title Row", () => {
    it("renders GitHub URL inline with project name, category badge, and stars", () => {
      renderModal(sampleEmptyFavorite);

      const nameEl = screen.getByTestId("detail-repo-name");
      const urlLink = screen.getByTestId("detail-github-url-link");

      expect(nameEl).toHaveTextContent("empty-repo");
      expect(urlLink).toHaveAttribute("href", "https://github.com/test-org/empty-repo");
      expect(urlLink).toHaveTextContent("https://github.com/test-org/empty-repo");

      // Verify that URL link is inside the same header block
      const headerContainer = nameEl.closest(".flex-wrap");
      expect(headerContainer).toContainElement(nameEl);
      expect(headerContainer).toContainElement(urlLink);

      // Verify full URL is viewable in title attribute
      expect(urlLink).toHaveAttribute("title", "https://github.com/test-org/empty-repo");
    });

    it("clicking GitHub URL opens external repository and stops propagation", () => {
      renderModal(sampleEmptyFavorite);

      const urlLink = screen.getByTestId("detail-github-url-link");
      fireEvent.click(urlLink);

      expect(tauriService.openGitHubUrl).toHaveBeenCalledWith("https://github.com/test-org/empty-repo");
    });

    it("renders long repository URL with truncate and does not break close button", () => {
      const longUrlFav: FavoriteItem = {
        ...sampleEmptyFavorite,
        github_url: "https://github.com/an-extremely-long-organization-name-with-many-subprojects/and-an-ultra-long-repository-name-that-could-overflow",
      };

      renderModal(longUrlFav);

      const urlLink = screen.getByTestId("detail-github-url-link");
      expect(urlLink).toHaveAttribute("title", longUrlFav.github_url);
      expect(urlLink.className).toContain("truncate");

      const closeBtn = screen.getByTestId("detail-close-btn");
      expect(closeBtn).toBeInTheDocument();
      expect(closeBtn).toBeVisible();
    });
  });

  // =========================================================================
  // R03 AI Analysis Folding Tests
  // =========================================================================
  describe("R03: AI Analysis Folding Behavior", () => {
    it("Case 1: when no analysis exists, keeps v1.1.2 empty state and does NOT render toggle button or empty report", () => {
      renderModal(sampleEmptyFavorite);

      // Empty state box rendered
      expect(screen.getByTestId("detail-analysis-empty")).toBeInTheDocument();

      // Generate button rendered
      expect(screen.getByTestId("detail-analyze-btn")).toBeInTheDocument();

      // Fold toggle button MUST NOT exist
      expect(screen.queryByTestId("detail-analysis-toggle-btn")).not.toBeInTheDocument();

      // Report container MUST NOT exist
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();
    });

    it("Case 2: saved report is default folded upon first opening, showing compact header and toggle button", () => {
      renderModal(sampleSavedFavorite);

      // Toggle button exists with aria-expanded="false"
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(toggleBtn).toBeInTheDocument();
      expect(toggleBtn).toHaveAttribute("aria-expanded", "false");
      expect(toggleBtn).toHaveTextContent("展开分析");

      // Saved badge exists
      expect(screen.getByTestId("detail-analysis-saved-badge")).toBeInTheDocument();
      expect(screen.getByTestId("detail-analysis-saved-badge")).toHaveTextContent("已保存报告");

      // Report 8-dimension body is NOT in DOM
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();
    });

    it("Case 2: clicking expand shows full 8-dimensional report, clicking collapse hides it", () => {
      renderModal(sampleSavedFavorite);

      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");

      // 1. Click to expand
      fireEvent.click(toggleBtn);
      expect(toggleBtn).toHaveAttribute("aria-expanded", "true");
      expect(toggleBtn).toHaveTextContent("收起分析");

      // Full report is rendered
      const reportContainer = screen.getByTestId("detail-analysis-report-container");
      expect(reportContainer).toBeInTheDocument();
      expect(reportContainer).toHaveTextContent("八维项目概览：高性能仓库管理工具");
      expect(reportContainer).toHaveTextContent("技术栈：Rust, Tauri 2, React, TypeScript");
      expect(reportContainer).toHaveTextContent("成熟度与维护：v1.1.2 活跃维护中");

      // 2. Click to collapse
      fireEvent.click(toggleBtn);
      expect(toggleBtn).toHaveAttribute("aria-expanded", "false");
      expect(toggleBtn).toHaveTextContent("展开分析");
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();
    });

    it("Case 3: newly generated candidate report is default folded with unsaved badge", async () => {
      vi.mocked(tauriService.analyzeRepository).mockResolvedValue(mockEnvelope);

      renderModal(sampleEmptyFavorite);

      await waitFor(() => {
        expect(screen.getByTestId("detail-analyze-btn")).not.toBeDisabled();
      });

      const generateBtn = screen.getByTestId("detail-analyze-btn");
      fireEvent.click(generateBtn);

      // Wait for analysis to complete
      await waitFor(() => {
        expect(screen.getByTestId("detail-analysis-draft-badge")).toBeInTheDocument();
      });

      // Newly generated report must be DEFAULT FOLDED
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(toggleBtn).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();

      // Click to expand and view candidate
      fireEvent.click(toggleBtn);
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();
      expect(screen.getByTestId("detail-analysis-report-container")).toHaveTextContent("八维项目概览：高性能仓库管理工具");
    });

    it("Case 4: re-analyzing keeps existing report accessible while progress is displayed", async () => {
      let resolveAnalysis: (value: AnalysisEnvelopeV1) => void;
      const pendingPromise = new Promise<AnalysisEnvelopeV1>((resolve) => {
        resolveAnalysis = resolve;
      });

      vi.mocked(tauriService.analyzeRepository).mockReturnValue(pendingPromise);

      renderModal(sampleSavedFavorite);

      await waitFor(() => {
        expect(screen.getByTestId("detail-reanalyze-btn")).not.toBeDisabled();
      });

      // Click re-analyze
      const reanalyzeBtn = screen.getByTestId("detail-reanalyze-btn");
      fireEvent.click(reanalyzeBtn);

      // Progress bar is visible
      expect(screen.getByTestId("detail-analyzing-progress")).toBeInTheDocument();

      // Existing report toggle button is STILL available
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(toggleBtn).toBeInTheDocument();

      // User can expand and read existing report during re-analysis!
      fireEvent.click(toggleBtn);
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();
      expect(screen.getByTestId("detail-analysis-report-container")).toHaveTextContent("八维项目概览：高性能仓库管理工具");

      // Complete re-analysis
      resolveAnalysis!(mockEnvelope);
      await waitFor(() => {
        expect(screen.queryByTestId("detail-analyzing-progress")).not.toBeInTheDocument();
      });
    });

    it("Case 4: re-analysis failure does NOT discard old saved report", async () => {
      vi.mocked(tauriService.analyzeRepository).mockRejectedValue(new Error("AI Provider network timeout"));

      renderModal(sampleSavedFavorite);

      await waitFor(() => {
        expect(screen.getByTestId("detail-reanalyze-btn")).not.toBeDisabled();
      });

      const reanalyzeBtn = screen.getByTestId("detail-reanalyze-btn");
      fireEvent.click(reanalyzeBtn);

      await waitFor(() => {
        expect(screen.getByTestId("detail-analysis-error")).toBeInTheDocument();
      });

      // Old report toggle button still exists!
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(toggleBtn).toBeInTheDocument();

      // Expand to verify old report is intact
      fireEvent.click(toggleBtn);
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();
      expect(screen.getByTestId("detail-analysis-report-container")).toHaveTextContent("八维项目概览：高性能仓库管理工具");
    });

    it("Case 5: closing and reopening detail modal resets to default folded", () => {
      const Harness = () => {
        const [isOpen, setIsOpen] = useState(true);
        return (
          <div>
            <button data-testid="reopen-modal-btn" onClick={() => setIsOpen(true)}>
              Reopen
            </button>
            <I18nProvider initialLocale="zh-Hans" initialIsInitialized={true}>
              <DiscoverySessionProvider>
                <FavoriteDetailModal
                  isOpen={isOpen}
                  favorite={sampleSavedFavorite}
                  onClose={() => setIsOpen(false)}
                  onUpdated={vi.fn()}
                  onDeleted={vi.fn()}
                />
              </DiscoverySessionProvider>
            </I18nProvider>
          </div>
        );
      };

      render(<Harness />);

      // 1. Expand report
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      fireEvent.click(toggleBtn);
      expect(toggleBtn).toHaveAttribute("aria-expanded", "true");
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();

      // 2. Close modal
      const closeBtn = screen.getByTestId("detail-close-btn");
      fireEvent.click(closeBtn);

      // 3. Reopen modal
      fireEvent.click(screen.getByTestId("reopen-modal-btn"));

      // Must be DEFAULT FOLDED on reopen!
      const reopenedToggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(reopenedToggleBtn).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();
    });

    it("Case 5: switching to a different favorite resets to default folded", () => {
      const anotherSavedFavorite: FavoriteItem = {
        ...sampleSavedFavorite,
        id: 103,
        name: "another-repo",
      };

      const Harness = () => {
        const [fav, setFav] = useState<FavoriteItem>(sampleSavedFavorite);
        return (
          <div>
            <button data-testid="switch-fav-btn" onClick={() => setFav(anotherSavedFavorite)}>
              Switch
            </button>
            <I18nProvider initialLocale="zh-Hans" initialIsInitialized={true}>
              <DiscoverySessionProvider>
                <FavoriteDetailModal
                  isOpen={true}
                  favorite={fav}
                  onClose={vi.fn()}
                  onUpdated={vi.fn()}
                  onDeleted={vi.fn()}
                />
              </DiscoverySessionProvider>
            </I18nProvider>
          </div>
        );
      };

      render(<Harness />);

      // 1. Expand report for first fav
      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      fireEvent.click(toggleBtn);
      expect(toggleBtn).toHaveAttribute("aria-expanded", "true");

      // 2. Switch to second fav
      fireEvent.click(screen.getByTestId("switch-fav-btn"));

      // Second fav must NOT inherit expanded state!
      const nextToggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(nextToggleBtn).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();
    });

    it("toggling expand/collapse never triggers save or AI call", () => {
      renderModal(sampleSavedFavorite);

      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      fireEvent.click(toggleBtn); // expand
      fireEvent.click(toggleBtn); // collapse
      fireEvent.click(toggleBtn); // expand

      expect(tauriService.updateFavoriteUserMetadata).not.toHaveBeenCalled();
      expect(tauriService.analyzeRepository).not.toHaveBeenCalled();
      expect(tauriService.suggestFavoriteAiMetadata).not.toHaveBeenCalled();
    });

    it("supports 3 UI languages: zh-Hans, zh-Hant, and en toggle text", () => {
      // 1. zh-Hans
      const { unmount: unmountHans } = renderModal(sampleSavedFavorite, "zh-Hans");
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("展开分析");
      fireEvent.click(screen.getByTestId("detail-analysis-toggle-btn"));
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("收起分析");
      unmountHans();

      // 2. zh-Hant
      const { unmount: unmountHant } = renderModal(sampleSavedFavorite, "zh-Hant");
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("展開分析");
      fireEvent.click(screen.getByTestId("detail-analysis-toggle-btn"));
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("收起分析");
      unmountHant();

      // 3. en
      const { unmount: unmountEn } = renderModal(sampleSavedFavorite, "en");
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("Expand Analysis");
      fireEvent.click(screen.getByTestId("detail-analysis-toggle-btn"));
      expect(screen.getByTestId("detail-analysis-toggle-btn")).toHaveTextContent("Collapse Analysis");
      unmountEn();
    });

    it("viewing saved report works even when AI is disabled", () => {
      vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
        ready: false,
        reason: "User disabled AI features in settings",
      });

      renderModal(sampleSavedFavorite);

      const toggleBtn = screen.getByTestId("detail-analysis-toggle-btn");
      expect(toggleBtn).toBeInTheDocument();

      fireEvent.click(toggleBtn);
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();
    });
  });
});
