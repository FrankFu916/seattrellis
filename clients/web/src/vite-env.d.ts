/// <reference types="vite/client" />

declare global {
  interface Window {
    /** Injected by the Tauri shell (D14 capability detection). */
    __TAURI_INTERNALS__?: unknown;
  }
}

export {};
