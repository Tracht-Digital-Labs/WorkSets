using System.Diagnostics;
using System.Runtime.InteropServices;

namespace WorkSets.Windows;

internal static class PackagedAppActivator
{
    private const uint ActivateOptionsNone = 0;

    internal static Process? Activate(string aumid, IReadOnlyList<string> arguments)
    {
        if (string.IsNullOrWhiteSpace(aumid) || aumid.Length > 255) throw new ArgumentException("Invalid AUMID.", nameof(aumid));
        var manager = (IApplicationActivationManager)new ApplicationActivationManager();
        var encoded = string.Join(' ', arguments.Select(QuoteArgument));
        var result = manager.ActivateApplication(aumid, encoded, ActivateOptionsNone, out var processId);
        Marshal.ThrowExceptionForHR(result);
        try { return processId == 0 ? null : Process.GetProcessById((int)processId); }
        catch (ArgumentException) { return null; }
    }

    private static string QuoteArgument(string argument)
    {
        if (argument.IndexOf('\0') >= 0) throw new ArgumentException("Arguments must not contain null bytes.");
        if (argument.Length > 32_768) throw new ArgumentException("Argument is too long.");
        if (argument.Length > 0 && argument.All(character => !char.IsWhiteSpace(character) && character != '"')) return argument;
        var result = new System.Text.StringBuilder("\"");
        var backslashes = 0;
        foreach (var character in argument)
        {
            if (character == '\\') { backslashes++; continue; }
            if (character == '"') result.Append('\\', backslashes * 2 + 1).Append('"');
            else { result.Append('\\', backslashes).Append(character); }
            backslashes = 0;
        }
        result.Append('\\', backslashes * 2).Append('"');
        return result.ToString();
    }

    [ComImport, Guid("2E941141-7F97-4756-BA1D-9DECDE894A3D")]
    private class ApplicationActivationManager;

    [ComImport, Guid("2E941141-7F97-4756-BA1D-9DECDE894A3D"), InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    private interface IApplicationActivationManager
    {
        int ActivateApplication(
            [MarshalAs(UnmanagedType.LPWStr)] string appUserModelId,
            [MarshalAs(UnmanagedType.LPWStr)] string arguments,
            uint options,
            out uint processId);
        int ActivateForFile();
        int ActivateForProtocol();
    }
}
