import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, waitFor, act, fireEvent } from "@testing-library/react";
import React from "react";
import { RepositoryCard } from "../components/RepositoryCard";
import { StarHistoryPreview } from "../components/StarHistoryPreview";
import {
  StarHistoryProvider,
  useStarHistory,
  MAX_STAR_HISTORY_CACHE_ENTRIES,
} from "../context/StarHistoryContext";
import { I18nProvider } from "../i18n/I18nContext";
import type { GitHubRepository } from "../types/github";

const mockInvoke = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

const sampleRepo: GitHubRepository = {
  github_repo_id: 12345,
  name: "react",
  github_url: "https://github.com/facebook/react",
  description: "The library for web and native user interfaces.",
  category: "Libraries & Frameworks",
  stars: 230000,
  license: "MIT",
  topics: ["javascript", "react", "ui"],
  pushed_at: "2026-03-01T12:00:00Z",
  releases: 150,
  languages: { JavaScript: 80, TypeScript: 20 },
};

const sampleSvg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 400"><text x="10" y="20">Star History</text></svg>`;

describe("Phase S6: Star History Frontend Integration Suite", () => {
  let createdUrls: string[] = [];
  let revokedUrls: string[] = [];
  let urlCounter = 0;
  let originalIntersectionObserver: typeof window.IntersectionObserver;

  beforeEach(() => {
    mockInvoke.mockReset();
    createdUrls = [];
    revokedUrls = [];
    urlCounter = 0;

    globalThis.URL.createObjectURL = vi.fn((_blob: Blob) => {
      urlCounter++;
      const url = `blob:http://localhost/test-uuid-${urlCounter}`;
      createdUrls.push(url);
      return url;
    });

    globalThis.URL.revokeObjectURL = vi.fn((url: string) => {
      revokedUrls.push(url);
    });

    originalIntersectionObserver = window.IntersectionObserver;
  });

  afterEach(() => {
    window.IntersectionObserver = originalIntersectionObserver;
    vi.restoreAllMocks();
  });

  it("renders RepositoryCard normally before star history preview is loaded", () => {
    // Keep preview hanging in loading state
    mockInvoke.mockReturnValue(new Promise(() => {}));

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <RepositoryCard repo={sampleRepo} />
        </StarHistoryProvider>
      </I18nProvider>
    );

    // Baseline metadata displays immediately
    expect(screen.getByText("react")).toBeInTheDocument();
    expect(screen.getByText("The library for web and native user interfaces.")).toBeInTheDocument();
    expect(screen.getByText("★")).toBeInTheDocument();
    expect(screen.getByText("230,000")).toBeInTheDocument();
    expect(screen.getByTestId("repo-releases")).toHaveTextContent("Releases 150");
    // Star history image is not yet rendered
    expect(screen.queryByTestId("star-history-image")).not.toBeInTheDocument();
  });

  it("renders contained preview image when supported embed is discovered and ready", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_star_history_preview") {
        expect(args).toEqual({ owner: "facebook", repo: "react" });
        return {
          status: "ready",
          svg_data: sampleSvg,
          embed_url: "https://api.star-history.com/svg?repos=facebook/react&type=Date",
          message: null,
        };
      }
      return undefined;
    });

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <RepositoryCard repo={sampleRepo} />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("star-history-image")).toBeInTheDocument();
    });

    const img = screen.getByTestId("star-history-image") as HTMLImageElement;
    expect(img.src).toContain("blob:http://localhost/test-uuid-1");
    expect(img.className).toContain("max-h-[120px]");
    expect(img.className).toContain("object-contain");
    expect(img.alt).toBe("facebook/react 的 Star History 图表"); // zh-Hans default in setup.ts
    expect(globalThis.URL.createObjectURL).toHaveBeenCalledTimes(1);
  });

  it("renders zero blank space when preview returns None (unsupported repository)", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_star_history_preview") {
        return {
          status: "none",
          svg_data: null,
          embed_url: null,
          message: null,
        };
      }
      return undefined;
    });

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <RepositoryCard repo={sampleRepo} />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_star_history_preview", {
        owner: "facebook",
        repo: "react",
      });
    });

    // Zero-height container is rendered, no preview image
    expect(screen.queryByTestId("star-history-image")).not.toBeInTheDocument();
    expect(screen.queryByTestId("star-history-preview")).not.toBeInTheDocument();
  });

  it("handles IPC preview error gracefully with zero layout shift and intact card functions", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_star_history_preview") {
        throw new Error("Network timeout while querying README");
      }
      return undefined;
    });

    const onFavoriteMock = vi.fn();

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <RepositoryCard repo={sampleRepo} onFavorite={onFavoriteMock} />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith("get_star_history_preview", {
        owner: "facebook",
        repo: "react",
      });
    });

    // Zero-height container, no crash
    expect(screen.queryByTestId("star-history-image")).not.toBeInTheDocument();

    // Favorite interaction still works flawlessly
    const favBtn = screen.getByTestId("favorite-btn");
    expect(favBtn).toBeInTheDocument();
    fireEvent.click(favBtn);
    expect(onFavoriteMock).toHaveBeenCalledWith(sampleRepo);
  });

  it("reuses in-memory cache and avoids redundant IPC calls across navigation / remount", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_star_history_preview") {
        return {
          status: "ready",
          svg_data: sampleSvg,
          embed_url: "https://api.star-history.com/svg?repos=facebook/react&type=Date",
          message: null,
        };
      }
      return undefined;
    });

    const { unmount } = render(
      <I18nProvider>
        <StarHistoryProvider>
          <RepositoryCard repo={sampleRepo} />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("star-history-image")).toBeInTheDocument();
    });

    const starCalls1 = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCalls1).toHaveLength(1);

    // Unmount and remount inside the SAME provider (simulating navigation)
    unmount();

    function NavHost() {
      const [show, setShow] = React.useState(true);
      return (
        <div>
          <button onClick={() => setShow(!show)} data-testid="toggle-btn">
            toggle
          </button>
          {show && <RepositoryCard repo={sampleRepo} />}
        </div>
      );
    }

    mockInvoke.mockClear();

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <NavHost />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId("star-history-image")).toBeInTheDocument();
    });
    // 1st mount in this provider made 1 star history call
    const starCalls2 = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCalls2).toHaveLength(1);

    // Toggle off (navigate away)
    fireEvent.click(screen.getByTestId("toggle-btn"));
    expect(screen.queryByTestId("star-history-image")).not.toBeInTheDocument();

    // Toggle on (navigate back)
    fireEvent.click(screen.getByTestId("toggle-btn"));
    await waitFor(() => {
      expect(screen.getByTestId("star-history-image")).toBeInTheDocument();
    });

    // Still only 1 star history call! Memory cache was reused!
    const starCalls3 = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCalls3).toHaveLength(1);
  });

  it("coalesces concurrent requests for the same repository into a single IPC call", async () => {
    let resolveIpc: (val: unknown) => void;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_star_history_preview") {
        return new Promise((res) => {
          resolveIpc = res;
        });
      }
      return Promise.resolve(undefined);
    });

    // Render 3 preview components for the same repository concurrently
    render(
      <I18nProvider>
        <StarHistoryProvider>
          <StarHistoryPreview owner="facebook" repo="react" />
          <StarHistoryPreview owner="facebook" repo="react" />
          <StarHistoryPreview owner="facebook" repo="react" />
        </StarHistoryProvider>
      </I18nProvider>
    );

    // Even though 3 components requested facebook/react simultaneously, exactly 1 get_star_history_preview call was made
    const starCalls = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCalls).toHaveLength(1);

    // Resolve the single in-flight promise
    await act(async () => {
      resolveIpc({
        status: "ready",
        svg_data: sampleSvg,
        embed_url: "https://api.star-history.com/svg?repos=facebook/react&type=Date",
        message: null,
      });
    });

    await waitFor(() => {
      const images = screen.getAllByTestId("star-history-image");
      expect(images).toHaveLength(3);
    });

    const finalStarCalls = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(finalStarCalls).toHaveLength(1);
  });

  it("enforces LRU eviction at MAX_STAR_HISTORY_CACHE_ENTRIES (32) and revokes evicted object URLs", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_star_history_preview") {
        const repo = args?.repo as string;
        return {
          status: "ready",
          svg_data: `<svg><text>${repo}</text></svg>`,
          embed_url: `https://api.star-history.com/svg?repos=test/${repo}&type=Date`,
          message: null,
        };
      }
      return undefined;
    });

    let contextApi: ReturnType<typeof useStarHistory> | null = null;

    function TestConsumer() {
      contextApi = useStarHistory();
      return null;
    }

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <TestConsumer />
        </StarHistoryProvider>
      </I18nProvider>
    );

    expect(contextApi).not.toBeNull();
    expect(MAX_STAR_HISTORY_CACHE_ENTRIES).toBe(32);

    // Load 32 distinct repositories sequentially
    for (let i = 1; i <= 32; i++) {
      await act(async () => {
        await contextApi!.requestPreview("owner", `repo-${i}`);
      });
    }

    expect(createdUrls).toHaveLength(32);
    expect(revokedUrls).toHaveLength(0);

    // Access repos 1 and 3..32 so repo-2 becomes the least recently used
    await act(async () => {
      contextApi!.getPreviewEntry("owner", "repo-1");
    });

    // Now request repo-33, exceeding limit of 32
    await act(async () => {
      await contextApi!.requestPreview("owner", "repo-33");
    });

    expect(createdUrls).toHaveLength(33);
    // LRU entry (repo-2) must have been evicted and its object URL revoked
    expect(revokedUrls).toHaveLength(1);
    expect(revokedUrls[0]).toBe(createdUrls[1]); // createdUrls[1] is repo-2's object URL

    // Querying evicted repo-2 returns undefined
    const evictedEntry = contextApi!.getPreviewEntry("owner", "repo-2");
    expect(evictedEntry).toBeUndefined();

    // repo-1 is still present
    const firstEntry = contextApi!.getPreviewEntry("owner", "repo-1");
    expect(firstEntry).toBeDefined();
  });

  it("revokes all object URLs when StarHistoryProvider unmounts", async () => {
    mockInvoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "get_star_history_preview") {
        const repo = args?.repo as string;
        return {
          status: "ready",
          svg_data: `<svg><text>${repo}</text></svg>`,
          embed_url: `https://api.star-history.com/svg?repos=test/${repo}&type=Date`,
          message: null,
        };
      }
      return undefined;
    });

    let contextApi: ReturnType<typeof useStarHistory> | null = null;

    function TestConsumer() {
      contextApi = useStarHistory();
      return null;
    }

    const { unmount } = render(
      <I18nProvider>
        <StarHistoryProvider>
          <TestConsumer />
        </StarHistoryProvider>
      </I18nProvider>
    );

    // Load 3 repositories
    await act(async () => {
      await contextApi!.requestPreview("owner", "repo-a");
      await contextApi!.requestPreview("owner", "repo-b");
      await contextApi!.requestPreview("owner", "repo-c");
    });

    expect(createdUrls).toHaveLength(3);
    expect(revokedUrls).toHaveLength(0);

    // Unmount provider
    unmount();

    // All 3 object URLs must be revoked
    expect(revokedUrls).toHaveLength(3);
    expect(revokedUrls).toEqual(expect.arrayContaining(createdUrls));
  });

  it("localizes alt text in en, zh-Hans, and zh-Hant", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_star_history_preview") {
        return {
          status: "ready",
          svg_data: sampleSvg,
          embed_url: "https://api.star-history.com/svg?repos=facebook/react&type=Date",
          message: null,
        };
      }
      return undefined;
    });

    // Test English
    const { unmount: unmountEn } = render(
      <I18nProvider initialLocale="en">
        <StarHistoryProvider>
          <StarHistoryPreview owner="facebook" repo="react" />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      const img = screen.getByTestId("star-history-image") as HTMLImageElement;
      expect(img.alt).toBe("Star History chart for facebook/react");
    });
    unmountEn();

    // Test zh-Hans
    const { unmount: unmountHans } = render(
      <I18nProvider initialLocale="zh-Hans">
        <StarHistoryProvider>
          <StarHistoryPreview owner="facebook" repo="react" />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      const img = screen.getByTestId("star-history-image") as HTMLImageElement;
      expect(img.alt).toBe("facebook/react 的 Star History 图表");
    });
    unmountHans();

    // Test zh-Hant
    const { unmount: unmountHant } = render(
      <I18nProvider initialLocale="zh-Hant">
        <StarHistoryProvider>
          <StarHistoryPreview owner="facebook" repo="react" />
        </StarHistoryProvider>
      </I18nProvider>
    );

    await waitFor(() => {
      const img = screen.getByTestId("star-history-image") as HTMLImageElement;
      expect(img.alt).toBe("facebook/react 的 Star History 圖表");
    });
    unmountHant();
  });

  it("waits for IntersectionObserver before requesting preview", async () => {
    let observerCallback: ((entries: IntersectionObserverEntry[]) => void) | null = null;
    const observedElements: Element[] = [];

    class MockObserver implements IntersectionObserver {
      readonly root: Element | Document | null = null;
      readonly rootMargin: string = "100px";
      readonly thresholds: ReadonlyArray<number> = [];
      constructor(cb: (entries: IntersectionObserverEntry[]) => void) {
        observerCallback = cb;
      }
      observe(target: Element) {
        observedElements.push(target);
      }
      unobserve() {}
      disconnect() {
        observedElements.length = 0;
      }
      takeRecords(): IntersectionObserverEntry[] {
        return [];
      }
    }

    window.IntersectionObserver = MockObserver as unknown as typeof window.IntersectionObserver;

    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_star_history_preview") {
        return {
          status: "ready",
          svg_data: sampleSvg,
          embed_url: "https://api.star-history.com/svg?repos=facebook/react&type=Date",
          message: null,
        };
      }
      return undefined;
    });

    render(
      <I18nProvider>
        <StarHistoryProvider>
          <StarHistoryPreview owner="facebook" repo="react" />
        </StarHistoryProvider>
      </I18nProvider>
    );

    // Initial state: element observed, but NOT intersecting -> star history IPC not called
    expect(observedElements.length).toBe(1);
    const starCallsBefore = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCallsBefore).toHaveLength(0);

    // Simulate non-intersecting event
    act(() => {
      observerCallback?.([
        {
          isIntersecting: false,
          target: observedElements[0],
        } as unknown as IntersectionObserverEntry,
      ]);
    });
    const starCallsNonIntersecting = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCallsNonIntersecting).toHaveLength(0);

    // Simulate intersecting event (entering viewport)
    act(() => {
      observerCallback?.([
        {
          isIntersecting: true,
          target: observedElements[0],
        } as unknown as IntersectionObserverEntry,
      ]);
    });

    // Star history IPC is now triggered
    const starCallsIntersecting = mockInvoke.mock.calls.filter(([c]) => c === "get_star_history_preview");
    expect(starCallsIntersecting).toHaveLength(1);
    expect(starCallsIntersecting[0][1]).toEqual({
      owner: "facebook",
      repo: "react",
    });

    await waitFor(() => {
      expect(screen.getByTestId("star-history-image")).toBeInTheDocument();
    });
  });
});
