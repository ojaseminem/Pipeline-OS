import { afterEach, describe, expect, it, vi } from "vitest";
import { APP_CATEGORY_LABELS, desktopApi, formatVersion, invokeDesktop, loadDashboard } from "./bridge";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => ({ command })),
}));

afterEach(() => {
  delete window.__TAURI_INTERNALS__;
  window.history.replaceState({}, "", "/");
});

describe("desktop bridge", () => {
  it("provides demo data only when explicitly requested outside Tauri", async () => {
    window.history.replaceState({}, "", "/?demo=true");
    const snapshot = await loadDashboard();
    expect(snapshot.networkEnabled).toBe(false);
    expect(snapshot.apps.some((app) => app.versions.length > 1)).toBe(true);
  });

  it("uses the native command bridge inside Tauri", async () => {
    window.__TAURI_INTERNALS__ = {};
    await expect(invokeDesktop("list_projects")).resolves.toEqual({ command: "list_projects" });
  });

  it("rejects native-only operations in a normal browser", async () => {
    await expect(invokeDesktop("list_projects")).rejects.toThrow("desktop runtime");
  });

  it("routes project pinning and profile launches through typed native commands", async () => {
    window.__TAURI_INTERNALS__ = {};
    await expect(desktopApi.pinProject("D:/Projects/Voidline", true)).resolves.toEqual({ command: "set_project_pinned" });
    await expect(desktopApi.launchProjectProfile("D:/Projects/Voidline", "editor")).resolves.toEqual({ command: "launch_project_profile" });
  });

  it("passes explicit confirmation to Git mutation commands", async () => {
    window.__TAURI_INTERNALS__ = {};
    await expect(desktopApi.gitSync("D:/Projects/Voidline", true)).resolves.toEqual({ command: "git_sync" });
    await expect(desktopApi.gitSwitch("D:/Projects/Voidline", "develop", true)).resolves.toEqual({ command: "git_switch" });
  });

  it("routes icon and update operations through native commands", async () => {
    window.__TAURI_INTERNALS__ = {};
    await expect(desktopApi.appIcon("C:/Apps/Blender/blender.exe")).resolves.toEqual({ command: "app_icon" });
    await expect(desktopApi.checkForUpdate()).resolves.toEqual({ command: "check_for_update" });
  });

  it("presents the unknown-version sentinel as readable text", () => {
    expect(formatVersion("0.0.0")).toBe("Unknown version");
    expect(formatVersion("2022.3.18")).toBe("2022.3.18");
  });

  it("labels every known application category", () => {
    expect(APP_CATEGORY_LABELS["game-engine"]).toBe("Game Engines");
    expect(APP_CATEGORY_LABELS["version-control"]).toBe("Version Control");
  });
});
