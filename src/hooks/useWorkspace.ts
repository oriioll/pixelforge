import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api, isTauri, onFileDrop, onProgress, onlyImages, pickFolder, pickImages } from "@/lib/api";
import { DEFAULT_INTENSITY, effectivePreset } from "@/lib/presets";
import type { EnhanceResult, EnvInfo, HistoryEntry, ImageInfo, OutputFormat, OutputSize, Preset, PresetMode, Stage } from "@/lib/types";

export interface Settings {
  mode: PresetMode;
  /** Intensity (0-100) remembered per preset. */
  intensity: Record<Preset, number>;
  size: OutputSize;
  model: string;
  denoise: number;
  faces: boolean;
  /** 0-100; null uses the preset's default fidelity. */
  fidelity: number | null;
  format: OutputFormat;
  outDir: string | null;
}

export const DEFAULT_SETTINGS: Settings = {
  mode: "auto",
  intensity: { football: DEFAULT_INTENSITY, mirror_selfie: DEFAULT_INTENSITY },
  size: "2k",
  model: "realesrgan-x4plus",
  denoise: 70,
  faces: true,
  fidelity: null,
  format: "jpeg",
  outDir: null,
};

export type ItemStatus = "idle" | "running" | "done" | "error" | "cancelled";

export interface Item {
  id: string;
  info: ImageInfo;
  status: ItemStatus;
  stage: Stage | null;
  fraction: number;
  result: EnhanceResult | null;
  error: string | null;
}

/** What the compare viewer shows. */
export interface ViewerSource {
  before: string;
  after: string | null;
  title: string;
}

let counter = 0;
const newId = () => `job-${Date.now().toString(36)}-${(counter++).toString(36)}`;

/**
 * All state and actions of the app: queue, settings, history and the batch runner.
 *
 * @returns the workspace state and its actions
 * @author oriioll
 * @since 2026-11-07
 */
