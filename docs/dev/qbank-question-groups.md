# 题库题组与共享材料（Issue #491）

> 状态：底座（模型/API/迁移/同步/导出契约）实施中。本文是后端契约与迁移说明，
> 不代表前端或导入管线已闭环。

## 1. 为什么新增题组，而不是复用单题或 `parent_id`

阅读材料、数学共享条件不能塞进某一题的正文（会重复存储、随机抽题丢上下文），
也不能复用 `questions.parent_id`（那里是**变式题**语义）。首版模型刻意保持最小：

- **一层平级题组**：`question_groups`（`reading` / `shared_condition` / `other`）。
- **组内多份有序材料**：`question_group_materials`，正文/配图/来源各存一份。
- **独立可答子题**：`questions` 新增 nullable `group_id` / `sub_label` / `group_order`。
- 组头**不是**一道题：不计题数、不产生作答记录、不参与掌握度统计。
- 首版不做任意嵌套、不做跨题库全局材料库、不做跨新 session 自动合并。

## 2. 数据模型（V20261021）

### question_groups

| 字段 | 语义 |
| --- | --- |
| `id` | `qg_{nanoid(10)}` |
| `exam_id` | 所属题目集；必须存在且未软删（触发器校验） |
| `group_type` | `reading` / `shared_condition` / `other` |
| `title` | 可选组标题 |
| `source_type` / `source_ref` | 来源类型与稳定来源身份（页/区块/来源文档） |
| `import_baseline_json` | 字段级导入基线（导入器内部使用，不承诺对外稳定） |
| `version` | 关系/材料变更版本，单调递增；**所有编辑命令的 CAS 基线** |
| `publish_state` | `staging`（对 UI 不可见）/ `complete`；直接 insert 默认 staging，正式发布必须走 `publish_group_bundle` |
| `expected_child_ids_json` / `expected_material_ids_json` | RowSync 完整性 manifest |
| `created_at` / `updated_at` / `deleted_at` | 软删语义；`updated_at` 单调（毫秒） |
| `device_id` / `local_version` | 行同步字段 |

### question_group_materials

| 字段 | 语义 |
| --- | --- |
| `id` | `qm_{nanoid(10)}` |
| `group_id` / `exam_id` | 必须与组同 exam 且组未软删（触发器校验） |
| `order_index` | 组内顺序，从 1 开始连续；组内唯一（partial unique） |
| `content_type` | `text` / `formula` / `figure` / `mixed` |
| `title` / `text` | 材料标题与正文（权威副本只存这里） |
| `images_json` | `QuestionImage[]`；hash 计入同步/备份资产闭包 |
| `source_ref` / `page_index` / `bbox_json` | 来源定位 |
| `group_version` | 写入时的组版本，用于可见性校验 |

### questions 新增列

| 字段 | 语义 |
| --- | --- |
| `group_id` | 所属组；NULL = 独立题（旧数据全部 NULL） |
| `sub_label` | 组内小问标签，如 `(1)` / `(a)`；非空时组内唯一 |
| `group_order` | 组内顺序，从 1 开始；非空时组内唯一 |
| `group_version` | 写入时的组版本 |

约束：`group_id` 为空时 `sub_label`/`group_order`/`group_version` 必须为空/0；
`group_id` 非空时 `group_order > 0` 且 `group_version > 0`（触发器硬校验）。

## 3. 原子发布与可见性

### 3.1 发布

唯一推荐入口：`VfsQuestionGroupRepo::publish_group_bundle_with_conn`（调用方持事务）
或 `publish_group_bundle`（自开事务）。顺序：

1. 纯参数校验（子题至少 1 个；reading/shared_condition 至少 1 份材料；
   `order_index` 与 `group_order` 从 1 连续唯一；`sub_label` 唯一；子题 exam 一致；
   禁止用 `parent_id` 表达题组关系）。
2. 插入组（`publish_state='staging'`）。
3. 插入材料（`group_version` 随 INSERT 写入）。
4. 插入子题（`group_id/sub_label/group_order/group_version` **随 INSERT 一次写入**）。
5. 校验存活成员数 == 提案数，写 manifest，置 `complete`。

任一步失败 → 整个事务回滚，不会留下半组。**禁止"先发布 question 再回写 group_id"**。

### 3.2 可见性（RowSync 安全）

`visible_group_sql()` 统一谓词：

- 组未软删且 `publish_state='complete'`；
- manifest 中每个 child/material id 都已落地、未软删；
- 存活成员计数 == manifest 长度；
- 每个成员的 `group_version` == 组 `version`（旧版本内容齐全 ≠ 新版本完整）。

