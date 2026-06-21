import { useState } from "react";
import { Boxes, Code2, Palette, Rocket, Box, Wrench } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";
import type { ThemePreference } from "../theme";

export type OnboardingPrefs = {
  role: string;
  interests: string[];
  scanRoots: string;
  theme: ThemePreference;
};

const ROLES = [
  { id: "artist", label: "Artist", icon: Palette, hint: "2D / 3D / texturing" },
  { id: "developer", label: "Developer", icon: Code2, hint: "gameplay / tools / engine" },
  { id: "technical-artist", label: "Technical Artist", icon: Wrench, hint: "pipeline / shaders / rigging" },
  { id: "generalist", label: "Generalist / Studio", icon: Rocket, hint: "a bit of everything" },
];

const INTERESTS = [
  { id: "game-engine", label: "Game Engines", icon: Box },
  { id: "dcc", label: "3D / DCC & VFX", icon: Boxes },
  { id: "art", label: "2D / Art & Texturing", icon: Palette },
  { id: "code", label: "Code & IDEs", icon: Code2 },
];

export function Onboarding({ open, onComplete, onSkip }: { open: boolean; onComplete: (prefs: OnboardingPrefs) => void; onSkip: () => void }) {
  const [step, setStep] = useState(0);
  const [role, setRole] = useState("");
  const [interests, setInterests] = useState<string[]>([]);
  const [scanRoots, setScanRoots] = useState("");
  const [theme, setTheme] = useState<ThemePreference>("system");

  const toggleInterest = (id: string) =>
    setInterests((current) => (current.includes(id) ? current.filter((value) => value !== id) : [...current, id]));

  const titles = ["Welcome to Vantadeck", "What do you work with?", "Where should we look?"];
  const descriptions = [
    "Your local-first launcher for projects, engines, and creative tools. A couple of quick questions to tailor it to you.",
    "Pick what you use most — we'll prioritize these in your library and quick launch.",
    "Choose where to scan for installed apps and your preferred appearance. You can change all of this later in Settings.",
  ];

  const canAdvance = step === 0 ? role !== "" : step === 1 ? interests.length > 0 : true;

  return (
    <Dialog open={open}>
      <DialogContent showCloseButton={false} className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{titles[step]}</DialogTitle>
          <DialogDescription>{descriptions[step]}</DialogDescription>
        </DialogHeader>

        {step === 0 ? (
          <div className="grid grid-cols-2 gap-3 py-2">
            {ROLES.map(({ id, label, icon: Icon, hint }) => (
              <button key={id} onClick={() => setRole(id)} className={cn("flex flex-col items-start gap-1 rounded-lg border p-3 text-left transition-colors", role === id ? "border-primary bg-primary/10" : "border-border hover:bg-muted/50")}>
                <Icon size={20} className="text-primary" /><span className="font-medium">{label}</span><span className="text-xs text-muted-foreground">{hint}</span>
              </button>
            ))}
          </div>
        ) : null}

        {step === 1 ? (
          <div className="grid grid-cols-2 gap-3 py-2">
            {INTERESTS.map(({ id, label, icon: Icon }) => (
              <button key={id} onClick={() => toggleInterest(id)} className={cn("flex items-center gap-2 rounded-lg border p-3 text-left transition-colors", interests.includes(id) ? "border-primary bg-primary/10" : "border-border hover:bg-muted/50")}>
                <Icon size={18} className="text-primary" /><span className="text-sm font-medium">{label}</span>
              </button>
            ))}
          </div>
        ) : null}

        {step === 2 ? (
          <div className="space-y-4 py-2">
            <label className="block space-y-1.5 text-sm"><span className="font-medium">Scan locations</span>
              <Input placeholder="Blank = all drives, or e.g. D:/Tools; E:/Apps" value={scanRoots} onChange={(event) => setScanRoots(event.target.value)} />
              <span className="text-xs text-muted-foreground">Leave blank to auto-scan standard install folders on every drive.</span>
            </label>
            <label className="flex items-center gap-3 text-sm"><span className="font-medium">Appearance</span>
              <select value={theme} onChange={(event) => setTheme(event.target.value as ThemePreference)} className="h-8 rounded-md border border-border bg-secondary px-2 text-sm">
                <option value="system">System</option><option value="dark">Dark</option><option value="light">Light</option>
              </select>
            </label>
          </div>
        ) : null}

        <DialogFooter className="sm:justify-between">
          <Button variant="ghost" onClick={onSkip}>Skip setup</Button>
          <div className="flex gap-2">
            {step > 0 ? <Button variant="outline" onClick={() => setStep((value) => value - 1)}>Back</Button> : null}
            {step < 2 ? <Button disabled={!canAdvance} onClick={() => setStep((value) => value + 1)}>Continue</Button>
              : <Button onClick={() => onComplete({ role, interests, scanRoots, theme })}>Finish &amp; scan</Button>}
          </div>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
