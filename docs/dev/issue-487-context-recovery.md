# Issue 487: Input Budget Recovery

Baseline: v0.10.7. Video playback changes for issue 492 are intentionally not
part of this submission.

## Changes

- Short histories, including histories with only two turns, may be compacted.
- The final request guard can omit old images while retaining current-turn
  images and original resource references.
- ChatV2 no longer truncates the current question or tool JSON to pass the guard.
- A single-variant request rejected by the guard can compact, reload history and
  rebuild the request once. Completed tools are not executed a second time.
- Local estimates do not become provider-reported token usage.
- Overflow and manual compaction can summarize task anchors. A summary-only
  boundary is identified by `tail_start_message_id == summary_message_id`;
  `range_end_message_id` retains the last summarized message. Reload, edit
  invalidation and branch copying understand this representation. Original
  messages are not deleted.
- Compaction accounts for historical images and uses the same effective output
  reserve as the final request guard.
- Budget error codes and diagnostics survive stream events and reach the
  localized frontend error channel.

## Verification

Focused regressions cover request input guards, short histories, compaction,
summary-only branch copying, frontend error mapping and settings updates that
must preserve backend compaction metadata.

```bash
npm run typecheck:native
npx vitest run src/features/chat/adapters/__tests__/TauriAdapter.errorChannel.test.ts src/features/chat/adapters/errors/__tests__/adapterErrorChannel.test.ts --maxWorkers=2
cd src-tauri
cargo test --lib --no-default-features --features mobile-slim,tokenizer_tiktoken chat_v2::pipeline::compaction
cargo test --lib --no-default-features --features mobile-slim,tokenizer_tiktoken llm_manager::model2_pipeline::tests
cargo test --lib --no-default-features --features mobile-slim,tokenizer_tiktoken chat_v2::handlers::manage_session
```

The recovery orchestration does not expand multi-variant compaction or change
the generic CJK token coefficient. Fixed prompts/tools or the current input may
still exceed the selected model's limit; this remains an actionable error.
