import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import type { InvokeArgs } from "@tauri-apps/api/core";
import { invoke } from "@tauri-apps/api/core";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { GitHubRepository } from "../types/github";
import { ArticleImportResult } from "../types/article";

const mockDirectRepo: GitHubRepository = {
  github_repo_id: 1001,
  github_url: "https://github.com/rust-lang/rust",
  name: "rust",
  description: "A language empowering everyone to build reliable and efficient software.",
  stars: 95000,
  license: "MIT",
  topics: ["rust", "compiler"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
  releases: 150,
  languages: { Rust: 95.5, C: 4.5 },
};

const mockArticleRepos: GitHubRepository[] = [
  {
    github_repo_id: 2001,
    github_url: "https://github.com/tokio-rs/tokio",
    name: "tokio",
    description: "An asynchronous runtime for Rust.",
    stars: 25000,
    license: "MIT",
    topics: ["async", "network"],
    pushed_at: "2026-03-01T12:00:00Z",
    category: "Developer Tools",
    releases: 80,
    languages: { Rust: 100.0 },
  },
  {
    github_repo_id: 2002,
    github_url: "https://github.com/tauri-apps/tauri",
    name: "tauri",
    description: "Build smaller, faster, and more secure desktop applications.",
    stars: 80000,
    license: "Apache-2.0",
    topics: ["rust", "desktop"],
    pushed_at: "2026-03-01T12:00:00Z",
    category: "Developer Tools",
    releases: 45,
    languages: { Rust: 70.0, TypeScript: 30.0 },
  },
];

const mockArticleResult: ArticleImportResult = {
  source_url: "https://example.com/blog/rust-ecosystem",
  repositories: mockArticleRepos,
  extracted_unique_count: 3,
  valid_count: 2,
  invalid_count: 1,
  skipped_count: 0,
  truncated: false,
  warning: null,
};

let mockFetchArticleError: { code: string; message: string } | null = null;
let mockFetchGitHubError: { code: string; message: string } | null = null;
let lastArticleUrl: string | null = null;
let lastGitHubUrl: string | null = null;
let favoritedIds: number[] = [];

const defaultMockInvoke = (cmd: string, args?: InvokeArgs) => {
  const argMap = args as Record<string, unknown> | undefined;
  switch (cmd) {
    case "github_get_repository":
      lastGitHubUrl = (argMap?.url as string) || null;
      if (mockFetchGitHubError) {
        return Promise.reject(mockFetchGitHubError);
      }
      return Promise.resolve(mockDirectRepo);

    case "import_repositories_from_article":
      lastArticleUrl = (argMap?.url as string) || null;
      if (mockFetchArticleError) {
        return Promise.reject(mockFetchArticleError);
      }
      return Promise.resolve(mockArticleResult);

    case "favorite_repository": {
      const input = argMap?.input as { github_repo_id?: number };
      if (input?.github_repo_id) {
        favoritedIds.push(input.github_repo_id);
      }
      return Promise.resolve({
        status: "succeeded",
        favorite_id: 99,
      });
    }

    case "check_favorite_identities_batch": {
      return Promise.resolve({
        matched_repo_ids: favoritedIds,
        matched_urls: [],
      });
    }

    default:
      return Promise.resolve({});
  }
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: InvokeArgs) => {
    return defaultMockInvoke(cmd, args);
  }),
}));

