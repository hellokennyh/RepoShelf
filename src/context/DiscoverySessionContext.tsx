import React, { createContext, useContext, useState, useCallback, useRef } from "react";
import {
  TrendingPeriod,
  TrendingResponse,
} from "../types/trending";
import {
  GitHubRepository,
  LastActivePreset,
} from "../types/github";
import { ArticleSummary } from "../types/article";
import { checkFavoriteIdentitiesBatch } from "../services/tauri";

export interface SessionTranslationItem {
  text: string;
  target_locale: string;
  source_hash: string;
  action: "translated" | "unchanged";
  status?: "valid" | "stale" | "corrupt";
}

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
}

export const DiscoverySessionContext = createContext<DiscoverySessionContextValue | null>(null);

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
  const localGenRef = useRef(0);

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
  };
}
