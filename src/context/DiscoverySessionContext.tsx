import React, { createContext, useContext, useState, useCallback, useRef, useEffect } from "react";
import {
  TrendingPeriod,
  TrendingResponse,
} from "../types/trending";
import {
  GitHubRepository,
  LastActivePreset,
  AiSearchReadiness,
} from "../types/github";
import { ArticleSummary } from "../types/article";
import {
  checkFavoriteIdentitiesBatch,
  getAiSearchReadiness,
} from "../services/tauri";
import { AnalysisEnvelopeV1 } from "../types/library";

export interface SessionTranslationItem {
  text: string;
  target_locale: string;
  source_hash: string;
  action: "translated" | "unchanged";
  status?: "valid" | "stale" | "corrupt";
}

export interface SessionAnalysisPreview {
  repoKey: string;
  github_repo_id?: number | null;
  canonical_url: string;
  envelope: AnalysisEnvelopeV1;
  cachedAt: number;
  byteSize: number;
}

export const SESSION_PREVIEW_MAX_COUNT = 64;
export const SESSION_PREVIEW_MAX_BYTES = 8 * 1024 * 1024; // 8 MiB

export interface TrendingSessionState {
  period: TrendingPeriod;
  limit: number;
  status: "idle" | "loading" | "success" | "empty" | "error";
  response: TrendingResponse | null;
  errorMessage: string | null;
  selectedUrls: Set<string>;
  generation: number;
  scrollPosition: number;
  translations: Record<string, SessionTranslationItem>;
}

export interface SearchSessionState {
  searchMode: "keyword" | "ai";
  query: string;
  aiInput: string;
  minStars: number | "";
  lastActive: LastActivePreset;
  limit: number;
  excludeSelected: boolean;
  excludeFavorites: boolean;
  status: "idle" | "loading" | "success" | "empty" | "error" | "rate_limited";
  results: GitHubRepository[];
  generatedQueries: string[];
  isPartial: boolean;
  errorMessage: string | null;
  selectedUrls: Set<string>;
  generation: number;
  scrollPosition: number;
  translations: Record<string, SessionTranslationItem>;
}

export interface AddSessionState {
  urlInput: string;
  loading: boolean;
  mode: "idle" | "direct" | "article";
  previewRepo: GitHubRepository | null;
  articleResults: GitHubRepository[];
  articleSummary: ArticleSummary | null;
  errorInfo: { type: string; message: string } | null;
  actionBanner: { type: "success" | "error"; message: string } | null;
  generation: number;
  scrollPosition: number;
  translations: Record<string, SessionTranslationItem>;
}

export interface DiscoverySessionContextValue {
  // Favorite state sync
  isFavorited: (repoId?: number | null, url?: string | null) => boolean;
  markFavorited: (repoId?: number | null, url?: string | null) => void;
  markUnfavorited: (repoId?: number | null, url?: string | null) => void;
  reconcileIdentities: (
    items: Array<{ github_repo_id?: number | null; github_url: string }>
  ) => Promise<void>;

  // Trending session
  trending: TrendingSessionState;
  setTrending: React.Dispatch<React.SetStateAction<TrendingSessionState>>;
  nextTrendingGeneration: () => number;
  saveTrendingScroll: (pos: number) => void;
  updateTrendingTranslation: (url: string, item: SessionTranslationItem) => void;

  // Search session
  search: SearchSessionState;
  setSearch: React.Dispatch<React.SetStateAction<SearchSessionState>>;
  nextSearchGeneration: () => number;
  saveSearchScroll: (pos: number) => void;
  updateSearchTranslation: (url: string, item: SessionTranslationItem) => void;

  // Add session
  add: AddSessionState;
  setAdd: React.Dispatch<React.SetStateAction<AddSessionState>>;
  nextAddGeneration: () => number;
  saveAddScroll: (pos: number) => void;
  updateAddTranslation: (url: string, item: SessionTranslationItem) => void;

  // AI readiness & configuration invalidation
  aiReadiness: AiSearchReadiness;
  aiReadinessLoading: boolean;
  refreshAiReadiness: () => Promise<AiSearchReadiness>;
  aiRevision: number;
  notifyAiConfigChanged: () => Promise<void>;

