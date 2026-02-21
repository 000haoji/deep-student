# 后端 Rust 技术债清单

> 生成时间: 2026-02-21 | 基于代码审计的完整清单，按优先级排列

---

## P0 — 全局警告抑制治理

### 1. `lib.rs` 全局 `#![allow]`（影响整个 crate 的 335 个 .rs 文件）

```
src-tauri/src/lib.rs:6-11
```

| 抑制项 | 影响 | 建议 |
|--------|------|------|
| `#![allow(unused_variables)]` | 遮盖未使用参数 | 移除，局部用 `_` 前缀或 `#[allow]` |
| `#![allow(unused_assignments)]` | 遮盖无效赋值 | 移除，修复实际问题 |
| `#![allow(unused_imports)]` | 遮盖多余 use 语句 | 移除，`cargo fix` 自动清理 |
| `#![allow(dead_code)]` | 遮盖所有未使用代码 | 移除，逐模块标注真正需保留的 |
| `#![allow(static_mut_refs)]` | 遮盖不安全静态引用 | 移除，改用 `OnceLock`/`LazyLock` |
| `#![allow(private_interfaces)]` | 遮盖 pub 函数暴露私有类型 | 移除，修正可见性 |

**操作步骤**:
1. 先移除 `unused_imports`，运行 `cargo fix --allow-dirty` 自动清理
2. 移除 `dead_code`，逐个模块处理编译错误（标注真正需要保留的用 `#[allow(dead_code)]`）
3. 移除 `unused_variables`，用 `_` 前缀标注有意忽略的参数
4. 最后处理 `static_mut_refs` 和 `private_interfaces`

### 2. 文件级 `#![allow]`（11 个文件）

| 文件 | 抑制项 | 建议 |
|------|--------|------|
| `persistent_message_queue.rs:1-2` | `unused_variables`, `dead_code` | 审计是否有未使用代码可删 |
| `dstu/handlers.rs:1-2` | `unused_variables`, `dead_code` | 6,267 行大文件，拆分后再治理 |
| `notes_exporter.rs:1-2` | `unused_variables`, `dead_code` | 审计并移除 |
| `lance_vector_store.rs:1-2` | `unused_variables`, `unused_assignments` | 审计并移除 |
| `database_optimizations.rs:1-2` | `unused_variables`, `dead_code` | 审计并移除 |
| `llm_manager/mod.rs:1-2` | `unused_variables`, `unused_assignments` | 4,053 行，拆分后治理 |
| `tools/web_search.rs:1-5` | `clippy::*`, `dead_code`, `unused_imports` | 2,970 行，有 5 条抑制最多 |
| `vfs/lance_store.rs:11-12` | `unused_variables`, `dead_code` | 审计并移除 |
| `database/mod.rs:1` | `unused_variables` | 5,860 行大文件 |
| `commands.rs:1` | `non_snake_case` | ✅ 合理（Tauri camelCase 要求） |
| `cmd/notes.rs:4` | `non_snake_case` | ✅ 合理（Tauri camelCase 要求） |

---

## P1 — ★ 废弃注释清理

### 3. `lib.rs` 中的废弃注释（40 处）

`src-tauri/src/lib.rs` 第 30-106 行密集分布了 **40 处** `★...已删除/已废弃/已移除` 注释。这些是历史清理记录，信息存在于 git history 中。

需删除的注释行（按行号）：
- L30: `// ★ structured_backup 已删除（2026-02-05 废弃功能清理）`
- L52: `// gemini_adapter 已移除`
- L56-58: `// ★ backup_improved 已删除...` / `// ★ backup_test_commands 已删除...` / `// ★ backup_tests...已删除...`
- L60: `// ★ importers 模块已移除（subject 概念废弃）`
- L80: `// ★ unified_chat 模块已删除（文档31清理），改用 chat_v2`
- L81: `// learning_hub 模块已废弃，改用 DSTU/VFS 统一资源访问`
- L85: `// ★ user_memory 模块已废弃（2026-01），改用 Memory-as-VFS`
- L91-92: `// ★ research 模块已删除` / `// ★ chat_search 模块已删除（文档31清理）`
- L94: `// ★ subject_research 模块已删除（文档31清理）`
- L100-101: `// ★ essay_grading_db 已删除...` / `// ★ canvas_board_db...已移除...`

