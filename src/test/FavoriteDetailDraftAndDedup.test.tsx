import { useState } from "react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import { FavoriteItem } from "../types/library";
import * as tauriService from "../services/tauri";
import { normalizeTagForDedup, filterSupplementalAiTags } from "../utils/tagUtils";

vi.mock("../services/tauri", () => ({
  updateFavoriteUserMetadata: vi.fn(),
  applyFavoriteAiMetadata: vi.fn(),
  suggestFavoriteAiMetadata: vi.fn(),
  getAiSearchReadiness: vi.fn(),
  refreshFavorite: vi.fn(),
  deleteFavorite: vi.fn(),
  openGitHubUrl: vi.fn(),
}));

const sampleFavorite: FavoriteItem = {
  id: 42,
  github_repo_id: 1001,
  github_url: "https://github.com/test-owner/test-repo",
  name: "test-repo",
  description: "A fast scraper tool for web data",
  category: "Automation",
  stars: 1200,
  license: "MIT",
  pushed_at: "2026-03-01T00:00:00Z",
  rating: 3,
  notes: "initial notes",
  favorited_at: "2026-03-01T10:00:00Z",
  metadata_refreshed_at: "2026-03-01T10:00:00Z",
  tags: [
    { id: 1, favorite_id: 42, tag: "web-scraping", source: "github" },
    { id: 2, favorite_id: 42, tag: "python", source: "github" },
    { id: 3, favorite_id: 42, tag: "my-scraper", source: "user" },
    { id: 4, favorite_id: 42, tag: "existing-ai", source: "ai" },
  ],
};

