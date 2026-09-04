fn main() {
    tauri_build::build();
    build_frontend_if_missing();
}

/// `cargo run -p igdm` must work from the workspace root without a manual
/// frontend build: compile-time asset embedding (generate_context!) fails
/// when `ui/dist` is absent. Build it once when missing (set
/// `IGDM_SKIP_FRONTEND=1` to bypass, e.g. headless CI).
fn build_frontend_if_missing() {
    if std::env::var_os("IGDM_SKIP_FRONTEND").is_some() {
        return;
    }
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let index = std::path::Path::new(&manifest).join("ui/dist/index.html");
    if index.exists() {
        return;
    }
    eprintln!("[igdm] frontend build missing — running `npm --prefix ui run build`");
    let ok = std::process::Command::new("npm")
        .args(["--prefix", "ui", "run", "build"])
        .current_dir(&manifest)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!(
            "[igdm] WARNING: frontend build failed; cargo run / tauri build will fail \
             on missing web assets (run `npm --prefix crates/igdm/ui install` first)"
        );
    }
}
