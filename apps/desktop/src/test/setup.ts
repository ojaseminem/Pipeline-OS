import "@testing-library/jest-dom/vitest";
import { afterEach, beforeEach } from "vitest";
import { cleanup } from "@testing-library/react";

// Tests exercise the steady-state app, not the first-run experience; mark
// onboarding complete so the setup dialog never intercepts interactions.
beforeEach(() => {
  localStorage.setItem("vantadeck.onboarded", "true");
  document.body.style.pointerEvents = "";
});

// Radix dialogs (command palette, onboarding) set body pointer-events: none while
// open; reset it between tests so a leaked overlay can't block later interactions.
afterEach(() => {
  cleanup();
  document.body.style.pointerEvents = "";
});

// jsdom lacks these APIs that Radix/cmdk rely on.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = globalThis.ResizeObserver ?? (ResizeObserverStub as unknown as typeof ResizeObserver);
if (!Element.prototype.scrollIntoView) {
  Element.prototype.scrollIntoView = () => {};
}

Object.defineProperty(window, "matchMedia", {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  }),
});

// jsdom has no PointerEvent; board drag-and-drop uses pointer events.
if (typeof window !== "undefined" && !("PointerEvent" in window)) {
  class PointerEventPolyfill extends MouseEvent {
    pointerId: number;
    constructor(type: string, init: PointerEventInit = {}) {
      super(type, init);
      this.pointerId = init.pointerId ?? 1;
    }
  }
  (window as unknown as { PointerEvent: typeof MouseEvent }).PointerEvent = PointerEventPolyfill;
}
