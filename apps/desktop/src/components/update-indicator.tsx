// Top-right update status: a download icon when an update is available, a
// progress ring while it downloads (hover for details, click to cancel), and
// clear next steps when it's ready, installing, or needs the installer.
import { AlertTriangle, ArrowDownToLine, Check, ExternalLink, Loader2, RotateCw, X } from "lucide-react";
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { desktopApi, isNativeRuntime, onUpdateState, openExternal, type UpdateInfo, type UpdateState } from "../bridge";

const mb = (bytes: number) => `${(bytes / 1_048_576).toFixed(1)} MB`;

function Ring({ value }: { value: number | null }) {
  const radius = 8;
  const circumference = 2 * Math.PI * radius;
  return (
    <svg viewBox="0 0 20 20" className="size-5 -rotate-90" aria-hidden>
      <circle cx="10" cy="10" r={radius} className="fill-none stroke-secondary" strokeWidth="2.5" />
      <circle cx="10" cy="10" r={radius} className={cn("fill-none stroke-primary transition-[stroke-dashoffset]", value == null && "animate-spin origin-center")} strokeWidth="2.5" strokeLinecap="round"
        strokeDasharray={circumference} strokeDashoffset={value == null ? circumference * 0.7 : circumference * (1 - value / 100)} />
    </svg>
  );
}

export function UpdateIndicator({ update, releaseNotesUrl }: { update: UpdateInfo | null; releaseNotesUrl: string }) {
  const [state, setState] = useState<UpdateState | null>(null);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!isNativeRuntime()) return;
    let off: (() => void) | undefined;
    let disposed = false;
    desktopApi.updateState().then(setState).catch(() => undefined);
    void onUpdateState(setState).then((unlisten) => { if (disposed) unlisten(); else off = unlisten; });
    return () => { disposed = true; off?.(); };
  }, []);

  const phase = state?.phase ?? "idle";
  const busy = phase === "downloading" || phase === "installing" || phase === "restarting";
  if (!update?.available && !busy && phase !== "ready" && phase !== "failed") return null;

  const version = state?.version ?? update?.version ?? "";
  const percent = state?.total ? Math.min(100, Math.round((state.downloaded / state.total) * 100)) : null;

  async function download() {
    try { setState(await desktopApi.updateDownload()); }
    catch (error) { toast.error("Couldn't start the update", { description: error instanceof Error ? error.message : String(error) }); }
  }
  async function install() {
    try { await desktopApi.updateInstall(); }
    catch { /* the state event carries the explanation */ }
  }
  async function installer() {
    const url = state?.installerUrl ?? (await desktopApi.updateInstallerUrl().catch(() => null));
    void openExternal(url ?? releaseNotesUrl);
  }

  const label = {
    idle: `Pipeline OS ${version} is available`,
    cancelled: `Pipeline OS ${version} is available`,
    downloading: percent != null ? `Downloading update: ${percent}%` : "Downloading update…",
    ready: `Update ${version} is ready to install`,
    installing: "Installing update…",
    restarting: "Restarting…",
    failed: "The update didn't install",
  }[phase];

  const icon = phase === "downloading" ? <Ring value={percent} />
    : phase === "installing" || phase === "restarting" ? <Loader2 size={17} className="animate-spin text-primary" />
      : phase === "ready" ? <span className="relative"><ArrowDownToLine size={17} className="text-success" /><Check size={9} className="absolute -bottom-0.5 -right-1 rounded-full bg-success text-background" /></span>
        : phase === "failed" ? <AlertTriangle size={17} className="text-warning" />
          : <span className="relative"><ArrowDownToLine size={17} className="text-primary" /><span className="absolute -right-1 -top-1 size-2 rounded-full bg-primary" /></span>;

  return (
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon" aria-label={label} title={phase === "downloading" && state?.total ? `${label} (${mb(state.downloaded)} of ${mb(state.total)})` : label}>{icon}</Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-80 space-y-3 p-3">
        <div>
          <p className="text-sm font-medium">{label}</p>
          {update?.notes && (phase === "idle" || phase === "cancelled") ? <p className="mt-1 line-clamp-3 text-xs text-muted-foreground">{update.notes}</p> : null}
        </div>
        {phase === "downloading" ? (
          <div className="space-y-1.5">
            <Progress value={percent ?? 5} className="h-1.5" />
            <p className="text-xs tabular-nums text-muted-foreground">{state?.total ? `${mb(state.downloaded)} of ${mb(state.total)}` : mb(state?.downloaded ?? 0)} · keeps going in the background</p>
          </div>
        ) : null}
        {state?.blocker && phase !== "failed" ? <p className="rounded-md bg-warning/10 p-2 text-xs text-warning">{state.blocker}</p> : null}
        {phase === "failed" && state?.error ? <p className="rounded-md bg-destructive/10 p-2 text-xs text-destructive">{state.error}</p> : null}
        <div className="flex flex-wrap justify-end gap-1.5">
          <Button variant="ghost" size="sm" onClick={() => void openExternal(releaseNotesUrl)}><ExternalLink size={13} /> What's new</Button>
          {phase === "idle" || phase === "cancelled" ? <Button size="sm" onClick={() => void download()}><ArrowDownToLine size={13} /> Download</Button> : null}
          {phase === "downloading" ? <Button variant="outline" size="sm" onClick={() => void desktopApi.updateCancel()}><X size={13} /> Cancel</Button> : null}
          {phase === "ready" ? <><Button variant="outline" size="sm" onClick={() => setOpen(false)}>Later</Button><Button size="sm" onClick={() => void install()}><RotateCw size={13} /> Install &amp; restart</Button></> : null}
          {phase === "failed" ? <><Button variant="outline" size="sm" onClick={() => void installer()}><ExternalLink size={13} /> Get installer</Button><Button size="sm" onClick={() => void download()}><RotateCw size={13} /> Try again</Button></> : null}
          {state?.blocker && (phase === "idle" || phase === "ready" || phase === "cancelled") ? <Button variant="outline" size="sm" onClick={() => void installer()}>Get installer</Button> : null}
        </div>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
