// Promise-based confirmation dialog: `if (!(await confirmAction("…"))) return;`
// Replaces window.confirm with an accessible, themed dialog that names the
// action on its button and visibly marks destructive operations.
import { useSyncExternalStore } from "react";
import { currentConfirm, settle, subscribe } from "../lib/confirm";
import { AlertTriangle } from "lucide-react";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";

/** Mount once near the app root. */
export function ConfirmHost() {
  const current = useSyncExternalStore(subscribe, currentConfirm, () => null);
  return (
    <Dialog open={!!current} onOpenChange={(open) => { if (!open) settle(false); }}>
      <DialogContent className="max-w-md" onOpenAutoFocus={(event) => {
        // Focus the safe choice for destructive actions, the action otherwise.
        event.preventDefault();
        const selector = current?.destructive ? "[data-confirm-cancel]" : "[data-confirm-ok]";
        (document.querySelector(selector) as HTMLElement | null)?.focus();
      }}>
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            {current?.destructive ? <AlertTriangle size={18} className="text-destructive" aria-hidden /> : null}
            {current?.title}
          </DialogTitle>
          {current?.description ? <DialogDescription className="whitespace-pre-line">{current.description}</DialogDescription> : null}
        </DialogHeader>
        <DialogFooter>
          <Button data-confirm-cancel variant="ghost" onClick={() => settle(false)}>{current?.cancelLabel ?? "Cancel"}</Button>
          <Button data-confirm-ok variant={current?.destructive ? "destructive" : "default"} onClick={() => settle(true)}>{current?.confirmLabel ?? "Continue"}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
