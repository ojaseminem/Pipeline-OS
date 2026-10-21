import { SHORTCUTS } from "../lib/shortcuts";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";

export function Kbd({ children }: { children: string }) {
  return <kbd className="rounded border border-border bg-muted px-1.5 py-0.5 font-mono text-[11px] text-muted-foreground">{children}</kbd>;
}

export function ShortcutsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Keyboard shortcuts</DialogTitle>
          <DialogDescription>Everything in Pipeline OS is reachable from the keyboard. On macOS use ⌘ for Ctrl.</DialogDescription>
        </DialogHeader>
        <ul className="divide-y divide-border">
          {SHORTCUTS.map((shortcut) => (
            <li key={shortcut.label} className="flex items-center justify-between gap-4 py-2 text-sm">
              <span>{shortcut.label}</span>
              <span className="flex shrink-0 gap-1">{shortcut.keys.map((key) => <Kbd key={key}>{key}</Kbd>)}</span>
            </li>
          ))}
        </ul>
      </DialogContent>
    </Dialog>
  );
}
