using System.Text.RegularExpressions;

namespace WorkSets.Core;

public sealed class PresetValidationException(IReadOnlyList<string> errors)
    : Exception(string.Join(Environment.NewLine, errors))
{
    public IReadOnlyList<string> Errors { get; } = errors;
}

public static partial class PresetValidation
{
    private const int MaxIconBytes = 128 * 1024;

    public static IReadOnlyList<string> Validate(PresetV1 preset)
    {
        var errors = new List<string>();
        if (preset.SchemaVersion != ProductInfo.PresetSchemaVersion)
            errors.Add($"Unsupported schemaVersion {preset.SchemaVersion}.");
        if (string.IsNullOrWhiteSpace(preset.Name) || preset.Name.Trim().Length > 120)
            errors.Add("name must contain between 1 and 120 characters.");
        if (preset.Description?.Length > 1_000)
            errors.Add("description must not exceed 1000 characters.");
        if (preset.Desktops.Count is < 1 or > 32)
            errors.Add("desktops must contain between 1 and 32 entries.");

        var desktopIds = preset.Desktops.Select(desktop => desktop.Id).ToHashSet();
        if (desktopIds.Count != preset.Desktops.Count)
            errors.Add("desktop IDs must be unique.");
        if (!preset.Desktops.Select(desktop => desktop.Index).Order().SequenceEqual(Enumerable.Range(0, preset.Desktops.Count)))
            errors.Add("desktop indices must be consecutive and zero-based.");

        var itemIds = new HashSet<Guid>();
        foreach (var item in preset.Items)
        {
            if (!itemIds.Add(item.Id)) errors.Add($"duplicate item ID {item.Id}.");
            if (!desktopIds.Contains(item.DesktopId)) errors.Add($"item {item.Id} references an unknown desktop.");
            if (string.IsNullOrWhiteSpace(item.DisplayName) || item.DisplayName.Length > 160)
                errors.Add($"item {item.Id} has an invalid displayName.");
            ValidateTarget(item, errors);
            ValidatePlacement(item, preset.Monitors, errors);
            if (!string.IsNullOrWhiteSpace(item.IconDataUrl)) ValidateIcon(item, errors);
        }

        ValidateHotkey(preset.Hotkey, "hotkey", errors);
        return errors;
    }

    public static void EnsureValid(PresetV1 preset)
    {
        var errors = Validate(preset);
        if (errors.Count > 0) throw new PresetValidationException(errors);
    }

    public static IReadOnlyList<string> ValidateSettings(AppSettings settings)
    {
        var errors = new List<string>();
        ValidateHotkey(settings.CaptureHotkey, "captureHotkey", errors);
        return errors;
    }

    private static void ValidateTarget(LaunchItem item, List<string> errors)
    {
        switch (item.LaunchTarget.Kind)
        {
            case LaunchTargetKind.Executable:
                if (string.IsNullOrWhiteSpace(item.LaunchTarget.Path) || !Path.IsPathFullyQualified(item.LaunchTarget.Path))
                    errors.Add($"item {item.Id} executable path must be absolute.");
                break;
            case LaunchTargetKind.PackagedApp:
                if (string.IsNullOrWhiteSpace(item.LaunchTarget.Aumid) || !AumidPattern().IsMatch(item.LaunchTarget.Aumid))
                    errors.Add($"item {item.Id} has an invalid AUMID.");
                break;
            case LaunchTargetKind.File:
                if (string.IsNullOrWhiteSpace(item.LaunchTarget.Path) || !Path.IsPathFullyQualified(item.LaunchTarget.Path))
                    errors.Add($"item {item.Id} file path must be absolute.");
                break;
            case LaunchTargetKind.Url:
                if (!Uri.TryCreate(item.LaunchTarget.Url, UriKind.Absolute, out var uri) || uri.Scheme is not ("http" or "https"))
                    errors.Add($"item {item.Id} URL must use HTTP or HTTPS.");
                break;
        }
        if (item.LaunchTarget.Arguments.Any(argument => argument.Contains('\0')))
            errors.Add($"item {item.Id} contains a null byte in its arguments.");
    }

    private static void ValidatePlacement(LaunchItem item, IReadOnlyList<MonitorRef> monitors, List<string> errors)
    {
        var placement = item.Placement;
        if (placement.Width < 1 || placement.Height < 1 || placement.Width > 100_000 || placement.Height > 100_000)
            errors.Add($"item {item.Id} has an invalid window size.");
        if (!IsFinite(placement.NormalizedX) || !IsFinite(placement.NormalizedY) ||
            !IsFinite(placement.NormalizedWidth) || !IsFinite(placement.NormalizedHeight) ||
            placement.NormalizedWidth <= 0 || placement.NormalizedHeight <= 0)
            errors.Add($"item {item.Id} has invalid normalized geometry.");
        if (monitors.Count > 0 && monitors.All(monitor => !string.Equals(monitor.Id, placement.MonitorId, StringComparison.OrdinalIgnoreCase)))
            errors.Add($"item {item.Id} references an unknown monitor.");
    }

    private static void ValidateIcon(LaunchItem item, List<string> errors)
    {
        const string prefix = "data:image/png;base64,";
        if (!item.IconDataUrl!.StartsWith(prefix, StringComparison.Ordinal))
        {
            errors.Add($"item {item.Id} icon must be a PNG data URL.");
            return;
        }
        try
        {
            if (Convert.FromBase64String(item.IconDataUrl[prefix.Length..]).Length > MaxIconBytes)
                errors.Add($"item {item.Id} icon exceeds {MaxIconBytes} bytes.");
        }
        catch (FormatException)
        {
            errors.Add($"item {item.Id} icon is not valid base64.");
        }
    }

    private static void ValidateHotkey(string? value, string field, List<string> errors)
    {
        if (string.IsNullOrWhiteSpace(value)) return;
        var parts = value.Split('+', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (parts.Length < 2 || parts.Distinct(StringComparer.OrdinalIgnoreCase).Count() != parts.Length)
            errors.Add($"{field} must contain modifiers and one unique key.");
    }

    private static bool IsFinite(double value) => !double.IsNaN(value) && !double.IsInfinity(value);

    [GeneratedRegex("^[A-Za-z0-9._-]+![A-Za-z0-9._-]+$")]
    private static partial Regex AumidPattern();
}
