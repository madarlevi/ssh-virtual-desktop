# Contributing

Use Windows 10/11 x64 with Node.js 24, stable Rust (MSVC target), Visual Studio Desktop development with C++, Windows SDK and WebView2. Run `npm ci` and `npm run tauri dev`. See [README](README.md) for details.

## Changes and pull requests

1. Open an issue for a substantial change; security reports use [SECURITY.md](SECURITY.md).
2. Create a branch from `main`. Keep changes focused and preserve the existing Tauri/Rust/Svelte boundaries.
3. Use TypeScript types for RPC changes. Use async native operations. Rust uses `cargo fmt`; frontend uses two-space indentation. Avoid unrelated reformatting.
4. Add meaningful tests for security boundaries and behavioral changes. Use synthetic credentials and controlled endpoints, never personal sessions or production SSH accounts.
5. Run the commands below and describe results and any untested platform paths in the PR. Include genuine screenshots for visual changes when available, with hostnames and other personal information redacted.
6. Update the Unreleased changelog and documentation. Open a PR using the template. Never include installers, build outputs, local profiles, private keys, session cookies or logs containing secrets.

```powershell
npm run versions:check
npm run workflows:check
npm run check
npm run test:ui
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --no-bundle
```

Connected SSH/SFTP and native browser tests require a disposable Linux test account. Unit tests and compilation do not replace that check. Mark skipped checks explicitly. Contributions are provided under the project's MIT license; retain third-party notices and do not copy incompatible assets.
