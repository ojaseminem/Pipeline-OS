import "@testing-library/jest-dom/vitest";
import { beforeEach } from "vitest";

// Tests exercise the steady-state app, not the first-run experience; mark
// onboarding complete so the setup dialog never intercepts interactions.
beforeEach(() => {
  localStorage.setItem("vantadeck.onboarded", "true");
});

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
