# Datenschutz / Privacy

Stand / Effective: 21. August 2026

## Kurzfassung

WorkSets benötigt kein Konto und überträgt in der Alpha keine Nutzungsanalyse. Presets, Einstellungen, Laufzeit-Zuordnungen, Sicherungen und Apply-Berichte bleiben auf dem Windows-Gerät. Eine optionale, anonyme Telemetrie sammelt in der Alpha ausschließlich begrenzte lokale Ereignisse und sendet sie an keinen Server.

WorkSets does not require an account and sends no usage analytics in the alpha. Presets, settings, runtime ownership records, backups, and apply reports remain on the Windows device. Optional anonymous telemetry records a small set of events locally in the alpha and sends them to no server.

## Lokal gespeicherte Daten / Data stored locally

WorkSets speichert unter `%LOCALAPPDATA%\Tracht Digital Solutions\WorkSets` insbesondere:

| Kategorie / Category | Inhalt / Contents | Zweck / Purpose |
| --- | --- | --- |
| Presets | Programmpfade oder AUMIDs, explizite Argumente, Datei-/HTTP(S)-Ziele, Fensterselektoren, Geometrie, Monitor- und Desktop-Referenzen | Arbeitsumgebung wiederherstellen / Restore a workspace |
| Einstellungen | Sprache, Theme, Autostart, Hotkeys, Desktop-Richtlinie, Telemetrie-Einwilligung | Gewähltes Verhalten beibehalten / Remember chosen behavior |
| Update-Prüfung / Update check | Zeitpunkt und Ergebnis der letzten Prüfung sowie öffentliche Release-URL | 24-Stunden-Intervall auch nach einem Neustart einhalten / Preserve the 24-hour interval across restarts |
| Laufzeitstatus | Von WorkSets gestartete Prozess-/Fensterzuordnungen einschließlich Prozess-ID und gegebenenfalls EXE-Pfad | Nur verwaltete Prozesse sicher behandeln / Safely manage only owned processes |
| Apply-Berichte | Preset-/Eintrags-IDs, Anzeigenamen, verwaltete Fenster-ID, Status, Dauer, Fehlerklassen und Anpassungshinweise | Ergebnisse erklären und Fehler diagnostizieren / Explain outcomes and diagnose failures |
| Optionale lokale Diagnose / Optional local diagnostics | Zeitstempel, Ereignisart, Status und aggregierte Anzahlen; höchstens 200 Ereignisse und 256 KiB | Lokale Fehlersuche nach Einwilligung / Consented local troubleshooting |
| Sicherungen | Jeweils vorherige Version atomar ersetzter lokaler JSON-Dateien | Wiederherstellung nach einem Schreibfehler / Recover from a write failure |

Fenstertitel können optional Teil eines vom Benutzer gewählten Selektors sein und verbleiben dann im lokalen Preset. WorkSets speichert keine Screenshots, Pixelinhalte, Tastatureingaben, Dokumentinhalte oder Browser-Verläufe. Browser-Tabs werden nur dann als URL gespeichert, wenn der Benutzer eine URL ausdrücklich als Startziel hinzufügt.

Window titles may be stored only when the user chooses a title-based selector; they remain in the local preset. WorkSets does not store screenshots, pixel contents, keystrokes, document contents, or browsing history. A browser URL is stored only when the user explicitly adds that URL as a launch target.

## Telemetrie / Telemetry

Telemetrie ist standardmäßig aus und erfordert eine ausdrückliche Einwilligung. In der Alpha werden auch nach Einwilligung nur lokale, begrenzte Ereignisse erfasst. Die rollierende Queue umfasst höchstens 200 Ereignisse beziehungsweise 256 KiB; zusätzlich bewahrt das atomare Schreiben höchstens eine vorherige Queue-Datei als Sicherung auf. Gespeichert werden nur Zeitstempel, allgemeine Ereignisart, Status und aggregierte Anzahlen. Ausgeschlossen sind Namen, vollständige Pfade, URLs, Argumente, Fenstertitel, Presetnamen, Monitor-IDs und Dokumentinhalte. Es gibt keinen Upload-Endpunkt.

Telemetry is off by default and requires explicit consent. During the alpha, consent enables only a rolling local queue capped at 200 events or 256 KiB; atomic replacement may retain at most one prior queue file as a backup. It stores timestamps, general event kinds, status, and aggregate counts only. Names, full paths, URLs, arguments, window titles, preset names, monitor identifiers, and document contents are excluded. No upload endpoint exists.

## Netzwerkzugriffe / Network access

Höchstens einmal innerhalb von 24 Stunden kann WorkSets die öffentliche GitHub-Releases-API abfragen, um auf einen neuen Prerelease hinzuweisen. Dabei erhält GitHub technisch übliche Verbindungsdaten wie IP-Adresse und User-Agent nach der [GitHub-Datenschutzerklärung](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement). WorkSets lädt und installiert Updates nie automatisch. Wenn Microsoft WebView2 noch fehlt, kann der Installer den WebView2-Bootstrapper von Microsoft herunterladen und ausführen; dafür gilt die [Microsoft-Datenschutzerklärung](https://privacy.microsoft.com/privacystatement). Weitere Netzwerkzugriffe entstehen nur, wenn der Benutzer ein gespeichertes HTTP(S)-Ziel oder einen externen Link öffnet.

At most once in 24 hours, WorkSets may query the public GitHub Releases API to notify the user about a new prerelease. GitHub necessarily receives ordinary connection data such as IP address and user agent under the [GitHub Privacy Statement](https://docs.github.com/en/site-policy/privacy-policies/github-general-privacy-statement). WorkSets never downloads or installs updates automatically. If Microsoft WebView2 is missing, the installer may download and run Microsoft's WebView2 bootstrapper under the [Microsoft Privacy Statement](https://privacy.microsoft.com/privacystatement). Other network access occurs only when the user opens a stored HTTP(S) target or an external link.

## Kontrolle und Löschung / Control and deletion

Presets lassen sich in der Anwendung löschen. Wird die Telemetrie-Einwilligung ausgeschaltet, löscht WorkSets die begrenzte lokale Diagnose-Queue automatisch. Eine vollständige lokale Löschung erfolgt durch Deinstallation und anschließendes Entfernen von `%LOCALAPPDATA%\Tracht Digital Solutions\WorkSets`. Vorher exportierte Presetdateien und vom Benutzer angelegte Sicherungen müssen separat gelöscht werden.

Presets can be deleted in the app. Turning off telemetry consent automatically deletes the limited local diagnostics queue. To remove all local data, uninstall WorkSets and then delete `%LOCALAPPDATA%\Tracht Digital Solutions\WorkSets`. Exported presets and user-created backups must be removed separately.

Datenschutz- oder Sicherheitsfragen bitte über ein privates [Security Advisory](https://github.com/Tracht-Digital-Solutions/WorkSets/security/advisories/new) stellen; keine sensiblen Daten in öffentliche Issues einfügen.

For privacy or security questions, open a private [Security Advisory](https://github.com/Tracht-Digital-Solutions/WorkSets/security/advisories/new). Never include sensitive data in a public issue.