describe("S3 AddRepositoryPage Article Import Test Suite", () => {
  beforeEach(() => {
    mockFetchArticleError = null;
    mockFetchGitHubError = null;
    lastArticleUrl = null;
    lastGitHubUrl = null;
    favoritedIds = [];
    vi.mocked(invoke).mockImplementation(defaultMockInvoke);
  });

  it("1. renders updated product copy on page subtitle and input label", () => {
    render(<AddRepositoryPage />);
    const elements = screen.getAllByText("URL地址：输入GitHub地址或者包含GitHub链接的文章");
    expect(elements.length).toBeGreaterThanOrEqual(2);
  });

  it("2. direct GitHub repo URL routes to github_get_repository and shows direct preview", async () => {
    render(<AddRepositoryPage />);

    const input = screen.getByTestId("add-repo-url-input");
    const fetchBtn = screen.getByTestId("add-repo-fetch-btn");

    fireEvent.change(input, { target: { value: "https://github.com/rust-lang/rust" } });
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId("repo-preview-container")).toBeInTheDocument();
    });

    expect(lastGitHubUrl).toBe("https://github.com/rust-lang/rust");
    expect(lastArticleUrl).toBeNull();
    expect(screen.getByText("Repository Preview")).toBeInTheDocument();
    expect(screen.getByText("rust")).toBeInTheDocument();
  });

  it("3. article URL routes to import_repositories_from_article and displays multi-card results with summary", async () => {
    render(<AddRepositoryPage />);

    const input = screen.getByTestId("add-repo-url-input");
    const fetchBtn = screen.getByTestId("add-repo-fetch-btn");

    fireEvent.change(input, { target: { value: "https://example.com/blog/rust-ecosystem" } });
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId("article-results-section")).toBeInTheDocument();
    });

    expect(lastArticleUrl).toBe("https://example.com/blog/rust-ecosystem");
    expect(lastGitHubUrl).toBeNull();

    // Verify summary counts: "找到 3 个 GitHub 仓库", "成功读取 2 个", "1 个无效或不可访问"
    const summaryContainer = screen.getByTestId("article-summary-counts");
    expect(summaryContainer).toHaveTextContent("找到 3 个 GitHub 仓库");
    expect(summaryContainer).toHaveTextContent("成功读取 2 个");
    expect(summaryContainer).toHaveTextContent("1 个无效或不可访问");

    // Verify multi cards rendered
    const card1 = screen.getByTestId("article-repo-card-2001");
    const card2 = screen.getByTestId("article-repo-card-2002");
    expect(card1).toBeInTheDocument();
    expect(card2).toBeInTheDocument();
    expect(within(card1).getByText("tokio")).toBeInTheDocument();
    expect(within(card2).getByText("tauri")).toBeInTheDocument();
  });

  it("4. displays truncated notice when article exceeds 50 unique repos", async () => {
    const truncatedResult: ArticleImportResult = {
      ...mockArticleResult,
      extracted_unique_count: 50,
      truncated: true,
    };
    mockFetchArticleError = null;

    vi.mocked(invoke).mockImplementation((cmd, args) => {
      if (cmd === "import_repositories_from_article") {
        return Promise.resolve(truncatedResult);
      }
      return defaultMockInvoke(cmd, args);
    });

    render(<AddRepositoryPage />);
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/mega-list" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("article-truncated-warning")).toBeInTheDocument();
    });
    expect(screen.getByText(/已仅处理前 50 个唯一 GitHub 仓库链接/)).toBeInTheDocument();
  });

  it("5. handles SSRF blocked address error properly", async () => {
    mockFetchArticleError = {
      code: "BlockedAddress",
      message: "Target IP is blocked",
    };

    render(<AddRepositoryPage />);
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "http://192.168.1.1/article" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });
    expect(screen.getByText(/目标地址已被安全策略拒绝访问/)).toBeInTheDocument();
  });

  it("6. rejects unsupported URL schemes before network call", async () => {
    render(<AddRepositoryPage />);
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "ftp://example.com/file" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });
    expect(screen.getByText(/不支持的 URL 协议 'ftp:'/)).toBeInTheDocument();
    expect(lastArticleUrl).toBeNull();
  });

  it("7. handles NoGitHubRepositories error cleanly", async () => {
    mockFetchArticleError = {
      code: "NoGitHubRepositories",
      message: "No repos found",
    };

    render(<AddRepositoryPage />);
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/no-github" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });
    expect(screen.getByText(/未在文章中发现有效的 GitHub 仓库链接/)).toBeInTheDocument();
  });

  it("8. S1 failed-refresh: preserves previous successful article results when subsequent request fails", async () => {
    render(<AddRepositoryPage />);

    // First request: successful article fetch
    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/good-article" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("article-results-section")).toBeInTheDocument();
      expect(screen.getByTestId("article-repo-card-2001")).toBeInTheDocument();
    });

    // Second request: fails with timeout
    mockFetchArticleError = {
      code: "Timeout",
      message: "Request timed out",
    };

    fireEvent.change(input, { target: { value: "https://example.com/slow-article" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });

    // Verify error is shown
    expect(screen.getByText(/抓取文章页面超时/)).toBeInTheDocument();
    // And previous successful results remain visible!
    expect(screen.getByTestId("article-results-section")).toBeInTheDocument();
    expect(screen.getByTestId("article-repo-card-2001")).toBeInTheDocument();
  });

  it("9. S1 generation safety: older delayed response does not overwrite newer direct GitHub response", async () => {
    let delayedResolve: (val: ArticleImportResult) => void;
    const delayedPromise = new Promise<ArticleImportResult>((resolve) => {
      delayedResolve = resolve;
    });

    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "import_repositories_from_article") {
        return delayedPromise;
      }
      if (cmd === "github_get_repository") {
        return Promise.resolve(mockDirectRepo);
      }
      return Promise.resolve({});
    });

    render(<AddRepositoryPage />);

    const input = screen.getByTestId("add-repo-url-input");
    const fetchBtn = screen.getByTestId("add-repo-fetch-btn");

    // 1. Start slow article request A
    fireEvent.change(input, { target: { value: "https://example.com/slow-article" } });
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-loading")).toBeInTheDocument();
    });

    // 2. User changes URL and submits fast direct GitHub request B (e.g. presses Enter in form)
    fireEvent.change(input, { target: { value: "https://github.com/rust-lang/rust" } });
    fireEvent.submit(input.closest("form")!);

    // Request B finishes quickly
    await waitFor(() => {
      expect(screen.getByTestId("repo-preview-container")).toBeInTheDocument();
      expect(screen.getByText("rust")).toBeInTheDocument();
    });

    // 3. Now slow request A finally completes
    delayedResolve!(mockArticleResult);

    // Page must remain on B (direct preview), not overwritten by A
    await waitFor(() => {
      expect(screen.queryByTestId("article-results-section")).not.toBeInTheDocument();
      expect(screen.getByText("rust")).toBeInTheDocument();
    });
  });

  it("10. can favorite individual repository from article results", async () => {
    render(<AddRepositoryPage />);

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/blog/rust-ecosystem" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("article-repo-card-2001")).toBeInTheDocument();
    });

    const card = screen.getByTestId("article-repo-card-2001");
    const favBtn = within(card).getByTestId("favorite-btn");
    expect(favBtn).toHaveTextContent("收藏");

    fireEvent.click(favBtn);

    await waitFor(() => {
      expect(favBtn).toHaveTextContent("已收藏");
      expect(screen.getByTestId("add-repo-action-banner")).toHaveTextContent(
        "成功将项目 tokio 收藏至本地库！"
      );
    });

    expect(favoritedIds).toContain(2001);
  });

  it("11. repository cards in article results display S2 releases and languages metadata", async () => {
    render(<AddRepositoryPage />);

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/blog/rust-ecosystem" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("article-repo-card-2001")).toBeInTheDocument();
    });

    const card1 = screen.getByTestId("article-repo-card-2001");
    // Check S2 Releases count displayed
    expect(within(card1).getByText("Releases 80")).toBeInTheDocument();

    const card2 = screen.getByTestId("article-repo-card-2002");
    expect(within(card2).getByText("Releases 45")).toBeInTheDocument();

    // Check S2 Languages displayed
    expect(within(card1).getByText("Rust")).toBeInTheDocument();
    expect(within(card1).getByText("100.0%")).toBeInTheDocument();
  });
});
