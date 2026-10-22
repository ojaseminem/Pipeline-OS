import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { ConfirmHost } from "./components/confirm";
import { confirmAction } from "./lib/confirm";
import { reloadPrefs, setPrefs } from "./lib/prefs";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (command: string) => {
    if (command === "dashboard_snapshot") {
      return {
        networkEnabled: false,
        continueProject: { name: "Real Project", path: "D:/Real", engine: "Unity", version: "6000.0", branch: "main", lastOpened: "Now" },
        pinnedProjects: [{ name: "Real Project", path: "D:/Real", engine: "Unity", version: "6000.0", branch: "main", lastOpened: "Now" }],
        recentProjects: [],
        apps: [],
        health: [],
      };
    }
    return [];
  }),
}));
// Keep the event module mocked too: the real one would load the real core
// module beside the mock above.
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => undefined) }));
vi.mock("@tauri-apps/api/webview", () => ({ getCurrentWebview: () => ({ onDragDropEvent: vi.fn(async () => () => undefined) }) }));

function nav(name: string) {
  return within(screen.getByRole("navigation", { name: "Primary navigation" })).getByRole("button", { name: new RegExp(`^${name}`) });
}

describe("dashboard", () => {
  beforeEach(() => {
    localStorage.clear();
    reloadPrefs();
    document.documentElement.removeAttribute("style");
    window.history.replaceState({}, "", "/?demo=true");
  });
  afterEach(() => {
    cleanup();
    delete window.__TAURI_INTERNALS__;
    window.history.replaceState({}, "", "/");
  });

  it("renders native dashboard data instead of sample project content", async () => {
    window.__TAURI_INTERNALS__ = {};
    window.history.replaceState({}, "", "/");
    localStorage.setItem("vantadeck.onboarded", "true");
    render(<App />);

    expect(await screen.findAllByText("Real Project")).not.toHaveLength(0);
    expect(screen.queryByText("Voidline")).not.toBeInTheDocument();
  });

  it("switches between pinned and recent project views", async () => {
    render(<App />);

    expect(screen.getByText("Emberfall")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Recent Projects" }));
    expect(screen.getByText("Atlas Prototype")).toBeInTheDocument();
    expect(screen.queryByText("Emberfall")).not.toBeInTheDocument();
  });

  it.each(["Projects", "Planning", "Applications", "Health", "Tools", "Activity", "Settings"])("opens the %s screen", async (name) => {
    render(<App />);
    await userEvent.click(nav(name));
    expect(screen.getByRole("heading", { level: 1, name })).toBeInTheDocument();
    expect(nav(name)).toHaveAttribute("aria-current", "page");
  });

  it("presents the Tools screen as an offline curated hub", async () => {
    render(<App />);
    await userEvent.click(nav("Tools"));
    expect(screen.getByRole("heading", { name: "Tool sources" })).toBeInTheDocument();
    expect(screen.getByText(/never executes downloaded installers/i)).toBeInTheDocument();
  });

  it("opens the command palette with Ctrl+K and from the header", async () => {
    render(<App />);
    await userEvent.keyboard("{Control>}k{/Control}");
    expect(await screen.findByPlaceholderText(/Type a command/i)).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await userEvent.click(screen.getByRole("button", { name: "Search and commands" }));
    expect(await screen.findByPlaceholderText(/Type a command/i)).toBeInTheDocument();
    expect(screen.getByText("Add project…")).toBeInTheDocument();
    expect(screen.getByText("Theme: Graphite")).toBeInTheDocument();
  });

  it("switches screens with Ctrl+number shortcuts", async () => {
    render(<App />);
    await userEvent.keyboard("{Control>}8{/Control}");
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    await userEvent.keyboard("{Control>}2{/Control}");
    expect(screen.getByRole("heading", { level: 1, name: "Projects" })).toBeInTheDocument();
  });

  it("applies a theme picked in Settings", async () => {
    render(<App />);
    await userEvent.click(nav("Settings"));
    await userEvent.click(screen.getByRole("button", { name: "Appearance" }));
    await userEvent.click(screen.getByRole("radio", { name: /Ember/ }));
    expect(document.documentElement.dataset.theme).toBe("ember");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
    await userEvent.click(screen.getByRole("radio", { name: /Paper/ }));
    expect(document.documentElement.classList.contains("dark")).toBe(false);
  });

  it("shows MCP setup for AI apps in Settings", async () => {
    render(<App />);
    await userEvent.click(nav("Settings"));
    await userEvent.click(screen.getByRole("button", { name: "AI & automation" }));
    expect(await screen.findByText(/claude mcp add pipeline-os/)).toBeInTheDocument();
  });

  it("connects every found AI app at once", async () => {
    render(<App />);
    await userEvent.click(nav("Settings"));
    await userEvent.click(screen.getByRole("button", { name: "AI & automation" }));
    const connectAll = await screen.findByRole("button", { name: /Connect all \(\d+\)/ });
    await userEvent.click(connectAll);
    expect(await screen.findByRole("button", { name: /All found apps connected/ })).toBeDisabled();
    expect(screen.getAllByText("Connected").length).toBeGreaterThanOrEqual(4);
  });

  it("applies accent, density, and interface size from Appearance", async () => {
    render(<App />);
    await userEvent.click(nav("Settings"));
    await userEvent.click(screen.getByRole("button", { name: "Appearance" }));
    await userEvent.click(screen.getByRole("radio", { name: "Violet" }));
    expect(document.documentElement.style.getPropertyValue("--primary")).toContain("295");
    await userEvent.click(screen.getByRole("radio", { name: "Compact" }));
    expect(document.documentElement.dataset.density).toBe("compact");
    await userEvent.click(screen.getByRole("radio", { name: "110%" }));
    expect(document.documentElement.style.fontSize).toBe("110%");
    await userEvent.click(screen.getByRole("button", { name: /Reset/ }));
    expect(document.documentElement.style.getPropertyValue("--primary")).toBe("");
  });

  it("collapses the sidebar to icons", async () => {
    render(<App />);
    await userEvent.click(nav("Settings"));
    await userEvent.click(screen.getByRole("button", { name: "Appearance" }));
    await userEvent.click(screen.getByRole("switch", { name: "Show sidebar labels" }));
    expect(within(screen.getByRole("navigation", { name: "Primary navigation" })).queryByText("Projects")).not.toBeInTheDocument();
    expect(nav("Projects")).toBeInTheDocument();
  });

  it("suggests projects found on this computer and adds them all at once", async () => {
    render(<App />);
    await userEvent.click(nav("Projects"));
    expect(await screen.findByText("Found on this computer")).toBeInTheDocument();
    expect(screen.getByText("Skyline")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add all 3" }));
    await waitFor(() => expect(screen.queryByText("Found on this computer")).not.toBeInTheDocument());
  });

  it("shows editor activity from Unity and Unreal logs", async () => {
    render(<App />);
    await userEvent.click(nav("Activity"));
    expect(await screen.findByText("In your editors")).toBeInTheDocument();
    expect(await screen.findByText("Lighting build · Reactor")).toBeInTheDocument();
    expect(screen.getByText("Player build · Voidline")).toBeInTheDocument();
  });

  it("opens the shortcuts sheet with ?", async () => {
    render(<App />);
    await userEvent.keyboard("?");
    expect(await screen.findByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
  });

  it("navigates to Projects when clicking 'View all projects'", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: /View all projects/ }));
    expect(screen.getByRole("heading", { level: 1, name: "Projects" })).toBeInTheDocument();
  });

  it("navigates to Applications from the Installed apps card", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Manage" }));
    expect(screen.getByRole("heading", { level: 1, name: "Applications" })).toBeInTheDocument();
  });

  it("opens a project's workspace when its row is clicked", async () => {
    render(<App />);
    await userEvent.click(screen.getByText("Emberfall"));
    for (const tab of ["Overview", "Source Control", "Pipeline", "Board", "Assets", "AI"]) {
      expect(screen.getByRole("tab", { name: tab })).toBeInTheDocument();
    }
  });

  it("offers a getting-started checklist that can be dismissed", async () => {
    window.__TAURI_INTERNALS__ = {};
    window.history.replaceState({}, "", "/");
    localStorage.setItem("vantadeck.onboarded", "true");
    render(<App />);
    expect(await screen.findByText("Set up Pipeline OS")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByText("Set up Pipeline OS")).not.toBeInTheDocument();
    expect(localStorage.getItem("vantadeck.setupDismissed")).toBe("true");
  });
});

