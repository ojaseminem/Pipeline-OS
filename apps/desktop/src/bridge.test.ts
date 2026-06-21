import { afterEach, describe, expect, it, vi } from "vitest";
import { desktopApi, invokeDesktop, loadDashboard } from "./bridge";

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
});
