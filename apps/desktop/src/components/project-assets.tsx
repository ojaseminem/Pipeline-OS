// Project "Assets" tab: concept art, references, source art, models, audio
// and video in one browsable library with shared review status, plus studio
// folder layouts that give every file a predictable home.
import { useEffect, useMemo, useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Box, Clipboard, FileAudio, FileText, Film, FolderOpen, FolderTree, ImageIcon, Layers, MoreHorizontal, Search, SquarePen } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import { desktopApi, isNativeRuntime, studioApi, studioAvailable, type AssetEntry, type AssetKind, type ReviewStatus } from "../bridge";
import { confirmAction } from "../lib/confirm";
import { REVIEW_META } from "../lib/asset-meta";
import { EmptyState } from "./empty-state";

const KIND_META: Record<AssetKind, { label: string; icon: typeof Box }> = {
  image: { label: "Images", icon: ImageIcon },
  source: { label: "Source art", icon: Layers },
  model: { label: "3D", icon: Box },
  video: { label: "Video", icon: Film },
  audio: { label: "Audio", icon: FileAudio },
  document: { label: "Docs", icon: FileText },
};

const PREVIEWABLE = /\.(png|jpe?g|webp|gif|bmp)$/i;

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 ** 2) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
}

function relativeTime(epochSeconds: number): string {
  const diff = Math.max(0, Date.now() / 1000 - epochSeconds);
  if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function join(root: string, relative: string): string {
  return `${root.replace(/[\\/]+$/, "")}/${relative}`;
}

/** Loads an image thumbnail only once it scrolls into view. */
function Thumb({ root, asset, className }: { root: string; asset: AssetEntry; className?: string }) {
  const ref = useRef<HTMLDivElement>(null);
  const [src, setSrc] = useState<string | null>(null);
  const previewable = asset.kind === "image" && PREVIEWABLE.test(asset.name) && isNativeRuntime();
  useEffect(() => {
    if (!previewable || !ref.current) return;
    let active = true;
    const observer = new IntersectionObserver((entries) => {
      if (!entries.some((entry) => entry.isIntersecting)) return;
      observer.disconnect();
      desktopApi.readImage(join(root, asset.path)).then((url) => { if (active) setSrc(url); }).catch(() => undefined);
    }, { rootMargin: "200px" });
    observer.observe(ref.current);
    return () => { active = false; observer.disconnect(); };
  }, [previewable, root, asset.path]);
  const Icon = KIND_META[asset.kind].icon;
  let hue = 0;
  for (const char of asset.folder) hue = (hue * 31 + char.charCodeAt(0)) % 360;
  return (
    <div ref={ref} className={cn("flex items-center justify-center overflow-hidden bg-secondary", className)} style={src ? undefined : { background: `linear-gradient(135deg, oklch(0.32 0.05 ${hue}), oklch(0.22 0.03 ${(hue + 60) % 360}))` }}>
      {src ? <img src={src} alt="" className="h-full w-full object-cover" loading="lazy" /> : <Icon size={28} className="text-white/70" aria-hidden />}
    </div>
  );
}

export function ProjectAssets({ projectPath, onCreateIssue }: { projectPath: string; onCreateIssue?: (asset: AssetEntry) => void }) {
  const available = studioAvailable();
  const queryClient = useQueryClient();
  const key = ["assets", projectPath];
  const assetsQuery = useQuery({ queryKey: key, queryFn: () => studioApi.assets(projectPath, 1200), enabled: available });
  const [kind, setKind] = useState<AssetKind | null>(null);
  const [status, setStatus] = useState<ReviewStatus | null>(null);
  const [folder, setFolder] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [preview, setPreview] = useState<AssetEntry | null>(null);
  const [layoutOpen, setLayoutOpen] = useState(false);

  const assets = useMemo(() => assetsQuery.data?.assets ?? [], [assetsQuery.data]);
  const topFolders = useMemo(() => [...new Set(assets.map((asset) => asset.folder.split("/")[0]).filter(Boolean))].sort(), [assets]);
  const counts = useMemo(() => {
    const result: Partial<Record<AssetKind, number>> = {};
    for (const asset of assets) result[asset.kind] = (result[asset.kind] ?? 0) + 1;
    return result;
  }, [assets]);
  const shown = assets.filter((asset) =>
    (!kind || asset.kind === kind)
    && (!status || asset.meta.status === status)
    && (!folder || asset.folder === folder || asset.folder.startsWith(`${folder}/`))
    && (!query || asset.path.toLowerCase().includes(query.toLowerCase())));

  async function setReview(asset: AssetEntry, next: ReviewStatus, note?: string | null) {
    const meta = { ...asset.meta, status: next, note: note === undefined ? asset.meta.note : note };
    queryClient.setQueryData(key, (data: typeof assetsQuery.data) => data && { ...data, assets: data.assets.map((item) => (item.path === asset.path ? { ...item, meta } : item)) });
    try { await studioApi.setAssetMeta(projectPath, asset.path, meta); }
    catch (error) { toast.error(error instanceof Error ? error.message : String(error)); await queryClient.invalidateQueries({ queryKey: key }); }
  }

  if (!available) return <EmptyState icon={ImageIcon} title="The asset library lives in the desktop app" description="Open Pipeline OS on your machine to browse concept art, references, and source files." />;

  const actions = (asset: AssetEntry) => (
    <DropdownMenuContent align="end" className="w-56">
      <DropdownMenuLabel>Review status</DropdownMenuLabel>
      {(Object.keys(REVIEW_META) as ReviewStatus[]).map((value) => <DropdownMenuItem key={value} onClick={() => void setReview(asset, value)}><span className={cn("h-2 w-2 rounded-full", REVIEW_META[value].className)} /> {REVIEW_META[value].label}</DropdownMenuItem>)}
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={!isNativeRuntime()} onClick={() => void desktopApi.openPath(join(projectPath, asset.path)).catch((error) => toast.error(String(error)))}><SquarePen size={14} /> Open in default app</DropdownMenuItem>
      <DropdownMenuItem disabled={!isNativeRuntime()} onClick={() => void desktopApi.openPath(join(projectPath, asset.folder)).catch((error) => toast.error(String(error)))}><FolderOpen size={14} /> Show in folder</DropdownMenuItem>
      <DropdownMenuItem onClick={() => { void navigator.clipboard?.writeText(asset.path); toast.success("Path copied."); }}><Clipboard size={14} /> Copy project path</DropdownMenuItem>
      {onCreateIssue ? <DropdownMenuItem onClick={() => onCreateIssue(asset)}><SquarePen size={14} /> Create issue about this</DropdownMenuItem> : null}
    </DropdownMenuContent>
  );

  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 className="text-base font-semibold">Asset library</h2>
          <p className="max-w-2xl text-sm text-muted-foreground">Concept art, references, source art, models, audio, and video across the project. Engine caches and git-ignored files are skipped. Review status is shared with the team in <code>.pipelineos/assets.toml</code>.</p>
        </div>
        <Button variant="outline" size="sm" onClick={() => setLayoutOpen(true)}><FolderTree size={14} /> Organize folders…</Button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-48 flex-1">
          <Search size={14} className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-muted-foreground" aria-hidden />
          <Input aria-label="Search assets" placeholder="Search file names and folders…" value={query} onChange={(event) => setQuery(event.target.value)} className="h-8 pl-8" />
        </div>
        <div className="flex flex-wrap gap-1" role="group" aria-label="Filter by type">
          <button onClick={() => setKind(null)} className={cn("rounded-full px-2.5 py-1 text-xs", !kind ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")} aria-pressed={!kind}>All {assets.length}</button>
          {(Object.keys(KIND_META) as AssetKind[]).filter((value) => counts[value]).map((value) => (
            <button key={value} onClick={() => setKind(value)} aria-pressed={kind === value} className={cn("rounded-full px-2.5 py-1 text-xs", kind === value ? "bg-primary text-primary-foreground" : "bg-secondary text-muted-foreground hover:text-foreground")}>{KIND_META[value].label} {counts[value]}</button>
          ))}
        </div>
        <select aria-label="Filter by review status" value={status ?? ""} onChange={(event) => setStatus((event.target.value || null) as ReviewStatus | null)} className="h-8 rounded-md border border-input bg-transparent px-2 text-sm">
          <option value="">Any status</option>
          {(Object.keys(REVIEW_META) as ReviewStatus[]).map((value) => <option key={value} value={value}>{REVIEW_META[value].label}</option>)}
        </select>
        {topFolders.length > 1 ? (
          <select aria-label="Filter by folder" value={folder ?? ""} onChange={(event) => setFolder(event.target.value || null)} className="h-8 rounded-md border border-input bg-transparent px-2 text-sm">
            <option value="">All folders</option>
            {topFolders.map((value) => <option key={value} value={value}>{value}/</option>)}
          </select>
        ) : null}
      </div>

      {assetsQuery.isLoading ? (
        <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(180px,1fr))]">{Array.from({ length: 8 }, (_, index) => <div key={index} className="h-48 animate-pulse rounded-xl bg-secondary/60" />)}</div>
      ) : !assets.length ? (
        <EmptyState icon={ImageIcon} title="No creative assets found yet" description="Drop concept art, references, or source files into the project, or set up a studio folder layout so everyone knows where things go." action={<Button onClick={() => setLayoutOpen(true)}><FolderTree size={15} /> Set up folders</Button>} />
      ) : !shown.length ? (
        <EmptyState compact icon={Search} title="Nothing matches these filters" action={<Button variant="outline" size="sm" onClick={() => { setKind(null); setStatus(null); setFolder(null); setQuery(""); }}>Clear filters</Button>} />
      ) : (
        <div className="grid gap-3 [grid-template-columns:repeat(auto-fill,minmax(180px,1fr))]">
          {shown.map((asset) => (
            <figure key={asset.path} className="group overflow-hidden rounded-xl border border-border bg-card transition-colors hover:border-primary/40">
              <button className="block w-full" onClick={() => setPreview(asset)} aria-label={`Preview ${asset.name}`}>
                <Thumb root={projectPath} asset={asset} className="aspect-[4/3] w-full" />
              </button>
              <figcaption className="space-y-1 p-2.5">
                <div className="flex items-center gap-1">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium" title={asset.path}>{asset.name}</span>
                  <DropdownMenu>
                    <DropdownMenuTrigger asChild><Button variant="ghost" size="icon" className="h-6 w-6 shrink-0" aria-label={`Actions for ${asset.name}`}><MoreHorizontal size={14} /></Button></DropdownMenuTrigger>
                    {actions(asset)}
                  </DropdownMenu>
                </div>
                <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                  <span className="truncate" title={asset.folder}>{asset.folder || "/"}</span>
                  <span className="ml-auto shrink-0">{relativeTime(asset.modified)}</span>
                </div>
                {asset.meta.status !== "none" ? <span className={cn("inline-block rounded-full px-2 py-0.5 text-[10px] font-medium", REVIEW_META[asset.meta.status].className)}>{REVIEW_META[asset.meta.status].label}</span> : null}
              </figcaption>
            </figure>
          ))}
        </div>
      )}
      {assetsQuery.data?.truncated ? <p className="text-xs text-muted-foreground">Showing the most recent {assets.length} assets.</p> : null}

      <AssetPreview root={projectPath} asset={preview} onClose={() => setPreview(null)} onReview={(asset, next, note) => { void setReview(asset, next, note); setPreview(null); }} />
      <LayoutDialog open={layoutOpen} projectPath={projectPath} onClose={() => setLayoutOpen(false)} onApplied={() => void queryClient.invalidateQueries({ queryKey: key })} />
    </div>
  );
}

