import { useEffect, useState } from "react";
import { Bot, KanbanSquare, Rocket, Workflow } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { PathInput } from "./path-input";
import { BrandGlyph } from "./brand";
import { ThemePicker } from "./settings-screen";
import type { ThemePreference } from "../theme";
import { platformExamples } from "../lib/platform";
import { DiscoveredProjects } from "./discovered-projects";
import { addDiscoveredProjects, useDiscoverySelection } from "../lib/discovery";
import { useQueryClient } from "@tanstack/react-query";
import { Loader2 } from "lucide-react";
import { useDiscovery } from "../lib/discovery";

export type OnboardingPrefs = {
  scanRoots: string;
  theme: ThemePreference;
};

const VALUE = [
  { icon: Rocket, title: "Launch at the right version", text: "Every engine and DCC version on your machine, and each project opens in the one it was pinned to." },
  { icon: Workflow, title: "One-click pipeline", text: "Builds, cooks, renders, and exports with live progress, shared with the team through the repo." },
  { icon: KanbanSquare, title: "Work and art in one place", text: "A git-native board per project, an asset library with review status, and source control built in." },
  { icon: Bot, title: "AI-ready", text: "Start Claude Code in any project, and let AI apps work with your projects through MCP." },
];

const STEPS = 4;

export function Onboarding({ open, onComplete, onSkip, initialTheme = "system", onPreviewTheme, onProjectsAdded }: {
  open: boolean;
  onProjectsAdded?: () => void;
  onComplete: (prefs: OnboardingPrefs) => void;
  onSkip: () => void;
  initialTheme?: ThemePreference;
  onPreviewTheme?: (theme: ThemePreference) => void;
}) {
  const [step, setStep] = useState(0);
  const [scanRoots, setScanRoots] = useState("");
  const [theme, setTheme] = useState<ThemePreference>(initialTheme);
  const titles = ["Welcome to Pipeline OS", "Make it yours", "Find your creative apps", "Bring in your projects"];
  const discovery = useDiscovery();
  const { refresh, scannedAt } = discovery;
  const selection = useDiscoverySelection(discovery.projects);
  const queryClient = useQueryClient();
  const [finishing, setFinishing] = useState(false);
  const chosen = discovery.projects.filter((project) => selection.selected.has(project.path));

  async function finish() {
    setFinishing(true);
    try {
      if (step === STEPS - 1 && chosen.length) {
        await addDiscoveredProjects(chosen, queryClient);
        onProjectsAdded?.();
      }
    } finally {
      setFinishing(false);
    }
    onComplete({ scanRoots, theme });
  }
  // Start looking for projects early so the list is ready on the last step.
  useEffect(() => {
    if (open && step >= 2 && !scannedAt) void refresh().catch(() => undefined);
  }, [open, step, scannedAt, refresh]);
  const descriptions = [
    "The local-first home for your game, CG, and VFX projects. No account, no telemetry.",
    "Pick a theme. You can change it any time in Settings.",
    `Pipeline OS looks for engines, DCCs, and editors in ${platformExamples().standardLocations}, and finds your Unity, Unreal, and Godot projects. Add extra folders if you keep tools elsewhere.`,
    "Projects found on this computer. Recently used ones are pre-selected; add the rest any time from Projects.",
  ];

  return (
    <Dialog open={open}>
      <DialogContent showCloseButton={false} className="flex max-h-[92vh] flex-col sm:max-w-3xl [&>*]:min-w-0">
        <DialogHeader>
          {step === 0 ? <BrandGlyph className="mb-1 h-12 w-12 rounded-xl" /> : null}
          <DialogTitle className="text-xl">{titles[step]}</DialogTitle>
          <DialogDescription>{descriptions[step]}</DialogDescription>
        </DialogHeader>

        <div className="-mx-1 min-h-0 flex-1 overflow-y-auto px-1">
        {step === 0 ? (
          <ul className="grid gap-3 py-1 sm:grid-cols-2">
            {VALUE.map(({ icon: Icon, title, text }) => (
              <li key={title} className="flex gap-3 rounded-lg border border-border p-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-primary/15 text-primary"><Icon size={16} aria-hidden /></span>
                <span><span className="block text-sm font-medium">{title}</span><span className="block text-xs text-muted-foreground">{text}</span></span>
              </li>
            ))}
          </ul>
        ) : null}

        {step === 1 ? <ThemePicker value={theme} onChange={(value) => { setTheme(value); onPreviewTheme?.(value); }} /> : null}

        {step === 2 ? (
          <div className="space-y-1.5 py-1 text-sm">
            <span className="font-medium">Extra folders to scan <span className="font-normal text-muted-foreground">(optional)</span></span>
            <PathInput ariaLabel="Scan locations" directory multi className="min-w-0" placeholder={platformExamples().toolFolders} value={scanRoots} onChange={setScanRoots} />
          </div>
        ) : null}

        {step === 3 ? <div className="py-1"><DiscoveredProjects compact selection={selection} showActions={false} onAdded={() => onProjectsAdded?.()} /></div> : null}
        </div>

        <DialogFooter className="items-center border-t border-border pt-4 sm:justify-between">
          <span className="flex items-center gap-3">
            <Button variant="ghost" onClick={onSkip}>Skip setup</Button>
            <span className="flex items-center gap-1.5" aria-label={`Step ${step + 1} of ${STEPS}`}>{Array.from({ length: STEPS }, (_, index) => <span key={index} className={`h-1.5 w-5 rounded-full transition-colors ${index <= step ? "bg-primary" : "bg-secondary"}`} />)}<span className="ml-1 text-xs text-muted-foreground">{step + 1}/{STEPS}</span></span>
          </span>
          <div className="flex gap-2">
            {step > 0 ? <Button variant="outline" onClick={() => setStep((value) => value - 1)}>Back</Button> : null}
            {step < STEPS - 1 ? <Button autoFocus key={`next-${step}`} onClick={() => setStep((value) => value + 1)}>{step === 0 ? "Get started" : "Continue"}</Button>
              : <Button autoFocus disabled={finishing} onClick={() => void finish()} title="Your apps are scanned when setup finishes">{finishing ? <Loader2 size={15} className="animate-spin" /> : null}{chosen.length ? `Add ${chosen.length} project${chosen.length === 1 ? "" : "s"} & finish` : "Finish setup"}</Button>}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
