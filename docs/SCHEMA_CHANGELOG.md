# 数据库结构变更登记清单 (Schema Changelog)

本文档记录所有数据库的结构变更历史，用于追踪 Schema 演进、迁移审计和版本管理。

---

## 文档说明

### 变更类型定义

| 类型代码 | 中文名称 | 说明 |
|---------|---------|------|
| `ADD_TABLE` | 新增表 | 创建新的数据表 |
| `ADD_COLUMN` | 新增列 | 向现有表添加新列 |
| `MODIFY_COLUMN` | 修改列 | 修改列的类型、默认值或约束 |
| `DROP_COLUMN` | 删除列 | 从表中移除列（SQLite 需要重建表） |
| `ADD_INDEX` | 新增索引 | 创建新的索引 |
| `DROP_INDEX` | 删除索引 | 删除现有索引 |
| `ADD_TRIGGER` | 新增触发器 | 创建新的触发器 |
| `DROP_TRIGGER` | 删除触发器 | 删除现有触发器 |
| `ADD_FTS` | 新增全文检索 | 创建 FTS5 虚拟表 |
| `DATA_MIGRATION` | 数据迁移 | 仅涉及数据变更，不涉及结构变更 |
| `REBUILD_TABLE` | 重建表 | 需要重建整个表（如移除 CHECK 约束） |

### 兼容级别定义

| 级别 | 中文名称 | 说明 |
|-----|---------|------|
| `BACKWARD_COMPATIBLE` | 向后兼容 | 旧版本代码可正常运行，新列可为空或有默认值 |
| `FORWARD_COMPATIBLE` | 向前兼容 | 新版本代码可读取旧版本数据 |
| `BREAKING` | 不兼容 | 需要同时升级代码和数据库，否则会出错 |
| `MIGRATION_REQUIRED` | 需要迁移 | 需要执行数据迁移脚本 |

### 迁移状态

| 状态 | 说明 |
|-----|------|
| `PENDING` | 待执行 |
| `APPLIED` | 已应用 |
| `DEPRECATED` | 已废弃 |
| `ROLLBACK` | 已回滚 |

---

## 数据库概览

| 数据库名称 | 文件名 | 当前版本 | 版本定义位置 | 用途 |
|-----------|--------|---------|-------------|------|
| 主数据库 | `mistakes.db` | **v41** | `src-tauri/src/database/mod.rs` | 错题、笔记、聊天记录等核心数据 |
| VFS 数据库 | `vfs.db` | **v35** | `src-tauri/src/vfs/schema_migration/config.rs` | 虚拟文件系统、资源管理、教材、向量索引 |
| Chat V2 数据库 | `chat_v2.db` | **v18** | `src-tauri/src/chat_v2/database.rs` | 新版对话系统 |
| LLM Usage 数据库 | `llm_usage.db` | **v1** | `src-tauri/src/llm_usage/database.rs` | LLM 使用统计 |
| Resources 数据库 | `resources.db` | **v1** | `src-tauri/src/resources/database.rs` | 独立资源库 |

> **注意**：`textbooks.db` 已迁移到 VFS 数据库中（`vfs.textbooks + vfs.blobs`），不再作为独立数据库维护。

---

## 主数据库 (mistakes.db)

**当前版本**: v41  
**版本文件**: `src-tauri/src/database/mod.rs`  
**迁移目录**: `src-tauri/src/database/`

### v41 (当前版本)

- **变更类型**: SKIP (空迁移)
- **影响表**: 无
- **变更内容**: 原计划为 mm_page_embeddings.indexing_mode，但 mm_page_embeddings 表已废弃，本版本跳过
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: 无（空迁移）
- **责任人**: 数据治理改造
- **应用日期**: 2026-01-29

### 变更记录模板

```markdown
### vXX (YYYY-MM-DD)

- **变更类型**: ADD_TABLE / ADD_COLUMN / MODIFY_COLUMN / ...
- **影响表**: `table_name`
- **变更内容**: 详细描述变更内容
- **兼容级别**: BACKWARD_COMPATIBLE / BREAKING / MIGRATION_REQUIRED
- **迁移SQL**: 
  ```sql
  ALTER TABLE xxx ADD COLUMN yyy TEXT DEFAULT '';
  ```
- **回滚SQL**: 
  ```sql
  -- SQLite 不支持 DROP COLUMN，需要重建表
  ```
- **责任人**: @username
- **PR/Issue**: #xxx
- **备注**: 其他需要说明的内容
```

