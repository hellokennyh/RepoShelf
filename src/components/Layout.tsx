import React from "react";
import { NavTab } from "../types/app";
import { Sidebar } from "./Sidebar";
import { Header } from "./Header";

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
  return (
    <div className="flex h-screen w-screen overflow-hidden bg-slate-900 text-slate-100">
      <Sidebar activeTab={activeTab} onTabChange={onTabChange} />
      <div className="flex flex-col flex-1 min-w-0 overflow-hidden">
        <Header activeTab={activeTab} />
        <main className="flex-1 overflow-y-auto p-6 max-w-[1920px] w-full mx-auto">
          {children}
        </main>
      </div>
    </div>
  );
};
