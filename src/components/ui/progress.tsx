import * as ProgressPrimitive from "@radix-ui/react-progress";
import { cn } from "@/lib/utils";

/**
 * Thin progress bar.
 *
 * @param props.value - 0..1
 * @param props.label - accessible name
 * @returns the progress element
 * @author oriioll
 * @since 2026-11-07
 */
export function Progress({ value, label, className }: { value: number; label: string; className?: string }) {
  const pct = Math.round(Math.min(1, Math.max(0, value)) * 100);
  return (
    <ProgressPrimitive.Root aria-label={label} value={pct} className={cn("h-[3px] w-full overflow-hidden rounded-full bg-border", className)}>
      <ProgressPrimitive.Indicator className="h-full bg-floodlight transition-[width] duration-200" style={{ width: `${pct}%` }} />
    </ProgressPrimitive.Root>
  );
}
