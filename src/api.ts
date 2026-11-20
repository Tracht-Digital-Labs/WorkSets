import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  BackendAppState,
  BackendApplyProgress,
  BackendApplyOptions,
  BackendApplyPlan,
  BackendApplyReport,
  BackendCaptureResult,
  BackendCompatibility,
  BackendDiagnostics,
  BackendImportResult,
  BackendLaunchTarget,
  BackendMonitorRef,
  BackendPresetV1,
  BackendSettings,
  BackendUpdateInfo,
} from "./backendTypes";
import type {
  AppSettings,
  AppState,
  ApplyOptions,
  ApplyPlan,
  ApplyProgressEvent,
  ApplyReport,
  ApplyRisk,
  CaptureResult,
  CompatibilityInfo,
  DiagnosticCheck,
  Diagnostics,
  ImportResult,
  LaunchItem,
  MonitorRef,
  PresetV1,
  UpdateInfo,
} from "./types";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

const STORAGE_KEY = "worksets.demo.state.v1";

export const isTauriRuntime = () => typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);

const id = () => globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
const isoNow = () => new Date().toISOString();

function monitorFromBackend(monitor: BackendMonitorRef): MonitorRef {
  return { ...monitor, friendlyName: monitor.deviceName };
}

function monitorToBackend(monitor: MonitorRef): BackendMonitorRef {
  const { friendlyName: _, ...backend } = monitor;
  return backend;
}

function targetFromBackend(target: BackendLaunchTarget): LaunchItem["launchTarget"] {
  if (target.kind === "executable") {
    return { kind: target.kind, path: target.path, arguments: target.args, workingDirectory: target.workingDirectory };
  }
  if (target.kind === "packagedApp") return { kind: target.kind, aumid: target.aumid, arguments: target.args };
  return target;
}

function targetToBackend(target: LaunchItem["launchTarget"]): BackendLaunchTarget {
  if (target.kind === "executable") {
    return { kind: target.kind, path: target.path, args: target.arguments, workingDirectory: target.workingDirectory };
  }
  if (target.kind === "packagedApp") return { kind: target.kind, aumid: target.aumid, args: target.arguments };
  return target;
}

export function presetFromBackend(preset: BackendPresetV1): PresetV1 {
  return {
    ...preset,
    schemaVersion: 1,
    monitors: preset.monitors.map(monitorFromBackend),
    items: preset.items.map((item) => ({
      ...item,
      launchTarget: targetFromBackend(item.launchTarget),
      selector: {
        executablePath: item.selector.executablePath,
        aumid: item.selector.aumid,
        windowClass: item.selector.className,
        titleContains: item.selector.titlePattern,
        instanceIndex: item.selector.instanceIndex,
      },
      placement: {
        monitorId: item.placement.monitor.id,
        ...item.placement.physical,
        normalizedX: item.placement.normalized.x,
        normalizedY: item.placement.normalized.y,
        normalizedWidth: item.placement.normalized.width,
        normalizedHeight: item.placement.normalized.height,
        state: item.placement.state,
      },
    })),
  };
}

export function presetToBackend(preset: PresetV1): BackendPresetV1 {
  return {
    ...preset,
    monitors: preset.monitors.map(monitorToBackend),
    items: preset.items.map((item) => {
      const monitor = preset.monitors.find((candidate) => candidate.id === item.placement.monitorId) ?? preset.monitors[0] ?? mockMonitors[0];
      return {
        id: item.id,
        displayName: item.displayName,
        desktopId: item.desktopId,
        launchTarget: targetToBackend(item.launchTarget),
        selector: {
          executablePath: item.selector.executablePath,
          aumid: item.selector.aumid,
          className: item.selector.windowClass,
          titlePattern: item.selector.titleContains,
          instanceIndex: item.selector.instanceIndex,
        },
        placement: {
          monitor: monitorToBackend(monitor),
          physical: {
            x: item.placement.x,
            y: item.placement.y,
            width: item.placement.width,
            height: item.placement.height,
          },
          normalized: {
            x: item.placement.normalizedX,
            y: item.placement.normalizedY,
            width: item.placement.normalizedWidth,
            height: item.placement.normalizedHeight,
          },
          state: item.placement.state,
        },
        lifecycle: item.lifecycle,
        enabled: item.enabled,
      };
    }),
  };
}

