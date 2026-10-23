# DeepSeek Import And Android Release Build

Date: 2026-10-23. Working-tree baseline: v0.10.7 / aabf0c1c.

## Android Completeness Correction

The 110349572-byte APK described below was a clean **mobile-slim** build, not
a full-feature replacement for the user's reference APK. The reference
`DeepStudent-v0.10.7-arm64-dev.apk` is 158391725 bytes and its native library
contains LanceDB, AWS S3 and exact-tokenizer implementations. The earlier
claim that the smaller build preserved all functionality was incorrect:
`mobile-slim` explicitly excludes `lance`, `mcp`, `tokenizer_tiktoken` and
`cloud_storage_s3` from the default feature set. A full build must enable all
default features and verify the final application Cargo fingerprint; a fresh
output directory alone does not establish feature completeness.

The reference APK certificate is
`06a30b4d98aac396f9bb0d14ba0bd130d30ddd6b88d391dca11157a37ee533fb`,
different from the available local development certificate
`88a25dd78b2ac888e31cf287f77b11ac921d2a5d3ba46d942645578e69674ce9`.
Matching package name and versionCode do not permit in-place updates across
these certificates. The reference signing private key was not found locally;
do not recommend uninstalling an existing installation to bypass this check.

The following Android sections document the superseded slim build, not the
requested full-feature replacement. Full-rebuild evidence is recorded under
the local delivery's `android-full-20261023/verification/` directory. Local
packages and verification records are not part of the source submission.

### Full-Rebuild Result

The replacement is `DeepStudent-v0.10.7-arm64-full-dev.apk`, 172150020 bytes
(172.15 MB / 164.18 MiB), SHA-256
`0d2bcbcd4a689342df2bacccf26308f5939a2034a4220be15ae1f3d4cfee53f7`.
The application library is 164317240 bytes. Locked npm dependencies, frontend,
all native dependencies/application and the Android project were rebuilt from
fresh output directories, with all default features enabled. Actual compiler
options and final Cargo fingerprints verify `lance`, `mcp`,
`tokenizer_tiktoken` and `cloud_storage_s3`; `mobile-slim` is absent.

Binary comparison verifies the Lance hybrid-query, AWS S3, exact-tokenizer,
new question-authoring runner and JNI implementations. The packaged native
library matches this compilation's SHA-256. All 23 reference assets, reference
permissions, native-library names, six managed Android components and PDFium
are present. The three reference-only `res/*.xml` files are unused generated
template icon/animation resources, not missing application assets.

Final checks pass for APK v2/v3 signatures, ARM64/API24+, JNI entry point,
FileProvider/keyboard configuration, uncompressed native libraries, 16 KiB ELF
LOAD and ZIP alignment, and native dependency closure against Android platform
libraries. Frontend type checking, six Android build guard/toolchain tests and
license compliance pass. No connected Android device was available, so this
does not establish real-device startup or end-to-end Android behavior.

The new development-signed package still cannot update the differently signed
reference installation. An unsigned copy is retained for the original signing
key; no application data was modified or removed.

## DeepSeek Failure

The desktop frontend log records `INVALID_JSON: expected value at line 1
column 1` at 04:15:37 (Asia/Shanghai). The corresponding persisted failure
evidence identifies `tree-deepseek-v4.1-flash` and begins with a Markdown report
and numbered question list instead of a JSON object. The model recognized the
document; this was not evidence of missing images, a database commit failure,
or an output-length failure. A separate relay model's successful import does
not establish compatibility of every DeepSeek relay.

The dedicated request previously put its detailed output contract only in
system, with natural-language source evidence and page images following it.
It now repeats the authoritative task/output contract in the last user text
part, after all original evidence. The extraction contract includes a complete
valid non-empty JSON example. This follows DeepSeek's documented recommendation
to explicitly mention JSON and provide a concrete output example.

No repair model, Markdown-to-question conversion, output-token reduction,
thinking-mode override, or automatic fallback to another model was introduced.
`json_object` remains enabled. Only an explicit unsupported-parameter error
permits the existing same-input retry without it. Format failures additionally
retain model, JSON mode, finish reason, usage and reasoning byte count.

## Verification

- Seven dedicated request tests pass, including exact model-adapter parameter
  preservation, contract placement after visual input, 1/19/20-page request
  counts, unchanged compatibility-retry input and failure classification.
- The source-runner regression passes 15 cases; its paid live fixture remains
  ignored by default. The qbank regression passes 135 cases.
- Actual configured DeepSeek relay: a synthetic question returned valid JSON.
- The same original eight-page PDF returned 38 questions and complete coverage
  in an explicit direct request.
- An opt-in Rust test then exercised the application's actual streamed request,
  source/coverage/document validators, draft service and atomic commit. It
  returned 38 questions from all eight pages, `finish_reason=stop`, and committed
  successfully to a temporary migrated VFS database in about 110 seconds.

Live source/settings databases were read-only. Credentials stayed in process
memory and were not written to the repository or delivery logs. The live test
did not modify the user's question bank.

## Android Findings

The previous local package used the fast recovery settings (`opt-level=0`,
256 application codegen units) and contained a roughly 546 MB native library.
The repository's normal release settings are `opt-level=z`, 16 codegen units,
LTO disabled and debug information disabled, with a 250 MiB native-library
guard. The old native library also has 4 KiB ELF LOAD alignment. ZIP alignment
alone does not make that library compatible with a 16 KiB page-size device.

The new build regenerates the Android project and uses a fresh, initially empty
Cargo target directory. All frontend assets, Rust dependencies/application and
Gradle output are built again. Managed Activity, SAF/HEIC/system-bars/installer/
external-file plugins, notification/audio/install permissions, FileProvider,
icons and PDFium are synchronized using the repository release workflow steps.

Toolchain matches `.github/workflows/reusable-build-android.yml`:

- Rust 1.98.1, Android ARM64 target.
- Java 17 (Temurin 17.0.21).
- NDK `29.0.13846066` and API-24 clang wrapper.
- Android build tools 36.0.0 / platform 36.
- `mobile-slim` normal Android release features; no recovery profile overrides.
- Normal Cargo release profile: `z`, codegen units 16, debug 0, LTO false.

The Gradle prebuilt guard now follows `CARGO_TARGET_DIR`, retaining canonical
JNI-path, ARM64 and mobile-entry-point validation. This reuses the freshly built
native library in packaging, not any previous package's library. A regression
test covers the isolated target path.

Final package checks include APK v2/v3 signatures, stable version code, managed
components/permissions/PDFium, native library size, ELF LOAD alignment, and
`zipalign -P 16`. This is a local development-certificate build, not an
officially signed/published release. Final sizes and hashes are recorded with
the delivered packages; approximate size is not a substitute for these checks.

## References

Fetched and read during this task:

- https://api-docs.deepseek.com/guides/json_mode
- https://api-docs.deepseek.com/guides/thinking_mode
- https://v2.tauri.app/distribute/google-play/
- https://v2.tauri.app/distribute/sign/android/
- https://v2.tauri.app/concept/size/

Repository release procedure and prior build report:

- `.github/workflows/reusable-build-android.yml`
- `docs/dev/optimization0824/progress/R3-WI-6-android-ci.md`

The generic `/distribute/android/` Tauri URL returned 404; the current Google
Play/APK page above was used instead. Android developer page-size documentation
fetches timed out; compatibility is checked against the actual ELF and APK
artifacts, not claimed as a successful fetch of that page.
