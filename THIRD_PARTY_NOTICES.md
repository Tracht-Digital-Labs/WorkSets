# Third-party notices

WorkSets is distributed under the MIT License. It includes or builds on third-party open-source software whose own copyright notices and license terms remain in force.

The following list identifies principal components; it is not a substitute for the complete dependency metadata in `package-lock.json`, `src-tauri/Cargo.lock`, and the test-only `fixtures/windows-app/Cargo.lock`.

| Component | Purpose | License |
| --- | --- | --- |
| [Tauri](https://github.com/tauri-apps/tauri) | Desktop application framework and bundler | Apache-2.0 OR MIT |
| [React](https://github.com/facebook/react) | User interface | MIT |
| [Fluent UI React](https://github.com/microsoft/fluentui) | Accessible Windows-oriented UI components | MIT |
| [Vite](https://github.com/vitejs/vite) | Frontend development and build tooling | MIT |
| [TypeScript](https://github.com/microsoft/TypeScript) | Type checking and compilation | Apache-2.0 |
| [Serde](https://github.com/serde-rs/serde) | Rust serialization | Apache-2.0 OR MIT |
| [windows-rs / windows-sys](https://github.com/microsoft/windows-rs) | Windows API bindings for the app and its test fixture | Apache-2.0 OR MIT |
| [VirtualDesktopAccessor / winvd](https://github.com/Ciantic/VirtualDesktopAccessor) | Historical virtual-desktop interface research and definitions | MIT |

No prebuilt third-party virtual-desktop DLL is downloaded or shipped. Any compatible interface definitions used by WorkSets are built from pinned, reviewable source and retain their original notices.

JavaScript and Rust dependency licenses are checked in CI for missing or prohibited copyleft declarations. For the exact source and license text of a transitive dependency, follow its package metadata and repository link from the lockfiles.

WorkSets is not affiliated with or endorsed by Microsoft. Windows, Microsoft Defender SmartScreen, Fluent UI, and WebView2 are trademarks or products of Microsoft Corporation.
