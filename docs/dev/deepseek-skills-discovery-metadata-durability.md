# Skills discovery + session metadata durability (2026-10-23)

## Scope

Investigation of `tree-deepseek-v4.1-flash` repeating `load_skills` with invented IDs while GLM worked, plus the follow-up session-metadata clobber found in the same logs.

Evidence sources:

- Local application and backend model-usage logs.
- A read-only local ChatV2 session database.

No external/paid model calls were made. Raw request bodies were not available (`debug.persist_logs=false`, `dev.show_raw_request=false`, debug request directory empty), so provider-wire behavior cannot be proved from this evidence set.

## Proved

1. The affected DeepSeek session exposed only the `load_skills` tool at `02:19:47`.
2. The trusted runtime already contained `qbank-tools`: the `skill_contents_keys` log for that send includes it.
3. DeepSeek emitted five consecutive invalid-ID calls (`question_bank`, `题库`, `quiz`, `builtin:question_bank`, ...). Every round completed `0/1 succeeded`.
4. The old `load_skills` failure message contained only `Missing: ...`, not the exact loadable IDs.
5. Failed tools are fed back to the model as `Error: {message}` only (`pipeline/history.rs`, `context.rs`); structured output is persisted for UI/audit but is not the LLM-facing tool content. Therefore adding IDs only to `ToolResultInfo.output` would not break the loop.
6. After edit/resend with GLM at `02:22:24`, the first call used exact `qbank-tools`, `dstu-tools`, `learning-resource`, `vfs-memory`, `ask-user` and succeeded with 63 tools. Registry/trust/tool injection was healthy.
7. Client-side pipeline preserves the system content array and the request prefix was accepted/cached (`31,301` prompt tokens first round; `31,872` cached tokens on the next).
8. The same session's `availableSkillsSnapshot` freeze log says `bytes=18249`, but final `chat_v2_sessions.metadata_json` has no `availableSkillsSnapshot`. The same second contains a `chat_v2_update_session_settings` call. This is a separate metadata durability bug, not the cause of the in-session loop.

## Hypotheses (not proved)

- DeepSeek may not have attended to the 18 KB `<available_skills>` catalog.
- The configured OpenAI-compatible relay may transform or ignore array-form system content.

Neither is claimed as fact. The fix does not depend on either hypothesis: it makes failure results recoverable in-band and makes settings updates unable to erase backend runtime metadata.

## Fixes

### Skills discovery

- `skills_executor.rs` now returns exact `available_skill_ids` (trusted runtime keys minus admission errors) in the model-facing failure text, plus structured `requested/missing/newly-loaded/already-loaded/rejected` fields.
- Empty `skills: []` is a discovery request: it returns exact IDs and loads nothing.
- `progressiveDisclosure.ts` mirrors the contract and adds `getAvailableSkillIdsForDiscovery()` with prompt visibility, trust, requires, dependency, and `disableAutoInvoke` filtering.
- No fuzzy matching, aliases, auto-loading, or dynamic per-session schema enum was added.

### Session metadata durability

- `update_session_settings_in_db` now does read + patch + write in one `IMMEDIATE` transaction, serialized with the repo freeze/advance helpers.
- Supplied metadata replaces the normal metadata snapshot (existing settings semantics, so omitted normal keys are still deleted); `Some(None)` clears normal keys.
- Backend-authoritative keys are ignored from the patch and re-stamped from the current DB row: `availableSkillsSnapshot`, `availableSkillsSnapshotGeneration`, `availableSkillsSnapshotPendingGeneration`, `toolFacePrefixGeneration`, `frozenToolSchemaOrder`, `toolSchemaDigest`, `microcompactAnchor`, `manuallyArchivedBy`, `groupArchivedBy`, `branchedFrom`.
- `last_compaction_id` is untouched because settings updates never write that column.

## Tests added

- Native: stale settings patch after freeze retains snapshot/generation/pending, tool-prefix keys, and microcompact anchor.
- Native: settings patch leaves summary-only 487 compaction boundaries and `last_compaction_id` unchanged.
- Native: normal metadata replacement updates supplied fields and still deletes omitted normal keys (including `chatV2Draft` for hidden-draft promotion), while authoritative keys survive; explicit clear keeps runtime keys.
- Frontend: unknown/empty `load_skills` returns exact trusted IDs in the model-facing message; untrusted IDs are not advertised.

## Residual risks

- If a renderer supplies a non-object metadata patch while authoritative keys exist, it is ignored with a warning to protect them. The current frontend contract is `object | null`.
- An explicit per-session enum for `load_skills` remains a possible future improvement, but it would need session-frozen schema bytes and should not mutate mid-session.
