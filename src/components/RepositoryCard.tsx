import React from "react";
import { GitHubRepository } from "../types/github";
import { TrendingRepositoryItem } from "../types/trending";
import { openGitHubUrl } from "../services/tauri";
import { LanguagesBar } from "./LanguagesBar";
import { StarHistoryPreview } from "./StarHistoryPreview";
import { useI18n } from "../i18n/I18nContext";
import { formatRepositoryDate } from "../utils/date";
import { useDiscoverySession } from "../context/DiscoverySessionContext";

function extractOwnerAndRepo(
  githubUrl?: string,
  name?: string
): { owner: string; repo: string } | null {
  if (githubUrl) {
    try {
      const parsed = new URL(githubUrl);
      const parts = parsed.pathname.split("/").filter(Boolean);
      if (parts.length >= 2) {
        let r = parts[1];
        if (r.endsWith(".git")) r = r.slice(0, -4);
        return { owner: parts[0], repo: r };
      }
    } catch {
      // ignore
    }
  }
  if (name && name.includes("/")) {
    const parts = name.split("/");
    if (parts.length === 2 && parts[0] && parts[1]) {
      return { owner: parts[0], repo: parts[1] };
    }
  }
  return null;
}

export interface RepositoryCardProps {
  repo: GitHubRepository | TrendingRepositoryItem;
  trendingRank?: number;
  periodStars?: number | null;
  periodLabel?: string;
  isFavorited?: boolean;
  onFavorite?: (repo: GitHubRepository | TrendingRepositoryItem) => void;
  onAnalyze?: (repo: GitHubRepository | TrendingRepositoryItem) => void;
  isAiReady?: boolean;
  aiDisabledReason?: string | null;
  selectable?: boolean;
  isSelected?: boolean;
  onToggleSelect?: (selected: boolean) => void;
  translation?: {
    text: string;
    target_locale: string;
    status?: string;
    action?: "translated" | "unchanged";
  } | null;
  translationAction?: "translated" | "unchanged";
  showDetails?: boolean;
}

export const RepositoryCard: React.FC<RepositoryCardProps> = ({
  repo,
  trendingRank,
  periodStars,
  periodLabel,
  isFavorited,
  onFavorite,
  onAnalyze,
  isAiReady: propIsAiReady,
  aiDisabledReason: propAiDisabledReason,
  selectable,
  isSelected,
  onToggleSelect,
  translation,
  translationAction,
  showDetails = true,
}) => {
  const { t } = useI18n();
  const sessionCtx = useDiscoverySession();

  const isAiReady =
    propIsAiReady !== undefined
      ? propIsAiReady
      : (sessionCtx?.aiReadiness?.ready ?? false);
  const aiDisabledReason =
    propAiDisabledReason !== undefined
      ? propAiDisabledReason
      : (sessionCtx?.aiReadiness?.reason ?? null);

  const handleOpenUrl = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
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
    ? formatRepositoryDate(repo.pushed_at, undefined, undefined, "Unknown")
    : "Unknown";

  const rawCreatedAt = "created_at" in repo ? repo.created_at : undefined;
  const formattedCreatedAt = formatRepositoryDate(
    rawCreatedAt,
    undefined,
    undefined,
    t("common.unknown")
  );

  const cardId = repo.github_repo_id ?? repo.name;
  const ownerRepo = extractOwnerAndRepo(repo.github_url, repo.name);

  return (
    <div
      data-testid={`repo-card-${cardId}`}
      className={`group relative rounded-lg border bg-slate-800/80 p-5 shadow-sm transition-all hover:border-slate-600 hover:shadow-md space-y-3 ${
        isSelected
          ? "border-indigo-500/80 bg-indigo-950/20"
          : "border-slate-700/80"
      }`}
    >
      {/* Top row: Name, Category, Badges, URL, Stars */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 min-w-0">
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
                data-testid="open-github-link"
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
            {repo.github_url && (
              <a
                href={repo.github_url}
                onClick={handleOpenUrl}
                target="_blank"
                rel="noreferrer"
                title={repo.github_url}
                data-testid="card-github-url"
                className="text-xs font-mono text-indigo-400 hover:text-indigo-300 hover:underline max-w-[200px] sm:max-w-xs md:max-w-sm truncate"
              >
                {repo.github_url}
              </a>
            )}
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
      <p className="text-sm text-slate-300 line-clamp-2" data-testid="repo-description">
        {repo.description || t("card.noDescription")}
      </p>

      {/* Description Translation Preview */}
      {translation && translation.text && (
        <div
          data-testid="repo-card-translation"
          className="rounded-md bg-slate-900/90 border border-indigo-900/50 p-2.5 space-y-1 text-sm text-slate-200"
        >
          <div className="flex items-center gap-2 flex-wrap text-xs">
            <span
              data-testid="translation-locale-badge"
              className="px-1.5 py-0.5 rounded text-[10px] font-semibold bg-indigo-950 text-indigo-300 border border-indigo-700/60 uppercase"
            >
              {translation.target_locale}
            </span>
            {isFavorited && (
              <span
                data-testid="already-favorited-translation-notice"
                className="text-[11px] text-amber-300/90"
              >
                {t("translation.alreadyFavoritedNotice")}
              </span>
            )}
          </div>
          <p className="text-sm text-slate-200 line-clamp-3 whitespace-pre-wrap">
            {translation.text}
          </p>
        </div>
      )}
      {((translation && (translation.status === "unchanged" || translation.action === "unchanged")) || (!translation && translationAction === "unchanged")) && (
        <div
          data-testid="repo-card-translation-unchanged"
          className="text-xs text-slate-400 italic pt-0.5"
        >
          {t("translation.unchangedNotice")}
        </div>
      )}

      {/* Topics & Languages (shown only when showDetails is true) */}
      {showDetails && (
        <>
          {/* Topics */}
          {repo.topics && repo.topics.length > 0 ? (
            <div className="flex flex-wrap gap-1.5 pt-1" data-testid="repo-topics-container">
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
        </>
      )}

      {/* Star History preview */}
      {ownerRepo && (
        <StarHistoryPreview owner={ownerRepo.owner} repo={ownerRepo.repo} />
      )}

      {/* Footer: License, Pushed at, Releases, and External link button */}
      <div className="pt-2 border-t border-slate-700/60 flex items-center justify-between text-xs text-slate-400">
        <div className="flex items-center gap-4 flex-wrap">
          <div>
            <span className="text-slate-500">{t("card.license")}: </span>
            <span className="text-slate-300 font-medium">
              {repo.license || "Unknown"}
            </span>
          </div>
          <div data-testid="repo-created-at">
            <span className="text-slate-500">{t("card.createdAt")}: </span>
            <span className="text-slate-300 font-medium">{formattedCreatedAt}</span>
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
            type="button"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onAnalyze?.(repo);
            }}
            disabled={!isAiReady}
            title={!isAiReady ? (aiDisabledReason || t("card.aiDisabledNotice")) : undefined}
            data-testid="analyze-btn"
            className={`inline-flex items-center gap-1 text-xs px-2.5 py-1 rounded transition-colors font-medium ${
              !isAiReady
                ? "bg-slate-800 text-slate-500 border border-slate-700/60 cursor-not-allowed opacity-60"
                : "bg-indigo-950/80 text-indigo-300 border border-indigo-700/60 hover:bg-indigo-600 hover:text-white"
            }`}
          >
            <span>{t("card.projectAnalysis")}</span>
          </button>
        </div>
      </div>
    </div>
  );
};
