# `crates/app` - CLI entry point and the self-updater

The `app` crate is both the CLI binary and the GUI (see
[07-app-gui.md](07-app-gui.md) for the GUI half). This file covers
`main.rs` and `update.rs` - the non-GUI parts.

## `main.rs`

`clap`-derived CLI with two subcommands:

- **`build`** - parse → eval → layout_ring → relax → (optionally)
  analyze_tension → (optionally) export SVG/OBJ. This is the CLI's whole
  job, and reading `build()` top to bottom is honestly the fastest way to
  see the entire library pipeline invoked in the "boring", non-GUI way -
  useful as a reference if you're debugging whether a bug is in the
  pipeline itself vs. somewhere GUI-specific. Crochet only: if the file is
  a chart-craft `.cgp` (`crossstitch::is_chart_source`), `build` stops
  with a clear message rather than a confusing crochet parse error. The
  chart crafts have no CLI yet.
- **`Gui { input: Option<PathBuf> }`** - launches `gui::run`.

`command: Option<Commands>` (not a required subcommand) plus
`cli.command.unwrap_or(Commands::Gui { input: None })` is the trick that
makes double-clicking the built executable (no args at all) fall through to
launching the GUI, rather than clap printing a usage error and exiting.

**Windows console handling** is the one platform-specific wrinkle worth
understanding if you ever touch packaging: `#![cfg_attr(windows,
windows_subsystem = "windows")]` at the top of the file tells the linker to
build a GUI-subsystem executable - this is what stops a blank terminal
window from popping up behind the GUI on launch. But that same flag means a
CLI invocation (`abyssal-thread.exe build ...` from an actual terminal)
would otherwise have nowhere to print to. `attach_parent_console()`
(Windows-only, called only when `std::env::args().len() > 1`, i.e. never
on a bare double-click) calls the Win32 `AttachConsole(ATTACH_PARENT_PROCESS)`
API and re-points `stdout`/`stderr` at the reattached console. This is a
good small example of "the flag that fixes one platform problem (console
flash) creates a different one (no CLI output), and the fix for the second
problem is conditional on how the process was actually invoked."

## `update.rs` - GitHub-releases self-updater

The whole file is one instance of a pattern worth learning generally:
**"fire a blocking operation on a background thread, hand the result back
through an `mpsc` channel, poll non-blockingly once per frame."** This is
the standard way to do blocking I/O (here: HTTP calls via `ureq`) inside an
immediate-mode GUI that has no async runtime at all - see
[09-rust-patterns-glossary.md](09-rust-patterns-glossary.md#threadchannel-instead-of-async)
for the deeper explanation of *why* this shape rather than `async`/`await`.

- `check_async`/`download_async` each spawn a `thread::spawn` closure that
  does the blocking work and `tx.send(result)`s it, returning the `rx` half
  immediately.
- `GoblinApp::poll_update_check`/`poll_update_download` (in `gui/mod.rs`)
  call `rx.try_recv()` once per frame - never `.recv()`, which would block
  the whole GUI thread until the network call finished.
- `Receiver<T>` implements neither `Clone` nor is cheap to hold onto
  indefinitely, so the pattern is: `Option<Receiver<...>>`, set to `Some`
  when a check starts, set back to `None` the moment `try_recv()` succeeds
  (or fails with `Disconnected`) - this is why `poll_update_check` starts
  with `let Some(rx) = &self.update_check_rx else { return };`.

`check()` compares the latest GitHub release's tag against
`CARGO_PKG_VERSION` via `semver::Version` - returns `None` (not an error)
both when already up to date *and* when this is a local/dev build ahead of
the latest release (`latest <= current`), so neither case triggers the
update dialog.

`pick_asset` matches this build's actual OS/preferred install method
against the naming convention `.github/workflows/release.yml`'s "Rename
assets" step produces - if you ever change that workflow's asset naming,
this function (and its tests) are the other end of that contract and need
to move together.

`update_check_is_manual` is the one flag worth remembering if you're
extending this: the **silent startup check** must never surface a failure
(no internet shouldn't nag anyone), but the **explicit "Check for Updates"
button** press should always tell the user *something* happened, success or
failure. Same underlying function (`check_async`), different
error-handling policy at the call site depending on *why* it was called -
a good small example of "the same operation can need different error UX
depending on whether the user explicitly asked for it."
