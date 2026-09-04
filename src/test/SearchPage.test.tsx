import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SearchPage } from "../pages/SearchPage";
import { GitHubRepository, RepositorySearchResponse } from "../types/github";

const mockSampleRepos: GitHubRepository[] = [
  {
    github_repo_id: 101,
    github_url: "https://github.com/tauri-apps/tauri",
    name: "tauri",
    description: "Build smaller, faster, and more secure desktop applications.",
    stars: 85000,
    license: "Apache-2.0",
    topics: ["rust", "desktop", "tauri"],
    pushed_at: "2026-03-01T12:00:00Z",
    category: "Developer Tools",
  },
  {
    github_repo_id: 102,
    github_url: "https://github.com/sqlx/sqlx",
    name: "sqlx",
    description: "The Rust SQL Toolkit.",
    stars: 15000,
    license: "MIT",
    topics: ["database", "sql"],
    pushed_at: "2026-02-28T10:00:00Z",
    category: "Data & Database",
  },
];

let mockSearchResponse: RepositorySearchResponse = {
  items: mockSampleRepos,
  total_count: 2,
};
let mockSearchError: { code: string; message: string } | null = null;
let lastSearchRequest: Record<string, unknown> | null = null;
let openedUrl: string | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: false, provider: null, base_url: null, model: null },
        });
      case "github_search_repositories":
        lastSearchRequest = args?.request as Record<string, unknown>;
        if (mockSearchError) {
          return Promise.reject(mockSearchError);
        }
        return Promise.resolve(mockSearchResponse);
      case "open_github_url":
        openedUrl = (args?.url as string) || null;
        return Promise.resolve();
      default:
        return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
    }
  }),
}));

describe("SearchPage Phase 2 Test Suite", () => {
  beforeEach(() => {
    mockSearchResponse = {
      items: [...mockSampleRepos],
      total_count: 2,
    };
    mockSearchError = null;
    lastSearchRequest = null;
    openedUrl = null;
    vi.clearAllMocks();
  });

  it("1. renders SearchPage with form, mode switcher, and idle state", () => {
    render(<SearchPage />);
    expect(screen.getByText("GitHub Repository Search")).toBeInTheDocument();
    expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    expect(screen.getByTestId("search-min-stars")).toBeInTheDocument();
    expect(screen.getByTestId("search-last-active")).toBeInTheDocument();
    expect(screen.getByTestId("search-limit")).toBeInTheDocument();
    expect(screen.getByText("准备搜索")).toBeInTheDocument();
  });

  it("2. AI mode is disabled with Phase 6 badge", () => {
    render(<SearchPage />);
    expect(screen.getByText("AI自然语言搜索")).toBeInTheDocument();
    expect(screen.getByText("Available in Phase 6")).toBeInTheDocument();
  });

  it("3. Phase 5 controls are disabled with badges", () => {
    render(<SearchPage />);
    expect(screen.getByText("排除以前入选")).toBeInTheDocument();
    expect(screen.getByText("排除已收藏")).toBeInTheDocument();
    const phase5Badges = screen.getAllByText("Phase 5");
    expect(phase5Badges.length).toBe(2);
  });

  it("4. handles successful search and passes keyword, stars, pushed_after, and limit", async () => {
    render(<SearchPage />);

    const queryInput = screen.getByTestId("search-query-input");
    const starsInput = screen.getByTestId("search-min-stars");
    const activeSelect = screen.getByTestId("search-last-active");
    const limitSelect = screen.getByTestId("search-limit");
    const submitBtn = screen.getByTestId("search-submit-btn");

    fireEvent.change(queryInput, { target: { value: "tauri desktop" } });
    fireEvent.change(starsInput, { target: { value: "100" } });
    fireEvent.change(activeSelect, { target: { value: "month" } });
    fireEvent.change(limitSelect, { target: { value: "50" } });

    fireEvent.click(submitBtn);

    await waitFor(() => {
      expect(screen.getByText("共找到 2 个公开项目")).toBeInTheDocument();
    });

    expect(screen.getByText("tauri")).toBeInTheDocument();
    expect(screen.getByText("sqlx")).toBeInTheDocument();

    // Verify parameters passed to backend IPC
    expect(lastSearchRequest).not.toBeNull();
    expect(lastSearchRequest?.query).toBe("tauri desktop");
    expect(lastSearchRequest?.min_stars).toBe(100);
    expect(lastSearchRequest?.limit).toBe(50);
    expect(typeof lastSearchRequest?.pushed_after).toBe("string");
  });

  it("5. handles empty search results", async () => {
    mockSearchResponse = { items: [], total_count: 0 };
    render(<SearchPage />);

    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "nonexistent query" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("search-empty")).toBeInTheDocument();
    });

    expect(screen.getByText("未找到符合条件的项目")).toBeInTheDocument();
  });

  it("6. handles generic search error state", async () => {
    mockSearchError = {
      code: "GitHubNetwork",
      message: "Network connection failed",
    };
    render(<SearchPage />);

    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "test" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("search-error")).toBeInTheDocument();
    });

    expect(screen.getByText(/Network connection failed/)).toBeInTheDocument();
  });

  it("7. handles rate limited search error state", async () => {
    mockSearchError = {
      code: "GitHubRateLimit",
      message: "GitHub API rate limit exhausted for resource 'search'",
    };
    render(<SearchPage />);

    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "test" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("search-rate-limited")).toBeInTheDocument();
    });

    expect(
      screen.getByText("GitHub API 访问速率已达上限")
    ).toBeInTheDocument();
  });

  it("8. clicking external GitHub link opens browser via open_github_url IPC", async () => {
    render(<SearchPage />);

    const queryInput = screen.getByTestId("search-query-input");
    fireEvent.change(queryInput, { target: { value: "tauri" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    const openLinks = screen.getAllByTestId("open-github-link");
    expect(openLinks.length).toBeGreaterThan(0);

    fireEvent.click(openLinks[0]);

    await waitFor(() => {
      expect(openedUrl).toBe("https://github.com/tauri-apps/tauri");
    });
  });
});
