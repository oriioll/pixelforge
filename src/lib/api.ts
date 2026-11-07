import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { EnhanceRequest, EnhanceResult, EnvInfo, HistoryEntry, ImageInfo, ProgressEvent } from "./types";

/** Extensions the Rust decoder can read. */
export const IMAGE_EXTENSIONS = ["jpg", "jpeg", "png"] as const;

/**
 * True when running inside the Tauri webview (false in a plain browser tab).
 *
 * @returns whether the Tauri IPC bridge is available
 * @author oriioll
 * @since 2026-11-07
 */
export const isTauri = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Keeps only the paths whose extension the app can decode.
 *
 * @param paths - absolute file paths
 * @returns supported image paths
 * @author oriioll
 * @since 2026-11-07
 */
export const onlyImages = (paths: string[]): string[] =>
  paths.filter((p) => (IMAGE_EXTENSIONS as readonly string[]).includes(p.split(".").pop()?.toLowerCase() ?? ""));

/**
 * Converts a local path into a URL the webview may display (asset protocol).
 * The Rust side must have allowed the file first (inspect_image / enhance do).
 *
 * @param path - absolute file path
 * @returns a displayable URL
 * @author oriioll
 * @since 2026-11-07
 */
export const fileUrl = (path: string): string => convertFileSrc(path);

export const api = {
  environment: () => invoke<EnvInfo>("environment"),
  inspectImage: (path: string) => invoke<ImageInfo>("inspect_image", { path }),
  /** @throws the string "cancelled" when the job was cancelled, otherwise an error message. */
  enhance: (req: EnhanceRequest) => invoke<EnhanceResult>("enhance", { req }),
  cancel: (id: string) => invoke<void>("cancel_job", { id }),
  historyList: () => invoke<HistoryEntry[]>("history_list"),
  historyRemove: (id: string) => invoke<void>("history_remove", { id }),
  historyClear: () => invoke<void>("history_clear"),
  revealInFolder: (path: string) => revealItemInDir(path),
};

/**
 * Opens the native file picker for images.
 *
 * @returns chosen paths, empty if cancelled
 * @author oriioll
 * @since 2026-11-07
 */
export async function pickImages(): Promise<string[]> {
  const res = await open({ multiple: true, filters: [{ name: "Imágenes", extensions: [...IMAGE_EXTENSIONS] }] });
  return Array.isArray(res) ? res : res ? [res] : [];
}

/**
 * Opens the native folder picker.
 *
 * @returns the folder path or null if cancelled
 * @author oriioll
 * @since 2026-11-07
 */
export async function pickFolder(): Promise<string | null> {
  const res = await open({ directory: true });
  return typeof res === "string" ? res : null;
}

/**
 * Subscribes to job progress events.
 *
 * @param handler - called for every progress event
 * @returns a function that unsubscribes
 * @author oriioll
 * @since 2026-11-07
 */
export const onProgress = (handler: (e: ProgressEvent) => void): Promise<UnlistenFn> =>
  listen<ProgressEvent>("pixelforge:progress", (e) => handler(e.payload));

/**
 * Subscribes to files dropped on the window.
 *
 * @param handlers - drag state and drop callbacks
 * @returns a function that unsubscribes
 * @author oriioll
 * @since 2026-11-07
 */
export function onFileDrop(handlers: { onHover: (hovering: boolean) => void; onDrop: (paths: string[]) => void }): Promise<UnlistenFn> {
  return getCurrentWebview().onDragDropEvent((event) => {
    const p = event.payload;
    if (p.type === "enter" || p.type === "over") handlers.onHover(true);
    else if (p.type === "leave") handlers.onHover(false);
    else if (p.type === "drop") {
      handlers.onHover(false);
      handlers.onDrop(p.paths);
    }
  });
}
