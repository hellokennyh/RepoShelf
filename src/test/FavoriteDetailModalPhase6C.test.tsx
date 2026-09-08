import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { SearchPage } from "../pages/SearchPage";
import { TrendingPage } from "../pages/TrendingPage";
import * as tauriService from "../services/tauri";
import { FavoriteItem } from "../types/library";

vi.mock("../services/tauri", () => ({
  openGitHubUrl: vi.fn(),
  refreshFavorite: vi.fn(),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
  getAiSearchReadiness: vi.fn(),
  suggestFavoriteAiMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
  getGitHubRepository: vi.fn(),
  searchGitHubRepositories: vi.fn(),
  searchAiGitHubRepositories: vi.fn(),
  fetchTrending: vi.fn(),
  checkFavoriteStatus: vi.fn(),
  favoriteRepository: vi.fn(),
  fetchSettings: vi.fn().mockResolvedValue({
    github: { default_search_limit: 20, default_trending_period: "daily" },
    ai: { enabled: false },
  }),
}));

const sampleFavorite: FavoriteItem = {
  id: 42,
  github_repo_id: 1042,
  github_url: "https://github.com/test-owner/test-repo",
  name: "test-repo",
  description: "A fast scraper tool for web data",
  category: "Other",
  stars: 1200,
  license: "MIT",
  pushed_at: "2026-03-01T00:00:00Z",
  rating: 3,
  notes: "User notes here",
  favorited_at: "2026-03-01T10:00:00Z",
  metadata_refreshed_at: "2026-03-01T10:00:00Z",
  tags: [
    { id: 1, favorite_id: 42, tag: "scraping", source: "github" },
    { id: 2, favorite_id: 42, tag: "crawler", source: "user" },
    { id: 3, favorite_id: 42, tag: "old-ai", source: "ai" },
  ],
};

