import * as ToggleGroup from "@radix-ui/react-toggle-group";
import { cn } from "@/lib/utils";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  disabled?: boolean;
  title?: string;
}

interface SegmentedProps<T extends string> {
  value: T;
  onValueChange: (value: T) => void;
  options: SegmentOption<T>[];
  label: string;
  className?: string;
}

/**
 * Single-choice segmented control built on Radix ToggleGroup.
 *
 * @param props - current value, options and change handler
 * @returns the control
 * @author oriioll
 * @since 2026-11-07
 */
export function Segmented<T extends string>({ value, onValueChange, options, label, className }: SegmentedProps<T>) {
  return (
    <ToggleGroup.Root
      type="single"
      aria-label={label}
      value={value}
      // Radix emits "" when the active item is pressed again; keep the selection.
      onValueChange={(v) => v && onValueChange(v as T)}
      className={cn("grid auto-cols-fr grid-flow-col rounded-md border border-border bg-surface p-0.5", className)}
    >
      {options.map((o) => (
        <ToggleGroup.Item
          key={o.value}
          value={o.value}
          disabled={o.disabled}
          title={o.title}
          className="h-7 rounded-[4px] px-2 text-xs text-muted transition-colors hover:text-foreground disabled:opacity-35 data-[state=on]:bg-surface-2 data-[state=on]:text-foreground data-[state=on]:shadow-[inset_0_0_0_1px_var(--color-border-strong)]"
        >
          {o.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}
