import { afterEach, describe, expect, it, vi } from "vitest";
import { chooseClassSaveTarget, writeClassFile } from "./classFiles";

afterEach(() => {
  delete (window as unknown as { showSaveFilePicker?: unknown }).showSaveFilePicker;
  vi.unstubAllGlobals();
});
describe("verified editable class persistence", () => {
  it("writes and closes the user-selected browser file before reporting saved", async () => {
    let close!: () => void;
    const closing = new Promise<void>((done) => { close = done; });
    const writer = { write: vi.fn().mockResolvedValue(undefined), close: vi.fn(() => closing), abort: vi.fn() };
    const handle = { createWritable: vi.fn().mockResolvedValue(writer) };
    const picker = vi.fn().mockResolvedValue(handle);
    (window as unknown as { showSaveFilePicker: unknown }).showSaveFilePicker = picker;
    const target = await chooseClassSaveTarget("Class.seattrellis.json");
    expect(target).toMatchObject({ kind: "browser" });
    let finished = false;
    const save = writeClassFile(target!, "Class.seattrellis.json", new Blob(["class document"])).then((outcome) => { finished = true; return outcome; });
    await vi.waitFor(() => expect(writer.close).toHaveBeenCalledOnce());
    expect(finished).toBe(false);
    close();
    expect(await save).toBe("saved");
    expect(await chooseClassSaveTarget("Class.seattrellis.json", target)).toBe(target);
    expect(picker).toHaveBeenCalledOnce();
  });

  it("aborts a failed write and never calls it saved", async () => {
    const writer = { write: vi.fn().mockRejectedValue(new Error("disk full")), close: vi.fn(), abort: vi.fn().mockResolvedValue(undefined) };
    const target = { kind: "browser" as const, handle: { createWritable: async () => writer } };
    await expect(writeClassFile(target, "Class.json", new Blob())).rejects.toThrow("disk full");
    expect(writer.abort).toHaveBeenCalledOnce();
    expect(writer.close).not.toHaveBeenCalled();
  });

  it("treats the user cancelling a destination as cancellation", async () => {
    (window as unknown as { showSaveFilePicker: unknown }).showSaveFilePicker = vi.fn().mockRejectedValue(new DOMException("cancelled", "AbortError"));
    expect(await chooseClassSaveTarget("Class.json")).toBeNull();
  });
});
