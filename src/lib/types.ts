/** Types mirroring the Rust side (serde camelCase / snake_case enums). */

export type Preset = "football" | "mirror_selfie";
export type PresetMode = Preset | "auto";
export type OutputSize = "2k" | "4k" | "x2";
export type OutputFormat = "png" | "jpeg";

export type Stage = "loading" | "restoring" | "upscaling" | "faces" | "tone" | "sharpening" | "saving" | "done";

export interface Suggestion {
  preset: Preset;
  confidence: number;
  reason: "pitch_detected" | "no_pitch_portrait" | "no_pitch";
}

export interface ImageInfo {
  path: string;
  name: string;
  width: number;
  height: number;
  suggestion: Suggestion;
}

export interface ModelInfo {
  id: string;
  label: string;
  installed: boolean;
  denoise_slider: boolean;
}

export interface EnvInfo {
  engineFound: boolean;
  engineError: string | null;
  models: ModelInfo[];
  facesAvailable: boolean;
}

export interface EnhanceRequest {
  id: string;
  input: string;
  outDir: string | null;
  preset: Preset;
  intensity: number;
  model: string;
  denoiseStrength: number;
  size: OutputSize;
  format: OutputFormat;
  jpegQuality: number;
  faces: boolean;
  faceFidelity: number | null;
  tile: number;
}

export interface EnhanceResult {
  id: string;
  output: string;
  width: number;
  height: number;
  durationMs: number;
  warnings: string[];
}

export interface ProgressEvent {
  id: string;
  stage: Stage;
  fraction: number;
}

export interface HistoryEntry {
  id: string;
  input: string;
  output: string;
  preset: Preset;
  intensity: number;
  size: OutputSize;
  model: string;
  width: number;
  height: number;
  createdAt: number;
  durationMs: number;
}
