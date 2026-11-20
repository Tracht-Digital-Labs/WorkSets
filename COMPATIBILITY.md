# Kompatibilität / Compatibility

WorkSets `0.1.0-alpha.1` unterstützt ausschließlich Windows 10/11 auf x64 und ARM64. Die Desktopsteuerung ist in Windows nicht vollständig öffentlich dokumentiert; deshalb ist jede Buildfamilie ein eigener, versionsgebundener Adapter.

WorkSets `0.1.0-alpha.1` supports Windows 10/11 on x64 and ARM64 only. Windows does not fully document the desktop-control interfaces, so each build family uses a separate version-gated adapter.

## Matrix

| System | Build | Adapter | x64-Status | ARM64-Status |
| --- | ---: | --- | --- | --- |
| Windows 11 26H1 | 28000 | Neuester Adapter nach Lesetest | Experimentell; kein eigener Alpha-Adapter | Experimentell; kein eigener Alpha-Adapter |
| Windows 11 25H2 | 26200 | `26200` | Primäres Alpha-Ziel; manuelle Drei-Monitor-Prüfung vor Release erforderlich | Nativer CI-Build; Desktopsteuerung experimentell |
| Windows 11 24H2 | 26100 | `26100` | Unterstützt; manuelle Prüfung empfohlen | Nativer CI-Build; Desktopsteuerung experimentell |
| Windows 11 23H2 | 22631 | `22621/22631` | Unterstützt; manuelle Prüfung empfohlen | Nativer CI-Build; Desktopsteuerung experimentell |
| Windows 11 22H2 | 22621 | `22621/22631` | Unterstützt; manuelle Prüfung empfohlen | Nativer CI-Build; Desktopsteuerung experimentell |
| Windows 10 22H2 | 19045 | `19045` | Experimentell; Erfassen, Verschieben und Schalten verfügbar, aber kein sicheres Erstellen/Entfernen von Desktops | Nativer CI-Build; gleiche Einschränkung, keine realen Gerätetests |
| Andere Builds | andere | Neuester kompatibler Adapter nach Lesetest | Experimentell | Experimentell |

“CI-Build” bestätigt, dass Quellcode und NSIS-Paket nativ auf einem von GitHub gehosteten Runner gebaut werden. Es bestätigt keine reale Explorer-, Multi-Monitor- oder virtuelle-Desktop-Interaktion. / “CI build” confirms that source and the NSIS package build natively on a GitHub-hosted runner. It does not validate real Explorer, multi-monitor, or virtual-desktop interaction.

Die Build-Zuordnung folgt der offiziellen [Windows-11-Releaseübersicht](https://learn.microsoft.com/en-us/windows/release-health/windows11-release-information) und der [Microsoft-Buildtabelle](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/inf-manufacturer-section). Der ARM64-CI-Job verwendet GitHubs nativen [`windows-11-arm`-Runner](https://github.blog/changelog/2025-08-07-arm64-hosted-runners-for-public-repositories-are-now-generally-available/). / Build mappings follow Microsoft's official release information; the ARM64 CI job uses GitHub's native `windows-11-arm` runner.

Microsoft hat den regulären Support für Windows 10 22H2 am 14. Oktober 2025 beendet; WorkSets-Kompatibilität verlängert den Betriebssystem-Support nicht. Für weiter genutzte Geräte gelten Microsofts [ESU- und Migrationshinweise](https://learn.microsoft.com/en-us/windows/release-health/status-windows-10-22h2). / Microsoft ended standard support for Windows 10 22H2 on 14 October 2025; WorkSets compatibility does not extend OS support. Consult Microsoft's ESU and migration guidance for devices that remain in use.

## Sicheres Verhalten / Safe behavior

- Ein unbekannter Windows-Build erhält zuerst einen rein lesenden Adaptertest.
- Der erste Broker- oder Adapterfehler öffnet den Circuit Breaker und deaktiviert weitere Desktop-Mutationen bis zum Neustart von WorkSets.
- Fenster- und Monitorfunktionen bleiben verfügbar, wenn sie sicher ausgeführt werden können; der Ergebnisbericht weist auf die Einschränkung hin.
- WorkSets läuft nicht erhöht. Fenster höher privilegierter Prozesse werden erkannt und übersprungen.
- Unverwaltete Anwendungen werden nie geschlossen. Vor dem Entfernen fremd belegter Desktops verlangt `ExactProtected` eine ausdrückliche Entscheidung.
- `ExactProtected` ist die einzige Richtlinie, die eine exakte globale Desktopzahl herstellt. `ManagedOnly` erstellt fehlende Desktops, entfernt in der Alpha jedoch keine, weil Windows-Desktop-IDs nicht dauerhaft portabel zugeordnet werden können; zusätzliche Desktops dürfen deshalb bestehen bleiben.

---

- An unknown Windows build first receives a read-only adapter probe.
- The first broker or adapter failure opens the circuit breaker and disables further desktop mutations until WorkSets restarts.
- Window and monitor features remain available where safe; the apply report explains the limitation.
- WorkSets does not run elevated. Windows belonging to higher-privileged processes are detected and skipped.
- Unmanaged applications are never closed. `ExactProtected` requires an explicit decision before removing desktops occupied by foreign windows.
- `ExactProtected` is the only policy that enforces an exact global desktop count. `ManagedOnly` creates missing desktops but does not remove any in this alpha because native Windows desktop IDs cannot be mapped portably across sessions; extra desktops may therefore remain.

## Manuelle Release-Prüfung / Manual release verification

Vor Veröffentlichung der Alpha wird auf Windows 11 Build 26200 x64 mit drei Monitoren geprüft: / Before publishing the alpha, verify on Windows 11 build 26200 x64 with three monitors:

- Capture and restore across multiple virtual desktops.
- Normal, maximized, and minimized window states.
- Negative coordinates, mixed DPI, monitor reorder, and monitor hot-plug.
- Safe close, skipped/elevated windows, partial failure, and Explorer restart.
- Repeated apply is idempotent; fixture geometry is within ±2 physical pixels for an unchanged topology.
- No unmanaged window is closed.

Windows 10 and ARM64 remain explicitly experimental until the same suite has passed on physical or representative virtual devices.

On Windows 10 build 19045, the alpha adapter can capture desktops, move windows, and switch desktops. It does not create or remove desktops because those mutations cannot be guaranteed safely; therefore `ExactProtected` cannot enforce an exact desktop count on that build and reports the limitation instead.

## Nicht unterstützt / Unsupported

- x86, Windows Server, Wine/Proton, MSI, and portable packages.
- Full restoration of browser sessions, tabs, unsaved documents, or private in-app state.
- Guaranteed placement where an application enforces a minimum size or continuously repositions its own windows.
