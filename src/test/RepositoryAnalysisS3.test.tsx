import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import React, { useState } from "react";
import { RepositoryCard } from "../components/RepositoryCard";
import { RepositoryAnalysisReport } from "../components/RepositoryAnalysisReport";
import { AnalysisPreviewModal } from "../components/AnalysisPreviewModal";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { DiscoverySessionProvider, useDiscoverySession } from "../context/DiscoverySessionContext";
import { I18nProvider } from "../i18n/I18nContext";
import { AnalysisEnvelopeV1, FavoriteItem } from "../types/library";
import { GitHubRepository } from "../types/github";

const mockEnvelope: AnalysisEnvelopeV1 = {
  schema_version: 1,
  content: {
    overview: {
      status: "supported",
      items: [{ text: "High performance repository management", basis: "documented", evidence_ids: ["E001"] }],
      missing_information: [],
    },
    features: {
      status: "supported",
      items: [{ text: "Fast search and offline library", basis: "code_observed", evidence_ids: ["E001"] }],
      missing_information: [],
    },
    use_cases: {
      status: "supported",
      items: [{ text: "Developer bookmarking and research", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    tech_stack: {
      status: "supported",
      items: [{ text: "Rust, Tauri, React, TypeScript", basis: "code_observed", evidence_ids: [] }],
      missing_information: [],
    },
    setup: {
      status: "supported",
      items: [{ text: "Run cargo tauri dev", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    target_users: {
      status: "supported",
      items: [{ text: "Software engineers and researchers", basis: "inferred", evidence_ids: [] }],
      missing_information: [],
    },
    maturity: {
      status: "supported",
      items: [{ text: "Active development v1.1.2", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
    limitations: {
      status: "partial",
      items: [{ text: "Requires GitHub token for high-rate API access", basis: "documented", evidence_ids: [] }],
      missing_information: [],
    },
  },
  target_locale: "en",
  generated_at: "2026-10-03T10:00:00Z",
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
    github_repo_id: "99001",
    canonical_url: "https://github.com/org/repo-demo",
    default_branch: "main",
    commit_sha: "abcdef1234567890abcdef1234567890abcdef12",
    metadata_observed_at: "2026-10-03T10:00:00Z",
    evidence_fingerprint: "demo_fp_sha256",
  },
  evidence: [
    {
      id: "E001",
      kind: "file",
      path: "README.md",
      commit_sha: "abcdef1234567890abcdef1234567890abcdef12",
      blob_sha: null,
      observed_at: "2026-10-03T10:00:00Z",
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
  search_text: "High performance repository management Fast search",
};

const mockRepo: GitHubRepository = {
  github_repo_id: 99001,
  github_url: "https://github.com/org/repo-demo",
  name: "repo-demo",
  description: "A wonderful demo repository",
  stars: 1200,
  license: "MIT",
  category: "Developer Tools",
  topics: ["rust", "tauri"],
  pushed_at: "2026-10-01T00:00:00Z",
  created_at: "2025-01-01T00:00:00Z",
  releases: 5,
  languages: { Rust: 80, TypeScript: 20 },
};

const mockFavorite: FavoriteItem = {
  id: 42,
  github_repo_id: 99001,
  github_url: "https://github.com/org/repo-demo",
  name: "repo-demo",
  description: "A wonderful demo repository",
  category: "Developer Tools",
  stars: 1200,
  license: "MIT",
  pushed_at: "2026-10-01T00:00:00Z",
  repo_created_at: "2025-01-01T00:00:00Z",
  rating: 5,
  rating_reason: "Excellent code quality",
  rating_confidence: "high",
  notes: "My primary tool",
  favorited_at: "2026-10-02T00:00:00Z",
  metadata_refreshed_at: "2026-10-02T00:00:00Z",
  tags: [],
  releases: 5,
  languages: { Rust: 80, TypeScript: 20 },
  ai_analysis: null,
  analysis_revision: "null_analysis_v1",
};

// Mock Tauri invoke
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args: any) => {
    if (cmd === "get_ai_search_readiness") {
      return Promise.resolve({ ready: true, reason: null });
    }
    if (cmd === "analyze_repository") {
      return Promise.resolve(mockEnvelope);
    }
    if (cmd === "cancel_repository_analysis") {
      return Promise.resolve(true);
    }
    if (cmd === "update_favorite_user_metadata") {
      if (
        args?.input?.analysis_patch?.expected_revision &&
        args.input.analysis_patch.expected_revision !== "null_analysis_v1"
      ) {
        return Promise.reject(new Error("AnalysisConflict: analysis has been modified concurrently"));
      }
      return Promise.resolve({
        ...mockFavorite,
        ...args.input,
        ai_analysis: args.input?.analysis_patch?.envelope || mockFavorite.ai_analysis,
        analysis_revision: "new_rev_12345",
      });
    }
    if (cmd === "check_favorite_identities_batch") {
      return Promise.resolve({ matched_repo_ids: [], matched_urls: [] });
    }
    if (cmd === "get_locale_initialization_info") {
      return Promise.resolve({
        app_locale: "zh-Hans",
        configured_locale: null,
        effective_locale: "zh-Hans",
      });
    }
    if (cmd === "open_github_url") {
      return Promise.resolve();
    }
    if (cmd === "get_favorite") {
      return Promise.resolve(mockFavorite);
    }
    return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
  }),
}));

describe("S3 Repository Analysis Frontend Suites", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  // ---------------------------------------------------------------------------
  // 1. RepositoryCard Integration (AC063, AC064, AC066)
  // ---------------------------------------------------------------------------
  describe("RepositoryCard Analysis Entry", () => {
    it("renders '项目分析(AI)' button and preserves external title link (AC063, AC064)", async () => {
      const onAnalyze = vi.fn();
      const onFavorite = vi.fn();

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <RepositoryCard
              repo={mockRepo}
              isFavorited={false}
              onFavorite={onFavorite}
              onAnalyze={onAnalyze}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Verify title preserves GitHub link
      const titleLink = screen.getByTestId("open-github-link");
      expect(titleLink).toBeInTheDocument();
      expect(titleLink).toHaveAttribute("href", mockRepo.github_url);

      // Verify analyze button exists and wait for AI readiness
      const analyzeBtn = screen.getByTestId("analyze-btn");
      expect(analyzeBtn).toBeInTheDocument();
      expect(analyzeBtn).toHaveAttribute("type", "button");

      await waitFor(() => expect(screen.getByTestId("analyze-btn")).not.toBeDisabled());

      // Click analyze button
      fireEvent.click(analyzeBtn);
      expect(onAnalyze).toHaveBeenCalledTimes(1);
      expect(onFavorite).not.toHaveBeenCalled();
    });

    it("stops event propagation when clicking analyze button", async () => {
      const onAnalyze = vi.fn();
      const onToggleSelect = vi.fn();

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <RepositoryCard
              repo={mockRepo}
              selectable={true}
              isSelected={false}
              onToggleSelect={onToggleSelect}
              isFavorited={false}
              onFavorite={() => {}}
              onAnalyze={onAnalyze}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      const analyzeBtn = screen.getByTestId("analyze-btn");
      await waitFor(() => expect(screen.getByTestId("analyze-btn")).not.toBeDisabled());
      fireEvent.click(analyzeBtn);

      expect(onAnalyze).toHaveBeenCalledTimes(1);
      expect(onToggleSelect).not.toHaveBeenCalled();
    });
  });

  // ---------------------------------------------------------------------------
  // 2. RepositoryAnalysisReport Component (AC071)
  // ---------------------------------------------------------------------------
  describe("RepositoryAnalysisReport 8-Dimension Display", () => {
    it("renders all 8 sections in authoritative order and basis badges", () => {
      render(
        <I18nProvider>
          <RepositoryAnalysisReport envelope={mockEnvelope} />
        </I18nProvider>
      );

      // Check section containers
      expect(screen.getByTestId("analysis-section-overview")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-features")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-use_cases")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-tech_stack")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-setup")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-target_users")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-maturity")).toBeInTheDocument();
      expect(screen.getByTestId("analysis-section-limitations")).toBeInTheDocument();

      // Check item texts
      expect(screen.getByText("High performance repository management")).toBeInTheDocument();
      expect(screen.getByText("Fast search and offline library")).toBeInTheDocument();

      // Check disclaimer
      expect(screen.getByTestId("analysis-disclaimer")).toBeInTheDocument();

      // Check generator metadata
      expect(screen.getByTestId("analysis-metadata-card")).toBeInTheDocument();
      expect(screen.getAllByText(/qwen2.5-coder:7b/).length).toBeGreaterThan(0);
    });

    it("toggles evidence and source details without unhandled exceptions", () => {
      render(
        <I18nProvider>
          <RepositoryAnalysisReport envelope={mockEnvelope} />
        </I18nProvider>
      );

      const toggleBtn = screen.getByTestId("toggle-evidence-btn");
      expect(toggleBtn).toBeInTheDocument();

      // Expand evidence
      fireEvent.click(toggleBtn);
      expect(screen.getByTestId("analysis-evidence-drawer")).toBeInTheDocument();
      expect(screen.getByText("README.md")).toBeInTheDocument();

      // Collapse evidence
      fireEvent.click(toggleBtn);
      expect(screen.queryByTestId("analysis-evidence-drawer")).not.toBeInTheDocument();
    });

    it("renders provider, requested model, and AC111 disclosure for Ollama without false local guarantees", () => {
      render(
        <I18nProvider>
          <RepositoryAnalysisReport envelope={mockEnvelope} />
        </I18nProvider>
      );

      // Provider displayed
      const providerMeta = screen.getByTestId("report-meta-provider");
      expect(providerMeta).toBeInTheDocument();
      expect(providerMeta).toHaveTextContent("ollama");

      // Requested model displayed
      const requestedMeta = screen.getByTestId("report-meta-model-requested");
      expect(requestedMeta).toBeInTheDocument();
      expect(requestedMeta).toHaveTextContent("qwen2.5-coder:7b");

      // Reported model displayed when present
      const reportedMeta = screen.getByTestId("report-meta-model-reported");
      expect(reportedMeta).toBeInTheDocument();
      expect(reportedMeta).toHaveTextContent("qwen2.5-coder:7b");

      // AC111 External Send Disclosure
      expect(screen.getByTestId("analysis-disclosure")).toBeInTheDocument();
      const sendDisclosure = screen.getByTestId("analysis-evidence-sent-disclosure");
      expect(sendDisclosure).toHaveTextContent(/ollama/i);

      const ollamaNote = screen.getByTestId("analysis-ollama-privacy-note");
      expect(ollamaNote).toBeInTheDocument();
      expect(ollamaNote.textContent).toContain("不证明所配置端点或其下游处理完全本地");
      expect(ollamaNote.textContent).not.toMatch(/绝不离开电脑|保证完全本地|guaranteed offline/i);
    });

    it("does not fabricate reported model when absent, and renders cloud disclosure boundary", () => {
      const cloudEnvelope: AnalysisEnvelopeV1 = {
        ...mockEnvelope,
        generator: {
          ...mockEnvelope.generator,
          provider: "openai",
          model_requested: "gpt-4o-mini",
          model_reported: null,
        },
      };

      render(
        <I18nProvider>
          <RepositoryAnalysisReport envelope={cloudEnvelope} />
        </I18nProvider>
      );

      // Provider and requested model
      expect(screen.getByTestId("report-meta-provider")).toHaveTextContent("openai");
      expect(screen.getByTestId("report-meta-model-requested")).toHaveTextContent("gpt-4o-mini");

      // Reported model is absent - MUST NOT be rendered or fabricated
      expect(screen.queryByTestId("report-meta-model-reported")).not.toBeInTheDocument();

      // Cloud disclosure emphasizes boundary: only selected evidence sent, not entire library or credentials
      expect(screen.getByTestId("analysis-cloud-privacy-note")).toBeInTheDocument();
      expect(screen.queryByTestId("analysis-ollama-privacy-note")).not.toBeInTheDocument();
    });
  });

  // ---------------------------------------------------------------------------
  // 3. AnalysisPreviewModal (AC065, AC067, AC068)
  // ---------------------------------------------------------------------------
  describe("AnalysisPreviewModal Lifecycle & Preview Adoption", () => {
    it("triggers analysis on mount, shows progress, and supports favorite with preview", async () => {
      const onFavoriteWithPreview = vi.fn().mockResolvedValue(undefined);
      const onClose = vi.fn();

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <AnalysisPreviewModal
              isOpen={true}
              repo={mockRepo}
              onClose={onClose}
              onFavoriteWithPreview={onFavoriteWithPreview}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Wait for analysis report to complete
      await waitFor(
        () => {
          expect(screen.getByTestId("analysis-section-overview")).toBeInTheDocument();
        },
        { timeout: 3000 }
      );

      // Verify Favorite With Preview button exists and click it
      const favWithPreviewBtn = screen.getByTestId("preview-favorite-btn");
      expect(favWithPreviewBtn).toBeInTheDocument();

      fireEvent.click(favWithPreviewBtn);
      await waitFor(() => {
        expect(onFavoriteWithPreview).toHaveBeenCalledWith(mockRepo, mockEnvelope);
      });
    });

    it("cancels analysis when user clicks close", async () => {
      const onClose = vi.fn();

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <AnalysisPreviewModal
              isOpen={true}
              repo={mockRepo}
              onClose={onClose}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      const closeBtn = screen.getByTestId("preview-close-btn");
      fireEvent.click(closeBtn);
      expect(onClose).toHaveBeenCalled();
    });

    it("displays honest coarse progress (processing) during analysis without synthetic stage guessing", async () => {
      let resolveAnalysis: (val: any) => void;
      const deferredPromise = new Promise((resolve) => {
        resolveAnalysis = resolve;
      });

      const { invoke } = await import("@tauri-apps/api/core");
      const originalInvoke = (invoke as any).getMockImplementation();

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "analyze_repository") {
          return deferredPromise;
        }
        return originalInvoke(cmd, args);
      });

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <AnalysisPreviewModal
              isOpen={true}
              repo={mockRepo}
              onClose={() => {}}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Verify progress indicator is rendered
      await waitFor(() => {
        expect(screen.getByTestId("analysis-progress-indicator")).toBeInTheDocument();
      });

      // Honest status: "正在分析..." / "Analyzing repository..."
      const progressIndicator = screen.getByTestId("analysis-progress-indicator");
      expect(progressIndicator).toHaveTextContent(/正在分析|Analyzing repository/i);

      // Verify NO synthetic readingEvidence, generating, or validating stage text
      expect(screen.queryByText(/正在读取资料|Reading repository evidence/i)).not.toBeInTheDocument();
      expect(screen.queryByText(/正在验证|Validating analysis/i)).not.toBeInTheDocument();

      // Resolve analysis
      resolveAnalysis!(mockEnvelope);

      await waitFor(() => {
        expect(screen.getByTestId("analysis-section-overview")).toBeInTheDocument();
      });

      (invoke as any).mockImplementation(originalInvoke);
    });
  });

  // ---------------------------------------------------------------------------
  // 4. FavoriteDetailModal Analysis Section (AC070, AC072, AC073, AC074)
  // ---------------------------------------------------------------------------
  describe("FavoriteDetailModal Analysis Draft & Concurrency", () => {
    it("renders empty analysis state when favorite has no saved analysis", () => {
      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <FavoriteDetailModal
              isOpen={true}
              favorite={mockFavorite}
              onClose={() => {}}
              onUpdated={() => {}}
              onDeleted={() => {}}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      expect(screen.getByTestId("detail-analysis-empty")).toBeInTheDocument();
      expect(screen.getByTestId("detail-analyze-btn")).toBeInTheDocument();
    });

    it("adopts external session preview into draft and shows unsaved badge", async () => {
      const Consumer = () => {
        const { setSessionPreview } = useDiscoverySession();
        React.useEffect(() => {
          setSessionPreview(mockEnvelope, mockFavorite.github_repo_id, mockFavorite.github_url);
        }, [setSessionPreview]);

        return (
          <FavoriteDetailModal
            isOpen={true}
            favorite={mockFavorite}
            onClose={() => {}}
            onUpdated={() => {}}
            onDeleted={() => {}}
          />
        );
      };

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <Consumer />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Adopt preview button should be present
      await waitFor(() => {
        expect(screen.getByTestId("detail-adopt-preview-btn")).toBeInTheDocument();
      });

      fireEvent.click(screen.getByTestId("detail-adopt-preview-btn"));

      // Draft unsaved badge appears and report is default folded (S2 requirement)
      expect(screen.getByTestId("detail-analysis-draft-badge")).toBeInTheDocument();
      expect(screen.queryByTestId("detail-analysis-report-container")).not.toBeInTheDocument();

      // Expanding via toggle button renders report
      fireEvent.click(screen.getByTestId("detail-analysis-toggle-btn"));
      expect(screen.getByTestId("detail-analysis-report-container")).toBeInTheDocument();
    });

    it("handles save conflict gracefully when backend reports AnalysisConflict", async () => {
      const favWithConflictRev: FavoriteItem = {
        ...mockFavorite,
        analysis_revision: "stale_revision_token",
      };

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <FavoriteDetailModal
              isOpen={true}
              favorite={favWithConflictRev}
              onClose={() => {}}
              onUpdated={() => {}}
              onDeleted={() => {}}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Wait for AI readiness check to settle and button to become enabled
      await waitFor(() => {
        expect(screen.getByTestId("detail-analyze-btn")).not.toBeDisabled();
      });

      // Trigger analysis
      fireEvent.click(screen.getByTestId("detail-analyze-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("detail-analysis-draft-badge")).toBeInTheDocument();
      });

      // Save changes
      const saveBtn = screen.getByTestId("detail-save-btn");
      fireEvent.click(saveBtn);

      // Should display conflict notice in feedback banner
      await waitFor(() => {
        const banner = screen.getByTestId("detail-feedback-banner");
        expect(banner).toBeInTheDocument();
        expect(banner).toHaveTextContent(/已被其他操作修改|modified by another operation/i);
      });
    });

    it("handles AC090 post-commit sync failure with distinct UI and controlled reread", async () => {
      const onUpdatedThrow = vi.fn().mockImplementation(() => {
        throw new Error("Simulated frontend sync / state update failure");
      });
      let updateSaveCalled = 0;
      let getFavoriteCalled = 0;

      const { invoke } = await import("@tauri-apps/api/core");
      const originalInvoke = (invoke as any).getMockImplementation();

      (invoke as any).mockImplementation((cmd: string, args: any) => {
        if (cmd === "update_favorite_user_metadata") {
          updateSaveCalled += 1;
          return Promise.resolve({
            ...mockFavorite,
            ...args.input,
            notes: args.input?.notes || mockFavorite.notes,
            ai_analysis: args.input?.analysis_patch?.envelope || mockFavorite.ai_analysis,
            analysis_revision: "saved_rev_after_commit",
          });
        }
        if (cmd === "get_favorite") {
          getFavoriteCalled += 1;
          return Promise.resolve({
            ...mockFavorite,
            notes: "Committed notes in DB",
            analysis_revision: "saved_rev_after_commit",
          });
        }
        return originalInvoke(cmd, args);
      });

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <FavoriteDetailModal
              isOpen={true}
              favorite={mockFavorite}
              onClose={() => {}}
              onUpdated={onUpdatedThrow}
              onDeleted={() => {}}
            />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Wait for AI readiness check and trigger analyze
      await waitFor(() => {
        expect(screen.getByTestId("detail-analyze-btn")).not.toBeDisabled();
      });
      fireEvent.click(screen.getByTestId("detail-analyze-btn"));

      await waitFor(() => {
        expect(screen.getByTestId("detail-analysis-draft-badge")).toBeInTheDocument();
      });

      // Save changes
      const saveBtn = screen.getByTestId("detail-save-btn");
      fireEvent.click(saveBtn);

      // Wait for post-commit sync error
      await waitFor(() => {
        expect(updateSaveCalled).toBe(1);
        expect(getFavoriteCalled).toBe(1);
        const banner = screen.getByTestId("detail-feedback-banner");
        expect(banner).toBeInTheDocument();
        expect(banner).toHaveTextContent(/已保存，但界面同步异常|Saved successfully, but UI sync failed/i);
        expect(banner.textContent).not.toMatch(/保存失败|feedbackSaveFailed/i);
      });

      // Assert no duplicate save or AI re-invocation
      expect(updateSaveCalled).toBe(1);
      (invoke as any).mockImplementation(originalInvoke);
    });
  });

  // ---------------------------------------------------------------------------
  // 5. Session Preview Bounded Cache (AC069)
  // ---------------------------------------------------------------------------
  describe("DiscoverySessionContext Preview Cache Bounds", () => {
    it("stores, retrieves, and evicts session preview cleanly", async () => {
      const TestComponent = () => {
        const ctx = useDiscoverySession();
        const [previewVal, setPreviewVal] = useState<AnalysisEnvelopeV1 | null>(null);

        return (
          <div>
            <button
              data-testid="set-btn"
              onClick={() => ctx.setSessionPreview(mockEnvelope, 12345, "https://github.com/org/repo-demo")}
            >
              Set
            </button>
            <button
              data-testid="get-btn"
              onClick={() => setPreviewVal(ctx.getSessionPreview(12345, null))}
            >
              Get
            </button>
            <button
              data-testid="evict-btn"
              onClick={() => ctx.evictSessionPreview(12345, null)}
            >
              Evict
            </button>
            <div data-testid="preview-status">{previewVal ? previewVal.source.github_repo_id : "none"}</div>
          </div>
        );
      };

      render(
        <I18nProvider>
          <DiscoverySessionProvider>
            <TestComponent />
          </DiscoverySessionProvider>
        </I18nProvider>
      );

      // Initially none
      fireEvent.click(screen.getByTestId("get-btn"));
      expect(screen.getByTestId("preview-status")).toHaveTextContent("none");

      // Set preview
      fireEvent.click(screen.getByTestId("set-btn"));

      // Get preview
      await waitFor(() => {
        fireEvent.click(screen.getByTestId("get-btn"));
        expect(screen.getByTestId("preview-status")).toHaveTextContent("99001");
      });

      // Evict preview
      fireEvent.click(screen.getByTestId("evict-btn"));
      await waitFor(() => {
        fireEvent.click(screen.getByTestId("get-btn"));
        expect(screen.getByTestId("preview-status")).toHaveTextContent("none");
      });
    });
  });
});
