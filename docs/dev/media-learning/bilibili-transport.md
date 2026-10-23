# Bilibili Transport

The Bilibili API, subtitle, authentication and MP4 Range clients share
`media::bilibili_transport`. The player and existing `.bilibili` descriptors
remain compatible. No video download or DASH remuxer is introduced.

## Routes And Identity

- `auto`: an available app QR-login session uses native first on every platform so
  ordinary imports/playback do not depend on an external browser staying awake.
  Otherwise a ready, capable browser bridge is preferred, then native. Lookup
  fallback still requires the same account identity on both routes.
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
present protocol/script version 1 / 1.0.1, consume the challenge and receive an
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
The manager's stream is drained into one byte-bounded request buffer while
credit is withheld: not every manager propagates consumer backpressure to its
network reader. The cap therefore remains enforceable without a Rust ACK.
HTTP headers may arrive after the `onloadstart` stream (status zero); neither
success nor an empty response is reported before real headers arrive.

Range responses validate status, Content-Range, total length and exact bytes.
After bytes are delivered, switching a signed URL or CDN requires an equal
strong ETag and total size. Without proof the player must start a new attempt,
restoring by time instead of concatenating potentially different files.
Playback errors retain the app player and buffered state. Retry creates a new
attempt; the official iframe is an explicit, separate external option.

On Android the managed plugin starts a visible `dataSync` foreground service
from the user-initiated pairing action. Requests and a live, approved bridge
connection renew a two-minute lease. An idle but responsive bridge therefore
does not lose its approval between imports. Only the latest pairing watcher can
revoke authorization; a transient plugin error is not treated as a user stop.
The notification can disconnect; stopping the service revokes Rust authorization.
The service is not sticky and cannot guarantee the external browser's tab will
continue executing while backgrounded, locked or reclaimed. Ordinary app
WebViews cannot install desktop userscripts.

Android WebView's intercepted-response loader applies the request Range to the
returned InputStream even when the app has already returned a 206 body. Wry's
ByteArrayInputStream therefore lost the offset twice: a 288 MB live video stalled
at 512 KiB while repeating the same Range. The managed `BilibiliRangeResponse`
adapter exposes a virtual prefix for WebView's range validation and consumes its
initial skip without discarding the bounded Rust body. Virtual skips are capped
at the JNI int limit. It allocates no prefix and affects only `bilistream` GET
206 responses. A Gradle preparation task applies the adapter after Wry generates
its Kotlin client and before Kotlin snapshots compiler inputs. Unknown Wry
templates fail the build instead of silently dropping the fix.

The redirect capability probe accepts standard redirect statuses, including the
301 normalization used by some managers, but requires the original final URL and
the exact expected local Location target. Probe failures remain visible after
authorization is revoked. Playback diagnostics omit source URLs, credentials and
decoder messages; background time is excluded from the frontend load deadline.

Reselecting a requested quality after Bilibili downgraded it creates a new
playback attempt instead of reusing the downgraded cache entry. Playback restores
the position and play/pause state. When the HTML5 MP4 response advertises the
requested quality but returns a lower one, a standard MP4 query is compared on
the same pinned route/account before caching. Only a higher-quality, single MP4
response replaces the source. This is not an entitlement bypass or DASH support.
Resolution diagnostics log requested/actual quality, route and login presence,
never the signed source URL or credentials.

On the logged-in Android device tested on 2026-10-23, both MP4 queries returned
quality 64 (720P) even though 80 appeared in the advertised options. Reselecting
1080P now issues a fresh request and reports the downgrade. Android acceptance
for this change is 720P; 1080P playback is not claimed. DASH remains outside this
change.

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

Linux WebKitGTK hands playback to GStreamer, which cannot open `bilistream://`.
The quality command returns a loopback HTTP URL on Linux. The server is bound to
`127.0.0.1`, validates the Host, app Origin and playback-session capability, and
reuses the pinned transport and CDN representation through the existing protocol
handler. Bounded CDN reads form one continuous HTTP body for the full requested
Range; exposing a single short chunk as the whole HTTP response truncates playback
in GStreamer. Android and other platforms retain the custom protocol.
DEB dependencies include the base, good and bad GStreamer plugin sets plus libav
for MP4 demuxing, H.264 parsing, AAC/H.264 decoding and WebVTT handling.

These automated checks do not establish real Tampermonkey/Violentmonkey support
on every browser. Release acceptance must record a real manager/browser version,
an independently logged-in browser account, real CDN playback and seeking,
logout/account switch, suspend/reconnect, and Android lock/background behavior.
Do not claim untested platforms or replace this with mocked GM callbacks.

An isolated Chromium 148.0.7778.96 / official Tampermonkey 5.4.1 test exercised
actual script installation, loopback handshake, manual redirects, positive
cookie control/anonymous isolation, Range/Referer, early oversized-response
abort, and real anonymous Bilibili playurl plus a 64 KiB CDN Range. It found the
OPENED/status-zero callback issue corrected in script 1.0.1. This is not a
logged-in-account, video-element playback, Android or Violentmonkey acceptance
claim. Manager permissions and page injection must be enabled by the user.

On 2026-10-23, the vivo Android 16 device reproduced the 288 MB video stall with
the delivered native library. The Range adapter allowed actual 720P video-element
playback and a seek to 60 seconds using the same library and network. Independent
WebView byte checks passed at offsets 3,000,000 and 3,000,000,000 with exact
65,536-byte payloads. These tests used an isolated `.bridgecheck` package, not
the user's data. They do not establish Android Edge/Tampermonkey background
reliability or logged-in-account playback acceptance.

Design reference: wangke-agent at
`03a2a6e8adfcceb9e736028ea05a7bd016f85bdc`; implementation here is independent,
using the existing Rust/React stack and Prost protocol types.