### 4. 其他文件的废弃注释（93 处 / 32 个文件）

重灾区：
| 文件 | 数量 |
|------|------|
| `chat_v2/pipeline_tests.rs` | 16 |
| `chat_v2/tools/registry.rs` | 9 |
| `chat_v2/pipeline.rs` | 7 |
| `chat_v2/tools/executor.rs` | 6 |
| `chat_v2/prompt_builder.rs` | 5 |
| `commands.rs` | 4 |
| `chat_v2/tools/builtin_retrieval_executor.rs` | 3 |
| `cmd/notes.rs` | 3 |
| `llm_manager/model2_pipeline.rs` | 3 |
| `vfs/repos/note_repo.rs` | 3 |

**建议**: `grep -rn '★.*已废弃\|★.*已删除\|★.*已移除' src-tauri/src/` 获取完整列表后批量清理。

---

## P2 — 超大文件拆分

### 5. God Files（>3000 行，共 17 个）

| 文件 | 行数 | 职责 | 拆分建议 |
|------|------|------|---------|
| `chat_v2/pipeline.rs` | **9,338** | 聊天管线：检索→prompt→LLM→工具→持久化 | 按阶段拆分: `retrieval.rs`, `llm_caller.rs`, `tool_loop.rs`, `persistence.rs` |
| `data_governance/commands.rs` | **8,664** | 数据治理所有 Tauri 命令 | 按子功能拆分: `schema_commands.rs`, `audit_commands.rs`, `migration_commands.rs`, `backup_commands.rs` |
| `vfs/handlers.rs` | **6,972** | VFS 所有 Tauri 命令 | 按资源类型拆分: `file_handlers.rs`, `folder_handlers.rs`, `index_handlers.rs`（index_handlers 已独立但可进一步拆） |
| `dstu/handlers.rs` | **6,267** | DSTU 访达协议所有处理器 | 按操作类型拆分: `read_handlers.rs`, `write_handlers.rs`, `search_handlers.rs` |
| `database/mod.rs` | **5,860** | 数据库 SQL 操作集中 | 按表域拆分: `chat_queries.rs`, `vfs_queries.rs`, `anki_queries.rs` |
| `commands.rs` | **5,480** | 主命令注册（部分已拆到 cmd/） | 继续拆分到 `cmd/` 子模块 |
| `chat_v2/tools/chatanki_executor.rs` | **5,335** | Anki 卡片制作工具 | 拆分 prompt 构建、解析、执行到独立模块 |
| `backup.rs` | **4,645** | 备份系统（旧版） | 考虑与 `data_governance/backup/mod.rs` (3,673行) 合并或明确职责边界 |
| `llm_manager/model2_pipeline.rs` | **4,562** | OCR/题目解析管线 | 按阶段拆分: `ocr_stage.rs`, `parse_stage.rs`, `result_builder.rs` |
| `lance_vector_store.rs` | **4,473** | LanceDB 向量存储 | 拆分 CRUD 操作、迁移逻辑、查询构建 |
| `vfs/indexing.rs` | **4,372** | 向量化索引服务 | 拆分 chunking、embedding、search |
| `data_governance/migration/coordinator.rs` | **4,118** | 迁移协调器 | 拆分 planning、execution、rollback |
| `llm_manager/mod.rs` | **4,053** | LLM 管理器 | 按 concern 拆分: `config.rs`, `streaming.rs`, `model_selection.rs` |
| `data_governance/sync/mod.rs` | **3,964** | 数据同步 | 拆分 conflict_resolution、merge_strategy |
| `data_governance/backup/mod.rs` | **3,673** | 备份（数据治理版） | 与 `backup.rs` 确认职责边界 |
| `chat_v2/tools/builtin_resource_executor.rs` | **3,633** | VFS 资源工具执行器 | 按操作类型拆分 |
| `chat_v2/repo.rs` | **3,466** | 聊天仓库层 | 按实体拆分: `session_repo.rs`, `message_repo.rs`, `block_repo.rs` |

### 6. 备份系统双轨问题

项目存在**两套备份系统**：
- `src-tauri/src/backup.rs` (4,645 行) — 旧版备份
- `src-tauri/src/data_governance/backup/mod.rs` (3,673 行) — 新版数据治理备份
- `src-tauri/src/backup_common.rs` — 共享组件
- `src-tauri/src/backup_config.rs` — 配置
- `src-tauri/src/backup_job_manager.rs` — 作业管理

