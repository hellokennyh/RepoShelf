import React, { useEffect, useState } from "react";
import {
  clearAiApiKey,
  clearGitHubToken,
  discoverOllamaEndpoints,
  fetchSecretStatus,
  fetchSettings,
  listOllamaModels,
  openGitHubUrl,
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
import { useDiscoverySession } from "../context/DiscoverySessionContext";

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
  const { locale, setLocale, t } = useI18n();
  const discoverySession = useDiscoverySession();
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [committedSettings, setCommittedSettings] = useState<Settings>(DEFAULT_SETTINGS);
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
  const [lastCheckedAt, setLastCheckedAt] = useState<Date | null>(null);
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
    type: "success" | "error" | "warning";
    message: string;
  } | null>(null);

  const isSettingsDirty = JSON.stringify(settings) !== JSON.stringify(committedSettings);

  const isAiConfigIncomplete =
    settings.ai.enabled &&
    (!settings.ai.model ||
      !settings.ai.model.trim() ||
      (settings.ai.provider !== "ollama" && !secretStatus.ai_api_key_configured));

  const formatResetTime = (resetAt: string | undefined): string => {
    if (!resetAt) return t("settings.quotaUnavailable");
    const d = new Date(resetAt);
    if (isNaN(d.getTime())) return resetAt;
    const localeMap: Record<UiLocale, string> = {
      en: "en-US",
      "zh-Hans": "zh-CN",
      "zh-Hant": "zh-TW",
    };
    return d.toLocaleString(localeMap[locale] || undefined, {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  };

  const renderQuotaBucket = (bucket: { remaining?: number | null; limit?: number | null } | undefined) => {
    if (
      !bucket ||
      bucket.remaining === undefined ||
      bucket.remaining === null ||
      bucket.limit === undefined ||
      bucket.limit === null
    ) {
      return t("settings.quotaUnavailable");
    }
    return `${bucket.remaining} / ${bucket.limit}`;
  };

  const loadAll = async () => {
    try {
      const [fetchedSettings, status] = await Promise.all([
        fetchSettings().catch(() => DEFAULT_SETTINGS),
        fetchSecretStatus().catch(() => ({
          github_token_configured: false,
          ai_api_key_configured: false,
        })),
      ]);
      if (fetchedSettings) {
        setSettings(fetchedSettings);
        setCommittedSettings(fetchedSettings);
      }
      if (status) setSecretStatus(status);
    } catch (err) {
      setFeedback({
        type: "error",
        message: err instanceof Error ? err.message : String(err),
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
      const aiChanged =
        updated.ai.enabled !== committedSettings.ai.enabled ||
        updated.ai.provider !== committedSettings.ai.provider ||
        updated.ai.base_url !== committedSettings.ai.base_url ||
        updated.ai.model !== committedSettings.ai.model;
      setSettings(updated);
      setCommittedSettings(updated);
      if (aiChanged && discoverySession?.notifyAiConfigChanged) {
        discoverySession.notifyAiConfigChanged();
      }

      const hasUnsavedSecrets =
        githubTokenInput.trim().length > 0 || aiApiKeyInput.trim().length > 0;

      if (hasUnsavedSecrets) {
        setFeedback({
          type: "warning",
          message: t("settings.feedbackSaveSettingsWithUnsavedSecrets"),
        });
      } else {
        setFeedback({
          type: "success",
          message: t("settings.feedbackSaveSettingsSuccess"),
        });
      }
    } catch (err) {
      setFeedback({
        type: "error",
        message: t("settings.feedbackSaveSettingsFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
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
          message: t("settings.feedbackLanguageSaveSuccess"),
        });
      } else {
        setFeedback({
          type: "error",
          message: t("settings.feedbackLanguageSaveFailed"),
        });
      }
    } catch {
      setFeedback({
        type: "error",
        message: t("settings.feedbackLanguageSaveFailed"),
      });
    } finally {
      setSavingLanguage(false);
    }
  };

  const handleSaveGitHubToken = async () => {
    if (!githubTokenInput.trim()) {
      setFeedback({
        type: "error",
        message: t("settings.tokenEmptyError"),
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
      // Invalidate previously displayed connection/quota result
      setConnectionStatus(null);
      setConnectionError(null);
      setLastCheckedAt(null);
      setFeedback({
        type: "success",
        message: t("settings.feedbackSaveTokenSuccess"),
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: t("settings.feedbackSaveTokenFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
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
      // Invalidate previously displayed authenticated connection/quota result
      setConnectionStatus(null);
      setConnectionError(null);
      setLastCheckedAt(null);
      setFeedback({
        type: "success",
        message: t("settings.feedbackClearTokenSuccess"),
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: t("settings.feedbackClearTokenFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
      });
    } finally {
      setSavingToken(false);
    }
  };

  const handleOpenGitHubLogin = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      await openGitHubUrl("https://github.com/login");
    } catch (err) {
      console.error("Failed to open GitHub login:", err);
    }
  };

  const handleTestConnection = async () => {
    try {
      setTestingConnection(true);
      setConnectionError(null);
      const status = await testGitHubConnection();
      setConnectionStatus(status);
      setLastCheckedAt(new Date());
    } catch (err: unknown) {
      const msg =
        err && typeof err === "object" && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      setConnectionError(msg);
      setConnectionStatus(null);
      setLastCheckedAt(new Date());
    } finally {
      setTestingConnection(false);
    }
  };

  const handleSaveAiApiKey = async () => {
    if (!aiApiKeyInput.trim()) {
      setFeedback({
        type: "error",
        message: t("settings.keyEmptyError"),
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
      if (discoverySession?.notifyAiConfigChanged) {
        discoverySession.notifyAiConfigChanged();
      }
      setFeedback({
        type: "success",
        message: t("settings.feedbackSaveKeySuccess"),
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: t("settings.feedbackSaveKeyFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
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
      if (discoverySession?.notifyAiConfigChanged) {
        discoverySession.notifyAiConfigChanged();
      }
      setFeedback({
        type: "success",
        message: t("settings.feedbackClearKeySuccess"),
      });
    } catch (err) {
      setFeedback({
        type: "error",
        message: t("settings.feedbackClearKeyFailed", {
          error: err instanceof Error ? err.message : String(err),
        }),
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
        setOllamaDiscoveryMessage(t("settings.ollamaNoEndpoints"));
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
        setOllamaDiscoveryMessage(t("settings.ollamaEndpointsDetected", { count: endpoints.length }));
      }
    } catch (err) {
      setOllamaDiscoveryMessage(
        t("settings.ollamaDiscoverFailed", { error: err instanceof Error ? err.message : String(err) })
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
        setModelFetchMessage(t("settings.ollamaNoModelsInstalled"));
      } else {
        setModelFetchMessage(t("settings.ollamaModelsRetrieved", { count: models.length }));
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
        t("settings.ollamaFetchModelsFailed", { error: err instanceof Error ? err.message : String(err) })
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
    <div className="space-y-6 w-full pb-12">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>⚙️</span> {t("settings.title")}
          <span className="text-sm font-normal text-slate-400 ml-1">{t("settings.subtitle")}</span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          {t("settings.desc")}
        </p>
      </div>

      {/* Global Feedback Banner */}
      {feedback && (
        <div
          role="alert"
          className={`p-4 rounded-lg flex items-center justify-between text-sm ${
            feedback.type === "success"
              ? "bg-emerald-950/70 border border-emerald-700 text-emerald-200"
              : feedback.type === "warning"
              ? "bg-amber-950/70 border border-amber-700 text-amber-200"
              : "bg-rose-950/70 border border-rose-700 text-rose-200"
          }`}
        >
          <span>{feedback.message}</span>
          <button
            onClick={() => setFeedback(null)}
            className="text-xs opacity-70 hover:opacity-100 underline ml-4"
          >
            {t("common.close")}
          </button>
        </div>
      )}

      {/* 0. Language / Interface Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-4">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
            <span>🌐</span> {t("settings.languageSection")}
          </h2>
          <span className="text-xs text-slate-400">
            {t("settings.languageSectionDesc")}
          </span>
        </div>

        <div className="max-w-xs">
          <label
            htmlFor="ui-language-select"
            className="block text-xs font-medium text-slate-300 mb-1.5"
          >
            {t("settings.displayLanguageLabel")}
          </label>
          <select
            id="ui-language-select"
            data-testid="ui-language-select"
            disabled={savingLanguage}
            value={locale}
            onChange={(e) => handleUpdateLanguage(e.target.value as UiLocale)}
            className="w-full bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
          >
            <option value="en">{t("settings.languages.en")}</option>
            <option value="zh-Hans">{t("settings.languages.zhHans")}</option>
            <option value="zh-Hant">{t("settings.languages.zhHant")}</option>
          </select>
          <p className="text-[11px] text-slate-400 mt-1.5">
            {t("settings.languageHelp")}
          </p>
        </div>
      </div>

      {/* 1. GitHub Configuration Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
            <span>🐙</span> {t("settings.githubSection")}
          </h2>
          <span className="text-xs text-slate-400">
            {t("settings.githubSectionDesc")}
          </span>
        </div>

        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Default Search Limit */}
          <div>
            <label
              htmlFor="github-search-limit"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              {t("settings.searchLimitLabel")}
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
              {t("settings.trendingPeriodLabel")}
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
              <option value="daily">{t("trending.periods.daily")}</option>
              <option value="weekly">{t("trending.periods.weekly")}</option>
              <option value="monthly">{t("trending.periods.monthly")}</option>
            </select>
          </div>
        </div>

        {/* GitHub PAT Input and Status */}
        <div className="pt-2 border-t border-slate-700/50 space-y-3">
          <div className="flex items-center justify-between flex-wrap gap-2">
            <a
              href="https://github.com/login"
              onClick={handleOpenGitHubLogin}
              target="_blank"
              rel="noreferrer"
              data-testid="github-token-title-link"
              className="text-sm font-semibold text-indigo-400 hover:text-indigo-300 hover:underline inline-flex items-center gap-1 focus:outline-none focus:ring-1 focus:ring-indigo-500 rounded"
              aria-label={t("settings.githubTokenTitleLink")}
            >
              <span>{t("settings.githubTokenTitleLink")}</span>
              <span className="text-xs" aria-hidden="true">↗</span>
            </a>
            <span
              data-testid="github-token-status"
              className={`text-xs px-2.5 py-0.5 rounded-full font-medium ${
                secretStatus.github_token_configured
                  ? "bg-emerald-900/60 text-emerald-300 border border-emerald-700"
                  : "bg-slate-700/60 text-slate-400 border border-slate-600"
              }`}
            >
              {secretStatus.github_token_configured
                ? t("settings.tokenConfigured")
                : t("settings.tokenNotConfigured")}
            </span>
          </div>

          <p className="text-xs text-slate-400">
            {t("settings.githubTokenLoginHelp")}
          </p>

          <div>
            <label
              htmlFor="github-pat-input"
              className="block text-xs font-medium text-slate-300 mb-1"
            >
              {t("settings.githubTokenLabel")}
            </label>
            <div className="flex gap-2">
              <input
                id="github-pat-input"
                type="password"
                placeholder={
                  secretStatus.github_token_configured
                    ? t("settings.githubTokenPlaceholderConfigured")
                    : t("settings.githubTokenPlaceholderEmpty")
                }
                value={githubTokenInput}
                onChange={(e) => setGithubTokenInput(e.target.value)}
                className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500"
              />
              <button
                onClick={handleSaveGitHubToken}
                disabled={savingToken || !githubTokenInput.trim()}
                className="px-4 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium rounded-md transition-colors whitespace-nowrap"
              >
                {savingToken ? t("common.saving") : t("settings.saveTokenBtn")}
              </button>
              <button
                onClick={handleClearGitHubToken}
                disabled={savingToken || !secretStatus.github_token_configured}
                className="px-3 py-2 bg-slate-700 hover:bg-rose-700 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium rounded-md transition-colors whitespace-nowrap"
              >
                {t("settings.clearTokenBtn")}
              </button>
            </div>
          </div>
          {!secretStatus.github_token_configured && (
            <p className="text-xs text-amber-400/90 bg-amber-950/20 border border-amber-800/30 rounded p-2">
              {t("settings.githubTokenWarning")}
            </p>
          )}
          <p className="text-xs text-slate-500">
            {t("settings.githubTokenHelp")}
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
                <div className="flex items-center justify-between flex-wrap gap-2">
                  <div className="flex items-center gap-2">
                    <span
                      data-testid="github-connection-status"
                      className="font-semibold text-emerald-400"
                    >
                      {t("settings.githubStatusConnected")}
                    </span>
                    <span
                      data-testid="github-connection-mode"
                      className="px-2 py-0.5 rounded bg-slate-800 border border-slate-700 text-slate-300 text-[11px]"
                    >
                      Mode: {connectionStatus.mode}
                    </span>
                  </div>
                  {lastCheckedAt && (
                    <span
                      data-testid="github-last-checked"
                      className="text-[11px] text-slate-400"
                    >
                      {t("settings.quotaLastCheckedLabel")}: {lastCheckedAt.toLocaleTimeString()}
                    </span>
                  )}
                </div>

                <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 pt-1 border-t border-slate-700/50">
                  <div className="space-y-0.5">
                    <span className="text-slate-400 block font-medium">
                      {t("settings.quotaCoreLabel")}
                    </span>
                    <span
                      data-testid="github-core-ratelimit"
                      className="font-mono text-slate-100 block"
                    >
                      {renderQuotaBucket(connectionStatus.rate_limit?.core)}
                    </span>
                    <span
                      data-testid="github-core-reset"
                      className="text-[11px] text-slate-400 block"
                    >
                      {t("settings.quotaResetTimeLabel")}: {formatResetTime(connectionStatus.rate_limit?.core?.reset_at)}
                    </span>
                  </div>
                  <div className="space-y-0.5">
                    <span className="text-slate-400 block font-medium">
                      {t("settings.quotaSearchLabel")}
                    </span>
                    <span
                      data-testid="github-search-ratelimit"
                      className="font-mono text-slate-100 block"
                    >
                      {renderQuotaBucket(connectionStatus.rate_limit?.search)}
                    </span>
                    <span
                      data-testid="github-search-reset"
                      className="text-[11px] text-slate-400 block"
                    >
                      {t("settings.quotaResetTimeLabel")}: {formatResetTime(connectionStatus.rate_limit?.search?.reset_at)}
                    </span>
                  </div>
                </div>
              </>
            )}

            {connectionError && (
              <div>
                <span className="font-semibold text-rose-400">✗ {t("common.error")}:</span>{" "}
                <span data-testid="github-connection-error">{connectionError}</span>
              </div>
            )}
          </div>
        )}

        {/* Action buttons */}
        <div className="pt-2 flex flex-col sm:flex-row sm:items-center justify-between gap-2">
          <div>
            {githubTokenInput.trim().length > 0 && (
              <span
                data-testid="github-unsaved-token-notice"
                className="text-xs text-amber-400/90"
              >
                {t("settings.githubTestingUnsavedTokenNotice")}
              </span>
            )}
          </div>

          <button
            data-testid="github-test-connection-btn"
            onClick={handleTestConnection}
            disabled={testingConnection}
            className="px-3 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white border border-indigo-500/50 rounded-md text-xs font-medium transition-colors self-end sm:self-auto cursor-pointer"
          >
            {testingConnection ? t("settings.testingGitHubBtn") : t("settings.testGitHubBtn")}
          </button>
        </div>
      </div>

      {/* 2. AI Configuration Card */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-5">
        <div className="flex items-center justify-between border-b border-slate-700/60 pb-3">
          <div>
            <h2 className="text-lg font-semibold text-slate-100 flex items-center gap-2">
              <span>🤖</span> {t("settings.aiSection")}
            </h2>
            <p className="text-xs text-slate-400 mt-0.5">
              {t("settings.aiSectionDesc")}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {isAiConfigIncomplete && (
              <span
                data-testid="ai-incomplete-badge"
                className="text-[11px] text-amber-400 bg-amber-950/40 border border-amber-800/40 px-2 py-0.5 rounded font-medium"
              >
                {t("settings.aiConfigIncomplete")}
              </span>
            )}
            <span className="text-xs text-slate-300">{t("settings.aiEnableLabel")}</span>
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
                {t("settings.aiProviderLabel")}
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
                  {t("settings.aiBaseUrlLabel")}
                </label>
                {settings.ai.provider === "ollama" && (
                  <button
                    type="button"
                    data-testid="ollama-discover-btn"
                    onClick={handleDiscoverOllama}
                    disabled={!settings.ai.enabled || discoveringOllama}
                    className="text-[11px] text-indigo-400 hover:text-indigo-300 disabled:opacity-50 underline flex items-center gap-1 cursor-pointer"
                  >
                    {discoveringOllama ? t("settings.ollamaDiscoveringBtn") : t("settings.ollamaDiscoverBtn")}
                  </button>
                )}
              </div>
              <input
                id="ai-base-url-input"
                type="text"
                disabled={!settings.ai.enabled}
                placeholder={
                  settings.ai.provider === "ollama"
                    ? "http://127.0.0.1:11434"
                    : t("settings.aiBaseUrlPlaceholder")
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
                  <span className="text-[10px] text-slate-400">{t("settings.ollamaDetectedLabel")}</span>
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
                  {t("settings.aiModelLabel")}
                </label>
                {settings.ai.provider === "ollama" && (
                  <button
                    type="button"
                    data-testid="ollama-refresh-models-btn"
                    onClick={handleFetchOllamaModels}
                    disabled={!settings.ai.enabled || fetchingModels}
                    className="text-[11px] text-indigo-400 hover:text-indigo-300 disabled:opacity-50 underline flex items-center gap-1 cursor-pointer"
                  >
                    {fetchingModels ? t("settings.ollamaFetchingModelsBtn") : t("settings.ollamaFetchModelsBtn")}
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
                    ? "qwen2.5:7b, llama3.2..."
                    : t("settings.aiModelPlaceholder")
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
                  <span className="text-[10px] text-slate-400">{t("settings.ollamaInstalledLabel")}</span>
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
                      aria-label={t("settings.ollamaInstalledLabel")}
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
                        {t("settings.ollamaMoreModels", { count: discoveredModels.length })}
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
                {t("settings.aiKeyLabel")}
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
                  ? t("settings.aiKeyStatusNotRequired")
                  : secretStatus.ai_api_key_configured
                  ? t("settings.aiKeyStatusConfigured")
                  : t("settings.aiKeyStatusNotConfigured")}
              </span>
            </div>

            {settings.ai.provider === "ollama" && (
              <p
                data-testid="ollama-no-key-notice"
                className="text-xs text-indigo-300/90 bg-indigo-950/40 border border-indigo-800/40 rounded p-2 flex items-center gap-1.5"
              >
                <span>💡</span>
                <span>{t("settings.ollamaNoKeyNotice")}</span>
              </p>
            )}

            <div className="flex gap-2">
              <input
                id="ai-api-key-input"
                type="password"
                disabled={!settings.ai.enabled || settings.ai.provider === "ollama"}
                placeholder={
                  settings.ai.provider === "ollama"
                    ? t("settings.aiKeyPlaceholderOllama")
                    : secretStatus.ai_api_key_configured
                    ? t("settings.aiKeyPlaceholderConfigured")
                    : t("settings.aiKeyPlaceholderEmpty")
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
                {savingApiKey ? t("common.saving") : t("settings.saveAiKeyBtn")}
              </button>
              <button
                onClick={handleClearAiApiKey}
                disabled={
                  savingApiKey || !secretStatus.ai_api_key_configured
                }
                className="px-3 py-2 bg-slate-700 hover:bg-rose-700 disabled:opacity-50 disabled:cursor-not-allowed text-slate-200 text-xs font-medium rounded-md transition-colors"
              >
                {t("settings.clearAiKeyBtn")}
              </button>
            </div>
            {settings.ai.provider !== "ollama" && !secretStatus.ai_api_key_configured && (
              <p className="text-xs text-amber-400/90 bg-amber-950/20 border border-amber-800/30 rounded p-2">
                {t("settings.aiKeyWarning")}
              </p>
            )}
            <p className="text-xs text-slate-500">
              {t("settings.aiKeyHelp")}
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
                      {t("settings.aiStatusConnected")}
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
                  <span className="font-semibold text-rose-400">✗ {t("common.error")}:</span>{" "}
                  <span data-testid="ai-connection-error">{aiConnectionError}</span>
                </div>
              )}
            </div>
          )}

          {/* Action buttons */}
          <div className="pt-2 flex items-center justify-end">
            <button
              data-testid="ai-test-connection-btn"
              onClick={handleTestAiConnection}
              disabled={testingAiConnection}
              className="px-3 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white border border-indigo-500/50 rounded-md text-xs font-medium transition-colors cursor-pointer disabled:cursor-not-allowed"
            >
              {testingAiConnection ? t("settings.testingAiBtn") : t("settings.testAiBtn")}
            </button>
          </div>
        </div>
      </div>

      {/* 3. Global Settings Actions & Secret Storage Notice */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-6 shadow-sm space-y-4">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
          <div className="space-y-1">
            <p className="text-xs text-slate-400">
              {t("settings.secretStorageNotice")}
            </p>
            {isSettingsDirty && (
              <p
                data-testid="unsaved-settings-notice"
                className="text-xs text-amber-400 font-medium"
              >
                ● {t("settings.unsavedChangesNotice")}
              </p>
            )}
          </div>
          <button
            data-testid="save-settings-btn"
            onClick={handleSaveSettings}
            disabled={savingSettings}
            className="px-6 py-2.5 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white text-sm font-medium rounded-md transition-colors shadow-sm self-end sm:self-auto cursor-pointer disabled:cursor-not-allowed whitespace-nowrap"
          >
            {savingSettings ? t("settings.savingSettingsBtn") : t("settings.saveSettingsBtn")}
          </button>
        </div>
      </div>
    </div>
  );
};
