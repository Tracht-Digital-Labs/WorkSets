using System.Diagnostics;
using System.IO.Pipes;
using System.Text;
using System.Text.Json;
using Microsoft.Win32;
using WorkSets.Core;

namespace WorkSets.Windows;

public sealed record DesktopSnapshot(int Count, int CurrentIndex);
public sealed record DesktopMutationResult(int Before, int After, int Created, int Removed);
public sealed record DesktopServiceStatus(
    int WindowsBuild,
    string Adapter,
    bool Supported,
    bool Experimental,
    bool CanMutate,
    bool CircuitOpen,
    string? LastError);

public interface IDesktopService : IAsyncDisposable
{
    DesktopServiceStatus Status { get; }
    Task<DesktopSnapshot> SnapshotAsync(CancellationToken cancellationToken = default);
    Task<int?> WindowDesktopIndexAsync(nint nativeHandle, CancellationToken cancellationToken = default);
    Task<DesktopMutationResult> EnsureCountAsync(int target, DesktopPolicy policy, bool allowRemove, CancellationToken cancellationToken = default);
    Task MoveWindowAsync(nint nativeHandle, int desktopIndex, CancellationToken cancellationToken = default);
    Task SwitchToAsync(int desktopIndex, CancellationToken cancellationToken = default);
}

public sealed class DesktopBrokerClient : IDesktopService
{
    private const int ProtocolVersion = 1;
    private readonly SemaphoreSlim gate = new(1, 1);
    private NamedPipeServerStream? pipe;
    private StreamReader? reader;
    private StreamWriter? writer;
    private Process? child;
    private Guid nonce;
    private ulong nextRequestId = 1;
    private bool safeProbeComplete;
    private bool circuitOpen;
    private string? lastError;

    public DesktopBrokerClient(string? brokerPath = null)
    {
        BrokerPath = brokerPath ?? LocateBroker();
        var build = ReadWindowsBuild();
        var adapter = build switch
        {
            19045 => "windows10-19045",
            22621 or 22631 => "windows11-22621/22631",
            >= 26100 and <= 26199 => "windows11-26100",
            >= 26200 and <= 26299 => "windows11-26200",
            >= 22000 => "experimental-latest",
            _ => "unavailable",
        };
        var supported = build == 19045 || build >= 22000;
        var experimental = build >= 22000 && build is not (22621 or 22631) && build is not (>= 26100 and <= 26299);
        Status = new DesktopServiceStatus(build, adapter, supported, experimental, supported, false, null);
    }

    public string BrokerPath { get; }
    public DesktopServiceStatus Status { get; private set; }

    public async Task<DesktopSnapshot> SnapshotAsync(CancellationToken cancellationToken = default)
    {
        var payload = await RequestAsync(new { action = "snapshot" }, mutation: false, cancellationToken).ConfigureAwait(false);
        safeProbeComplete = true;
        return new DesktopSnapshot(payload.GetProperty("count").GetInt32(), payload.GetProperty("currentIndex").GetInt32());
    }

    public async Task<int?> WindowDesktopIndexAsync(nint nativeHandle, CancellationToken cancellationToken = default)
    {
        var payload = await RequestAsync(new { action = "windowDesktopIndex", nativeId = nativeHandle.ToInt64() }, mutation: false, cancellationToken).ConfigureAwait(false);
        return payload.ValueKind == JsonValueKind.Null ? null : payload.GetInt32();
    }

    public async Task<DesktopMutationResult> EnsureCountAsync(int target, DesktopPolicy policy, bool allowRemove, CancellationToken cancellationToken = default)
    {
        var payload = await RequestAsync(new
        {
            action = "ensureCount",
            target,
            policy = policy == DesktopPolicy.ExactProtected ? "exactProtected" : "managedOnly",
            allowRemove,
        }, mutation: true, cancellationToken).ConfigureAwait(false);
        return new DesktopMutationResult(
            payload.GetProperty("before").GetInt32(), payload.GetProperty("after").GetInt32(),
            payload.GetProperty("created").GetInt32(), payload.GetProperty("removed").GetInt32());
    }

