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
    let ui = std::path::Path::new(&manifest).join("ui");
    if ui.join("dist/index.html").exists() {
        return;
    }
    // The project is bun-only (bun.lock, bun scripts). Run bun *inside* ui/
    // rather than passing `--cwd`: bun 1.4 rejects `bun --cwd <dir> run
    // <script>` (it prints the `bun run` usage instead of running anything),
    // only the `--cwd=<dir>` form is accepted.
    eprintln!("[igdm] frontend build missing — running `bun run build` in crates/igdm/ui");
    let ok = std::process::Command::new("bun")
        .args(["run", "build"])
        .current_dir(&ui)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        eprintln!(
            "[igdm] WARNING: frontend build failed; cargo run / tauri build will fail \
             on missing web assets (run `bun install --cwd crates/igdm/ui` first)"
        );
    }
}
