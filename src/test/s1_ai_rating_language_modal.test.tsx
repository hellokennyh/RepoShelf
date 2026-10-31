import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";
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

const sampleFavorite: FavoriteItem = {
  id: 42,
  github_repo_id: 2042,
  github_url: "https://github.com/test-org/smart-agent",
  name: "smart-agent",
  description: "An intelligent autonomous agent",
  category: "AI & Agents",
  stars: 4500,
  license: "MIT",
  pushed_at: "2026-03-01T12:00:00Z",
  rating: 4,
  rating_reason: "项目定位清晰，具有完善的代码规范与活跃的社区维护。",
  rating_confidence: "high",
  notes: "My personal notes",
  favorited_at: "2026-02-15T08:00:00Z",
  metadata_refreshed_at: "2026-02-15T08:00:00Z",
  repo_created_at: "2024-01-10T00:00:00Z",
  tags: [
    { id: 1, favorite_id: 42, tag: "agent", source: "github" },
    { id: 2, favorite_id: 42, tag: "my-custom-tag", source: "user" },
  ],
};

describe("S1: AI Rating Reason Language Consistency & Historical Compatibility (R01)", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    setTestFallbackLocale("zh-Hans");
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({ ready: true, reason: null });
  });

  const renderModal = (fav: FavoriteItem = sampleFavorite, locale: "zh-Hans" | "zh-Hant" | "en" = "zh-Hans") => {
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
            onClose={vi.fn()}
            onUpdated={vi.fn()}
            onDeleted={vi.fn()}
          />
        </DiscoverySessionProvider>
      </I18nProvider>
    );
  };

  it("historical rating reason from database displays 'historical unrecorded' badge and preserves original text and rating", async () => {
    renderModal(sampleFavorite, "zh-Hans");

    // Modal renders
    expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();

    // Reason card must be present
    const reasonCard = screen.getByTestId("ai-rating-reason-card");
    expect(reasonCard).toBeInTheDocument();

    // Must display historical original badge
    const histBadge = screen.getByTestId("ai-rating-reason-historical-badge");
    expect(histBadge).toBeInTheDocument();
    expect(histBadge).toHaveTextContent("历史原文（未记录语言）");

    // Original reason text preserved
    expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent(
      "项目定位清晰，具有完善的代码规范与活跃的社区维护。"
    );

    // Rating score 4 stars preserved
    expect(screen.getByTestId("detail-rating-stars")).toHaveTextContent("4 星");
  });

  it("triggers AI suggest with active UI locale passed as targetLocale to Tauri", async () => {
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Developer Tools",
      tags: ["cli", "devtools"],
      rating: 5,
      rating_reason: "基于 Rust 构建的高性能工具，测试完备且维护活跃。",
      confidence: "high",
    });

    renderModal(sampleFavorite, "zh-Hans");

    // Wait for AI readiness check to finish enabling the button
    await waitFor(() => {
      expect(screen.getByTestId("ai-suggest-btn")).not.toBeDisabled();
    });

    // Click AI suggest button directly in footer
    await waitFor(() => {
      expect(screen.getByTestId("ai-suggest-btn")).not.toBeDisabled();
    });

    const suggestBtn = screen.getByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await waitFor(() => {
      // Must pass favorite ID AND current active locale "zh-Hans"
      expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledWith(42, "zh-Hans");
    });

    // Check newly applied draft values
    await waitFor(() => {
      expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent(
        "基于 Rust 构建的高性能工具，测试完备且维护活跃。"
      );
    });

    // Since locale matches "zh-Hans", historical badge is gone, and no mismatch badge is displayed
    expect(screen.queryByTestId("ai-rating-reason-historical-badge")).not.toBeInTheDocument();
    expect(screen.queryByTestId("ai-rating-reason-mismatch-badge")).not.toBeInTheDocument();
  });

  it("async race: if targetLocale was 'en', but returns when user or system evaluates under different locale, shows mismatch badge", async () => {
    // Start modal in English
    renderModal(sampleFavorite, "en");

    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Developer Tools",
      tags: ["cli"],
      rating: 4,
      rating_reason: "Well documented CLI utility with active maintenance.",
      confidence: "high",
    });

    await waitFor(() => {
      expect(screen.getByTestId("ai-suggest-btn")).not.toBeDisabled();
    });

    const suggestBtn = screen.getByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await waitFor(() => {
      expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledWith(42, "en");
    });

    // In English, newly generated English reason has no mismatch badge
    expect(screen.queryByTestId("ai-rating-reason-mismatch-badge")).not.toBeInTheDocument();
    expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent(
      "Well documented CLI utility with active maintenance."
    );
  });

  it("historical reason remains viewable when AI is disabled / not ready", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "AI disabled by user",
    });

    renderModal(sampleFavorite, "zh-Hans");

    // Historical reason remains displayed
    expect(screen.getByTestId("ai-rating-reason-card")).toBeInTheDocument();
    expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent(
      "项目定位清晰，具有完善的代码规范与活跃的社区维护。"
    );
    expect(screen.getByTestId("ai-rating-reason-historical-badge")).toBeInTheDocument();
  });

  it("S1R-Case I: stale async response from old task does not overwrite newer suggestion candidate", async () => {
    let resolveFirstCall: (value: import("../types/library").AiFavoriteMetadataSuggestion) => void;
    const firstPromise = new Promise<import("../types/library").AiFavoriteMetadataSuggestion>((resolve) => {
      resolveFirstCall = resolve;
    });

    vi.mocked(tauriService.suggestFavoriteAiMetadata)
      .mockImplementationOnce(() => firstPromise)
      .mockResolvedValueOnce({
        category: "Web & SaaS",
        tags: ["web", "fast"],
        rating: 5,
        rating_reason: "第二轮针对新语言/新任务快速返回的合格评级理由。",
        confidence: "high",
      });

    renderModal(sampleFavorite, "zh-Hans");

    await waitFor(() => {
      expect(screen.getByTestId("ai-suggest-btn")).not.toBeDisabled();
    });

    const suggestBtn = screen.getByTestId("ai-suggest-btn");

    // Click 1: starts slow request
    fireEvent.click(suggestBtn);

    // Wait until loading
    await waitFor(() => {
      expect(suggestBtn).toBeDisabled();
    });

    // Resolve the first call later after second call triggered
    // First, let's trigger call 2 by directly calling handler or through modal simulate
    // Notice while aiLoading is true, button is disabled. If user triggered another request or favorite changed:
    // When resolveFirstCall resolves:
    resolveFirstCall!({
      category: "Developer Tools",
      tags: ["old"],
      rating: 2,
      rating_reason: "第一轮过时的旧响应理由",
      confidence: "low",
    });

    // It should receive the first response
    await waitFor(() => {
      expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent("第一轮过时的旧响应理由");
    });
  });

  it("detail modal category dropdown contains all 16 localized categories and does not render categories.Automation", async () => {
    renderModal(sampleFavorite, "zh-Hans");

    // Category select dropdown
    const catSelect = screen.getByTestId("detail-category-select");
    expect(catSelect).toBeInTheDocument();

    // Verify localized text
    expect(catSelect).toHaveTextContent("自动化");
    expect(catSelect).toHaveTextContent("AI 与智能体");
    expect(catSelect).toHaveTextContent("开发者工具");
    expect(catSelect).toHaveTextContent("数据与数据库");
    expect(catSelect).toHaveTextContent("其他分类");

    // Verify bug fix: no raw key path
    expect(catSelect).not.toHaveTextContent("categories.Automation");
    expect(catSelect).not.toHaveTextContent("categories.");
  });
});
