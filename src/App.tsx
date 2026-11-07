import { Download, FolderClock, ImageIcon, X } from "lucide-react";
import { useState } from "react";
import { CompareViewer } from "@/components/CompareViewer";
import { ControlsPanel } from "@/components/ControlsPanel";
import { DropZone } from "@/components/DropZone";
import { HistoryPanel } from "@/components/HistoryPanel";
import { QueueList } from "@/components/QueueList";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { useWorkspace } from "@/hooks/useWorkspace";
import { cn } from "@/lib/utils";

type Tab = "queue" | "history";

/**
 * Application shell: queue/history on the left, viewer in the middle, settings on the right.
 *
 * @returns the app
 * @author oriioll
 * @since 2026-11-07
 */
export default function App() {
  const ws = useWorkspace();
  const [tab, setTab] = useState<Tab>("queue");
  const pending = ws.items.filter((i) => i.status !== "done").length;
  const done = ws.items.filter((i) => i.status === "done").length;
  const overall = ws.items.length === 0 ? 0 : ws.items.reduce((sum, i) => sum + (i.status === "done" ? 1 : i.status === "running" ? i.fraction : 0), 0) / ws.items.length;

  return (
    <div className="grid h-full grid-cols-[280px_minmax(0,1fr)_320px] grid-rows-[44px_minmax(0,1fr)_40px]">
      <header className="col-span-3 flex items-center justify-between border-b border-border px-4">
        <div className="flex items-center gap-2">
          <img src="/icon.svg" alt="" className="size-5" />
          <span className="font-semibold tracking-tight">pixelforge</span>
        </div>
        <div className="flex items-center gap-2 text-xs text-muted">
          <span className={cn("size-1.5 rounded-full", ws.env?.engineFound ? "bg-ok" : "bg-danger")} />
          {ws.env === null ? "Comprobando motor…" : ws.env.engineFound ? "Real-ESRGAN listo" : "Motor no encontrado"}
        </div>
      </header>

      <aside className="flex min-h-0 flex-col border-r border-border">
        <div role="tablist" className="flex h-10 shrink-0 border-b border-border">
          {([["queue", "Cola", ImageIcon], ["history", "Historial", FolderClock]] as const).map(([id, label, Icon]) => (
            <button key={id} role="tab" aria-selected={tab === id} onClick={() => setTab(id)} className={cn("flex flex-1 items-center justify-center gap-1.5 border-b-2 text-xs transition-colors", tab === id ? "border-floodlight text-foreground" : "border-transparent text-muted hover:text-foreground")}>
              <Icon className="size-3.5" />{label}
              {id === "queue" && ws.items.length > 0 && <span className="font-mono tabular-nums">{ws.items.length}</span>}
            </button>
          ))}
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto">{tab === "queue" ? <QueueList ws={ws} /> : <HistoryPanel ws={ws} />}</div>
        {tab === "queue" && (
          <div className="flex gap-2 border-t border-border p-3">
            <Button variant="secondary" size="sm" className="flex-1" onClick={() => void ws.chooseImages()}>Añadir imágenes</Button>
            {done > 0 && <Button variant="ghost" size="sm" onClick={ws.clearFinished}>Quitar listas</Button>}
          </div>
        )}
      </aside>

      <main className="relative min-h-0 min-w-0">
        {ws.viewer ? <CompareViewer before={ws.viewer.before} after={ws.viewer.after} title={ws.viewer.title} /> : <DropZone onChoose={() => void ws.chooseImages()} />}
        {ws.notice && (
          <div role="status" className="absolute inset-x-4 bottom-4 flex items-start justify-between gap-3 rounded-md border border-border-strong bg-surface-2 px-3 py-2">
            <span>{ws.notice}</span>
            <button aria-label="Cerrar aviso" onClick={() => ws.setNotice(null)} className="text-muted hover:text-foreground"><X className="size-3.5" /></button>
          </div>
        )}
        {ws.dragging && (
          <div className="pointer-events-none absolute inset-0 z-20 flex items-center justify-center border-2 border-dashed border-floodlight bg-background/85">
            <p className="flex items-center gap-2 text-base"><Download className="size-4" />Suelta las imágenes para añadirlas</p>
          </div>
        )}
      </main>

      <aside className="flex min-h-0 flex-col border-l border-border">
        <div className="min-h-0 flex-1"><ControlsPanel ws={ws} /></div>
        <div className="border-t border-border p-3">
          {ws.running ? (
            <Button variant="danger" className="w-full" onClick={ws.cancel}>Cancelar</Button>
          ) : (
            <Button className="w-full" disabled={pending === 0 || !ws.env?.engineFound} onClick={() => void ws.run()}>
              {pending === 0 ? "Nada que mejorar" : pending === 1 ? "Mejorar 1 imagen" : `Mejorar ${pending} imágenes`}
            </Button>
          )}
        </div>
      </aside>

      <footer className="col-span-3 flex items-center gap-4 border-t border-border px-4 text-xs text-muted">
        <span className="font-mono tabular-nums">{done}/{ws.items.length}</span>
        <Progress value={overall} label="Progreso total" className="max-w-xs" />
        <span className="ml-auto">Todo se procesa en local, sin conexión.</span>
      </footer>
    </div>
  );
}
