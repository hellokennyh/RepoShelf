import React from "react";
import { NavTab } from "../types/app";
import { useI18n } from "../i18n/I18nContext";

interface HeaderProps {
  activeTab: NavTab;
}

export const Header: React.FC<HeaderProps> = ({ activeTab }) => {
  const { t } = useI18n();

  const tabTitles: Record<NavTab, string> = {
    trending: t("nav.trending"),
    search: t("nav.search"),
    add: t("nav.add"),
    library: t("nav.library"),
    settings: t("nav.settings"),
  };

  return (
    <header className="h-14 bg-slate-900/80 border-b border-slate-800 px-6 flex items-center justify-between backdrop-blur-sm">
      <div className="flex items-center gap-2">
        <span className="text-sm font-medium text-slate-300">RepoShelf</span>
        <span className="text-slate-600">/</span>
        <span className="text-sm font-semibold text-slate-100">{tabTitles[activeTab]}</span>
      </div>

      <div className="flex items-center gap-3" />
    </header>
  );
};
