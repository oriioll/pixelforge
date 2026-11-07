import { FolderOpen, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { api, fileUrl } from "@/lib/api";
import { basename, formatDate, formatDims } from "@/lib/format";
import { presetLabel } from "@/lib/presets";
import type { Workspace } from "@/hooks/useWorkspace";

/**
 * Past results, newest first. Clicking one opens it in the comparer.
 *
 * @param props.ws - the workspace state
 * @returns the history list
 * @author oriioll
 * @since 2026-11-07
 */
export function HistoryPanel({ ws }: { ws: Workspace }) {
  if (ws.history.length === 0) return <p className="p-4 text-muted">Todavía no has mejorado ninguna imagen.</p>;
  return (
    <div>
      <ul className="divide-y divide-border">
        {ws.history.map((h) => (
          <li key={h.id} className="group flex items-center gap-3 px-4 py-3">
            <button className="flex min-w-0 flex-1 items-center gap-3 text-left" onClick={() => ws.viewHistory(h)}>
              <img src={fileUrl(h.output)} alt="" loading="lazy" className="size-10 shrink-0 rounded-sm bg-surface-2 object-cover" onError={(e) => (e.currentTarget.style.visibility = "hidden")} />
              <span className="min-w-0 flex-1">
                <span className="block truncate" title={h.output}>{basename(h.output)}</span>
                <span className="block truncate text-xs text-muted">
                  {presetLabel(h.preset)} · <span className="font-mono tabular-nums">{formatDims(h.width, h.height)}</span> · {formatDate(h.createdAt)}
                </span>
              </span>
            </button>
            <div className="flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100 focus-within:opacity-100">
              <Button variant="ghost" size="icon" aria-label="Mostrar en la carpeta" onClick={() => void api.revealInFolder(h.output)}><FolderOpen className="size-3.5" /></Button>
              <Button variant="ghost" size="icon" aria-label="Quitar del historial" onClick={() => void api.historyRemove(h.id).then(ws.refreshHistory)}><Trash2 className="size-3.5" /></Button>
            </div>
          </li>
        ))}
      </ul>
      <div className="p-4">
        <Button variant="secondary" size="sm" onClick={() => void api.historyClear().then(ws.refreshHistory)}>Vaciar historial</Button>
      </div>
    </div>
  );
}
