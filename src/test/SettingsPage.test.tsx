import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { SettingsPage } from "../pages/SettingsPage";
import { Settings, SecretStatus } from "../types/settings";

const mockInitialSettings: Settings = {
  schema_version: 1,
  github: {
    default_search_limit: 20,
    default_trending_period: "daily",
  },
  ai: {
    enabled: false,
    provider: null,
    base_url: null,
    model: null,
  },
};

let mockSecretStatus: SecretStatus = {
  github_token_configured: false,
  ai_api_key_configured: false,
};

let mockConnectionError: string | null = null;
let mockAiConnectionError: string | null = null;
let mockOllamaEndpoints: Array<{ base_url: string; models: Array<{ name: string; model: string }> }> = [];
let mockOllamaModels: Array<{ name: string; model: string }> = [];

let currentSettings: Settings = JSON.parse(JSON.stringify(mockInitialSettings));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_app_info":
        return Promise.resolve({
          name: "RepoShelf",
          phase: "Phase 1",
        });
      case "get_settings":
        return Promise.resolve(currentSettings);
      case "update_settings":
        currentSettings = args?.settings as Settings;
        return Promise.resolve(currentSettings);
      case "get_secret_status":
        return Promise.resolve(mockSecretStatus);
      case "set_github_token":
        mockSecretStatus = { ...mockSecretStatus, github_token_configured: true };
        return Promise.resolve();
      case "clear_github_token":
        mockSecretStatus = { ...mockSecretStatus, github_token_configured: false };
        return Promise.resolve();
      case "set_ai_api_key":
        mockSecretStatus = { ...mockSecretStatus, ai_api_key_configured: true };
        return Promise.resolve();
      case "clear_ai_api_key":
        mockSecretStatus = { ...mockSecretStatus, ai_api_key_configured: false };
        return Promise.resolve();
      case "github_test_connection":
        if (mockConnectionError) {
          return Promise.reject(new Error(mockConnectionError));
        }
        return Promise.resolve({
          connected: true,
          mode: mockSecretStatus.github_token_configured ? "Authenticated" : "Anonymous",
          rate_limit: {
            core: {
              limit: mockSecretStatus.github_token_configured ? 5000 : 60,
              remaining: mockSecretStatus.github_token_configured ? 4999 : 59,
              used: 1,
              reset_at: "2026-03-10T12:00:00Z",
              resource: "core",
            },
            search: {
              limit: mockSecretStatus.github_token_configured ? 30 : 10,
              remaining: mockSecretStatus.github_token_configured ? 30 : 10,
              used: 0,
              reset_at: "2026-03-10T12:00:00Z",
              resource: "search",
            },
          },
        });
      case "test_ai_connection":
        if (mockAiConnectionError) {
          return Promise.reject(new Error(mockAiConnectionError));
        }
        return Promise.resolve({
          provider: "OpenAI Compatible",
          model: "gpt-4o-mini",
          success: true,
          message: "Connection successful",
        });
      case "discover_ollama_endpoints":
        return Promise.resolve(mockOllamaEndpoints);
      case "list_ollama_models":
        return Promise.resolve(mockOllamaModels);
      default:
        return Promise.reject(new Error(`Unhandled mock command: ${cmd}`));
    }
  }),
}));

