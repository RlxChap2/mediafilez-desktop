# Provider notes

Reviewed 2026-08-23. This note records upstream contracts and the narrow implementation choices they support. It does not claim that every URL on the web can be downloaded: yt-dlp explicitly says listed extractors can break as sites change, unlisted sites may work through generic/embed extraction, and the only reliable check is an actual attempt ([supported-sites notice](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md#supported-sites)).

## Recommended desktop chain

Use a domain- and media-aware chain instead of sending every URL through every engine:

1. Direct media/HTML media discovery, with strict URL, redirect, content-type, size, and file validation.
2. yt-dlp for video/audio and generic embedded media.
3. gallery-dl earlier for galleries, image-heavy URLs, Pinterest, and Instagram collections.
4. Cobalt only through a user-configured, self-hosted, or explicitly permissioned v11 instance.
5. `kkkinstagram` only as an opt-in Instagram-specific experiment after trusted extractors fail.

Each attempt should return a structured result (`success`, `unsupported`, `authentication`, `rate_limited`, `temporary`, `invalid_media`, or `cancelled`). Retry only temporary failures, cool down endpoints after rate limits, and validate the published file before declaring success. Job concurrency and yt-dlp fragment concurrency should remain separate limits.

## Cobalt v11

### Contract

- The official container registry currently identifies the production package family as v11 (11.7.1 at review time) ([official package versions](https://github.com/imputnet/cobalt/pkgs/container/cobalt/versions)).
- The processing endpoint is `POST /` on the configured instance, not the retired v7 `/api/json` route. Every request needs `Accept: application/json` and `Content-Type: application/json`; `url` is the only required body field ([API documentation](https://github.com/imputnet/cobalt/blob/main/docs/api.md#post-)).
- Useful v11 request fields are `downloadMode` (`auto`, `audio`, `mute`), `videoQuality`, `audioFormat`, `audioBitrate`, `filenameStyle`, `disableMetadata`, `alwaysProxy`, and `localProcessing`. YouTube also supports codec/container, HLS, dub-language, and better-audio preferences ([request schema](https://github.com/imputnet/cobalt/blob/main/docs/api.md#api-schema)).
- Authentication, when required by the instance, is `Authorization: Api-Key <token>` or `Authorization: Bearer <token>`. Missing authentication is reported with an `api.auth.<method>.missing` error code ([authentication](https://github.com/imputnet/cobalt/blob/main/docs/api.md#authentication)).
- A response is always JSON with one of five statuses: `tunnel`, `redirect`, `local-processing`, `picker`, or `error`. `tunnel`/`redirect` returns one URL and filename; `picker` returns multiple photo/video/GIF items; `local-processing` returns tunnels plus an operation such as merge, mute, audio, GIF, or remux ([response schema](https://github.com/imputnet/cobalt/blob/main/docs/api.md#response)). Do not collapse `picker` or `local-processing` into “invalid media.”
- `GET /` exposes instance version, canonical URL, start time, supported services, and git metadata, so it is the appropriate capability/health probe. Processing routes are rate limited and expose `RateLimit-*` headers; tunnel responses may provide exact `Content-Length` or a non-authoritative `Estimated-Content-Length` ([instance information and tunnel headers](https://github.com/imputnet/cobalt/blob/main/docs/api.md#get-)).

### Instance policy

Cobalt states that there is no public pre-hosted API and recommends deploying an instance; its API docs also say hosted endpoints such as `api.cobalt.tools` are bot-protected and are not for use by other projects without explicit permission ([API README](https://github.com/imputnet/cobalt/blob/main/api/README.md#accessing-the-api), [API warning](https://github.com/imputnet/cobalt/blob/main/docs/api.md#cobalt-api-documentation)). MediaFilez Desktop uses configured or permissioned endpoints by default. Its legacy community directory runs only after explicit opt-in. For an owned instance, upstream recommends Docker Compose and protection with API keys or Turnstile when public-facing ([self-hosting guide](https://github.com/imputnet/cobalt/blob/main/docs/run-an-instance.md#how-to-run-a-cobalt-instance)).

Until local-processing is fully implemented and tested with FFmpeg, send `localProcessing: "disabled"`. Once implemented, handle every documented operation and validate all tunnel inputs before invoking FFmpeg.

## yt-dlp on Windows

- yt-dlp ships many dedicated extractors plus generic/embed extraction, but upstream makes no universal support guarantee ([supported-sites notice](https://github.com/yt-dlp/yt-dlp/blob/master/supportedsites.md#supported-sites)). Keep “try this URL” as product wording rather than “supports every site.”
- FFmpeg/ffprobe are strongly recommended for merging and post-processing. Full YouTube support also needs `yt-dlp-ejs` and a JavaScript runtime; Deno is upstream's recommended runtime ([dependencies](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#strongly-recommended)). `--js-runtimes` supports Deno, Node, QuickJS, and Bun in that priority order, with only Deno enabled by default ([runtime option](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#general-options)).
- Most official builds include `curl_cffi` impersonation support; the documented exceptions are the Unix zipimport binary and Windows x86 build. TLS fingerprint impersonation may be required by some sites, but yt-dlp warns that forcing it for every request can reduce speed and stability ([impersonation dependency](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#impersonation), [`--impersonate`](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#network-options)). Use a normal attempt first, then retry eligible 403/TLS-fingerprint failures with an available target.
- Cookies can come from a Netscape-format file or directly from Brave, Chrome, Chromium, Edge, Firefox, Opera, Safari, Vivaldi, or Whale using `--cookies-from-browser` ([cookie options](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#filesystem-options), [cookie FAQ](https://github.com/yt-dlp/yt-dlp/wiki/FAQ#how-do-i-pass-cookies-to-yt-dlp)). Browser-cookie failure must remain recoverable: explain that the browser may need closing, then allow a cookie file or another browser/profile. Never copy cookies into logs or persistent job history.
- Retry controls are separate: `--retries`, `--fragment-retries`, `--file-access-retries`, `--extractor-retries`, and type-specific `--retry-sleep`, including bounded exponential backoff ([download retries](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#download-options), [extractor retries](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#extractor-options)). `--concurrent-fragments` accelerates DASH/HLS downloads independently of the app's job queue.
- Official release assets publish SHA-256/SHA-512 manifests and signatures. Upstream recommends the nightly channel for regular users because site changes can make stable stale ([release verification and channels](https://github.com/yt-dlp/yt-dlp/blob/master/README.md#update)). For a desktop release, pin the exact tested executable and checksum; update the managed tool separately and atomically instead of changing it in the middle of a job.

## gallery-dl on Windows

- gallery-dl offers a standalone Windows executable with Python and required packages included, so users do not need to install Python or gallery-dl manually. The upstream README notes a Microsoft Visual C++ x86 runtime requirement ([standalone executable](https://github.com/mikf/gallery-dl/blob/master/README.rst#standalone-executable)). The app should provision the executable into its managed tools directory, verify it, probe `--version`, and surface the runtime prerequisite only if the probe demonstrates it is missing.
- Active development and stable releases have moved to Codeberg. As of this review, the official latest-release API reports `v1.32.9` and publishes `gallery-dl.exe`, `gallery-dl_x86.exe`, `SHA256SUMS`, and detached signatures ([latest release API](https://codeberg.org/api/v1/repos/mikf/gallery-dl/releases/latest), [v1.32.9 checksums](https://codeberg.org/mikf/gallery-dl/releases/download/v1.32.9/SHA256SUMS)). The SHA-256 for the 64-bit `gallery-dl.exe` in that manifest is `a3f7eb5ad0fdb6176dd0044b583ced7e7d918f27221b6f729825d243daff44fe`.
- The official supported-sites table lists Instagram posts, reels, stories, highlights, collections, and profiles, and Pinterest pins, `pin.it` links, boards/sections, searches, profiles, and video pins. Both are marked as cookie-aware ([supported sites](https://github.com/mikf/gallery-dl/blob/master/docs/supportedsites.md)). Pass the same selected browser or cookie file policy used by yt-dlp rather than creating a second authentication model.
- Useful Windows/reliability options include `--windows-filenames`, `--cookies-from-browser`, `-R/--retries`, `--sleep-retries`, and `--sleep-429`. The standalone tool provides `--update-check`, `-U/--update`, and version/channel targeting ([command-line options](https://gdl-org.github.io/docs/options.html)). For the app-managed copy, prefer: query official release metadata, download to a temporary file, compare with the release checksum, probe the binary, then atomically replace the old version. Keep the previous working binary for rollback.

## `kkkinstagram`: verified boundary

There is no published API contract, ownership statement connecting the domain to a repository, uptime policy, or stable response schema that could be verified from a first-party source. The public `kkscript/kk` repository only describes itself as fixing Instagram/TikTok embeds and providing an API; it does not document endpoints or identify `kkkinstagram.com` ([repository](https://github.com/kkscript/kk)).

Empirical checks on 2026-08-23 found two user-agent-dependent paths for the user's [triple-k example](https://www.kkkinstagram.com/reels/DcV3RyRz0sq/). This observation is not an API contract. A normal browser request redirected through `kkclip.com/open/ig/...` to an HTML “Open in App” page containing advertising and analytics. `Discordbot/2.0` received HTTP 302 directly to a signed `scontent.cdninstagram.com` MP4 URL. `HEAD` returned 405, so any probe must use a bounded `GET`. The double-k spelling also produced the direct CDN redirect for the Discord user agent. Other post types, operator identity, privacy, and future availability remain unverified.

If retained, implement it as a disabled-by-default adapter for canonical public Instagram `/reel`, `/reels`, `/p`, or `/tv` paths only. Send no cookies or credentials, permit only HTTPS redirects to a tight Instagram CDN allowlist, cap redirects/body size/time, and require a valid media content type plus file signature. Never market it as an API or general-purpose extractor.

## Vencord conventions worth carrying over

The supplied [Vencord review guide](https://raw.githubusercontent.com/Vendicated/Vencord/c68c4668dc8f3c259689fe21a47ba4728e3693f1/.gemini/styleguide.md) is specific to a TypeScript Discord modification, so only these portable conventions apply:

- Prefer existing and declarative APIs; every listener, timer, subscription, process, and temporary resource needs balanced cleanup.
- Use early returns, flat control flow, `const`, precise types, and natural user-facing errors. Avoid `any`, double casts, empty catches, unexplained constants, and robotic messages.
- Keep one concern per change; remove dead code; justify every dependency; do not introduce one-use wrappers, speculative abstractions, redundant guards, or obvious explanatory comments.
- Use `Map`/`Set` for frequent keyed lookup, `.find()`/`.some()` for single-result queries, and `Promise.all()` only for genuinely independent work.
- In React, effects that allocate persistent resources must return cleanup. In Rust, use the equivalent ownership/RAII boundary and explicit cancellation cleanup.

Do not copy Vencord-specific prohibitions literally. Security invariants, unsafe boundaries, external protocol quirks, and non-obvious Rust ownership constraints still deserve concise comments and structured logging.

## Claims intentionally excluded

No primary-source description of Assyst's or Nekotina's private downloader algorithms was found, so there is nothing reliable to copy. “Scraping sites” without a published contract, permission, or source code should not become automatic fallbacks. A maintained local extractor with verified releases is safer and more diagnosable than an undocumented third-party proxy.
