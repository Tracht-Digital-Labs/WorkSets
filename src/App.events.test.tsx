import { act, render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";
import App from "./App";
import { worksetsApi, worksetsEvents } from "./api";
import { compatibilityFixture, presetFixture, settingsFixture } from "./test/fixtures";

async function renderNativeApp() {
  window.__TAURI_INTERNALS__ = {};
  const preset = presetFixture();
  vi.spyOn(worksetsApi, "getAppState").mockResolvedValue({
    version: "0.1.0-alpha.1",
    settings: settingsFixture(),
    presets: [preset],
    compatibility: compatibilityFixture(),
    busy: false,
    dataDirectory: "C:\\WorkSets",
  });
  vi.spyOn(worksetsApi, "captureWorkspace").mockResolvedValue({ preset, warnings: [] });
  vi.spyOn(worksetsApi, "planPresetApply").mockResolvedValue({
    presetId: preset.id,
    canApply: true,
    desktopChanges: 1,
    reusedWindows: 1,
    launchedWindows: 0,
    closedWindows: 0,
    adjustedWindows: 0,
    risks: [{
      id: "foreign",
      code: "foreignWindowsOnRemovedDesktops",
      kind: "foreignWindows",
      title: "Fremde Fenster",
      detail: "Fenster schützen",
      blocking: true,
      affectedItemIds: [],
    }],
  });

  let trayHandler: ((presetId: string) => void) | undefined;
  let shortcutHandler: ((shortcut: string) => void) | undefined;
  vi.spyOn(worksetsEvents, "onTrayApply").mockImplementation(async (handler) => {
    trayHandler = handler;
    return vi.fn();
  });
  vi.spyOn(worksetsEvents, "onGlobalShortcut").mockImplementation(async (handler) => {
    shortcutHandler = handler;
    return vi.fn();
  });

  render(<App />);
  expect(await screen.findByRole("heading", { name: "Bereit für den nächsten Kontext?" })).toBeVisible();
  await waitFor(() => {
    expect(trayHandler).toBeTypeOf("function");
    expect(shortcutHandler).toBeTypeOf("function");
  });
  return {
    preset,
    tray: (presetId: string) => trayHandler?.(presetId),
    shortcut: (shortcut: string) => shortcutHandler?.(shortcut),
  };
}

describe("App native events", () => {
  it("öffnet die Erfassung über den globalen Hotkey", async () => {
    const events = await renderNativeApp();
    expect(document.documentElement).toHaveAttribute("lang", "de");

    act(() => events.shortcut("Control+Shift+KeyW"));
    expect(await screen.findByRole("dialog", { name: "Aktuelles Setup erfassen" })).toBeVisible();
  });

  it("öffnet ein Preset über das Tray", async () => {
    const events = await renderNativeApp();

    act(() => events.tray(events.preset.id));
    expect(await screen.findByRole("dialog", { name: "„Fokus“ aktivieren" })).toBeVisible();
  });

  it("öffnet ein Preset über dessen globalen Hotkey", async () => {
    const events = await renderNativeApp();

    act(() => events.shortcut("Alt+Control+Digit1"));
    expect(await screen.findByRole("dialog", { name: "„Fokus“ aktivieren" })).toBeVisible();
  });
});
