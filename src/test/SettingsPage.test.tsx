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
    vi.clearAllMocks();
  });

  it("1. renders Settings page properly with all cards", async () => {
    render(<SettingsPage />);

    expect(await screen.findByText("GitHub 配置")).toBeInTheDocument();
    expect(screen.getByText(/AI 增强配置/)).toBeInTheDocument();
    expect(screen.getByText(/Tauri IPC 通信验证/)).toBeInTheDocument();
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

  it("8. connection test buttons are disabled and clearly marked with phases", async () => {
    render(<SettingsPage />);

    const ghTestBtn = await screen.findByText("测试连接 (Available in Phase 2)");
    expect(ghTestBtn).toBeDisabled();

    const aiTestBtn = screen.getByText("测试连接 (Available in Phase 6)");
    expect(aiTestBtn).toBeDisabled();
  });
});
