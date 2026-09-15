import React from "react";
import { GitHubRepository } from "../types/github";
import { TrendingRepositoryItem } from "../types/trending";
import { openGitHubUrl } from "../services/tauri";
import { LanguagesBar } from "./LanguagesBar";
import { useI18n } from "../i18n/I18nContext";

export interface RepositoryCardProps {
  repo: GitHubRepository | TrendingRepositoryItem;
  trendingRank?: number;
  periodStars?: number | null;
  periodLabel?: string;
  isFavorited?: boolean;
  onFavorite?: (repo: GitHubRepository | TrendingRepositoryItem) => void;
  selectable?: boolean;
  isSelected?: boolean;
  onToggleSelect?: (selected: boolean) => void;
}

export const RepositoryCard: React.FC<RepositoryCardProps> = ({
  repo,
  trendingRank,
  periodStars,
  periodLabel,
  isFavorited,
  onFavorite,
  selectable,
  isSelected,
  onToggleSelect,
}) => {
  const { t } = useI18n();

  const handleOpenUrl = async (e: React.MouseEvent) => {
    e.preventDefault();
    try {
      await openGitHubUrl(repo.github_url);
    } catch (err) {
      console.error("Failed to open URL in browser:", err);
    }
  };

  const rank =
    trendingRank !== undefined
      ? trendingRank
      : "trending_rank" in repo
      ? repo.trending_rank
      : undefined;

  const pStars =
    periodStars !== undefined
      ? periodStars
      : "period_stars" in repo
      ? repo.period_stars
      : undefined;

  const formattedDate = repo.pushed_at
    ? new Date(repo.pushed_at).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "Unknown";

  const cardId = repo.github_repo_id ?? repo.name;

  return (
    <div
      data-testid={`repo-card-${cardId}`}
      className={`group relative rounded-lg border bg-slate-800/80 p-5 shadow-sm transition-all hover:border-slate-600 hover:shadow-md space-y-3 ${
        isSelected
          ? "border-indigo-500/80 bg-indigo-950/20"
          : "border-slate-700/80"
      }`}
    >
      {/* Top row: Name, Category, Badges, Stars */}
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1 flex-1 min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            {selectable && (
              <input
                type="checkbox"
                data-testid={`repo-checkbox-${cardId}`}
                checked={isSelected || false}
                onChange={(e) => onToggleSelect?.(e.target.checked)}
                className="h-4 w-4 rounded border-slate-600 bg-slate-900 text-indigo-600 focus:ring-indigo-500 cursor-pointer"
              />
            )}
            {rank !== undefined && (
              <span
                data-testid="trending-rank"
                className="px-2 py-0.5 rounded text-xs font-bold bg-amber-500/20 text-amber-300 border border-amber-500/40"
              >
                #{rank}
              </span>
            )}
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
            {pStars != null && (
              <span
                data-testid="period-stars"
                className="px-2 py-0.5 rounded text-xs font-medium bg-emerald-950/60 text-emerald-400 border border-emerald-700/50"
              >
                +{pStars.toLocaleString()} {periodLabel || t("card.stars")}
              </span>
            )}
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
          <span>
            {repo.stars != null ? repo.stars.toLocaleString() : "Unknown"}
          </span>
        </div>
      </div>

      {/* Description */}
      <p className="text-sm text-slate-300 line-clamp-2">
        {repo.description || t("card.noDescription")}
      </p>

      {/* Topics */}
      {repo.topics && repo.topics.length > 0 ? (
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
      ) : (
        <div data-testid="empty-topics" className="text-xs text-slate-500 italic pt-1">
          {t("card.noTopics")}
        </div>
      )}

      {/* Languages breakdown */}
      <div className="pt-1" data-testid="repo-languages">
        <LanguagesBar languages={repo.languages} mode="compact" />
      </div>

      {/* Footer: License, Pushed at, Releases, and External link button */}
      <div className="pt-2 border-t border-slate-700/60 flex items-center justify-between text-xs text-slate-400">
        <div className="flex items-center gap-4 flex-wrap">
          <div>
            <span className="text-slate-500">{t("card.license")}: </span>
            <span className="text-slate-300 font-medium">
              {repo.license || "Unknown"}
            </span>
          </div>
          <div>
            <span className="text-slate-500">{t("card.lastActive")}: </span>
            <span className="text-slate-300 font-medium">{formattedDate}</span>
          </div>
          <div data-testid="repo-releases">
            <span className="text-slate-300 font-medium">
              {repo.releases !== null && repo.releases !== undefined
                ? `Releases ${repo.releases}`
                : t("card.releasesUnknown")}
            </span>
          </div>
        </div>

        <div className="flex items-center gap-3">
          {onFavorite && (
            <button
              type="button"
              onClick={() => onFavorite(repo)}
              data-testid="favorite-btn"
              className={`inline-flex items-center gap-1 text-xs px-2.5 py-1 rounded transition-colors font-medium ${
                isFavorited
                  ? "bg-amber-500/20 text-amber-300 border border-amber-500/40 hover:bg-amber-500/30"
                  : "bg-slate-700/60 text-slate-300 border border-slate-600/60 hover:bg-indigo-600 hover:text-white"
              }`}
            >
              <span>{isFavorited ? "★" : "☆"}</span>
              <span>{isFavorited ? t("card.favorited") : t("card.favorite")}</span>
            </button>
          )}

          <button
            onClick={handleOpenUrl}
            data-testid="open-github-link"
            className="inline-flex items-center gap-1 text-xs text-indigo-400 hover:text-indigo-300 transition-colors"
          >
            <span>{t("card.openInGitHub")}</span>
            <span>↗</span>
          </button>
        </div>
      </div>
    </div>
  );
};
