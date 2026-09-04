import React from "react";
import { GitHubRepository } from "../types/github";
import { openGitHubUrl } from "../services/tauri";

interface RepositoryCardProps {
  repo: GitHubRepository;
}

export const RepositoryCard: React.FC<RepositoryCardProps> = ({ repo }) => {
  const handleOpenUrl = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      await openGitHubUrl(repo.github_url);
    } catch (err) {
      console.error("Failed to open URL in browser:", err);
    }
  };

  const formattedDate = repo.pushed_at
    ? new Date(repo.pushed_at).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "未知";

  return (
    <div
      data-testid={`repo-card-${repo.github_repo_id}`}
      className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-3 hover:border-slate-600 transition-colors"
    >
      {/* Header: Name, Category, and Stars */}
      <div className="flex items-start justify-between gap-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2.5 flex-wrap">
            <h3 className="text-base font-semibold text-slate-100 hover:text-indigo-400 transition-colors">
              <a
                href={repo.github_url}
                onClick={handleOpenUrl}
                target="_blank"
                rel="noreferrer"
                className="hover:underline"
              >
                {repo.name}
              </a>
            </h3>
            <span
              data-testid="repo-category"
              className="px-2.5 py-0.5 rounded-full text-xs font-medium bg-indigo-950/80 text-indigo-300 border border-indigo-700/60"
            >
              {repo.category}
            </span>
          </div>
          <div className="text-xs text-slate-400">
            <a
              href={repo.github_url}
              onClick={handleOpenUrl}
              className="text-slate-500 hover:text-slate-400 truncate block max-w-lg"
            >
              {repo.github_url}
            </a>
          </div>
        </div>

        <div className="flex items-center gap-1.5 px-2.5 py-1 bg-slate-900 border border-slate-700 rounded-md text-amber-300 text-xs font-medium shrink-0">
          <span>★</span>
          <span>{repo.stars.toLocaleString()}</span>
        </div>
      </div>

      {/* Description */}
      <p className="text-sm text-slate-300 line-clamp-2">
        {repo.description || "暂无项目简介"}
      </p>

      {/* Topics */}
      {repo.topics && repo.topics.length > 0 && (
        <div className="flex flex-wrap gap-1.5 pt-1">
          {repo.topics.map((topic) => (
            <span
              key={topic}
              className="px-2 py-0.5 rounded text-[11px] bg-slate-700/60 text-slate-300 border border-slate-600/60"
            >
              #{topic}
            </span>
          ))}
        </div>
      )}

      {/* Footer: License, Pushed at, and External link button */}
      <div className="pt-2 border-t border-slate-700/60 flex items-center justify-between text-xs text-slate-400">
        <div className="flex items-center gap-4">
          <div>
            <span className="text-slate-500">License: </span>
            <span className="text-slate-300 font-medium">
              {repo.license || "Unknown"}
            </span>
          </div>
          <div>
            <span className="text-slate-500">最后活跃: </span>
            <span className="text-slate-300 font-medium">{formattedDate}</span>
          </div>
        </div>

        <button
          onClick={handleOpenUrl}
          data-testid="open-github-link"
          className="inline-flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 transition-colors"
        >
          <span>在 GitHub 打开</span>
          <span>↗</span>
        </button>
      </div>
    </div>
  );
};
