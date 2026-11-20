using System.Text;
using System.Text.Json;

namespace WorkSets.Core;

public sealed class PresetStore
{
    private const int MaxImportBytes = 2 * 1024 * 1024;
    private readonly SemaphoreSlim gate = new(1, 1);

    public PresetStore(string? root = null)
    {
        Root = root ?? Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            ProductInfo.Publisher,
            ProductInfo.Name);
        PresetsDirectory = Path.Combine(Root, "presets");
        ReportsDirectory = Path.Combine(Root, "reports");
        BackupsDirectory = Path.Combine(Root, "backups");
        Directory.CreateDirectory(PresetsDirectory);
        Directory.CreateDirectory(ReportsDirectory);
        Directory.CreateDirectory(BackupsDirectory);
    }

    public string Root { get; }
    public string PresetsDirectory { get; }
    public string ReportsDirectory { get; }
    public string BackupsDirectory { get; }

    public async Task<IReadOnlyList<PresetV1>> ListPresetsAsync(CancellationToken cancellationToken = default)
    {
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var presets = new List<PresetV1>();
            foreach (var file in Directory.EnumerateFiles(PresetsDirectory, "*.json", SearchOption.TopDirectoryOnly))
            {
                var preset = await ReadJsonAsync<PresetV1>(file, cancellationToken).ConfigureAwait(false);
                PresetValidation.EnsureValid(preset);
                presets.Add(preset);
            }
            return presets.OrderByDescending(preset => preset.UpdatedAt).ToArray();
        }
        finally { gate.Release(); }
    }

    public async Task<PresetV1?> LoadPresetAsync(Guid id, CancellationToken cancellationToken = default)
    {
        var path = PresetPath(id);
        if (!File.Exists(path)) return null;
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var preset = await ReadJsonAsync<PresetV1>(path, cancellationToken).ConfigureAwait(false);
            PresetValidation.EnsureValid(preset);
            return preset;
        }
        finally { gate.Release(); }
    }

    public async Task<PresetV1> SavePresetAsync(PresetV1 preset, CancellationToken cancellationToken = default)
    {
        PresetValidation.EnsureValid(preset);
        var normalized = preset with { UpdatedAt = DateTimeOffset.UtcNow };
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await AtomicWriteJsonAsync(PresetPath(normalized.Id), normalized, cancellationToken).ConfigureAwait(false);
            return normalized;
        }
        finally { gate.Release(); }
    }

    public async Task DeletePresetAsync(Guid id, CancellationToken cancellationToken = default)
    {
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var source = PresetPath(id);
            if (!File.Exists(source)) return;
            var backup = Path.Combine(BackupsDirectory, $"deleted-{id:N}-{DateTimeOffset.UtcNow:yyyyMMddHHmmssfff}.json");
            File.Move(source, backup);
        }
        finally { gate.Release(); }
    }

    public async Task<AppSettings> LoadSettingsAsync(CancellationToken cancellationToken = default)
    {
        var path = Path.Combine(Root, "settings.json");
        if (!File.Exists(path)) return new AppSettings();
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var settings = await ReadJsonAsync<AppSettings>(path, cancellationToken).ConfigureAwait(false);
            var errors = PresetValidation.ValidateSettings(settings);
            if (errors.Count > 0) throw new PresetValidationException(errors);
            return settings;
        }
        finally { gate.Release(); }
    }

    public async Task SaveSettingsAsync(AppSettings settings, CancellationToken cancellationToken = default)
    {
        var errors = PresetValidation.ValidateSettings(settings);
        if (errors.Count > 0) throw new PresetValidationException(errors);
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await AtomicWriteJsonAsync(Path.Combine(Root, "settings.json"), settings, cancellationToken).ConfigureAwait(false);
            if (!settings.TelemetryOptIn)
            {
                TryDelete(Path.Combine(Root, "telemetry.json"));
                TryDelete(Path.Combine(BackupsDirectory, "telemetry.json.bak"));
            }
        }
        finally { gate.Release(); }
    }

    public async Task SaveApplyReportAsync(ApplyReport report, CancellationToken cancellationToken = default)
    {
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await AtomicWriteJsonAsync(Path.Combine(ReportsDirectory, $"{report.Id:N}.json"), report, cancellationToken).ConfigureAwait(false);
            await AtomicWriteJsonAsync(Path.Combine(Root, "last-apply-report.json"), report, cancellationToken).ConfigureAwait(false);
        }
        finally { gate.Release(); }
    }

    public async Task<ApplyReport?> LoadLastApplyReportAsync(CancellationToken cancellationToken = default)
    {
        var path = Path.Combine(Root, "last-apply-report.json");
        if (!File.Exists(path)) return null;
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try { return await ReadJsonAsync<ApplyReport>(path, cancellationToken).ConfigureAwait(false); }
        finally { gate.Release(); }
    }

    public async Task<(PresetV1 Preset, IReadOnlyList<Notice> Warnings)> PreviewImportAsync(
        string json,
        CancellationToken cancellationToken = default)
    {
        if (Encoding.UTF8.GetByteCount(json) > MaxImportBytes)
            throw new InvalidDataException($"Preset imports are limited to {MaxImportBytes} bytes.");
        var preset = JsonSerializer.Deserialize<PresetV1>(json, JsonDefaults.Options)
            ?? throw new InvalidDataException("The file does not contain a preset.");
        PresetValidation.EnsureValid(preset);
        var warnings = new List<Notice>();
        if (await LoadPresetAsync(preset.Id, cancellationToken).ConfigureAwait(false) is not null)
        {
            preset = preset with { Id = Guid.NewGuid(), CreatedAt = DateTimeOffset.UtcNow, UpdatedAt = DateTimeOffset.UtcNow };
            warnings.Add(new Notice("presetIdReassigned", "The imported preset received a new local ID.", NoticeSeverity.Warning));
        }
        return (preset, warnings);
    }

    public string Export(PresetV1 preset)
    {
        PresetValidation.EnsureValid(preset);
        return JsonSerializer.Serialize(preset, JsonDefaults.Options);
    }

    private string PresetPath(Guid id) => Path.Combine(PresetsDirectory, $"{id:N}.json");

    private static async Task<T> ReadJsonAsync<T>(string path, CancellationToken cancellationToken)
    {
        await using var stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.Read, 64 * 1024, FileOptions.Asynchronous | FileOptions.SequentialScan);
        return await JsonSerializer.DeserializeAsync<T>(stream, JsonDefaults.Options, cancellationToken).ConfigureAwait(false)
            ?? throw new InvalidDataException($"{Path.GetFileName(path)} is empty or invalid.");
    }

    private async Task AtomicWriteJsonAsync<T>(string destination, T value, CancellationToken cancellationToken)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(destination)!);
        var temp = Path.Combine(Path.GetDirectoryName(destination)!, $".{Path.GetFileName(destination)}.{Guid.NewGuid():N}.tmp");
        try
        {
            await using (var stream = new FileStream(temp, FileMode.CreateNew, FileAccess.Write, FileShare.None, 64 * 1024, FileOptions.Asynchronous | FileOptions.WriteThrough))
            {
                await JsonSerializer.SerializeAsync(stream, value, JsonDefaults.Options, cancellationToken).ConfigureAwait(false);
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
                stream.Flush(flushToDisk: true);
            }
            if (File.Exists(destination))
            {
                var backup = Path.Combine(BackupsDirectory, $"{Path.GetFileName(destination)}.bak");
                File.Replace(temp, destination, backup, ignoreMetadataErrors: true);
            }
            else File.Move(temp, destination);
        }
        finally { TryDelete(temp); }
    }

    private static void TryDelete(string path)
    {
        try { if (File.Exists(path)) File.Delete(path); }
        catch (IOException) { }
        catch (UnauthorizedAccessException) { }
    }
}
