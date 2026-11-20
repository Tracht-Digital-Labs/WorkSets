# WorkSets Windows window fixture

Small, non-production Win32 application used for manual capture/apply tests. It
never reads documents or user data. Build it with:

```powershell
cargo build --manifest-path fixtures/windows-app/Cargo.toml --release
```

Run one of the deterministic modes:

```powershell
worksets-window-fixture normal
worksets-window-fixture delayed
worksets-window-fixture multiple
worksets-window-fixture minimum-size
worksets-window-fixture save-dialog
worksets-window-fixture hung
worksets-window-fixture single-instance
```

- `delayed` creates its window after 1.5 seconds.
- `multiple` creates two top-level windows in one process.
- `minimum-size` enforces a 640 × 420 pixel minimum tracking size.
- `save-dialog` asks before accepting `WM_CLOSE`, simulating unsaved work.
- `hung` shows a window and then intentionally stops pumping messages.
- `single-instance` uses a per-user mutex and focuses the first window when run again.

The fixture is intentionally not included in release installers.

After a release build, run `./fixtures/windows-app/smoke.ps1` to exercise every
mode. The script only terminates fixture process IDs that it started itself.