function compatibilityFromBackend(compatibility: BackendCompatibility): CompatibilityInfo {
  const desktopCapability = !compatibility.virtualDesktops.canRead || compatibility.virtualDesktops.provider === "unavailable"
    ? "unavailable"
    : compatibility.virtualDesktops.experimental
      ? "experimental"
      : "available";
  return {
    supported: compatibility.supported,
    platform: compatibility.os.build >= 22_000 ? "Windows 11" : "Windows 10",
    architecture: compatibility.os.architecture,
    windowsBuild: compatibility.os.build,
    desktopCapability,
    message: compatibility.virtualDesktops.lastError ?? (compatibility.warnings.map((notice) => notice.message).join(" ") || undefined),
  };
}

function settingsFromBackend(settings: BackendSettings): AppSettings {
  return {
    language: settings.language === "en" ? "en" : "de",
    theme: settings.theme,
    runAtStartup: settings.runAtStartup,
    minimizeToTray: settings.minimizeToTray,
    telemetryOptIn: settings.telemetryOptIn,
    checkForUpdates: settings.checkForUpdates,
    onboardingComplete: settings.onboardingComplete,
    defaultPolicy: settings.defaultPolicy,
    captureHotkey: settings.captureHotkey,
  };
}

function settingsToBackend(settings: AppSettings, presets: PresetV1[]): BackendSettings {
  return {
    ...settings,
    closeBehavior: settings.minimizeToTray ? "tray" : "quit",
    hotkeys: Object.fromEntries(presets.flatMap((preset) => (preset.hotkey ? [[preset.id, preset.hotkey]] : []))),
  };
}

function reportFromBackend(report: BackendApplyReport): ApplyReport {
  const status = report.status === "needsAttention" ? "partial" : report.status;
  return {
    id: report.id,
    presetId: report.presetId,
    startedAt: report.startedAt,
    finishedAt: report.finishedAt,
    status,
    steps: report.steps.map((step) => ({
      id: step.step,
      label: step.step,
      state: step.status === "success" ? "completed" : step.status === "skipped" ? "pending" : step.status,
      detail: step.message,
    })),
    issues: [
      ...report.notices.map((notice, index) => ({
        id: notice.code,
        occurrenceId: `notice-${notice.code}-${index}`,
        severity: notice.severity,
        title: notice.code,
        detail: notice.message,
      })),
      ...report.items.filter((item) => item.status === "failed" || item.status === "skipped").map((item) => ({
        id: item.itemId,
        occurrenceId: `item-${item.itemId}`,
        severity: item.status === "failed" ? "error" as const : "warning" as const,
        title: item.displayName,
        detail: item.message ?? item.status,
        itemId: item.itemId,
      })),
    ],
    reusedWindows: report.reusedCount,
    launchedWindows: report.launchedCount,
    closedWindows: report.closedCount,
    adjustedWindows: report.adjustedCount,
  };
}

