# 决策记录：开放 issue 修复范围（2026-10-11）

## 用户指令

> 455、470、471、472 不修复，留待上游解决。其余只进行 bug 类 issue 修复。
> 修复，跑测试，提 pr，确保 github 测试通过。

## 范围裁定

上游开放 issue 按类型与可核实性归类后，**本轮只修 3 条 bug 类 issue**：

| # | 类型 | 本轮处理 |
|---|---|---|
| 468 | bug | **修**（`attachment_read` 对 `res_*` 必然失败） |
| 465 | bug | **修**（AI 写入的题目图片无法加载） |
| 469 | bug | **修**（会话 `meta_json` 体量失控） |
| 470 | bug | 用户明确跳过，留待上游 |
| 471 | bug | 用户明确跳过，留待上游 |
| 472 | bug | 用户明确跳过，留待上游 |
| 455 | bug | 用户明确跳过，留待上游 |
| 462 | bug | 已由 PR #463 修复（open） |
| 467 | enhancement | 非 bug，不做 |
| 466 | enhancement | 非 bug，不做 |
| 464 | enhancement | 非 bug，不做 |
| 461 | bug | 维护者已认领「下个版本修复」，不重复劳动 |
| 377 / 57 / 46 | bug | 维护者已回访/跟进，无本地可核实的代码缺陷，不做 |

## 实现口径（与 issue 建议的差异，均有本地取证）

1. **#468**：三处 `COALESCE(r.content, '')` → `r.data`（vfs `resources` 表无 `content` 列）。
   并把 ID→`files.id` 的解析收敛为 `VfsAttachmentRepo::resolve_to_file_id`（SSOT），
   图片字节走 `VfsAttachmentRepo::get_content_with_conn`（内联 base64 / original_path /
   blob_hash 三级兜底）；blob 缺失返回结构化 `ATTACHMENT_BLOB_MISSING`。
2. **#465**：读取侧 `vfs_get_attachment_content` 接受 `res_*`；写入侧
   `normalize_question_images` 把 `res_*` 归一化为 `file_*`（无法解析则结构化报错、
   不落库）；前端失败写 `'error'` 哨兵并渲染可见占位（不再无限转圈）。
3. **#469**：**只做 before/after 去重**。
   - issue 建议的「`toolResults` 不再写进 `meta_json`」**未采纳**：`meta.tool_results`
     是 Gemini 3 `thought_signature` / `reasoning_content` 跨轮回传的唯一通道
     （`pipeline/compaction/segmentation.rs:62`、`pipeline/history.rs:1051`、
     `pipeline/multi_variant.rs:3779`），删除会破坏工具调用历史重放。
   - issue 建议的「load_session 按用途裁剪 `_meta` / 懒加载」属 IPC 契约变更
     （需要配套按需拉取），归为增强，本轮不做。
   - 保留一份（存 `before`），读侧统一 `after.or(before)`（`send_message.rs` /
     `TauriAdapter.ts` 已是该语义）；运行中确有技能加载时由 `skills_executor`
     覆盖 `after`。
   - 另按 issue 建议 5，把 `remapLegacyBuiltinToolName` 的快照扫描改为按消息缓存的
     工具名索引（`WeakMap`），消除 O(块数 × 快照数 × schema 数)。

## 验证（RULES #8：测试通过才允许构建）

- `cargo test --lib -- chat_v2::tools::attachment_executor`：8 passed（含 3 条新增）
- `cargo test --lib -- chat_v2::tools::qbank_executor`：15 passed（含 1 条新增）
- `cargo test --lib -- chat_v2::pipeline::persistence`：6 passed（含 1 条新增）
- `cargo test --lib`：全量见构建日志
- `npx tsc --noEmit`：通过
- `npx eslint <changed files>`：0 errors
- `npx vitest run src/features/chat/core/store/__tests__/`：150 passed
