import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { GitHubRepository } from "../types/github";

const mockPreviewRepo: GitHubRepository = {
  github_repo_id: 2001,
  github_url: "https://github.com/octocat/Hello-World",
  name: "Hello-World",
  description: "My first repository on GitHub!",
  stars: 2500,
  license: "MIT",
  topics: ["tutorial", "octocat"],
  pushed_at: "2026-01-15T08:00:00Z",
  category: "Developer Tools",
};

let mockFetchError: { code: string; message: string } | null = null;
let lastFetchedUrl: string | null = null;
let openedUrl: string | null = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "github_get_repository":
        lastFetchedUrl = (args?.url as string) || null;
        if (mockFetchError) {
          return Promise.reject(mockFetchError);
        }
        return Promise.resolve(mockPreviewRepo);
      case "open_github_url":
        openedUrl = (args?.url as string) || null;
        return Promise.resolve();
      default:
        return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
    }
  }),
}));

describe("AddRepositoryPage Phase 2 Test Suite", () => {
  beforeEach(() => {
    mockFetchError = null;
    lastFetchedUrl = null;
    openedUrl = null;
    vi.clearAllMocks();
  });

  it("1. renders AddRepositoryPage properly with form and idle state", () => {
    render(<AddRepositoryPage />);
    expect(screen.getByText("Add GitHub Repository")).toBeInTheDocument();
    expect(screen.getByTestId("add-repo-url-input")).toBeInTheDocument();
    expect(screen.getByTestId("add-repo-fetch-btn")).toBeInTheDocument();
    expect(screen.getByText("等待输入项目地址")).toBeInTheDocument();
  });

  it("2. handles valid submit and displays repository preview", async () => {
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    const fetchBtn = screen.getByTestId("add-repo-fetch-btn");

    fireEvent.change(urlInput, {
      target: { value: "https://github.com/octocat/Hello-World" },
    });
    fireEvent.click(fetchBtn);

    await waitFor(() => {
      expect(screen.getByTestId("repo-preview-container")).toBeInTheDocument();
    });

    expect(screen.getByText("Hello-World")).toBeInTheDocument();
    expect(screen.getByText("Developer Tools")).toBeInTheDocument();
    expect(screen.getByText("My first repository on GitHub!")).toBeInTheDocument();
    expect(screen.getByText("★")).toBeInTheDocument();
    expect(screen.getByText("2,500")).toBeInTheDocument();
    expect(lastFetchedUrl).toBe("https://github.com/octocat/Hello-World");
  });

  it("3. handles invalid repository URL error", async () => {
    mockFetchError = {
      code: "InvalidRepositoryUrl",
      message: "URL must contain both owner and repository name",
    };
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, { target: { value: "https://github.com/owner" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });

    expect(screen.getByText(/URL 格式无效/)).toBeInTheDocument();
  });

  it("4. handles repository not found 404 error", async () => {
    mockFetchError = {
      code: "GitHubNotFound",
      message: "Repository not found on GitHub",
    };
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, {
      target: { value: "https://github.com/notfound/repo" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });

    expect(screen.getByText(/未找到该项目/)).toBeInTheDocument();
  });

  it("5. handles private repository unsupported rejection", async () => {
    mockFetchError = {
      code: "PrivateRepositoryUnsupported",
      message: "Private repositories are unsupported in RepoShelf V1",
    };
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, {
      target: { value: "https://github.com/company/private-repo" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });

    expect(
      screen.getByText(/RepoShelf V1 仅支持公开开源项目/)
    ).toBeInTheDocument();
  });

  it("6. handles rate limit error", async () => {
    mockFetchError = {
      code: "GitHubRateLimit",
      message: "Rate limit exhausted",
    };
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, {
      target: { value: "https://github.com/octocat/Hello-World" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-repo-error")).toBeInTheDocument();
    });

    expect(screen.getByText(/GitHub API 请求频率已达上限/)).toBeInTheDocument();
  });

  it("7. clicking external link in preview opens browser via open_github_url IPC", async () => {
    render(<AddRepositoryPage />);

    const urlInput = screen.getByTestId("add-repo-url-input");
    fireEvent.change(urlInput, {
      target: { value: "https://github.com/octocat/Hello-World" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("repo-preview-container")).toBeInTheDocument();
    });

    const openLink = screen.getByTestId("open-github-link");
    fireEvent.click(openLink);

    await waitFor(() => {
      expect(openedUrl).toBe("https://github.com/octocat/Hello-World");
    });
  });
});
