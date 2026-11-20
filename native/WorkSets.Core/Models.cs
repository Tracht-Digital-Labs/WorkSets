using System.Text.Json.Serialization;

namespace WorkSets.Core;

public static class ProductInfo
{
    public const string Name = "WorkSets";
    public const string Publisher = "Tracht Digital Solutions";
    public const string Version = "0.1.0-alpha.1";
    public const int PresetSchemaVersion = 1;
}

[JsonConverter(typeof(JsonStringEnumConverter<DesktopPolicy>))]
public enum DesktopPolicy { ExactProtected, ManagedOnly }

[JsonConverter(typeof(JsonStringEnumConverter<LifecyclePolicy>))]
public enum LifecyclePolicy { Reuse, Restart, LeaveOpen }

[JsonConverter(typeof(JsonStringEnumConverter<WindowState>))]
public enum WindowState { Normal, Maximized, Minimized }

[JsonConverter(typeof(JsonStringEnumConverter<LaunchTargetKind>))]
public enum LaunchTargetKind { Executable, PackagedApp, File, Url }

[JsonConverter(typeof(JsonStringEnumConverter<ThemePreference>))]
public enum ThemePreference { System, Light, Dark }

[JsonConverter(typeof(JsonStringEnumConverter<Language>))]
public enum Language { De, En }

[JsonConverter(typeof(JsonStringEnumConverter<NoticeSeverity>))]
public enum NoticeSeverity { Info, Warning, Error }

[JsonConverter(typeof(JsonStringEnumConverter<ApplyStatus>))]
public enum ApplyStatus { Success, Partial, Failed, Cancelled }

[JsonConverter(typeof(JsonStringEnumConverter<StepStatus>))]
public enum StepStatus { Pending, Running, Completed, Warning, Failed, Skipped }

[JsonConverter(typeof(JsonStringEnumConverter<ItemApplyStatus>))]
public enum ItemApplyStatus { Reused, Launched, Adjusted, Skipped, Failed }

public sealed record RectI32(int X, int Y, int Width, int Height)
{
    public int Right => X + Width;
    public int Bottom => Y + Height;
}

public sealed record MonitorRef(
    string Id,
    string DeviceName,
    string FriendlyName,
    RectI32 Bounds,
    RectI32 WorkArea,
    uint DpiX,
    uint DpiY,
    bool Primary);

public sealed record DesktopSpec(Guid Id, string Name, int Index);

public sealed record LaunchTarget
{
    public required LaunchTargetKind Kind { get; init; }
    public string? Path { get; init; }
    public IReadOnlyList<string> Arguments { get; init; } = [];
    public string? WorkingDirectory { get; init; }
    public string? Aumid { get; init; }
    public string? Url { get; init; }

    public static LaunchTarget Executable(string path, IEnumerable<string>? arguments = null, string? workingDirectory = null) =>
        new() { Kind = LaunchTargetKind.Executable, Path = path, Arguments = arguments?.ToArray() ?? [], WorkingDirectory = workingDirectory };

    public static LaunchTarget PackagedApp(string aumid, IEnumerable<string>? arguments = null) =>
        new() { Kind = LaunchTargetKind.PackagedApp, Aumid = aumid, Arguments = arguments?.ToArray() ?? [] };

    public static LaunchTarget File(string path) => new() { Kind = LaunchTargetKind.File, Path = path };
    public static LaunchTarget WebUrl(string url) => new() { Kind = LaunchTargetKind.Url, Url = url };
}

public sealed record WindowSelector
{
    public string? ExecutablePath { get; init; }
    public string? Aumid { get; init; }
    public string? WindowClass { get; init; }
    public string? TitleContains { get; init; }
    public int InstanceIndex { get; init; }
}

public sealed record WindowPlacementSpec
{
    public required string MonitorId { get; init; }
    public int X { get; init; }
    public int Y { get; init; }
    public int Width { get; init; }
    public int Height { get; init; }
    public double NormalizedX { get; init; }
    public double NormalizedY { get; init; }
    public double NormalizedWidth { get; init; }
    public double NormalizedHeight { get; init; }
    public WindowState State { get; init; }
}

public sealed record LaunchItem
{
    public Guid Id { get; init; } = Guid.NewGuid();
    public required string DisplayName { get; init; }
    public required Guid DesktopId { get; init; }
    public required LaunchTarget LaunchTarget { get; init; }
    public required WindowSelector Selector { get; init; }
    public required WindowPlacementSpec Placement { get; init; }
    public LifecyclePolicy Lifecycle { get; init; } = LifecyclePolicy.Reuse;
    public bool Enabled { get; init; } = true;
    public bool Elevated { get; init; }
    public string? IconDataUrl { get; init; }
}