function planFromBackend(plan: BackendApplyPlan): ApplyPlan {
  const reuse = plan.actions.filter((action) => action.kind === "reuseWindow").length;
  const launch = plan.actions.filter((action) => action.kind === "launchItem").length;
  const hasForeignWindows = plan.foreignWindows.length > 0;
  const hasExperimentalWarning = plan.warnings.some((notice) => notice.code.toLowerCase().includes("experimental"));
  const confirmForeignWindows = plan.requiresConfirmation && hasForeignWindows;
  const confirmExperimentalDesktop = plan.requiresConfirmation && (
    plan.confirmationReason === "experimentalDesktopProvider"
    || (!plan.confirmationReason && !hasForeignWindows && hasExperimentalWarning)
  );
  const risks: ApplyRisk[] = plan.warnings.map((notice, index) => {
    const code = notice.code.toLowerCase();
    const kind = code.includes("foreign")
      ? "foreignWindows" as const
      : code.includes("monitor")
        ? "missingMonitor" as const
        : code.includes("elevated") || code.includes("notmanageable")
          ? "elevatedWindow" as const
          : code.includes("experimental") || code.includes("desktop") || code.includes("circuit")
            ? "desktopExperimental" as const
            : code.includes("launchtarget") || code.includes("missing") || code.includes("notfound")
              ? "missingTarget" as const
              : "notice" as const;
    const optionalMissingTarget = code.includes("leaveopen");
    const requiredMissingTarget = kind === "missingTarget" && !optionalMissingTarget && (
      notice.severity === "error" || code.includes("launchtarget") || code.includes("required") || code.includes("notfound")
    );
    return {
      id: `${notice.code}-${index}`,
      code: notice.code,
      kind,
      title: notice.code,
      detail: notice.message,
      blocking: (kind === "foreignWindows" && confirmForeignWindows)
        || (kind === "desktopExperimental" && confirmExperimentalDesktop)
        || requiredMissingTarget,
      affectedItemIds: [],
      count: kind === "foreignWindows" ? plan.foreignWindows.length : undefined,
    };
  });
  if (confirmForeignWindows && !risks.some((risk) => risk.kind === "foreignWindows")) {
    risks.unshift({
      id: "foreign-windows",
      code: "foreignWindowsOnRemovedDesktops",
      kind: "foreignWindows",
      title: `${plan.foreignWindows.length} unmanaged windows`,
      detail: "Unmanaged windows must be moved before extra desktops are removed.",
      blocking: true,
      affectedItemIds: [],
      count: plan.foreignWindows.length,
    });
  }
  if (confirmExperimentalDesktop && !risks.some((risk) => risk.kind === "desktopExperimental")) {
    risks.unshift({
      id: "experimental-desktop-provider",
      code: "experimentalDesktopProvider",
      kind: "desktopExperimental",
      title: "Experimental desktop control",
      detail: "Virtual desktop mutations require confirmation for this session.",
      blocking: true,
      affectedItemIds: [],
    });
  }
  return {
    presetId: plan.presetId,
    confirmationReason: plan.confirmationReason,
    canApply: !plan.warnings.some((notice) => notice.severity === "error"),
    desktopChanges: Math.abs(plan.targetDesktopCount - plan.currentDesktopCount),
    reusedWindows: reuse,
    launchedWindows: launch,
    closedWindows: 0,
    adjustedWindows: plan.actions.filter((action) => action.kind === "placeWindow").length,
    risks,
  };
}

function optionsToBackend(options?: ApplyOptions): BackendApplyOptions {
  return {
    confirmDesktopChanges: Boolean(options?.confirmForeignWindows || options?.confirmDesktopMutation),
    forceCloseManaged: Boolean(options?.forceOwnedProcesses),
    startupTimeoutMs: 10_000,
  };
}

const mockMonitors: MonitorRef[] = [
  {
    id: "monitor-primary",
    deviceName: "DISPLAY1",
    friendlyName: "Studio Display",
    bounds: { x: 0, y: 0, width: 2560, height: 1440 },
    workArea: { x: 0, y: 0, width: 2560, height: 1380 },
    dpiX: 120,
    dpiY: 120,
    primary: true,
  },
  {
    id: "monitor-secondary",
    deviceName: "DISPLAY2",
    friendlyName: "Side Display",
    bounds: { x: -1920, y: 160, width: 1920, height: 1080 },
    workArea: { x: -1920, y: 160, width: 1920, height: 1040 },
    dpiX: 96,
    dpiY: 96,
    primary: false,
  },
];

