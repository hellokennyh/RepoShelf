import React from "react";
import { NavTab } from "../types/app";

interface HeaderProps {
  activeTab: NavTab;
}

const tabTitles: Record<NavTab, string> = {
  trending: "热门项目",
  search: "搜索项目",
  add: "添加项目",
  library: "项目浏览",
  settings: "设置",
};

export const Header: React.FC<HeaderProps> = ({ activeTab }) => {
  return (
    <header className="h-14 bg-slate-900/80 border-b border-slate-800 px-6 flex items-center justify-between backdrop-blur-sm">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-slate-300">RepoShelf</span>
        <span className="text-slate-600">/</span>
        <span className="text-sm font-semibold text-slate-100">{tabTitles[activeTab]}</span>
      </div>

      <div className="flex items-center gap-3">
        <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-500/10 text-emerald-400 border border-emerald-500/20">
          ● Phase 0 Baseline
        </span>
      </div>
    </header>
  );
};
