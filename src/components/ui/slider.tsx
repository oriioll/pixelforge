import * as SliderPrimitive from "@radix-ui/react-slider";
import { cn } from "@/lib/utils";

interface SliderProps {
  value: number;
  onValueChange: (value: number) => void;
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  /** Accessible name; the visible label lives next to the slider. */
  label: string;
  className?: string;
}

/**
 * Single-thumb slider (shadcn/ui style, floodlight range).
 *
 * @param props - value, change handler and bounds
 * @returns the slider element
 * @author oriioll
 * @since 2026-11-07
 */
export function Slider({ value, onValueChange, min = 0, max = 100, step = 1, disabled, label, className }: SliderProps) {
  return (
    <SliderPrimitive.Root
      className={cn("relative flex h-5 w-full touch-none items-center select-none data-[disabled]:opacity-40", className)}
      value={[value]}
      onValueChange={(v) => onValueChange(v[0])}
      min={min}
      max={max}
      step={step}
      disabled={disabled}
    >
      <SliderPrimitive.Track className="relative h-[3px] grow rounded-full bg-border-strong">
        <SliderPrimitive.Range className="absolute h-full rounded-full bg-floodlight" />
      </SliderPrimitive.Track>
      <SliderPrimitive.Thumb aria-label={label} className="block size-3.5 rounded-full border border-foreground bg-background transition-transform hover:scale-110" />
    </SliderPrimitive.Root>
  );
}
