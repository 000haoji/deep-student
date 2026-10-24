# Question Authoring v2 Implementation

Date: 2026-10-23. Baseline: v0.10.7 / aabf0c1c.
Status: core implementation and local package verification complete; acceptance boundaries below.

## Contract

- New extraction uses complete ordered source evidence and a root JSON object.
- The selected multimodal model reads original images directly. A configured
  visual model is used only to adapt visual evidence for a text-only main model.
- `json_object` is enabled for dedicated question requests. Explicit unsupported
  parameter errors permit one same-input retry without that parameter. Ordinary
  chat/tool calling is not forced into JSON mode.
- Output budgets come from effective model configuration. No import-specific
  token or text-prefix clipping is applied to new requests. Real provider limits
  surface as errors; missing pages are never silently accepted.
- Source documents, including separate answer files, share one ordered bundle.
  Documents of 1-20 pages use one main request. Larger bundles are also sent
  whole unless the provider refuses them; there is no hidden summary pipeline.
- Parsing creates a persisted draft. Only selected, validated question identities
  are written to formal tables in the confirmation transaction.
- Shared reading passages and common conditions are material/group entities;
  each child question retains independent practice, grading and history identity.
- `parent_id` remains the variant relationship, never group membership.

## Compatibility

Existing question IDs and study history must be preserved. Issue 487 context
recovery remains compatible. Legacy checkpoints are version-routed;
new jobs do not enter the retired visual-first structuring path.

Dedicated request errors retain incomplete/invalid responses for diagnosis, not
as successful imports. A resumable source checkpoint records original source
evidence and a non-secret model configuration fingerprint. Changed model settings
require a new attempt rather than silently altering a resumed task.

## Verification Log

Final focused verification is described in the follow-up reports.
Mock provider tests do not establish real OCR/model quality. Real-device Android
verification and external model requests are separate from build verification.

Official OpenAI documentation fetches were blocked by the current network's
access protection. Provider compatibility is therefore being checked against
the existing adapters and local mocked protocol tests, not claimed as an online
documentation verification.

Completed checks so far: frontend native TypeScript check and production bundle;
126 frontend import/material/citation/entry regression tests; 32 practice/export
tests (some overlap); material mapping/chat regression tests (overlap);
Playwright component cases at 390px and 1280px; all 70 VFS SQL migrations in
sequence; Rust mocked model/source tests (9), authoring transactions (4), portable
export (4), generation (23), grading (35), 487 compaction (45, 1 ignored),
migration registry (12). Final runs supersede early failed
fixtures; no failed result is counted as passing.

Final builds use full desktop and ARM64 Android default features. Earlier fast
and `mobile-slim` packages were superseded: the slim profile omits LanceDB, MCP,
exact token counting and S3. See `deepseek-import-and-android-release-20261023.md`
for the full-feature rebuild and verification boundaries. Android is signed
with a local development key, not the official release certificate.

## Verification Boundaries

Local packages and diagnostic logs are not committed. Superseded packages were
removed after preserving the latest APK/deb and historical verification records.

Final targeted Rust runs: qbank suite 99 passing (includes authoring, generation,
grading, source adapters and tool contracts); ordered source/model runner 9;
question repository 11; portable/CSV export 4; exam repository 3; material/source
blob references 1; embedded ID aliases 1; migration registry 12. These are focused
suites, not all 6500+ library tests. All earlier failed new fixtures were corrected
and rerun; the missing migration setup was a test-fixture issue, not ignored.

The final desktop executable starts with a temporary XDG data/config/cache root,
renders the first-run agreement, and migrates VFS to 20261023. No user agreement
was accepted and no real user data was modified. An earlier Xvfb/portal startup
reported the eight-second empty-root watchdog; the extended isolated run rendered
successfully. Android ABI, permissions, PDFium inclusion, ZIP alignment and APK
v2/v3 development signatures were verified. Physical-device installation remains
unverified. A later opt-in test verifies one real DeepSeek provider/document
combination; it does not establish quality for every model and source format.

## Acceptance Boundaries

Update 2026-10-23 after local user testing: the administrative surfaces and
historical/range recovery gaps below have now been implemented in the follow-up.
See `question-import-test-fixes-20261023.md` for the fixes and final verification.
The old list is retained as the first package's boundary, not the current status.

The core model-route, full-evidence, draft/commit and shared-material workflows
are implemented. The broader design document is not a claim that every proposed
administrative surface has shipped:

- Materials are editable with OCC and group practice is scoped to its selected
  members. A general-purpose UI for group membership reordering, ungrouping and
  deleting groups/materials is not provided. Backend membership updates preserve
  dependencies; removing children with unresolved material dependencies fails
  explicitly rather than dropping required conditions.
- Existing question IDs/history are retained. There is no automatic semantic
  migration of historical preview-only cards into guessed formal groups and no
  interactive semantic diff/reconciliation wizard for ambiguous source reparses.
  Ambiguous reparses fail without overwriting existing content.
- More than 20 pages are sent whole when supported. Automatic lossless chapter
  batching is not implemented; provider limits require a suitable model or
  deliberate source splitting instead of a hidden summary pipeline.
- Sources/provenance are retained and question/group citations navigate to stable
  entities. A dedicated source-page/bbox viewer handoff and group/material editing
  chat tools remain outside the implemented UI/tool surface.
- Existing structured CSV import remains separate. Shared groups are not silently
  flattened into a purported lossless CSV backup; JSON/ZIP is the portable path.
- No paid/external model requests, real-device Android import/playback run, cloud
  two-device sync test or full integration-test suite was performed.

Current remaining verification boundaries are real-model quality, Android device
workflows, and two-device cloud synchronization. Large-source processing uses an
explicit full-evidence range picker after actual provider limits; no automatic
semantic inference is claimed. Ambiguous/grouped reparses expose only the
implemented explicit actions and never guess a replacement relationship.
