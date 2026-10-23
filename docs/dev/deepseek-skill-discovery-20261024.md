# DeepSeek Skill Discovery And Context Controls

## Evidence

The Linux application log at 2026-10-24 02:28-02:30 (Asia/Shanghai) shows
12 failed `builtin-load_skills` calls. Arguments arrived as valid JSON; the
requested IDs (`file`, `docx`, `pdf`, `shell`, etc.) were not registered IDs.
The runtime contained `docx-tools`, `qbank-tools`, `learning-resource`, and
other exact IDs. This is a discovery failure, not missing stream arguments.

The user reported success while detailed request logs were enabled, followed
by failure when disabled. The saved 02:39 requests include the full system
catalog. That run still mixed valid and invalid IDs in its first tool calls;
partial loading allowed it to proceed. A new 02:49 session, after logging was
disabled, returned only guessed IDs. These are different turns and sessions,
not a controlled same-request comparison, so they do not prove a logging
switch changes provider behavior.

Three small synthetic calls used the configured relay and model, with no user
documents. A random exact skill ID appeared in system content as either a
text-block array or a string. Both returned HTTP 200 but guessed `diagnostic`
instead of the exact ID. Moving the same kind of catalog into a user message
returned the exact ID. This suggests a system-instruction visibility/following
problem on that route, but does not distinguish the model from relay handling.
It is not an official-endpoint verification or a statistical model evaluation.

## Official Contract

Checked against DeepSeek's public documentation on 2026-10-24:

- [First call](https://api-docs.deepseek.com/): `deepseek-flash` is served by
  V4.1-Flash-1017. The custom `tree-deepseek-v4.1-flash` relay name is not an
  official model ID and was not renamed automatically.
- [Chat Completions](https://api-docs.deepseek.com/api/create-chat-completion):
  system content is a string; output limits use `max_tokens`.
- [Thinking mode](https://api-docs.deepseek.com/guides/thinking_mode): when
  tools are present, all historical assistant `reasoning_content` must be
  passed back, even for turns without a tool call.
- [Tool calls](https://api-docs.deepseek.com/guides/tool_calls): `strict` is a
  Beta feature requiring the Beta endpoint and compatible schemas. Do not
  enable it globally on an unknown relay. `required`/named tool choices are
  not supported in thinking mode.
- [Responses](https://api-docs.deepseek.com/guides/responses_api): a separate
  protocol, with `instructions` and `max_output_tokens`; do not apply the Chat
  Completions system-content normalization to it.

## Repair Scope

DeepSeek Chat Completions request preparation now flattens text-only system
parts and maps `max_completion_tokens` to `max_tokens` before logging and
network transmission. Explicit `max_tokens` wins. User images, tool calls,
reasoning history, other models, and Responses/Anthropic remain unchanged.

Invalid/empty/missing skill requests return sorted exact runtime-admitted IDs
in the error string that the tool loop actually sends to the model. Partial
loads preserve the valid subset and report a recovery hint for missing IDs.
Suggestions do not automatically load any skill or disclose rejected skills
or skill bodies. No logging switch is required by either change.

The advanced context panel previously ignored the global default model in
unpinned sessions. It could infer roughly 512K from the output limit instead
of reading the configured 1,000,448-token window. It now resolves runtime
override, pinned model, or global default in the same order as the composer,
and refreshes after model configuration/assignment changes. The slider can
reach 1,000,448; the automatic input budget still reserves output and headroom.

These are source changes, not an update to the locally installed DEB. The
observed logging correlation is not claimed as fully explained or resolved.
