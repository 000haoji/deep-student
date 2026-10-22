# 题库包 native JSON v2（`deep-student.qbank.bundle`）

> 实现：`src-tauri/src/question_bundle_service.rs`。本文是契约说明；测试结论以
> 实际运行结果为准。
> 副作用边界：运行时读写用户 VFS 数据库与 `vfs_blobs/`；导出只读，导入只新增
> 行并在同一事务内提交。不访问网络、不写包外路径、不覆盖既有题目的教学内容。

## 1. 为什么需要独立主格式

CSV/TXT/Markdown/Anki 无法表达“一组共享材料 + 多个有序子题”的关系。若把材料
复制进每道题正文，权威库会出现多份副本；若只导出题目数组，回导时材料必丢。
因此：

- **native JSON v2 是唯一完整往返格式**（组、材料、顺序、来源、删除记录、真实图片字节）。
- CSV / Markdown / Anki 只是**导出投影**：把材料展开到每行/每卡，并显式标记
  `lossy` 与 `assets_not_included`，绝不写回权威库。

## 2. 头部（固定，不可替换）

```json
{
  "schema": "deep-student.qbank.bundle",
  "format_version": 2,
  "exported_at": "2026-10-21T12:00:00Z",
  "exam": { "name": "英语阅读理解 A", "source_exam_id": "exam_xxx" }
}
```

版本门禁：`schema` 不等于上面的常量、或 `format_version != 2` 一律拒绝
（更高版本也拒绝，不静默降级）。字段名固定为 `schema`，不再引入
`schemaVersion` 等别名。

v1 兼容：根节点是**独立题数组**时按 legacy 导入（无组/材料/资产），
`BundleImportResult.legacy_v1 = true`。

## 3. 结构

```json
{
  "schema": "deep-student.qbank.bundle",
  "format_version": 2,
  "exported_at": "...",
  "exam": { "name": "...", "source_exam_id": "exam_xxx" },

  "sources": [
    { "source_id": "doc:1", "source_type": "ocr", "source_ref": "doc:1", "page_index": 1 }
  ],

  "groups": [
    { "group_ref": "qg_ab12", "group_type": "reading|shared_condition|other",
      "title": "Passage A", "source_ref": "doc:1", "version": 3 }
  ],

  "materials": [
    { "material_ref": "qm_cd34", "group_ref": "qg_ab12", "order_index": 1,
      "content_type": "text|formula|figure|mixed", "title": null,
      "text": "Passage ...",
      "images": [ { "asset_id": "<sha256>", "name": "p1.png", "mime": "image/png" } ],
      "source_id": "doc:1", "source_ref": "doc:1", "page_index": 2, "bbox": null }
  ],

  "children": [
    { "client_ref": "q_1", "group_ref": "qg_ab12", "group_order": 1,
      "sub_label": "(1)", "question": { ...QUESTION... } }
  ],

  "standalone_questions": [
    { "client_ref": "q_9", "question": { ...QUESTION... } }
  ],

  "order": ["q_1", "q_2", "q_9"],

  "tombstones": [
    { "kind": "group|material|question", "ref_id": "qg_ab12", "deleted_at": "..." }
  ],

  "assets": [
    { "asset_id": "<sha256>", "sha256": "<sha256>", "size": 12345,
      "mime": "image/png", "name": "p1.png", "encoding": "base64", "data": "<真实blob字节>" }
  ]
}
```

### QUESTION

```json
{
  "content": "题干",
  "options": [ { "key": "A", "content": "..." } ],
  "answer": "A",
  "explanation": "...",
  "structured_data": null,
  "question_type": "single_choice|...",
  "difficulty": "medium",
  "tags": ["reading"],
  "source_type": "ocr|imported|ai_generated|manual",
  "source_ref": "q:12",
  "question_label": "1",
  "images": [ { "asset_id": "<sha256>", "name": "q1.png", "mime": "image/png" } ],
  "study": {
    "status": "new", "user_answer": null, "is_correct": null, "user_note": null,
    "is_favorite": false, "is_bookmarked": false,
    "attempt_count": 0, "correct_count": 0, "last_attempt_at": null
  }
}
```

