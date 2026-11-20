export type ThemePreference = "system" | "light" | "dark";
export type Language = "de" | "en";
export type DesktopPolicy = "exactProtected" | "managedOnly";
export type LifecyclePolicy = "reuse" | "restart" | "leaveOpen";
export type WindowState = "normal" | "maximized" | "minimized";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface MonitorRef {
  id: string;
  deviceName: string;
  friendlyName: string;
  bounds: Rect;
  workArea: Rect;
  dpiX: number;
  dpiY: number;
  primary: boolean;
}

export interface DesktopSpec {
  id: string;
  name: string;
  index: number;
}

export type LaunchTarget =
  | { kind: "executable"; path: string; arguments: string[]; workingDirectory?: string }
  | { kind: "packagedApp"; aumid: string; arguments: string[] }
  | { kind: "file"; path: string }
  | { kind: "url"; url: string };

export interface WindowSelector {
  executablePath?: string;
  aumid?: string;
  windowClass?: string;
  titleContains?: string;
  instanceIndex: number;
}

export interface WindowPlacementSpec {
  monitorId: string;
  x: number;
  y: number;
  width: number;
  height: number;
  normalizedX: number;
  normalizedY: number;
  normalizedWidth: number;
  normalizedHeight: number;
  state: WindowState;
}

export interface LaunchItem {
  id: string;
  displayName: string;
  desktopId: string;
  launchTarget: LaunchTarget;
  selector: WindowSelector;
  placement: WindowPlacementSpec;
  lifecycle: LifecyclePolicy;
  enabled: boolean;
  elevated?: boolean;
  iconDataUrl?: string;
}

export interface PresetV1 {
  schemaVersion: 1;
  id: string;
  name: string;
  description?: string;
  createdAt: string;
  updatedAt: string;
  desktopPolicy: DesktopPolicy;
  desktops: DesktopSpec[];
  monitors: MonitorRef[];
  items: LaunchItem[];
  hotkey?: string;
}

export interface CompatibilityInfo {
  supported: boolean;
  platform: string;
  architecture: string;
  windowsBuild?: number;
  desktopCapability: "available" | "experimental" | "unavailable";
  message?: string;
}

export interface AppSettings {
  language: Language;
  theme: ThemePreference;
  runAtStartup: boolean;
  minimizeToTray: boolean;
  telemetryOptIn: boolean;
  checkForUpdates: boolean;
  onboardingComplete: boolean;
  defaultPolicy: DesktopPolicy;
  captureHotkey: string;
}

export interface AppState {
  version: string;
  presets: PresetV1[];
  settings: AppSettings;
  compatibility: CompatibilityInfo;
  busy: boolean;
  dataDirectory: string;
  lastApplyReport?: ApplyReport;
}

export interface CaptureResult {
  preset: PresetV1;
  warnings: Notice[];
}

export interface Notice {
  code: string;
  message: string;
  severity: "info" | "warning" | "error";
}

export type ApplyRiskKind =
  | "foreignWindows"
  | "missingMonitor"
  | "elevatedWindow"
  | "missingTarget"
  | "desktopExperimental"
  | "notice";

export interface ApplyRisk {
  id: string;
  code: string;
  kind: ApplyRiskKind;
  title: string;
  detail: string;
  blocking: boolean;
  affectedItemIds: string[];
  count?: number;
}

export interface ApplyPlan {
  presetId: string;
  confirmationReason?: "foreignWindows" | "experimentalDesktopProvider";
  canApply: boolean;
  desktopChanges: number;
  reusedWindows: number;
  launchedWindows: number;
  closedWindows: number;
  adjustedWindows: number;
  risks: ApplyRisk[];
}

export interface ApplyOptions {
  confirmForeignWindows?: boolean;
  confirmDesktopMutation?: boolean;
  forceOwnedProcesses?: boolean;
}

export type ApplyStepState = "pending" | "running" | "completed" | "warning" | "failed";

export interface ApplyStep {
  id: string;
  label: string;
  state: ApplyStepState;
  detail?: string;
}

export interface ApplyProgressEvent {
  presetId: string;
  step: ApplyStep["id"];
  state: ApplyStepState;
  detail?: string;
}

export interface ApplyIssue {
  id: string;
  occurrenceId?: string;
  severity: "info" | "warning" | "error";
  title: string;
  detail: string;
  itemId?: string;
}

export interface ApplyReport {
  id: string;
  presetId: string;
  status: "success" | "partial" | "failed" | "cancelled";
  startedAt: string;
  finishedAt: string;
  steps: ApplyStep[];
  issues: ApplyIssue[];
  reusedWindows: number;
  launchedWindows: number;
  closedWindows: number;
  adjustedWindows: number;
}

export interface UpdateInfo {
  available: boolean;
  currentVersion: string;
  latestVersion?: string;
  releaseUrl?: string;
  checkedAt: string;
  prerelease: boolean;
  error?: string;
}

export interface DiagnosticCheck {
  id: string;
  label: string;
  status: "ok" | "warning" | "error";
  detail: string;
  detailKey?: string;
  detailValues?: Record<string, string | number>;
}

export interface Diagnostics {
  appVersion: string;
  generatedAt: string;
  storagePath: string;
  brokerProtocol: string;
  sessionMutationsEnabled: boolean;
  checks: DiagnosticCheck[];
}

export interface ImportResult {
  valid: boolean;
  preset?: PresetV1;
  warnings: Notice[];
  errors: string[];
  committed: boolean;
}

export type ViewId = "presets" | "editor" | "settings" | "diagnostics";
