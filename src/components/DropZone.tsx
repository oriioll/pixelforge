import { ImagePlus } from "lucide-react";
import { Button } from "@/components/ui/button";

/**
 * Empty state of the workspace: drop images or pick them.
 *
 * @param props.onChoose - opens the native picker
 * @returns the empty state
 * @author oriioll
 * @since 2026-11-07
 */
export function DropZone({ onChoose }: { onChoose: () => void }) {
  return (
    <div className="flex h-full items-center justify-center p-10">
      <div className="flex w-full max-w-lg flex-col items-center gap-5 rounded-lg border border-dashed border-border-strong px-8 py-14 text-center">
        <ImagePlus className="size-7 text-muted" strokeWidth={1.5} />
        <div className="space-y-1.5">
          <h2 className="text-base font-medium">Arrastra tus fotos aquí</h2>
          <p className="text-muted">JPG o PNG. Todo se procesa en tu equipo, sin subir nada.</p>
        </div>
        <Button onClick={onChoose}>Elegir imágenes</Button>
      </div>
    </div>
  );
}
