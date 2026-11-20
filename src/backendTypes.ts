/** Canonical camelCase DTOs emitted and accepted by the Rust/Tauri boundary. */
export interface BackendRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BackendMonitorRef {
  id: string;
  deviceName: string;
  bounds: BackendRect;
  workArea: BackendRect;
  dpiX: number;
  dpiY: number;
  primary: boolean;
}

export interface BackendNormalizedRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface BackendWindowPlacement {
  monitor: BackendMonitorRef;
  physical: BackendRect;
  normalized: BackendNormalizedRect;
  state: "normal" | "maximized" | "minimized";
}

export type BackendLaunchTarget =
  | { kind: "executable"; path: string; args: string[]; workingDirectory?: string }
  | { kind: "packagedApp"; aumid: string; args: string[] }
  | { kind: "file"; path: string }
  | { kind: "url"; url: string };

export interface BackendWindowSelector {
  executablePath?: string;
  aumid?: string;
  className?: string;
  titlePattern?: string;
  instanceIndex: number;
}

export interface BackendLaunchItem {
  id: string;
  displayName: string;
  desktopId: string;
  launchTarget: BackendLaunchTarget;
  selector: BackendWindowSelector;
  placement: BackendWindowPlacement;
  lifecycle: "reuse" | "restart" | "leaveOpen";
  enabled: boolean;
}

export interface BackendPresetV1 {
  schemaVersion: number;
  id: string;
  name: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  desktopPolicy: "exactProtected" | "managedOnly";
  desktops: Array<{ id: string; name: string; index: number }>;
  monitors: BackendMonitorRef[];
  items: BackendLaunchItem[];
  hotkey?: string;
}

export interface BackendNotice {
  code: string;
  message: string;
  severity: "info" | "warning" | "error";
}

export interface BackendSettings {
  language: string;
  theme: "system" | "light" | "dark";
  runAtStartup: boolean;
  minimizeToTray: boolean;
  telemetryOptIn: boolean;
  checkForUpdates: boolean;
  onboardingComplete: boolean;
  defaultPolicy: "exactProtected" | "managedOnly";
  captureHotkey: string;
  closeBehavior: "tray" | "quit";
  hotkeys: Record<string, string>;
}

export interface BackendCompatibility {
  os: { major: number; minor: number; build: number; revision: number; architecture: string };
  supported: boolean;
  virtualDesktops: {
    provider: "windows10Legacy" | "windows11_22h2" | "windows11_24h2" | "windows11_25h2" | "experimentalLatest" | "unavailable";
    providerVersion: string;
    experimental: boolean;
    canRead: boolean;
    canCreate: boolean;
    canRemove: boolean;
    canMoveWindows: boolean;
    mutationsDisabled: boolean;
    lastError?: string;
    isolatedProcess: boolean;
  };
  windows: { canCapture: boolean; canLaunch: boolean; canPlace: boolean; canCloseManaged: boolean };
  warnings: BackendNotice[];
}

export interface BackendApplyStepResult {
  step: "preflight" | "desktops" | "reuse" | "launch" | "placement" | "verify" | "closeObsolete" | "activate";
  status: "success" | "skipped" | "warning" | "failed";
  durationMs: number;
  message?: string;
}

export interface BackendApplyReport {
  id: string;
  presetId: string;
  startedAt: string;
  finishedAt: string;
  status: "success" | "partial" | "needsAttention" | "cancelled" | "failed";
  steps: BackendApplyStepResult[];
  items: Array<{
    itemId: string;
    displayName: string;
    status: "launched" | "reused" | "adjusted" | "skipped" | "failed";
    nativeWindowId?: string;
    message?: string;
  }>;
  notices: BackendNotice[];
  launchedCount: number;
  reusedCount: number;
  closedCount: number;
  adjustedCount: number;
}

export interface BackendAppState {
  version: string;
  settings: BackendSettings;
  presets: BackendPresetV1[];
  compatibility: BackendCompatibility;
  busy: boolean;
  dataDirectory: string;
  lastApplyReport?: BackendApplyReport;
}

export interface BackendApplyProgress {
  presetId: string;
  step:
    | "preflight"
    | "desktops"
    | "reuse"
    | "launch"
    | "placement"
    | "verify"
    | "closeObsolete"
    | "activate";
  status: "pending" | "running" | "success" | "skipped" | "warning" | "failed";
  message?: string;
}

export interface BackendCapturedWindow {
  nativeId: string;
  processId: number;
  title: string;
  application: string;
  className: string;
  executablePath?: string;
  aumid?: string;
  placement: BackendWindowPlacement;
  desktopIndex?: number;
  elevated: boolean;
  manageable: boolean;
}

export interface BackendCaptureResult {
  preset: BackendPresetV1;
  capturedWindows: BackendCapturedWindow[];
  warnings: BackendNotice[];
}

export type BackendPlannedAction =
  | { kind: "createDesktops"; count: number }
  | { kind: "removeDesktops"; count: number }
  | { kind: "reuseWindow"; itemId: string }
  | { kind: "launchItem"; itemId: string }
  | { kind: "placeWindow"; itemId: string };

export interface BackendApplyPlan {
  presetId: string;
  targetDesktopCount: number;
  currentDesktopCount: number;
  requiresConfirmation: boolean;
  confirmationReason?: "foreignWindows" | "experimentalDesktopProvider";
  actions: BackendPlannedAction[];
  foreignWindows: Array<{ nativeId: string; title: string; application: string; desktopIndex?: number }>;
  warnings: BackendNotice[];
}

export interface BackendApplyOptions {
  confirmDesktopChanges: boolean;
  forceCloseManaged: boolean;
  startupTimeoutMs: number;
}

export interface BackendUpdateInfo {
  checkedAt: string;
  currentVersion: string;
  latestVersion?: string;
  available: boolean;
  downloadUrl?: string;
  error?: string;
}

export interface BackendDiagnostics {
  version: string;
  compatibility: BackendCompatibility;
  presetCount: number;
  busy: boolean;
  dataDirectory: string;
  lastApplyStatus?: BackendApplyReport["status"];
  brokerProtocolVersion: number;
  privacy: {
    telemetryTransportEnabled: boolean;
    telemetryContainsNamesPathsUrlsOrTitles: boolean;
  };
}

export interface BackendImportResult {
  preset: BackendPresetV1;
  committed: boolean;
  warnings: BackendNotice[];
}
