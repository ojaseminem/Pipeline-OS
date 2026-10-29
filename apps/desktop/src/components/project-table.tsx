// Home project list. A launcher's core job is launching, so each row leads
// with "Launch" (open in the engine at the pinned version); clicking the row
// opens the project's workspace.
import { Clipboard, FolderOpen, GitBranch, KanbanSquare, LayoutDashboard, MoreHorizontal, Pin, Play, Workflow } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import { cn } from "@/lib/utils";
import type { Project } from "../data";
import { formatLastOpened } from "../lib/format";
import { categoryClass } from "../lib/categories";
import { ProjectThumb } from "./thumbnail";
import { AppIcon } from "./app-icon";
import { EmptyState } from "./empty-state";
import { FolderPlus } from "lucide-react";

export type ProjectActions = {
  onOpen: (project: Project, tab?: string) => void;
  onLaunch: (project: Project) => void;
  onOpenFolder: (project: Project) => void;
  onCopyPath: (project: Project) => void;
  onAddProject?: () => void;
  /** Pin/unpin to Home; omit to hide the pin button. */
  onTogglePin?: (project: Project) => void;
  isPinned?: (project: Project) => boolean;
};

const COLUMNS = "grid-cols-[minmax(0,2.2fr)_minmax(0,1.4fr)_auto] lg:grid-cols-[minmax(0,2.2fr)_minmax(0,1.4fr)_minmax(0,1fr)_auto]";

export function ProjectTable({ projects, actions, emptyText }: { projects: Project[]; actions: ProjectActions; emptyText?: string }) {
  return (
    <div className="overflow-hidden rounded-xl border border-border bg-card" role="table" aria-label="Projects">
      <div className={cn("grid gap-3 border-b border-border bg-muted/40 px-4 py-2 text-xs font-medium uppercase tracking-wide text-muted-foreground", COLUMNS)} role="row">
        <span role="columnheader">Project</span><span role="columnheader">Engine</span><span role="columnheader" className="hidden lg:block">Last opened</span><span role="columnheader" className="sr-only">Actions</span>
      </div>
      {projects.length === 0 ? (
        <EmptyState compact icon={FolderPlus} title={emptyText ?? "No projects here yet"} description="Pin projects you use often, or add a project folder to get started." action={actions.onAddProject ? <Button size="sm" onClick={actions.onAddProject}><FolderPlus size={14} /> Add project</Button> : null} />
      ) : projects.map((project) => (
        <div
          key={project.path}
          role="row"
          tabIndex={0}
          onClick={() => actions.onOpen(project)}
          onKeyDown={(event) => { if (event.key === "Enter" && event.target === event.currentTarget) actions.onOpen(project); }}
          className={cn("grid cursor-pointer items-center gap-3 border-b border-border px-4 py-2.5 text-sm transition-colors last:border-0 hover:bg-muted/40 focus-visible:bg-muted/40", COLUMNS)}
        >
          <span className="flex min-w-0 items-center gap-3" role="cell">
            <ProjectThumb projectPath={project.path} thumbnail={project.thumbnail} className="h-9 w-14 shrink-0" alt="" />
            <span className="flex min-w-0 flex-col">
              <span className="flex items-center gap-2"><strong className="truncate font-medium">{project.name}</strong>{project.category ? <span className={cn("shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium", categoryClass(project.category))}>{project.category}</span> : null}</span>
              <small className="truncate text-xs text-muted-foreground" title={project.path}>{project.path}</small>
            </span>
          </span>
          <span className="flex min-w-0 flex-col text-muted-foreground" role="cell">
            <span className="flex min-w-0 items-center gap-1.5"><AppIcon executable={project.engineExecutable ?? undefined} size={15} /><span className="truncate">{project.engine}{project.version ? ` ${project.version}` : ""}</span></span>
            {project.branch ? <span className="flex min-w-0 items-center gap-1 text-xs"><GitBranch size={12} className="shrink-0" /><span className="truncate">{project.branch}</span></span> : null}
          </span>
          <span className="hidden whitespace-nowrap text-muted-foreground lg:block" role="cell">{formatLastOpened(project.lastOpened) || "—"}</span>
          <span className="flex items-center justify-end gap-1" role="cell" onClick={(event) => event.stopPropagation()}>
            {actions.onTogglePin ? (() => {
              const pinned = actions.isPinned?.(project) ?? false;
              return <Button variant="ghost" size="icon" className={cn("h-8 w-8", pinned ? "text-primary" : "text-muted-foreground")} aria-pressed={pinned} aria-label={pinned ? `Unpin ${project.name}` : `Pin ${project.name}`} title={pinned ? "Unpin from Home" : "Pin to Home"} onClick={() => actions.onTogglePin?.(project)}>{pinned ? <Pin size={15} className="fill-current" /> : <Pin size={15} />}</Button>;
            })() : null}
            <Button size="sm" variant="outline" className="hover:border-primary/50 [&_svg]:text-primary" disabled={!project.engineId} title={project.engineId ? `Open in ${project.engine}` : "Link an engine to launch"} onClick={() => actions.onLaunch(project)}><Play size={13} /> Launch</Button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant="ghost" size="icon" className="h-8 w-8" aria-label={`More actions for ${project.name}`}><MoreHorizontal size={16} /></Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-52">
                <DropdownMenuItem onClick={() => actions.onOpen(project)}><LayoutDashboard size={14} /> Open workspace</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onOpen(project, "source")}><GitBranch size={14} /> Source control</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onOpen(project, "pipeline")}><Workflow size={14} /> Pipeline tasks</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onOpen(project, "board")}><KanbanSquare size={14} /> Board</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={() => actions.onOpenFolder(project)}><FolderOpen size={14} /> Show in folder</DropdownMenuItem>
                <DropdownMenuItem onClick={() => actions.onCopyPath(project)}><Clipboard size={14} /> Copy path</DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </span>
        </div>
      ))}
    </div>
  );
}