function makeItem(
  displayName: string,
  desktopId: string,
  monitorId: string,
  placement: [number, number, number, number],
  target: LaunchItem["launchTarget"],
  lifecycle: LaunchItem["lifecycle"] = "reuse",
): LaunchItem {
  const [x, y, width, height] = placement;
  const monitor = mockMonitors.find((entry) => entry.id === monitorId) ?? mockMonitors[0];
  return {
    id: id(),
    displayName,
    desktopId,
    launchTarget: target,
    selector: {
      executablePath: target.kind === "executable" ? target.path : undefined,
      aumid: target.kind === "packagedApp" ? target.aumid : undefined,
      titleContains: displayName,
      instanceIndex: 0,
    },
    placement: {
      monitorId,
      x,
      y,
      width,
      height,
      normalizedX: (x - monitor.workArea.x) / monitor.workArea.width,
      normalizedY: (y - monitor.workArea.y) / monitor.workArea.height,
      normalizedWidth: width / monitor.workArea.width,
      normalizedHeight: height / monitor.workArea.height,
      state: "normal",
    },
    lifecycle,
    enabled: true,
  };
}

export function createBlankPreset(name = "Untitled"): PresetV1 {
  const desktopId = id();
  const now = isoNow();
  return {
    schemaVersion: 1,
    id: id(),
    name,
    description: "",
    createdAt: now,
    updatedAt: now,
    desktopPolicy: "exactProtected",
    desktops: [{ id: desktopId, name: "Desktop 1", index: 0 }],
    monitors: structuredClone(mockMonitors),
    items: [],
  };
}

function capturedPreset(name = "Captured workspace"): PresetV1 {
  const preset = createBlankPreset(name);
  const secondDesktop = { id: id(), name: "Desktop 2", index: 1 };
  preset.desktops.push(secondDesktop);
  preset.items = [
    makeItem(
      "Microsoft Edge",
      preset.desktops[0].id,
      "monitor-primary",
      [80, 72, 1480, 1180],
      { kind: "executable", path: "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe", arguments: [] },
    ),
    makeItem(
      "Visual Studio Code",
      preset.desktops[0].id,
      "monitor-secondary",
      [-1840, 230, 1760, 900],
      { kind: "executable", path: "C:\\Users\\demo\\AppData\\Local\\Programs\\Microsoft VS Code\\Code.exe", arguments: [] },
    ),
    makeItem(
      "Microsoft Teams",
      secondDesktop.id,
      "monitor-primary",
      [1610, 72, 860, 1180],
      { kind: "packagedApp", aumid: "MSTeams_8wekyb3d8bbwe!MSTeams", arguments: [] },
      "leaveOpen",
    ),
    {
      ...makeItem(
        "PowerShell (Administrator)",
        secondDesktop.id,
        "monitor-primary",
        [520, 320, 1100, 700],
        { kind: "executable", path: "C:\\Program Files\\PowerShell\\7\\pwsh.exe", arguments: [] },
      ),
      enabled: false,
      elevated: true,
    },
  ];
  return preset;
}

const defaultSettings: AppSettings = {
  language: "de",
  theme: "system",
  runAtStartup: false,
  minimizeToTray: true,
  telemetryOptIn: false,
  checkForUpdates: true,
  onboardingComplete: false,
  defaultPolicy: "exactProtected",
  captureHotkey: "Ctrl+Shift+W",
};

const defaultCompatibility: CompatibilityInfo = {
  supported: true,
  platform: "Windows 11 Pro",
  architecture: "x64",
  windowsBuild: 26200,
  desktopCapability: "experimental",
  message: "Browser fixture for Windows 11 build 26200",
};

interface MockState {
  presets: PresetV1[];
  settings: AppSettings;
  compatibility: CompatibilityInfo;
  lastApplyReport?: ApplyReport;
}

function initialMockState(): MockState {
  const focus = capturedPreset("Fokus & Recherche");
  focus.description = "Recherche links, Schreiben in der Mitte, Kommunikation separat.";
  focus.items = focus.items.filter((item) => !item.elevated);
  return {
    presets: [focus],
    settings: defaultSettings,
    compatibility: defaultCompatibility,
  };
}

function readMockState(): MockState {
  if (typeof localStorage === "undefined") return initialMockState();
  const raw = localStorage.getItem(STORAGE_KEY);
  if (!raw) {
    const state = initialMockState();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    return state;
  }
  try {
    const parsed = JSON.parse(raw) as Partial<MockState>;
    return {
      presets: Array.isArray(parsed.presets) ? parsed.presets : [],
      settings: { ...defaultSettings, ...parsed.settings },
      compatibility: { ...defaultCompatibility, ...parsed.compatibility },
      lastApplyReport: parsed.lastApplyReport,
    };
  } catch {
    const state = initialMockState();
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    return state;
  }
}