function AssetPreview({ root, asset, onClose, onReview }: { root: string; asset: AssetEntry | null; onClose: () => void; onReview: (asset: AssetEntry, status: ReviewStatus, note: string | null) => void }) {
  const [note, setNote] = useState("");
  const [status, setStatus] = useState<ReviewStatus>("none");
  const [current, setCurrent] = useState<string | null>(null);
  if (asset && asset.path !== current) { setCurrent(asset.path); setNote(asset.meta.note ?? ""); setStatus(asset.meta.status); }
  return (
    <Dialog open={!!asset} onOpenChange={(open) => { if (!open) { onClose(); setCurrent(null); } }}>
      <DialogContent className="sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle className="truncate">{asset?.name}</DialogTitle>
          <DialogDescription>{asset ? `${asset.folder || "/"} · ${formatSize(asset.size)} · modified ${relativeTime(asset.modified)}` : null}</DialogDescription>
        </DialogHeader>
        {asset ? <Thumb root={root} asset={asset} className="max-h-[55vh] min-h-56 w-full rounded-lg [&_img]:object-contain" /> : null}
        {asset ? (
          <div className="grid gap-3 sm:grid-cols-[180px_1fr]">
            <label className="space-y-1 text-sm"><span className="font-medium">Review status</span>
              <select value={status} onChange={(event) => setStatus(event.target.value as ReviewStatus)} className="h-9 w-full rounded-md border border-input bg-transparent px-2 text-sm">
                {(Object.keys(REVIEW_META) as ReviewStatus[]).map((value) => <option key={value} value={value}>{REVIEW_META[value].label}</option>)}
              </select>
            </label>
            <label className="space-y-1 text-sm"><span className="font-medium">Feedback note</span><Textarea rows={2} value={note} onChange={(event) => setNote(event.target.value)} placeholder="e.g. Silhouette reads well; push the rim light" /></label>
          </div>
        ) : null}
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Close</Button>
          {asset ? <Button onClick={() => onReview(asset, status, note.trim() || null)}>Save review</Button> : null}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

function LayoutDialog({ open, projectPath, onClose, onApplied }: { open: boolean; projectPath: string; onClose: () => void; onApplied: () => void }) {
  const layouts = useQuery({ queryKey: ["asset-layouts"], queryFn: () => studioApi.layouts(), enabled: open, staleTime: Infinity });
  const [selected, setSelected] = useState<string>("game-art");
  const layout = layouts.data?.find((item) => item.id === selected) ?? layouts.data?.[0];
  async function apply() {
    if (!layout) return;
    if (!(await confirmAction({ title: `Create the "${layout.name}" folders?`, description: "Missing folders are created (with a .gitkeep so they can be committed). Nothing is moved, renamed, or deleted.", confirmLabel: "Create folders" }))) return;
    try {
      const result = await studioApi.applyLayout(projectPath, layout.id);
      toast.success(result.created.length ? `Created ${result.created.length} folder${result.created.length === 1 ? "" : "s"}.` : "All folders already exist.");
      onApplied();
      onClose();
    } catch (error) { toast.error(error instanceof Error ? error.message : String(error)); }
  }
  return (
    <Dialog open={open} onOpenChange={(value) => { if (!value) onClose(); }}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Organize project folders</DialogTitle>
          <DialogDescription>Pick a structure so concept art, references, source files, and deliveries always land in the same place, for everyone on the team.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-3 sm:grid-cols-[220px_1fr]">
          <div className="space-y-1.5" role="radiogroup" aria-label="Folder layouts">
            {(layouts.data ?? []).map((item) => (
              <button key={item.id} role="radio" aria-checked={selected === item.id} onClick={() => setSelected(item.id)} className={cn("w-full rounded-lg border p-2.5 text-left text-sm transition-colors", selected === item.id ? "border-primary bg-primary/10" : "border-border hover:bg-muted/40")}>
                <span className="block font-medium">{item.name}</span>
                <span className="mt-0.5 block text-xs text-muted-foreground">{item.description}</span>
              </button>
            ))}
          </div>
          <div className="max-h-80 overflow-y-auto rounded-lg border border-border bg-muted/20 p-3 font-mono text-xs">
            {layout?.folders.map((path) => {
              const depth = path.split("/").length - 1;
              return <div key={path} style={{ paddingLeft: depth * 14 }} className="flex items-center gap-1.5 py-0.5"><FolderTree size={12} className="text-primary" aria-hidden />{path.split("/").pop()}</div>;
            })}
          </div>
        </div>
        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button onClick={() => void apply()} disabled={!layout}>Create folders</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
