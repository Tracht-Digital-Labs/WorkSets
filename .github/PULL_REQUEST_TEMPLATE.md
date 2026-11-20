## Ziel / Goal

<!-- Was ändert sich und warum? / What changes, and why? -->

## Prüfung / Verification

<!-- Tests, Betriebssystem/Build, Architektur, Monitore/DPI / Tests, OS/build, architecture, monitors/DPI -->

- [ ] `npm run lint`, `npm run typecheck`, `npm run test -- --run`, and `npm run build` pass.
- [ ] `cargo fmt --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`, `cargo test --locked --all-features`, and `cargo check --locked --all-targets --all-features` pass.
- [ ] `cargo fmt --manifest-path fixtures/windows-app/Cargo.toml --check` and `cargo check --manifest-path fixtures/windows-app/Cargo.toml --locked` pass.
- [ ] Neue oder geänderte Logik ist getestet. / New or changed logic is tested.
- [ ] UI-Änderungen wurden per Tastatur sowie in Hell-/Dunkelmodus geprüft. / UI changes were checked by keyboard and in light/dark mode.

## Sicherheit und Datenschutz / Security and privacy

- [ ] Keine Shell-Befehlsstrings oder unnötigen Berechtigungen. / No shell command strings or unnecessary permissions.
- [ ] Keine Pfade, URLs, Fenstertitel, Monitor-IDs oder anderen persönlichen Daten in Logs/Telemetrie. / No paths, URLs, titles, monitor IDs, or other personal data in logs/telemetry.
- [ ] Änderungen an Datenformat, Speicherung, Netzwerkzugriff oder Prozesssteuerung sind unten erklärt. / Changes to data format, storage, network access, or process control are explained below.

<!-- Auswirkungen oder "Keine" / Impact or "None" -->

## Screenshots / Screenshots

<!-- Für UI-Änderungen ohne private Fensterinhalte / For UI changes, with no private window content -->
