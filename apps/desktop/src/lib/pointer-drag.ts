// Pointer-based drag and drop for boards. HTML5 drag-and-drop is unreliable
// inside the desktop WebView (the OS file-drop handler intercepts it), so
// cards follow the pointer and drop onto elements marked `data-drop-zone`.
import { useCallback, useEffect, useRef, useState } from "react";

export type DragState<T> = { item: T; x: number; y: number; zone: string | null; width: number; label: string };

const THRESHOLD = 5;

function zoneAt(x: number, y: number): string | null {
  if (typeof document.elementFromPoint !== "function") return null;
  const element = document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-drop-zone]");
  return element?.dataset.dropZone ?? null;
}

/**
 * Returns `drag` (current drag, for rendering a ghost and highlighting the
 * zone under the pointer) and `bind(item, label)` to spread on each card.
 * `onDrop(item, zone)` fires when a card is released over a zone.
 */
export function usePointerDrag<T>(onDrop: (item: T, zone: string) => void) {
  const [drag, setDrag] = useState<DragState<T> | null>(null);
  const pending = useRef<{ item: T; label: string; startX: number; startY: number; width: number; pointerId: number } | null>(null);
  const dragging = useRef<DragState<T> | null>(null);
  const suppressClick = useRef(false);
  const dropRef = useRef(onDrop);
  useEffect(() => { dropRef.current = onDrop; }, [onDrop]);

  useEffect(() => {
    function move(event: PointerEvent) {
      const start = pending.current;
      if (!start || event.pointerId !== start.pointerId) return;
      if (!dragging.current && Math.hypot(event.clientX - start.startX, event.clientY - start.startY) < THRESHOLD) return;
      const next = { item: start.item, label: start.label, width: start.width, x: event.clientX, y: event.clientY, zone: zoneAt(event.clientX, event.clientY) };
      dragging.current = next;
      setDrag(next);
      event.preventDefault();
    }
    function up(event: PointerEvent) {
      const start = pending.current;
      if (!start || event.pointerId !== start.pointerId) return;
      const current = dragging.current;
      pending.current = null;
      dragging.current = null;
      setDrag(null);
      if (current) {
        suppressClick.current = true;
        window.setTimeout(() => { suppressClick.current = false; }, 0);
        const zone = zoneAt(event.clientX, event.clientY);
        if (zone) dropRef.current(current.item, zone);
      }
    }
    function cancel(event: KeyboardEvent) {
      if (event.key === "Escape" && pending.current) {
        pending.current = null;
        dragging.current = null;
        setDrag(null);
      }
    }
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    window.addEventListener("pointercancel", up);
    window.addEventListener("keydown", cancel);
    return () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      window.removeEventListener("pointercancel", up);
      window.removeEventListener("keydown", cancel);
    };
  }, []);

  const bind = useCallback((item: T, label: string) => ({
    onPointerDown: (event: React.PointerEvent<HTMLElement>) => {
      if (event.button !== 0) return;
      const target = event.target as HTMLElement;
      // Let inputs, menus, and buttons inside a card work normally.
      if (target.closest("input, textarea, select, [role=menuitem], [data-no-drag]")) return;
      pending.current = { item, label, startX: event.clientX, startY: event.clientY, width: (event.currentTarget as HTMLElement).offsetWidth, pointerId: event.pointerId };
    },
    // A drag ends with a click on the card; don't treat it as "open".
    onClickCapture: (event: React.MouseEvent) => {
      if (suppressClick.current) { event.stopPropagation(); event.preventDefault(); }
    },
    style: { touchAction: "none" as const },
  }), []);

  return { drag, bind };
}
