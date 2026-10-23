import { describe, expect, it } from "vitest";
import { THEMES, isThemePreference, resolveTheme, themeMode } from "./theme";

describe("resolveTheme", () => {
  it("uses the operating-system preference in system mode", () => {
    expect(resolveTheme("system", true)).toBe("dark");
    expect(resolveTheme("system", false)).toBe("light");
  });

  it("preserves an explicit theme", () => {
    expect(resolveTheme("dark", false)).toBe("dark");
    expect(resolveTheme("light", true)).toBe("light");
    expect(resolveTheme("graphite", false)).toBe("graphite");
  });

  it("knows each theme's light/dark mode", () => {
    expect(themeMode("paper")).toBe("light");
    expect(themeMode("ember")).toBe("dark");
  });

  it("validates stored preferences", () => {
    expect(isThemePreference("nord")).toBe(true);
    expect(isThemePreference("system")).toBe(true);
    expect(isThemePreference("neon")).toBe(false);
    expect(new Set(THEMES.map((theme) => theme.id)).size).toBe(THEMES.length);
  });
});
