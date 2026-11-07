import * as SwitchPrimitive from "@radix-ui/react-switch";
import { cn } from "@/lib/utils";

interface SwitchProps {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  label: string;
  className?: string;
}

/**
 * On/off switch (shadcn/ui style).
 *
 * @param props - state, handler and accessible label
 * @returns the switch element
 * @author oriioll
 * @since 2026-11-07
 */
export function Switch({ checked, onCheckedChange, disabled, label, className }: SwitchProps) {
  return (
    <SwitchPrimitive.Root
      aria-label={label}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      className={cn("relative h-5 w-9 shrink-0 rounded-full border border-border-strong bg-surface-2 transition-colors data-[state=checked]:border-floodlight data-[state=checked]:bg-floodlight disabled:opacity-40", className)}
    >
      <SwitchPrimitive.Thumb className="block size-3.5 translate-x-0.5 rounded-full bg-foreground transition-transform data-[state=checked]:translate-x-[18px] data-[state=checked]:bg-background" />
    </SwitchPrimitive.Root>
  );
}
