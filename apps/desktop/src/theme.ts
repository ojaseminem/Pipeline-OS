import { useEffect, useState, useSyncExternalStore } from "react";

/** Built-in themes. `dark` (Midnight) and `light` (Daylight) keep their
 *  historical ids so stored preferences stay valid. */
export const THEMES = [
  { id: "dark", name: "Midnight", mode: "dark", description: "Deep blue-slate with a teal accent. The default.", swatch: ["#151a24", "#1c2230", "#4fd1c5"] },
  { id: "light", name: "Daylight", mode: "light", description: "Bright and calm for well-lit rooms.", swatch: ["#f7f8fa", "#ffffff", "#2bb3a6"] },
  { id: "glass", name: "Glass", mode: "dark", description: "Frosted translucent panels over a soft aurora backdrop.", swatch: ["#2a2350", "#3b4b7a", "#7cc8f0"] },
  { id: "frost", name: "Frost", mode: "light", description: "Light frosted glass with pastel depth.", swatch: ["#dfe8fb", "#f4ecfa", "#3f6fe0"] },
  { id: "graphite", name: "Graphite", mode: "dark", description: "Neutral near-black like most DCC apps, so colors read true.", swatch: ["#161616", "#1e1e1e", "#e8a33d"] },
  { id: "nord", name: "Nordic", mode: "dark", description: "Cool, low-contrast arctic blues for long sessions.", swatch: ["#2e3440", "#3b4252", "#88c0d0"] },
  { id: "nightshade", name: "Nightshade", mode: "dark", description: "Deep violet with a magenta accent.", swatch: ["#1f1a2e", "#282238", "#e07ad0"] },
  { id: "forest", name: "Forest", mode: "dark", description: "Mossy greens that are easy on the eyes.", swatch: ["#17221d", "#1d2a24", "#6fd68a"] },
  { id: "ember", name: "Ember", mode: "dark", description: "Warm charcoal with a forge-orange accent.", swatch: ["#1b1512", "#241c18", "#f97316"] },
  { id: "paper", name: "Paper", mode: "light", description: "Warm off-white with an indigo accent; easy on print-calibrated eyes.", swatch: ["#f6f3ec", "#fffdf8", "#5b5bd6"] },
  { id: "contrast", name: "High contrast", mode: "dark", description: "Pure black, bright text and strong focus rings for accessibility.", swatch: ["#000000", "#0a0a0a", "#ffd60a"] },
] as const;

export type BuiltinThemeId = (typeof THEMES)[number]["id"];
export type ThemeId = BuiltinThemeId;
export type CustomThemeId = `custom:${string}`;
export type ThemePreference = "system" | BuiltinThemeId | CustomThemeId;
export type ResolvedTheme = BuiltinThemeId | CustomThemeId;

// ------------------------------------------------------------ custom themes
// Shared as `.postheme` files: JSON with a format marker, like VS Code or
// Windows Terminal themes, so they're easy to read, diff, and hand-edit.

export const THEME_FILE_EXTENSION = ".postheme";
export const THEME_FORMAT = "pipelineos-theme";

export const THEME_TOKENS = [
  "background", "foreground", "card", "card-foreground", "popover", "popover-foreground",
  "primary", "primary-foreground", "secondary", "secondary-foreground", "muted", "muted-foreground",
  "accent", "accent-foreground", "destructive", "border", "input", "ring",
  "sidebar", "sidebar-foreground", "sidebar-primary", "sidebar-primary-foreground",
  "sidebar-accent", "sidebar-accent-foreground", "sidebar-border", "sidebar-ring",
] as const;
export type ThemeToken = (typeof THEME_TOKENS)[number];

export type CustomTheme = {
  id: string;
  name: string;
  author?: string;
  mode: "dark" | "light";
  /** Frosted, translucent panels. */
  glass?: boolean;
  /** Up to four colors blended into a soft backdrop gradient. */
  backdrop?: string[];
  tokens: Partial<Record<ThemeToken, string>>;
};

export type ThemeFile = { format: typeof THEME_FORMAT; version: 1 } & Omit<CustomTheme, "id">;

