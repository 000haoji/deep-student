# Bilibili Transport

The Bilibili API, subtitle, authentication and MP4 Range clients share
`media::bilibili_transport`. The player and existing `.bilibili` descriptors
remain compatible. No video download or DASH remuxer is introduced.

## Routes And Identity

- `auto`: a ready, capable browser bridge first, otherwise native.
- `browser_bridge`, `native`, `proxy`: fixed routes, without silent fallback.
- An automatic lookup may restart once on another compatible route after a
  transport timeout, network failure or bridge loss. Authentication, rate-limit
  and protocol errors do not trigger route roulette. Authenticated attempts
  cannot silently become anonymous or switch account.
- Proxy is an explicitly configured HTTP(S) forward proxy. The native account's
  cookies are never forwarded on this route. Proxy participation in automatic
  fallback is opt-in.
- App QR login always manages the separate native account. Browser cookies stay
  in the userscript manager. Disconnecting the bridge does not log the browser
  out of Bilibili.
- Backend route/auth generations isolate metadata and playback sessions. A
  playback URL never selects a new route when its pinned session becomes stale.

## Browser Pairing

The app starts a separate server on a random `127.0.0.1` port. It serves the
bundled page and userscript at `/deepstudent-bili-bridge/v1/`. Only this path is
matched by the script; it exports neither a generic fetch API nor cookies.

The one-use challenge is carried in the fragment and removed immediately by
the script. The WebSocket requires the exact local Host/Origin. A script must
present protocol/script version 1 / 1.0.0, consume the challenge and receive an
explicit app-side approval before executing requests. A short-lived in-memory
resume grant is rotated on reconnect; revocation invalidates it. Refreshing the
page loses sandbox memory and requires fresh pairing. Account verification is
required before ready, around import attempts, and periodically while paired.

Parsing capability requires controlled redirects, account verification and
credential isolation probes. Media additionally requires streamed Range reads,
the required headers, successful anonymous requests, and early cancellation of
ignored Range and oversized chunked responses. Managers that cannot prove the
media requirements are not advertised as media-capable. A successful socket or
an iframe load is not evidence of media playback.

## Bounds And Recovery

Requests have an absolute deadline and a cancellation token. Redirects are
validated on every hop, including relative redirects and the restricted PCDN
4483 port. Native DNS excludes internal addresses; the explicitly trusted proxy
and browser manager own their network/DNS stack.

Bridge response bytes use binary frames, not base64 video in JSON. Frames are
at most 64 KiB and every head/chunk waits for Rust credit. At most four requests
and two media reads run concurrently. Subtitle bodies are capped at 20 MiB;
API/gRPC bodies and decompressed protobuf messages at 2 MiB. Media requests are
512 KiB, initially 64 KiB. The outer playback deadline is 24 seconds. The script
has an independent watchdog and aborts its GM handle on cancellation or loss.

Range responses validate status, Content-Range, total length and exact bytes.
After bytes are delivered, switching a signed URL or CDN requires an equal
strong ETag and total size. Without proof the player must start a new attempt,
restoring by time instead of concatenating potentially different files.
Playback errors retain the app player and buffered state. Retry creates a new
attempt; the official iframe is an explicit, separate external option.

On Android the managed plugin starts a visible `dataSync` foreground service
from the user-initiated pairing action. Requests renew a two-minute lease. The
notification can disconnect; stopping the service revokes Rust authorization.
The service is not sticky and cannot guarantee the external browser's tab will
continue executing while backgrounded, locked or reclaimed. Ordinary app
WebViews cannot install desktop userscripts.

## Import Semantics

Video metadata remains available if subtitles fail. Subtitle states distinguish
available, confirmed empty, login required, failed and unsupported. Unknown
JSON/null is not confirmed absence. Incompatible web responses can try typed
gRPC on the same route; gzip and framing are bounded. Expired subtitle URLs are
refreshed once. New file registration, subtitles and extracted text share one
transaction. Duplicate imports preserve existing subtitles/progress; explicit
refetch is the replacement operation. Cancel reaches the current upstream
request, and failed/remaining batch parts are reported separately.

## Local Verification

```sh
npm run typecheck:native
npm run lint
CI=true npx vitest run --maxWorkers=2
node --test scripts/__tests__/*.test.mjs
npx playwright test -c playwright-ct.config.ts tests/ct/media --workers=2
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked \
  --no-default-features --features mobile-slim,tokenizer_tiktoken media::
npm run licenses:check
NODE_OPTIONS=--max-old-space-size=6144 npm run build
```

The lighter feature set above is for native tests only. Delivered Android and
deb artifacts must enable all default application features. The Android project
sync must include `scripts/ci/prepare-bilibili-android.mjs`, both Kotlin classes,
foreground-service permissions and the non-exported manifest service. Verify
actual packaged classes, Cargo fingerprints, signatures and 16 KiB alignment.

These automated checks do not establish real Tampermonkey/Violentmonkey support
on every browser. Release acceptance must record a real manager/browser version,
an independently logged-in browser account, real CDN playback and seeking,
logout/account switch, suspend/reconnect, and Android lock/background behavior.
Do not claim untested platforms or replace this with mocked GM callbacks.

Design reference: wangke-agent at
`03a2a6e8adfcceb9e736028ea05a7bd016f85bdc`; implementation here is independent,
using the existing Rust/React stack and Prost protocol types.