export function useWorkspace() {
  const [env, setEnv] = useState<EnvInfo | null>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [historyView, setHistoryView] = useState<HistoryEntry | null>(null);
  const [history, setHistory] = useState<HistoryEntry[]>([]);
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [running, setRunning] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const settingsRef = useRef(settings);
  const itemsRef = useRef(items);
  const cancelRequested = useRef(false);
  settingsRef.current = settings;
  itemsRef.current = items;

  const patchItem = useCallback((id: string, patch: Partial<Item>) => setItems((all) => all.map((i) => (i.id === id ? { ...i, ...patch } : i))), []);
  const refreshHistory = useCallback(() => api.historyList().then(setHistory).catch(() => undefined), []);

  const addPaths = useCallback(async (paths: string[]) => {
    const fresh = onlyImages(paths).filter((p) => !itemsRef.current.some((i) => i.info.path === p));
    if (fresh.length === 0) {
      if (paths.length > 0) setNotice("Solo se admiten imágenes JPG y PNG.");
      return;
    }
    const added: Item[] = [];
    for (const path of fresh) {
      try {
        const info = await api.inspectImage(path);
        added.push({ id: newId(), info, status: "idle", stage: null, fraction: 0, result: null, error: null });
      } catch (e) {
        setNotice(`No se pudo leer ${path.split(/[\\/]/).pop()}: ${String(e)}`);
      }
    }
    if (added.length === 0) return;
    setItems((all) => [...all, ...added]);
    setHistoryView(null);
    setSelectedId((cur) => cur ?? added[0].id);
  }, []);

  // Environment, history, progress events and window-level drag & drop.
  useEffect(() => {
    if (!isTauri()) {
      setNotice("Abre pixelforge desde la aplicación de escritorio: en el navegador no hay motor de imagen.");
      return;
    }
    api.environment().then((e) => {
      setEnv(e);
      setSettings((s) => (e.models.find((m) => m.id === s.model)?.installed ? s : { ...s, model: e.models.find((m) => m.installed)?.id ?? s.model, faces: s.faces && e.facesAvailable }));
    });
    refreshHistory();
    const unlisten = [
      onProgress((e) => patchItem(e.id, { stage: e.stage, fraction: e.fraction })),
      onFileDrop({ onHover: setDragging, onDrop: (p) => void addPaths(p) }),
    ];
    return () => void unlisten.forEach((u) => u.then((fn) => fn()));
  }, [addPaths, patchItem, refreshHistory]);

  const update = useCallback(<K extends keyof Settings>(key: K, value: Settings[K]) => setSettings((s) => ({ ...s, [key]: value })), []);

  const selected = useMemo(() => items.find((i) => i.id === selectedId) ?? null, [items, selectedId]);
  const selectedPreset = selected ? effectivePreset(settings.mode, selected.info.suggestion) : settings.mode === "auto" ? "football" : settings.mode;
  const setIntensity = useCallback((value: number) => setSettings((s) => ({ ...s, intensity: { ...s.intensity, [selectedPreset]: value } })), [selectedPreset]);

  const viewer: ViewerSource | null = useMemo(() => {
    if (historyView) return { before: historyView.input, after: historyView.output, title: historyView.output.split(/[\\/]/).pop() ?? "" };
    if (selected) return { before: selected.info.path, after: selected.result?.output ?? null, title: selected.info.name };
    return null;
  }, [historyView, selected]);

  const run = useCallback(async () => {
    const cfg = settingsRef.current;
    cancelRequested.current = false;
    setRunning(true);
    setNotice(null);
    const queue = itemsRef.current.filter((i) => i.status !== "done" && i.status !== "running");
    for (const item of queue) {
      if (cancelRequested.current) break;
      const preset = effectivePreset(cfg.mode, item.info.suggestion);
      patchItem(item.id, { status: "running", stage: "loading", fraction: 0, error: null });
      try {
        const result = await api.enhance({
          id: item.id,
          input: item.info.path,
          outDir: cfg.outDir,
          preset,
          intensity: cfg.intensity[preset] / 100,
          model: cfg.model,
          denoiseStrength: cfg.denoise / 100,
          size: cfg.size,
          format: cfg.format,
          jpegQuality: 95,
          faces: cfg.faces,
          faceFidelity: cfg.fidelity === null ? null : cfg.fidelity / 100,
          tile: 0,
        });
        patchItem(item.id, { status: "done", stage: "done", fraction: 1, result });
        if (result.warnings.includes("faces_unavailable")) setNotice("No se restauraron caras: faltan los modelos de CodeFormer en esta instalación.");
      } catch (e) {
        const msg = String(e);
        patchItem(item.id, msg === "cancelled" ? { status: "cancelled", fraction: 0 } : { status: "error", error: msg });
      }
    }
    setRunning(false);
    void refreshHistory();
  }, [patchItem, refreshHistory]);

  const cancel = useCallback(() => {
    cancelRequested.current = true;
    itemsRef.current.filter((i) => i.status === "running").forEach((i) => void api.cancel(i.id));
  }, []);

  const removeItem = useCallback((id: string) => {
    setItems((all) => all.filter((i) => i.id !== id));
    setSelectedId((cur) => (cur === id ? null : cur));
  }, []);

  const clearFinished = useCallback(() => {
    setItems((all) => all.filter((i) => i.status !== "done"));
    setSelectedId((cur) => (itemsRef.current.find((i) => i.id === cur)?.status === "done" ? null : cur));
  }, []);

  return {
    env, items, selected, selectedPreset, viewer, history, settings, running, dragging, notice,
    update, setIntensity, run, cancel, removeItem, clearFinished, refreshHistory, setNotice,
    addPaths,
    select: (id: string) => { setHistoryView(null); setSelectedId(id); },
    viewHistory: (entry: HistoryEntry | null) => setHistoryView(entry),
    chooseImages: async () => addPaths(await pickImages()),
    chooseFolder: async () => { const dir = await pickFolder(); if (dir) update("outDir", dir); },
  };
}

export type Workspace = ReturnType<typeof useWorkspace>;
