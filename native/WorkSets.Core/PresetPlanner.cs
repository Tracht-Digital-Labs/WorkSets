namespace WorkSets.Core;

public static class PresetPlanner
{
    public static ApplyPlan Build(
        PresetV1 preset,
        WorkspaceSnapshot snapshot,
        IReadOnlySet<nint>? managedWindowHandles = null)
    {
        PresetValidation.EnsureValid(preset);
        managedWindowHandles ??= new HashSet<nint>();
        var enabled = preset.Items.Where(item => item.Enabled).ToArray();
        var used = new HashSet<nint>();
        var reused = 0;
        var launched = 0;
        var adjusted = 0;
        var risks = new List<ApplyRisk>();

        foreach (var item in enabled)
        {
            var match = FindMatch(item, snapshot.Windows, used);
            if (item.Elevated || match?.Elevated == true || match is { Manageable: false })
            {
                risks.Add(new ApplyRisk(
                    $"elevated-{item.Id:N}", "elevatedWindow", "elevatedWindow",
                    item.DisplayName, "Windows privilege isolation prevents WorkSets from managing this window.",
                    false, [item.Id]));
                continue;
            }
            if (match is not null && item.Lifecycle != LifecyclePolicy.Restart)
            {
                used.Add(match.NativeHandle);
                reused++;
            }
            else launched++;

            if (snapshot.Monitors.All(monitor => !string.Equals(monitor.Id, item.Placement.MonitorId, StringComparison.OrdinalIgnoreCase)))
            {
                adjusted++;
                risks.Add(new ApplyRisk(
                    $"monitor-{item.Id:N}", "missingMonitor", "missingMonitor",
                    item.DisplayName, "The saved monitor is unavailable; the window will be kept visible on the best replacement.",
                    false, [item.Id]));
            }
            if (!TargetLooksAvailable(item.LaunchTarget))
            {
                risks.Add(new ApplyRisk(
                    $"target-{item.Id:N}", "missingTarget", "missingTarget",
                    item.DisplayName, "The launch target is currently unavailable.",
                    true, [item.Id]));
            }
        }

        var desktopChanges = preset.Desktops.Count - snapshot.DesktopCount;
        if (desktopChanges != 0 && !snapshot.DesktopMutationAvailable)
        {
            risks.Add(new ApplyRisk(
                "desktop-unavailable", "desktopMutationUnavailable", "notice",
                "Virtual desktop control unavailable", "The requested desktop count cannot be created or removed on this Windows build.",
                true, []));
        }
        string? confirmationReason = null;
        if (desktopChanges != 0 && snapshot.DesktopMutationExperimental)
        {
            confirmationReason = "experimentalDesktopProvider";
            risks.Add(new ApplyRisk(
                "desktop-experimental", "experimentalDesktopProvider", "desktopExperimental",
                "Experimental desktop control", "This Windows build uses a safely isolated experimental desktop adapter for this session.",
                true, []));
        }

        var matchedHandles = used.ToHashSet();
        var foreignWindows = snapshot.Windows.Where(window =>
            window.DesktopIndex is int index && index >= preset.Desktops.Count &&
            !managedWindowHandles.Contains(window.NativeHandle) && !matchedHandles.Contains(window.NativeHandle)).ToArray();
        if (preset.DesktopPolicy == DesktopPolicy.ExactProtected && desktopChanges < 0 && foreignWindows.Length > 0)
        {
            confirmationReason = "foreignWindows";
            risks.Add(new ApplyRisk(
                "foreign-windows", "foreignWindowsOnRemovedDesktops", "foreignWindows",
                "Protect unmanaged windows", "Unmanaged windows must be moved before their desktop can be removed.",
                true, [], foreignWindows.Length));
        }

        var obsoleteManaged = managedWindowHandles.Count(handle => !matchedHandles.Contains(handle));
        return new ApplyPlan
        {
            PresetId = preset.Id,
            ConfirmationReason = confirmationReason,
            CanApply = risks.All(risk => risk.Code is "foreignWindowsOnRemovedDesktops" or "experimentalDesktopProvider" || !risk.Blocking),
            DesktopChanges = Math.Abs(desktopChanges),
            ReusedWindows = reused,
            LaunchedWindows = launched,
            ClosedWindows = obsoleteManaged,
            AdjustedWindows = adjusted,
            Risks = risks,
        };
    }

