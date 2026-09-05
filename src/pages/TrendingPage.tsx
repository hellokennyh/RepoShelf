import React, { useEffect, useState } from "react";
import { RepositoryCard } from "../components/RepositoryCard";
import { fetchSettings, fetchTrending } from "../services/tauri";
import {
  TrendingPeriod,
  TrendingResponse,
} from "../types/trending";

export const TrendingPage: React.FC = () => {
  const [period, setPeriod] = useState<TrendingPeriod>("daily");
  const [limit, setLimit] = useState<number>(20);

  // States: "idle" | "loading" | "success" | "empty" | "error"
  const [status, setStatus] = useState<
    "idle" | "loading" | "success" | "empty" | "error"
  >("idle");
  const [response, setResponse] = useState<TrendingResponse | null>(null);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  // Read default_trending_period from settings on mount (WITHOUT auto-fetching)
  useEffect(() => {
    fetchSettings()
      .then((s) => {
        if (s?.github?.default_trending_period) {
          const p = s.github.default_trending_period;
          if (p === "daily" || p === "weekly" || p === "monthly") {
            setPeriod(p);
          }
        }
      })
      .catch((err) => {
        console.warn("Failed to load default trending period:", err);
      });
  }, []);

  const handleFetch = async () => {
    setStatus("loading");
    setErrorMessage(null);

    try {
      const res = await fetchTrending(period, limit);
      setResponse(res);
      if (res.items.length === 0) {
        setStatus("empty");
      } else {
        setStatus("success");
      }
    } catch (err: unknown) {
      console.error("Failed to fetch trending repositories:", err);
      const msg =
        typeof err === "object" && err !== null && "message" in err
          ? String((err as { message: unknown }).message)
          : String(err);
      setErrorMessage(msg || "获取热门项目失败，请稍后重试。");
      setStatus("error");
    }
  };

  const getPeriodLabel = (p: TrendingPeriod): string => {
    switch (p) {
      case "daily":
        return "stars today";
      case "weekly":
        return "stars this week";
      case "monthly":
        return "stars this month";
    }
  };

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="border-b border-slate-700 pb-4">
        <h1 className="text-2xl font-bold text-slate-100 flex items-center gap-2">
          <span>🔥</span> 热门项目
          <span className="text-xs font-normal text-slate-400 font-mono ml-2">
            GitHub Trending
          </span>
        </h1>
        <p className="text-slate-400 mt-1 text-sm">
          浏览 GitHub 官方热门开源项目，支持按天、周、月查看与元数据补充。
        </p>
      </div>

      {/* Controls Bar */}
      <div className="rounded-lg border border-slate-700 bg-slate-800/80 p-5 shadow-sm space-y-4">
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-4 items-end">
          {/* Period */}
          <div>
            <label
              htmlFor="trending-period-select"
              className="block text-xs font-medium text-slate-300 mb-1.5"
            >
              时间周期
            </label>
            <select
              id="trending-period-select"
              data-testid="trending-period-select"
              value={period}
              onChange={(e) => setPeriod(e.target.value as TrendingPeriod)}
              className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            >
              <option value="daily">Today</option>
              <option value="weekly">This week</option>
              <option value="monthly">This month</option>
            </select>
          </div>

          {/* Limit */}
          <div>
            <label
              htmlFor="trending-limit-select"
              className="block text-xs font-medium text-slate-300 mb-1.5"
            >
              返回数量
            </label>
            <select
              id="trending-limit-select"
              data-testid="trending-limit-select"
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="w-full rounded-md border border-slate-600 bg-slate-900 px-3 py-2 text-sm text-slate-100 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500"
            >
              <option value={10}>10</option>
              <option value={20}>20</option>
              <option value={30}>30</option>
              <option value={50}>50</option>
            </select>
          </div>

          {/* Action Button */}
          <div>
            <button
              type="button"
              onClick={handleFetch}
              disabled={status === "loading"}
              data-testid="trending-fetch-btn"
              className="w-full inline-flex justify-center items-center gap-2 rounded-md bg-indigo-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-indigo-500 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 focus:ring-offset-slate-900 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {status === "loading" ? (
                <>
                  <span className="inline-block animate-spin">⟳</span>
                  <span>获取中...</span>
                </>
              ) : (
                <>
                  <span>🔥</span>
                  <span>获取热门项目 / Refresh</span>
                </>
              )}
            </button>
          </div>
        </div>
      </div>

      {/* Partial Warning Banner */}
      {status === "success" && response?.partial_warning && (
        <div
          data-testid="trending-partial-warning"
          className="rounded-lg border border-amber-500/40 bg-amber-950/30 p-4 text-sm text-amber-200 flex items-start gap-3"
        >
          <span className="text-amber-400 text-lg shrink-0">⚠️</span>
          <div className="space-y-1">
            <p className="font-medium">{response.partial_warning}</p>
            {response.enrichment_summary.rate_limited &&
              response.enrichment_summary.rate_limit_reset && (
                <p className="text-xs text-amber-300/80">
                  GitHub API 限额重置时间:{" "}
                  {new Date(
                    response.enrichment_summary.rate_limit_reset * 1000
                  ).toLocaleTimeString()}
                </p>
              )}
          </div>
        </div>
      )}

      {/* Short Source Result Notice */}
      {status === "success" && response?.is_short_source && (
        <div
          data-testid="trending-short-source-notice"
          className="rounded-md border border-slate-700 bg-slate-800/60 px-4 py-2.5 text-xs text-slate-300 flex items-center gap-2"
        >
          <span>ℹ️</span>
          <span>当前数据源只返回了 {response.returned_count} 个热门项目。</span>
        </div>
      )}

      {/* State Renderings */}
      {status === "idle" && (
        <div
          data-testid="trending-idle-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/30 p-12 text-center"
        >
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-800 text-slate-400 mb-3 text-2xl">
            🔥
          </div>
          <h3 className="text-base font-medium text-slate-200">
            浏览 GitHub 热门项目
          </h3>
          <p className="text-slate-400 mt-2 text-sm max-w-md mx-auto">
            请选择时间周期和返回数量，点击上方「获取热门项目」按钮开始获取实时 GitHub Trending 官方排行。
          </p>
        </div>
      )}

      {status === "loading" && (
        <div
          data-testid="trending-loading-state"
          className="rounded-lg border border-slate-700 bg-slate-800/40 p-12 text-center space-y-3"
        >
          <div className="inline-block animate-spin text-3xl text-indigo-400">
            ⟳
          </div>
          <p className="text-slate-300 text-sm font-medium">
            正在从 GitHub 获取热门项目并补充元数据...
          </p>
          <p className="text-slate-500 text-xs">
            正在解析排名并进行安全受限并发补充 (最大并发 4)
          </p>
        </div>
      )}

      {status === "empty" && (
        <div
          data-testid="trending-empty-state"
          className="rounded-lg border border-dashed border-slate-700 bg-slate-800/40 p-12 text-center"
        >
          <div className="inline-flex items-center justify-center w-12 h-12 rounded-full bg-slate-800 text-slate-400 mb-3 text-xl">
            🔍
          </div>
          <h3 className="text-base font-medium text-slate-200">暂无热门项目数据</h3>
          <p className="text-slate-400 mt-1 text-sm">
            GitHub Trending 数据源当前未返回任何项目，请稍后再试。
          </p>
        </div>
      )}

      {status === "error" && (
        <div
          data-testid="trending-error-state"
          className="rounded-lg border border-rose-500/40 bg-rose-950/30 p-6 text-center space-y-3"
        >
          <div className="text-rose-400 text-2xl">⚠️</div>
          <h3 className="text-base font-medium text-rose-200">获取热门项目失败</h3>
          <p className="text-rose-300 text-sm max-w-md mx-auto">
            {errorMessage || "网络请求或 HTML 解析异常"}
          </p>
          <button
            type="button"
            onClick={handleFetch}
            className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded bg-rose-900/60 hover:bg-rose-900 text-rose-200 border border-rose-700 text-xs font-medium transition-colors"
          >
            <span>重试 / Retry</span>
          </button>
        </div>
      )}

      {status === "success" && response && (
        <div className="space-y-4">
          <div className="flex items-center justify-between text-xs text-slate-400 px-1">
            <div>
              共返回{" "}
              <span className="font-semibold text-slate-200">
                {response.returned_count}
              </span>{" "}
              个项目（严格保持 GitHub Trending 排名）
            </div>
            <div>
              周期:{" "}
              <span className="text-indigo-300 font-medium">
                {period === "daily"
                  ? "Today"
                  : period === "weekly"
                  ? "This week"
                  : "This month"}
              </span>
            </div>
          </div>

          <div
            data-testid="trending-results-list"
            className="space-y-3"
          >
            {response.items.map((repo) => (
              <RepositoryCard
                key={`${repo.trending_rank}-${repo.github_url}`}
                repo={repo}
                trendingRank={repo.trending_rank}
                periodStars={repo.period_stars}
                periodLabel={getPeriodLabel(response.period)}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
};
