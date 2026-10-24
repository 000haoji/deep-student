# Question Import Test Fixes

Date: 2026-10-23. Baseline: v0.10.7 / aabf0c1c, uncommitted worktree.

## Observed Failures

The application logs and read-only local VFS/chat database show separate failures:

- A 38-question import had committed, but loading the session failed because the
  formal repository serialized `source_type=imported` while the legacy preview
  accepted only `import_file`. The preview enum also lacked true/false,
  matching, ordering, and numeric question types.
- Chat authoring requests omitted required source identities or referenced
  shared materials absent from the same document. The tool schema did not
  explain these nested fields precisely. Invalid requests also allocated empty
  containers before validation.
- Later 52-question / 8-material / 8-group requests were fully saved as drafts,
  but placed exam summaries in `coverage`. This made them require review, while
  chat had no tool to read, review, or confirm an existing saved draft.
- The generic tool JSON repair path could accept a truncated authoring prefix,
  omitting trailing materials. Authoring calls now reject incomplete arguments.
- DOCX embedded-image source descriptors lacked stable hashes required at
  confirmation. All source types now retain complete source identity.

## Implemented

- Explicit source-type projection and compatible preview deserialization;
  complete legacy question-type enum.
- Validation before target creation, actionable field/identity errors, exact
  source/coverage tool schemas, and target reuse on idempotent chat retries.
- Saved-draft list/read/review/confirm chat tools and a pending-draft UI entry.
  Review retains question/material identities and does not regenerate content.
- Complete JSON fences may be stripped as a transport wrapper; invalid JSON,
  prose-plus-JSON, or incomplete output is never repaired into a successful
  question document.
- Group/material administration in UI and chat, including OCC, reorder,
  membership changes, reference-safe deletion, and ungrouping that preserves
  necessary material. Prior submissions retain their historical snapshots.
- Original source page/bbox viewer, including cached library PDF pages and
  explicit unavailable/unknown-page states.
- Historical preview audit and explicit migration, plus source-slot reparse
  comparison and explicit keep/new/replace decisions. Resolution is atomic and
  idempotent; existing IDs/history are preserved. Unsupported isolated grouped
  imports are not offered as available actions.
- Provider-limit range planning and explicit original-range selection. Default
  requests remain whole-document, including pages 1-20. Range retries preserve
  original evidence and partial coverage, never use a summary handoff, and do
  not claim the entire document was processed. Source pages merge across ranges.
- Copied source/material/group IDs retain their domain prefixes.

## DeepSeek Skills

See `deepseek-skills-discovery-metadata-durability.md` for the full evidence and
the distinction between proved application bugs and unproved provider/model
hypotheses. Missing-skill failures now include exact trusted runtime IDs in the
LLM-visible error, and settings updates preserve backend-owned frozen catalogs.

## Verification Boundaries

Verification results are recorded with the final packages. Mock-provider tests
and local saved-draft replay do not establish real OCR/model quality. No external
model calls, Android-device import, or two-device cloud sync are claimed. User
application data was only read; no repair SQL was run against the live database.
