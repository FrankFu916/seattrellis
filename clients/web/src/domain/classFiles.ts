import { isTauriDesktop } from "./desktop";

/** Only user-selected files are writable. No renderer-chosen absolute path. */
type BrowserHandle = {
  createWritable(): Promise<{ write(data: Blob): Promise<void>; close(): Promise<void>; abort(): Promise<void> }>;
};
export type ClassSaveTarget = { kind: "desktop"; path: string } | { kind: "browser"; handle: BrowserHandle } | { kind: "download" };

export async function chooseClassSaveTarget(filename: string, existing?: ClassSaveTarget | null): Promise<ClassSaveTarget | null> {
  if (existing && existing.kind !== "download") return existing;
  if (isTauriDesktop()) {
    const { invoke } = await import("@tauri-apps/api/core");
    const path = await invoke<string | null>("pick_save_file", { filename });
    return path ? { kind: "desktop", path } : null;
  }
  const picker = (window as Window & {
    showSaveFilePicker?: (options: unknown) => Promise<BrowserHandle>;
  }).showSaveFilePicker;
  if (picker) {
    try {
      const handle = await picker({ suggestedName: filename,
        types: [{ description: "SeatTrellis class", accept: { "application/json": [".seattrellis.json"] } }] });
      return { kind: "browser", handle };
    } catch (error) {
      if (error instanceof DOMException && error.name === "AbortError") return null;
      throw error;
    }
  }
  return { kind: "download" };
}

export async function writeClassFile(target: ClassSaveTarget, filename: string, blob: Blob): Promise<"saved" | "downloaded"> {
  if (target.kind === "desktop") {
    const { invoke } = await import("@tauri-apps/api/core");
    await invoke("write_user_file", { path: target.path, content: Array.from(new Uint8Array(await blob.arrayBuffer())) });
    return "saved";
  }
  if (target.kind === "browser") {
    const writable = await target.handle.createWritable();
    try {
      await writable.write(blob);
      await writable.close();
    } catch (error) {
      await writable.abort().catch(() => undefined);
      throw error;
    }
    return "saved";
  }
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  // Keep the URL alive until the browser has claimed the download.
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  return "downloaded";
}
