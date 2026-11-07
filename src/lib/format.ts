/**
 * Formats milliseconds as `4.2 s` or `1 min 12 s`.
 *
 * @param ms - duration in milliseconds
 * @returns a short human readable duration
 * @author oriioll
 * @since 2026-11-07
 */
export function formatDuration(ms: number): string {
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)} s`;
  return `${Math.floor(s / 60)} min ${Math.round(s % 60)} s`;
}

/**
 * Last path segment, for both `/` and `\` separators.
 *
 * @param path - file path
 * @returns the file name
 * @author oriioll
 * @since 2026-11-07
 */
export const basename = (path: string): string => path.split(/[\\/]/).pop() ?? path;

/**
 * Formats pixel dimensions, e.g. `1440 × 2560`.
 *
 * @param w - width in px
 * @param h - height in px
 * @returns the formatted dimensions
 * @author oriioll
 * @since 2026-11-07
 */
export const formatDims = (w: number, h: number): string => `${w} × ${h}`;

/**
 * Formats a unix timestamp (seconds) with the user's locale.
 *
 * @param seconds - unix time in seconds
 * @returns a short date and time
 * @author oriioll
 * @since 2026-11-07
 */
export const formatDate = (seconds: number): string =>
  new Date(seconds * 1000).toLocaleString(undefined, { day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit" });