---

## VFS 数据库 (vfs.db)

**当前版本**: v35  
**版本文件**: `src-tauri/src/vfs/schema_migration/config.rs`  
**迁移目录**: `src-tauri/src/vfs/migrations/`

### v35 (当前版本)

- **变更类型**: ADD_COLUMN, ADD_TABLE
- **影响表**: `questions`, `question_sync_conflicts`, `question_sync_logs`, `exam_sheets`
- **变更内容**: 题目集同步冲突策略
  - 新增 `questions.sync_status`, `questions.last_synced_at`, `questions.remote_id`, `questions.content_hash` 列
  - 新增 `question_sync_conflicts` 表（同步冲突记录）
  - 新增 `question_sync_logs` 表（同步日志）
  - 新增 `exam_sheets.sync_enabled` 列
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/036_question_sync.sql`
- **责任人**: 待填写

### v34

- **变更类型**: ADD_TABLE
- **影响表**: `review_plans`, `review_history`, `review_stats`
- **变更内容**: 复习计划与间隔重复系统（SM-2 算法）
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/035_review_plans.sql`

### v33

- **变更类型**: ADD_FTS
- **影响表**: `questions_fts`（FTS5 虚拟表）
- **变更内容**: 题目全文检索支持
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/034_questions_fts.sql`

### v32

- **变更类型**: ADD_INDEX
- **影响表**: `files`
- **变更内容**: 修复 files 表 content_hash 唯一索引
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/033_files_content_hash_unique.sql`

### v31

- **变更类型**: REBUILD_TABLE
- **影响表**: `files`
- **变更内容**: files 表迁移重构
- **兼容级别**: MIGRATION_REQUIRED
- **迁移SQL**: `src-tauri/src/vfs/migrations/032_files_migration.sql`

### v30

- **变更类型**: ADD_COLUMN
- **影响表**: `attachments`, `folder_items`
- **变更内容**: 附件与文件夹集成
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/031_attachment_folder_integration.sql`

### v29

- **变更类型**: ADD_COLUMN
- **影响表**: `vfs_embedding_dims`
- **变更内容**: 向量维度-模型绑定
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/030_embedding_dims_model_binding.sql`

### v28

- **变更类型**: DATA_MIGRATION
- **影响表**: 多表
- **变更内容**: 废弃旧索引字段
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/029_deprecate_old_index_fields.sql`

### v27

- **变更类型**: ADD_TABLE
- **影响表**: `vfs_index_units`, `vfs_index_segments`, `vfs_embedding_dims`
- **变更内容**: VFS 统一索引架构
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/028_unified_index.sql`

### v26

- **变更类型**: ADD_TABLE
- **影响表**: `memory_config`
- **变更内容**: 记忆配置表
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/027_memory_config.sql`

### v25

- **变更类型**: ADD_TABLE
- **影响表**: `questions`, `question_history`, `question_bank_stats`
- **变更内容**: 智能题目集独立数据层
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/026_question_bank.sql`

### v24

- **变更类型**: ADD_TABLE
- **影响表**: `mindmaps`
- **变更内容**: 知识导图表
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/vfs/migrations/025_mindmaps.sql`

### v1-v23

> 详细历史记录请参考 `src-tauri/src/vfs/schema_migration/config.rs` 中的 `MIGRATIONS` 数组。

---

## Chat V2 数据库 (chat_v2.db)

**当前版本**: v18  
**版本文件**: `src-tauri/src/chat_v2/database.rs`  
**迁移定义**: 内嵌在 `database.rs` 中

### v18 (当前版本)

- **变更类型**: ADD_TABLE, ADD_COLUMN, ADD_INDEX
- **影响表**: `chat_v2_session_groups`, `chat_v2_sessions`
- **变更内容**: 会话分组支持
  - 新增 `chat_v2_session_groups` 表（分组配置、默认技能、System Prompt）
  - `chat_v2_sessions` 新增 `group_id` 列
  - 新增分组与会话索引
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/migrations/chat_v2/V20260204__session_groups.sql`
- **责任人**: 会话分组改造
- **应用日期**: 2026-02-03

