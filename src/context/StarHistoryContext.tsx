import React, {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import { getStarHistoryPreview } from "../services/tauri";
import { StarHistoryCacheEntry } from "../types/starHistory";

export const MAX_STAR_HISTORY_CACHE_ENTRIES = 32;

export interface StarHistoryContextValue {
  getPreviewEntry: (owner: string, repo: string) => StarHistoryCacheEntry | undefined;
  requestPreview: (owner: string, repo: string) => Promise<void>;
  invalidatePreview: (owner: string, repo: string) => void;
  subscribe: (owner: string, repo: string, listener: (entry?: StarHistoryCacheEntry) => void) => () => void;
}

function makeKey(owner: string, repo: string): string {
  return `${owner.trim().toLowerCase()}/${repo.trim().toLowerCase()}`;
}

const fallbackContext: StarHistoryContextValue = {
  getPreviewEntry: () => undefined,
  requestPreview: async () => {},
  invalidatePreview: () => {},
  subscribe: () => () => {},
};

export const StarHistoryContext = createContext<StarHistoryContextValue | null>(null);

export const StarHistoryProvider: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => {
  const [, setVersion] = useState(0);

  // In-memory session cache
  const cacheRef = useRef<Map<string, StarHistoryCacheEntry>>(new Map());

  // In-flight request coalescing map
  const inFlightRef = useRef<Map<string, Promise<void>>>(new Map());

  // Listeners for individual repository key updates
  const listenersRef = useRef<Map<string, Set<(entry?: StarHistoryCacheEntry) => void>>>(new Map());

  // Clean up all object URLs on provider unmount
  useEffect(() => {
    return () => {
      for (const entry of cacheRef.current.values()) {
        if (entry.objectUrl) {
          try {
            URL.revokeObjectURL(entry.objectUrl);
          } catch {
            // ignore
          }
        }
      }
      cacheRef.current.clear();
      listenersRef.current.clear();
      inFlightRef.current.clear();
    };
  }, []);

  const notifyListeners = useCallback((key: string, entry?: StarHistoryCacheEntry) => {
    const set = listenersRef.current.get(key);
    if (set) {
      for (const listener of set) {
        try {
          listener(entry);
        } catch {
          // ignore listener errors
        }
      }
    }
  }, []);

  const evictOldestIfNeeded = useCallback(() => {
    while (cacheRef.current.size > MAX_STAR_HISTORY_CACHE_ENTRIES) {
      let oldestKey: string | null = null;
      let oldestTime = Infinity;

      for (const [k, entry] of cacheRef.current.entries()) {
        if (entry.lastAccessed < oldestTime) {
          oldestTime = entry.lastAccessed;
          oldestKey = k;
        }
      }

      if (oldestKey) {
        const evicted = cacheRef.current.get(oldestKey);
        if (evicted?.objectUrl) {
          try {
            URL.revokeObjectURL(evicted.objectUrl);
          } catch {
            // ignore
          }
        }
        cacheRef.current.delete(oldestKey);
        notifyListeners(oldestKey, undefined);
      } else {
        break;
      }
    }
  }, [notifyListeners]);

  const getPreviewEntry = useCallback((owner: string, repo: string): StarHistoryCacheEntry | undefined => {
    const key = makeKey(owner, repo);
    const entry = cacheRef.current.get(key);
    if (entry) {
      entry.lastAccessed = Date.now();
    }
    return entry;
  }, []);

  const subscribe = useCallback((owner: string, repo: string, listener: (entry?: StarHistoryCacheEntry) => void) => {
    const key = makeKey(owner, repo);
    let set = listenersRef.current.get(key);
    if (!set) {
      set = new Set();
      listenersRef.current.set(key, set);
    }
    set.add(listener);
    return () => {
      const s = listenersRef.current.get(key);
      if (s) {
        s.delete(listener);
        if (s.size === 0) {
          listenersRef.current.delete(key);
        }
      }
    };
  }, []);

  const invalidatePreview = useCallback((owner: string, repo: string) => {
    const key = makeKey(owner, repo);
    const existing = cacheRef.current.get(key);
    if (existing?.objectUrl) {
      try {
        URL.revokeObjectURL(existing.objectUrl);
      } catch {
        // ignore
      }
    }
    cacheRef.current.delete(key);
    notifyListeners(key, undefined);
    setVersion((v) => v + 1);
  }, [notifyListeners]);

  const requestPreview = useCallback(async (owner: string, repo: string): Promise<void> => {
    const key = makeKey(owner, repo);

    // If already in cache (ready, none, error), no duplicate work needed
    const existing = cacheRef.current.get(key);
    if (existing && existing.status !== "unknown") {
      existing.lastAccessed = Date.now();
      return;
    }

    // Coalesce duplicate in-flight requests for the same repository
    if (inFlightRef.current.has(key)) {
      return inFlightRef.current.get(key);
    }

    // Set loading state in cache
    const loadingEntry: StarHistoryCacheEntry = {
      status: "loading",
      lastAccessed: Date.now(),
    };
    cacheRef.current.set(key, loadingEntry);
    evictOldestIfNeeded();
    notifyListeners(key, loadingEntry);
    setVersion((v) => v + 1);

    const promise = (async () => {
      try {
        const res = await getStarHistoryPreview(owner, repo);

        // Guard against undefined or malformed responses in tests or mock environments
        if (!res || typeof res !== "object" || !("status" in res)) {
          return;
        }

        let objectUrl: string | undefined;
        if (res.status === "ready" && res.svg_data) {
          try {
            const blob = new Blob([res.svg_data], { type: "image/svg+xml" });
            objectUrl = URL.createObjectURL(blob);
          } catch {
            // fallback if Blob / createObjectURL fails
          }
        }

        let finalEntry: StarHistoryCacheEntry;
        if (res.status === "ready") {
          finalEntry = {
            status: "ready",
            svgData: res.svg_data,
            embedUrl: res.embed_url,
            objectUrl,
            lastAccessed: Date.now(),
          };
        } else if (res.status === "none") {
          finalEntry = {
            status: "none",
            lastAccessed: Date.now(),
          };
        } else {
          finalEntry = {
            status: "error",
            errorMessage: res.message,
            lastAccessed: Date.now(),
          };
        }

        cacheRef.current.set(key, finalEntry);
        evictOldestIfNeeded();
        notifyListeners(key, finalEntry);
        setVersion((v) => v + 1);
      } catch (err: unknown) {
        const errorEntry: StarHistoryCacheEntry = {
          status: "error",
          errorMessage: err instanceof Error ? err.message : String(err),
          lastAccessed: Date.now(),
        };
        cacheRef.current.set(key, errorEntry);
        evictOldestIfNeeded();
        notifyListeners(key, errorEntry);
        setVersion((v) => v + 1);
      } finally {
        inFlightRef.current.delete(key);
      }
    })();

    inFlightRef.current.set(key, promise);
    return promise;
  }, [evictOldestIfNeeded, notifyListeners]);

  return (
    <StarHistoryContext.Provider
      value={{
        getPreviewEntry,
        requestPreview,
        invalidatePreview,
        subscribe,
      }}
    >
      {children}
    </StarHistoryContext.Provider>
  );
};

export function useStarHistory(): StarHistoryContextValue {
  const ctx = useContext(StarHistoryContext);
  return ctx || fallbackContext;
}

export function useStarHistoryEntry(owner: string, repo: string): StarHistoryCacheEntry | undefined {
  const { getPreviewEntry, subscribe } = useStarHistory();
  const [entry, setEntry] = useState<StarHistoryCacheEntry | undefined>(() =>
    getPreviewEntry(owner, repo)
  );

  useEffect(() => {
    setEntry(getPreviewEntry(owner, repo));
    return subscribe(owner, repo, (updated) => {
      setEntry(updated);
    });
  }, [owner, repo, getPreviewEntry, subscribe]);

  return entry;
}