  // Analysis session preview cache
  getSessionPreview: (repoId?: number | null, url?: string | null) => AnalysisEnvelopeV1 | null;
  setSessionPreview: (envelope: AnalysisEnvelopeV1, repoId?: number | null, url?: string | null) => void;
  evictSessionPreview: (repoId?: number | null, url?: string | null) => void;
  activePreviewKey: string | null;
  setActivePreviewKey: (key: string | null) => void;
}

export const DiscoverySessionContext = createContext<DiscoverySessionContextValue | null>(null);

export function computeSessionPreviewKey(repoId?: number | null, url?: string | null): string {
  if (repoId != null && repoId > 0) {
    return `id:${repoId}`;
  }
  const norm = normalizeUrl(url);
  if (norm) {
    return `url:${norm}`;
  }
  return `raw:${url || "unknown"}`;
}

function normalizeUrl(url?: string | null): string | null {
  if (!url) return null;
  const trimmed = url.trim();
  if (!trimmed) return null;
  try {
    const parsed = new URL(trimmed);
    const host = parsed.hostname.toLowerCase();
    if (host === "github.com" || host === "www.github.com") {
      const parts = parsed.pathname.split("/").filter(Boolean);
      if (parts.length >= 2) {
        let repo = parts[1];
        if (repo.endsWith(".git")) repo = repo.slice(0, -4);
        return `https://github.com/${parts[0]}/${repo}`;
      }
    }
  } catch {
    // fallback to string trim
  }
  return trimmed.replace(/\/+$/, "");
}

