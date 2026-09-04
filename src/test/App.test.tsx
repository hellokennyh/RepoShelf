import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import App from "../App";

// Mock Tauri invoke to simulate IPC in test environment
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockImplementation((cmd: string) => {
    if (cmd === "get_app_info") {
      return Promise.resolve({
        name: "RepoShelf",
        phase: "Phase 0",
      });
    }
    if (cmd === "get_settings") {
      return Promise.resolve({
        schema_version: 1,
        github: {
          default_search_limit: 20,
          default_trending_period: "daily",
        },
        ai: {
          enabled: false,
          provider: null,
          base_url: null,
          model: null,
        },
      });
    }
    return Promise.reject(new Error(`Unhandled command: ${cmd}`));
  }),
}));

describe("RepoShelf App Shell Smoke Tests", () => {
  it("renders the RepoShelf application brand", () => {
    render(<App />);
    const brandElements = screen.getAllByText("RepoShelf");
    expect(brandElements.length).toBeGreaterThan(0);
  });

  it("renders all five primary navigation entries", () => {
    render(<App />);
    expect(screen.getByTestId("nav-trending")).toHaveTextContent("热门项目");
    expect(screen.getByTestId("nav-search")).toHaveTextContent("搜索项目");
    expect(screen.getByTestId("nav-add")).toHaveTextContent("添加项目");
    expect(screen.getByTestId("nav-library")).toHaveTextContent("项目浏览");
    expect(screen.getByTestId("nav-settings")).toHaveTextContent("设置");
  });

  it("defaults to Trending page", () => {
    render(<App />);
    expect(screen.getByText("GitHub Trending")).toBeInTheDocument();
    expect(screen.getByTestId("trending-idle-state")).toBeInTheDocument();
  });

  it("switches to Search page on navigation click", () => {
    render(<App />);
    fireEvent.click(screen.getByTestId("nav-search"));
    expect(screen.getByText("GitHub Repository Search")).toBeInTheDocument();
  });

  it("switches to Add page on navigation click", () => {
    render(<App />);
    fireEvent.click(screen.getByTestId("nav-add"));
    expect(screen.getByText("Add GitHub Repository")).toBeInTheDocument();
  });

  it("switches to Library page on navigation click", () => {
    render(<App />);
    fireEvent.click(screen.getByTestId("nav-library"));
    expect(screen.getByText("Local Repository Library")).toBeInTheDocument();
  });

  it("switches to Settings page and displays IPC verification section", async () => {
    render(<App />);
    fireEvent.click(screen.getByTestId("nav-settings"));
    expect(screen.getByText("Settings & Preferences")).toBeInTheDocument();
    expect(screen.getByText(/Tauri IPC 通信验证/)).toBeInTheDocument();

    const appName = await screen.findByTestId("ipc-app-name");
    expect(appName).toHaveTextContent("RepoShelf");

    const appPhase = await screen.findByTestId("ipc-app-phase");
    expect(appPhase).toHaveTextContent("Phase 0");
  });
});