两套系统合计 **~8,300 行**，需要明确哪些功能已完全迁移到数据治理版，旧版中哪些可以删除。

---

## P3 — TODO/FIXME 审计

### 7. TODO/FIXME 标记（396 处 / 47 个文件）

排除 `todo_executor.rs` 中的工具字面量（182 处）后，实际技术债标记约 **214 处**。

重灾区需逐个审计：

| 文件 | 数量 | 类别 |
|------|------|------|
| `chat_v2/tools/registry.rs` | 31 | 工具注册表待扩展 |
| `chat_v2/pipeline.rs` | 22 | 管线待优化项（性能、错误处理） |
| `chat_v2/handlers/send_message.rs` | 21 | 消息发送流程待完善 |
| `vfs/indexing.rs` | 15 | 索引服务待优化（分块策略、增量更新） |
| `dstu/path_types.rs` | 10 | 路径类型系统待完善 |
| `dstu/handlers.rs` | 9 | 处理器待优化 |
| `data_governance/migration/chat_v2.rs` | 8 | 迁移脚本待完善 |
| `vfs/ref_handlers.rs` | 8 | 引用处理待优化 |
| `dstu/path_parser.rs` | 7 | 路径解析待完善 |
| `vfs/handlers.rs` | 7 | VFS 处理器待优化 |

**建议**: 对每个 TODO 分类为 "真正待办"（录入 GitHub Issue）或 "已过时"（直接删除）。

---

## P4 — 模块组织优化

### 8. 顶层模块过多

`lib.rs` 声明了 **~60 个顶层 `pub mod`**，平铺在 crate root 下。部分模块应收归到子模块中：

| 当前位置 | 建议归入 |
|---------|---------|
| `anki_connect_service.rs`, `apkg_exporter_service.rs`, `enhanced_anki_service.rs`, `streaming_anki_service.rs` | `anki/` 子模块 |
| `backup.rs`, `backup_common.rs`, `backup_config.rs`, `backup_job_manager.rs` | `backup/` 子模块（或合并入 `data_governance/backup/`） |
| `question_bank_service.rs`, `question_export_service.rs`, `question_import_service.rs`, `question_sync_service.rs`, `qbank_grading.rs` | `qbank/` 子模块 |
| `pdf_ocr_service.rs`, `pdf_protocol.rs`, `pdfium_utils.rs` | `pdf/` 子模块 |
| `error_details.rs`, `error_recovery.rs`, `workflow_error_handler.rs` | `error/` 子模块 |
| `crash_logger.rs`, `debug_commands.rs`, `debug_logger.rs` | `debug/` 子模块 |
| `file_manager.rs`, `unified_file_manager.rs` | 合并或收归到 `vfs/` |
| `lance_vector_store.rs`, `vector_store.rs` | `vector/` 子模块 |
| `notes_exporter.rs`, `notes_manager.rs` | `notes/` 子模块（或收归到 `vfs/`） |

### 9. LLM 适配器目录

`src-tauri/src/llm_manager/adapters/` 包含 13 个适配器文件：

```
anthropic.rs, deepseek.rs, doubao.rs, ernie.rs, gemini.rs,
generic_openai.rs, grok.rs, minimax.rs, mistral.rs, mod.rs,
moonshot.rs, qwen.rs, zhipu.rs
```

这部分组织良好，无需拆分。但 `llm_manager/mod.rs` (4,053 行) 本身需要拆分。

---

## P5 — 其他技术债

### 10. `commands.rs` 的 `#![allow(non_snake_case)]`

`commands.rs:1` 和 `cmd/notes.rs:4` 使用 `#![allow(non_snake_case)]` 是合理的（Tauri 2.x 要求顶层参数为 camelCase）。但建议：
- 改用 `#[allow(non_snake_case)]` 标注在**每个函数**上，而非文件级
- 或使用 `#[tauri::command(rename_all = "camelCase")]` attribute

### 11. `session_manager.rs` 的 `async_fn_in_trait`

```
src-tauri/src/session_manager.rs:4
#![allow(async_fn_in_trait)]
```

