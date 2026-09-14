fn main() {
    // Embeds `icons/icon.ico` into the compiled .exe's own PE resources -
    // this is what Explorer, the taskbar's default (pre-launch) icon, and
    // the Start Menu tile actually read; it's independent of the runtime
    // window icon `eframe::NativeOptions` sets (see `gui::load_app_icon`),
    // which only takes effect once the app is already running.
    //
    // `#[cfg(target_os = "windows")]` reflects the HOST this build script
    // itself runs on, which only matches the crate's actual compile target
    // for a native build - exactly how `release.yml`/`ci.yml` build (each
    // platform's job runs on that platform's own runner, never cross-
    // compiled from a different host). Cross-compiling this crate to
    // Windows from a non-Windows host would need a differently-gated
    // build.rs (checking the `CARGO_CFG_TARGET_OS` env var instead) plus a
    // Windows resource compiler for that host, neither of which this
    // project's build currently needs.
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        if let Err(e) = res.compile() {
            println!("cargo:warning=failed to embed Windows icon resource: {e}");
        }
    }
}
