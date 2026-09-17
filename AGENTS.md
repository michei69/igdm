# Project Practices

## Testing (MANDATORY)

- NEVER write, run, or add tests — no unit tests, no e2e, no Playwright, no vitest, no new test files or test dependencies.
- NEVER run `cargo test`, `vitest`, `playwright test`, or any test command.
- This project has no test suite on purpose; test files were removed and must stay removed.
- After any change, ask the user to test manually. Do not verify behavior yourself with test tooling.
- Do not reintroduce or restore deleted test files or test configuration.

## Build & verify (instead of tests)

Toolchain: [bun](https://bun.sh) for the frontend; stable Rust + rustfmt + clippy for the workspace.

Rust (workspace root):

- `cargo clippy --workspace --all-targets`
- `cargo fmt --all` (check only: `cargo fmt --all -- --check`)
- Backend-only check without a frontend build: `IGDM_SKIP_FRONTEND=1 cargo check` — otherwise `crates/igdm/build.rs` runs `bun --cwd ui run build` when `ui/dist` is missing

Frontend (`crates/igdm/ui`):

- `bun install`
- `bun run typecheck` — `tsc --noEmit`
- `bun run lint` — oxlint with the shared config plus the custom anti-slop rules in `tools/oxlint/anti-slop/`
- `bun run format:check` — oxfmt (`bun run format` to fix)
- `bun run doctor` — react-doctor scan (keep the score at 100)
- `bun run build` — `tsc --noEmit && vite build` (writes `ui/dist`, which `build.rs` embeds)

Before calling a change done: run the checks above and report their results.
