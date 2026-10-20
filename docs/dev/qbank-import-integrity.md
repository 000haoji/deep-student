# Question-bank document import integrity

## Scope and authorization

The user requested diagnosis of incomplete document imports, answer sections appearing as questions, and inconsistent question counts. After reviewing the diagnosis, the user requested implementation, local tests, a pull request, and Linux deb / Android packages, and subsequently requested continuation of that work.

This change concerns future imports and safe replay of versioned import checkpoints. It does not migrate or delete existing user questions, answers, attempts, notes, or flashcards. Historical banks without stable source identities require a separately authorized, backed-up migration; text similarity must never be used as a deletion identity.

## Import contracts

### Typed source blocks

Extraction produces `question`, `answer`, `explanation`, `heading`, `material`, `figure`, or `unknown` blocks. Only validated question blocks can become practice questions. Unmatched or ambiguous answers remain in the persisted import report rather than being converted into question stems.

Answer association uses a unique original label within its section. A bare label can only be used when it uniquely identifies a question in the document. Counts, array order, or similar stems do not establish ownership. Mathematical punctuation, options, and repeated questions in different sections are not discarded by a fuzzy deduplicator.

Source identities and source spans are carried through extraction, cross-page merging, structuring, and publication. Structuring responses must map one-to-one to the supplied source IDs. Truncated or interrupted model responses are failures, not completed batches. Blocks with unresolved continuation markers are deferred rather than published as truncated questions; after neighboring pages are recovered, the complete question is first published under its leading source identity. Declared figures that cannot be extracted are recorded as quality failures instead of silently completing the import.

### Staging and publication

Text and DOCX extraction results are staged in `ImportCheckpointState.candidate_results`. Failed pages and chunks remain retryable. Visual structuring checkpoints are persisted per batch. Model callbacks do not publish questions directly.

The publication boundary uses an immediate SQLite transaction for questions, the complete preview projection, question-bank statistics, resource data, import report, and checkpoint finalization. Stable card identity derives from the session, document version, and source identity. Replay preserves user edits, answers, notes, attempt history, and soft-deletion tombstones.

Only a fully completed import clears its checkpoint. Partial, failed, and review-required outcomes retain their quality report and, where usable, their retry material. Questions with failed declared figures are deferred until extraction succeeds, so replay does not have to overwrite an already published imageless question. The report is not proof that a model extracted every semantic detail correctly; uncertain coverage and unmatched answers must remain visible.

### Source and answer provenance

`questions.source_ref` retains the document and block identity, original label and section, page indices, source spans, and source metadata. Explicitly unverified answer or explanation evidence cannot be published or used to fill an existing empty field. Safe fills must retain field-level provenance and leave existing user values intact.

Non-visual evidence must be grounded in supplied source text or an explicitly mapped DOCX image, not in the model's own reconstructed `raw_text`. An answer inferred by a model is not an extracted answer.

## API additions and compatibility

### Persisted import report

`ExamSheetSessionMetadata.import_report` is optional for compatibility with older banks. Its version-1 payload includes:

- `status`: `completed`, `partial`, `needs_review`, or `failed`.
- Failed pages/chunks, incomplete batches, quarantined blocks, and conflicts.
- Matched/excluded source entries and provenance/audit information.
- `resume_available`, which is true only when a usable checkpoint exists.

The frontend restores warnings from this persisted report instead of relying only on transient progress events.

### Original source images

`qbank_get_source_images` accepts optional `pageIndex` and `metadataOnly` arguments in addition to `examId`.

- No new arguments: preserve the existing full-image response contract.
- `metadataOnly: true`: return the source image directory with empty `dataUrl` values and no image reads/base64 encoding.
- `pageIndex`: read only the selected global preview page.

Responses retain camel-case `blobHash`, `dataUrl`, and `pageIndex`. Preview page indices are global across appended documents; legacy `metadata.source_image_hashes` is only a fallback when the preview has no source blobs. The image dialog prefers `source_ref.preview_page_indices` over document-local page indices, loads one page on demand, and releases its image URLs when closed.

### Question exclusions

