# Contributing

Keep pull requests focused and explain the user-visible change. New site-specific extraction logic should normally go upstream to yt-dlp or gallery-dl instead of becoming a second extractor inside this repository.

## Code shape

- Put React state and orchestration in `src/app`; keep feature UI inside its matching `src/features` folder.
- Keep provider argument building in `src-tauri/src/providers`. Process control, fallback order, file validation, and publication belong in `downloader.rs`.
- Keep functions small enough to scan, but do not add one-use wrappers that hide a direct library call.
- Prefer early returns over deep nesting. Name values by purpose and let TypeScript or Rust infer local types when the type is obvious.
- Comments should explain a security boundary, protocol quirk, or reason. Do not narrate the next line.
- Any external response is untrusted. Preserve URL checks, redirect checks, size limits, safe filename rules, and atomic file publication.
- A provider failure must leave no partial output, staging directory, child process, cookie copy, or persisted token.

## Adding a provider

Add a small adapter under `src-tauri/src/providers`, then wire it into the mode-aware chain in `downloader.rs`. State which modes and hosts it handles. Include tests for URL rewriting or argument construction, cancellation cleanup, invalid media responses, and the provider's position in the chain. Third-party network services must be opt-in unless they are endpoints configured by the user.

Before opening a pull request:

```powershell
pnpm install --frozen-lockfile
pnpm test
pnpm test:smoke
pnpm build

Set-Location src-tauri
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

Do not commit media, cookies, API tokens, downloaded executables, certificates, or private URLs. Use a private security advisory for vulnerabilities.

By contributing, you agree that your contribution may be licensed under the MIT License.
