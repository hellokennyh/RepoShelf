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

  const renderContent = () => {
    switch (activeTab) {
      case "trending":
        return <TrendingPage />;
      case "search":
        return <SearchPage />;
      case "add":
        return <AddRepositoryPage />;
      case "library":
        return <LibraryPage />;
      case "settings":
        return <SettingsPage />;
      default:
        return <TrendingPage />;
    }
  };

  return (
    <Layout activeTab={activeTab} onTabChange={setActiveTab}>
      {renderContent()}
    </Layout>
  );
};

export default App;
