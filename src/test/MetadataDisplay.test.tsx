import { render, screen } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import { RepositoryCard } from "../components/RepositoryCard";
import { FavoriteDetailModal } from "../components/FavoriteDetailModal";
import type { GitHubRepository } from "../types/github";
import type { FavoriteItem } from "../types/library";

describe("S2 Extended Metadata Display Suite", () => {
  describe("RepositoryCard", () => {
    it("renders releases count and compact languages bar when present", () => {
      const mockRepo: GitHubRepository = {
        github_repo_id: 123,
        name: "awesome-rust",
        github_url: "https://github.com/rust/awesome-rust",
        description: "A great rust repo",
        category: "Languages",
        stars: 5000,
        license: "MIT",
        topics: ["rust", "systems"],
        pushed_at: "2026-03-01T00:00:00Z",
        releases: 42,
        languages: { Rust: 92.5, C: 7.5 },
      };

      render(<RepositoryCard repo={mockRepo} />);

      expect(screen.getByTestId("repo-releases")).toHaveTextContent("Releases 42");
      expect(screen.getByTestId("repo-languages")).toBeInTheDocument();
      expect(screen.getByText("Rust")).toBeInTheDocument();
      expect(screen.getByText("92.5%")).toBeInTheDocument();
    });

    it("renders releases placeholder 'Releases —' and unknown languages when null", () => {
      const mockRepo: GitHubRepository = {
        github_repo_id: 124,
        name: "empty-repo",
        github_url: "https://github.com/unknown/empty-repo",
        description: null,
        category: "Other",
        stars: 0,
        license: null,
        topics: [],
        pushed_at: null,
        releases: null,
        languages: null,
      };

      render(<RepositoryCard repo={mockRepo} />);

      expect(screen.getByTestId("repo-releases")).toHaveTextContent("Releases —");
      expect(screen.getByTestId("languages-unknown")).toHaveTextContent("Languages —");
    });
  });

  describe("FavoriteDetailModal", () => {
    it("renders releases in metadata grid and full languages bar", () => {
      const mockFavorite: FavoriteItem = {
        id: 1,
        github_repo_id: 101,
        github_url: "https://github.com/org/repo",
        name: "repo",
        description: "A wonderful project",
        category: "Development",
        stars: 1234,
        license: "Apache-2.0",
        pushed_at: "2026-03-01T00:00:00Z",
        rating: 5,
        rating_reason: null,
        rating_confidence: null,
        notes: "My notes",
        favorited_at: "2026-03-01T00:00:00Z",
        metadata_refreshed_at: "2026-03-01T00:00:00Z",
        tags: [],
        releases: 15,
        languages: { TypeScript: 80.0, CSS: 20.0 },
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

      // Verify Releases in grid
      expect(screen.getByTestId("detail-releases-count")).toHaveTextContent("15");

      // Verify Languages section
      expect(screen.getByText("Languages 代码语言分布")).toBeInTheDocument();
      expect(screen.getByText("TypeScript")).toBeInTheDocument();
      expect(screen.getByText("80.0%")).toBeInTheDocument();
      expect(screen.getByText("CSS")).toBeInTheDocument();
      expect(screen.getByText("20.0%")).toBeInTheDocument();
    });

    it("renders releases placeholder when releases is null", () => {
      const mockFavorite: FavoriteItem = {
        id: 2,
        github_repo_id: 102,
        github_url: "https://github.com/org/repo2",
        name: "repo2",
        description: null,
        category: "Other",
        stars: 0,
        license: null,
        pushed_at: null,
        rating: null,
        rating_reason: null,
        rating_confidence: null,
        notes: "",
        favorited_at: "2026-03-01T00:00:00Z",
        metadata_refreshed_at: "2026-03-01T00:00:00Z",
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

      expect(screen.getByTestId("detail-releases-count")).toHaveTextContent("Releases —");
      expect(screen.getByTestId("languages-unknown")).toHaveTextContent("Languages —");
    });
  });
});
