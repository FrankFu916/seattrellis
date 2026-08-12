/// <reference types="vite/client" />

declare global {
  type DesktopRosterFile = {
    name: string;
    content_base64: string;
    content_type: string;
  };

  type DesktopBridgeApi = {
    open_roster_file?: () => Promise<DesktopRosterFile | null>;
    open_recent_file?: (path: string) => Promise<DesktopRosterFile | null>;
    list_recent_files?: () => Promise<Array<{ name: string; path: string }>>;
    save_export_file?: (
      filename: string,
      contentBase64: string,
    ) => Promise<{ saved: boolean; name: string }>;
    choose_project_folder?: () => Promise<string | null>;
  };

  interface Window {
    pywebview?: {
      api?: DesktopBridgeApi;
    };
    /** Injected by the Tauri shell (D14 capability detection). */
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