自 Rust 1.75+ 已稳定 `async fn in trait`。该 allow 可能不再需要（取决于 MSRV）。验证当前 Rust toolchain 版本后决定是否移除。

### 12. 工具执行器数量

`src-tauri/src/chat_v2/tools/` 包含 **31 个文件**（含 mod.rs）：

```
academic_search_executor.rs    knowledge_executor.rs
anki_executor.rs               memory_executor.rs
ask_user_executor.rs           mod.rs
attachment_executor.rs         paper_save_executor.rs
attempt_completion.rs          pptx_executor.rs
builtin_resource_executor.rs   qbank_executor.rs
builtin_retrieval_executor.rs  registry.rs
canvas_executor.rs             skills_executor.rs
canvas_tools.rs                sleep_executor.rs
chatanki_executor.rs           subagent_executor.rs
docx_executor.rs               template_executor.rs
executor.rs                    todo_executor.rs
executor_registry.rs           types.rs
fetch_executor.rs              workspace_executor.rs
general_executor.rs            xlsx_executor.rs
injector.rs
```

其中 `chatanki_executor.rs` (5,335 行) 和 `builtin_resource_executor.rs` (3,633 行) 过大，其余文件粒度合理。

---

## 附录: 按紧急度的操作路径

### 第一阶段（1 天）
1. 清理 `lib.rs` 中 40 处 ★废弃注释
2. `cargo fix --allow-dirty` 清理 unused_imports
3. 移除 `lib.rs` 中 `#![allow(unused_imports)]`

### 第二阶段（1 周）
4. 移除 `#![allow(dead_code)]`，逐模块修复
5. 清理其他 32 个文件中的 93 处 ★废弃注释
6. 审计 9 个文件级 `#![allow]`（排除 2 个合理的 non_snake_case）

### 第三阶段（2-4 周）
7. 拆分 `pipeline.rs` (9,338 → 4×~2,300)
8. 拆分 `data_governance/commands.rs` (8,664 → 4×~2,100)
9. 拆分 `vfs/handlers.rs` (6,972 → 3×~2,300)
10. 明确备份系统双轨职责，删除旧版冗余代码

### 第四阶段（持续）
11. 模块归类优化（anki/, qbank/, pdf/, error/ 子模块）
12. TODO/FIXME 逐个审计和清理

---

# 二轮调查新发现

> 追加时间: 2026-02-21 第二轮深入调查

---

## P1-NEW — unwrap/expect 滥用（潜在 panic 风险）

### 13. `.unwrap()` 分布（1,131 处，排除测试代码）

| 文件 | unwrap 数 | 风险等级 | 说明 |
|------|----------|---------|------|
| `chat_v2/repo.rs` | **145** | 🔴 高 | 大部分在测试区（L2280+），但需确认生产代码中的部分 |
| `data_governance/backup/mod.rs` | **109** | 🔴 高 | 备份流程中 unwrap 失败 = 备份中断 |
| `data_governance/migration/coordinator.rs` | **83** | 🟡 中 | 迁移流程（失败影响大但频率低） |
| `adapters/gemini-openai-converter.rs` | **47** | 🟡 中 | API 转换层，输入异常可 panic |
| `data_governance/sync/mod.rs` | **44** | 🔴 高 | 同步流程 unwrap = 同步中断 |
| `data_space.rs` | **43** | 🟡 中 | 数据空间管理 |
| `document_parser.rs` | **37** | 🟡 中 | 文档解析（用户上传的文件格式不可控） |
| `backup_common.rs` | **32** | 🟡 中 | 备份通用工具 |
| `vfs/database.rs` | **31** | 🟡 中 | 数据库操作 |
| `chat_v2/types.rs` | **29** | 🟡 中 | 类型转换 |

**关键 pattern**: 多数 unwrap 用于 `serde_json::to_string().unwrap()` 和 `row.get().unwrap()` — 前者在序列化已知类型时合理，后者在数据库 schema 稳定时可接受，但不够防御性。

### 14. `.expect()` 分布（410 处，排除测试代码）

| 文件 | expect 数 | 风险 | 说明 |
|------|----------|------|------|
| `vfs/database.rs` | **72** | 🟡 | 全部在测试区 |
| `chat_v2/database.rs` | **36** | 🟡 | 全部在测试区 |
| `chat_v2/resource_repo.rs` | **29** | 🟡 | 多数在测试区 |
| `llm_usage/database.rs` | **26** | 🟡 | 多数在测试区 |
| `vfs/repos/folder_repo.rs` | **23** | 🟡 | 多数在测试区 |

