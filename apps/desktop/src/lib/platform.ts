// The OS the UI runs on, for platform-appropriate copy and examples
// (paths, shortcuts). Detected from the WebView's user agent.
export type Platform = "windows" | "mac" | "linux";

export function currentPlatform(): Platform {
  const agent = typeof navigator === "undefined" ? "" : `${navigator.userAgent} ${navigator.platform ?? ""}`;
  if (/Mac|iPhone|iPad/i.test(agent)) return "mac";
  if (/Win/i.test(agent)) return "windows";
  return "linux";
}

const EXAMPLES: Record<Platform, { toolFolders: string; projectFolder: string; standardLocations: string }> = {
  windows: { toolFolders: "e.g. D:/Tools; E:/Apps", projectFolder: "D:/Projects/MyGame", standardLocations: "Program Files, Unity Hub, Epic Games, Steam and every drive's common install folders" },
  mac: { toolFolders: "e.g. /Volumes/Tools; ~/Apps", projectFolder: "~/Projects/MyGame", standardLocations: "/Applications, ~/Applications, Unity Hub, Epic Games and Homebrew" },
  linux: { toolFolders: "e.g. ~/Apps; /opt/tools", projectFolder: "~/Projects/MyGame", standardLocations: "/opt, ~/Applications, ~/.local, Flatpak, Snap and your PATH" },
};

export function platformExamples(platform: Platform = currentPlatform()) {
  return EXAMPLES[platform];
}

/** Shortcut key label for this OS: ⌘/⌥/⇧ on macOS, Ctrl/Alt/Shift elsewhere. */
export function keyLabel(key: string, platform: Platform = currentPlatform()): string {
  if (platform !== "mac") return key;
  return ({ Ctrl: "⌘", Alt: "⌥", Shift: "⇧" } as Record<string, string>)[key] ?? key;
}