所有读取路径（列表/详情/搜索/错题/随机/统计/AI 工具/导出）必须经过该谓词。
子题为 staging/不完整组的情况一律不返回，避免半组泄露。

### 3.3 RowSync 乱序与墓碑

- 应用顺序：`exam_sheets` → `question_groups` → `question_group_materials` →
  `questions`（`apply_dependency_rank`）。
- 跨批次乱序：组行缺 `exam_sheets`、材料/子题缺 live 组时，写入
  `__question_group_sync_staging`（本地辅助表，不参与同步）；批次结束按依赖顺序
  重放（最多 4 轮），父行到达后自动落地。不是"报错隐藏"，也不是永久失败；
  `attempts`/`last_error` 可审计。父行被软删时不重试（保留暂存供诊断）。
- manifest 自愈**只认显式墓碑**：批次末尾 `repair_question_group_manifests`
  仅当 manifest 内的成员"本地存在且 `deleted_at` 非空"时才收缩；成员本地完全
  不存在（partial/乱序）或版本不一致时绝不收缩，组保持不可见等待补齐。
- 回归：`group_rowsync_child_first_and_tombstone_apply_through_production_path`
  （真实 `apply_downloaded_changes`，覆盖 child-first 暂存→drain、缺成员不可见、
  显式 tombstone 后收缩）与 `group_manifest_repair_requires_explicit_tombstones`。

## 4. CAS（乐观锁）

- 编辑命令**强制** `expected_version`（组版本），不支持省略绕过。
- 材料没有独立版本：材料/关系变更同事务 `group.version += 1`，并把新版本
  重新盖章到所有存活成员（`group_version`）。
- 冲突错误串：`QBANK_CONFLICT: expected_version={}, actual_version={}`。
- `updated_at` 单调生成（毫秒，严格大于旧值）。

## 5. 删除语义

- 单删子题：仅软删该题；同一事务（SAVEPOINT）内从组 manifest 移除该 id、
  推进组版本、重盖章剩余成员并重新入队索引；删掉最后一题后组不可见（空壳组）。
- 单删材料：必须携带 `expected_version`（组版本 CAS）；组内仍有存活子题且这是
  最后一份材料时拒绝（`QBANK_GROUP_MATERIAL_IN_USE`）；用户应替换材料或显式删整组。
- 删整组：先 `qbank_preview_group_delete` 展示受影响子题，再
  `qbank_delete_question_group(group_id, expected_version, confirmed=true)`；
  同事务软删组 + 材料 + 子题。
- 首版不做解绑/移组（会丢共享条件），不隐式复制正文、不留下孤儿题。

## 6. 命令与 TS 契约

```
qbank_get_question_group_context(exam_id, question_id?|card_id?, purpose?)
qbank_get_question_group(group_id)
qbank_list_question_groups(exam_id, page?, page_size?)
qbank_create_question_group(params)
qbank_update_question_group(group_id, update, expected_version)
qbank_preview_group_delete(group_id)
qbank_delete_question_group(group_id, expected_version, confirmed)
qbank_create_group_material(params)
qbank_update_group_material(material_id, update, expected_version)
qbank_delete_group_material(material_id, expected_version)
qbank_publish_question_group(bundle)
qbank_search_group_materials(keyword, exam_id?, limit?) -> GroupMaterialHit[]
```

`qbank_search_group_materials` 是材料检索的派生入口（命中片段 + 具体子题定位），
权威材料正文仍只存 `question_group_materials`；未修改 `qbank_search_questions` 的
既有分页/高亮语义。

序列化保持仓库既有 snake_case（与 `Question` 一致）：

```ts
interface QuestionGroup {
  id: string; exam_id: string;
  group_type: 'reading' | 'shared_condition' | 'other';
  title: string | null; source_type: string; source_ref: string | null;
  version: number; publish_state: 'staging' | 'complete';
  expected_child_ids: string[]; expected_material_ids: string[];
  created_at: string; updated_at: string; deleted_at: string | null;
}
interface QuestionGroupMaterial {
  id: string; group_id: string; exam_id: string; order_index: number;
  content_type: 'text' | 'formula' | 'figure' | 'mixed';
  title: string | null; text: string; images: QuestionImage[];
  source_ref: string | null; page_index: number | null;
  bbox: [number, number, number, number] | null;
  group_version: number; created_at: string; updated_at: string; deleted_at: string | null;
}
interface QuestionSiblingBrief {
  id: string; sub_label: string | null; question_label: string | null;
  group_order: number | null; question_type: string; status: string;
  content: string; options: QuestionOption[] | null; // 不含 answer/explanation
}
interface GroupProgress { total: number; new: number; in_progress: number; mastered: number; review: number; }
interface QuestionGroupContext {
  group: QuestionGroup; materials: QuestionGroupMaterial[];
  siblings: QuestionSiblingBrief[]; sibling_count: number; group_progress: GroupProgress;
}
```

