# Context Compaction Policy

## Context Window Controls

The advanced panel resolves the runtime model override, pinned session model,
or global default in that order, and reacts to model and assignment changes.
The slider accepts the exact configured context window, including 1,000,448
tokens. Automatic input budgets continue to reserve output tokens and headroom.

The final integrated revision also includes PR #493's trusted runtime skill
discovery feedback and the DeepSeek Chat Completions request normalization.
The latter was initially held as a local patch and is included after the user's
subsequent instruction to submit the remaining fixes. Request formatting does
not depend on whether detailed logging is enabled.

## Manual Compaction

The context-usage popover's existing compact button and the exact `/compact`
composer command invoke `chat_v2_compact_session`. This is a local command, not
a user message sent to the chat model, and it does not consume pending files.
Completion exposes the command alongside skills without registering a fake
skill. Compaction is disabled during generation; concurrent clicks are guarded.

Manual compaction calls the configured compaction model, falling back to the
active chat model when no dedicated model is assigned. Its prompt contains task
anchors, older conversation data, and the previous summary when present. The
model's structured summary is persisted transactionally, then the composer
reloads the session and updates its context usage.

Oversized conversation/tool input is split into bounded summary requests rather
than cutting off the end of a long message before it reaches the summary model.

Manual compaction has no minimum usage percentage and bypasses automatic failure
cooldowns. Its verbatim tail target is 25% of the currently visible history,
capped at 64K tokens, rather than 25% of the model's entire input window. It
preserves at least the latest complete turn, including its tools and signatures.
The older range is summarized by the model, not discarded. Original database
messages remain available, and the existing undo operation restores the previous
view. Sessions without an older range report that condition explicitly.

If the summary request fails or its repair still produces an invalid summary,
manual compaction reports failure without committing a static substitute or
hiding original messages. Automatic compaction retains the existing explicitly
marked degraded-summary fallback.

## Visible Progress And Completion

Manual `/compact` and button actions show an immediate start notification and
a persistent spinner/status line above the composer. Completion shows a success
notification with before/after token estimates when available; skipped and
failed commands explain the result without claiming success. Pending input and
attachments remain intact, and duplicate compaction/submission is blocked.

Automatic preflight, in-loop, overflow-recovery and end-of-turn compactions emit
`compaction_started` once real summary work begins. The session store drives the
same persistent composer status. A scope guard emits `compaction_finished` on
all exits, including cancellation, errors and dropped futures. Successful commit
then emits the existing `compaction_completed` notification and usage update;
failure retains the existing `compaction_failed` feedback. A request skipped
before summary work starts does not create a misleading progress notification.
Runtime progress is session-local and is not persisted with conversation data.

## Automatic Compaction

Existing provider-usage checkpoints remain in place. Additionally, streaming
ChatV2 requests preflight their complete assembled payload before network I/O:
system text, tools, historical reasoning and tool results, current input and
attachments, and image reservations all participate in the same estimate.

At 85% of the effective input limit, the local `COMPACTION_REQUIRED` result
returns control to ChatV2. It includes the actually selected model and budget,
so fallback models and window downsizing use their own limits. It is explicitly
nonretryable for network/model failover. The orchestrator compacts and reloads
history, then rebuilds the request before sending. Each tool round gets at most
one such attempt; a failed/skipped attempt can still send an unchanged request
that fits the hard limit, but an oversized request stops with an actionable
error. Parallel variants wait for a concurrent session compaction and reload
its view; their in-flight tool chains are preserved outside that DB prefix.

History loaders no longer select only the newest 50-400 messages. SQLite
hydration remains parameter-batched. Both history-level FIFO and final-request
FIFO/text-tail trimming are removed. Nonstream requests also fail rather than
mutating an oversized input. The ordinary history loader also stops replacing
older, unsummarized tool outputs with placeholders. Provider request logs do not
control this policy.

## Regression Coverage

- Ordinary and variant loaders retain 450 short messages, including when the
  configured input budget is too small to send them without compaction.
- Complete-request preflight counts system/tools/current attachment/image data,
  handles a smaller selected model, and never mutates turns or tool pairs.
- A local HTTP summary model proves that manual compaction below 20% occupancy
  sends conversation data to the model and persists its returned summary.
- A summary-model failure leaves the active view and original rows unchanged.
- A headless ChatV2 round proves automatic compaction occurs before its first
  chat request and that the rebuilt request includes the summary and current
  user message. An uncompactible oversized request stops before network I/O.
- Composer tests cover the existing button and `/compact` at 200K/1M tokens,
  preserving pending files and not sending/queuing a chat message.
- Additional regressions cover visible pending state, duplicate prevention,
  automatic lifecycle feedback and completion/failure notifications. These were
  added with the final feedback change; the full test suite was not rerun at the
  user's request. Final package builds and static checks are reported separately.

Full local CI results and build provenance are recorded separately in the
ignored `.local-work/verification/` and `.local-work/deliverables/` directories.
