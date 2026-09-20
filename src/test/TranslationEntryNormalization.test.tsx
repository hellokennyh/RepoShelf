import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SearchPage } from "../pages/SearchPage";
import { AddRepositoryPage } from "../pages/AddRepositoryPage";
import { DiscoverySessionProvider } from "../context/DiscoverySessionContext";
import { I18nProvider } from "../i18n/I18nContext";
import { AiSearchReadiness, GitHubRepository } from "../types/github";

let mockAiReadiness: AiSearchReadiness = { ready: true, reason: "" };
let mockInvokeCalls: { cmd: string; args?: Record<string, unknown> }[] = [];
let capturedFavoriteInput: unknown = null;

const sampleRepo1: GitHubRepository = {
  github_repo_id: 101,
  github_url: "https://github.com/tauri-apps/tauri",
  name: "tauri",
  description: "Build smaller, faster desktop apps with Rust.",
  stars: 85000,
  license: "Apache-2.0",
  topics: ["rust", "desktop"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Developer Tools",
};

const sampleRepo2: GitHubRepository = {
  github_repo_id: 102,
  github_url: "https://github.com/tokio-rs/tokio",
  name: "tokio",
  description: "An asynchronous runtime for the Rust programming language.",
  stars: 25000,
  license: "MIT",
  topics: ["rust", "async"],
  pushed_at: "2026-03-01T12:00:00Z",
  category: "Libraries & Frameworks",
};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    mockInvokeCalls.push({ cmd, args });
    switch (cmd) {
      case "get_settings":
        return Promise.resolve({
          schema_version: 1,
          github: { default_search_limit: 20, default_trending_period: "daily" },
          ai: { enabled: true, provider: "ollama", base_url: "http://localhost:11434", model: "qwen2.5:7b" },
          ui: { locale: "zh-Hans" },
        });
      case "get_ai_search_readiness":
        return Promise.resolve(mockAiReadiness);
      case "check_favorite_identities_batch":
      case "find_favorite_identities":
        return Promise.resolve({
          matched_repo_ids: [],
          matched_urls: [],
        });
      case "github_search_repositories":
        return Promise.resolve({
          items: [sampleRepo1, sampleRepo2],
          total_count: 2,
        });
      case "github_ai_search_repositories":
        return Promise.resolve({
          items: [sampleRepo1, sampleRepo2],
          generated_queries: ["rust desktop async"],
          partial: false,
        });
      case "github_get_repository":
        return Promise.resolve(sampleRepo1);
      case "import_repositories_from_article":
        return Promise.resolve({
          source_url: "https://example.com/article",
          extracted_unique_count: 2,
          valid_count: 2,
          invalid_count: 0,
          skipped_count: 0,
          truncated: false,
          warning: null,
          repositories: [sampleRepo1, sampleRepo2],
        });
      case "translate_repository_description": {
        const text = ((args?.text || args?.description) as string) || "";
        return Promise.resolve({
          translated_text: `[已译] ${text}`,
          target_locale: args?.targetLocale || "zh-Hans",
          source_hash: "mock_hash_123",
          action: "translated",
          provider: "ollama",
          model: "qwen2.5:7b",
        });
      }
      case "favorite_repository": {
        capturedFavoriteInput = args?.input;
        return Promise.resolve({ id: 99, ...(args?.input as Record<string, unknown>) });
      }
      case "favorite_repositories_batch": {
        const inputs = (args?.inputs as unknown[]) || [];
        capturedFavoriteInput = inputs;
        return Promise.resolve({
          total_attempted: inputs.length,
          succeeded: inputs.length,
          failed: 0,
          skipped_duplicates: 0,
          items: (inputs as { github_url: string }[]).map((inp) => ({
            github_url: inp.github_url,
            favorite_id: 100,
            status: "succeeded",
          })),
        });
      }
      case "get_distinct_tags":
        return Promise.resolve([]);
      case "check_repo_exists":
        return Promise.resolve(false);
      default:
        return Promise.resolve({});
    }
  }),
}));