### v17

- **变更类型**: REBUILD_TABLE
- **影响表**: `chat_v2_messages`
- **变更内容**: 移除 `chat_v2_messages.role` 列的 CHECK 约束，支持 system/tool 角色
  - 原约束：`CHECK(role IN ('user', 'assistant'))`
  - Rust `MessageRole` 枚举已扩展：User, Assistant, System, Tool
  - SQLite 不支持直接修改 CHECK，需要重建表
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/chat_v2/migrations/017_extend_role_check.sql`
- **责任人**: 数据治理改造
- **应用日期**: 2026-01-30

### v16

- **变更类型**: ADD_TABLE
- **影响表**: `sleep_block`, `subagent_task`
- **变更内容**: Agent 协作系统睡眠块持久化
  - 新增 `sleep_block` 表（持久化主代理的睡眠状态）
  - 新增 `subagent_task` 表（记录需要恢复的子代理任务）
- **兼容级别**: BACKWARD_COMPATIBLE
- **迁移SQL**: `src-tauri/src/chat_v2/migrations/016_sleep_blocks.sql`
- **责任人**: Agent 协作系统

### v15

- **变更类型**: ADD_COLUMN, DATA_MIGRATION
- **影响表**: `chat_v2_session_state`
- **变更内容**: 手动激活 Skill ID 列表（多选支持）
  - 新增 `active_skill_ids_json` 列
  - 将旧的单选数据从 `active_skill_id` 迁移到新的多选字段
- **兼容级别**: BACKWARD_COMPATIBLE

### v14

- **变更类型**: ADD_COLUMN
- **影响表**: `chat_v2_session_state`
- **变更内容**: 新增 `active_skill_id` 列（手动激活 Skill ID 持久化）
- **兼容级别**: BACKWARD_COMPATIBLE

### v13

- **变更类型**: ADD_COLUMN
- **影响表**: `chat_v2_session_state`
- **变更内容**: 新增 `loaded_skill_ids_json` 列（渐进披露 Skills 状态持久化）
- **兼容级别**: BACKWARD_COMPATIBLE

### v12

- **变更类型**: ADD_TABLE, ADD_COLUMN
- **影响表**: `workspace_index`, `chat_v2_sessions`
- **变更内容**: 工作区索引表和会话扩展（Agent 协作系统）
- **兼容级别**: BACKWARD_COMPATIBLE

### v11

- **变更类型**: ADD_TABLE
- **影响表**: `chat_v2_todo_lists`
- **变更内容**: TodoList 持久化（支持消息中断后继续执行）
- **兼容级别**: BACKWARD_COMPATIBLE

### v10

- **变更类型**: ADD_COLUMN, ADD_INDEX
- **影响表**: `chat_v2_blocks`
- **变更内容**: 新增 `first_chunk_at` 列用于块的精确排序
- **兼容级别**: BACKWARD_COMPATIBLE

### v9

- **变更类型**: REBUILD_TABLE, DROP_INDEX
- **影响表**: `chat_v2_sessions`
- **变更内容**: 移除 subject 字段（文档28清理）
- **兼容级别**: MIGRATION_REQUIRED

### v8

- **变更类型**: ADD_INDEX
- **影响表**: `chat_v2_messages`
- **变更内容**: 性能优化索引
- **兼容级别**: BACKWARD_COMPATIBLE

### v7

- **变更类型**: ADD_COLUMN
- **影响表**: `chat_v2_sessions`
- **变更内容**: 会话简介字段（`description`, `summary_hash`）
- **兼容级别**: BACKWARD_COMPATIBLE

### v6

- **变更类型**: REBUILD_TABLE
- **影响表**: `resources`
- **变更内容**: 资源库扩展（移除 CHECK 约束，添加 `storage_mode`, `updated_at`）
- **兼容级别**: MIGRATION_REQUIRED

### v5

- **变更类型**: ADD_TABLE
- **影响表**: `resources`
- **变更内容**: 资源库表（统一上下文注入系统）
- **兼容级别**: BACKWARD_COMPATIBLE

### v4

- **变更类型**: ADD_COLUMN
- **影响表**: `chat_v2_session_state`
- **变更内容**: 上下文引用支持（`pending_context_refs_json`）
- **兼容级别**: BACKWARD_COMPATIBLE

### v3

- **变更类型**: ADD_COLUMN, ADD_INDEX
- **影响表**: `chat_v2_messages`, `chat_v2_blocks`
- **变更内容**: 变体支持（多模型并行执行）
- **兼容级别**: BACKWARD_COMPATIBLE

### v2

- **变更类型**: ADD_TABLE, ADD_COLUMN
- **影响表**: `chat_v2_session_mistakes`, `chat_v2_attachments`, `chat_v2_session_state`
- **变更内容**: Schema 对齐（会话-错题关联、块级附件、完整 ChatParams）
- **兼容级别**: BACKWARD_COMPATIBLE

### v1

- **变更类型**: ADD_TABLE
- **影响表**: `chat_v2_sessions`, `chat_v2_messages`, `chat_v2_blocks`, `chat_v2_attachments`, `chat_v2_session_state`
- **变更内容**: Chat V2 初始 Schema
- **兼容级别**: N/A (初始版本)

---

## LLM Usage 数据库 (llm_usage.db)

**当前版本**: v1  
**版本文件**: `src-tauri/src/llm_usage/database.rs`  
**迁移目录**: `src-tauri/src/llm_usage/migrations/`

### v1 (当前版本)

- **变更类型**: ADD_TABLE
- **影响表**: `schema_version`, `llm_usage_logs`, `llm_usage_daily`
- **变更内容**: 初始化 Schema
  - `schema_version` - 版本管理表
  - `llm_usage_logs` - 使用日志主表
  - `llm_usage_daily` - 每日汇总表
- **兼容级别**: N/A (初始版本)
- **迁移SQL**: `src-tauri/src/llm_usage/migrations/001_init.sql`

---

## Resources 数据库 (resources.db)

**当前版本**: v1  
**版本文件**: `src-tauri/src/resources/database.rs`

### v1 (当前版本)

- **变更类型**: ADD_TABLE
- **影响表**: `schema_version`, `resources`
- **变更内容**: 初始化 Schema
  - `schema_version` - 版本管理表
  - `resources` - 资源存储表（无 type CHECK 约束，支持任意类型）
- **兼容级别**: N/A (初始版本)

---

## 变更提交规范

### 提交前检查清单

- [ ] 更新 `CURRENT_DB_VERSION` / `CURRENT_SCHEMA_VERSION` 常量
- [ ] 创建对应版本号的迁移 SQL 文件
- [ ] 更新本文档的变更记录
- [ ] 迁移 SQL 需要是幂等的（可重复执行）
- [ ] 添加必要的列存在性检查
- [ ] 测试向后兼容性
- [ ] 更新相关的 checksum（如有）

### 命名规范

- 迁移文件：`{version_number}_{name}.sql`，如 `036_question_sync.sql`
- 版本号：三位数字，如 `001`, `035`
- 名称：使用 snake_case，简洁描述变更内容

---

## 附录：SQLite 特殊处理

### 1. 无法直接删除列

SQLite 不支持 `ALTER TABLE DROP COLUMN`（3.35.0+ 版本支持但不建议使用）。
需要通过重建表的方式删除列：

```sql
-- 1. 创建新表（不包含要删除的列）
CREATE TABLE new_table (...);

-- 2. 迁移数据
INSERT INTO new_table SELECT col1, col2, ... FROM old_table;

-- 3. 删除旧表
DROP TABLE old_table;

-- 4. 重命名新表
ALTER TABLE new_table RENAME TO old_table;

-- 5. 重建索引
CREATE INDEX ...;
```

### 2. 幂等迁移

使用 `IF NOT EXISTS` 和条件判断确保迁移可重复执行：

```sql
-- 创建表（幂等）
CREATE TABLE IF NOT EXISTS my_table (...);

-- 添加列（需要先检查）
-- 在 Rust 代码中检查列是否存在，再执行 ALTER TABLE
```

### 3. 外键约束

启用外键约束需要在每个连接建立时执行：

```sql
PRAGMA foreign_keys = ON;
```

---

*最后更新: 2026-01-30*
