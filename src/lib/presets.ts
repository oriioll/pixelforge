import type { Preset, PresetMode, Stage, Suggestion } from "./types";

export interface PresetMeta {
  id: Preset;
  label: string;
  /** One-line description of what the preset protects and improves. */
  summary: string;
}

export const PRESETS: PresetMeta[] = [
  { id: "football", label: "Fútbol", summary: "Césped, lluvia y cielo oscuro. Nitidez en el jugador, el público desenfocado se respeta." },
  { id: "mirror_selfie", label: "Selfie de espejo", summary: "Piel con textura, ojos nítidos, luz y balance de blancos corregidos." },
];

export const presetLabel = (p: Preset): string => PRESETS.find((m) => m.id === p)?.label ?? p;

/** Nominal intensity (matches `NOMINAL_INTENSITY` in the core crate). */
export const DEFAULT_INTENSITY = 60;

/**
 * Resolves the preset an item will use.
 *
 * @param mode - the user's selection ("auto" follows the suggestion)
 * @param suggestion - suggestion for the item
 * @returns the effective preset
 * @author oriioll
 * @since 2026-11-07
 */
export const effectivePreset = (mode: PresetMode, suggestion: Suggestion): Preset => (mode === "auto" ? suggestion.preset : mode);

/**
 * Explains why a preset was suggested.
 *
 * @param s - the suggestion
 * @returns Spanish text for the UI
 * @author oriioll
 * @since 2026-11-07
 */
export function suggestionText(s: Suggestion): string {
  const name = presetLabel(s.preset);
  const how = s.confidence >= 0.7 ? "Detectado" : "Probable";
  return s.reason === "pitch_detected" ? `${how}: ${name} (se ve césped)` : `${how}: ${name} (no se ve césped)`;
}

export const STAGE_LABEL: Record<Stage, string> = {
  loading: "Leyendo imagen",
  restoring: "Quitando artefactos JPEG",
  upscaling: "Escalando con Real-ESRGAN",
  faces: "Restaurando caras",
  tone: "Luz y color",
  sharpening: "Nitidez selectiva",
  saving: "Guardando",
  done: "Listo",
};