`Question` 新增：`group_id: string | null`、`sub_label: string | null`、
`group_order: number | null`、可选 `group_context?: QuestionGroupContext`。

兄弟题简介**必须**包含必要题干与选项（数学 (2) 依赖 (1) 的条件），但绝不包含
标准答案、解析、结构化答案或作答结果。

## 7. 迁移与回退

- 迁移文件：`src-tauri/migrations/vfs/V20261021__question_groups.sql`（不可变，发布后只新增）。
- Rust 定义：`data_governance::migration::vfs::V20261021_QUESTION_GROUPS`，
  lock manifest 由 `node scripts/check-migrations.mjs --update` 生成。
- 旧数据：三个 relation 列为 NULL，行为完全不变；旧的题目/答案/学习记录原样保留。
- **旧客户端不得写新 schema**：新表与 `questions` 新列不能被旧端安全写入/忽略；
  旧端应拒绝写入题组数据或保持只读。不承诺原地降级；回退使用升级前备份或明确的
  兼容导出，不能覆盖用户当前库。
- 兼容性必须同时验证两条真实路径：
  1. **全量快照/备份**：新表随 `vfs.db` 进入 ZIP；恢复后跑迁移补齐 schema；
     备份 schema 版本高于当前应用已知版本时 fail-close
     （`BackupManager::check_manifest_compatibility`，负向回归
     `backup_manifest_from_newer_schema_version_is_rejected`）。
  2. **逐行 RowSync**：分类/触发器/manifest 完整性/乱序暂存（见 §3.3），
     未就绪时不得向 UI 暴露半组。
- 全量备份：新表随 vfs.db 自动进入 ZIP；恢复后由迁移补齐 schema。
  已有的 `schema_version > known` 拒绝恢复门禁保持不变。
- RowSync：`question_groups`（**整行 Lww**）、`question_group_materials`（Lww）
  已注册分类与 `__change_log` 触发器；`record_id` 使用行自身 id。
  组行不做字段级拼接：`version/publish_state/expected_*_ids_json` 是关系完整性
  与 CAS 凭据，逐列合并会与 manifest 撕裂；冲突时整行按 `updated_at` LWW。
- 资产闭包：`question_group_materials.images_json[*].hash` 与
  `questions.images_json[*].hash` 一样计入 blob ref_count 重算。

## 8. 验证状态与已知边界

已落地并有真实测试证据：
- 迁移 SQL：`tests/question_group_contract_tests.rs` 6/6（FTS 旧 token 移除、软删/恢复/
  硬删、integrity-check、同 exam/软删约束、顺序/子标签唯一、manifest 可见性）。
- repo/发布：`question_group_repo.rs` 单测（原子发布、整组回滚、CAS、上下文不含答案、
  单删子题 manifest 修复、stale delete 不半删、材料搜索）。
- 同步：真实 `apply_downloaded_changes` 乱序/墓碑综合测试（§3.3）。
- 备份：`full_backup_restore_roundtrips_question_group_material_blob`（真实迁移库 +
  合成材料图片 blob → `backup_with_assets` → 删除库/资产 → restore → 组关系、
  material hash、blob 字节、材料 FTS 全部还原）与版本门禁负向测试
  `backup_manifest_from_newer_schema_version_is_rejected`。

**未完成/不得声称**：
- 旧版本 app-data fixture 的升级/故障演练未在本轮执行（门禁入口为
  `scripts/migration-ci/run-fixture-upgrades.sh` + 模板 gate）。
- Android/真机 release 未验证；本轮没有应用构建、安装或用户数据库迁移。
- JSON v2 全量导入导出与 CSV 资产闭包由 bundle 服务实现，不在本模块测试范围。

## 9. 已知边界 / 未闭环项

- JSON v2 全量导入导出、CSV 关系投影与 asset base64 闭包由独立 bundle 服务实现
  （`question_bundle_service.rs`），不在本底座模块内。
- typed 组装（`groups.rs`）与导入发布接入由 #490 cooperator 完成；
  `question_import_service.rs` 不在本模块所有权内。
- tombstones（JSON v2 导出）：`VfsQuestionGroupRepo::list_group_tombstones_with_conn` /
  `list_material_tombstones_with_conn` / `list_child_tombstone_ids_with_conn`。
- 学习状态恢复：`VfsQuestionRepo::restore_learning_state_with_conn`（同事务、可选 CAS）。
- 索引补偿：材料变更后由调用方使用
  `material_change_reindex_question_ids_with_conn` 获取候选，并在同一事务内调用
  索引 ledger 入队；本模块不新建第二套索引机制。
