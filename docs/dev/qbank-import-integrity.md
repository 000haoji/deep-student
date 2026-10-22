# Question-bank document import integrity

## Scope and authorization

The user requested diagnosis of incomplete document imports, answer sections appearing as questions, and inconsistent question counts, then authorized a full implementation of the documented improvement plan. This document describes the implemented contracts and their explicit boundaries.

This change concerns future imports and safe replay of versioned import checkpoints. It does not migrate, rewrite, or delete existing user questions, answers, attempts, notes, or flashcards. Historical banks without stable identities require a separately authorized, backed-up migration; text similarity is never used as a deletion identity. User-excluded source identities and soft-deleted questions are never resurrected by a replay.

All regression work uses synthetic fixtures, temporary directories and migrated test databases. No user database, real document, installed application, or device was used for the implementation or validation described here.

## Checkpoints, attempts and resume

### Versioned checkpoint (schema 2)

`ImportCheckpointState` now persists an explicit contract:

- `schema_version` (current `2`), `pipeline_version`, `parser_version`, `prompt_version`, `structurer_version`;
- `input_fingerprint`, derived from the persisted source document identity, page/blob hashes and extracted text;
- `attempt_id` and a monotonic `revision` used by the checkpoint writer.

Compatibility rules on resume:

- A checkpoint with a newer `schema_version` than the running build is rejected; it is never reset silently.
- A legacy checkpoint (`schema_version` 0) that already contains cached page/batch results is rejected because its typed interpretation predates the current contract.
- With cached results present, any stage version that differs from the current build fails closed with the stage name and both versions in the error. This includes a schema-2 checkpoint whose stage versions are still `0`.
- A stored non-empty `input_fingerprint` is recomputed from the persisted input fields; a mismatch rejects reuse instead of resuming against different input.
- Only a checkpoint without cached results is stamped with the current stage versions during migration.

### Session ownership and append

- A caller-supplied `session_id` is an append request. The session must exist, must not be soft-deleted, must have an allowed status (`importing`, `parsing`, `needs_review`, `partial`, `failed`, `completed`, `excluded`), and must not carry an unfinished import checkpoint. Appending cannot overwrite a resumable attempt.
- Resume requires a resumable status (`importing`, `needs_review`, `partial`, `failed`) plus a non-empty checkpoint. Bank-level `import_mode` is not used to restrict append document formats; a mode mismatch is only considered for an explicit resume request that declares its mode.
- One in-process lease per session prevents a resume and an append from interleaving. A competing attempt receives a clear "already running" error instead of racing.
- The resume entry point compares the checkpoint it read with the stored JSON before writing the migrated attempt (`UPDATE ... WHERE import_state_json IS <read value>`). A stale writer is rejected. All checkpoint writes refuse missing or soft-deleted sessions.
- Cancelling a resume only stops that attempt; published questions and the checkpoint remain intact.

### Stage-aware resume

Reported recovery actions are executable only when the persisted checkpoint can actually re-run that stage:

- `reidentify_page`: pages with no cached VLM result are re-analysed. Cached pages whose blocks still carry an unresolved `continues_from_previous`/`continues_to_next` marker are invalidated as well, together with the necessary adjacent page(s). Invalidating pages also clears downstream structuring caches.
- `retry_batches`: visual structuring validates each cached batch independently; sentinel/failed batches are re-requested, including non-contiguous failures. Text/DOCX staging reuses successful slots by input fingerprint and retries cleared slots.
- `retry_figures`: figure extraction re-runs from the cached page analyses on resume.
- Review-only findings (ambiguous identity/answer evidence, unresolved continuation without page evidence, empty-result review, incompatible checkpoint) do not produce a resume action. `resume_available` is true only when at least one executable stage action exists and the import is not `completed`/`excluded`.

A checkpoint that cannot execute the reported blocking action produces `reimport_or_review_invalid_checkpoint` and no resume button.

## Import contracts

### Typed source blocks and evidence

Extraction produces `question`, `answer`, `explanation`, `heading`, `material`, `figure`, or `unknown` blocks. Only validated question blocks can become practice questions. Unmatched or ambiguous answers remain auditable and are never converted into question stems.

Answer association uses a unique original label within its section. A bare label is only used when it uniquely identifies a question. Counts, array order, or similar stems do not establish ownership. Math punctuation, options, and repeated questions in different sections are not removed by a fuzzy deduplicator.

Source identities and source spans are carried through extraction, cross-page merging, structuring, and publication. Structuring responses must map one-to-one to supplied source IDs. Truncated or interrupted model responses are failures, not completed batches. Blocks with unresolved continuation markers are deferred rather than published as truncated questions.