const COLOR = /^(#[0-9a-f]{3,8}|(?:oklch|oklab|lch|lab|rgb|rgba|hsl|hsla)\(\s*[0-9.,%\s/+-]+\))$/i;

export function isColor(value: unknown): value is string {
  return typeof value === "string" && value.length <= 64 && COLOR.test(value.trim());
}

function slug(name: string): string {
  return name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "").slice(0, 40) || "theme";
}

/** Validates an imported file; only known tokens with plain color values pass. */
export function parseThemeFile(raw: string): CustomTheme {
  let data: unknown;
  try { data = JSON.parse(raw); } catch { throw new Error("This file isn't valid JSON."); }
  const file = data as Partial<ThemeFile> & { tokens?: Record<string, unknown> };
  if (!file || typeof file !== "object" || file.format !== THEME_FORMAT) throw new Error(`Not a Pipeline OS theme (expected "format": "${THEME_FORMAT}").`);
  if (file.version !== 1) throw new Error(`Unsupported theme version ${String(file.version)}.`);
  const name = typeof file.name === "string" && file.name.trim() ? file.name.trim().slice(0, 48) : "Imported theme";
  const tokens: Partial<Record<ThemeToken, string>> = {};
  for (const token of THEME_TOKENS) {
    const value = file.tokens?.[token];
    if (isColor(value)) tokens[token] = value.trim();
  }
  if (!tokens.background || !tokens.foreground || !tokens.primary) throw new Error("A theme needs at least background, foreground, and primary colors.");
  return {
    id: slug(name),
    name,
    author: typeof file.author === "string" ? file.author.slice(0, 64) : undefined,
    mode: file.mode === "light" ? "light" : "dark",
    glass: file.glass === true,
    backdrop: Array.isArray(file.backdrop) ? file.backdrop.filter(isColor).slice(0, 4) : undefined,
    tokens,
  };
}

export function serializeTheme(theme: Omit<CustomTheme, "id">): string {
  const file: ThemeFile = { format: THEME_FORMAT, version: 1, name: theme.name, author: theme.author, mode: theme.mode, glass: theme.glass, backdrop: theme.backdrop, tokens: theme.tokens };
  return `${JSON.stringify(file, null, 2)}\n`;
}

/** Captures the theme currently on screen (including accent overrides). */
export function captureCurrentTheme(name: string): Omit<CustomTheme, "id"> {
  const style = getComputedStyle(document.documentElement);
  const tokens: Partial<Record<ThemeToken, string>> = {};
  for (const token of THEME_TOKENS) {
    const value = style.getPropertyValue(`--${token}`).trim();
    if (isColor(value)) tokens[token] = value;
  }
  const mode = document.documentElement.classList.contains("dark") ? "dark" : "light";
  const glass = ["glass", "frost"].includes(document.documentElement.dataset.theme ?? "") || document.documentElement.dataset.glass === "true";
  return { name, mode, glass, tokens };
}

const STORE_KEY = "vantadeck.customThemes";
const listeners = new Set<() => void>();

function readCustom(): CustomTheme[] {
  try {
    const list = JSON.parse(localStorage.getItem(STORE_KEY) ?? "[]") as unknown[];
    return list.flatMap((entry) => {
      try {
        const theme = parseThemeFile(JSON.stringify({ ...(entry as object), format: THEME_FORMAT, version: 1 }));
        return [{ ...theme, id: String((entry as CustomTheme).id ?? theme.id) }];
      } catch { return []; }
    });
  } catch { return []; }
}

let customThemes: CustomTheme[] = readCustom();

function persistCustom(next: CustomTheme[]) {
  customThemes = next;
  try { localStorage.setItem(STORE_KEY, JSON.stringify(next)); } catch { /* storage unavailable */ }
  for (const listener of listeners) listener();
}

export function getCustomThemes(): CustomTheme[] { return customThemes; }

