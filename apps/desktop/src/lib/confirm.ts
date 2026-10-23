// Promise-based confirmation: `if (!(await confirmAction("…"))) return;`
// The dialog itself is <ConfirmHost/> (components/confirm.tsx).

export type ConfirmOptions = {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  destructive?: boolean;
};

type Pending = ConfirmOptions & { resolve: (ok: boolean) => void };

let pending: Pending | null = null;
export const currentConfirm = () => pending;
const listeners = new Set<() => void>();
const notify = () => listeners.forEach((listener) => listener());

/** Asks the user to confirm. Resolves false on cancel, Escape, or dismiss. */
export function confirmAction(options: ConfirmOptions | string): Promise<boolean> {
  const normalized = typeof options === "string" ? { title: options } : options;
  pending?.resolve(false);
  return new Promise((resolve) => {
    pending = { ...normalized, resolve };
    notify();
  });
}

export function settle(ok: boolean) {
  const current = pending;
  pending = null;
  notify();
  current?.resolve(ok);
}

export function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