describe("project workspace (demo backend)", () => {
  beforeEach(() => {
    localStorage.clear();
    reloadPrefs();
    window.history.replaceState({}, "", "/?demo=true");
  });
  afterEach(() => {
    cleanup();
    window.history.replaceState({}, "", "/");
  });

  async function openTab(project: string, tab: string) {
    render(<App />);
    await userEvent.click(screen.getByText(project));
    await userEvent.click(screen.getByRole("tab", { name: tab }));
  }

  it("requires a trust review before running repository tasks, then runs them", async () => {
    setPrefs({ engineWatch: false }); // count only pipeline tasks in the sidebar badge
    await openTab("Mech Bay", "Pipeline");
    expect(await screen.findByText(/came from the project files/)).toBeInTheDocument();
    const runButtons = screen.getAllByRole("button", { name: /^Run$/ });
    await userEvent.click(runButtons[0]);
    expect(await screen.findByRole("heading", { name: /Review project tasks/ })).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: /Trust and allow running/ }));
    await waitFor(() => expect(screen.queryByText(/came from the project files/)).not.toBeInTheDocument());
    await userEvent.click(screen.getAllByRole("button", { name: /^Run$/ })[0]);
    expect(await screen.findByText("Run history")).toBeInTheDocument();
    expect(nav("Activity")).toHaveTextContent("1");
  });

  it("adds issues to the board and moves them between columns", async () => {
    await openTab("Riftbound", "Board");
    const todo = await screen.findByRole("region", { name: "To do column" });
    await userEvent.click(within(todo).getByRole("button", { name: "Add issue to To do" }));
    await userEvent.type(within(todo).getByRole("textbox", { name: "New issue title" }), "Fix lightmap seams{Enter}");
    expect(await within(todo).findByText("Fix lightmap seams")).toBeInTheDocument();
    await userEvent.click(within(todo).getByRole("button", { name: "Actions for Fix lightmap seams" }));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Review" }));
    const review = screen.getByRole("region", { name: "Review column" });
    expect(await within(review).findByText("Fix lightmap seams")).toBeInTheDocument();
  });

  it("connects a tracker and syncs its issues onto the board", async () => {
    await openTab("Emberfall", "Board");
    await userEvent.click(await screen.findByRole("button", { name: /Connect tracker/ }));
    await userEvent.click(screen.getByRole("radio", { name: "Jira" }));
    await userEvent.type(screen.getByPlaceholderText("https://studio.atlassian.net"), "https://studio.atlassian.net");
    await userEvent.type(screen.getByPlaceholderText("VOID"), "VOID");
    await userEvent.click(screen.getByRole("button", { name: "Connect" }));
    await userEvent.click(await screen.findByRole("button", { name: /Sync Jira/ }));
    expect(await screen.findByText("Ship demo build to publisher")).toBeInTheDocument();
    expect(screen.getByText("VOID-142")).toBeInTheDocument();
  });

  it("lists assets with review status and filters by type", async () => {
    await openTab("Northbreak", "Assets");
    expect(await screen.findByText("reactor_room_v4.png")).toBeInTheDocument();
    expect(screen.getAllByText("In review").length).toBeGreaterThan(0);
    await userEvent.click(screen.getByRole("button", { name: /^Audio/ }));
    expect(screen.getByText("reactor_hum_loop.wav")).toBeInTheDocument();
    expect(screen.queryByText("reactor_room_v4.png")).not.toBeInTheDocument();
  });

  it("shows the AI brief and agent launchers", async () => {
    await openTab("Project Helix", "AI");
    expect(await screen.findByRole("button", { name: /Open Claude Code here/ })).toBeInTheDocument();
    expect((await screen.findAllByText(/project brief/)).length).toBeGreaterThan(0);
  });
});

describe("confirm dialog", () => {
  afterEach(() => { cleanup(); });

  it("resolves true on confirm and false on cancel", async () => {
    render(<ConfirmHost />);
    let result: Promise<boolean> = Promise.resolve(false);
    act(() => { result = confirmAction({ title: "Delete it?", confirmLabel: "Delete", destructive: true }); });
    await userEvent.click(await screen.findByRole("button", { name: "Delete" }));
    await expect(result).resolves.toBe(true);

    act(() => { result = confirmAction("Proceed?"); });
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await expect(result).resolves.toBe(false);
  });
});
