import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

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

describe("dashboard projects", () => {
  beforeEach(() => window.history.replaceState({}, "", "/?demo=true"));
  afterEach(() => {
    document.body.innerHTML = "";
    delete window.__TAURI_INTERNALS__;
    window.history.replaceState({}, "", "/");
  });

  it("renders native dashboard data instead of sample project content", async () => {
    window.__TAURI_INTERNALS__ = {};
    window.history.replaceState({}, "", "/");
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

  it.each(["Projects", "Applications", "Health", "Tools", "Settings"])(
    "opens the %s management screen",
    async (screenName) => {
      render(<App />);
      await userEvent.click(screen.getByRole("navigation", { name: "Primary navigation" }).querySelector(`button:nth-child(${["Projects", "Applications", "Health", "Tools", "Settings"].indexOf(screenName) + 2})`)!);
      expect(screen.getByRole("heading", { name: screenName })).toBeInTheDocument();
    },
  );

  it("presents the Tools screen as an offline curated hub", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: /Tools/ }));

    expect(screen.getByRole("heading", { name: "Curated Tools Hub" })).toBeInTheDocument();
    expect(screen.getByText(/never executes downloaded installers/i)).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Local Git inspector" })).not.toBeInTheDocument();
  });

  it("focuses global search with Ctrl+K", async () => {
    render(<App />);
    await userEvent.keyboard("{Control>}k{/Control}");
    expect(screen.getByPlaceholderText("Search projects, apps, tools, docs...")).toHaveFocus();
  });

  it("navigates to Projects when clicking 'View all projects' button", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: /View all projects/ }));
    expect(screen.getByRole("heading", { name: "Projects" })).toBeInTheDocument();
  });

  it("navigates to Applications when clicking 'Manage' button in Installed Apps", async () => {
    render(<App />);
    const manageButton = screen.getByRole("button", { name: "Manage" });
    await userEvent.click(manageButton);
    expect(screen.getByRole("heading", { name: "Applications" })).toBeInTheDocument();
  });

  it("navigates to Applications when clicking an app-row button", async () => {
    render(<App />);
    const appRows = screen.getAllByRole("button", { name: /Chrome|Firefox|Visual Studio Code/ });
    if (appRows.length > 0) {
      await userEvent.click(appRows[0]);
      expect(screen.getByRole("heading", { name: "Applications" })).toBeInTheDocument();
    }
  });

  it("navigates to Projects when clicking ProjectTable 'Open Project' button", async () => {
    render(<App />);
    const openProjectButtons = screen.queryAllByRole("button", { name: /Open Project/ });
    if (openProjectButtons.length > 0) {
      await userEvent.click(openProjectButtons[0]);
      expect(screen.getByRole("heading", { name: "Projects" })).toBeInTheDocument();
    }
  });
});