function writeMockState(state: MockState) {
  if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
}

const wait = (milliseconds: number) => new Promise((resolve) => window.setTimeout(resolve, milliseconds));

function invokeOrMock<T>(command: string, args: Record<string, unknown> | undefined, mock: () => Promise<T> | T): Promise<T> {
  if (isTauriRuntime()) return invoke<T>(command, args);
  return Promise.resolve(mock());
}

async function invokeMapped<TBackend, TUi>(
  command: string,
  args: Record<string, unknown> | undefined,
  map: (value: TBackend) => TUi,
  mock: () => Promise<TUi> | TUi,
): Promise<TUi> {
  if (isTauriRuntime()) return map(await invoke<TBackend>(command, args));
  return mock();
}

let cachedPresets: PresetV1[] = [];

function errorMessage(reason: unknown): string {
  if (reason instanceof Error) return reason.message;
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object") {
    const candidate = reason as { message?: unknown; code?: unknown };
    if (typeof candidate.message === "string") return candidate.message;
    if (typeof candidate.code === "string") return candidate.code;
  }
  return String(reason);
}

function applyProgressFromBackend(progress: BackendApplyProgress): ApplyProgressEvent {
  const step = progress.step === "placement"
    ? "launch"
    : progress.step === "closeObsolete"
      ? "close"
      : progress.step === "activate"
        ? "finalize"
        : progress.step;
  const state = progress.status === "success" || progress.status === "skipped"
    ? "completed"
    : progress.status === "pending" || progress.status === "running" || progress.status === "warning" || progress.status === "failed"
      ? progress.status
      : "running";
  return { presetId: progress.presetId, step, state, detail: progress.message };
}

async function listenWhenNative<T>(eventName: string, handler: (payload: T) => void): Promise<UnlistenFn> {
  if (!isTauriRuntime()) return () => undefined;
  return listen<T>(eventName, (event) => handler(event.payload));
}

export const worksetsEvents = {
  onTrayApply(handler: (presetId: string) => void): Promise<UnlistenFn> {
    return listenWhenNative<{ presetId: string }>("worksets-tray-apply", ({ presetId }) => handler(presetId));
  },

  onGlobalShortcut(handler: (shortcut: string) => void): Promise<UnlistenFn> {
    return listenWhenNative<{ shortcut: string }>("worksets-global-shortcut", ({ shortcut }) => handler(shortcut));
  },

  onApplyProgress(handler: (progress: ApplyProgressEvent) => void): Promise<UnlistenFn> {
    return listenWhenNative<BackendApplyProgress>("worksets-apply-progress", (progress) => {
      handler(applyProgressFromBackend(progress));
    });
  },
};

