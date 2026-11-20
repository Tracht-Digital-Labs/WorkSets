# WorkSets public alpha

> **Nicht für ungespeicherte produktive Arbeit empfohlen. / Not recommended for unsaved production work.**

WorkSets by Tracht Digital Solutions erfasst und restauriert Windows-Arbeitsbereiche mit virtuellen Desktops, Programmfenstern und Multi-Monitor-Platzierung. Diese Alpha ist für frühe Tests und Feedback gedacht.

WorkSets by Tracht Digital Solutions captures and restores Windows workspaces with virtual desktops, application windows, and multi-monitor placement. This alpha is intended for early testing and feedback.

## Downloads

- `WorkSets_0.1.0-alpha.1_windows_x64_setup.exe`
- `WorkSets_0.1.0-alpha.1_windows_arm64_setup.exe`
- `SHA256SUMS.txt`

Both installers are per-user NSIS packages and bootstrap Microsoft WebView2 when required. ARM64 and Windows 10 desktop control are experimental.

## Vor der Installation / Before installing

- Die Installer sind absichtlich **nicht Authenticode-signiert**. Microsoft Defender SmartScreen kann warnen.
- Lade Dateien nur von diesem offiziellen GitHub-Release herunter.
- Vergleiche SHA-256 mit `SHA256SUMS.txt` oder prüfe die GitHub-Attestierung:

  ```powershell
  gh attestation verify .\WorkSets_0.1.0-alpha.1_windows_x64_setup.exe --repo Tracht-Digital-Solutions/WorkSets
  ```

---

- The installers are deliberately **not Authenticode-signed**. Microsoft Defender SmartScreen may warn.
- Download files only from this official GitHub release.
- Compare SHA-256 with `SHA256SUMS.txt` or verify the GitHub attestation using the command above.

## Kompatibilität / Compatibility

- Primary target: Windows 11 25H2 build 26200 on x64.
- Supported adapters: Windows 11 builds 22621/22631 and 26100/26200.
- Experimental: Windows 10 22H2 build 19045 (capture/move/switch only; no safe desktop create/remove), Windows 11 26H1 build 28000 and other unknown builds after a read probe, and all ARM64 desktop-control paths.
- `ExactProtected` is the only exact-count policy. `ManagedOnly` can create missing desktops but deliberately leaves extra desktops in place in this alpha because native desktop IDs are not portable across sessions.

Microsoft ended standard support for Windows 10 22H2 in October 2025. WorkSets' experimental compatibility does not extend Microsoft's OS support; use an eligible ESU configuration or migrate to a supported Windows release.
- Unsupported: x86, MSI, portable installs, and complete restoration of browser/document sessions.

Read [COMPATIBILITY.md](https://github.com/Tracht-Digital-Solutions/WorkSets/blob/main/COMPATIBILITY.md) before testing. Save documents before applying a preset and report problems through the bilingual issue forms.

## Veröffentlichungskontrolle / Publication gate

Der Workflow erstellt absichtlich nur einen Draft-Prerelease. Vor manueller Veröffentlichung müssen der x64-Installer auf Windows 11 Build 26200 installiert, ein Preset-Wechsel auf drei Monitoren geprüft und die Deinstallation bestätigt werden.

The workflow intentionally creates a draft prerelease only. Before manual publication, install the x64 package on Windows 11 build 26200, test a preset switch across three monitors, and verify uninstallation.
