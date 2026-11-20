import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import i18n from "../i18n";

class ResizeObserverStub implements ResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(globalThis, "ResizeObserver", { value: ResizeObserverStub, writable: true });
Object.defineProperty(globalThis, "PointerEvent", { value: MouseEvent, writable: true });
Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { value: vi.fn(), writable: true });
Object.defineProperty(HTMLElement.prototype, "setPointerCapture", { value: vi.fn(), writable: true });
Object.defineProperty(HTMLElement.prototype, "releasePointerCapture", { value: vi.fn(), writable: true });
Object.defineProperty(navigator, "clipboard", {
  configurable: true,
  value: { writeText: vi.fn().mockResolvedValue(undefined) },
});

beforeEach(async () => {
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    writable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
  localStorage.clear();
  await i18n.changeLanguage("de");
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  delete window.__TAURI_INTERNALS__;
});
