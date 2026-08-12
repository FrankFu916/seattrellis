/**
 * Desktop bridge (PD-D14: unified file selection).
 *
 * The workbench runs in two environments that differ in how files are
 * picked:
 *  - Tauri desktop: native dialogs via `tauri-plugin-dialog` + two small
 *    shell commands (`read_user_file` / `write_user_file`);
 *  - Browser: `input[type=file]` + HTML5 drag-and-drop + the backend's
 *    trusted-root reader for typed paths.
 * (The legacy pywebview shell bridge from v1 was retired with M6.)
 *
 * Security red line (PD-D14): typed paths are relative to the backend's
 * trusted root only — absolute paths, `..` traversal and drive prefixes
 * are rejected client-side first, then again by the backend.
 */

/** True when running inside the Tauri WebView (bridge injected by the shell). */
export function isTauriDesktop(): boolean {
  return (
    typeof window !== "undefined" &&
    "__TAURI_INTERNALS__" in window &&
    Boolean(window.__TAURI_INTERNALS__)
  );
}

/** macOS detection for platform-adaptive chrome (design direction §5). */
export function isMacOS(): boolean {
  return (
    typeof navigator !== "undefined" &&
    /Macintosh|Mac OS X|MacIntel/i.test(navigator.userAgent) &&
    !/Windows|Linux/i.test(navigator.userAgent)
  );
}

/**
 * The platform's primary shortcut modifier, for hint text: `⌘` on macOS,
 * `Ctrl` elsewhere (design direction §5: browser cannot probe → Ctrl).
 */
export function platformModifierLabel(): string {
  return isMacOS() ? "⌘" : "Ctrl";
}

/**
 * Validate a manually typed path for the trusted-root reader.
 *
 * Mirrors the backend rules (`trusted_relative_path` in seattrellis-server):
 * non-empty, relative only, no `..` segments, no backslash separators.
 * Client-side check is input-level UX; the backend enforces the real
 * boundary.
 */
export function isTrustedRelativePath(raw: string): boolean {
  if (!raw || raw.includes("\0") || raw.includes("\\")) {
    return false;
  }
  if (raw.startsWith("/")) {
    return false;
  }
  if (/^[A-Za-z]:/.test(raw)) {
    return false;
  }
  const segments = raw.split("/");
  if (segments.some((segment) => segment === "..")) {
    return false;
  }
  return segments.some((segment) => segment !== "" && segment !== ".");
}

/**
 * Open the native file dialog (Tauri only) and return the chosen file as a
 * `File`, or `null` when the user cancels. Bytes cross the bridge through
 * the shell's `read_user_file` command; the path never leaves the shell.
 */
export async function pickFileWithDialog(
  extensions: string[],
  label: string,
): Promise<File | null> {
  if (!isTauriDesktop()) {
    return null;
  }
  const { open } = await import("@tauri-apps/plugin-dialog");
  const selected = await open({
    multiple: false,
    directory: false,
    filters: [{ name: label, extensions }],
  });
  if (typeof selected !== "string" || !selected) {
    return null;
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const bytes = (await invoke("read_user_file", {
    path: selected,
  })) as number[];
  const name = selected.split(/[\\/]/).pop() ?? "file";
  return new File([new Uint8Array(bytes)], name);
}

/**
 * Save `blob` through the native save dialog (Tauri only). Returns
 * `"saved"` / `"cancelled"`, or `"unavailable"` outside Tauri so callers
 * can fall back to the browser download.
 */
export async function saveBlobWithDialog(
  filename: string,
  blob: Blob,
): Promise<"saved" | "cancelled" | "unavailable"> {
  if (!isTauriDesktop()) {
    return "unavailable";
  }
  const { save } = await import("@tauri-apps/plugin-dialog");
  const path = await save({ defaultPath: filename });
  if (typeof path !== "string" || !path) {
    return "cancelled";
  }
  const { invoke } = await import("@tauri-apps/api/core");
  const bytes = new Uint8Array(await blob.arrayBuffer());
  await invoke("write_user_file", { path, content: Array.from(bytes) });
  return "saved";
}
