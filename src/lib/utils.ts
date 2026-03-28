/**
 * Utility helpers shared across the Meridian Mail frontend.
 *
 * `cn` behaves similarly to popular helpers like `classnames` or `clsx`,
 * letting us compose className strings while safely ignoring falsy values.
 */
import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