describe("FavoriteDetailModal Phase 6-C Acceptance Tests", () => {
  const onUpdated = vi.fn();
  const onDeleted = vi.fn();
  const onClose = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("P6C-FE-01: FavoriteDetail shows AI suggestion action button", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).toBeInTheDocument();
    expect(suggestBtn).toHaveTextContent("AI 建议分类和标签");
  });

  it("P6C-FE-02: Action disabled/unavailable when AI not ready", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "未配置 AI API Key (AI API Key not configured)",
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).toBeDisabled();
    expect(suggestBtn).toHaveAttribute(
      "title",
      "未配置 AI API Key (AI API Key not configured)"
    );
  });

  it("P6C-FE-03: Action enabled when AI ready", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    expect(suggestBtn).not.toBeDisabled();
  });

  it("P6C-FE-04, 05, 06, 07, 08, 09: Click invokes suggestion command with favorite_id only, shows loading state, and displays suggestion preview", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });

    let resolveSuggestion!: (
      val: import("../types/library").AiFavoriteMetadataSuggestion
    ) => void;
    const suggestionPromise =
      new Promise<import("../types/library").AiFavoriteMetadataSuggestion>(
        (resolve) => {
          resolveSuggestion = resolve;
        }
      );
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockReturnValue(
      suggestionPromise
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    // P6C-FE-04: invokes suggestion command with favorite_id only
    expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledWith(42);
    expect(tauriService.suggestFavoriteAiMetadata).toHaveBeenCalledTimes(1);

    // P6C-FE-05: loading state shown
    expect(suggestBtn).toHaveTextContent("AI 分析建议中...");
    expect(suggestBtn).toBeDisabled();

    // Resolve suggestion
    resolveSuggestion!({
      category: "Automation",
      tags: ["scraping", "bot", "crawler"],
    });

    // P6C-FE-06: Suggestion Category displayed
    const catEl = await screen.findByTestId("ai-suggested-category");
    expect(catEl).toHaveTextContent("Automation");

    // P6C-FE-07: Suggestion Tags displayed
    const tagsEl = await screen.findByTestId("ai-suggested-tags");
    expect(tagsEl).toHaveTextContent("#scraping");
    expect(tagsEl).toHaveTextContent("#bot");
    expect(tagsEl).toHaveTextContent("#crawler");

    // P6C-FE-08: Apply visible
    const applyBtn = screen.getByTestId("ai-apply-btn");
    expect(applyBtn).toBeInTheDocument();

    // P6C-FE-09: Cancel visible
    const cancelBtn = screen.getByTestId("ai-cancel-btn");
    expect(cancelBtn).toBeInTheDocument();
  });

  it("P6C-FE-10, 11: Cancel clears suggestion without calling Apply or altering DB", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scraping"],
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const cancelBtn = screen.getByTestId("ai-cancel-btn");

    fireEvent.click(cancelBtn);

    // P6C-FE-10: no apply call
    expect(tauriService.applyFavoriteAiMetadata).not.toHaveBeenCalled();

    // P6C-FE-11: clears suggestion box
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();

    // Current category remains unchanged
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Other");
  });

  it("P6C-FE-12, 13, 14, 15, 16, 17: Apply invokes apply command, updates category and AI tags, and preserves unsaved local user edits", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["bot", "fast-scrape"],
    });

    const updatedAfterApply: FavoriteItem = {
      ...sampleFavorite,
      category: "Automation",
      tags: [
        { id: 1, favorite_id: 42, tag: "scraping", source: "github" },
        { id: 2, favorite_id: 42, tag: "crawler", source: "user" },
        { id: 10, favorite_id: 42, tag: "bot", source: "ai" },
        { id: 11, favorite_id: 42, tag: "fast-scrape", source: "ai" },
      ],
    };
    vi.mocked(tauriService.applyFavoriteAiMetadata).mockResolvedValue(
      updatedAfterApply
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Simulate unsaved user edits before AI suggestion
    // 1. Change notes
    const notesInput = (await screen.findByTestId(
      "detail-notes-textarea"
    )) as HTMLTextAreaElement;
    fireEvent.change(notesInput, {
      target: { value: "UNSAVED_LOCAL_NOTE_EDIT" },
    });

    // 2. Change rating to 5
    const star5 = screen.getByTestId("detail-star-5");
    fireEvent.click(star5);

    // 3. Add an unsaved user tag
    const tagInput = screen.getByTestId("detail-add-tag-input");
    fireEvent.change(tagInput, { target: { value: "unsaved-user-tag" } });
    fireEvent.submit(tagInput.closest("form")!);

    // Verify unsaved tag rendered in editable section
    expect(screen.getByTestId("user-tag-unsaved-user-tag")).toBeInTheDocument();

    // Now trigger AI suggestion
    const suggestBtn = screen.getByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const applyBtn = screen.getByTestId("ai-apply-btn");

    // Click Apply
    fireEvent.click(applyBtn);

    await waitFor(() => {
      // P6C-FE-12: Apply invokes apply command with same favorite_id
      expect(tauriService.applyFavoriteAiMetadata).toHaveBeenCalledWith(
        42,
        "Automation",
        ["bot", "fast-scrape"]
      );
    });

    // P6C-FE-13: displayed category updated to Automation
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Automation");

    // P6C-FE-14: displayed AI tags updated to new tags
    expect(screen.getByText("#bot")).toBeInTheDocument();
    expect(screen.getByText("#fast-scrape")).toBeInTheDocument();
    expect(screen.queryByText("#old-ai")).not.toBeInTheDocument();

    // P6C-FE-15: preserves unsaved local user tags
    expect(screen.getByTestId("user-tag-unsaved-user-tag")).toBeInTheDocument();

    // P6C-FE-16: preserves unsaved local rating
    expect(screen.getByText("5 星")).toBeInTheDocument();

    // P6C-FE-17: preserves unsaved local notes
    expect(notesInput.value).toBe("UNSAVED_LOCAL_NOTE_EDIT");

    // Suggestion box cleared after apply
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-18: Suggestion state resets when favorite changes", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scrape"],
    });

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");

    // Switch to a different favorite
    const differentFav: FavoriteItem = {
      ...sampleFavorite,
      id: 99,
      name: "different-repo",
      category: "Data & Database",
    };

    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={differentFav}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Suggestion box must be cleared
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-19: Suggestion state resets when modal closes and reopens", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["scrape"],
    });

    const { rerender } = render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");

    // Close modal
    rerender(
      <FavoriteDetailModal
        isOpen={false}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Reopen modal
    rerender(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Suggestion box must be reset
    expect(screen.queryByTestId("ai-suggestion-box")).not.toBeInTheDocument();
  });

  it("P6C-FE-20: Suggestion error does not close modal and displays concise error", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockRejectedValue(
      new Error("AI connection timed out")
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    const banner = await screen.findByTestId("detail-feedback-banner");
    expect(banner).toHaveTextContent("AI connection timed out");

    // Modal stays open
    expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("P6C-FE-21: Apply error does not falsely update UI and keeps suggestion available", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
      category: "Automation",
      tags: ["bot"],
    });
    vi.mocked(tauriService.applyFavoriteAiMetadata).mockRejectedValue(
      new Error("Database write locked")
    );

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    const suggestBtn = await screen.findByTestId("ai-suggest-btn");
    fireEvent.click(suggestBtn);

    await screen.findByTestId("ai-suggestion-box");
    const applyBtn = screen.getByTestId("ai-apply-btn");

    fireEvent.click(applyBtn);

    const banner = await screen.findByTestId("detail-feedback-banner");
    expect(banner).toHaveTextContent("应用 AI 建议失败: Database write locked");

    // Category not falsely updated
    const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
    expect(select.value).toBe("Other");

    // Suggestion box remains available for retry or cancel
    expect(screen.getByTestId("ai-suggestion-box")).toBeInTheDocument();
  });

  it("P6C-FE-22, 23, 24: Existing Save, Refresh, and Delete remain functional", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
    vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValue({
      ...sampleFavorite,
      notes: "saved notes",
    });
    vi.mocked(tauriService.refreshFavorite).mockResolvedValue({
      favorite_id: 42,
      status: "succeeded",
      favorite: sampleFavorite,
    });
    vi.mocked(tauriService.deleteFavorite).mockResolvedValue(true);

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // Save
    const saveBtn = screen.getByTestId("detail-save-btn");
    fireEvent.click(saveBtn);
    await waitFor(() => {
      expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalled();
    });

    // Refresh
    const refreshBtn = screen.getByTestId("detail-refresh-btn");
    fireEvent.click(refreshBtn);
    await waitFor(() => {
      expect(tauriService.refreshFavorite).toHaveBeenCalledWith(42);
    });

    // Delete
    const deleteBtn = screen.getByTestId("detail-delete-btn");
    fireEvent.click(deleteBtn);
    const confirmBtn = await screen.findByTestId("confirm-delete-btn");
    fireEvent.click(confirmBtn);
    await waitFor(() => {
      expect(tauriService.deleteFavorite).toHaveBeenCalledWith(42);
      expect(onDeleted).toHaveBeenCalledWith(42);
    });
  });

  it("P6C-FE-25: Existing github/ai/user tag visual distinction is preserved", async () => {
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: false,
      reason: "AI disabled",
    });

    render(
      <FavoriteDetailModal
        isOpen={true}
        favorite={sampleFavorite}
        onClose={onClose}
        onUpdated={onUpdated}
        onDeleted={onDeleted}
      />
    );

    // GitHub tag shows (github) suffix
    expect(screen.getByText("(github)")).toBeInTheDocument();
    // AI tag shows (ai) suffix
    expect(screen.getByText("(ai)")).toBeInTheDocument();
    // User tag is in editable container with remove button
    expect(screen.getByTestId("user-tag-crawler")).toBeInTheDocument();
  });

  it("P6C-FE-26: AddRepositoryPage has NO AI suggestion UI", () => {
    render(<AddRepositoryPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
    expect(screen.queryByText(/AI 建议/i)).not.toBeInTheDocument();
  });

  it("P6C-FE-27: SearchPage behavior unchanged and has no favorite AI suggestion button", () => {
    render(<SearchPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
  });

  it("P6C-FE-28: TrendingPage behavior unchanged and has no favorite AI suggestion button", () => {
    render(<TrendingPage onNavigateToFavorite={vi.fn()} />);
    expect(screen.queryByTestId("ai-suggest-btn")).not.toBeInTheDocument();
  });
});
