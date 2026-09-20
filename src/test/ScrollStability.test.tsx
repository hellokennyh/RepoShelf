import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import App from "../App";
import { GitHubRepository } from "../types/github";

const sampleRepos: GitHubRepository[] = Array.from({ length: 15 }, (_, i) => ({
  github_repo_id: 100 + i,
  github_url: `https://github.com/test-org/repo-${i}`,
  name: `repo-${i}`,
  description: `Description for repository number ${i}. Providing extensive library functionality.`,
  stars: 1000 * (i + 1),
  license: "MIT",
  topics: ["rust", "test"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, _args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_app_info":
        return Promise.resolve({ name: "RepoShelf", phase: "v1.1.1 S3" });
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: true, provider: "ollama", base_url: "http://localhost:11434", model: "qwen2.5:7b" },
          ui: { locale: "en" },
        });
      case "get_ai_search_readiness":
        return Promise.resolve({ ready: true, reason: "" });
      case "check_favorite_identities_batch":
      case "find_favorite_identities":
        return Promise.resolve({ matched_repo_ids: [], matched_urls: [] });
      case "get_trending":
        return Promise.resolve({
          period: "daily",
          requested_limit: 15,
          returned_count: 15,
          items: sampleRepos.map((r, idx) => ({ ...r, trending_rank: idx + 1, period_stars: 50 })),
          enrichment_summary: { attempted: 15, succeeded: 15, failed: 0, rate_limited: false, rate_limit_reset: null },
          is_short_source: false,
          partial_warning: null,
        });
      case "github_search_repositories":
        return Promise.resolve({ items: sampleRepos, total_count: 15 });
      case "get_distinct_tags":
        return Promise.resolve([]);
      case "check_repo_exists":
        return Promise.resolve(false);
      case "translate_repository_description":
        return Promise.resolve({
          translated_text: "[Translated] Description",
          target_locale: "en",
          source_hash: "hash_123",
          action: "translated",
          provider: "ollama",
          model: "qwen2.5:7b",
        });
      default:
        return Promise.resolve({});
    }
  }),
}));

describe("Stage S3: Cross-Page Scroll Stability & Jump-to-Top Prevention", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("AC050, AC053: Checkbox toggles and selection state changes do NOT reset scrollTop", async () => {
    const { container } = render(<App />);

    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("repo-0")).toBeInTheDocument();
    });

    const mainEl = container.querySelector("main") as HTMLElement;
    expect(mainEl).toBeTruthy();

    // User scrolls down reading items
    mainEl.scrollTop = 450;
    expect(mainEl.scrollTop).toBe(450);

    // Toggle a checkbox in the list
    const checkbox0 = screen.getByTestId("repo-checkbox-100");
    fireEvent.click(checkbox0);
    expect(checkbox0).toBeChecked();

    // Verify scrollTop remained completely stable at 450
    expect(mainEl.scrollTop).toBe(450);

    // Toggle another checkbox
    const checkbox1 = screen.getByTestId("repo-checkbox-101");
    fireEvent.click(checkbox1);
    expect(checkbox1).toBeChecked();

    // Still stable at 450
    expect(mainEl.scrollTop).toBe(450);
  });

  it("AC052, AC054: Batch translation execution does NOT reset scrollTop to 0", async () => {
    const { container } = render(<App />);

    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("repo-0")).toBeInTheDocument();
    });

    const mainEl = container.querySelector("main") as HTMLElement;
    expect(mainEl).toBeTruthy();

    // User scrolls to 320px
    mainEl.scrollTop = 320;
    expect(mainEl.scrollTop).toBe(320);

    // Start batch translate
    const batchBtn = screen.getByTestId("trending-batch-translate-btn");
    fireEvent.click(batchBtn);

    // Wait for at least one translation to complete
    await waitFor(() => {
      expect(screen.getAllByText(/\[Translated\]/i).length).toBeGreaterThan(0);
    });

    // Scroll must not jump back to 0!
    expect(mainEl.scrollTop).toBe(320);
  });

  it("AC058, AC059: Navigating between tabs saves and restores scroll positions accurately", async () => {
    const { container } = render(<App />);

    fireEvent.click(screen.getByTestId("trending-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("repo-0")).toBeInTheDocument();
    });

    const mainEl = container.querySelector("main") as HTMLElement;
    expect(mainEl).toBeTruthy();

    // 1. On Trending tab: scroll to 280
    mainEl.scrollTop = 280;

    // 2. Switch to Search tab
    fireEvent.click(screen.getByTestId("nav-search"));
    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    // Search starts at 0
    expect(mainEl.scrollTop).toBe(0);

    // Perform query and scroll to 190
    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "test" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("repo-0")).toBeInTheDocument();
    });
    mainEl.scrollTop = 190;

    // 3. Switch back to Trending tab
    fireEvent.click(screen.getByTestId("nav-trending"));
    await waitFor(() => {
      expect(screen.getByText("repo-0")).toBeInTheDocument();
    });

    // Trending scroll must be accurately restored to 280
    expect(mainEl.scrollTop).toBe(280);

    // 4. Switch back to Search tab
    fireEvent.click(screen.getByTestId("nav-search"));
    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    // Search scroll must be accurately restored to 190
    expect(mainEl.scrollTop).toBe(190);
  });
});