Non-visual evidence must be grounded in supplied source text or an explicitly mapped DOCX image, never in the model's own reconstructed `raw_text`. An answer inferred by a model is not an extracted answer.

### Answer tables and explicit multi-label answers

The answer-merge module records answer-table cell evidence and layout diagnostics. Labels such as `4、5` are expanded into an explicit label set, but a mapping is only applied when the source states it; a shared value is never copied across questions by count or position. Layouts the parser cannot support remain review items instead of being silently guessed.

### Options

Any non-empty `options` payload is structurally validated, regardless of the declared question type. Keys must be non-empty, alphanumeric and unique (case-insensitively); contents must be non-empty and unique after representation normalization. A non-choice payload can keep valid options; malformed options quarantine the candidate.

Unknown model question-type enums are not silently coerced to `other`; they quarantine the candidate with `unknown_question_type_enum`. A legitimate `other` value remains valid.

### Representation normalization and three-way edit protection

`question_import_service::normalize` provides a versioned, whitelist-only normalization key (`REPRESENTATION_NORMALIZATION_VERSION = 1`): Unicode NFKC, full-/half-width punctuation, whitespace, LaTeX math delimiters, `^`/`_` grouping and a small set of display-only commands. It never claims general algebraic equivalence; charges, exponents, units and coefficient facts remain significant.

Re-import compares three values per field: the current database value, the last successful import baseline, and this proposal.

- The baseline is stored per question in `source_ref.import_baseline` with the normalization version.
- With a usable baseline: an unchanged user edit is preserved; a representation-only change is not treated as an edit; a real conflict enters review with `baseline_conflict`.
- Without a baseline, a non-empty current value enters review with `legacy_record_without_baseline`; the code does not invent a baseline from similarity.
- A pure replay with a usable baseline and no field write does not modify `source_ref`, keeping previews and projections idempotent.
- A version mismatch of the stored baseline makes it unusable (conservative branch).

## Publication, report and indexing

### Staging and atomic publication

Text and DOCX extraction results are staged in `ImportCheckpointState.candidate_results`; visual page analyses and structured batches are staged per page/batch. Model callbacks never publish questions directly.

The publication boundary uses one immediate SQLite transaction for:

- question creation/updates, preview projection, question-bank statistics and resource data;
- the persisted import report and checkpoint finalization;
- group, material and child-question publication (below);
- index task enqueueing (below).

Stable card identity derives from the session, document identity and source identity. Replay preserves user edits, answers, notes, attempt history and soft-deletion tombstones.

### Groups, materials and children

Materials are no longer discarded as non-questions. The pipeline captures typed blocks before answer merging and assembles groups from evidence-backed hints:

- A `material`/`heading`/`figure` block may propose a group only when it is itself source-grounded, has a non-empty section, and lists explicit, bounded, unique sub-question labels. Invalid hints are removed, recorded as `group_hint_rejected`, and require review.
- Visual imports take hints from the VLM blocks captured after figure extraction and before answer merging. Text and DOCX imports take them from the normalized candidate JSON and rebuild the same typed blocks before publication, so the checkpoint keeps the evidence across resume.
- Group, materials and children are published inside the caller's publication transaction. Group children are excluded from the standalone path, so a child is never published twice; a blocked group never publishes a partial set.
- Assembly requires each label to match a unique real question block in the same section. It does not pick the nearest article, use counts, or infer relations from output order. Ambiguity is reported and blocks the group.

The group tables and repository publication are owned by the question-group implementation. This import path only consumes that contract and keeps standalone imports compatible.

### Import report

`metadata.import_report` is an additively extended, versioned report. New and current fields include:

- `status`: `completed`, `partial`, `needs_review`, `failed`, or `excluded`;
- `empty_import_reason`: `no_questions`, `no_publishable_questions`, or `all_sources_excluded_by_user`;
- counts: `published_count`, `publishable_count`, `inserted_count`, `failed_count`, `incomplete_batch_count`, `excluded_by_user_count`, figure/deferred counts;
- `recovery_actions`: stage-specific actions with `action`, `reason`, `severity`, and the affected `pages`/`batch_ids`/`source_ids`;
- `completion`: derived status, `blocking_count`, `warning_count`, and per-category counts;
- `groups` and `group_publish`: group counts, conflicts, unconsumed materials and publish outcomes;
- checkpoint contract versions and the input fingerprint.

A zero-question publication is never `completed`. Truly empty input, input containing only non-question blocks, and a user's explicit exclusion of every source have distinct terminal states. The `excluded` status is a user decision, not an import failure. Rule-recognised page furniture and explicit user exclusions are warnings; failed pages/batches, unresolved continuations, figure failures, identity/validation findings and conflicts are blocking.

