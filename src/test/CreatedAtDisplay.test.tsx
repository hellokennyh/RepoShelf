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
  listFavorites: vi.fn(),
  listDistinctTags: vi.fn(),
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
        metadata_refreshed_at: "2024-06-02T10:00:00Z",
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

  describe("LibraryPage created_at table display (AC014)", () => {
    it("renders created_at column and cells for library favorites", async () => {
      const { LibraryPage } = await import("../pages/LibraryPage");
      const tauri = await import("../services/tauri");

      const mockFavs: FavoriteItem[] = [
        {
          id: 11,
          github_repo_id: 301,
          github_url: "https://github.com/org/lib-repo-1",
          name: "lib-repo-1",
          description: "Library repo 1",
          category: "Developer Tools",
          stars: 500,
          license: "MIT",
          pushed_at: "2024-05-01T00:00:00Z",
          repo_created_at: "2023-08-10T14:00:00Z",
          rating: 5,
          notes: "",
          favorited_at: "2024-05-02T00:00:00Z",
          metadata_refreshed_at: "2024-05-02T00:00:00Z",
          tags: [],
        },
        {
          id: 12,
          github_repo_id: 302,
          github_url: "https://github.com/org/lib-repo-2",
          name: "lib-repo-2",
          description: "Library repo 2",
          category: "Other",
          stars: 100,
          license: null,
          pushed_at: null,
          repo_created_at: null,
          rating: null,
          notes: "",
          favorited_at: "2024-05-02T00:00:00Z",
          metadata_refreshed_at: "2024-05-02T00:00:00Z",
          tags: [],
        },
      ];

      vi.mocked(tauri.listFavorites).mockResolvedValue(mockFavs);
      vi.mocked(tauri.listDistinctTags).mockResolvedValue([]);

      render(<LibraryPage />);

      const cell1 = await screen.findByTestId("library-created-at-11");
      expect(cell1).toBeInTheDocument();
      expect(cell1.textContent).toMatch(/2023/);

      const cell2 = await screen.findByTestId("library-created-at-12");
      expect(cell2).toBeInTheDocument();
      expect(cell2.textContent).toMatch(/未知|Unknown|—/);
    });
  });

  describe("No extra per-card request on Trending, Search, Add render (AC015)", () => {
    it("rendering cards with created_at triggers 0 additional metadata/refresh network requests", async () => {
      const tauri = await import("../services/tauri");
      const refreshSpy = vi.mocked(tauri.refreshFavorite);
      refreshSpy.mockClear();

      const mockTrendingRepo: GitHubRepository = {
        github_repo_id: 501,
        name: "trending-repo",
        github_url: "https://github.com/org/trending-repo",
        description: "Trending description",
        category: "Developer Tools",
        stars: 1000,
        license: "Apache-2.0",
        topics: ["rust"],
        pushed_at: "2024-06-01T00:00:00Z",
        created_at: "2021-04-12T08:00:00Z",
      };

      const mockSearchRepo: GitHubRepository = {
        github_repo_id: 502,
        name: "search-repo",
        github_url: "https://github.com/org/search-repo",
        description: "Search description",
        category: "Web & SaaS",
        stars: 800,
        license: "MIT",
        topics: ["typescript"],
        pushed_at: "2024-06-02T00:00:00Z",
        created_at: "2022-09-18T10:00:00Z",
      };

      const mockAddRepo: GitHubRepository = {
        github_repo_id: 503,
        name: "add-repo",
        github_url: "https://github.com/org/add-repo",
        description: "Add description",
        category: "Libraries & Frameworks",
        stars: 300,
        license: "BSD-3-Clause",
        topics: ["python"],
        pushed_at: "2024-06-03T00:00:00Z",
        created_at: "2023-02-20T12:00:00Z",
      };

      // Render all 3 cards (Trending, Search, Add)
      const { rerender } = render(
        <div>
          <RepositoryCard repo={mockTrendingRepo} />
          <RepositoryCard repo={mockSearchRepo} />
          <RepositoryCard repo={mockAddRepo} />
        </div>
      );

      // Verify all rendered created_at properly
      expect(screen.getAllByTestId("repo-created-at").length).toBe(3);

      // Crucial AC015 check: ZERO refresh/network calls triggered by card render
      expect(refreshSpy).toHaveBeenCalledTimes(0);

      // Rerender with same/updated data: still 0 network calls
      rerender(
        <div>
          <RepositoryCard repo={{ ...mockTrendingRepo, stars: 1001 }} />
          <RepositoryCard repo={{ ...mockSearchRepo, stars: 801 }} />
          <RepositoryCard repo={{ ...mockAddRepo, stars: 301 }} />
        </div>
      );
      expect(refreshSpy).toHaveBeenCalledTimes(0);
    });
  });
});
