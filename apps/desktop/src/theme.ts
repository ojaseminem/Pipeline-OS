import { useEffect, useState } from "react";

/** Built-in themes. `dark` (Midnight) and `light` (Daylight) keep their
 *  historical ids so stored preferences stay valid. */
export const THEMES = [
  { id: "dark", name: "Midnight", mode: "dark", description: "Deep blue-slate with a teal accent. The default.", swatch: ["#151a24", "#1c2230", "#4fd1c5"] },
  { id: "light", name: "Daylight", mode: "light", description: "Bright and calm for well-lit rooms.", swatch: ["#f7f8fa", "#ffffff", "#2bb3a6"] },
  { id: "graphite", name: "Graphite", mode: "dark", description: "Neutral near-black like most DCC apps, so colors read true.", swatch: ["#161616", "#1e1e1e", "#e8a33d"] },
  { id: "nord", name: "Nordic", mode: "dark", description: "Cool, low-contrast arctic blues for long sessions.", swatch: ["#2e3440", "#3b4252", "#88c0d0"] },
  { id: "ember", name: "Ember", mode: "dark", description: "Warm charcoal with a forge-orange accent.", swatch: ["#1b1512", "#241c18", "#f97316"] },
  { id: "paper", name: "Paper", mode: "light", description: "Warm off-white with an indigo accent; easy on print-calibrated eyes.", swatch: ["#f6f3ec", "#fffdf8", "#5b5bd6"] },
  { id: "contrast", name: "High contrast", mode: "dark", description: "Pure black, bright text and strong focus rings for accessibility.", swatch: ["#000000", "#0a0a0a", "#ffd60a"] },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];
export type ThemePreference = "system" | ThemeId;
export type ResolvedTheme = ThemeId;

const THEME_IDS = new Set<string>(THEMES.map((theme) => theme.id));

export function isThemePreference(value: unknown): value is ThemePreference {
  return value === "system" || (typeof value === "string" && THEME_IDS.has(value));
}

export function resolveTheme(preference: ThemePreference, systemPrefersDark: boolean): ResolvedTheme {
  return preference === "system" ? (systemPrefersDark ? "dark" : "light") : preference;
}

export function themeMode(theme: ResolvedTheme): "dark" | "light" {
  return THEMES.find((entry) => entry.id === theme)?.mode ?? "dark";
}

export function useTheme() {
  const [preference, setPreference] = useState<ThemePreference>(() => {
    try {
      const stored = localStorage.getItem("vantadeck.theme");
      return isThemePreference(stored) ? stored : "system";
    } catch {
      return "system";
    }
  });

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = resolveTheme(preference, media.matches);
      const mode = themeMode(resolved);
      document.documentElement.dataset.theme = resolved;
      document.documentElement.style.colorScheme = mode;
      document.documentElement.classList.toggle("dark", mode === "dark");
    };
    apply();
    media.addEventListener("change", apply);
    try { localStorage.setItem("vantadeck.theme", preference); } catch { /* storage unavailable */ }
    return () => media.removeEventListener("change", apply);
  }, [preference]);

  return { preference, setPreference };
}
