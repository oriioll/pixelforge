import { cva, type VariantProps } from "class-variance-authority";
import * as React from "react";
import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 rounded-md font-medium whitespace-nowrap transition-colors disabled:pointer-events-none disabled:opacity-40",
  {
    variants: {
      variant: {
        default: "bg-foreground text-background hover:bg-white",
        secondary: "border border-border-strong bg-surface-2 text-foreground hover:bg-border",
        ghost: "text-muted hover:bg-surface-2 hover:text-foreground",
        danger: "border border-danger/50 text-danger hover:bg-danger/10",
      },
      size: { sm: "h-7 px-2.5 text-xs", md: "h-9 px-3.5", icon: "size-7" },
    },
    defaultVariants: { variant: "default", size: "md" },
  },
);

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement>, VariantProps<typeof buttonVariants> {}

/**
 * Button in the app's visual language (shadcn/ui style).
 *
 * @param props - native button props plus `variant` and `size`
 * @returns the button element
 * @author oriioll
 * @since 2026-11-07
 */
export function Button({ className, variant, size, ...props }: ButtonProps) {
  return <button className={cn(buttonVariants({ variant, size }), className)} {...props} />;
}
