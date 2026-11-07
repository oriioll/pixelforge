import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

/**
 * Merges class names, resolving Tailwind conflicts (last one wins).
 *
 * @param inputs - class values accepted by clsx
 * @returns the merged class string
 * @author oriioll
 * @since 2026-11-07
 */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}