/** Adds (or replaces, by id) a custom theme and returns its preference id. */
export function saveCustomTheme(theme: CustomTheme): CustomThemeId {
  let id = theme.id;
  const taken = new Set(customThemes.map((entry) => entry.id));
  if (taken.has(id) && customThemes.find((entry) => entry.id === id)?.name !== theme.name) {
    let n = 2;
    while (taken.has(`${id}-${n}`)) n++;
    id = `${id}-${n}`;
  }
  persistCustom([...customThemes.filter((entry) => entry.id !== id), { ...theme, id }]);
  return `custom:${id}`;
}

export function deleteCustomTheme(id: string) {
  persistCustom(customThemes.filter((entry) => entry.id !== id));
}

export function reloadCustomThemes() {
  customThemes = readCustom();
  for (const listener of listeners) listener();
}

export function useCustomThemes(): CustomTheme[] {
  return useSyncExternalStore((listener) => { listeners.add(listener); return () => listeners.delete(listener); }, () => customThemes);
}

function findCustom(preference: string): CustomTheme | undefined {
  return preference.startsWith("custom:") ? customThemes.find((theme) => `custom:${theme.id}` === preference) : undefined;
}

// ------------------------------------------------------------ resolution

const THEME_IDS = new Set<string>(THEMES.map((theme) => theme.id));

export function isThemePreference(value: unknown): value is ThemePreference {
  return value === "system" || (typeof value === "string" && (THEME_IDS.has(value) || Boolean(findCustom(value))));
}

export function resolveTheme(preference: ThemePreference, systemPrefersDark: boolean): ResolvedTheme {
  return preference === "system" ? (systemPrefersDark ? "dark" : "light") : preference;
}

export function themeMode(theme: ResolvedTheme): "dark" | "light" {
  const custom = findCustom(theme);
  if (custom) return custom.mode;
  return THEMES.find((entry) => entry.id === theme)?.mode ?? "dark";
}

/** Writes a custom theme as a stylesheet rule (so accent overrides, which are
 *  inline styles, still win). Values are validated colors only. */
function applyCustomStyles(theme: CustomTheme | undefined) {
  const id = "pos-custom-theme";
  let element = document.getElementById(id) as HTMLStyleElement | null;
  if (!theme) { element?.remove(); return; }
  if (!element) {
    element = document.createElement("style");
    element.id = id;
    document.head.appendChild(element);
  }
  const vars = THEME_TOKENS.filter((token) => isColor(theme.tokens[token])).map((token) => `--${token}: ${theme.tokens[token]};`).join(" ");
  const stops = (theme.backdrop ?? []).filter(isColor);
  const positions = ["10% -10%", "100% 0%", "50% 120%", "0% 100%"];
  const backdrop = stops.length
    ? `[data-theme="custom"] body { background: ${stops.map((color, index) => `radial-gradient(1000px 650px at ${positions[index]}, ${color}, transparent 60%)`).join(", ")}, ${theme.tokens.background}; background-attachment: fixed; }`
    : "";
  element.textContent = `[data-theme="custom"] { ${vars} } ${backdrop}`;
}

export function useTheme() {
  const custom = useCustomThemes();
  const [preference, setPreference] = useState<ThemePreference>(() => {
    try {
      const stored = localStorage.getItem("vantadeck.theme");
      return isThemePreference(stored) ? stored : "system";
    } catch {
      return "system";
    }
  });
  // A deleted custom theme falls back to the system theme.
  const effective: ThemePreference = preference.startsWith("custom:") && !custom.some((theme) => `custom:${theme.id}` === preference) ? "system" : preference;

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => {
      const resolved = resolveTheme(effective, media.matches);
      const mode = themeMode(resolved);
      const customTheme = findCustom(resolved);
      applyCustomStyles(customTheme);
      document.documentElement.dataset.theme = customTheme ? "custom" : resolved;
      if (customTheme?.glass) document.documentElement.dataset.glass = "true"; else delete document.documentElement.dataset.glass;
      document.documentElement.style.colorScheme = mode;
      document.documentElement.classList.toggle("dark", mode === "dark");
    };
    apply();
    media.addEventListener("change", apply);
    try { localStorage.setItem("vantadeck.theme", effective); } catch { /* storage unavailable */ }
    return () => media.removeEventListener("change", apply);
  }, [effective, custom]);

  return { preference: effective, setPreference };
}
