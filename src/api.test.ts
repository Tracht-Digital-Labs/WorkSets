import type { Event } from "@tauri-apps/api/event";
import { vi } from "vitest";
import type {
  BackendApplyReport,
  BackendCompatibility,
  BackendSettings,
} from "./backendTypes";

const { invokeMock, listenMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  listenMock: vi.fn(),
}));

vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));
vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));

import { createNewPresetDraft, presetToBackend, worksetsApi, worksetsEvents } from "./api";
import { presetFixture, settingsFixture } from "./test/fixtures";

const backendSettings = (): BackendSettings => ({
  ...settingsFixture(),
  language: "de",
  closeBehavior: "tray",
  hotkeys: { "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa": "Ctrl+Alt+1" },
});

const backendCompatibility = (): BackendCompatibility => ({
  os: { major: 10, minor: 0, build: 26200, revision: 100, architecture: "x64" },
  supported: true,
  virtualDesktops: {
    provider: "windows11_25h2",
    providerVersion: "26200",
    experimental: true,
    canRead: true,
    canCreate: true,
    canRemove: true,
    canMoveWindows: true,
    mutationsDisabled: false,
    isolatedProcess: true,
  },
  windows: { canCapture: true, canLaunch: true, canPlace: true, canCloseManaged: true },
  warnings: [],
});

const backendReport = (): BackendApplyReport => ({
  id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
  presetId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  startedAt: "2026-08-21T08:00:00.000Z",
  finishedAt: "2026-08-21T08:00:01.000Z",
  status: "success",
  steps: [{ step: "preflight", status: "success", durationMs: 4 }],
  items: [],
  notices: [],
  launchedCount: 0,
  reusedCount: 1,
  closedCount: 0,
  adjustedCount: 0,
});

