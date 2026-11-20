# Security / Sicherheit

## Sicherheitslücken melden

Bitte melde vermutete Sicherheitslücken **nicht in einem öffentlichen Issue**. Verwende stattdessen die Funktion „Report a vulnerability“ im Bereich [Security Advisories](https://github.com/Tracht-Digital-Labs/WorkSets/security/advisories/new). Beschreibe betroffene Versionen, nachvollziehbare Schritte, mögliche Auswirkungen und – falls vorhanden – einen Vorschlag zur Behebung. Teile keine personenbezogenen Daten oder vertraulichen Dokumente.

Wir bestätigen den Eingang nach Möglichkeit innerhalb von drei Werktagen, prüfen die Meldung, stimmen eine Veröffentlichung ab und nennen die meldende Person auf Wunsch. Bitte veröffentliche Details erst, nachdem ein Fix verfügbar ist oder eine gemeinsame Frist vereinbart wurde.

## Report a vulnerability

Please do **not** disclose suspected vulnerabilities in a public issue. Use “Report a vulnerability” under [Security Advisories](https://github.com/Tracht-Digital-Labs/WorkSets/security/advisories/new). Include affected versions, reproducible steps, possible impact, and a suggested remediation if available. Do not include personal data or confidential documents.

We aim to acknowledge reports within three business days, investigate them, coordinate disclosure, and credit the reporter if desired. Please wait to publish details until a fix is available or an agreed disclosure deadline has passed.

## Unterstützte Versionen / Supported versions

| Version | Sicherheitsupdates / Security updates |
| --- | --- |
| Aktueller `0.1.x`-Prerelease / Latest `0.1.x` prerelease | Ja / Yes |
| Ältere Prereleases / Older prereleases | Nein / No |
| Unveröffentlichte Forks / Unreleased forks | Nein / No |

## Sicherheitsmodell / Security model

- WorkSets läuft als normaler Benutzer und versucht nicht, die Windows-Rechteisolation zu umgehen.
- Erhöhte Fenster werden übersprungen. Erzwungenes Beenden darf nur für eindeutig von WorkSets gestartete Prozesse angeboten werden.
- Importierte Presets sind Daten, kein Skript: Argumente werden als Listen verarbeitet und nicht an eine Shell übergeben.
- Der Desktop-Broker läuft als separater Kindprozess und kommuniziert über eine zufällig benannte, ausschließlich lokale Current-User-Named-Pipe. Ihre Zugriffssteuerung erlaubt nur dem aktuellen Benutzer und `SYSTEM` den Zugriff; Remote-Clients werden abgewiesen. Eltern- und Kindprozess prüfen gegenseitig ihre Prozess-IDs sowie Protokollversion, einmalige UUID-Nonce und Request-IDs. Bei einem Broker-/Adapterfehler werden Desktop-Änderungen für die Sitzung abgeschaltet.
- Releases sind in der Alpha nicht Authenticode-signiert. Prüfsummen und GitHub-Attestierungen schützen die Integrität, ersetzen aber keine vertrauenswürdige Herausgebersignatur.

---

- WorkSets runs as a standard user and does not attempt to bypass Windows privilege isolation.
- Elevated windows are skipped. Force termination may only be offered for processes that WorkSets can prove it started.
- Imported presets are data, not scripts: arguments are handled as arrays and never passed through a shell.
- The desktop broker runs as a separate child process and communicates through a randomly named, local-only Current-User named pipe. Its access control permits only the current user and `SYSTEM`; remote clients are rejected. Parent and child mutually validate their process IDs together with the protocol version, a one-time UUID nonce, and request IDs. A broker or adapter failure disables desktop mutations for the session.
- Alpha releases are not Authenticode-signed. Checksums and GitHub attestations protect integrity but do not replace a trusted publisher signature.
