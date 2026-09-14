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
  const mainRef = useRef<HTMLElement | null>(null);
  const prevTabRef = useRef<NavTab>(activeTab);

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

  // Restore incoming tab scroll position after render
  useLayoutEffect(() => {
    if (!mainRef.current) return;

    if (sessionCtx) {
      let targetScroll = 0;
      if (activeTab === "trending") {
        targetScroll = sessionCtx.trending.scrollPosition;
      } else if (activeTab === "search") {
        targetScroll = sessionCtx.search.scrollPosition;
      } else if (activeTab === "add") {
        targetScroll = sessionCtx.add.scrollPosition;
      }

      mainRef.current.scrollTop = targetScroll;
    }
    prevTabRef.current = activeTab;
  }, [activeTab, sessionCtx]);

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
