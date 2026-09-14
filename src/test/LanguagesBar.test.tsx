import { render, screen } from "@testing-library/react";
import { describe, it, expect } from "vitest";
import { LanguagesBar } from "../components/LanguagesBar";

describe("LanguagesBar Component", () => {
  it("renders null or undefined as 'Languages —' in compact mode", () => {
    const { rerender } = render(<LanguagesBar languages={null} mode="compact" />);
    expect(screen.getByTestId("languages-unknown")).toBeInTheDocument();
    expect(screen.getByText("Languages —")).toBeInTheDocument();

    rerender(<LanguagesBar languages={undefined} mode="compact" />);
    expect(screen.getByTestId("languages-unknown")).toBeInTheDocument();
    expect(screen.getByText("Languages —")).toBeInTheDocument();
  });

  it("renders empty object {} as '无代码语言统计' in both compact and full modes", () => {
    const { rerender } = render(<LanguagesBar languages={{}} mode="compact" />);
    expect(screen.getByTestId("languages-empty")).toBeInTheDocument();
    expect(screen.getByText("无代码语言统计")).toBeInTheDocument();

    rerender(<LanguagesBar languages={{}} mode="full" />);
    expect(screen.getByTestId("languages-empty")).toBeInTheDocument();
    expect(screen.getByText("无代码语言统计")).toBeInTheDocument();
  });

  it("renders single 100% language correctly", () => {
    render(<LanguagesBar languages={{ Rust: 100 }} mode="compact" />);
    expect(screen.getByTestId("languages-bar-container")).toBeInTheDocument();
    expect(screen.getByTestId("languages-segmented-bar")).toBeInTheDocument();
    expect(screen.getByText("Rust")).toBeInTheDocument();
    expect(screen.getByText("100.0%")).toBeInTheDocument();
  });

  it("renders multiple languages sorted descending by percentage", () => {
    render(
      <LanguagesBar
        languages={{
          HTML: 5.2,
          TypeScript: 70.1,
          Rust: 24.7,
        }}
        mode="full"
      />
    );
    expect(screen.getByTestId("languages-bar-container")).toBeInTheDocument();

    const legendItems = screen.getByTestId("languages-legend");
    // TypeScript (70.1%) should appear before Rust (24.7%), then HTML (5.2%)
    expect(legendItems.textContent).toMatch(/TypeScript.*70\.1%.*Rust.*24\.7%.*HTML.*5\.2%/);
  });

  it("truncates with +count in compact mode when more than 3 languages", () => {
    render(
      <LanguagesBar
        languages={{
          Python: 50.0,
          Go: 20.0,
          Rust: 15.0,
          JavaScript: 10.0,
          Shell: 5.0,
        }}
        mode="compact"
      />
    );
    expect(screen.getByTestId("languages-bar-container")).toBeInTheDocument();
    expect(screen.getByTestId("languages-more-count")).toHaveTextContent("+2");
    expect(screen.getByText("Python")).toBeInTheDocument();
    expect(screen.getByText("Go")).toBeInTheDocument();
    expect(screen.getByText("Rust")).toBeInTheDocument();
    expect(screen.queryByText("JavaScript")).not.toBeInTheDocument();
  });
});