export const DiscoverySessionProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  // Application-level favorite identities
  const [favoritedRepoIds, setFavoritedRepoIds] = useState<Set<number>>(new Set());
  const [favoritedUrls, setFavoritedUrls] = useState<Set<string>>(new Set());

  // Generation counters stored in refs to guarantee monotonic uniqueness
  const trendingGenRef = useRef(0);
  const searchGenRef = useRef(0);
  const addGenRef = useRef(0);

  // Trending session: default limit = 10 (R03)
  const [trending, setTrending] = useState<TrendingSessionState>({
    period: "daily",
    limit: 10,
    status: "idle",
    response: null,
    errorMessage: null,
    selectedUrls: new Set(),
    generation: 0,
    scrollPosition: 0,
    translations: {},
  });

  // Search session: default limit = 20
  const [search, setSearch] = useState<SearchSessionState>({
    searchMode: "keyword",
    query: "",
    aiInput: "",
    minStars: "",
    lastActive: "half_year",
    limit: 20,
    excludeSelected: false,
    excludeFavorites: false,
    status: "idle",
    results: [],
    generatedQueries: [],
    isPartial: false,
    errorMessage: null,
    selectedUrls: new Set(),
    generation: 0,
    scrollPosition: 0,
    translations: {},
  });

  // Add session
  const [add, setAdd] = useState<AddSessionState>({
    urlInput: "",
    loading: false,
    mode: "idle",
    previewRepo: null,
    articleResults: [],
    articleSummary: null,
    errorInfo: null,
    actionBanner: null,
    generation: 0,
    scrollPosition: 0,
    translations: {},
  });

  // AI readiness & configuration invalidation state
  const [aiReadiness, setAiReadiness] = useState<AiSearchReadiness>({
    ready: false,
    reason: null,
  });
  const [aiReadinessLoading, setAiReadinessLoading] = useState(true);
  const [aiRevision, setAiRevision] = useState(0);

  const refreshAiReadiness = useCallback(async () => {
    try {
      const r = await getAiSearchReadiness();
      setAiReadiness(r);
      setAiReadinessLoading(false);
      if (!r.ready) {
        setSearch((prev) => (prev.searchMode === "ai" ? { ...prev, searchMode: "keyword" } : prev));
      }
      return r;
    } catch {
      const fallback: AiSearchReadiness = { ready: false, reason: null };
      setAiReadiness(fallback);
      setAiReadinessLoading(false);
      setSearch((prev) => (prev.searchMode === "ai" ? { ...prev, searchMode: "keyword" } : prev));
      return fallback;
    }
  }, []);

  const notifyAiConfigChanged = useCallback(async () => {
    setAiRevision((prev) => prev + 1);
    try {
      const r = await getAiSearchReadiness();
      setAiReadiness(r);
      setAiReadinessLoading(false);
      if (!r.ready) {
        searchGenRef.current += 1;
        setSearch((prev) => ({
          ...prev,
          searchMode: "keyword",
          generation: searchGenRef.current,
          status: prev.searchMode === "ai" && prev.status === "loading" ? "idle" : prev.status,
        }));
      }
    } catch {
      searchGenRef.current += 1;
      setAiReadiness({ ready: false, reason: null });
      setAiReadinessLoading(false);
      setSearch((prev) => ({
        ...prev,
        searchMode: "keyword",
        generation: searchGenRef.current,
        status: prev.searchMode === "ai" && prev.status === "loading" ? "idle" : prev.status,
      }));
    }
  }, []);

  useEffect(() => {
    refreshAiReadiness();
  }, [refreshAiReadiness]);

  const isFavorited = useCallback(
    (repoId?: number | null, url?: string | null): boolean => {
      if (repoId != null && favoritedRepoIds.has(repoId)) {
        return true;
      }
      const norm = normalizeUrl(url);
      if (norm && favoritedUrls.has(norm)) {
        return true;
      }
      if (url && favoritedUrls.has(url)) {
        return true;
      }
      return false;
    },
    [favoritedRepoIds, favoritedUrls]
  );

  const markFavorited = useCallback(
    (repoId?: number | null, url?: string | null) => {
      if (repoId != null) {
        setFavoritedRepoIds((prev) => new Set(prev).add(repoId));
      }
      const norm = normalizeUrl(url);
      if (norm) {
        setFavoritedUrls((prev) => new Set(prev).add(norm));
      } else if (url) {
        setFavoritedUrls((prev) => new Set(prev).add(url));
      }
    },
    []
  );

  const markUnfavorited = useCallback(
    (repoId?: number | null, url?: string | null) => {
      if (repoId != null) {
        setFavoritedRepoIds((prev) => {
          const next = new Set(prev);
          next.delete(repoId);
          return next;
        });
      }
      const norm = normalizeUrl(url);
      setFavoritedUrls((prev) => {
        const next = new Set(prev);
        if (norm) next.delete(norm);
        if (url) next.delete(url);
        return next;
      });
    },
    []
  );

  const reconcileIdentities = useCallback(
    async (items: Array<{ github_repo_id?: number | null; github_url: string }>) => {
      if (!items || items.length === 0) return;

      const repoIds: number[] = [];
      const urls: string[] = [];

      for (const item of items) {
        if (item.github_repo_id != null) {
          repoIds.push(item.github_repo_id);
        }
        if (item.github_url) {
          urls.push(item.github_url);
        }
      }

      try {
        const res = await checkFavoriteIdentitiesBatch(repoIds, urls);
        if (res.matched_repo_ids.length > 0 || res.matched_urls.length > 0) {
          setFavoritedRepoIds((prev) => {
            const next = new Set(prev);
            for (const id of res.matched_repo_ids) {
              next.add(id);
            }
            return next;
          });

          setFavoritedUrls((prev) => {
            const next = new Set(prev);
            for (const u of res.matched_urls) {
              const norm = normalizeUrl(u);
              if (norm) next.add(norm);
              next.add(u);
            }
            return next;
          });
        }
      } catch (err) {
        console.warn("Failed to batch reconcile favorite identities:", err);
      }
    },
    []
  );

  const nextTrendingGeneration = useCallback(() => {
    trendingGenRef.current += 1;
    return trendingGenRef.current;
  }, []);

  const saveTrendingScroll = useCallback((pos: number) => {
    setTrending((prev) => ({ ...prev, scrollPosition: pos }));
  }, []);

  const nextSearchGeneration = useCallback(() => {
    searchGenRef.current += 1;
    return searchGenRef.current;
  }, []);

  const saveSearchScroll = useCallback((pos: number) => {
    setSearch((prev) => ({ ...prev, scrollPosition: pos }));
  }, []);

  const nextAddGeneration = useCallback(() => {
    addGenRef.current += 1;
    return addGenRef.current;
  }, []);

  const saveAddScroll = useCallback((pos: number) => {
    setAdd((prev) => ({ ...prev, scrollPosition: pos }));
  }, []);

  const updateTrendingTranslation = useCallback((url: string, item: SessionTranslationItem) => {
    const norm = normalizeUrl(url) || url;
    setTrending((prev) => ({
      ...prev,
      translations: {
        ...prev.translations,
        [norm]: item,
        [url]: item,
      },
    }));
  }, []);

  const updateSearchTranslation = useCallback((url: string, item: SessionTranslationItem) => {
    const norm = normalizeUrl(url) || url;
    setSearch((prev) => ({
      ...prev,
      translations: {
        ...prev.translations,
        [norm]: item,
        [url]: item,
      },
    }));
  }, []);

  const updateAddTranslation = useCallback((url: string, item: SessionTranslationItem) => {
    const norm = normalizeUrl(url) || url;
    setAdd((prev) => ({
      ...prev,
      translations: {
        ...prev.translations,
        [norm]: item,
        [url]: item,
      },
    }));
  }, []);

  // Analysis session preview cache state & methods
  const [analysisPreviews, setAnalysisPreviews] = useState<Record<string, SessionAnalysisPreview>>({});
  const [activePreviewKey, setActivePreviewKey] = useState<string | null>(null);

  const getSessionPreview = useCallback(
    (repoId?: number | null, url?: string | null): AnalysisEnvelopeV1 | null => {
      const key = computeSessionPreviewKey(repoId, url);
      const item = analysisPreviews[key];
      return item ? item.envelope : null;
    },
    [analysisPreviews]
  );

  const evictSessionPreview = useCallback(
    (repoId?: number | null, url?: string | null) => {
      const key = computeSessionPreviewKey(repoId, url);
      setAnalysisPreviews((prev) => {
        if (!(key in prev)) return prev;
        const next = { ...prev };
        delete next[key];
        return next;
      });
    },
    []
  );

  const setSessionPreview = useCallback(
    (envelope: AnalysisEnvelopeV1, repoId?: number | null, url?: string | null) => {
      const key = computeSessionPreviewKey(repoId, url);
      const serialized = JSON.stringify(envelope);
      const byteSize = (typeof Blob !== "undefined" ? new Blob([serialized]).size : serialized.length * 2) || serialized.length;

      setAnalysisPreviews((prev) => {
        const next = { ...prev };
        let currentCount = Object.keys(next).length;
        let currentBytes = Object.values(next).reduce((acc, cur) => acc + cur.byteSize, 0);

        if (key in next) {
          currentBytes -= next[key].byteSize;
          currentCount -= 1;
        }

        while (currentCount >= SESSION_PREVIEW_MAX_COUNT || currentBytes + byteSize > SESSION_PREVIEW_MAX_BYTES) {
          const evictableKeys = Object.keys(next).filter(
            (k) => k !== activePreviewKey && k !== key
          );
          if (evictableKeys.length === 0) {
            throw new Error("Session preview budget reached (maximum 64 previews or 8 MiB). Cannot cache new previews.");
          }
          evictableKeys.sort((a, b) => next[a].cachedAt - next[b].cachedAt);
          const oldestKey = evictableKeys[0];
          currentBytes -= next[oldestKey].byteSize;
          currentCount -= 1;
          delete next[oldestKey];
        }

        next[key] = {
          repoKey: key,
          github_repo_id: repoId,
          canonical_url: normalizeUrl(url) || url || "",
          envelope,
          cachedAt: Date.now(),
          byteSize,
        };
        return next;
      });
    },
    [activePreviewKey]
  );

  return (
    <DiscoverySessionContext.Provider
      value={{
        isFavorited,
        markFavorited,
        markUnfavorited,
        reconcileIdentities,
        trending,
        setTrending,
        nextTrendingGeneration,
        saveTrendingScroll,
        updateTrendingTranslation,
        search,
        setSearch,
        nextSearchGeneration,
        saveSearchScroll,
        updateSearchTranslation,
        add,
        setAdd,
        nextAddGeneration,
        saveAddScroll,
        updateAddTranslation,
        aiReadiness,
        aiReadinessLoading,
        refreshAiReadiness,
        aiRevision,
        notifyAiConfigChanged,
        getSessionPreview,
        setSessionPreview,
        evictSessionPreview,
        activePreviewKey,
        setActivePreviewKey,
      }}
    >
      {children}
    </DiscoverySessionContext.Provider>
  );
};