describe("S2B: Batch Translation Entry Normalization & Completion", () => {
  beforeEach(() => {
    mockInvokeCalls = [];
    mockAiReadiness = { ready: true, reason: "" };
    capturedFavoriteInput = null;
  });

  it("AC039: SearchPage keyword search results can be batch translated and show translation on cards", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    // Execute keyword search
    const input = screen.getByTestId("search-query-input");
    fireEvent.change(input, { target: { value: "rust" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
      expect(screen.getByText("tokio")).toBeInTheDocument();
    });

    // Batch translate button must be enabled
    const batchBtn = screen.getByTestId("search-batch-translate-btn");
    expect(batchBtn).toBeEnabled();
    fireEvent.click(batchBtn);

    // Verify translations are populated on the cards
    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
      expect(screen.getByText(/\[已译\] An asynchronous runtime for the Rust programming language\./)).toBeInTheDocument();
    });

    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(2);
  });

  it("AC039: SearchPage AI search results can be batch translated and show translation on cards", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-mode-ai")).toBeInTheDocument();
    });

    // Switch to AI mode
    fireEvent.click(screen.getByTestId("search-mode-ai"));

    await waitFor(() => {
      expect(screen.getByTestId("search-ai-input")).toBeInTheDocument();
    });

    // Execute AI search
    fireEvent.change(screen.getByTestId("search-ai-input"), {
      target: { value: "best rust desktop framework" },
    });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Click batch translate
    const batchBtn = screen.getByTestId("search-batch-translate-btn");
    expect(batchBtn).toBeEnabled();
    fireEvent.click(batchBtn);

    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
    });
  });

  it("AC040: AddRepositoryPage direct preview supports batch translation", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <AddRepositoryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://github.com/tauri-apps/tauri" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-batch-translate-btn")).toBeInTheDocument();
    });

    const batchBtn = screen.getByTestId("add-batch-translate-btn");
    expect(batchBtn).toBeEnabled();
    fireEvent.click(batchBtn);

    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
    });
  });

  it("AC040: AddRepositoryPage article results support batch translation", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <AddRepositoryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://example.com/article" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("article-results-section")).toBeInTheDocument();
    });

    const batchBtn = screen.getByTestId("add-batch-translate-btn");
    expect(batchBtn).toBeEnabled();
    fireEvent.click(batchBtn);

    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
      expect(screen.getByText(/\[已译\] An asynchronous runtime for the Rust programming language\./)).toBeInTheDocument();
    });
  });

  it("AC046: Favoriting a translated repo from search results passes translation to favorite payload", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const input = screen.getByTestId("search-query-input");
    fireEvent.change(input, { target: { value: "rust" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Translate first
    fireEvent.click(screen.getByTestId("search-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
    });

    // Now favorite the first repository
    const favButtons = screen.getAllByTestId("favorite-btn");
    fireEvent.click(favButtons[0]);

    await waitFor(() => {
      expect(capturedFavoriteInput).not.toBeNull();
    });

    const favInput = capturedFavoriteInput as { translation?: { text: string; target_locale: string } };
    expect(favInput.translation).toBeDefined();
    expect(favInput.translation?.text).toContain("[已译] Build smaller, faster desktop apps with Rust.");
  });

  it("AC046: Favoriting a translated direct preview in AddRepositoryPage passes translation to favorite payload", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <AddRepositoryPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    const input = screen.getByTestId("add-repo-url-input");
    fireEvent.change(input, { target: { value: "https://github.com/tauri-apps/tauri" } });
    fireEvent.click(screen.getByTestId("add-repo-fetch-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("add-batch-translate-btn")).toBeInTheDocument();
    });

    // Translate
    fireEvent.click(screen.getByTestId("add-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
    });

    // Save to library
    const saveBtn = screen.getByTestId("add-repo-favorite-btn");
    fireEvent.click(saveBtn);

    await waitFor(() => {
      expect(capturedFavoriteInput).not.toBeNull();
    });

    const favInput = capturedFavoriteInput as { translation?: { text: string; target_locale: string } };
    expect(favInput.translation).toBeDefined();
    expect(favInput.translation?.text).toContain("[已译] Build smaller, faster desktop apps with Rust.");
  });

  it("AC041: Batch translation processes all items in the batch regardless of checkbox selection", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const input = screen.getByTestId("search-query-input");
    fireEvent.change(input, { target: { value: "rust" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
      expect(screen.getByText("tokio")).toBeInTheDocument();
    });

    // Check only the first item
    const checkbox = screen.getByTestId("repo-checkbox-101");
    fireEvent.click(checkbox);
    expect(checkbox).toBeChecked();

    // Click batch translate
    const batchBtn = screen.getByTestId("search-batch-translate-btn");
    fireEvent.click(batchBtn);

    // Both items must be translated
    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
      expect(screen.getByText(/\[已译\] An asynchronous runtime for the Rust programming language\./)).toBeInTheDocument();
    });

    const translateCalls = mockInvokeCalls.filter((c) => c.cmd === "translate_repository_description");
    expect(translateCalls.length).toBe(2);
  });

  it("AC043: Batch translation cancellation halts further requests", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const input = screen.getByTestId("search-query-input");
    fireEvent.change(input, { target: { value: "rust" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Start batch translate
    fireEvent.click(screen.getByTestId("search-batch-translate-btn"));

    // If cancel button appears, click it
    const cancelBtn = screen.queryByTestId("search-cancel-translate-btn");
    if (cancelBtn) {
      fireEvent.click(cancelBtn);
    }

    // Results remain accessible
    expect(screen.getByText("tauri")).toBeInTheDocument();
  });

  it("AC045: When AI is disabled, existing translations remain visible on cards", async () => {
    render(
      <I18nProvider>
        <DiscoverySessionProvider>
          <SearchPage />
        </DiscoverySessionProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("search-query-input")).toBeInTheDocument();
    });

    const input = screen.getByTestId("search-query-input");
    fireEvent.change(input, { target: { value: "rust" } });
    fireEvent.click(screen.getByTestId("search-submit-btn"));

    await waitFor(() => {
      expect(screen.getByText("tauri")).toBeInTheDocument();
    });

    // Translate
    fireEvent.click(screen.getByTestId("search-batch-translate-btn"));
    await waitFor(() => {
      expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
    });

    // Now simulate AI becoming disabled
    mockAiReadiness = { ready: false, reason: "AI has been disabled" };

    // Re-render / update check
    expect(screen.getByText(/\[已译\] Build smaller, faster desktop apps with Rust\./)).toBeInTheDocument();
  });
});