describe("FavoriteDetailDraftAndDedup Acceptance Suite", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(tauriService.getAiSearchReadiness).mockResolvedValue({
      ready: true,
      reason: null,
    });
  });

  describe("Tag Deduplication Utilities Contract", () => {
    it("normalizeTagForDedup normalizes case and trims whitespace", () => {
      expect(normalizeTagForDedup("  LLM  ")).toBe("llm");
      expect(normalizeTagForDedup("Web-Scraping")).toBe("web-scraping");
      expect(normalizeTagForDedup("desktop-app")).toBe("desktop-app");
    });

    it("filterSupplementalAiTags removes exact matches, case-insensitive, and whitespace duplicates", () => {
      const ghTopics = ["web-scraping", "PYTHON", "  fastapi  "];
      const aiSuggestions = [
        "Web-Scraping", // duplicate of web-scraping (case insensitive)
        "python",       // duplicate of PYTHON
        "fastapi",      // duplicate of fastapi (whitespace trimmed)
        "crawler",      // supplemental (unique)
        "data-mining",  // supplemental (unique)
        "crawler",      // intra-ai duplicate
      ];
      const result = filterSupplementalAiTags(aiSuggestions, ghTopics);
      expect(result).toEqual(["crawler", "data-mining"]);
    });

    it("filterSupplementalAiTags preserves distinct but similar tags", () => {
      const ghTopics = ["video"];
      const aiSuggestions = ["video-generation", "ai-video"];
      const result = filterSupplementalAiTags(aiSuggestions, ghTopics);
      expect(result).toEqual(["video-generation", "ai-video"]);
    });
  });

  describe("Scenario A — Discard (X closes modal and drops all draft modifications)", () => {
    it("AI suggestion generated -> user clicks X -> zero DB mutations and reopened modal has original values", async () => {
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Developer Tools",
        tags: ["web-scraping", "ai-crawler"], // "web-scraping" duplicates GitHub tag
        rating: 5,
        rating_reason: "High quality scraping library",
        confidence: "high",
      });

      const onClose = vi.fn();
      const onUpdated = vi.fn();

      const Harness = () => {
        const [isOpen, setIsOpen] = useState(true);
        return (
          <div>
            <button data-testid="reopen-btn" onClick={() => setIsOpen(true)}>
              Reopen
            </button>
            <FavoriteDetailModal
              isOpen={isOpen}
              favorite={sampleFavorite}
              onClose={() => {
                setIsOpen(false);
                onClose();
              }}
              onUpdated={onUpdated}
              onDeleted={vi.fn()}
            />
          </div>
        );
      };

      render(<Harness />);

      // Verify initial state
      const select = screen.getByTestId("detail-category-select") as HTMLSelectElement;
      expect(select.value).toBe("Automation");
      expect(screen.getByText("3 星")).toBeInTheDocument();

      // Click AI suggest button
      const suggestBtn = screen.getByTestId("ai-suggest-btn");
      await waitFor(() => {
        expect(suggestBtn).not.toBeDisabled();
      });
      fireEvent.click(suggestBtn);

      await waitFor(() => {
        const catSelect = screen.getByTestId("detail-category-select") as HTMLSelectElement;
        expect(catSelect.value).toBe("Developer Tools");
        expect(screen.getByTestId("ai-suggested-rating")).toHaveTextContent("5 星");
        expect(screen.getByTestId("ai-rating-reason-text")).toHaveTextContent("High quality scraping library");
        expect(screen.getByTestId("ai-suggested-tags")).toHaveTextContent("#ai-crawler");
        expect(screen.getByTestId("ai-suggested-tags")).not.toHaveTextContent("#web-scraping");
      });

      // Confirm zero DB writes occurred
      expect(tauriService.updateFavoriteUserMetadata).not.toHaveBeenCalled();
      expect(tauriService.applyFavoriteAiMetadata).not.toHaveBeenCalled();
      expect(onUpdated).not.toHaveBeenCalled();

      // Click X (detail-close-btn)
      const closeBtn = screen.getByTestId("detail-close-btn");
      fireEvent.click(closeBtn);

      expect(onClose).toHaveBeenCalled();
      expect(screen.queryByTestId("favorite-detail-modal")).not.toBeInTheDocument();

      // Reopen modal
      const reopenBtn = screen.getByTestId("reopen-btn");
      fireEvent.click(reopenBtn);

      // Verify NONE of the unsaved changes remain: restored from persisted favorite
      expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();
      const reopenedSelect = screen.getByTestId("detail-category-select") as HTMLSelectElement;
      expect(reopenedSelect.value).toBe("Automation"); // restored original
      expect(screen.getByText("3 星")).toBeInTheDocument(); // restored original
      expect(screen.queryByText("High quality scraping library")).not.toBeInTheDocument();
      expect(screen.queryByText(/#ai-crawler/)).not.toBeInTheDocument();
      expect(screen.getByText(/#existing-ai/)).toBeInTheDocument(); // original AI tag
    });
  });

  describe("Scenario B — Save & Exit (Atomic persistence and automatic close)", () => {
    it("Button is labeled '保存退出' and clicking it atomically saves all draft fields and closes modal", async () => {
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Developer Tools",
        tags: ["web-scraping", "fast-scraper"],
        rating: 5,
        rating_reason: "High speed parser",
        confidence: "high",
      });

      const persistedUpdated: FavoriteItem = {
        ...sampleFavorite,
        category: "Libraries & Frameworks", // user will override
        rating: 4, // user will override
        notes: "updated notes",
        tags: [
          { id: 1, favorite_id: 42, tag: "web-scraping", source: "github" },
          { id: 2, favorite_id: 42, tag: "python", source: "github" },
          { id: 10, favorite_id: 42, tag: "custom-tag", source: "user" },
          { id: 20, favorite_id: 42, tag: "fast-scraper", source: "ai" },
        ],
      };

      vi.mocked(tauriService.updateFavoriteUserMetadata).mockResolvedValue(persistedUpdated);

      const onClose = vi.fn();
      const onUpdated = vi.fn();

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={onClose}
          onUpdated={onUpdated}
          onDeleted={vi.fn()}
        />
      );

      // Verify old button name "保存修改" is NOT present and new "保存退出" IS present
      expect(screen.queryByText("保存修改")).not.toBeInTheDocument();
      const saveBtn = screen.getByTestId("detail-save-btn");
      expect(saveBtn).toHaveTextContent("保存退出");

      // Generate AI suggestion
      const suggestBtn = screen.getByTestId("ai-suggest-btn");
      await waitFor(() => {
        expect(suggestBtn).not.toBeDisabled();
      });
      fireEvent.click(suggestBtn);
      await waitFor(() => {
        expect(screen.getByTestId("ai-suggested-tags")).toHaveTextContent("#fast-scraper");
      });

      // User manually customizes draft fields:
      // 1. Change Category to "Libraries & Frameworks"
      const categorySelect = screen.getByTestId("detail-category-select");
      fireEvent.change(categorySelect, { target: { value: "Libraries & Frameworks" } });

      // 2. Change Rating to 4
      fireEvent.click(screen.getByTestId("detail-star-4"));

      // 3. Edit notes
      const notesArea = screen.getByTestId("detail-notes-textarea");
      fireEvent.change(notesArea, { target: { value: "updated notes" } });

      // 4. Add user tag "custom-tag"
      const tagInput = screen.getByTestId("detail-add-tag-input");
      fireEvent.change(tagInput, { target: { value: "custom-tag" } });
      fireEvent.submit(tagInput.closest("form")!);

      // Click "保存退出"
      fireEvent.click(saveBtn);

      // Verify atomic call with ALL draft fields
      await waitFor(() => {
        expect(tauriService.updateFavoriteUserMetadata).toHaveBeenCalledWith({
          favorite_id: 42,
          category: "Libraries & Frameworks",
          rating: 4,
          rating_reason: "High speed parser",
          rating_confidence: "high",
          notes: "updated notes",
          user_tags: ["my-scraper", "custom-tag"],
          ai_tags: ["fast-scraper"],
        });
        // onUpdated called with backend result
        expect(onUpdated).toHaveBeenCalledWith(persistedUpdated);
        // Modal automatically closed
        expect(onClose).toHaveBeenCalled();
      });
    });

    it("Save failure displays error banner, keeps modal open, and keeps draft intact", async () => {
      vi.mocked(tauriService.updateFavoriteUserMetadata).mockRejectedValue(
        new Error("Disk I/O failure")
      );

      const onClose = vi.fn();
      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={onClose}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      // Modify notes
      const notesArea = screen.getByTestId("detail-notes-textarea") as HTMLTextAreaElement;
      fireEvent.change(notesArea, { target: { value: "unsaved draft note" } });

      // Click 保存退出
      fireEvent.click(screen.getByTestId("detail-save-btn"));

      // Error banner is displayed
      const banner = await screen.findByTestId("detail-feedback-banner");
      expect(banner).toHaveTextContent("保存失败: Disk I/O failure");

      // Modal stays open
      expect(onClose).not.toHaveBeenCalled();
      expect(screen.getByTestId("favorite-detail-modal")).toBeInTheDocument();

      // Draft remains intact for retry
      expect(notesArea.value).toBe("unsaved draft note");
    });
  });

  describe("Scenario C — Tag Labels Presentation", () => {
    it("GitHub tags show NO '(github)' marker; AI tags show '(ai)' marker", () => {
      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      const modal = screen.getByTestId("favorite-detail-modal");

      // GitHub tags
      expect(within(modal).getByText("#web-scraping")).toBeInTheDocument();
      expect(within(modal).getByText("#python")).toBeInTheDocument();
      // Absolutely NO "(github)" text anywhere in the modal
      expect(within(modal).queryByText("(github)")).not.toBeInTheDocument();
      expect(within(modal).queryByText(/github/i, { selector: ".text-\\[10px\\]" })).not.toBeInTheDocument();

      // AI tag has (ai) marker
      expect(within(modal).getByText("#existing-ai")).toBeInTheDocument();
      expect(within(modal).getByText("(ai)")).toBeInTheDocument();
    });
  });

  describe("Parent State Isolation & Refresh Invariant", () => {
    it("Draft edits in modal do not mutate the parent's favorite object", () => {
      const parentFavCopy = JSON.parse(JSON.stringify(sampleFavorite));
      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={parentFavCopy}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      // Modify form inputs
      fireEvent.change(screen.getByTestId("detail-notes-textarea"), {
        target: { value: "locally changed notes" },
      });
      fireEvent.click(screen.getByTestId("detail-star-5"));

      // Parent object must remain unchanged
      expect(parentFavCopy.notes).toBe("initial notes");
      expect(parentFavCopy.rating).toBe(3);
    });

    it("GitHub refresh preserves unsaved draft and filters draft AI tags against refreshed topics", async () => {
      // Simulate that GitHub refresh added a new topic "fast-scraper"
      const refreshedFav: FavoriteItem = {
        ...sampleFavorite,
        stars: 1300,
        tags: [
          { id: 1, favorite_id: 42, tag: "web-scraping", source: "github" },
          { id: 2, favorite_id: 42, tag: "python", source: "github" },
          { id: 5, favorite_id: 42, tag: "fast-scraper", source: "github" }, // newly added by GitHub
          { id: 3, favorite_id: 42, tag: "my-scraper", source: "user" },
          { id: 4, favorite_id: 42, tag: "existing-ai", source: "ai" },
        ],
      };

      vi.mocked(tauriService.refreshFavorite).mockResolvedValue({
        status: "succeeded",
        favorite_id: 42,
        favorite: refreshedFav,
      });

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={sampleFavorite}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      // User sets draft AI tags that include "fast-scraper"
      vi.mocked(tauriService.suggestFavoriteAiMetadata).mockResolvedValue({
        category: "Automation",
        tags: ["fast-scraper", "another-ai-tag"],
        rating: 4,
        rating_reason: "Cool tool",
        confidence: "medium",
      });

      const suggestBtn = screen.getByTestId("ai-suggest-btn");
      await waitFor(() => {
        expect(suggestBtn).not.toBeDisabled();
      });
      fireEvent.click(suggestBtn);
      await waitFor(() => {
        expect(screen.getByTestId("ai-suggested-tags")).toHaveTextContent("#fast-scraper");
      });

      // User modifies notes
      const notesArea = screen.getByTestId("detail-notes-textarea") as HTMLTextAreaElement;
      fireEvent.change(notesArea, { target: { value: "unpersisted draft note" } });

      // Click Refresh
      fireEvent.click(screen.getByTestId("detail-refresh-btn"));

      await waitFor(() => {
        expect(tauriService.refreshFavorite).toHaveBeenCalledWith(42);
      });

      // Draft note must still be preserved
      expect(notesArea.value).toBe("unpersisted draft note");

      const tagsContainer = screen.getByText("GitHub Topics & 元数据标签 (只读)").parentElement!;
      // Newly added GitHub topic "fast-scraper" appears under GitHub topics (without (ai))
      const ghBadge = within(tagsContainer).getByText("#fast-scraper");
      expect(ghBadge).not.toHaveTextContent("(ai)");

      // "fast-scraper" no longer exists as an AI tag with (ai) suffix
      expect(within(tagsContainer).queryByText(/#fast-scraper.*\(ai\)/)).not.toBeInTheDocument();

      // Supplemental draft AI tag is preserved with (ai)
      expect(within(tagsContainer).getByText(/#another-ai-tag/)).toBeInTheDocument();
      expect(within(tagsContainer).getByText(/#another-ai-tag/)).toHaveTextContent("(ai)");
    });
  });
});
