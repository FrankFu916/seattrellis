import { Component, type ErrorInfo, type ReactNode } from "react";

type ErrorBoundaryProps = {
  children: ReactNode;
};

type ErrorBoundaryState = {
  error: Error | null;
};

function prefersChinese(): boolean {
  return (
    typeof navigator !== "undefined" &&
    typeof navigator.language === "string" &&
    navigator.language.toLowerCase().startsWith("zh")
  );
}

/**
 * Top-level crash guard (W6): a render error anywhere in the workbench must
 * not blank the page silently. The fallback is bilingual because the locale
 * state lives inside the crashed tree; it shows the error summary so a bug
 * report stays possible and offers the only safe recovery — a reload.
 */
export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("Workbench crashed", error, info.componentStack);
  }

  render(): ReactNode {
    const { error } = this.state;
    if (!error) {
      return this.props.children;
    }
    const zh = prefersChinese();
    return (
      <div
        role="alert"
        data-testid="error-boundary"
        style={{
          display: "flex",
          flexDirection: "column",
          gap: "12px",
          alignItems: "flex-start",
          justifyContent: "center",
          minHeight: "100vh",
          padding: "24px",
          fontFamily: "system-ui, sans-serif",
        }}
      >
        <h1 style={{ fontSize: "1.25rem", margin: 0 }}>
          {zh
            ? "界面发生错误，请刷新重试。"
            : "Something went wrong. Please refresh and try again."}
        </h1>
        <p style={{ margin: 0 }}>
          {zh
            ? "Something went wrong. Please refresh and try again."
            : "界面发生错误，请刷新重试。"}
        </p>
        <p style={{ margin: 0 }}>
          {zh
            ? "你的本地班级文件没有受到影响。"
            : "Your local class files are not affected."}
        </p>
        <pre
          data-testid="error-boundary-summary"
          style={{ whiteSpace: "pre-wrap", maxWidth: "100%" }}
        >
          {`${error.name}: ${error.message}`}
        </pre>
        <button type="button" onClick={() => window.location.reload()}>
          {zh ? "刷新页面" : "Reload"}
        </button>
      </div>
    );
  }
}
