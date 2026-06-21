export type Project = {
  name: string;
  path: string;
  lastOpened: string;
  engine: string;
  version: string;
  branch: string;
};

export const pinnedProjects: Project[] = [
  { name: "Emberfall", path: "D:/Dev/Projects/Emberfall", lastOpened: "Today, 9:15 AM", engine: "Unreal Engine", version: "5.3.2", branch: "develop" },
  { name: "Project Helix", path: "D:/Dev/Projects/Helix", lastOpened: "Yesterday, 4:28 PM", engine: "Unity", version: "2022.3.18f1", branch: "main" },
  { name: "Mech Bay", path: "D:/Dev/Projects/MechBay", lastOpened: "2 days ago", engine: "Unreal Engine", version: "5.2.1", branch: "main" },
  { name: "Northbreak", path: "D:/Dev/Projects/Northbreak", lastOpened: "May 10, 2025", engine: "Unity", version: "2020.3.48f1", branch: "release" },
  { name: "Riftbound", path: "D:/Dev/Projects/Riftbound", lastOpened: "May 8, 2025", engine: "Unreal Engine", version: "5.3", branch: "main" },
];

export const recentProjects: Project[] = [
  { name: "Atlas Prototype", path: "D:/Dev/Projects/Atlas", lastOpened: "Today, 7:40 AM", engine: "Godot", version: "4.3", branch: "prototype" },
  { name: "Quiet Harbor", path: "D:/Dev/Projects/QuietHarbor", lastOpened: "3 days ago", engine: "Unity", version: "2022.3.18f1", branch: "main" },
];

export const installedApps = [
  { name: "Blender", category: "dcc", versions: ["4.1.1 LTS (Default)", "3.6.5 LTS", "2.93.18 LTS"] },
  { name: "Unreal Engine", category: "game-engine", versions: ["5.3.2 (Default)", "5.2.1", "5.1.1"] },
  { name: "Unity", category: "game-engine", versions: ["2022.3.18f1 (Default)"] },
  { name: "Visual Studio Code", category: "code", versions: ["1.87.2 (Default)"] },
];