The completion screen reads all formal questions with pagination. Excluding cards resolves exact shared card IDs, soft-deletes those formal questions, and updates preview, resource data, and statistics in one transaction. Legacy banks whose preview and question identities disagree are rejected with `QBANK_IMPORT_REVIEW_REQUIRED`; no text-based guessing or partial deletion is attempted. Exclusion cannot be combined with create/edit operations in one request.

## Resource limits

Imports share a single process-wide memory permit. Image input is limited to a 200 MiB batch and 64 million pixels per image, with invalid dimensions rejected before decoding. Frontend image reads are sequential. Figure extraction retains at most three page images in its cache. These are capacity safeguards, not claims of measured end-to-end memory performance.

## Validation and remaining limits

Regression tests use synthetic content and temporary databases. They cover scoped question/answer association, replay preservation, soft-deletion, complete multi-page projections beyond 100 questions, transaction rollback, truncation/alignment gates, source-image targeting, and completion-screen exclusion behavior.

Build and test results are recorded in the delivery record for the specific commit. Local application data, original user documents, build logs, and binary packages are not included in this repository or uploaded with the pull request. Installed-application GUI, real-device Android, and live-model acceptance tests must not be claimed unless actually performed.

## Known limitations

These are confirmed gaps in the current implementation, not claims that they are acceptable.

- **Normalized output versus literal validation.** LaTeX is the intended normalized form for formulas and ions so the frontend can render them consistently. Validation, however, removes only whitespace before comparing the model output with the verified source text. An equivalent form such as `$Ca^{2+}$` against `Ca²⁺`, or a full-width/half-width variation, can therefore reject an otherwise valid question. `preserve_existing` compares raw strings too, so a replay whose fallback path retains `（考点：…）` can be reported as `existing_question_edited` even when the user did not edit anything.
- **Inline option splitting requires leading whitespace.** `split_inline_option_markers` only starts a new option when the option letter is preceded by whitespace. A source such as `C．……）D．……` keeps `D` inside `C`; the local fallback then publishes the wrong option count, while a model that returns four options is rejected by the exact-count check.
- **Resume cannot repair VLM-level defects.** Resume reuses the cached per-page VLM analyses and only re-runs structuring. Unresolved `continues_from_previous/to_next` markers therefore remain unresolved on every resume. `resume_available` is true for any non-completed import with a checkpoint, even when no remaining issue is fixable by resuming; the UI can offer an effectively endless retry.
- **Unknown page furniture blocks completion.** Page headers/footers classified as `unknown` are recorded as quarantined entries, and any quarantined entry keeps the report in `needs_review`. There is no distinction between page furniture and a potentially missed question.
- **Tabular and multi-label answers are not associated.** Answer blocks shaped as `题号 1 2 … / 答案 B A …`, blocks labelled `4、5`, and labels that are ambiguous across sections cannot be matched to a single question. The answers remain absent rather than being guessed, but an explicit unsupported-format report or deterministic table parser is still missing.
- **Failed-batch diagnostics are too thin.** A batch that fails identity/schema/coverage validation records only the valid-object count; the raw model response is not retained. Diagnosing whether the cause is formatting, an invented answer, or a malformed option requires reproduction or guesswork.
- **Model availability is not preflighted.** The VLM candidate chain is tried at import time. Invalid model ids, providers without a channel, and rate limits produce repeated 502/503 retries before a working candidate is found. A cached availability probe or circuit breaker is not implemented.
- **Publication and indexing can contend on SQLite.** A VFS indexing write concurrent with the publication transaction can fail with `database is locked`. Publication itself remains atomic and questions are committed; the indexing failure is recorded but not retried with a dedicated backoff.
- **Code versus model boundary.** Page block segmentation is VLM-driven and per-question stem/option splitting is LLM-driven; code performs cross-page assembly, identity/coverage checks, answer association, and publication. Code cannot enumerate every layout variant. The intended boundary is that code enforces structural and evidence invariants, while normalization-aware equivalence and genuinely ambiguous cases should be resolved by canonicalization, a lightweight verifier, or explicit human review rather than repeated retries.