public sealed record PresetV1
{
    public int SchemaVersion { get; init; } = ProductInfo.PresetSchemaVersion;
    public Guid Id { get; init; } = Guid.NewGuid();
    public required string Name { get; init; }
    public string? Description { get; init; }
    public DateTimeOffset CreatedAt { get; init; } = DateTimeOffset.UtcNow;
    public DateTimeOffset UpdatedAt { get; init; } = DateTimeOffset.UtcNow;
    public DesktopPolicy DesktopPolicy { get; init; } = DesktopPolicy.ExactProtected;
    public IReadOnlyList<DesktopSpec> Desktops { get; init; } = [];
    public IReadOnlyList<MonitorRef> Monitors { get; init; } = [];
    public IReadOnlyList<LaunchItem> Items { get; init; } = [];
    public string? Hotkey { get; init; }
}

public sealed record AppSettings
{
    public Language Language { get; init; } = Language.De;
    public ThemePreference Theme { get; init; } = ThemePreference.System;
    public bool RunAtStartup { get; init; }
    public bool MinimizeToTray { get; init; } = true;
    public bool TelemetryOptIn { get; init; }
    public bool CheckForUpdates { get; init; } = true;
    public bool OnboardingComplete { get; init; }
    public DesktopPolicy DefaultPolicy { get; init; } = DesktopPolicy.ExactProtected;
    public string CaptureHotkey { get; init; } = "Ctrl+Shift+W";
}

public sealed record Notice(string Code, string Message, NoticeSeverity Severity);

public sealed record CapturedWindow
{
    public required nint NativeHandle { get; init; }
    public required uint ProcessId { get; init; }
    public required string DisplayName { get; init; }
    public string? ExecutablePath { get; init; }
    public string? Aumid { get; init; }
    public required string WindowClass { get; init; }
    public required string Title { get; init; }
    public required WindowPlacementSpec Placement { get; init; }
    public int? DesktopIndex { get; init; }
    public bool Manageable { get; init; } = true;
    public bool Elevated { get; init; }
    public string? IconDataUrl { get; init; }
}

public sealed record WorkspaceSnapshot(
    IReadOnlyList<MonitorRef> Monitors,
    IReadOnlyList<CapturedWindow> Windows,
    int DesktopCount,
    int CurrentDesktopIndex,
    bool DesktopMutationAvailable,
    bool DesktopMutationExperimental);

public sealed record ApplyRisk(
    string Id,
    string Code,
    string Kind,
    string Title,
    string Detail,
    bool Blocking,
    IReadOnlyList<Guid> AffectedItemIds,
    int Count = 0);

public sealed record ApplyPlan
{
    public required Guid PresetId { get; init; }
    public string? ConfirmationReason { get; init; }
    public bool CanApply { get; init; }
    public int DesktopChanges { get; init; }
    public int ReusedWindows { get; init; }
    public int LaunchedWindows { get; init; }
    public int ClosedWindows { get; init; }
    public int AdjustedWindows { get; init; }
    public IReadOnlyList<ApplyRisk> Risks { get; init; } = [];
}

public sealed record ApplyOptions
{
    public bool ConfirmForeignWindows { get; init; }
    public bool ConfirmDesktopMutation { get; init; }
    public bool ForceOwnedProcesses { get; init; }
    public int StartupTimeoutMs { get; init; } = 15_000;
}

public sealed record ApplyStepResult(string Id, StepStatus Status, long DurationMs, string? Message = null);
public sealed record ApplyIssue(string Id, NoticeSeverity Severity, string Title, string Detail, Guid? ItemId = null, Guid? OccurrenceId = null);
public sealed record ItemApplyResult(Guid ItemId, string DisplayName, ItemApplyStatus Status, string? NativeWindowId = null, string? Message = null);

public sealed record ApplyReport
{
    public Guid Id { get; init; } = Guid.NewGuid();
    public required Guid PresetId { get; init; }
    public ApplyStatus Status { get; init; }
    public DateTimeOffset StartedAt { get; init; }
    public DateTimeOffset FinishedAt { get; init; }
    public IReadOnlyList<ApplyStepResult> Steps { get; init; } = [];
    public IReadOnlyList<ApplyIssue> Issues { get; init; } = [];
    public IReadOnlyList<ItemApplyResult> Items { get; init; } = [];
    public int ReusedWindows { get; init; }
    public int LaunchedWindows { get; init; }
    public int ClosedWindows { get; init; }
    public int AdjustedWindows { get; init; }
}

public sealed record CompatibilityInfo(
    bool Supported,
    string Platform,
    string Architecture,
    int? WindowsBuild,
    string DesktopCapability,
    string? Message = null);

public sealed record DiagnosticCheck(string Id, string Label, string Status, string Detail);
public sealed record Diagnostics(
    string AppVersion,
    DateTimeOffset GeneratedAt,
    string StoragePath,
    string BrokerProtocol,
    bool SessionMutationsEnabled,
    IReadOnlyList<DiagnosticCheck> Checks);
