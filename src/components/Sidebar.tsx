import React from "react";
import { NavTab, APP_VERSION_DISPLAY } from "../types/app";
import { useI18n } from "../i18n/I18nContext";

interface SidebarProps {
  activeTab: NavTab;
  onTabChange: (tab: NavTab) => void;
}

export const Sidebar: React.FC<SidebarProps> = ({ activeTab, onTabChange }) => {
  const { t } = useI18n();

  const navItems: { id: NavTab; label: string; icon: string }[] = [
    { id: "trending", label: t("nav.trending"), icon: "🔥" },
    { id: "search", label: t("nav.search"), icon: "🔎" },
    { id: "add", label: t("nav.add"), icon: "➕" },
    { id: "library", label: t("nav.library"), icon: "📚" },
    { id: "settings", label: t("nav.settings"), icon: "⚙️" },
  ];

  return (
    <aside className="w-56 bg-slate-950 border-r border-slate-800 flex flex-col justify-between shrink-0 select-none">
      <div>
        {/* Brand */}
        <div className="h-14 flex items-center px-5 border-b border-slate-800 gap-2.5">
          <div className="w-7 h-7 rounded bg-indigo-600 flex items-center justify-center text-white font-bold text-sm shadow">
            RS
          </div>
          <div>
            <span className="font-semibold text-slate-100 tracking-tight text-base block leading-none">
              RepoShelf
            </span>
            <span className="text-[10px] text-slate-400 font-mono" data-testid="app-version">
              {APP_VERSION_DISPLAY}
            </span>
          </div>
        </div>

        {/* Navigation items */}
        <nav className="p-3 space-y-1">
          {navItems.map((item) => {
            const isActive = activeTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => onTabChange(item.id)}
                className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-md text-sm font-medium transition-colors ${
                  isActive
                    ? "bg-indigo-600/20 text-indigo-400 border border-indigo-500/30"
                    : "text-slate-400 hover:bg-slate-900 hover:text-slate-200 border border-transparent"
                }`}
                data-testid={`nav-${item.id}`}
              >
                <span className="text-base">{item.icon}</span>
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>
      </div>

      {/* Footer info */}
      <div className="p-4 border-t border-slate-800 text-[11px] text-slate-500">
        <p>Local-First Desktop</p>
      </div>
    </aside>
  );
};
