import { createPortal } from "react-dom";
import type { DragState } from "../lib/pointer-drag";

/** The card that follows the pointer while dragging on a board. */
export function DragGhost<T>({ drag }: { drag: DragState<T> | null }) {
  if (!drag) return null;
  return createPortal(
    <div
      aria-hidden
      className="pointer-events-none fixed z-[200] rotate-2 rounded-lg border border-primary/60 bg-card px-3 py-2 text-sm font-medium shadow-xl"
      style={{ left: drag.x + 8, top: drag.y + 8, width: Math.min(drag.width, 320) }}
    >
      <span className="line-clamp-2">{drag.label}</span>
    </div>,
    document.body,
  );
}
