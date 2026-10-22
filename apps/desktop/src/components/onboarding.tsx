import { useState } from "react";
import { Bot, KanbanSquare, Rocket, Workflow } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { PathInput } from "./path-input";
import { BrandGlyph } from "./brand";
import { ThemePicker } from "./settings-screen";
import type { ThemePreference } from "../theme";
import { platformExamples } from "../lib/platform";

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

const STEPS = 3;

export function Onboarding({ open, onComplete, onSkip, initialTheme = "system", onPreviewTheme }: {
  open: boolean;
  onComplete: (prefs: OnboardingPrefs) => void;
  onSkip: () => void;
  initialTheme?: ThemePreference;
  onPreviewTheme?: (theme: ThemePreference) => void;
}) {
  const [step, setStep] = useState(0);
  const [scanRoots, setScanRoots] = useState("");
  const [theme, setTheme] = useState<ThemePreference>(initialTheme);
  const titles = ["Welcome to Pipeline OS", "Make it yours", "Find your creative apps"];
  const descriptions = [
    "The local-first home for your game, CG, and VFX projects. No account, no telemetry.",
    "Pick a theme. You can change it any time in Settings.",
    `Pipeline OS looks for engines, DCCs, and editors in ${platformExamples().standardLocations}, and finds your Unity, Unreal, and Godot projects. Add extra folders if you keep tools elsewhere.`,
  ];

  return (
    <Dialog open={open}>
      <DialogContent showCloseButton={false} className="sm:max-w-2xl">
        <DialogHeader>
          {step === 0 ? <BrandGlyph className="mb-1 h-12 w-12 rounded-xl" /> : null}
          <DialogTitle className="text-xl">{titles[step]}</DialogTitle>
          <DialogDescription>{descriptions[step]}</DialogDescription>
        </DialogHeader>

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

        <DialogFooter className="items-center sm:justify-between">
          <span className="flex items-center gap-3">
            <Button variant="ghost" onClick={onSkip}>Skip setup</Button>
            <span className="flex gap-1.5" aria-label={`Step ${step + 1} of ${STEPS}`}>{Array.from({ length: STEPS }, (_, index) => <span key={index} className={`h-1.5 w-5 rounded-full ${index <= step ? "bg-primary" : "bg-secondary"}`} />)}</span>
          </span>
          <div className="flex gap-2">
            {step > 0 ? <Button variant="outline" onClick={() => setStep((value) => value - 1)}>Back</Button> : null}
            {step < STEPS - 1 ? <Button onClick={() => setStep((value) => value + 1)}>{step === 0 ? "Get started" : "Continue"}</Button>
              : <Button onClick={() => onComplete({ scanRoots, theme })}>Scan and finish</Button>}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