`study` 可选；`include_study_state=false` 时导出省略。导入只把它应用到**新建**
题目（既有题永不覆盖）：状态/答案/笔记/收藏走更新接口，答题次数/正确次数/
最近作答时间在同一事务内做一次窄 `UPDATE questions ... WHERE id = <新 id>`；
事务末尾重算 `question_bank_stats`，保证统计一致。

## 4. 校验规则（导入前、导出后都执行）

- `exam.name` 非空。
- `group_ref` / `material_ref` / `client_ref` / `source_id` / `asset_id` 各自唯一。
- 材料必须引用存在的 `group_ref`；子题必须引用存在的 `group_ref`。
- `materials.order_index` 在组内 **1..M 连续且唯一**（与
  `question_group_materials.order_index` 同口径）。
- `children.group_order` 在组内 **1..N 连续且唯一**（与 `questions.group_order` 同口径）。
- `order` 必须恰好覆盖全部 `client_ref`（子题 + 独立题），不重复不遗漏。
- `sources / groups / materials / children / standalone_questions / order /
  tombstones / assets` 均为**必填数组**（可以为空数组）；缺失即反序列化失败，
  不用 serde 默认值吞掉结构缺失。
- 题目至少题干或图片之一非空；选项 key 不重复。
- `assets[].sha256` 为 64 位 hex；`assets[].name` 只能是普通文件名
  （禁止 `/`、`\`、`..`、绝对路径）。
- 图片引用必须能在 `assets` 中找到（导出 `include_assets=false` 时跳过此项，
  并返回 `round_trip_safe=false`）。

## 5. 资产闭包（真实字节，不是 manifest）

- 导出：按 `QuestionImage.hash` 在 `vfs_blobs/<前缀>/<hash>.*` 找到真实文件，
  canonicalize 后必须仍在 blob 根目录内（路径封闭），再校验内容 SHA-256，
  以 base64 写入 `assets[].data`。
- **缺失即错误**：blob 文件不存在或哈希不符 → `asset_missing: ...`，不产出
  残缺包。`include_assets=false` 是显式 lossy 投影（`round_trip_safe=false` +
  `assets_not_included` warning）。
- 导入：strict base64 解码 → 校验 `size` 与 `sha256` → `VfsBlobRepo::store_blob_with_conn`
  （同哈希去重）→ `VfsFileRepo::create_file_with_conn` 建 VFS 文件引用。
  任一失败整包回滚。
- 不复制相对路径以外的信息；不上传任何字节。

## 6. 导入语义

- `target.kind = "new_exam"`：新建题目集（`exam.name` 或 `target.name`）。
- `target.kind = "merge"`：写入既有题目集 `exam_id`；目标不存在直接拒绝。
- `dry_run` 默认 `true`：只做全部校验并报告将写入数量，不建题目集、不落资产。
- 整包一个事务（`BEGIN IMMEDIATE`）；组经
  `VfsQuestionGroupRepo::publish_group_bundle_with_conn`（组/材料/子题同 txn
  原子发布），独立题经 `VfsQuestionRepo::create_question_with_conn`。任一失败
  整体 `ROLLBACK`，不会留下半组、孤立子题或空题目集。
- **ID 重映射**：导入永远生成新主键，`client_ref → new_id` 记入 `id_map`。
  因此题干完全相同的两道题（不同 `client_ref`）都会保留，不按题干去重。
- **tombstones**：`kind/ref_id` 命中的组/材料/题跳过；组墓碑会连带跳过其成员，
  不复活半组。
- **保留用户编辑**：merge 时目标里已有同 `source_ref` 的题目不覆盖；若冲突发生在
  组内，整组跳过（`group_skipped_source_ref_conflict`），绝不生成“少子题”的半组。
  独立题冲突给 `existing_source_ref_skipped`。组本身按 `source_ref` 幂等
  （`group_skipped_existing_source_ref`）。
- **顺序归一化**：墓碑或去重导致成员缺口时，材料 `order_index` 与子题
  `group_order` 按相对顺序重排为 1..M / 1..N（repo 要求连续唯一）；相对次序不变。
- 每组仍是一次 `publish_group_bundle_with_conn`：组 + 材料 + 子题同事务原子发布。

## 7. 导出/导入 API

```rust
BundleExportRequest { exam_id, file_path: Option<String>, include_assets: bool = true, include_study_state: bool = true }
BundleExportResult { schema, format_version, file_path, file_size, sha256, round_trip_safe,
                     groups, materials, questions, standalone_questions, assets, tombstones,
                     warnings: Vec<BundleWarning{code,message,ref_id}>, json: Option<String> }