**好消息**: expect 多数集中在 `#[cfg(test)]` 区块，生产代码中的 expect 相对较少。

### 15. `panic!` 分布（约 20 处非测试代码）

| 文件 | 行号 | 上下文 |
|------|------|--------|
| `data_governance/commands.rs` | L8548 | `panic!("poison registry lock")` — **🔴 Mutex 毒化直接 panic** |
| `data_governance/migration/mod.rs` | L318 | `panic!(...)` — **🔴 迁移异常 panic** |
| `data_governance/migration/script_checker.rs` | L539 | `panic!("{}", msg)` — **🔴 脚本检查失败 panic** |
| `chat_v2/repo.rs` | L3222 | `panic!("不应该删除消息...")` — 断言性 panic（可用 debug_assert 替代） |
| `mcp/auth.rs` | L423 | `panic!("Expected API key token")` — 测试代码 |

**建议**:
1. `data_governance/commands.rs:8548` 的 Mutex poison panic 应改为 `Err` 返回（Mutex 毒化在多线程环境中可能发生）
2. `migration/mod.rs:318` 和 `script_checker.rs:539` 应改为返回 `anyhow::Error`
3. 其余测试中的 panic 可保留

---

## P1-NEW — unsafe 代码审计

### 16. unsafe 使用（6 处生产代码）

| 文件 | 行号 | 用途 | 风险 |
|------|------|------|------|
| `pdfium_utils.rs:24-25` | `unsafe impl Send for SyncPdfium {}` / `unsafe impl Sync for SyncPdfium {}` | 让 Pdfium 实例可跨线程共享 | 🔴 **高** — 需要确认 pdfium-render 底层是否真正线程安全。若非线程安全，这是 UB |
| `backup_common.rs:405-406` | `unsafe { libc::statvfs(...) }` | 获取磁盘可用空间 | 🟡 低 — 标准 POSIX API 调用，参数正确 |
| `backup_common.rs:456` | `unsafe { libc::statfs(...) }` | 同上（macOS 路径） | 🟡 低 |
| `lib.rs:708` | `unsafe { ... }` | 应用启动时的平台特定初始化 | 🟡 需审查上下文 |
| `mcp/global.rs:572` | `unsafe { ... }` | MCP 全局状态访问 | 🟡 需审查 |
| `ocr_adapters/system_ocr/macos.rs:17,30` | `unsafe fn recognize_text_inner` | macOS OCR API（Objective-C 互操作） | 🟡 低 — ObjC FFI 必须 unsafe |

**重点关注**: `SyncPdfium` 的 `unsafe impl Send + Sync` 是最危险的一处。若 pdfium 底层使用全局状态或不可重入函数，跨线程调用会导致数据竞争。

---

## P2-NEW — Cargo.toml 依赖债务

### 17. 注释掉的 crate 和废弃 feature

| 位置 | 内容 | 说明 |
|------|------|------|
| L58 | `# blake3 = "1.5"  # 临时禁用避免C编译问题` | "临时"禁用已持续较长时间 |
| L59 | `# keyring = "2.0"  # 已禁用：改用加密文件存储` | 已有替代方案，注释可删除 |
| L222-223 | `# neo4j = []` | 废弃 feature 占位注释 |
| L225-228 | `db_migration = []`, `http = []`, `old_migration_impl = []` | 3 个空 feature，注释说"仅为消除条件编译告警" |

### 18. vendor 目录 patch

```toml
[patch.crates-io]
lancedb = { path = "vendor/lancedb" }
object_store = { path = "vendor/object_store" }
```

使用本地 vendor patch 意味着这两个 crate 无法通过 `cargo update` 自动更新。需要文档记录：
- 为什么需要 patch（可能是 chrono/arrow 版本冲突，见 L239 注释）
- vendor 版本基于上游哪个 commit
- 何时可以回退到 crates.io 版本

### 19. 过时版本

