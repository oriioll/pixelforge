import { AlertCircle, Check, X } from "lucide-react";
import { Progress } from "@/components/ui/progress";
import { fileUrl } from "@/lib/api";
import { formatDims, formatDuration } from "@/lib/format";
import { effectivePreset, presetLabel, STAGE_LABEL } from "@/lib/presets";
import type { Item, Workspace } from "@/hooks/useWorkspace";
import { cn } from "@/lib/utils";

/**
 * Queue of images with per-item status and progress.
 *
 * @param props.ws - the workspace state
 * @returns the list
 * @author oriioll
 * @since 2026-11-07
 */
export function QueueList({ ws }: { ws: Workspace }) {
  if (ws.items.length === 0) return <p className="p-4 text-muted">Aún no hay imágenes en la cola.</p>;
  return (
    <ul className="divide-y divide-border">
      {ws.items.map((item) => (
        <QueueRow key={item.id} item={item} selected={ws.selected?.id === item.id} ws={ws} />
      ))}
    </ul>
  );
}

function QueueRow({ item, selected, ws }: { item: Item; selected: boolean; ws: Workspace }) {
  const preset = effectivePreset(ws.settings.mode, item.info.suggestion);
  return (
    <li className={cn("group relative flex gap-3 px-4 py-3", selected && "bg-surface")}>
      {selected && <span className="absolute inset-y-0 left-0 w-0.5 bg-floodlight" />}
      <button className="flex min-w-0 flex-1 gap-3 text-left" onClick={() => ws.select(item.id)}>
        <img src={fileUrl(item.info.path)} alt="" loading="lazy" className="size-10 shrink-0 rounded-sm bg-surface-2 object-cover" />
        <span className="min-w-0 flex-1 space-y-1">
          <span className="block truncate" title={item.info.name}>{item.info.name}</span>
          {item.status === "running" ? (
            <span className="block space-y-1">
              <Progress value={item.fraction} label={`Progreso de ${item.info.name}`} />
              <span className="block truncate text-xs text-muted">{item.stage ? STAGE_LABEL[item.stage] : ""}</span>
            </span>
          ) : item.status === "done" && item.result ? (
            <span className="flex items-center gap-1.5 text-xs text-ok">
              <Check className="size-3" />
              <span className="font-mono tabular-nums">{formatDims(item.result.width, item.result.height)}</span>
              <span className="text-muted">· {formatDuration(item.result.durationMs)}</span>
            </span>
          ) : item.status === "error" ? (
            <span className="flex items-start gap-1.5 text-xs text-danger"><AlertCircle className="mt-0.5 size-3 shrink-0" /><span className="line-clamp-2">{item.error}</span></span>
          ) : (
            <span className="block truncate text-xs text-muted">
              <span className="font-mono tabular-nums">{formatDims(item.info.width, item.info.height)}</span> · {presetLabel(preset)}
              {item.status === "cancelled" && " · cancelada"}
            </span>
          )}
        </span>
      </button>
      <button aria-label={`Quitar ${item.info.name}`} disabled={item.status === "running"} onClick={() => ws.removeItem(item.id)} className="self-start rounded-sm p-1 text-muted opacity-0 transition-opacity group-hover:opacity-100 hover:text-foreground focus-visible:opacity-100 disabled:hidden">
        <X className="size-3.5" />
      </button>
    </li>
  );
}