export const worksetsApi = {
  async getAppState(): Promise<AppState> {
    const state = await invokeMapped<BackendAppState, AppState>("get_app_state", undefined, (backend) => ({
      version: backend.version,
      presets: backend.presets.map(presetFromBackend),
      settings: settingsFromBackend(backend.settings),
      compatibility: compatibilityFromBackend(backend.compatibility),
      busy: backend.busy,
      dataDirectory: backend.dataDirectory,
      lastApplyReport: backend.lastApplyReport ? reportFromBackend(backend.lastApplyReport) : undefined,
    }), () => {
      const mock = readMockState();
      return {
        version: "0.1.0-alpha.1",
        presets: mock.presets,
        settings: mock.settings,
        compatibility: mock.compatibility,
        busy: false,
        dataDirectory: "%LOCALAPPDATA%\\Tracht Digital Solutions\\WorkSets",
        lastApplyReport: mock.lastApplyReport,
      };
    });
    cachedPresets = state.presets;
    return state;
  },

  async listPresets(): Promise<PresetV1[]> {
    const presets = await invokeMapped<BackendPresetV1[], PresetV1[]>("list_presets", undefined, (value) => value.map(presetFromBackend), () => readMockState().presets);
    cachedPresets = presets;
    return presets;
  },

  getCompatibility(): Promise<CompatibilityInfo> {
    return invokeMapped<BackendCompatibility, CompatibilityInfo>("get_compatibility", undefined, compatibilityFromBackend, () => readMockState().compatibility);
  },

  async captureWorkspace(name?: string): Promise<CaptureResult> {
    return invokeMapped<BackendCaptureResult, CaptureResult>("capture_workspace", name ? { name } : undefined, (backend) => {
      const preset = presetFromBackend(backend.preset);
      preset.items = preset.items.map((item) => {
        const captured = backend.capturedWindows.find((window) =>
          window.executablePath?.toLowerCase() === item.selector.executablePath?.toLowerCase()
          && window.className === item.selector.windowClass,
        );
        return captured ? { ...item, elevated: captured.elevated, enabled: item.enabled && captured.manageable } : item;
      });
      return { preset, warnings: backend.warnings };
    }, async () => {
      await wait(360);
      return {
        preset: capturedPreset(name),
        warnings: [{
          code: "elevatedWindowCaptured",
          message: "One elevated window cannot be managed.",
          severity: "warning",
        }],
      };
    });
  },

  async savePreset(preset: PresetV1): Promise<PresetV1> {
    const savedPreset = { ...preset, name: preset.name.trim(), updatedAt: isoNow() };
    const result = await invokeMapped<BackendPresetV1, PresetV1>("save_preset", { preset: presetToBackend(savedPreset) }, presetFromBackend, () => {
      const state = readMockState();
      const index = state.presets.findIndex((entry) => entry.id === savedPreset.id);
      if (index >= 0) state.presets[index] = structuredClone(savedPreset);
      else state.presets.unshift(structuredClone(savedPreset));
      writeMockState(state);
      return savedPreset;
    });
    cachedPresets = [result, ...cachedPresets.filter((entry) => entry.id !== result.id)];
    return result;
  },

  async deletePreset(presetId: string): Promise<void> {
    await invokeOrMock("delete_preset", { presetId }, () => {
      const state = readMockState();
      state.presets = state.presets.filter((preset) => preset.id !== presetId);
      writeMockState(state);
    });
    cachedPresets = cachedPresets.filter((preset) => preset.id !== presetId);
  },

  planPresetApply(presetId: string): Promise<ApplyPlan> {
    return invokeMapped<BackendApplyPlan, ApplyPlan>("plan_preset_apply", { presetId }, planFromBackend, () => {
      const state = readMockState();
      const preset = state.presets.find((entry) => entry.id === presetId);
      if (!preset) throw new Error("Preset not found");
      const enabled = preset.items.filter((item) => item.enabled);
      const elevated = enabled.filter((item) => item.elevated);
      const risks: ApplyRisk[] = elevated.map((item) => ({
        id: `elevated-${item.id}`,
        code: "windowNotManageable",
        kind: "elevatedWindow" as const,
        title: item.displayName,
        detail: "The elevated window will be skipped.",
        blocking: false,
        affectedItemIds: [item.id],
      }));
      if (preset.desktopPolicy === "exactProtected" && preset.desktops.length > 1) {
        risks.push({
          id: "foreign-windows",
          code: "foreignWindowsOnRemovedDesktops",
          kind: "foreignWindows",
          title: "2 unrelated windows",
          detail: "They must be moved before an extra desktop can be removed.",
          blocking: true,
          affectedItemIds: [],
          count: 2,
        });
      }
      return {
        presetId,
        canApply: true,
        desktopChanges: Math.max(0, preset.desktops.length - 1),
        reusedWindows: Math.max(0, enabled.length - 1),
        launchedWindows: Math.min(1, enabled.length),
        closedWindows: 1,
        adjustedWindows: 0,
        risks,
      };
    });
  },

  async applyPreset(presetId: string, options?: ApplyOptions): Promise<ApplyReport> {
    return invokeMapped<BackendApplyReport, ApplyReport>("apply_preset", { presetId, options: optionsToBackend(options) }, reportFromBackend, async () => {
      await wait(1150);
      const state = readMockState();
      const preset = state.presets.find((entry) => entry.id === presetId);
      if (!preset) throw new Error("Preset not found");
      const now = new Date();
      const enabled = preset.items.filter((item) => item.enabled && !item.elevated);
      const report: ApplyReport = {
        id: id(),
        presetId,
        status: "success",
        startedAt: new Date(now.getTime() - 1200).toISOString(),
        finishedAt: now.toISOString(),
        reusedWindows: Math.max(0, enabled.length - 1),
        launchedWindows: Math.min(1, enabled.length),
        closedWindows: 1,
        adjustedWindows: 0,
        issues: [],
        steps: ["preflight", "desktops", "reuse", "launch", "verify", "close", "finalize"].map((step) => ({
          id: step,
          label: step,
          state: "completed",
        })),
      };
      state.lastApplyReport = report;
      writeMockState(state);
      return report;
    });
  },

  cancelApply(): Promise<void> {
    return invokeOrMock("cancel_apply", undefined, () => undefined);
  },

  async getSettings(): Promise<AppSettings> {
    return invokeMapped<BackendSettings, AppSettings>("get_settings", undefined, settingsFromBackend, () => readMockState().settings);
  },

  async saveSettings(settings: AppSettings): Promise<AppSettings> {
    const saved = await invokeMapped<BackendSettings, AppSettings>("save_settings", { settings: settingsToBackend(settings, cachedPresets) }, settingsFromBackend, () => {
      const state = readMockState();
      state.settings = structuredClone(settings);
      writeMockState(state);
      return settings;
    });
    return saved;
  },

  checkForUpdates(): Promise<UpdateInfo> {
    return invokeMapped<BackendUpdateInfo, UpdateInfo>("check_for_updates", undefined, (backend) => ({
      available: backend.available,
      currentVersion: backend.currentVersion,
      latestVersion: backend.latestVersion,
      releaseUrl: backend.downloadUrl,
      checkedAt: backend.checkedAt,
      prerelease: true,
      error: backend.error,
    }), async () => {
      await wait(320);
      return {
        available: true,
        currentVersion: "0.1.0-alpha.1",
        latestVersion: "0.1.0-alpha.2",
        releaseUrl: "https://github.com/Tracht-Digital-Solutions/WorkSets/releases",
        checkedAt: isoNow(),
        prerelease: true,
      };
    });
  },

  getDiagnostics(): Promise<Diagnostics> {
    return invokeMapped<BackendDiagnostics, Diagnostics>("get_diagnostics", undefined, (backend) => ({
      appVersion: backend.version,
      generatedAt: isoNow(),
      storagePath: backend.dataDirectory,
      brokerProtocol: `v${backend.brokerProtocolVersion} · ${backend.compatibility.virtualDesktops.provider} ${backend.compatibility.virtualDesktops.providerVersion}`,
      sessionMutationsEnabled: !backend.compatibility.virtualDesktops.mutationsDisabled,
      checks: [
        {
          id: "storage",
          label: "Preset storage",
          status: "ok",
          detail: `${backend.presetCount} presets available.`,
          detailKey: "available",
          detailValues: { count: backend.presetCount },
        },
        {
          id: "broker",
          label: "Desktop broker",
          status: backend.compatibility.virtualDesktops.provider === "unavailable" ? "error" : backend.compatibility.virtualDesktops.experimental ? "warning" : "ok",
          detail: backend.compatibility.virtualDesktops.lastError ?? backend.compatibility.virtualDesktops.providerVersion,
        },
        {
          id: "privacy",
          label: "Telemetry privacy",
          status: !backend.privacy.telemetryTransportEnabled && !backend.privacy.telemetryContainsNamesPathsUrlsOrTitles ? "ok" : "error",
          detail: "No sensitive fields or transport enabled.",
          detailKey: "safe",
        },
      ] as DiagnosticCheck[],
    }), async () => {
      await wait(220);
      const state = readMockState();
      return {
        appVersion: "0.1.0-alpha.1",
        generatedAt: isoNow(),
        storagePath: "%LOCALAPPDATA%\\Tracht Digital Solutions\\WorkSets",
        brokerProtocol: "v1 · adapter 26200 (experimental)",
        sessionMutationsEnabled: state.compatibility.desktopCapability !== "unavailable",
        checks: [
          { id: "storage", label: "Preset storage", status: "ok", detail: "Atomic storage is available.", detailKey: "atomic" },
          {
            id: "broker",
            label: "Desktop broker",
            status: "warning",
            detail: "Build 26200 uses the experimental adapter.",
            detailKey: "experimental",
            detailValues: { build: 26200 },
          },
          { id: "monitors", label: "Monitor topology", status: "ok", detail: "2 monitors detected.", detailKey: "detected", detailValues: { count: 2 } },
          { id: "hotkeys", label: "Global shortcuts", status: "ok", detail: "No conflicts detected.", detailKey: "clear" },
        ] as DiagnosticCheck[],
      };
    });
  },

  exportPreset(presetId: string): Promise<string> {
    return invokeOrMock("export_preset", { presetId }, () => {
      const preset = readMockState().presets.find((entry) => entry.id === presetId);
      if (!preset) throw new Error("Preset not found");
      return JSON.stringify(presetToBackend(preset), null, 2);
    });
  },

  async importPreset(json: string, commit = false): Promise<ImportResult> {
    if (isTauriRuntime()) {
      try {
        const backend = await invoke<BackendImportResult>("import_preset", { json, commit });
        const preset = presetFromBackend(backend.preset);
        if (backend.committed) {
          cachedPresets = [preset, ...cachedPresets.filter((entry) => entry.id !== preset.id)];
        }
        return {
          valid: true,
          preset,
          warnings: backend.warnings,
          errors: [],
          committed: backend.committed,
        };
      } catch (reason) {
        return { valid: false, warnings: [], errors: [errorMessage(reason)], committed: false };
      }
    }
    return Promise.resolve((() => {
      try {
        const parsed = JSON.parse(json) as Partial<BackendPresetV1>;
        const errors: string[] = [];
        if (parsed.schemaVersion !== 1) errors.push("Unsupported schemaVersion.");
        if (typeof parsed.name !== "string" || !parsed.name.trim()) errors.push("name is required.");
        if (!Array.isArray(parsed.desktops) || parsed.desktops.length === 0) errors.push("At least one desktop is required.");
        if (!Array.isArray(parsed.items)) errors.push("items must be an array.");
        if (errors.length > 0) return { valid: false, warnings: [], errors, committed: false };
        const backendPreset: BackendPresetV1 = {
          ...(parsed as BackendPresetV1),
          id: commit ? id() : (parsed.id ?? id()),
          name: (parsed.name ?? "").trim(),
          updatedAt: isoNow(),
        };
        const preset = presetFromBackend(backendPreset);
        if (commit) {
          const state = readMockState();
          state.presets.unshift(preset);
          writeMockState(state);
        }
        return { valid: true, preset, warnings: [], errors: [], committed: commit };
      } catch {
        return { valid: false, warnings: [], errors: ["Invalid JSON."], committed: false };
      }
    })());
  },

  getPresetSchema(): Promise<Record<string, unknown>> {
    return invokeOrMock("get_preset_schema", undefined, () => ({
      $schema: "https://json-schema.org/draft/2020-12/schema",
      title: "PresetV1",
      type: "object",
      required: ["schemaVersion", "id", "name", "createdAt", "updatedAt", "desktopPolicy", "desktops", "monitors", "items"],
    }));
  },
};

export async function createNewPresetDraft(name = ""): Promise<PresetV1> {
  if (!isTauriRuntime()) return createBlankPreset(name);
  const captured = await worksetsApi.captureWorkspace();
  return {
    ...captured.preset,
    name,
    description: "",
    items: [],
    hotkey: undefined,
  };
}

export function resetBrowserMock(overrides?: Partial<MockState>) {
  if (typeof localStorage === "undefined") return;
  const initial = initialMockState();
  writeMockState({ ...initial, ...overrides });
}
