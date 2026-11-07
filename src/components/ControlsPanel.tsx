import { FolderOpen } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { Slider } from "@/components/ui/slider";
import { Switch } from "@/components/ui/switch";
import { PRESETS, presetLabel, suggestionText } from "@/lib/presets";
import type { Workspace } from "@/hooks/useWorkspace";
import type { OutputFormat, OutputSize, PresetMode } from "@/lib/types";
import { cn } from "@/lib/utils";

/** CodeFormer fidelity defaults per preset (matches `face_fidelity` in the core crate). */
const DEFAULT_FIDELITY = { football: 80, mirror_selfie: 90 } as const;

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2.5 border-b border-border px-4 py-4">
      <h3 className="text-xs font-medium text-muted">{title}</h3>
      {children}
    </section>
  );
}

function Row({ label, value, children }: { label: string; value?: string; children: React.ReactNode }) {
  return (
    <div className="space-y-1.5">
      <div className="flex items-baseline justify-between">
        <span>{label}</span>
        {value && <span className="font-mono text-xs text-muted tabular-nums">{value}</span>}
      </div>
      {children}
    </div>
  );
}

/**
 * Right panel: preset, intensity, output and face options.
 *
 * @param props.ws - the workspace state
 * @returns the settings panel
 * @author oriioll
 * @since 2026-11-07
 */
export function ControlsPanel({ ws }: { ws: Workspace }) {
  const { settings: s, update, env, selected } = ws;
  const general = env?.models.find((m) => m.id === "realesr-general-x4v3");
  const current = env?.models.find((m) => m.id === s.model);
  const fidelity = s.fidelity ?? DEFAULT_FIDELITY[ws.selectedPreset];
  const modes: { id: PresetMode; label: string; summary: string }[] = [
    { id: "auto", label: "Automático", summary: "Elige el preset según cada foto." },
    ...PRESETS,
  ];

  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <Section title="Preset">
          <div role="radiogroup" aria-label="Preset" className="space-y-1.5">
            {modes.map((m) => (
              <button
                key={m.id}
                role="radio"
                aria-checked={s.mode === m.id}
                onClick={() => update("mode", m.id)}
                className={cn("w-full rounded-md border px-3 py-2 text-left transition-colors", s.mode === m.id ? "border-floodlight bg-surface" : "border-border hover:border-border-strong")}
              >
                <span className="block font-medium">{m.label}</span>
                <span className="block text-xs text-muted">{m.summary}</span>
              </button>
            ))}
          </div>
          {selected && s.mode === "auto" && <p className="text-xs text-muted">{suggestionText(selected.info.suggestion)}</p>}
          {selected && s.mode !== "auto" && s.mode !== selected.info.suggestion.preset && selected.info.suggestion.confidence >= 0.7 && (
            <p className="text-xs text-muted">
              Esta foto parece de {presetLabel(selected.info.suggestion.preset).toLowerCase()}.{" "}
              <button className="text-floodlight underline underline-offset-2" onClick={() => update("mode", selected.info.suggestion.preset)}>Usar ese preset</button>
            </p>
          )}
        </Section>

        <Section title={`Intensidad · ${presetLabel(ws.selectedPreset)}`}>
          <Row label="Fuerza del efecto" value={`${s.intensity[ws.selectedPreset]}`}>
            <Slider label="Intensidad" value={s.intensity[ws.selectedPreset]} onValueChange={ws.setIntensity} />
          </Row>
        </Section>

        <Section title="Salida">
          <Segmented<OutputSize> label="Tamaño de salida" value={s.size} onValueChange={(v) => update("size", v)} options={[{ value: "2k", label: "2K" }, { value: "4k", label: "4K" }, { value: "x2", label: "Original ×2" }]} />
          <Segmented<OutputFormat> label="Formato" value={s.format} onValueChange={(v) => update("format", v)} options={[{ value: "jpeg", label: "JPG 95" }, { value: "png", label: "PNG" }]} />
          <div className="flex items-center gap-2">
            <Button variant="secondary" size="sm" onClick={() => void ws.chooseFolder()}><FolderOpen className="size-3.5" />Carpeta de salida</Button>
            {s.outDir && <button className="truncate text-xs text-muted hover:text-foreground" title={`${s.outDir} · clic para volver a la carpeta original`} onClick={() => update("outDir", null)}>{s.outDir}</button>}
          </div>
          {!s.outDir && <p className="text-xs text-muted">Se guarda junto a la foto original.</p>}
        </Section>

        <Section title="Modelo de escalado">
          <Segmented<string>
            label="Modelo"
            value={s.model}
            onValueChange={(v) => update("model", v)}
            options={[
              { value: "realesrgan-x4plus", label: "x4plus", disabled: !env?.models.find((m) => m.id === "realesrgan-x4plus")?.installed, title: "Fotos reales, el más fiel" },
              { value: "realesr-general-x4v3", label: "general x4v3", disabled: !general?.installed, title: general?.installed ? "Más rápido, limpia mejor imágenes muy comprimidas" : "No incluido en esta instalación" },
            ]}
          />
          {current?.denoise_slider && (
            <Row label="Fuerza de denoise" value={`${s.denoise}`}>
              <Slider label="Fuerza de denoise" value={s.denoise} onValueChange={(v) => update("denoise", v)} />
            </Row>
          )}
          {general && !general.installed && <p className="text-xs text-muted">general x4v3 no está en esta instalación; se usa x4plus.</p>}
        </Section>

        <Section title="Caras">
          <div className="flex items-center justify-between">
            <span>Restaurar con CodeFormer</span>
            <Switch label="Restaurar caras con CodeFormer" checked={s.faces && !!env?.facesAvailable} disabled={!env?.facesAvailable} onCheckedChange={(v) => update("faces", v)} />
          </div>
          {!env?.facesAvailable && <p className="text-xs text-muted">Los modelos de caras no están en esta instalación.</p>}
          <Row label="Fidelidad" value={`${fidelity}`}>
            <Slider label="Fidelidad de la cara" value={fidelity} disabled={!s.faces || !env?.facesAvailable} onValueChange={(v) => update("fidelity", v)} />
          </Row>
          <p className="text-xs text-muted">Más alta conserva mejor la expresión original.</p>
        </Section>
      </div>
    </div>
  );
}
