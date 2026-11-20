import { createBlankPreset } from "../api";
import type { AppSettings, ApplyReport, CompatibilityInfo, LaunchItem, PresetV1 } from "../types";

export const settingsFixture = (overrides: Partial<AppSettings> = {}): AppSettings => ({
  language: "de",
  theme: "system",
  runAtStartup: false,
  minimizeToTray: true,
  telemetryOptIn: false,
  checkForUpdates: false,
  onboardingComplete: true,
  defaultPolicy: "exactProtected",
  captureHotkey: "Ctrl+Shift+W",
  ...overrides,
});

export const compatibilityFixture = (overrides: Partial<CompatibilityInfo> = {}): CompatibilityInfo => ({
  supported: true,
  platform: "Windows 11",
  architecture: "x64",
  windowsBuild: 26200,
  desktopCapability: "experimental",
  ...overrides,
});

export function presetFixture(overrides: Partial<PresetV1> = {}): PresetV1 {
  const preset = createBlankPreset("Fokus");
  const monitor = preset.monitors[0];
  const item: LaunchItem = {
    id: "11111111-1111-4111-8111-111111111111",
    displayName: "Editor",
    desktopId: preset.desktops[0].id,
    launchTarget: { kind: "executable", path: "C:\\Apps\\Editor.exe", arguments: ["--new-window"] },
    selector: {
      executablePath: "C:\\Apps\\Editor.exe",
      windowClass: "EditorWindow",
      titleContains: "Editor",
      instanceIndex: 0,
    },
    placement: {
      monitorId: monitor.id,
      x: 100,
      y: 120,
      width: 960,
      height: 720,
      normalizedX: (100 - monitor.workArea.x) / monitor.workArea.width,
      normalizedY: (120 - monitor.workArea.y) / monitor.workArea.height,
      normalizedWidth: 960 / monitor.workArea.width,
      normalizedHeight: 720 / monitor.workArea.height,
      state: "normal",
    },
    lifecycle: "reuse",
    enabled: true,
  };
  return {
    ...preset,
    id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
    name: "Fokus",
    description: "Konzentriertes Arbeiten",
    items: [item],
    hotkey: "Ctrl+Alt+1",
    ...overrides,
  };
}

export const reportFixture = (overrides: Partial<ApplyReport> = {}): ApplyReport => ({
  id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
  presetId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  status: "success",
  startedAt: "2026-08-21T08:00:00.000Z",
  finishedAt: "2026-08-21T08:00:01.000Z",
  steps: [{ id: "preflight", label: "preflight", state: "completed" }],
  issues: [],
  reusedWindows: 1,
  launchedWindows: 0,
  closedWindows: 0,
  adjustedWindows: 0,
  ...overrides,
});
