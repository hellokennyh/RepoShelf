import React, { useContext, useLayoutEffect, useRef } from "react";
import { NavTab } from "../types/app";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";
import { DiscoverySessionContext } from "../context/DiscoverySessionContext";

interface LayoutProps {
  activeTab: NavTab;
  onTabChange: (tab: NavTab) => void;
  children: React.ReactNode;
}

export const Layout: React.FC<LayoutProps> = ({
  activeTab,
  onTabChange,
  children,
}) => {
  const sessionCtx = useContext(DiscoverySessionContext);
  const sessionCtxRef = useRef(sessionCtx);
  sessionCtxRef.current = sessionCtx;

  const mainRef = useRef<HTMLElement | null>(null);
  const prevTabRef = useRef<NavTab | null>(null);

  // Save outgoing tab scroll position before switching tab
  const handleTabChange = (nextTab: NavTab) => {
    if (sessionCtx && mainRef.current) {
      const currentScroll = mainRef.current.scrollTop;
      if (activeTab === "trending") {
        sessionCtx.saveTrendingScroll(currentScroll);
      } else if (activeTab === "search") {
        sessionCtx.saveSearchScroll(currentScroll);
      } else if (activeTab === "add") {
        sessionCtx.saveAddScroll(currentScroll);
      }
    }
    onTabChange(nextTab);
  };

  // Restore incoming tab scroll position after render ONLY on genuine tab transitions
  useLayoutEffect(() => {
    const mainEl = mainRef.current;
    if (!mainEl) return;

    if (prevTabRef.current !== activeTab) {
      const ctx = sessionCtxRef.current;
      if (ctx) {
        let targetScroll = 0;
        if (activeTab === "trending") {
          targetScroll = ctx.trending.scrollPosition;
        } else if (activeTab === "search") {
          targetScroll = ctx.search.scrollPosition;
        } else if (activeTab === "add") {
          targetScroll = ctx.add.scrollPosition;
        }

        mainEl.scrollTop = targetScroll;
      }
      prevTabRef.current = activeTab;
    }

    return () => {
      // Save outgoing tab scroll position upon tab unmount / switch
      const ctx = sessionCtxRef.current;
      if (ctx && mainEl) {
        const currentScroll = mainEl.scrollTop;
        if (activeTab === "trending") {
          ctx.saveTrendingScroll(currentScroll);
        } else if (activeTab === "search") {
          ctx.saveSearchScroll(currentScroll);
        } else if (activeTab === "add") {
          ctx.saveAddScroll(currentScroll);
        }
      }
    };
  }, [activeTab]);

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-slate-900 text-slate-100">
      <Sidebar activeTab={activeTab} onTabChange={handleTabChange} />
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        <Header activeTab={activeTab} />
        <main
          ref={mainRef}
          className="flex-1 overflow-y-auto p-6 max-w-[1920px] w-full mx-auto"
        >
          {children}
        </main>
      </div>
    </div>
  );
};