### Diagnostics and redaction

The persisted report contains locations, counts, reasons and hashes, not duplicated user material. `raw_text`, field values, evidence quotes, model JSON and conflict values are replaced by SHA-256 plus character counts. Model-provided `source_metadata` is reduced to an allowlist (spans, verification flags, review flags). Original material remains in the protected source/`source_ref` store.

## Original source images

`qbank_get_source_images` accepts optional `pageIndex` and `metadataOnly` arguments in addition to `examId`.

- No new arguments: preserve the existing full-image response contract.
- `metadataOnly: true`: return the source image directory with empty `dataUrl` values and no image reads/base64 encoding.
- `pageIndex`: read only the selected global preview page.

Responses retain camel-case `blobHash`, `dataUrl`, and `pageIndex`. Preview page indices are global across appended documents; legacy `metadata.source_image_hashes` is only a fallback when the preview has no source blobs. The image dialog prefers `source_ref.preview_page_indices`, loads one page on demand, and releases image URLs when closed.

## Question exclusions

The completion screen reads all formal questions with pagination. Excluding cards resolves exact shared card IDs, soft-deletes those formal questions, and updates preview, resource data and statistics in one transaction. Legacy banks whose preview and question identities disagree are rejected with `QBANK_IMPORT_REVIEW_REQUIRED`; no text-based guessing or partial deletion is attempted. Exclusion cannot be combined with create/edit operations in one request.

## Indexing

Publication and indexing are separate states. The publisher enqueues the question IDs it actually created or changed into the persistent index ledger (`vfs_index_units`, one unit per question) inside the publication transaction. A pure replay that changes nothing enqueues nothing. After commit, the index worker claims the pending units; a crashed or failed worker is recovered by the ledger's own stale-claim/backoff logic. Index retry never republishes questions.

## Resource limits

Imports share a single process-wide memory permit. Image input is limited to a 200 MiB batch and 64 million pixels per image, with invalid dimensions rejected before decoding. Frontend image reads are sequential. Figure extraction retains at most three page images in its cache. These are capacity safeguards, not claims of measured end-to-end memory performance.

## Validation status and remaining limits

Regression fixtures are synthetic and use temporary migrated databases. They cover: scoped question/answer association; replay preservation and tombstone identity; three-way edit baselines including `Ca²⁺`/`$Ca^{2+}$`, different charges, user edits and legacy records; checkpoint schema/stage-version and fingerprint rejection; per-session lease and CAS rejection; empty/all-excluded terminal states; report redaction/severity/recovery actions; index enqueue on change and no-op replay; and text/DOCX/visual group assembly with atomic publication.

The final Rust/Vitest results for a given commit are recorded in the corresponding delivery record. This document does not assert that any suite is green. Application builds, installed-application GUI checks, real-document acceptance and Android device tests were not performed for this documentation update.

## Known limitations

These are confirmed current limits, not claims that they are acceptable.

- **Answer layouts are deliberately narrow.** Only layouts the answer-merge module can align to an explicit label/table cell are associated. Other table shapes, cross-section label reuse, or ambiguous label sets remain in the report as review items; there is no general document-layout solver.
- **Representation equivalence is whitelist-only.** The normalizer does not decide algebraic equivalence, unit conversion, or arbitrary formula equivalence. Uncovered spellings and expression conflicts require human review.
- **Group hints require explicit evidence.** There is no automatic "nearest article" binding, arbitrary group nesting, cross-group material sharing, or inference from counts/order. A missing or ambiguous child blocks the whole group rather than producing a partial group.
- **Cross-session idempotency is not claimed.** Stable card identity includes the session and document identity. Re-importing the same document into a *new* session creates a new bank; merging into an existing bank uses its own namespace and is not an automatic document-version merge.
- **Group child replay semantics follow the group repository.** Child relations and material updates on replay are owned by that repository; this import path does not re-edit child answers outside its publication contract.
- **Model health and provider diagnostics are separate.** Preflighting candidate models and recording provider health are handled by the model-health work; this import path does not retry or probe providers itself.
- **Diagnostics stay local and redacted.** Raw model responses are not persisted in the ordinary import report. Verbatim failure capture, if enabled, must be an explicit, local, time-limited diagnostic mode and must not be uploaded.
- **Code versus model boundary.** Page segmentation is VLM-driven and per-question splitting is LLM-driven; code enforces identity, coverage, evidence, transaction and state invariants. Code cannot enumerate every layout variant. Genuinely ambiguous cases are expected to reach review, not to be retried until an answer appears.