describe("native WorkSets API", () => {
  it("verdrahtet alle Tauri-Commands mit dem kanonischen camelCase-Vertrag", async () => {
    window.__TAURI_INTERNALS__ = {};
    const preset = presetToBackend(presetFixture());
    const nativeMonitor = { ...preset.monitors[0], id: "native-display-9", deviceName: "\\\\.\\DISPLAY9" };
    const nativePreset: typeof preset = {
      ...preset,
      monitors: [nativeMonitor],
      items: preset.items.map((item) => ({ ...item, placement: { ...item.placement, monitor: nativeMonitor } })),
    };
    const compatibility = backendCompatibility();
    const settings = backendSettings();
    const report = backendReport();

    invokeMock.mockImplementation(async (command: string, args?: Record<string, unknown>) => {
      switch (command) {
        case "get_app_state": return {
          version: "0.1.0-alpha.1",
          settings,
          presets: [preset],
          compatibility,
          busy: false,
          dataDirectory: "C:\\WorkSets",
          lastApplyReport: report,
        };
        case "list_presets": return [preset];
        case "capture_workspace": return { preset: nativePreset, capturedWindows: [], warnings: [] };
        case "save_preset": return args?.preset;
        case "delete_preset": return undefined;
        case "plan_preset_apply": return {
          presetId: preset.id,
          targetDesktopCount: 1,
          currentDesktopCount: 1,
          requiresConfirmation: false,
          actions: [{ kind: "reuseWindow", itemId: preset.items[0].id }],
          foreignWindows: [],
          warnings: [],
        };
        case "apply_preset": return report;
        case "cancel_apply": return undefined;
        case "get_settings": return settings;
        case "save_settings": return args?.settings;
        case "get_compatibility": return compatibility;
        case "get_diagnostics": return {
          version: "0.1.0-alpha.1",
          compatibility,
          presetCount: 1,
          busy: false,
          dataDirectory: "C:\\WorkSets",
          lastApplyStatus: "success",
          brokerProtocolVersion: 1,
          privacy: { telemetryTransportEnabled: false, telemetryContainsNamesPathsUrlsOrTitles: false },
        };
        case "check_for_updates": return {
          checkedAt: "2026-08-21T08:00:00.000Z",
          currentVersion: "0.1.0-alpha.1",
          available: false,
        };
        case "export_preset": return JSON.stringify(preset);
        case "import_preset": return { preset, committed: true, warnings: [] };
        case "get_preset_schema": return { title: "PresetV1", type: "object" };
        default: throw new Error(`Unexpected command ${command}`);
      }
    });

    const state = await worksetsApi.getAppState();
    expect(state.compatibility.platform).toBe("Windows 11");
    expect(state.lastApplyReport?.id).toBe(report.id);
    await worksetsApi.listPresets();
    await worksetsApi.captureWorkspace("Test");
    const newDraft = await createNewPresetDraft();
    expect(newDraft.items).toEqual([]);
    expect(newDraft.monitors.map((monitor) => monitor.id)).toEqual(["native-display-9"]);
    expect(newDraft.monitors.some((monitor) => monitor.friendlyName === "Studio Display")).toBe(false);
    await worksetsApi.savePreset(presetFixture());
    await worksetsApi.getSettings();
    await worksetsApi.saveSettings(settingsFixture());
    await worksetsApi.deletePreset(preset.id);
    await worksetsApi.planPresetApply(preset.id);
    await worksetsApi.applyPreset(preset.id, { confirmForeignWindows: true, forceOwnedProcesses: true });
    await worksetsApi.applyPreset(preset.id);
    await worksetsApi.cancelApply();
    await worksetsApi.getCompatibility();
    await worksetsApi.getDiagnostics();
    await worksetsApi.checkForUpdates();
    await worksetsApi.exportPreset(preset.id);
    await worksetsApi.importPreset(JSON.stringify(preset), true);
    await worksetsApi.getPresetSchema();

    expect(invokeMock).toHaveBeenCalledWith("capture_workspace", { name: "Test" });
    expect(invokeMock).toHaveBeenCalledWith("apply_preset", {
      presetId: preset.id,
      options: { confirmDesktopChanges: true, forceCloseManaged: true, startupTimeoutMs: 10_000 },
    });
    expect(invokeMock).toHaveBeenCalledWith("apply_preset", {
      presetId: preset.id,
      options: { confirmDesktopChanges: false, forceCloseManaged: false, startupTimeoutMs: 10_000 },
    });
    expect(invokeMock).toHaveBeenCalledWith("save_settings", {
      settings: expect.objectContaining({
        captureHotkey: "Ctrl+Shift+W",
        hotkeys: { [preset.id]: "Ctrl+Alt+1" },
      }),
    });
    expect(invokeMock.mock.calls.map(([command]) => command)).toEqual(expect.arrayContaining([
      "get_app_state",
      "list_presets",
      "capture_workspace",
      "save_preset",
      "delete_preset",
      "plan_preset_apply",
      "apply_preset",
      "cancel_apply",
      "get_settings",
      "save_settings",
      "get_compatibility",
      "get_diagnostics",
      "check_for_updates",
      "export_preset",
      "import_preset",
      "get_preset_schema",
    ]));
  });

  it("übersetzt Tray-, Hotkey- und Apply-Progress-Ereignisse", async () => {
    window.__TAURI_INTERNALS__ = {};
    const listeners = new Map<string, (event: Event<unknown>) => void>();
    listenMock.mockImplementation(async (eventName: string, handler: (event: Event<unknown>) => void) => {
      listeners.set(eventName, handler);
      return vi.fn();
    });
    const tray = vi.fn();
    const shortcut = vi.fn();
    const progress = vi.fn();

    await worksetsEvents.onTrayApply(tray);
    await worksetsEvents.onGlobalShortcut(shortcut);
    await worksetsEvents.onApplyProgress(progress);
    listeners.get("worksets-tray-apply")?.({ event: "worksets-tray-apply", id: 1, payload: { presetId: "preset-1" } });
    listeners.get("worksets-global-shortcut")?.({ event: "worksets-global-shortcut", id: 2, payload: { shortcut: "Ctrl+Alt+1" } });
    listeners.get("worksets-apply-progress")?.({
      event: "worksets-apply-progress",
      id: 3,
      payload: { presetId: "preset-1", step: "closeObsolete", status: "success", message: "done" },
    });

    expect(tray).toHaveBeenCalledWith("preset-1");
    expect(shortcut).toHaveBeenCalledWith("Ctrl+Alt+1");
    expect(progress).toHaveBeenCalledWith({ presetId: "preset-1", step: "close", state: "completed", detail: "done" });
  });

  it("klassifiziert Broker-, optionale und fehlende Zielhinweise ohne Fehlalarm", async () => {
    window.__TAURI_INTERNALS__ = {};
    invokeMock.mockResolvedValue({
      presetId: "preset-1",
      targetDesktopCount: 2,
      currentDesktopCount: 1,
      requiresConfirmation: true,
      actions: [],
      foreignWindows: [
        { nativeId: "1", title: "One", application: "App", desktopIndex: 2 },
        { nativeId: "2", title: "Two", application: "App", desktopIndex: 2 },
      ],
      warnings: [
        { code: "experimentalDesktopProvider", message: "Experimental", severity: "warning" },
        { code: "leaveOpenWindowMissing", message: "Optional window is closed", severity: "info" },
        { code: "windowNotManageable", message: "Elevated window is skipped", severity: "warning" },
        { code: "launchTargetNotFound", message: "Required target is missing", severity: "error" },
        { code: "adapterNotice", message: "Informational", severity: "info" },
        { code: "foreignWindowsOnRemovedDesktops", message: "Move unmanaged windows", severity: "warning" },
      ],
    });

    const plan = await worksetsApi.planPresetApply("preset-1");

    expect(plan.canApply).toBe(false);
    expect(plan.risks.map(({ kind, blocking }) => ({ kind, blocking }))).toEqual([
      { kind: "desktopExperimental", blocking: false },
      { kind: "missingTarget", blocking: false },
      { kind: "elevatedWindow", blocking: false },
      { kind: "missingTarget", blocking: true },
      { kind: "notice", blocking: false },
      { kind: "foreignWindows", blocking: true },
    ]);
    expect(plan.risks.find((risk) => risk.kind === "foreignWindows")?.count).toBe(2);
  });

  it("mappt eine experimentelle Sitzungsbestätigung ohne fingierte Fremdfenster", async () => {
    window.__TAURI_INTERNALS__ = {};
    invokeMock.mockImplementation(async (command: string) => {
      if (command === "plan_preset_apply") {
        return {
          presetId: "preset-1",
          targetDesktopCount: 2,
          currentDesktopCount: 1,
          requiresConfirmation: true,
          confirmationReason: "experimentalDesktopProvider",
          actions: [{ kind: "createDesktops", count: 1 }],
          foreignWindows: [],
          warnings: [{
            code: "experimentalDesktopProvider",
            message: "Virtual desktop control is experimental.",
            severity: "warning",
          }],
        };
      }
      if (command === "apply_preset") return backendReport();
      throw new Error(`Unexpected command ${command}`);
    });

    const plan = await worksetsApi.planPresetApply("preset-1");

    expect(plan.confirmationReason).toBe("experimentalDesktopProvider");
    expect(plan.risks).toEqual([
      expect.objectContaining({ kind: "desktopExperimental", blocking: true }),
    ]);
    expect(plan.risks.some((risk) => risk.kind === "foreignWindows")).toBe(false);

    await worksetsApi.applyPreset("preset-1", { confirmDesktopMutation: true });
    expect(invokeMock).toHaveBeenLastCalledWith("apply_preset", {
      presetId: "preset-1",
      options: { confirmDesktopChanges: true, forceCloseManaged: false, startupTimeoutMs: 10_000 },
    });
  });
});
