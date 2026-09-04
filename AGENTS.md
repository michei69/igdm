# Project Practices

## Testing (MANDATORY)

- NEVER write, run, or add tests — no unit tests, no e2e, no Playwright, no vitest, no new test files or test dependencies.
- NEVER run `cargo test`, `vitest`, `playwright test`, or any test command.
- This project has no test suite on purpose; test files were removed and must stay removed.
- After any change, ask the user to test manually. Do not verify behavior yourself with test tooling.
- Do not reintroduce or restore deleted test files or test configuration.
