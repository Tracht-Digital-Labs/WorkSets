# WorkSets

**Windows-Arbeitsbereiche als Presets – by Tracht Digital Labs**

[![CI](https://github.com/Tracht-Digital-Labs/WorkSets/actions/workflows/ci.yml/badge.svg)](https://github.com/Tracht-Digital-Labs/WorkSets/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Tracht-Digital-Labs/WorkSets?include_prereleases&label=alpha)](https://github.com/Tracht-Digital-Labs/WorkSets/releases)
[![License: all rights reserved](https://img.shields.io/badge/License-all%20rights%20reserved-292522.svg)](LICENSE)

<img src="public/worksets-mark.svg" alt="WorkSets Logo: drei überlappende Arbeitsflächen" width="112">

> [!WARNING]
> WorkSets `0.1.0-alpha.1` ist eine öffentliche Alpha. Virtuelle Desktops verwenden nicht dokumentierte Windows-Schnittstellen. Speichere offene Dokumente, bevor du ein Preset wechselst. Windows 10 und ARM64 gelten in dieser Version als experimentell.

Das produktive Logo ist das vektorbasierte WorkSets-Symbol oben. Das [generierte Branding- und UI-Moodboard](docs/brand/worksets-concept.png) dokumentiert nur die frühe visuelle Richtung und ist kein finales Firmen- oder Produktlogo. / The vector WorkSets mark above is the production logo. The [generated branding and UI moodboard](docs/brand/worksets-concept.png) records early visual direction only; it is not a final company or product logo.

[English version](#english)

## Was ist WorkSets?

WorkSets erfasst eine Windows-Arbeitsumgebung und stellt sie später wieder her: virtuelle Desktops, Programme, Fensterpositionen und -größen, Monitorzuordnung sowie normaler, maximierter oder minimierter Zustand. Ein Preset-Wechsel öffnet und ordnet nur die ausgewählten Programme; unverwaltete Programme werden niemals automatisch beendet.

Die Anwendung ist für mehrere Monitore, unterschiedliche DPI-Skalierungen und negative Bildschirmkoordinaten ausgelegt. Sie läuft ohne Administratorrechte und kennzeichnet erhöhte Fenster, die Windows vor einem normalen Prozess schützt.

### Kernfunktionen

- Aktuelle Arbeitsumgebung erfassen und als Preset bearbeiten.
- Presets mit `ExactProtected`- oder `ManagedOnly`-Desktop-Richtlinie anwenden.
- Bestehende Fenster wiederverwenden, Programme neu starten oder Einträge geöffnet lassen.
- EXE-/Store-Apps, Dateien und HTTP(S)-URLs als Startziele speichern.
- Globale Hotkeys, Tray-Steuerung, optionaler Autostart und Deutsch/Englisch.
- Lokale JSON-Speicherung mit atomarem Austausch und Sicherungskopien.
- Detaillierter Ergebnisbericht mit transparenten Teilfehlern und Monitor-Ersatzplatzierungen.

WorkSets speichert keine Screenshots, Fensterinhalte oder Dokumentinhalte. Details stehen in der [Datenschutzerklärung](PRIVACY.md).

`ExactProtected` stellt als einzige Richtlinie eine exakte globale Desktopzahl her. `ManagedOnly` erstellt fehlende Desktops, entfernt in dieser Alpha aber keine und kann deshalb zusätzliche Desktops bestehen lassen.

## Kompatibilität

| Windows-Version | Build-Familie | x64 | ARM64 |
| --- | ---: | --- | --- |
| Windows 11 26H1 | 28000 | Experimenteller Lesetest; kein eigener Alpha-Adapter | Experimentell; CI-Build |
| Windows 11 25H2 | 26200 | Primäres Alpha-Ziel | Experimentell; CI-Build |
| Windows 11 24H2 | 26100 | Unterstützter Adapter | Experimentell; CI-Build |
| Windows 11 22H2/23H2 | 22621/22631 | Unterstützter Adapter | Experimentell; CI-Build |
| Windows 10 22H2 | 19045 | Experimentell; keine sichere Änderung der Desktopzahl | Experimentell; CI-Build, keine sichere Änderung der Desktopzahl |

Andere Builds führen zunächst nur einen sicheren Lesetest aus. Scheitert er, deaktiviert WorkSets Desktop-Änderungen für die laufende Sitzung. x86 wird nicht unterstützt. Die vollständigen Einschränkungen und Teststufen stehen in [COMPATIBILITY.md](COMPATIBILITY.md).

## Installation

1. Öffne den neuesten [GitHub-Prerelease](https://github.com/Tracht-Digital-Labs/WorkSets/releases).
2. Lade den Installer für `x64` oder `arm64` herunter.
3. Vergleiche optional dessen SHA-256-Wert mit `SHA256SUMS.txt` und prüfe die Provenienz mit `gh attestation verify <datei> --repo Tracht-Digital-Labs/WorkSets`.
4. Starte den Installer für den aktuellen Benutzer.

Die Alpha ist absichtlich **nicht Authenticode-signiert**. Microsoft Defender SmartScreen kann deshalb „Windows hat den PC geschützt“ anzeigen. Installiere ausschließlich Assets aus dem offiziellen Repository und prüfe Prüfsumme bzw. Attestierung. Eine solche Warnung ist kein Beleg für Schadsoftware, aber ein Hinweis darauf, dass Windows dem Herausgeber noch nicht vertraut.

## Entwicklung

Voraussetzungen sind Windows 10/11, Node.js 24, Rust 1.96, WebView2 sowie die C++-Desktopkomponenten der Visual Studio Build Tools.

```powershell
npm ci
npm run tauri -- dev
```

Für reine Frontend-Arbeit startet `npm run dev` die Browser-Vorschau mit sicheren Beispieldaten. Windows-Fenster und virtuelle Desktops werden nur in der Tauri-App angesprochen.

Qualitätsprüfungen:

```powershell
npm run lint
npm run typecheck
npm run test -- --run
npm run build

Set-Location src-tauri
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo check --locked --all-targets --all-features
```

Einen lokalen NSIS-Installer erzeugt `npm run tauri -- build --bundles nsis -- --locked`. Beiträge folgen [CONTRIBUTING.md](CONTRIBUTING.md); Sicherheitslücken bitte nicht als öffentliches Issue melden, sondern gemäß [SECURITY.md](SECURITY.md).

Die deterministische [Windows-Testanwendung](fixtures/windows-app/README.md) deckt verzögerte und mehrere Fenster, Mindestgrößen, Speichern-Dialoge, Hänger und Single-Instance-Verhalten ab. Sie ist reiner Testcode und wird nicht in den Installer aufgenommen.

## Projektstatus und Grenzen

- Browser-Tabs, interne App-Sitzungen und ungespeicherte Dokumentinhalte können nicht rekonstruiert werden.
- Anwendungen dürfen Mindestgrößen erzwingen oder eine angeforderte Position überschreiben; WorkSets meldet dann einen Teilerfolg.
- Die Alpha bietet keine Cloud-Synchronisation, Benutzerkonten, x86-, MSI- oder Portable-Pakete.
- Automatische Updates werden nicht installiert. WorkSets prüft höchstens einmal täglich auf öffentliche Prereleases und öffnet auf Wunsch die Downloadseite.

**Öffentlich einsehbar, alle Rechte vorbehalten.** Es besteht keine allgemeine Erlaubnis zum Kopieren, Verändern oder kommerziellen Weiterverwenden; die vollständigen Bedingungen stehen in [LICENSE](LICENSE). Früher wirksam eingeräumte Nutzungsrechte (ältere Fassungen enthielten eine MIT-Lizenzangabe) bleiben unberührt. Drittanbieter-Hinweise befinden sich in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

---

## English

**Windows workspace presets – by Tracht Digital Labs**

> [!WARNING]
> WorkSets `0.1.0-alpha.1` is a public alpha. Virtual desktop support relies on undocumented Windows interfaces. Save open documents before switching a preset. Windows 10 and ARM64 are experimental in this release.

## What is WorkSets?

WorkSets captures a Windows workspace and restores it later: virtual desktops, applications, window positions and sizes, monitor assignments, and normal, maximized, or minimized state. Switching a preset opens and arranges only the selected applications; unmanaged applications are never closed automatically.

The app is designed for multiple monitors, mixed DPI scaling, and negative screen coordinates. It runs without administrator rights and marks elevated windows that Windows prevents a normal process from managing.

### Highlights

- Capture the current workspace and edit it as a preset.
- Apply presets with either the `ExactProtected` or `ManagedOnly` desktop policy.
- Reuse existing windows, relaunch an app, or leave an item open.
- Store EXE/Store apps, files, and HTTP(S) URLs as launch targets.
- Global hotkeys, tray controls, optional launch at sign-in, and German/English UI.
- Local JSON storage with atomic replacement and backups.
- Detailed apply reports for partial failures and monitor fallback placement.

WorkSets never stores screenshots, window contents, or document contents. See the [privacy notice](PRIVACY.md) for details.

`ExactProtected` is the only policy that enforces an exact global desktop count. `ManagedOnly` creates missing desktops but removes none in this alpha, so extra desktops may remain.

## Compatibility

| Windows version | Build family | x64 | ARM64 |
| --- | ---: | --- | --- |
| Windows 11 26H1 | 28000 | Experimental read probe; no dedicated alpha adapter | Experimental; CI build |
| Windows 11 25H2 | 26200 | Primary alpha target | Experimental; CI build |
| Windows 11 24H2 | 26100 | Supported adapter | Experimental; CI build |
| Windows 11 22H2/23H2 | 22621/22631 | Supported adapter | Experimental; CI build |
| Windows 10 22H2 | 19045 | Experimental; no safe desktop-count mutation | Experimental CI build; no safe desktop-count mutation |

Unknown builds receive a safe read-only probe first. If it fails, WorkSets disables desktop mutations for the current session. x86 is unsupported. See [COMPATIBILITY.md](COMPATIBILITY.md) for the complete support and test matrix.

## Install

1. Open the latest [GitHub prerelease](https://github.com/Tracht-Digital-Labs/WorkSets/releases).
2. Download the `x64` or `arm64` installer.
3. Optionally compare its SHA-256 digest with `SHA256SUMS.txt` and verify provenance with `gh attestation verify <file> --repo Tracht-Digital-Labs/WorkSets`.
4. Run the per-user installer.

This alpha is deliberately **not Authenticode-signed**. Microsoft Defender SmartScreen may therefore display “Windows protected your PC.” Install only assets from the official repository and verify the checksum or attestation. The warning is not evidence of malware, but it does mean Windows does not yet trust the publisher identity.

## Develop

You need Windows 10/11, Node.js 24, Rust 1.96, WebView2, and the C++ desktop components from Visual Studio Build Tools.

```powershell
npm ci
npm run tauri -- dev
```

For frontend-only work, `npm run dev` starts the browser preview with safe sample data. Native window and virtual-desktop APIs are accessed only by the Tauri app.

Run the checks shown in the German development section above. Create a local NSIS installer with `npm run tauri -- build --bundles nsis -- --locked`. See [CONTRIBUTING.md](CONTRIBUTING.md) before contributing. Report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

The deterministic [Windows fixture app](fixtures/windows-app/README.md) covers delayed and multiple windows, minimum sizing, save prompts, hangs, and single-instance behavior. It is test-only code and is not packaged in the installer.

## Status and limitations

- Browser tabs, internal application sessions, and unsaved document contents cannot be reconstructed.
- Applications may enforce minimum sizes or override requested placement; WorkSets reports a partial result in those cases.
- The alpha does not include cloud sync, user accounts, x86, MSI, or portable packages.
- Updates are never installed automatically. WorkSets checks public prereleases no more than once per day and only opens the download page after user action.

**Publicly viewable, all rights reserved.** Public access grants no general permission to copy, modify or commercially reuse the code; see [LICENSE](LICENSE) for the complete terms. Rights granted earlier (older versions carried an MIT notice) remain unaffected. Third-party notices are in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