    public static CapturedWindow? FindMatch(LaunchItem item, IReadOnlyList<CapturedWindow> windows, IReadOnlySet<nint> used)
    {
        var candidates = windows.Where(window => SelectorMatches(item, window)).OrderBy(window => window.NativeHandle).ToArray();
        var index = item.Selector.InstanceIndex;
        return index >= 0 && index < candidates.Length && !used.Contains(candidates[index].NativeHandle)
            ? candidates[index]
            : null;
    }

    public static bool SelectorMatches(LaunchItem item, CapturedWindow window)
    {
        var selector = item.Selector;
        if (!string.IsNullOrWhiteSpace(selector.ExecutablePath) &&
            !PathEquals(selector.ExecutablePath, window.ExecutablePath)) return false;
        if (!string.IsNullOrWhiteSpace(selector.Aumid) &&
            !string.Equals(selector.Aumid, window.Aumid, StringComparison.OrdinalIgnoreCase)) return false;
        if (!string.IsNullOrWhiteSpace(selector.WindowClass) &&
            !string.Equals(selector.WindowClass, window.WindowClass, StringComparison.Ordinal)) return false;
        if (!string.IsNullOrWhiteSpace(selector.TitleContains) &&
            !window.Title.Contains(selector.TitleContains, StringComparison.CurrentCultureIgnoreCase)) return false;
        return !string.IsNullOrWhiteSpace(selector.ExecutablePath) || !string.IsNullOrWhiteSpace(selector.Aumid) ||
               !string.IsNullOrWhiteSpace(selector.WindowClass) || !string.IsNullOrWhiteSpace(selector.TitleContains);
    }

    public static (RectI32 Rectangle, bool Adjusted, MonitorRef Monitor) MapPlacement(
        WindowPlacementSpec placement,
        IReadOnlyList<MonitorRef> monitors)
    {
        if (monitors.Count == 0) throw new InvalidOperationException("No monitor is available.");
        var requested = monitors.FirstOrDefault(monitor => string.Equals(monitor.Id, placement.MonitorId, StringComparison.OrdinalIgnoreCase));
        var monitor = requested ?? monitors.FirstOrDefault(candidate => candidate.Primary) ?? monitors[0];
        var adjusted = requested is null;
        var area = monitor.WorkArea;
        var width = Math.Clamp((int)Math.Round(placement.NormalizedWidth * area.Width), 80, area.Width);
        var height = Math.Clamp((int)Math.Round(placement.NormalizedHeight * area.Height), 60, area.Height);
        var x = area.X + (int)Math.Round(placement.NormalizedX * area.Width);
        var y = area.Y + (int)Math.Round(placement.NormalizedY * area.Height);
        x = Math.Clamp(x, area.X, area.Right - width);
        y = Math.Clamp(y, area.Y, area.Bottom - height);
        return (new RectI32(x, y, width, height), adjusted, monitor);
    }

    private static bool TargetLooksAvailable(LaunchTarget target) => target.Kind switch
    {
        LaunchTargetKind.Executable or LaunchTargetKind.File => !string.IsNullOrWhiteSpace(target.Path) && File.Exists(target.Path),
        LaunchTargetKind.PackagedApp => !string.IsNullOrWhiteSpace(target.Aumid),
        LaunchTargetKind.Url => Uri.TryCreate(target.Url, UriKind.Absolute, out var uri) && uri.Scheme is "http" or "https",
        _ => false,
    };

    private static bool PathEquals(string expected, string? actual)
    {
        if (string.IsNullOrWhiteSpace(actual)) return false;
        try { return string.Equals(Path.GetFullPath(expected), Path.GetFullPath(actual), StringComparison.OrdinalIgnoreCase); }
        catch (Exception) when (expected.Length < 32_768 && actual.Length < 32_768) { return false; }
    }
}
