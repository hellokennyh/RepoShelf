import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import { I18nProvider } from "../i18n/I18nContext";

let mockImportResult: unknown = null;
let mockImportError: unknown = null;

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    switch (cmd) {
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: true, provider: "ollama", base_url: "http://localhost:11434", model: "qwen2.5:7b" },
          ui: { locale: "zh-Hans" },
        });
      case "get_ai_search_readiness":
        return Promise.resolve({ ready: true, reason: "" });
      case "check_favorite_identities_batch":
      case "find_favorite_identities":
        return Promise.resolve({ matched_repo_ids: [], matched_urls: [] });
      case "import_repositories_from_article":
        if (mockImportError) {
          return Promise.reject(mockImportError);
        }
        return Promise.resolve(mockImportResult);
      default:
        return Promise.resolve({});
    }
  }),
}));

describe("Stage S4: WeChat Article Enhancement & Challenge Detection", () => {
  beforeEach(() => {
    mockImportResult = null;
    mockImportError = null;
  });

  it("AC069: Displays challengeDetected error message when WeChat security verification is triggered", async () => {
    mockImportError = {
      code: "ChallengeDetected",
      message: "微信公众号安全验证或环境异常，无法自动提取内容",
    };

    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <AddRepositoryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, {
      target: { value: "https://mp.weixin.qq.com/s/mock_verification_id" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText(/文章页面包含安全验证、环境异常提示或需要登录/i)).toBeInTheDocument();
    });
  });

  it("AC067: Displays successfully extracted repositories from valid WeChat article", async () => {
    mockImportResult = {
      source_url: "https://mp.weixin.qq.com/s/valid_article_id",
      extracted_unique_count: 1,
      valid_count: 1,
      invalid_count: 0,
      skipped_count: 0,
      truncated: false,
      warning: null,
      repositories: [
        {
          github_repo_id: 888,
          github_url: "https://github.com/astral-sh/uv",
          name: "uv",
          description: "An extremely fast Python package and project manager.",
          stars: 45000,
          license: "MIT",
          topics: ["python", "rust"],
          pushed_at: "2026-03-01T12:00:00Z",
          category: "Developer Tools",
        },
      ],
    };

    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <AddRepositoryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, {
      target: { value: "https://mp.weixin.qq.com/s/valid_article_id" },
    });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByText("uv")).toBeInTheDocument();
      expect(screen.getByText("An extremely fast Python package and project manager.")).toBeInTheDocument();
    });
  });
});
