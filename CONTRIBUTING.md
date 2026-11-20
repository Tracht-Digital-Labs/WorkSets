# Contributing / Mitwirken

Danke für dein Interesse an WorkSets. Beiträge sind auf Deutsch oder Englisch willkommen. / Thank you for your interest in WorkSets. Contributions are welcome in German or English.

## Vor dem Start / Before you start

- Suche zuerst in bestehenden Issues und eröffne für größere Änderungen ein Feature-Issue.
- Melde Sicherheitslücken ausschließlich privat gemäß [SECURITY.md](SECURITY.md).
- Halte Änderungen klein, nachvollziehbar und auf ein Problem begrenzt.
- Verwende keine proprietären Windows-Schnittstellendefinitionen ohne nachweisbar kompatible Lizenz und Herkunft.

---

- Search existing issues first and open a feature issue before a substantial change.
- Report vulnerabilities privately according to [SECURITY.md](SECURITY.md).
- Keep changes focused, reviewable, and limited to one concern.
- Do not add proprietary Windows interface definitions without a documented compatible license and provenance.

## Lokales Setup / Local setup

Erforderlich sind Windows 10/11, Node.js aus `.nvmrc`, Rust aus `rust-toolchain.toml`, WebView2 und Visual Studio Build Tools mit C++-Desktopkomponenten.

You need Windows 10/11, the Node.js version in `.nvmrc`, the Rust toolchain in `rust-toolchain.toml`, WebView2, and Visual Studio Build Tools with the C++ desktop workload.

```powershell
npm ci
npm run tauri -- dev
```

`npm run dev` startet nur die sichere Browser-Vorschau mit Beispieldaten. / `npm run dev` starts only the safe browser preview with sample data.

## Vor einem Pull Request / Before a pull request

```powershell
npm run lint
npm run typecheck
npm run test -- --run
npm run build
./scripts/check-licenses.ps1

Set-Location src-tauri
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
cargo check --locked --all-targets --all-features

Set-Location ..\fixtures\windows-app
cargo fmt --check
cargo check --locked
```

Ein Pull Request soll das Verhalten und die Motivation beschreiben, passende Tests enthalten, Datenschutz-/Sicherheitsfolgen benennen und bei UI-Änderungen Screenshots für Hell- und Dunkelmodus zeigen. Änderungen am Preset-Schema benötigen eine Migration, Schema-Tests und aktualisierte Beispieldaten.

A pull request should explain behavior and motivation, include suitable tests, call out privacy/security impact, and show light- and dark-mode screenshots for UI changes. Preset schema changes require a migration, schema tests, and updated fixtures.

Für sichere manuelle Fenster-, Schließ- und Single-Instance-Tests steht die isolierte [Windows-Testanwendung](fixtures/windows-app/README.md) bereit. Sie gehört ausschließlich zum Testcode und wird nicht im Release-Installer ausgeliefert. / Use the isolated [Windows fixture app](fixtures/windows-app/README.md) for safe manual window, close, and single-instance tests. It is test-only code and is never included in a release installer.

## Stil und Sicherheit / Style and safety

- TypeScript bleibt strikt typisiert; Rust-Code muss `clippy -D warnings` bestehen.
- Shell-Befehlsstrings für Startziele sind verboten. Executable und Argumentliste bleiben getrennt und werden validiert.
- Tests dürfen keine echten Benutzerfenster schließen oder Desktopzustände ohne explizite Fixture-Isolation verändern.
- Personenbezogene Pfade, URLs, Fenstertitel oder Monitor-IDs gehören weder in Logs noch in Telemetrie oder Test-Fixtures.
- Öffentliche APIs, `PresetV1`-Felder und sichtbare Texte werden in Deutsch und Englisch dokumentiert.

## Lizenz / License

Mit einem Beitrag erklärst du dich damit einverstanden, ihn unter der [MIT-Lizenz](LICENSE) des Projekts bereitzustellen. / By contributing, you agree to license your contribution under the project's [MIT License](LICENSE).