    public async Task MoveWindowAsync(nint nativeHandle, int desktopIndex, CancellationToken cancellationToken = default) =>
        _ = await RequestAsync(new { action = "moveWindow", nativeId = nativeHandle.ToInt64(), desktopIndex }, mutation: true, cancellationToken).ConfigureAwait(false);

    public async Task SwitchToAsync(int desktopIndex, CancellationToken cancellationToken = default) =>
        _ = await RequestAsync(new { action = "switchTo", desktopIndex }, mutation: true, cancellationToken).ConfigureAwait(false);

    private async Task<JsonElement> RequestAsync(object action, bool mutation, CancellationToken cancellationToken)
    {
        if (mutation && circuitOpen) throw new InvalidOperationException("Virtual desktop mutations are disabled for this session after a broker error.");
        if (mutation && Status.Experimental && !safeProbeComplete) _ = await SnapshotAsync(cancellationToken).ConfigureAwait(false);
        await gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            await EnsureConnectedAsync(cancellationToken).ConfigureAwait(false);
            var requestId = nextRequestId++;
            var envelope = new { protocolVersion = ProtocolVersion, requestId, nonce, request = action };
            await writer!.WriteLineAsync(JsonSerializer.Serialize(envelope, JsonDefaults.Options).AsMemory(), cancellationToken).ConfigureAwait(false);
            await writer.FlushAsync(cancellationToken).ConfigureAwait(false);
            var responseLine = await reader!.ReadLineAsync(cancellationToken).AsTask().WaitAsync(TimeSpan.FromSeconds(8), cancellationToken).ConfigureAwait(false);
            if (responseLine is null) throw new IOException("The desktop broker closed its pipe.");
            using var response = JsonDocument.Parse(responseLine);
            var root = response.RootElement;
            if (root.GetProperty("protocolVersion").GetInt32() != ProtocolVersion ||
                root.GetProperty("requestId").GetUInt64() != requestId ||
                root.GetProperty("nonce").GetGuid() != nonce)
                throw new InvalidDataException("The desktop broker response did not match the authenticated request.");
            if (!root.GetProperty("ok").GetBoolean())
            {
                var error = root.TryGetProperty("error", out var value) ? value : default;
                var code = error.ValueKind == JsonValueKind.Object && error.TryGetProperty("code", out var codeValue) ? codeValue.GetString() : "desktopBrokerError";
                var message = error.ValueKind == JsonValueKind.Object && error.TryGetProperty("message", out var messageValue) ? messageValue.GetString() : "The desktop broker rejected the operation.";
                throw new DesktopBrokerException(code ?? "desktopBrokerError", message ?? "The desktop broker rejected the operation.");
            }
            return root.GetProperty("payload").Clone();
        }
        catch (Exception exception) when (exception is not OperationCanceledException)
        {
            Disconnect();
            circuitOpen = true;
            lastError = exception.Message;
            Status = Status with { CanMutate = false, CircuitOpen = true, LastError = lastError };
            throw;
        }
        finally { gate.Release(); }
    }

    private async Task EnsureConnectedAsync(CancellationToken cancellationToken)
    {
        if (pipe is { IsConnected: true } && child is { HasExited: false }) return;
        Disconnect();
        if (!File.Exists(BrokerPath)) throw new FileNotFoundException("The isolated desktop broker was not found.", BrokerPath);
        nonce = Guid.NewGuid();
        nextRequestId = 1;
        var pipeLeaf = $"TrachtDigitalSolutions.WorkSets.{Environment.ProcessId}.{Guid.NewGuid():N}";
        pipe = new NamedPipeServerStream(
            pipeLeaf, PipeDirection.InOut, 1, PipeTransmissionMode.Byte,
            PipeOptions.Asynchronous | PipeOptions.CurrentUserOnly, 16 * 1024, 16 * 1024);
        var start = new ProcessStartInfo(BrokerPath)
        {
            UseShellExecute = false,
            CreateNoWindow = true,
            WindowStyle = ProcessWindowStyle.Hidden,
        };
        start.ArgumentList.Add("--pipe-name");
        start.ArgumentList.Add($@"\\.\pipe\{pipeLeaf}");
        start.ArgumentList.Add("--parent-pid");
        start.ArgumentList.Add(Environment.ProcessId.ToString(System.Globalization.CultureInfo.InvariantCulture));
        child = Process.Start(start) ?? throw new InvalidOperationException("Windows did not start the desktop broker.");
        try
        {
            await pipe.WaitForConnectionAsync(cancellationToken).WaitAsync(TimeSpan.FromSeconds(8), cancellationToken).ConfigureAwait(false);
            if (!NativeInterop.GetNamedPipeClientProcessId(pipe.SafePipeHandle, out var clientProcessId) || clientProcessId != child.Id)
                throw new UnauthorizedAccessException("The desktop broker pipe was connected by an unexpected process.");
            reader = new StreamReader(pipe, new UTF8Encoding(false, true), detectEncodingFromByteOrderMarks: false, 16 * 1024, leaveOpen: true);
            writer = new StreamWriter(pipe, new UTF8Encoding(false), 16 * 1024, leaveOpen: true) { AutoFlush = true, NewLine = "\n" };
            var requestId = nextRequestId++;
            var handshake = new { protocolVersion = ProtocolVersion, requestId, nonce, request = new { action = "handshake" } };
            await writer.WriteLineAsync(JsonSerializer.Serialize(handshake, JsonDefaults.Options).AsMemory(), cancellationToken).ConfigureAwait(false);
            var line = await reader.ReadLineAsync(cancellationToken).AsTask().WaitAsync(TimeSpan.FromSeconds(8), cancellationToken).ConfigureAwait(false);
            if (line is null) throw new IOException("The desktop broker did not complete its handshake.");
            using var response = JsonDocument.Parse(line);
            var root = response.RootElement;
            if (!root.GetProperty("ok").GetBoolean() || root.GetProperty("requestId").GetUInt64() != requestId || root.GetProperty("nonce").GetGuid() != nonce)
                throw new UnauthorizedAccessException("The desktop broker handshake failed authentication.");
        }
        catch
        {
            Disconnect();
            throw;
        }
    }

    private void Disconnect()
    {
        try { writer?.Dispose(); } catch (IOException) { }
        try { reader?.Dispose(); } catch (IOException) { }
        try { pipe?.Dispose(); } catch (IOException) { }
        if (child is { HasExited: false })
        {
            try { child.Kill(entireProcessTree: false); child.WaitForExit(1_000); } catch (InvalidOperationException) { }
        }
        child?.Dispose();
        writer = null;
        reader = null;
        pipe = null;
        child = null;
    }

    public async ValueTask DisposeAsync()
    {
        await gate.WaitAsync().ConfigureAwait(false);
        try
        {
            if (pipe is { IsConnected: true } && writer is not null)
            {
                var requestId = nextRequestId++;
                var shutdown = new { protocolVersion = ProtocolVersion, requestId, nonce, request = new { action = "shutdown" } };
                try { await writer.WriteLineAsync(JsonSerializer.Serialize(shutdown, JsonDefaults.Options)).ConfigureAwait(false); }
                catch (IOException) { }
            }
            Disconnect();
        }
        finally { gate.Release(); gate.Dispose(); }
    }

    private static int ReadWindowsBuild()
    {
        var value = Registry.GetValue(@"HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuildNumber", null)?.ToString();
        return int.TryParse(value, out var build) ? build : Environment.OSVersion.Version.Build;
    }

    private static string LocateBroker()
    {
        var configured = Environment.GetEnvironmentVariable("WORKSETS_DESKTOP_BROKER");
        if (!string.IsNullOrWhiteSpace(configured) && File.Exists(configured)) return Path.GetFullPath(configured);
        var adjacent = Path.Combine(AppContext.BaseDirectory, "worksets-desktop-broker.exe");
        if (File.Exists(adjacent)) return adjacent;
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        for (var depth = 0; depth < 8 && directory is not null; depth++, directory = directory.Parent)
        {
            var debug = Path.Combine(directory.FullName, "src-tauri", "target", "debug", "worksets-desktop-broker.exe");
            if (File.Exists(debug)) return debug;
        }
        return adjacent;
    }
}

public sealed class DesktopBrokerException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
