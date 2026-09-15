import React, { useEffect, useState } from "react";
import {
  clearAiApiKey,
  clearGitHubToken,
  discoverOllamaEndpoints,
  fetchSecretStatus,
  fetchSettings,
  listOllamaModels,
  saveAiApiKey,
  saveGitHubToken,
  saveSettings,
  testAiConnection,
  testGitHubConnection,
} from "../services/tauri";
import { GitHubConnectionStatus } from "../types/github";
import {
  AiConnectionResult,
  AiProvider,
  OllamaModelInfo,
  SecretStatus,
  Settings,
  TrendingPeriod,
  UiLocale,
} from "../types/settings";
import { useI18n } from "../i18n/I18nContext";

const DEFAULT_SETTINGS: Settings = {
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

export const SettingsPage: React.FC = () => {
  const { locale, setLocale } = useI18n();
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [savingLanguage, setSavingLanguage] = useState(false);
  const [secretStatus, setSecretStatus] = useState<SecretStatus>({
    github_token_configured: false,
    ai_api_key_configured: false,
  });

  const [savingSettings, setSavingSettings] = useState(false);
  const [savingToken, setSavingToken] = useState(false);
  const [savingApiKey, setSavingApiKey] = useState(false);
  const [testingConnection, setTestingConnection] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<GitHubConnectionStatus | null>(null);
  const [connectionError, setConnectionError] = useState<string | null>(null);
  const [testingAiConnection, setTestingAiConnection] = useState(false);
  const [aiConnectionStatus, setAiConnectionStatus] = useState<AiConnectionResult | null>(null);
  const [aiConnectionError, setAiConnectionError] = useState<string | null>(null);

  // Ollama discovery & models state
  const [discoveringOllama, setDiscoveringOllama] = useState(false);
  const [detectedEndpoints, setDetectedEndpoints] = useState<string[]>([]);
  const [ollamaDiscoveryMessage, setOllamaDiscoveryMessage] = useState<string | null>(null);

  const [fetchingModels, setFetchingModels] = useState(false);
  const [discoveredModels, setDiscoveredModels] = useState<OllamaModelInfo[]>([]);
  const [modelFetchMessage, setModelFetchMessage] = useState<string | null>(null);

  // Transient secret input state (NEVER populated from backend, cleared on save)
  const [githubTokenInput, setGithubTokenInput] = useState("");
  const [aiApiKeyInput, setAiApiKeyInput] = useState("");

  // Feedback notifications
  const [feedback, setFeedback] = useState<{
    type: "success" | "error";
    message: string;
  } | null>(null);

  const loadAll = async () => {
    try {
      const [fetchedSettings, status] = await Promise.all([
        fetchSettings().catch(() => DEFAULT_SETTINGS),
        fetchSecretStatus().catch(() => ({
          github_token_configured: false,
          ai_api_key_configured: false,
        })),
      ]);
      if (fetchedSettings) setSettings(fetchedSettings);
      if (status) setSecretStatus(status);
    } catch (err) {
      setFeedback({
        type: "error",
        message: `加载配置失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    }
  };

  useEffect(() => {
    loadAll();
  }, []);

  const handleSaveSettings = async () => {
    try {
      setSavingSettings(true);
      setFeedback(null);
      const updated = await saveSettings(settings);
      setSettings(updated);
      setFeedback({
        type: "success",
        message: "偏好设置已成功保存到 settings.json",
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: `保存设置失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingSettings(false);
    }
  };

  const handleUpdateLanguage = async (newLocale: UiLocale) => {
    try {
      setSavingLanguage(true);
      setFeedback(null);
      const success = await setLocale(newLocale);
      if (success) {
        setSettings((prev) => ({ ...prev, ui_language: newLocale }));
        setFeedback({
          type: "success",
          message: "语言设置已成功保存",
        });
      } else {
        setFeedback({
          type: "error",
          message: "语言设置保存失败",
        });
      }
    } catch (err) {
      setFeedback({
        type: "error",
        message: `更新语言失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingLanguage(false);
    }
  };

  const handleSaveGitHubToken = async () => {
    if (!githubTokenInput.trim()) {
      setFeedback({
        type: "error",
        message: "GitHub Token 不能为空",
      });
      return;
    }

    try {
      setSavingToken(true);
      setFeedback(null);
      await saveGitHubToken(githubTokenInput);
      setGithubTokenInput(""); // Immediately clear plaintext from state
      const status = await fetchSecretStatus();
      setSecretStatus(status);
      setFeedback({
        type: "success",
        message: "GitHub Token 已安全存入系统凭据管理器",
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: `保存 GitHub Token 失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingToken(false);
    }
  };

  const handleClearGitHubToken = async () => {
    try {
      setSavingToken(true);
      setFeedback(null);
      await clearGitHubToken();
      setGithubTokenInput("");
      const status = await fetchSecretStatus();
      setSecretStatus(status);
      setFeedback({
        type: "success",
        message: "GitHub Token 已从系统凭据管理器中清除",
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: `清除 GitHub Token 失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingToken(false);
    }
  };

  const handleTestConnection = async () => {
    try {
      setTestingConnection(true);
      setConnectionError(null);
      const status = await testGitHubConnection();
      setConnectionStatus(status);
    } catch (err: unknown) {
      const msg =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      setConnectionError(msg);
      setConnectionStatus(null);
    } finally {
      setTestingConnection(false);
    }
  };

  const handleSaveAiApiKey = async () => {
    if (!aiApiKeyInput.trim()) {
      setFeedback({
        type: "error",
        message: "AI API Key 不能为空",
      });
      return;
    }

    try {
      setSavingApiKey(true);
      setFeedback(null);
      await saveAiApiKey(aiApiKeyInput);
      setAiApiKeyInput(""); // Immediately clear plaintext from state
      const status = await fetchSecretStatus();
      setSecretStatus(status);
      setFeedback({
        type: "success",
        message: "AI API Key 已安全存入系统凭据管理器",
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: `保存 AI API Key 失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingApiKey(false);
    }
  };

  const handleClearAiApiKey = async () => {
    try {
      setSavingApiKey(true);
      setFeedback(null);
      await clearAiApiKey();
      setAiApiKeyInput("");
      const status = await fetchSecretStatus();
      setSecretStatus(status);
      setFeedback({
        type: "success",
        message: "AI API Key 已从系统凭据管理器中清除",
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: `清除 AI API Key 失败: ${err instanceof Error ? err.message : String(err)}`,
      });
    } finally {
      setSavingApiKey(false);
    }
  };

  const handleDiscoverOllama = async () => {
    try {
      setDiscoveringOllama(true);
      setOllamaDiscoveryMessage(null);
      const results = await discoverOllamaEndpoints();
      if (results.length === 0) {
        setOllamaDiscoveryMessage("未检测到可用的本地 Ollama 服务");
        setDetectedEndpoints([]);
      } else {
        const endpoints = results.map((r) => r.base_url);
        setDetectedEndpoints(endpoints);
        // Automatically populate base_url if empty or if containing legacy /v1 path
        if (!settings.ai.base_url || settings.ai.base_url.includes("/v1")) {
          setSettings((prev) => ({
            ...prev,
            ai: {
              ...prev.ai,
              base_url: endpoints[0],
            },
          }));
        }
        // Extract and deduplicate installed models across detected endpoints
        const allModels = results.flatMap((r) => r.models);
        const uniqueModels: OllamaModelInfo[] = [];
        const seen = new Set<string>();
        for (const m of allModels) {
          if (!seen.has(m.model)) {
            seen.add(m.model);
            uniqueModels.push(m);
          }
        }
        setDiscoveredModels(uniqueModels);
        if (uniqueModels.length > 0 && !settings.ai.model) {
          setSettings((prev) => ({
            ...prev,
            ai: {
              ...prev.ai,
              model: uniqueModels[0].model,
            },
          }));
        }
        setOllamaDiscoveryMessage(`成功检测到 ${endpoints.length} 个本地 Ollama 服务`);
      }
    } catch (err) {
      setOllamaDiscoveryMessage(
        `检测失败: ${err instanceof Error ? err.message : String(err)}`
      );
    } finally {
      setDiscoveringOllama(false);
    }
  };

  const handleFetchOllamaModels = async () => {
    try {
      setFetchingModels(true);
      setModelFetchMessage(null);
      const models = await listOllamaModels(settings.ai.base_url || undefined);
      setDiscoveredModels(models);
      if (models.length === 0) {
        setModelFetchMessage("Ollama 未安装任何模型");
      } else {
        setModelFetchMessage(`已获取 ${models.length} 个本地模型`);
        if (!settings.ai.model) {
          setSettings((prev) => ({
            ...prev,
            ai: {
              ...prev.ai,
              model: models[0].model,
            },
          }));
        }
      }
    } catch (err) {
      setModelFetchMessage(
        `获取模型失败: ${err instanceof Error ? err.message : String(err)}`
      );
    } finally {
      setFetchingModels(false);
    }
  };

  const handleTestAiConnection = async () => {
    try {
      setTestingAiConnection(true);
      setAiConnectionError(null);
      setAiConnectionStatus(null);
      // Auto-save settings first so that testing tests the current form inputs!
      const saved = await saveSettings(settings);
      setSettings(saved);
      const result = await testAiConnection();
      setAiConnectionStatus(result);
    } catch (err: unknown) {
      const msg =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      setAiConnectionError(msg);
      setAiConnectionStatus(null);
    } finally {
      setTestingAiConnection(false);
    }
  };

  return (
    <div className="space-y-6 max-w-4xl pb-12">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>⚙️</span> 设置
          <span className="text-sm font-normal text-slate-400 ml-1">Settings & Preferences</span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          管理 GitHub Token、可选 AI Provider 与应用偏好。凭据通过系统安全凭据管理器安全存储。
        </p>
      </div>

      {/* Global Feedback Banner */}
      {feedback && (
        <div
          role="alert"
          className={`p-4 rounded-lg flex items-center justify-between text-sm ${
            feedback.type === "success"
              ? "bg-emerald-950/70 border border-emerald-700 text-emerald-200"
              : "bg-rose-950/70 border border-rose-700 text-rose-200"
          }`}
        >
          <span>{feedback.message}</span>
          <button
            onClick={() => setFeedback(null)}
            className="text-xs opacity-70 hover:opacity-100 underline ml-4"
          >
            关闭
          </button>
        </div>
      )}

      {/* 0. Language / Interface Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-4">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
            <span>🌐</span> 界面语言 (UI Language)
          </h2>
          <span className="text-xs text-slate-400">
            持久化保存于 settings.json，立即生效无需重启
          </span>
        </div>

        <div className="max-w-xs">
          <label
            htmlFor="ui-language-select"
            className="block text-xs font-medium text-slate-300 mb-1.5"
          >
            显示语言 / Display Language
          </label>
          <select
            id="ui-language-select"
            data-testid="ui-language-select"
            disabled={savingLanguage}
            value={locale}
            onChange={(e) => handleUpdateLanguage(e.target.value as UiLocale)}
            className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
          >
            <option value="en">English</option>
            <option value="zh-Hans">简体中文</option>
            <option value="zh-Hant">繁體中文</option>
          </select>
          <p className="text-[11px] text-slate-400 mt-1.5">
            修改语言将直接更新应用界面，且不会重置正在进行的搜索、热门列表或详情草稿。
          </p>
        </div>
      </div>

      {/* 1. GitHub Configuration Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
            <span>🐙</span> GitHub 配置
          </h2>
          <span className="text-xs text-slate-400">
            非敏感参数存入 settings.json，Token 存入系统凭据管理器
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Default Search Limit */}
          <div>
            <label
              htmlFor="github-search-limit"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              默认搜索结果上限 (1 - 100)
            </label>
            <input
              id="github-search-limit"
              type="number"
              min={1}
              max={100}
              value={settings.github.default_search_limit}
              onChange={(e) => {
                const val = parseInt(e.target.value, 10) || 1;
                setSettings({
                  ...settings,
                  github: {
                    ...settings.github,
                    default_search_limit: Math.max(1, Math.min(100, val)),
                  },
                });
              }}
              className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            />
          </div>

          {/* Default Trending Period */}
          <div>
            <label
              htmlFor="github-trending-period"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              默认热门周期
            </label>
            <select
              id="github-trending-period"
              value={settings.github.default_trending_period}
              onChange={(e) => {
                setSettings({
                  ...settings,
                  github: {
                    ...settings.github,
                    default_trending_period: e.target.value as TrendingPeriod,
                  },
                });
              }}
              className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            >
              <option value="daily">Today (Daily)</option>
              <option value="weekly">This Week (Weekly)</option>
              <option value="monthly">This Month (Monthly)</option>
            </select>
          </div>
        </div>

        {/* GitHub PAT Input and Status */}
        <div className="pt-2 border-t border-slate-700/50 space-y-3">
          <div className="flex items-center justify-between">
            <label
              htmlFor="github-pat-input"
              className="block text-xs font-medium text-slate-300"
            >
              GitHub Personal Access Token
            </label>
            <span
              data-testid="github-token-status"
              className={`text-xs px-2.5 py-0.5 rounded-full font-medium ${
                secretStatus.github_token_configured
                  ? "bg-emerald-900/60 text-emerald-300 border border-emerald-700"
                  : "bg-slate-700/60 text-slate-400 border border-slate-600"
              }`}
            >
              {secretStatus.github_token_configured
                ? "Configured (已配置)"
                : "Not configured (未配置)"}
            </span>
          </div>

          <div className="flex gap-2">
            <input
              id="github-pat-input"
              type="password"
              placeholder={
                secretStatus.github_token_configured
                  ? "•••••••••••••••• (如需更新请输入新 Token)"
                  : "输入 ghp_ 开头的 Personal Access Token"
              }
              value={githubTokenInput}
              onChange={(e) => setGithubTokenInput(e.target.value)}
              className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
            />
            <button
              onClick={handleSaveGitHubToken}
              disabled={savingToken || !githubTokenInput.trim()}
              className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium rounded-md transition-colors"
            >
              {savingToken ? "保存中..." : "保存 Token"}
            </button>
            <button
              onClick={handleClearGitHubToken}
              disabled={savingToken || !secretStatus.github_token_configured}
              className="px-3 py-2 bg-slate-700 hover:bg-rose-700 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium rounded-md transition-colors"
            >
              清除 Token
            </button>
          </div>
          {!secretStatus.github_token_configured && (
            <p className="text-xs text-amber-400/90 bg-amber-950/20 border border-amber-800/30 rounded p-2">
              未配置 GitHub Token。GitHub API 匿名访问限额较低 (60次/小时)，建议配置 Token。
            </p>
          )}
          <p className="text-xs text-slate-500">
            Token 仅保存在本机操作系统安全凭据管理器中，绝不写入普通文件或发送到未授权网络。
          </p>
        </div>

        {/* Connection test status panel */}
        {(connectionStatus || connectionError) && (
          <div
            data-testid="github-connection-result"
            className={`p-3 rounded-md border text-xs space-y-2 ${
              connectionStatus
                ? "bg-slate-900/80 border-emerald-700/60 text-slate-200"
                : "bg-rose-950/40 border-rose-800/60 text-rose-300"
            }`}
          >
            {connectionStatus && (
              <>
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span
                      data-testid="github-connection-status"
                      className="font-semibold text-emerald-400"
                    >
                      Connected (已连接)
                    </span>
                    <span
                      data-testid="github-connection-mode"
                      className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 text-[11px]"
                    >
                      Mode: {connectionStatus.mode}
                    </span>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3 pt-1 border-t border-slate-700/50">
                  <div>
                    <span className="text-slate-400 block">Core Rate Limit:</span>
                    <span
                      data-testid="github-core-ratelimit"
                      className="font-mono text-slate-100"
                    >
                      {connectionStatus.rate_limit.core.remaining} /{" "}
                      {connectionStatus.rate_limit.core.limit}
                    </span>
                    {connectionStatus.rate_limit.core.reset_at && (
                      <span className="text-[10px] text-slate-500 block truncate">
                        Reset: {connectionStatus.rate_limit.core.reset_at}
                      </span>
                    )}
                  </div>
                  <div>
                    <span className="text-slate-400 block">Search Rate Limit:</span>
                    <span
                      data-testid="github-search-ratelimit"
                      className="font-mono text-slate-100"
                    >
                      {connectionStatus.rate_limit.search.remaining} /{" "}
                      {connectionStatus.rate_limit.search.limit}
                    </span>
                    {connectionStatus.rate_limit.search.reset_at && (
                      <span className="text-[10px] text-slate-500 block truncate">
                        Reset: {connectionStatus.rate_limit.search.reset_at}
                      </span>
                    )}
                  </div>
                </div>
              </>
            )}

            {connectionError && (
              <div>
                <span className="font-semibold text-rose-400">✗ 连接失败:</span>{" "}
                <span data-testid="github-connection-error">{connectionError}</span>
              </div>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div className="pt-2 flex items-center justify-between">
          <button
            onClick={handleSaveSettings}
            disabled={savingSettings}
            className="px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:opacity-50 text-slate-100 text-xs font-medium rounded-md transition-colors"
          >
            {savingSettings ? "正在保存偏好..." : "保存 GitHub 偏好设置"}
          </button>

          <button
            data-testid="github-test-connection-btn"
            onClick={handleTestConnection}
            disabled={testingConnection}
            className="px-3 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white border border-indigo-500/50 rounded-md text-xs font-medium transition-colors"
          >
            {testingConnection ? "正在测试连接..." : "测试连接"}
          </button>
        </div>
      </div>

      {/* 2. AI Configuration Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
              <span>🤖</span> AI 增强配置 (可选)
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              AI 是可选增强能力，完全不配置 AI 仍可使用所有基础功能
            </p>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-xs text-slate-300">启用 AI:</span>
            <button
              role="switch"
              aria-checked={settings.ai.enabled}
              onClick={() => {
                setSettings({
                  ...settings,
                  ai: {
                    ...settings.ai,
                    enabled: !settings.ai.enabled,
                  },
                });
              }}
              className={`relative inline-flex h-6 w-11 items-center rounded-full transition-colors focus:outline-none ${
                settings.ai.enabled ? "bg-indigo-600" : "bg-slate-700"
              }`}
            >
              <span
                className={`inline-block h-4 w-4 transform rounded-full bg-white transition-transform ${
                  settings.ai.enabled ? "translate-x-6" : "translate-x-1"
                }`}
              />
            </button>
          </div>
        </div>

        {/* AI Form Fields */}
        <div
          className={`space-y-4 transition-opacity ${
            settings.ai.enabled ? "opacity-100" : "opacity-60"
          }`}
        >
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            {/* Provider */}
            <div>
              <label
                htmlFor="ai-provider-select"
                className="block text-xs font-medium text-slate-300 mb-1"
              >
                AI Provider
              </label>
              <select
                id="ai-provider-select"
                disabled={!settings.ai.enabled}
                value={settings.ai.provider || "openai_compatible"}
                onChange={(e) => {
                  const newProvider = e.target.value as AiProvider;
                  let updatedBaseUrl = settings.ai.base_url;
                  if (newProvider === "ollama") {
                    if (!updatedBaseUrl || updatedBaseUrl.includes("/v1")) {
                      updatedBaseUrl = "http://127.0.0.1:11434";
                    }
                  }
                  setSettings({
                    ...settings,
                    ai: {
                      ...settings.ai,
                      provider: newProvider,
                      base_url: updatedBaseUrl,
                    },
                  });
                }}
                className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
              >
                <option value="openai_compatible">OpenAI Compatible</option>
                <option value="anthropic">Anthropic</option>
                <option value="gemini">Google Gemini</option>
                <option value="ollama">Ollama (Local)</option>
              </select>
            </div>

            {/* Base URL */}
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <label
                  htmlFor="ai-base-url-input"
                  className="block text-xs font-medium text-slate-300"
                >
                  Base URL
                </label>
                {settings.ai.provider === "ollama" && (
                  <button
                    type="button"
                    data-testid="ollama-discover-btn"
                    onClick={handleDiscoverOllama}
                    disabled={!settings.ai.enabled || discoveringOllama}
                    className="text-[11px] text-indigo-400 hover:text-indigo-300 disabled:opacity-50 underline flex items-center gap-1 cursor-pointer"
                  >
                    {discoveringOllama ? "检测中..." : "🔍 检测本地 Ollama"}
                  </button>
                )}
              </div>
              <input
                id="ai-base-url-input"
                type="text"
                disabled={!settings.ai.enabled}
                placeholder={
                  settings.ai.provider === "ollama"
                    ? "例如: http://127.0.0.1:11434"
                    : "例如: https://api.openai.com/v1 或 http://localhost:11434"
                }
                value={settings.ai.base_url || ""}
                onChange={(e) => {
                  setSettings({
                    ...settings,
                    ai: {
                      ...settings.ai,
                      base_url: e.target.value.trim() || null,
                    },
                  });
                }}
                className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
              />
              {settings.ai.provider === "ollama" && detectedEndpoints.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5 pt-1">
                  <span className="text-[10px] text-slate-400">已检测:</span>
                  {detectedEndpoints.map((ep) => (
                    <button
                      key={ep}
                      type="button"
                      onClick={() =>
                        setSettings((prev) => ({
                          ...prev,
                          ai: { ...prev.ai, base_url: ep },
                        }))
                      }
                      className={`px-2 py-0.5 rounded text-[10px] border font-mono transition-colors ${
                        settings.ai.base_url === ep
                          ? "bg-indigo-900/60 border-indigo-600 text-indigo-200"
                          : "bg-slate-800 border-slate-700 text-slate-300 hover:border-slate-500"
                      }`}
                    >
                      {ep}
                    </button>
                  ))}
                </div>
              )}
              {settings.ai.provider === "ollama" && ollamaDiscoveryMessage && (
                <p className="text-[11px] text-slate-400 mt-0.5">{ollamaDiscoveryMessage}</p>
              )}
            </div>

            {/* Model */}
            <div className="space-y-1">
              <div className="flex items-center justify-between">
                <label
                  htmlFor="ai-model-input"
                  className="block text-xs font-medium text-slate-300"
                >
                  Model
                </label>
                {settings.ai.provider === "ollama" && (
                  <button
                    type="button"
                    data-testid="ollama-refresh-models-btn"
                    onClick={handleFetchOllamaModels}
                    disabled={!settings.ai.enabled || fetchingModels}
                    className="text-[11px] text-indigo-400 hover:text-indigo-300 disabled:opacity-50 underline flex items-center gap-1 cursor-pointer"
                  >
                    {fetchingModels ? "获取中..." : "🔄 刷新模型"}
                  </button>
                )}
              </div>
              <input
                id="ai-model-input"
                type="text"
                list={
                  settings.ai.provider === "ollama" && discoveredModels.length > 0
                    ? "ollama-models-datalist"
                    : undefined
                }
                disabled={!settings.ai.enabled}
                placeholder={
                  settings.ai.provider === "ollama"
                    ? "例如: qwen3.5:9b 或 llama3.2"
                    : "例如: gpt-4o-mini 或 claude-3-5-sonnet"
                }
                value={settings.ai.model || ""}
                onChange={(e) => {
                  setSettings({
                    ...settings,
                    ai: {
                      ...settings.ai,
                      model: e.target.value.trim() || null,
                    },
                  });
                }}
                className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
              />
              {settings.ai.provider === "ollama" && discoveredModels.length > 0 && (
                <datalist id="ollama-models-datalist">
                  {discoveredModels.map((m) => (
                    <option key={m.model} value={m.model}>
                      {m.model}
                    </option>
                  ))}
                </datalist>
              )}
              {settings.ai.provider === "ollama" && discoveredModels.length > 0 && (
                <div className="flex flex-wrap items-center gap-1.5 pt-1">
                  <span className="text-[10px] text-slate-400">已安装:</span>
                  {discoveredModels.slice(0, 4).map((m) => (
                    <button
                      key={m.model}
                      type="button"
                      onClick={() =>
                        setSettings((prev) => ({
                          ...prev,
                          ai: { ...prev.ai, model: m.model },
                        }))
                      }
                      className={`px-2 py-0.5 rounded text-[10px] border font-mono transition-colors ${
                        settings.ai.model === m.model
                          ? "bg-indigo-900/60 border-indigo-600 text-indigo-200"
                          : "bg-slate-800 border-slate-700 text-slate-300 hover:border-slate-500"
                      }`}
                    >
                      {m.model}
                    </button>
                  ))}
                  {discoveredModels.length > 4 && (
                    <select
                      aria-label="更多模型"
                      value={settings.ai.model || ""}
                      onChange={(e) => {
                        if (e.target.value) {
                          setSettings((prev) => ({
                            ...prev,
                            ai: { ...prev.ai, model: e.target.value },
                          }));
                        }
                      }}
                      className="bg-slate-800 border border-slate-700 text-slate-300 text-[10px] rounded px-1 py-0.5"
                    >
                      <option value="" disabled>
                        更多 ({discoveredModels.length})...
                      </option>
                      {discoveredModels.map((m) => (
                        <option key={m.model} value={m.model}>
                          {m.model}
                        </option>
                      ))}
                    </select>
                  )}
                </div>
              )}
              {settings.ai.provider === "ollama" && modelFetchMessage && (
                <p className="text-[11px] text-slate-400 mt-0.5">{modelFetchMessage}</p>
              )}
            </div>
          </div>

          {/* AI API Key Input and Status */}
          <div className="pt-2 border-t border-slate-700/50 space-y-3">
            <div className="flex items-center justify-between">
              <label
                htmlFor="ai-api-key-input"
                className="block text-xs font-medium text-slate-300"
              >
                AI API Key
              </label>
              <span
                data-testid="ai-key-status"
                className={`text-xs px-2.5 py-0.5 rounded-full font-medium ${
                  settings.ai.provider === "ollama"
                    ? "bg-slate-800 text-slate-300 border border-slate-600"
                    : secretStatus.ai_api_key_configured
                    ? "bg-emerald-900/60 text-emerald-300 border border-emerald-700"
                    : "bg-slate-700/60 text-slate-400 border border-slate-600"
                }`}
              >
                {settings.ai.provider === "ollama"
                  ? "Not required (无需配置)"
                  : secretStatus.ai_api_key_configured
                  ? "Configured (已配置)"
                  : "Not configured (未配置)"}
              </span>
            </div>

            {settings.ai.provider === "ollama" && (
              <p
                data-testid="ollama-no-key-notice"
                className="text-xs text-indigo-300/90 bg-indigo-950/40 border border-indigo-800/40 rounded p-2 flex items-center gap-1.5"
              >
                <span>💡</span>
                <span>Ollama 为本地模型服务，无需配置 API Key。</span>
              </p>
            )}

            <div className="flex gap-2">
              <input
                id="ai-api-key-input"
                type="password"
                disabled={!settings.ai.enabled || settings.ai.provider === "ollama"}
                placeholder={
                  settings.ai.provider === "ollama"
                    ? "Ollama 本地模型无需 API Key"
                    : secretStatus.ai_api_key_configured
                    ? "•••••••••••••••• (如需更新请输入新 API Key)"
                    : "输入对应 Provider 的 API Key"
                }
                value={aiApiKeyInput}
                onChange={(e) => setAiApiKeyInput(e.target.value)}
                className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
              />
              <button
                onClick={handleSaveAiApiKey}
                disabled={
                  savingApiKey ||
                  !settings.ai.enabled ||
                  settings.ai.provider === "ollama" ||
                  !aiApiKeyInput.trim()
                }
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium rounded-md transition-colors"
              >
                {savingApiKey ? "保存中..." : "保存 API Key"}
              </button>
              <button
                onClick={handleClearAiApiKey}
                disabled={
                  savingApiKey || !secretStatus.ai_api_key_configured
                }
                className="px-3 py-2 bg-slate-700 hover:bg-rose-700 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium rounded-md transition-colors"
              >
                清除 Key
              </button>
            </div>
            {settings.ai.provider !== "ollama" && !secretStatus.ai_api_key_configured && (
              <p className="text-xs text-amber-400/90 bg-amber-950/20 border border-amber-800/30 rounded p-2">
                未配置 AI API Key。测试连接和云端 AI 功能需要配置有效的 API Key。
              </p>
            )}
            <p className="text-xs text-slate-500">
              API Key 同样仅保存在系统凭据管理器中，前端与配置文件不包含明文 Key。
            </p>
          </div>

          {/* Connection test status panel */}
          {(aiConnectionStatus || aiConnectionError) && (
            <div
              data-testid="ai-connection-result"
              className={`p-3 rounded-md border text-xs space-y-2 ${
                aiConnectionStatus
                  ? "bg-slate-900/80 border-emerald-700/60 text-slate-200"
                  : "bg-rose-950/40 border-rose-800/60 text-rose-300"
              }`}
            >
              {aiConnectionStatus && (
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2">
                    <span
                      data-testid="ai-connection-status"
                      className="font-semibold text-emerald-400"
                    >
                      Connected (已连接)
                    </span>
                    <span
                      data-testid="ai-connection-provider"
                      className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 text-[11px]"
                    >
                      Provider: {aiConnectionStatus.provider}
                    </span>
                    <span
                      data-testid="ai-connection-model"
                      className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 text-[11px]"
                    >
                      Model: {aiConnectionStatus.model}
                    </span>
                  </div>
                </div>
              )}

              {aiConnectionError && (
                <div>
                  <span className="font-semibold text-rose-400">✗ 连接失败:</span>{" "}
                  <span data-testid="ai-connection-error">{aiConnectionError}</span>
                </div>
              )}
            </div>
          )}

          {/* Action buttons */}
          <div className="pt-2 flex items-center justify-between">
            <button
              onClick={handleSaveSettings}
              disabled={savingSettings}
              className="px-4 py-2 bg-slate-700 hover:bg-slate-600 disabled:opacity-50 text-slate-100 text-xs font-medium rounded-md transition-colors"
            >
              {savingSettings ? "正在保存偏好..." : "保存 AI 偏好设置"}
            </button>

            <button
              data-testid="ai-test-connection-btn"
              onClick={handleTestAiConnection}
              disabled={testingAiConnection}
              className="px-3 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white border border-indigo-500/50 rounded-md text-xs font-medium transition-colors cursor-pointer disabled:cursor-not-allowed"
            >
              {testingAiConnection ? "正在测试 AI 连接..." : "测试连接"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
