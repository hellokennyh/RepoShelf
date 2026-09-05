import React, { useState } from "react";
import { NavTab } from "./types/app";
import { Layout } from "./components/Layout";
import { TrendingPage } from "./pages/TrendingPage";
import { SearchPage } from "./pages/SearchPage";
import { AddRepositoryPage } from "./pages/AddRepositoryPage";
import { LibraryPage } from "./pages/LibraryPage";
import { SettingsPage } from "./pages/SettingsPage";

export const App: React.FC = () => {
  const [activeTab, setActiveTab] = useState<NavTab>("trending");
  const [openFavoriteId, setOpenFavoriteId] = useState<number | null>(null);

  const handleNavigateToFavorite = (favId: number) => {
    setOpenFavoriteId(favId);
    setActiveTab("library");
  };

  const renderContent = () => {
    switch (activeTab) {
      case "trending":
        return <TrendingPage onNavigateToFavorite={handleNavigateToFavorite} />;
      case "search":
        return <SearchPage onNavigateToFavorite={handleNavigateToFavorite} />;
      case "add":
        return <AddRepositoryPage onNavigateToFavorite={handleNavigateToFavorite} />;
      case "library":
        return (
          <LibraryPage
            initialOpenFavoriteId={openFavoriteId}
            onClearInitialFavoriteId={() => setOpenFavoriteId(null)}
          />
        );
      case "settings":
        return <SettingsPage />;
      default:
        return <TrendingPage onNavigateToFavorite={handleNavigateToFavorite} />;
    }
  };

  return (
    <Layout activeTab={activeTab} onTabChange={setActiveTab}>
      {renderContent()}
    </Layout>
  );
};

export default App;
