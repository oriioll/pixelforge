import { Minus, Plus } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { fileUrl } from "@/lib/api";
import { cn } from "@/lib/utils";

type Mode = "slide" | "before" | "after";
const MAX_ZOOM = 8;

interface CompareViewerProps {
  before: string;
  after: string | null;
  title: string;
}

/**
 * Before/after comparison with a draggable divider, wheel zoom and panning, so the result
 * can be checked at pixel level like zooming on a phone.
 *
 * @param props.before - path of the original
 * @param props.after - path of the result, or null while it does not exist
 * @param props.title - file name shown in the toolbar
 * @returns the viewer
 * @author oriioll
 * @since 2026-11-07
 */
export function CompareViewer({ before, after, title }: CompareViewerProps) {
  const [mode, setMode] = useState<Mode>("slide");
  const [split, setSplit] = useState(50);
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });
  const box = useRef<HTMLDivElement>(null);
  const drag = useRef<{ kind: "split" | "pan"; x: number; y: number; vx: number; vy: number } | null>(null);
  const hasAfter = after !== null;
  const effective: Mode = hasAfter ? mode : "before";

  useEffect(() => setView({ scale: 1, x: 0, y: 0 }), [before, after]);

  const zoomAt = useCallback((factor: number, cx: number, cy: number) => {
    setView((v) => {
      const scale = Math.min(MAX_ZOOM, Math.max(1, v.scale * factor));
      if (scale === 1) return { scale, x: 0, y: 0 };
      const k = scale / v.scale;
      return { scale, x: cx - (cx - v.x) * k, y: cy - (cy - v.y) * k };
    });
  }, []);

  // Wheel needs a non-passive listener to stop the page from scrolling.
  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoomAt(e.deltaY < 0 ? 1.15 : 1 / 1.15, e.clientX - r.left - r.width / 2, e.clientY - r.top - r.height / 2);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomAt]);

  const onPointerDown = (e: React.PointerEvent, kind: "split" | "pan") => {
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { kind, x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const d = drag.current;
    const r = box.current?.getBoundingClientRect();
    if (!d || !r) return;
    if (d.kind === "split") setSplit(Math.min(100, Math.max(0, ((e.clientX - r.left) / r.width) * 100)));
    else setView((v) => ({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }));
  };
  const endDrag = () => (drag.current = null);

  const img = (src: string, alt: string) => (
    <img
      src={fileUrl(src)}
      alt={alt}
      draggable={false}
      className="absolute inset-0 size-full object-contain"
      style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`, transformOrigin: "center" }}
    />
  );

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex h-11 shrink-0 items-center justify-between gap-3 border-b border-border px-4">
        <span className="truncate text-xs text-muted" title={title}>{title}</span>
        <div className="flex items-center gap-3">
          <Segmented<Mode>
            label="Vista"
            value={effective}
            onValueChange={setMode}
            className="w-64"
            options={[
              { value: "slide", label: "Comparar", disabled: !hasAfter },
              { value: "before", label: "Original" },
              { value: "after", label: "Mejorada", disabled: !hasAfter },
            ]}
          />
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" aria-label="Alejar" onClick={() => zoomAt(1 / 1.5, 0, 0)}><Minus className="size-3.5" /></Button>
            <button className="w-12 font-mono text-xs text-muted tabular-nums hover:text-foreground" onClick={() => setView({ scale: 1, x: 0, y: 0 })} title="Restablecer zoom">
              {Math.round(view.scale * 100)}%
            </button>
            <Button variant="ghost" size="icon" aria-label="Acercar" onClick={() => zoomAt(1.5, 0, 0)}><Plus className="size-3.5" /></Button>
          </div>
        </div>
      </div>

      <div
        ref={box}
        className={cn("relative min-h-0 flex-1 overflow-hidden bg-[repeating-conic-gradient(#121213_0%_25%,#0f0f10_0%_50%)] bg-[length:16px_16px]", view.scale > 1 ? "cursor-grab active:cursor-grabbing" : "cursor-default")}
        onPointerDown={(e) => view.scale > 1 && onPointerDown(e, "pan")}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onDoubleClick={() => setView({ scale: 1, x: 0, y: 0 })}
      >
        {hasAfter && effective !== "before" && img(after, "Imagen mejorada")}
        {effective === "before" && img(before, "Imagen original")}
        {effective === "slide" && (
          <>
            <div className="absolute inset-0" style={{ clipPath: `inset(0 ${100 - split}% 0 0)` }}>{img(before, "Imagen original")}</div>
            <span className="pointer-events-none absolute top-3 left-3 rounded-sm bg-background/80 px-1.5 py-0.5 text-[11px] text-muted">Original</span>
            <span className="pointer-events-none absolute top-3 right-3 rounded-sm bg-background/80 px-1.5 py-0.5 text-[11px] text-floodlight">Mejorada</span>
            <div className="absolute inset-y-0 w-px bg-floodlight" style={{ left: `${split}%` }} />
            <div
              role="slider"
              tabIndex={0}
              aria-label="Posición del comparador"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(split)}
              className="absolute top-1/2 z-10 flex h-10 w-5 -translate-x-1/2 -translate-y-1/2 cursor-ew-resize items-center justify-center rounded-md border border-floodlight bg-background"
              style={{ left: `${split}%` }}
              onPointerDown={(e) => { e.stopPropagation(); onPointerDown(e, "split"); }}
              onKeyDown={(e) => {
                if (e.key === "ArrowLeft") setSplit((s) => Math.max(0, s - 2));
                if (e.key === "ArrowRight") setSplit((s) => Math.min(100, s + 2));
              }}
            >
              <span className="h-4 w-px bg-floodlight" />
            </div>
          </>
        )}
      </div>
    </div>
  );
}