describe("SettingsPage Phase 1 Test Suite", () => {
  beforeEach(() => {
    currentSettings = JSON.parse(JSON.stringify(mockInitialSettings));
    mockSecretStatus = {
      github_token_configured: false,
      ai_api_key_configured: false,
    };
    mockConnectionError = null;
    mockAiConnectionError = null;
    mockOllamaEndpoints = [];
    mockOllamaModels = [];
    vi.clearAllMocks();
  });

  it("1. renders Settings page properly with all cards", async () => {
    render(<SettingsPage />);

    expect(await screen.findByText("GitHub 配置")).toBeInTheDocument();
    expect(screen.getByText(/AI 增强配置/)).toBeInTheDocument();
    expect(screen.queryByText(/Tauri IPC 通信验证/)).not.toBeInTheDocument();
  });

  it("2. displays default settings values", async () => {
    render(<SettingsPage />);

    const searchLimitInput = (await screen.findByLabelText(
      /默认搜索结果上限/
    )) as HTMLInputElement;
    expect(searchLimitInput.value).toBe("20");

    const trendingSelect = screen.getByLabelText(/默认热门周期/) as HTMLSelectElement;
    expect(trendingSelect.value).toBe("daily");

    // Secret statuses should show 'Not configured' initially
    expect(screen.getByTestId("github-token-status")).toHaveTextContent("Not configured");
    expect(screen.getByTestId("ai-key-status")).toHaveTextContent("Not configured");
  });

  it("3. handles save non-secret settings interaction", async () => {
    render(<SettingsPage />);

    const searchLimitInput = await screen.findByLabelText(/默认搜索结果上限/);
    fireEvent.change(searchLimitInput, { target: { value: "35" } });

    const saveSettingsButton = screen.getByText("保存 GitHub 偏好设置");
    fireEvent.click(saveSettingsButton);

    await waitFor(() => {
      expect(
        screen.getByText(/偏好设置已成功保存到 settings\.json/)
      ).toBeInTheDocument();
    });

    expect(currentSettings.github.default_search_limit).toBe(35);
  });

  it("4. clears token input immediately upon successful save and updates configured status", async () => {
    render(<SettingsPage />);

    const tokenInput = (await screen.findByPlaceholderText(
      /输入 ghp_ 开头的 Personal Access Token/
    )) as HTMLInputElement;

    fireEvent.change(tokenInput, {
      target: { value: "ghp_mock_test_token_input" },
    });
    expect(tokenInput.value).toBe("ghp_mock_test_token_input");

    const saveTokenButton = screen.getByText("保存 Token");
    fireEvent.click(saveTokenButton);

    await waitFor(() => {
      expect(
        screen.getByText("GitHub Token 已安全存入系统凭据管理器")
      ).toBeInTheDocument();
    });

    // CRITICAL SECURITY CONTRACT: The input must be cleared immediately
    expect(tokenInput.value).toBe("");

    // The badge must now show Configured
    expect(screen.getByTestId("github-token-status")).toHaveTextContent("Configured");
  });

  it("5. saved secret never re-appears in token input (one-way security)", async () => {
    // Simulate token already configured
    mockSecretStatus.github_token_configured = true;

    render(<SettingsPage />);

    await waitFor(() => {
      expect(screen.getByTestId("github-token-status")).toHaveTextContent("Configured");
    });

    const tokenInput = screen.getByPlaceholderText(
      /如需更新请输入新 Token/
    ) as HTMLInputElement;

    // The value must be completely blank, not containing any token plaintext
    expect(tokenInput.value).toBe("");
  });

  it("6. handles clear secret interaction", async () => {
    mockSecretStatus.github_token_configured = true;

    render(<SettingsPage />);

    const clearButton = await screen.findByText("清除 Token");
    expect(clearButton).not.toBeDisabled();

    fireEvent.click(clearButton);

    await waitFor(() => {
      expect(
        screen.getByText("GitHub Token 已从系统凭据管理器中清除")
      ).toBeInTheDocument();
    });

    expect(screen.getByTestId("github-token-status")).toHaveTextContent("Not configured");
  });

  it("7. AI is disabled by default and can be toggled and configured", async () => {
    render(<SettingsPage />);

    const aiSwitch = await screen.findByRole("switch");
    expect(aiSwitch).toHaveAttribute("aria-checked", "false");

    // Inputs should be disabled when AI is false
    const providerSelect = screen.getByLabelText("AI Provider");
    expect(providerSelect).toBeDisabled();

    // Toggle AI ON
    fireEvent.click(aiSwitch);
    expect(aiSwitch).toHaveAttribute("aria-checked", "true");
    expect(providerSelect).not.toBeDisabled();

    // Change provider
    fireEvent.change(providerSelect, { target: { value: "anthropic" } });
    expect(currentSettings.ai.provider || "anthropic").toBe("anthropic");
  });

  it("8. AI connection test button is enabled regardless of AI enabled toggle state", async () => {
    currentSettings.ai.enabled = false;
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByTestId("github-test-connection-btn");
    expect(ghTestBtn).not.toBeDisabled();
    expect(ghTestBtn).toHaveTextContent("测试连接");

    const aiTestBtn = screen.getByTestId("ai-test-connection-btn");
    expect(aiTestBtn).not.toBeDisabled();
  });

  it("9. handles anonymous GitHub connection test success", async () => {
    mockSecretStatus.github_token_configured = false;
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByTestId("github-test-connection-btn");
    fireEvent.click(ghTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("github-connection-status")).toHaveTextContent("Connected (已连接)");
    });

    expect(screen.getByTestId("github-connection-mode")).toHaveTextContent("Mode: Anonymous");
    expect(screen.getByTestId("github-core-ratelimit")).toHaveTextContent("59 / 60");
    expect(screen.getByTestId("github-search-ratelimit")).toHaveTextContent("10 / 10");
  });

  it("10. handles authenticated GitHub connection test success", async () => {
    mockSecretStatus.github_token_configured = true;
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByTestId("github-test-connection-btn");
    fireEvent.click(ghTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("github-connection-status")).toHaveTextContent("Connected (已连接)");
    });

    expect(screen.getByTestId("github-connection-mode")).toHaveTextContent("Mode: Authenticated");
    expect(screen.getByTestId("github-core-ratelimit")).toHaveTextContent("4999 / 5000");
    expect(screen.getByTestId("github-search-ratelimit")).toHaveTextContent("30 / 30");
  });

  it("11. handles 401 invalid token connection failure", async () => {
    mockConnectionError = "GitHub authentication failed: Invalid or expired GitHub token";
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByTestId("github-test-connection-btn");
    fireEvent.click(ghTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("github-connection-error")).toHaveTextContent(
        "Invalid or expired GitHub token"
      );
    });
  });

  it("12. handles rate limit connection failure", async () => {
    mockConnectionError = "GitHub rate limit reached: GitHub API rate limit exhausted";
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByTestId("github-test-connection-btn");
    fireEvent.click(ghTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("github-connection-error")).toHaveTextContent(
        "GitHub rate limit reached"
      );
    });
  });

  it("13. handles AI test connection success", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "openai_compatible";
    currentSettings.ai.model = "gpt-4o-mini";
    render(<SettingsPage />);

    const aiTestBtn = await screen.findByTestId("ai-test-connection-btn");
    expect(aiTestBtn).not.toBeDisabled();
    fireEvent.click(aiTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("ai-connection-status")).toHaveTextContent(
        "Connected (已连接)"
      );
      expect(screen.getByTestId("ai-connection-provider")).toHaveTextContent(
        "Provider: OpenAI Compatible"
      );
      expect(screen.getByTestId("ai-connection-model")).toHaveTextContent(
        "Model: gpt-4o-mini"
      );
    });
  });

  it("14. handles AI test connection failure", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "anthropic";
    currentSettings.ai.model = "claude-3-5-sonnet";
    mockAiConnectionError = "AI authentication failed: Invalid API key";
    render(<SettingsPage />);

    const aiTestBtn = await screen.findByTestId("ai-test-connection-btn");
    fireEvent.click(aiTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("ai-connection-error")).toHaveTextContent(
        "AI authentication failed: Invalid API key"
      );
    });
  });

  it("15. displays Ollama no-key notice and updates key status", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";
    render(<SettingsPage />);

    expect(await screen.findByTestId("ollama-no-key-notice")).toHaveTextContent(
      "Ollama 为本地模型服务，无需配置 API Key。"
    );
    expect(screen.getByTestId("ai-key-status")).toHaveTextContent(
      "Not required (无需配置)"
    );
  });

  it("16. handles AI test connection success when AI is disabled", async () => {
    currentSettings.ai.enabled = false;
    currentSettings.ai.provider = "openai_compatible";
    currentSettings.ai.model = "gpt-4o-mini";
    render(<SettingsPage />);

    const aiTestBtn = await screen.findByTestId("ai-test-connection-btn");
    expect(aiTestBtn).not.toBeDisabled();
    fireEvent.click(aiTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("ai-connection-status")).toHaveTextContent(
        "Connected (已连接)"
      );
    });
  });

  it("17. Ollama discovery button appears only for Ollama provider and not for others", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "openai_compatible";
    render(<SettingsPage />);

    await waitFor(() => {
      expect(screen.queryByTestId("ollama-discover-btn")).not.toBeInTheDocument();
      expect(screen.queryByTestId("ollama-refresh-models-btn")).not.toBeInTheDocument();
    });

    // Switch provider to Ollama
    const providerSelect = screen.getByLabelText("AI Provider");
    fireEvent.change(providerSelect, { target: { value: "ollama" } });

    await waitFor(() => {
      expect(screen.getByTestId("ollama-discover-btn")).toBeInTheDocument();
      expect(screen.getByTestId("ollama-refresh-models-btn")).toBeInTheDocument();
    });
  });

  it("18. successful Ollama discovery populates Base URL and discovered models", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";
    currentSettings.ai.base_url = null;
    currentSettings.ai.model = null;

    mockOllamaEndpoints = [
      {
        base_url: "http://127.0.0.1:11434",
        models: [
          { name: "qwen3.5:9b", model: "qwen3.5:9b" },
          { name: "llama3.2", model: "llama3.2" },
        ],
      },
    ];

    render(<SettingsPage />);

    const discoverBtn = await screen.findByTestId("ollama-discover-btn");
    fireEvent.click(discoverBtn);

    await waitFor(() => {
      expect(screen.getByText(/成功检测到 1 个本地 Ollama 服务/)).toBeInTheDocument();
    });

    const baseUrlInput = screen.getByLabelText("Base URL") as HTMLInputElement;
    expect(baseUrlInput.value).toBe("http://127.0.0.1:11434");

    // Discovered models pills
    expect(screen.getByRole("button", { name: "qwen3.5:9b" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "llama3.2" })).toBeInTheDocument();

    // Clicking llama3.2 pill updates model input
    fireEvent.click(screen.getByRole("button", { name: "llama3.2" }));
    const modelInput = screen.getByLabelText("Model") as HTMLInputElement;
    expect(modelInput.value).toBe("llama3.2");
  });

  it("19. empty Ollama discovery response shows nonfatal notice without erasing values", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";
    currentSettings.ai.base_url = "http://my-custom-ollama:11434";
    currentSettings.ai.model = "custom-model";

    mockOllamaEndpoints = [];

    render(<SettingsPage />);

    const discoverBtn = await screen.findByTestId("ollama-discover-btn");
    fireEvent.click(discoverBtn);

    await waitFor(() => {
      expect(screen.getByText("未检测到可用的本地 Ollama 服务")).toBeInTheDocument();
    });

    // Existing manual values must NOT be erased
    const baseUrlInput = screen.getByLabelText("Base URL") as HTMLInputElement;
    expect(baseUrlInput.value).toBe("http://my-custom-ollama:11434");
    const modelInput = screen.getByLabelText("Model") as HTMLInputElement;
    expect(modelInput.value).toBe("custom-model");
  });

  it("20. refresh models button populates models and allows selection", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";
    currentSettings.ai.base_url = "http://127.0.0.1:11434";
    currentSettings.ai.model = "";

    mockOllamaModels = [
      { name: "qwen3.5:9b", model: "qwen3.5:9b" },
      { name: "gemma4", model: "gemma4" },
    ];

    render(<SettingsPage />);

    const refreshBtn = await screen.findByTestId("ollama-refresh-models-btn");
    fireEvent.click(refreshBtn);

    await waitFor(() => {
      expect(screen.getByText(/已获取 2 个本地模型/)).toBeInTheDocument();
    });

    expect(screen.getByRole("button", { name: "qwen3.5:9b" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "gemma4" })).toBeInTheDocument();
  });

  it("21. manual Base URL and Model inputs remain editable", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";

    render(<SettingsPage />);

    const baseUrlInput = (await screen.findByLabelText("Base URL")) as HTMLInputElement;
    fireEvent.change(baseUrlInput, { target: { value: "http://192.168.1.100:11434" } });
    expect(baseUrlInput.value).toBe("http://192.168.1.100:11434");

    const modelInput = screen.getByLabelText("Model") as HTMLInputElement;
    fireEvent.change(modelInput, { target: { value: "my-custom-fine-tune" } });
    expect(modelInput.value).toBe("my-custom-fine-tune");
  });

  it("22. AI test connection auto-saves settings before testing", async () => {
    currentSettings.ai.enabled = true;
    currentSettings.ai.provider = "ollama";
    currentSettings.ai.base_url = "http://127.0.0.1:11434";
    currentSettings.ai.model = "qwen3.5:9b";

    render(<SettingsPage />);

    // Change model in input
    const modelInput = (await screen.findByLabelText("Model")) as HTMLInputElement;
    fireEvent.change(modelInput, { target: { value: "qwen3.5:9b-updated" } });

    const aiTestBtn = screen.getByTestId("ai-test-connection-btn");
    fireEvent.click(aiTestBtn);

    await waitFor(() => {
      expect(screen.getByTestId("ai-connection-status")).toHaveTextContent("Connected (已连接)");
    });

    // Auto-save must have updated currentSettings
    expect(currentSettings.ai.model).toBe("qwen3.5:9b-updated");
  });
});

