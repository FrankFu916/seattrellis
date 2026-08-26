import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ErrorBoundary } from "./ErrorBoundary";

function ThrowingChild(): never {
  throw new Error("boom-test");
}

describe("ErrorBoundary (W6: top-level crash guard)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("renders its children while nothing crashes", () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    render(
      <ErrorBoundary>
        <p>workbench</p>
      </ErrorBoundary>,
    );

    expect(screen.getByText("workbench")).toBeInTheDocument();
    expect(screen.queryByTestId("error-boundary")).toBeNull();
    expect(consoleError).not.toHaveBeenCalled();
  });

  it("shows the bilingual fallback plus the error summary on a crash", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    vi.stubGlobal("navigator", { language: "en-US" });
    render(
      <ErrorBoundary>
        <ThrowingChild />
      </ErrorBoundary>,
    );

    const boundary = screen.getByTestId("error-boundary");
    expect(boundary).toHaveTextContent("Something went wrong");
    expect(boundary).toHaveTextContent("界面发生错误，请刷新重试");
    expect(screen.getByTestId("error-boundary-summary")).toHaveTextContent(
      "boom-test",
    );
    expect(consoleError).toHaveBeenCalled();
  });

  it("uses Chinese as the primary fallback language for zh systems", () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    vi.stubGlobal("navigator", { language: "zh-CN" });
    render(
      <ErrorBoundary>
        <ThrowingChild />
      </ErrorBoundary>,
    );

    const boundary = screen.getByTestId("error-boundary");
    expect(boundary).toHaveTextContent("界面发生错误，请刷新重试。");
    expect(boundary).toHaveTextContent("Something went wrong");
  });

  it("reloads the page from the fallback action", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const reload = vi.fn();
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...window.location, reload },
    });
    const user = userEvent.setup();
    render(
      <ErrorBoundary>
        <ThrowingChild />
      </ErrorBoundary>,
    );

    await user.click(screen.getByRole("button"));
    expect(reload).toHaveBeenCalled();
  });
});
