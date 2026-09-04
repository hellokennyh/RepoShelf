import React, { useState } from "react";
import { RepositoryCard } from "../components/RepositoryCard";
import { getGitHubRepository } from "../services/tauri";
import { GitHubRepository } from "../types/github";

export const AddRepositoryPage: React.FC = () => {
  const [urlInput, setUrlInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [previewRepo, setPreviewRepo] = useState<GitHubRepository | null>(null);
  const [errorInfo, setErrorInfo] = useState<{
    type: string;
    message: string;
  } | null>(null);

  const handleFetchMetadata = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = urlInput.trim();
    if (!trimmed) return;

    setLoading(true);
    setErrorInfo(null);
    setPreviewRepo(null);

    try {
      const repo = await getGitHubRepository(trimmed);
      setPreviewRepo(repo);
    } catch (err: unknown) {
      const errObj = err as { code?: string; message?: string } | undefined;
      const code = errObj?.code || "Unknown";
      const message = errObj?.message || String(err);

      let userFacingMessage = message;
      if (code === "InvalidRepositoryUrl") {
        userFacingMessage = `URL 格式无效: ${message}`;
      } else if (code === "GitHubNotFound") {
        userFacingMessage = "未找到该项目，请核对仓库地址是否拼写正确。";
      } else if (code === "PrivateRepositoryUnsupported") {
        userFacingMessage = "RepoShelf V1 仅支持公开开源项目，不支持私有仓库。";
      } else if (code === "GitHubAuth") {
        userFacingMessage = "GitHub 身份验证失败，请检查所配置的 Token 是否有效。";
      } else if (code === "GitHubRateLimit") {
        userFacingMessage = `GitHub API 请求频率已达上限: ${message}`;
      } else if (code === "GitHubTimeout") {
        userFacingMessage = "请求超时，请检查网络连接或稍后重试。";
      } else if (code === "GitHubNetwork") {
        userFacingMessage = `网络连接错误: ${message}`;
      }

      setErrorInfo({
        type: code,
        message: userFacingMessage,
      });
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>➕</span> 添加项目
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            Add GitHub Repository
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          输入公开 GitHub 项目地址，提取元数据并预览项目详情。
        </p>
      </div>

      {/* URL Input Form */}
      <form
        onSubmit={handleFetchMetadata}
        className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-3"
      >
        <label
          htmlFor="add-repo-url-input"
          className="block text-xs font-medium text-slate-300"
        >
          GitHub Repository URL
        </label>
        <div className="flex gap-3">
          <input
            id="add-repo-url-input"
            data-testid="add-repo-url-input"
            type="text"
            placeholder="例如: https://github.com/tauri-apps/tauri"
            value={urlInput}
            onChange={(e) => setUrlInput(e.target.value)}
            className="flex-1 bg-slate-900 border border-slate-700 rounded-md px-3.5 py-2 text-sm text-slate-100 focus:outline-none focus:border-indigo-500 font-mono"
          />
          <button
            type="submit"
            data-testid="add-repo-fetch-btn"
            disabled={loading || !urlInput.trim()}
            className="px-5 py-2 bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-xs font-medium rounded-md transition-colors shrink-0"
          >
            {loading ? "获取中..." : "获取项目信息"}
          </button>
        </div>
        <p className="text-xs text-slate-500">
          支持标准 GitHub URL（如 https://github.com/owner/repo），自动规范化处理。
        </p>
      </form>

      {/* Loading state */}
      {loading && (
        <div
          data-testid="add-repo-loading"
          className="rounded-lg border border-slate-700 bg-slate-800/50 p-10 text-center"
        >
          <div className="inline-block animate-spin w-8 h-8 border-4 border-indigo-500 border-t-transparent rounded-full mb-3" />
          <p className="text-sm text-slate-300">正在获取项目元数据...</p>
        </div>
      )}

      {/* Error state */}
      {errorInfo && !loading && (
        <div
          data-testid="add-repo-error"
          className="rounded-lg border border-rose-800/60 bg-rose-950/40 p-5 text-rose-300 space-y-1 text-sm"
        >
          <div className="font-semibold text-rose-200 flex items-center gap-1.5">
            <span>⚠️</span> 获取项目信息失败 ({errorInfo.type})
          </div>
          <p className="text-xs text-rose-300/90">{errorInfo.message}</p>
        </div>
      )}

      {/* Preview Card */}
      {previewRepo && !loading && (
        <div className="space-y-3">
          <div className="flex items-center justify-between text-xs text-slate-400 px-1">
            <span className="font-medium text-slate-300">Repository Preview</span>
            <span className="text-slate-500">元数据预览（尚未存入本地库）</span>
          </div>

          <div data-testid="repo-preview-container">
            <RepositoryCard repo={previewRepo} />
          </div>
        </div>
      )}

      {/* Idle state */}
      {!previewRepo && !errorInfo && !loading && (
        <div className="rounded-lg border border-dashed border-slate-700/80 bg-slate-800/30 p-10 text-center text-slate-400 space-y-2">
          <div className="text-3xl">📥</div>
          <h3 className="text-sm font-medium text-slate-300">等待输入项目地址</h3>
          <p className="text-xs text-slate-500 max-w-sm mx-auto">
            在上方输入框粘贴任何公开的 GitHub Repository 链接，点击“获取项目信息”即可预览。
          </p>
        </div>
      )}
    </div>
  );
};