BundleImportRequest { bundle: serde_json::Value, target, dry_run: bool = true, include_study_state: bool = true }
BundleImportTarget  = { "kind": "new_exam", "name": Option<String> }
                    | { "kind": "merge", "exam_id": String }
BundleImportResult  { schema, format_version, dry_run, committed, exam_id,
                      legacy_v1, groups_created, materials_created, questions_created,
                      questions_skipped, assets_created, assets_deduped, tombstones_applied,
                      id_map: Vec<{client_ref, kind, new_id}>, warnings }
```

命令（由主代理在 `commands.rs` / `lib.rs` 注册，本模块不注册）：

```rust
#[tauri::command]
qbank_export_bundle_json(request: BundleExportRequest, state: State<'_, AppState>)
    -> Result<BundleExportResult, AppError>

#[tauri::command]
qbank_import_bundle_json(request: BundleImportRequest, state: State<'_, AppState>)
    -> Result<BundleImportResult, AppError>
```

模块注册：`src-tauri/src/lib.rs` 需要 `pub mod question_bundle_service;`
（当前归属主代理，本文件不自改 `lib.rs`）。

## 8. CSV / Markdown / Anki 投影

`project_flat_rows` / `project_csv_text` / `project_markdown` 提供扁平投影：

- 每行一个可答子题/独立题，`group_context` 展开该组全部材料正文（按
  `order_index` 升序），图片以 `[图片: 文件名]` 占位。
- 固定带 `lossy` 与 `assets_not_included` 提示；投影不写库，也不冒充可往返。
- CSV 额外列：`group_ref, group_title, sub_label, group_order, group_context`，
  与 `question_export_service` 的扁平字段互补；公式注入前缀按 OWASP 中和。

`question_export_service.rs` 的 `EXPORTABLE_FIELDS` 已新增 `group_id` /
`sub_label` / `group_order` 三列（扁平列，直接来自 `questions` 行）；
共享材料正文**不在那里展开**——需要材料展开请用本模块的
`project_csv_text`（显式 lossy），两套口径不互相冒充。

## 9. 已知限制

- 导出 tombstones 用服务内**只读**查询补充（repo 未暴露软删行接口）：三条
  `SELECT ... WHERE exam_id = ? AND deleted_at IS NOT NULL`，限定同题目集；
  不写库、不跨考试。
- 前端旧实现（camelCase / `schemaVersion` / 无 assets）不是本格式；UI 必须调用
  本服务的命令，直接下载题目数组会丢失关系与图片。

## 10. 测试

- `src-tauri/src/question_bundle_service.rs::tests`：解析/版本门禁、必填数组缺失拒绝、语义校验、
  资产 base64+sha256+size、路径封闭与 `asset_missing`、投影展开与 lossy 标记、
  tombstone 展开、CSV 转义。
- `src-tauri/tests/question_bundle_v2_tests.rs`：真实迁移库 + 真实 blob 的导出/
  导入往返、ID 重映射、相同题干保留、dry_run 不写库、merge 保留用户编辑、
  tombstones、v1 数组、题组经真实 repo 发布/读回、半组失败整包回滚、
  材料图片缺失导出报错、软删题 tombstones 导出/回灌、学习状态与答题计数恢复。

运行（与仓库既有门禁一致）：

```bash
cargo nextest run --locked --test question_bundle_v2_tests
cargo nextest run --locked --lib -E 'test(question_bundle_service)'
```
