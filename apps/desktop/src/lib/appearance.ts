// Appearance preferences beyond the theme: accent color, density, interface
// scale, motion, and sidebar labels. Stored per machine and applied to the
// document root, so they work with every theme.
import { useEffect, useState } from "react";

export const ACCENTS = [
  { id: "theme", name: "Theme default", color: null, foreground: null },
  { id: "teal", name: "Teal", color: "oklch(0.74 0.13 184)", foreground: "oklch(0.18 0.03 200)" },
  { id: "blue", name: "Blue", color: "oklch(0.66 0.16 255)", foreground: "oklch(0.98 0.01 255)" },
  { id: "violet", name: "Violet", color: "oklch(0.64 0.19 295)", foreground: "oklch(0.98 0.01 295)" },
  { id: "pink", name: "Pink", color: "oklch(0.7 0.19 350)", foreground: "oklch(0.18 0.04 350)" },
  { id: "red", name: "Red", color: "oklch(0.64 0.21 25)", foreground: "oklch(0.98 0.01 25)" },
  { id: "orange", name: "Orange", color: "oklch(0.74 0.17 55)", foreground: "oklch(0.2 0.04 55)" },
  { id: "amber", name: "Amber", color: "oklch(0.82 0.15 85)", foreground: "oklch(0.22 0.04 85)" },
  { id: "green", name: "Green", color: "oklch(0.72 0.17 150)", foreground: "oklch(0.18 0.04 150)" },
] as const;

export type AccentId = (typeof ACCENTS)[number]["id"];
export type Density = "comfortable" | "compact";
export type Motion = "system" | "full" | "reduced";

export type Appearance = {
  accent: AccentId;
  density: Density;
  /** Interface scale in percent (90–125). */
  scale: number;
  motion: Motion;
  sidebarLabels: boolean;
};

export const DEFAULT_APPEARANCE: Appearance = { accent: "theme", density: "comfortable", scale: 100, motion: "system", sidebarLabels: true };
export const SCALES = [90, 100, 110, 125] as const;

const KEY = "vantadeck.appearance";

export function loadAppearance(): Appearance {
  try {
    const parsed = JSON.parse(localStorage.getItem(KEY) ?? "{}") as Partial<Appearance>;
    const value = { ...DEFAULT_APPEARANCE, ...parsed };
    if (!ACCENTS.some((accent) => accent.id === value.accent)) value.accent = "theme";
    if (!SCALES.includes(value.scale as (typeof SCALES)[number])) value.scale = 100;
    return value;
  } catch {
    return DEFAULT_APPEARANCE;
  }
}

export function applyAppearance(value: Appearance, root: HTMLElement = document.documentElement) {
  const accent = ACCENTS.find((entry) => entry.id === value.accent);
  for (const token of ["--primary", "--ring", "--sidebar-primary", "--sidebar-ring"]) {
    if (accent?.color) root.style.setProperty(token, accent.color); else root.style.removeProperty(token);
  }
  for (const token of ["--primary-foreground", "--sidebar-primary-foreground"]) {
    if (accent?.foreground) root.style.setProperty(token, accent.foreground); else root.style.removeProperty(token);
  }
  root.dataset.density = value.density;
  root.dataset.motion = value.motion;
  root.style.fontSize = value.scale === 100 ? "" : `${value.scale}%`;
}

export function useAppearance() {
  const [appearance, setAppearance] = useState<Appearance>(loadAppearance);
  useEffect(() => {
    applyAppearance(appearance);
    try { localStorage.setItem(KEY, JSON.stringify(appearance)); } catch { /* storage unavailable */ }
  }, [appearance]);
  const update = (patch: Partial<Appearance>) => setAppearance((current) => ({ ...current, ...patch }));
  return { appearance, update, reset: () => setAppearance(DEFAULT_APPEARANCE) };
}