const defaultTrending: TrendingSessionState = {
  period: "daily",
  limit: 10,
  status: "idle",
  response: null,
  errorMessage: null,
  selectedUrls: new Set(),
  generation: 0,
  scrollPosition: 0,
  translations: {},
};

const defaultSearch: SearchSessionState = {
  searchMode: "keyword",
  query: "",
  aiInput: "",
  minStars: "",
  lastActive: "half_year",
  limit: 20,
  excludeSelected: false,
  excludeFavorites: false,
  status: "idle",
  results: [],
  generatedQueries: [],
  isPartial: false,
  errorMessage: null,
  selectedUrls: new Set(),
  generation: 0,
  scrollPosition: 0,
  translations: {},
};

const defaultAdd: AddSessionState = {
  urlInput: "",
  loading: false,
  mode: "idle",
  previewRepo: null,
  articleResults: [],
  articleSummary: null,
  errorInfo: null,
  actionBanner: null,
  generation: 0,
  scrollPosition: 0,
  translations: {},
};


export function useDiscoverySession(): DiscoverySessionContextValue {
  const ctx = useContext(DiscoverySessionContext);
  // Fallback local state if rendered outside DiscoverySessionProvider
  const [localTrending, setLocalTrending] = useState<TrendingSessionState>(defaultTrending);
  const [localSearch, setLocalSearch] = useState<SearchSessionState>(defaultSearch);
  const [localAdd, setLocalAdd] = useState<AddSessionState>(defaultAdd);
  const [localFavoritedRepoIds, setLocalFavoritedRepoIds] = useState<Set<number>>(new Set());
  const [localFavoritedUrls, setLocalFavoritedUrls] = useState<Set<string>>(new Set());
  const [localAnalysisPreviews, setLocalAnalysisPreviews] = useState<Record<string, SessionAnalysisPreview>>({});
  const [localActivePreviewKey, setLocalActivePreviewKey] = useState<string | null>(null);
  const localGenRef = useRef(0);

  const [localAiReadiness, setLocalAiReadiness] = useState<AiSearchReadiness>({
    ready: false,
    reason: null,
  });
  const [localAiReadinessLoading, setLocalAiReadinessLoading] = useState(true);
  const [localAiRevision, setLocalAiRevision] = useState(0);

  const refreshLocalAiReadiness = useCallback(async () => {
    try {
      const r = await getAiSearchReadiness();
      setLocalAiReadiness(r);
      setLocalAiReadinessLoading(false);
      if (!r.ready) {
        setLocalSearch((prev) => (prev.searchMode === "ai" ? { ...prev, searchMode: "keyword" } : prev));
      }
      return r;
    } catch {
      const fallback: AiSearchReadiness = { ready: false, reason: null };
      setLocalAiReadiness(fallback);
      setLocalAiReadinessLoading(false);
      setLocalSearch((prev) => (prev.searchMode === "ai" ? { ...prev, searchMode: "keyword" } : prev));
      return fallback;
    }
  }, []);

  const notifyLocalAiConfigChanged = useCallback(async () => {
    setLocalAiRevision((prev) => prev + 1);
    try {
      const r = await getAiSearchReadiness();
      setLocalAiReadiness(r);
      setLocalAiReadinessLoading(false);
      if (!r.ready) {
        localGenRef.current += 1;
        setLocalSearch((prev) => ({
          ...prev,
          searchMode: "keyword",
          generation: localGenRef.current,
          status: prev.searchMode === "ai" && prev.status === "loading" ? "idle" : prev.status,
        }));
      }
    } catch {
      localGenRef.current += 1;
      setLocalAiReadiness({ ready: false, reason: null });
      setLocalAiReadinessLoading(false);
      setLocalSearch((prev) => ({
        ...prev,
        searchMode: "keyword",
        generation: localGenRef.current,
        status: prev.searchMode === "ai" && prev.status === "loading" ? "idle" : prev.status,
      }));
    }
  }, []);

  if (ctx) {
    return ctx;
  }

  return {
    isFavorited: (repoId?: number | null, url?: string | null) => {
      if (repoId != null && localFavoritedRepoIds.has(repoId)) return true;
      const norm = normalizeUrl(url);
      if (norm && localFavoritedUrls.has(norm)) return true;
      if (url && localFavoritedUrls.has(url)) return true;
      return false;
    },
    markFavorited: (repoId?: number | null, url?: string | null) => {
      if (repoId != null) setLocalFavoritedRepoIds((prev) => new Set(prev).add(repoId));
      const norm = normalizeUrl(url);
      if (norm) {
        setLocalFavoritedUrls((prev) => new Set(prev).add(norm));
      } else if (url) {
        setLocalFavoritedUrls((prev) => new Set(prev).add(url));
      }
    },
    markUnfavorited: (repoId?: number | null, url?: string | null) => {
      if (repoId != null) {
        setLocalFavoritedRepoIds((prev) => {
          const next = new Set(prev);
          next.delete(repoId);
          return next;
        });
      }
      const norm = normalizeUrl(url);
      setLocalFavoritedUrls((prev) => {
        const next = new Set(prev);
        if (norm) next.delete(norm);
        if (url) next.delete(url);
        return next;
      });
    },
    reconcileIdentities: async () => {},
    trending: localTrending,
    setTrending: setLocalTrending,
    nextTrendingGeneration: () => {
      localGenRef.current += 1;
      return localGenRef.current;
    },
    saveTrendingScroll: (pos: number) => {
      setLocalTrending((prev) => ({ ...prev, scrollPosition: pos }));
    },
    updateTrendingTranslation: (url: string, item: SessionTranslationItem) => {
      const norm = normalizeUrl(url) || url;
      setLocalTrending((prev) => ({
        ...prev,
        translations: { ...prev.translations, [norm]: item, [url]: item },
      }));
    },
    search: localSearch,
    setSearch: setLocalSearch,
    nextSearchGeneration: () => {
      localGenRef.current += 1;
      return localGenRef.current;
    },
    saveSearchScroll: (pos: number) => {
      setLocalSearch((prev) => ({ ...prev, scrollPosition: pos }));
    },
    updateSearchTranslation: (url: string, item: SessionTranslationItem) => {
      const norm = normalizeUrl(url) || url;
      setLocalSearch((prev) => ({
        ...prev,
        translations: { ...prev.translations, [norm]: item, [url]: item },
      }));
    },
    add: localAdd,
    setAdd: setLocalAdd,
    nextAddGeneration: () => {
      localGenRef.current += 1;
      return localGenRef.current;
    },
    saveAddScroll: (pos: number) => {
      setLocalAdd((prev) => ({ ...prev, scrollPosition: pos }));
    },
    updateAddTranslation: (url: string, item: SessionTranslationItem) => {
      const norm = normalizeUrl(url) || url;
      setLocalAdd((prev) => ({
        ...prev,
        translations: { ...prev.translations, [norm]: item, [url]: item },
      }));
    },
    aiReadiness: localAiReadiness,
    aiReadinessLoading: localAiReadinessLoading,
    refreshAiReadiness: refreshLocalAiReadiness,
    aiRevision: localAiRevision,
    notifyAiConfigChanged: notifyLocalAiConfigChanged,
    getSessionPreview: (repoId?: number | null, url?: string | null) => {
      const key = computeSessionPreviewKey(repoId, url);
      const item = localAnalysisPreviews[key];
      return item ? item.envelope : null;
    },
    evictSessionPreview: (repoId?: number | null, url?: string | null) => {
      const key = computeSessionPreviewKey(repoId, url);
      setLocalAnalysisPreviews((prev) => {
        if (!(key in prev)) return prev;
        const next = { ...prev };
        delete next[key];
        return next;
      });
    },
    setSessionPreview: (envelope: AnalysisEnvelopeV1, repoId?: number | null, url?: string | null) => {
      const key = computeSessionPreviewKey(repoId, url);
      const serialized = JSON.stringify(envelope);
      const byteSize = (typeof Blob !== "undefined" ? new Blob([serialized]).size : serialized.length * 2) || serialized.length;
      setLocalAnalysisPreviews((prev) => {
        const next = { ...prev };
        let currentCount = Object.keys(next).length;
        let currentBytes = Object.values(next).reduce((acc, cur) => acc + cur.byteSize, 0);
        if (key in next) {
          currentBytes -= next[key].byteSize;
          currentCount -= 1;
        }
        while (currentCount >= SESSION_PREVIEW_MAX_COUNT || currentBytes + byteSize > SESSION_PREVIEW_MAX_BYTES) {
          const evictableKeys = Object.keys(next).filter((k) => k !== localActivePreviewKey && k !== key);
          if (evictableKeys.length === 0) {
            throw new Error("Session preview budget reached (maximum 64 previews or 8 MiB). Cannot cache new previews.");
          }
          evictableKeys.sort((a, b) => next[a].cachedAt - next[b].cachedAt);
          const oldest = evictableKeys[0];
          currentBytes -= next[oldest].byteSize;
          currentCount -= 1;
          delete next[oldest];
        }
        next[key] = {
          repoKey: key,
          github_repo_id: repoId,
          canonical_url: normalizeUrl(url) || url || "",
          envelope,
          cachedAt: Date.now(),
          byteSize,
        };
        return next;
      });
    },
    activePreviewKey: localActivePreviewKey,
    setActivePreviewKey: setLocalActivePreviewKey,
  };
}
