import { type ClassValue, clsx } from "clsx";
import { twMerge } from "tailwind-merge";

// Merge Tailwind classes with correct precedence (later wins on conflict).
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
