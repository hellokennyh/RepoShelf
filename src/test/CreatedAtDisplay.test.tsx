import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { RepositoryCard } from "../components/RepositoryCard";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import type { GitHubRepository } from "../types/github";
import type { FavoriteItem } from "../types/library";

vi.mock("../services/tauri", () => ({
  openGitHubUrl: vi.fn(),
  getAiSearchReadiness: vi.fn().mockResolvedValue({ ready: false, reason: null }),
  updateFavoriteUserMetadata: vi.fn(),
  deleteFavorite: vi.fn(),
  refreshFavorite: vi.fn(),
}));

describe("S1 CreatedAt Display Suite (AC013, AC014, AC015)", () => {
  describe("RepositoryCard created_at display", () => {
    it("renders formatted created_at date when present", () => {
      const mockRepo: GitHubRepository = {
        github_repo_id: 101,
        name: "test-created-at",
        github_url: "https://github.com/org/test-created-at",
        description: "Test description",
        category: "Development",
        stars: 100,
        license: "MIT",
        topics: ["react"],
        pushed_at: "2024-03-01T00:00:00Z",
        created_at: "2023-01-15T10:00:00Z",
        releases: 5,
        languages: { Rust: 100 },
      };

      render(<RepositoryCard repo={mockRepo} />);

      const createdAtEl = screen.getByTestId("repo-created-at");
      expect(createdAtEl).toBeInTheDocument();
      // Should contain 2023 and 1 / Jan
      expect(createdAtEl.textContent).toMatch(/2023/);
    });

    it("renders unknown/fallback when created_at is null or missing", () => {
      const mockRepo: GitHubRepository = {
        github_repo_id: 102,
        name: "test-missing-created-at",
        github_url: "https://github.com/org/test-missing",
        description: "Test description",
        category: "Other",
        stars: 50,
        license: null,
        topics: [],
        pushed_at: null,
        created_at: null,
        releases: null,
        languages: null,
      };

      render(<RepositoryCard repo={mockRepo} />);

      const createdAtEl = screen.getByTestId("repo-created-at");
      expect(createdAtEl).toBeInTheDocument();
      expect(createdAtEl.textContent).toMatch(/未知|Unknown|—/);
    });
  });

  describe("FavoriteDetailModal created_at display", () => {
    it("renders created_at in metadata grid when present", () => {
      const mockFavorite: FavoriteItem = {
        id: 1,
        github_repo_id: 201,
        github_url: "https://github.com/org/detail-repo",
        name: "detail-repo",
        description: "Detailed description",
        category: "Tools",
        stars: 320,
        license: "MIT",
        pushed_at: "2024-06-01T00:00:00Z",
        repo_created_at: "2022-11-20T08:30:00Z",
        rating: 4,
        rating_reason: null,
        rating_confidence: null,
        notes: "Some notes",
        favorited_at: "2024-06-02T10:00:00Z",
        metadata_refreshed_at: "2024-06-02T10:00:00Z",
        tags: [],
        releases: 2,
        languages: { Go: 100 },
      };

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFavorite}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      const detailCreatedAtEl = screen.getByTestId("detail-created-at");
      expect(detailCreatedAtEl).toBeInTheDocument();
      expect(detailCreatedAtEl.textContent).toMatch(/2022/);
      expect(detailCreatedAtEl.textContent).toMatch(/11/);
    });

    it("renders fallback placeholder in metadata grid when repo_created_at is null", () => {
      const mockFavorite: FavoriteItem = {
        id: 2,
        github_repo_id: 202,
        github_url: "https://github.com/org/detail-repo-null",
        name: "detail-repo-null",
        description: "Detailed description",
        category: "Tools",
        stars: 10,
        license: null,
        pushed_at: null,
        repo_created_at: null,
        rating: null,
        rating_reason: null,
        rating_confidence: null,
        notes: "",
        favorited_at: "2024-06-02T10:00:00Z",
        metadata_refreshed_at: null,
        tags: [],
        releases: null,
        languages: null,
      };

      render(
        <FavoriteDetailModal
          isOpen={true}
          favorite={mockFavorite}
          onClose={vi.fn()}
          onUpdated={vi.fn()}
          onDeleted={vi.fn()}
        />
      );

      const detailCreatedAtEl = screen.getByTestId("detail-created-at");
      expect(detailCreatedAtEl).toBeInTheDocument();
      expect(detailCreatedAtEl.textContent).toMatch(/未知|Unknown|—/);
    });
  });
});
