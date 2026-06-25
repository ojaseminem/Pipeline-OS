// Project categories (ArtStation-style) and their tag colors. A project's
// category is inferred from its linked apps on the backend and can be overridden
// per project.

export const PROJECT_CATEGORIES = [
  "Game Dev",
  "3D",
  "Sculpting",
  "Texturing",
  "Animation",
  "VFX",
  "2D / Concept",
  "Other",
] as const;

/// Tailwind classes for a category's colored tag (works in light/dark).
export function categoryClass(category?: string | null): string {
  switch (category) {
    case "Game Dev": return "bg-blue-500/15 text-blue-400";
    case "3D": return "bg-cyan-500/15 text-cyan-400";
    case "Sculpting": return "bg-amber-500/15 text-amber-500";
    case "Texturing": return "bg-violet-500/15 text-violet-400";
    case "Animation": return "bg-pink-500/15 text-pink-400";
    case "VFX": return "bg-emerald-500/15 text-emerald-400";
    case "2D / Concept": return "bg-orange-500/15 text-orange-400";
    default: return "bg-secondary text-muted-foreground";
  }
}