| 依赖 | 当前版本 | 说明 |
|------|---------|------|
| `rusqlite` | 0.29.0 | 最新 0.31+，0.29 不支持 SQLite 3.45+ 特性 |
| `reqwest` | 0.11 | 最新 0.12+，0.11 分支已不积极维护 |
| `hyper` | 0.14 | 最新 1.x，0.14 是 legacy API |
| `image` | 0.24 | 最新 0.25，pdfium-render 通过 `image_024` feature 绑定了该版本 |
| `zip` | 0.6 | 最新 2.x |

**注**: 版本升级需谨慎，特别是 `rusqlite`（需验证迁移兼容性）和 `reqwest`（API 变更较大）。`image` 被 pdfium-render 的 feature flag 锁定。

---

## P2-NEW — 数据库 Schema 债务

### 20. 多数据库架构

项目使用 **4 个独立的 SQLite 数据库**，各有独立的迁移目录：

| 数据库 | 迁移目录 | 迁移文件数 |
|--------|---------|-----------|
| 主数据库 (database/mod.rs) | **内联 DDL**（无迁移文件） | — |
| VFS 数据库 | `migrations/vfs/` | 15 |
| Chat V2 数据库 | `migrations/chat_v2/` | 8 |
| LLM Usage 数据库 | `migrations/llm_usage/` | 4 |
| Mistakes 数据库 | `migrations/mistakes/` | 6 |

**问题**:
- **主数据库**的 DDL 是**内联在 Rust 代码中**的（`database/mod.rs` 有 192 处 `CREATE TABLE`），没有独立的迁移文件
- 主数据库中 `review_analyses` 和 `review_chat_messages` 的 CREATE TABLE 出现了 **3 次**（重复定义）
- `custom_anki_templates` 的 CREATE TABLE 出现了 **2 次**
- 新数据库（VFS/Chat V2/LLM Usage/Mistakes）使用 refinery 迁移框架，迁移管理规范

### 21. 内联 DDL 表清单（主数据库 database/mod.rs）

以下表的 schema 直接写在 Rust 代码中，不由迁移脚本管理：

```
schema_version, chat_messages, temp_sessions, review_analyses (x3),
review_chat_messages (x3), settings, document_tasks, anki_cards,
document_control_states (x2), migration_progress, custom_anki_templates (x2),
vectorized_data, rag_sub_libraries, review_sessions, review_session_mistakes,
search_logs, exam_sheet_sessions
```

其中带 `(xN)` 的表有 N 个重复的 CREATE TABLE 语句（分布在不同的版本迁移函数中，如 `migrate_v3_to_v4`、`migrate_v5_to_v6` 等），这是因为内联迁移的每个版本函数都完整包含该版本的建表语句。

**建议**: 长期应将主数据库也迁移到 refinery 框架，统一迁移管理方式。

---

## P3-NEW — 过度 clone

### 22. `.clone()` 调用（4,161 处，排除测试）

| 文件 | clone 数 | 说明 |
|------|---------|------|
| `chat_v2/pipeline.rs` | **363** | 管线在多个阶段间传递数据时大量 clone |
| `chat_v2/tools/chatanki_executor.rs` | **332** | Anki 卡片生成大量字符串 clone |
| `data_governance/commands.rs` | **205** | 命令处理层 clone |
| `dstu/handlers.rs` | **140** | DSTU 处理器 clone |
| `llm_manager/mod.rs` | **100** | LLM 管理 clone |

**注**: 这不一定全是问题（Rust 中 clone 常用于所有权转移），但 `pipeline.rs` 的 363 次 clone 暗示可能有通过引用传递减少分配的优化空间。配合该文件 9,338 行的体量，重构时应同步评估 clone 成本。

---

## 附录 B: 操作优先级更新

### 新增 P0 项
- 审查 `SyncPdfium` 的 `unsafe impl Send + Sync`，确认线程安全性
- 修复 `data_governance/commands.rs:8548` 的 Mutex poison panic

### 新增 P1 项
- 备份系统 (`backup/mod.rs`) 的 109 处 unwrap 应逐步替换为 `?` 或 `.map_err()`
- 清理 Cargo.toml 中 4 处注释掉的依赖和 3 个空 feature
- 记录 vendor patch 的来源和回退计划

### 新增 P2 项
- 评估 rusqlite 0.29→0.31、reqwest 0.11→0.12 升级可行性
- 主数据库 DDL 内联→refinery 迁移框架统一
- `pipeline.rs` 重构时评估 clone 优化空间
