# Agent 工具面总览

> 更新日期：2026-07-20  
> 运行时事实源：`src/features/chat/skills/builtin-tools/*.ts` 的 `embeddedTools`，
> 以及 `src-tauri/src/chat_v2/tools/*_executor.rs`。本文记录产品边界、
> 通用契约与领域索引；字段级 JSON Schema 以运行时事实源为准，禁止在本文另造第二套契约。

## 1. 通用契约

### 1.1 渐进披露

首轮只提供 `load_skills`。模型加载某个 builtin skill 后，才获得该 skill 的
`embeddedTools`。技能不再通过 `allowedTools` 限制其他工具；该字段仅作为旧
SKILL.md 包的兼容元数据保留。后端 executor、前端 skill schema、skill 工作流说明
任一缺失，都不算可用能力。

### 1.2 敏感度与审批

| 级别 | 含义 | 典型操作 |
| --- | --- | --- |
| Low | 只读或无实质副作用，通常直接执行 | list/get/search/stats/observe |
| Medium | 普通写入或会触发外部计算；是否审批服从用户策略 | 创建、更新、软删、生成内容 |
| High | 破坏性、系统级或难以恢复；谨慎/宽松档必须逐项审批，Full Access 档仅受不可覆盖守卫约束 | 永久删除、批删、备份/同步、全量覆盖、workbench undo；本地 shell 的写/网络/管道/脚本及未知命令 |

具体调用的运行时事实源是
`ToolExecutorRegistry::get_sensitivity_for_call(tool_name, arguments)`，不是本文或前端清单。
`ToolExecutorRegistry::describe_risk_for_call` 可供后端设置/查询面复用同一事实源，返回该次调用的
`baseSensitivity`、执行器是否按参数动态分级的 `dynamic`，以及 `protected`/
`neverRemember` 审批约束。动态工具必须带真实参数查询，不能只读取名称级默认值。
外部 MCP 工具默认 Medium；名称语义为 shell/execute-command 或文件 write/delete/patch/
append/create 的外部 MCP 工具为 High。上述 shell/文件调用同时受精确 scope 与
never-remember 规则约束。

审批预设只在 Craft（做一做）模式生效；Ask/Plan 的只读与计划门禁始终优先：

| 预设 | Craft 行为 |
| --- | --- |
| Cautious | Low 自动执行；Medium/High/Unknown 逐项审批，不记住批准 |
| Relaxed（默认） | Low/Medium 自动执行；High/Unknown 逐项审批 |
| Full Access | 普通工具免审批；本地 shell 仍受 runtime roots、平台沙箱、网络策略及不可覆盖命令守卫约束 |
| Danger Full Access | 普通工具免审批；本地 shell 显式取消文件/网络沙箱，但仍保留超时、进程树清理、输出上限、敏感环境变量过滤与不可覆盖命令守卫 |

不可覆盖命令守卫在 tool loop 和最终 spawn 前各检查一次：可识别的灾难命令（包括常见
shell/PowerShell/cmd 字面包装）直接拒绝；强推、基础设施变更、shell 动态/编码执行等要求
一次性人工确认，即使处于 Full Access 也不旁路。递归删除若指向确定的根/HOME/受保护根
（含 `~/*`、`$HOME`、`$(pwd)` 等已知展开）直接拒绝；指向无法静态解析的展开（如
`rm -rf "$TARGET"`、未知 cwd 下的裸 `*`）降级为一次性确认；指向具体子路径
（如 `rm -rf target/debug`、`~/proj/node_modules`）不额外拦截，由 runtime root
校验兜底。该守卫不是任意代码的形式化证明：动态生成的
脚本内容无法可靠静态判定；Full Access 依靠 OS 沙箱继续限制影响面，Danger Full Access
则由用户显式接受宿主机风险。外部 MCP 在独立进程/远端内部执行的行为不受本地 shell
守卫或沙箱保证。

批量删除、永久删除及覆盖性操作必须先用 `builtin-ask_user` 明确对象、数量和后果。
密钥、OAuth token、WebDAV/S3/FTP 密码不得出现在工具参数、回执或日志中。

当前审批 UI 统一只提供单次允许/拒绝，不再展示实际执行链未稳定消费的“本会话允许/始终允许”。
后端兼容 remember 时，只有 `approval_scope.rs` 明确列入 always-confirm/never-remember 的家族
强制每次确认且禁止复用：权限升级（skill install/workshop apply、skill remove、skill trust
request（grant）、MCP server propose/update/remove、custom_agent apply/remove、runtime
root request、automation propose）、高危 Workbench、领域破坏性操作、DSTU purge、backup/sync，
以及按具体参数判定的 shell/高风险外部 MCP。`skill_set_enabled`、`mcp_server_set_enabled`
为 Medium、scope 绑定“目标 + 启停方向”，不在该 never-remember 清单。`index_rebuild`、
`memory_export_all` 虽为 High，当前也不属于该清单；不得仅凭 High 标签虚构更严格策略。

### 1.3 用户可见消息 i18n

新增或改造的 executor 用户消息统一返回：

```json
{
  "messageKey": "chat.tools.domain.event",
  "messageParams": { "id": "..." },
  "messageFallback": {
    "zh-CN": "可直接阅读的中文回退",
    "en-US": "Readable English fallback"
  },
  "message": "可直接阅读的中文回退 / Readable English fallback"
}
```

- `messageKey` 是稳定机器键；不得拼接实体 ID 或自由文本。
- `messageParams` 必须是对象，只放渲染参数，不放凭据或大段正文。
- 前端优先按当前语言解析 `messageKey + messageParams`；无词条时使用对应
  `messageFallback`；旧消费者及 LLM 可直接读取双语 `message`。
- 结构化错误同样保留 `code/retryable`，并携带上述消息字段。
- executor 最终失败边界使用 `ensure_localized_error`：已有完整领域错误保留原 message key/fallback
  并校准机器参数；legacy JSON 保留实际 `code` 并补齐消息字段；裸字符串才写入最多 2000
  字符的 `detail`，避免复制或泄露大型结构化对象。`messageParams.code` 必须与顶层实际
  `code` 一致。
- 引用标签 `[知识库-N]`、`[图片-N]`、`[记忆-N]`、`[搜索-N]` 是协议标记，不翻译；
  检索 executor 的 citation guide 同时给出双语可读说明。

题库工具使用 QuestionDocument v2：共享 `materials`、`groups` 和独立 `questions` 同事务提交。
`qbank_import_document(mode=preview)` 只创建草稿，明确导入用 `mode=commit`；需要复核时仍不自动提交。
对话命题直接使用当前对话模型组织结构化参数，不额外调用默认出题模型；后台委托继承本轮可信模型快照。
只有 `committed=true` 回执可以报告已创建，并分别读取 `imported_count` 与 `total_questions`。
材料、组不计为题；`partial`、`needs_review`、未匹配答案和覆盖不足须如实说明。
题目引用兼容 `[题目集:exam_id#q_id:名称]` 和题组 `#qgroup_id` 定位；JSON/ZIP 导出保留材料关系。

当前已按此约定改造 `user_todo_executor`、`qbank_executor`、`paper_save_executor` 和
`builtin_retrieval_executor`。公共 helper 位于 `chat_v2/tools/arg_utils.rs`。

### 1.4 返回与错误

- 读列表默认单页不超过 20；返回 `total/page/page_size/has_more/truncated` 或等价字段。
- 单个长文本字段最多返回 2000 字符，并用 `fieldsTruncated` 或字段级 `truncated` 标明。
- 更新类工具在底层支持时要求 `expected_updated_at`/`expectedVersion`；冲突后必须重新读，
  禁止盲重试。
- 常见错误族：`INVALID_ARGUMENT`、`*_NOT_FOUND`、`*_OCC_REQUIRED`、`*_CONFLICT`、
  `*_UNAVAILABLE`、`*_FAILED`、`APPROVAL_REQUIRED`、`CANCELLED`、`RESULT_UNKNOWN`。
  精确 code 和 `retryable` 以 executor 回执为准；不得仅解析自然语言 message。

## 2. 领域工具索引

下表覆盖 builtin skill 中公开的工具。参数为工作流摘要，完整 required/enum/range 见对应
skill schema；返回列说明主要可观测结果。`L/M/H` 分别表示 Low/Medium/High。

| 领域 | 工具 | 主要参数与返回 | 敏感度 / 错误族 |
| --- | --- | --- | --- |
| 用户确认 | `builtin-ask_user` | 问题、选项；返回用户选择/文本 | L；`INVALID_ARGUMENT` |
| 资源读取 | `builtin-folder_list`, `builtin-resource_list`, `builtin-resource_search`, `builtin-resource_read` | folder/resource、查询、分页/页码；返回完整可分页内容与位置 | L；`RESOURCE_*` |
| DSTU 组织 | `builtin-dstu_folder_create`, `builtin-dstu_folder_rename`, `builtin-dstu_rename`, `builtin-dstu_move`, `builtin-dstu_delete`, `builtin-dstu_restore`, `builtin-dstu_list_trash`, `builtin-dstu_set_favorite`, `builtin-dstu_purge`, `builtin-dstu_upload_file` | 路径/ID、目标文件夹、版本；返回实体、路径、软删/恢复信息 | list/favorite L，purge H，其余 M；`DSTU_*` |
| 会话与分组 | `builtin-session_list`, `builtin-session_get`, `builtin-session_get_messages`, `builtin-session_search`, `builtin-session_stats`, `builtin-session_rename`, `builtin-session_move`, `builtin-session_batch_move`, `builtin-session_batch_tag`, `builtin-session_batch_ops`, `builtin-session_tag_add`, `builtin-session_tag_remove`, `builtin-session_archive`, `builtin-session_restore`, `builtin-session_export`, `builtin-group_list`, `builtin-group_create`, `builtin-group_update`, `builtin-tag_list_all` | session/group/tag、日期范围、分页、导出格式；消息分页默认且最大 20 条，正文/块摘要等单字段最多 2000 字符并标记 `truncated`，thinking 块不回显；Markdown 导出仅返回最多 2000 字符预览及 `totalChars/truncated`，`format=note` 将完整内容无损写入指定 folder | 读 L，普通组织/restore/export M，archive H；`SESSION_*` |
| 题库 | `builtin-qbank_list`, `builtin-qbank_list_questions`, `builtin-qbank_get_question`, `builtin-qbank_search_questions`, `builtin-qbank_get_stats`, `builtin-qbank_get_learning_trend`, `builtin-qbank_get_activity_heatmap`, `builtin-qbank_get_knowledge_stats`, `builtin-qbank_get_daily_practice`, `builtin-qbank_get_check_in_calendar`, `builtin-qbank_get_next_question`, `builtin-qbank_create_question`, `builtin-qbank_update_question`, `builtin-qbank_toggle_favorite`, `builtin-qbank_delete_questions`, `builtin-qbank_submit_answer`, `builtin-qbank_ai_grade`, `builtin-qbank_start_timed_practice`, `builtin-qbank_generate_mock_exam`, `builtin-qbank_submit_mock_exam`, `builtin-qbank_generate_paper`, `builtin-qbank_generate_variant`, `builtin-qbank_reset_progress`, `builtin-qbank_export`, `builtin-qbank_import_document`, `builtin-qbank_batch_import` | exam/card、筛选、OCC、用户答案、试卷配置；返回 bounded question、统计、UI handoff、文件路径/内容 | 读 L，写/grade M，批删 H。`start_timed_practice`/`generate_mock_exam` 只生成未持久化 handoff，`submit_mock_exam` 只校验 UI 结果并计算未持久化成绩单，`generate_variant` 只读原题并返回生成提示，四者当前均 L；`QBANK_*` |
| 待办 | `builtin-user_todo_list_lists`, `builtin-user_todo_list_items`, `builtin-user_todo_get_summary`, `builtin-user_todo_search`, `builtin-user_todo_list_trash`, `builtin-user_todo_create_item`, `builtin-user_todo_update_item`, `builtin-user_todo_complete_item`, `builtin-user_todo_delete_item`, `builtin-user_todo_create_list`, `builtin-user_todo_update_list`, `builtin-user_todo_delete_list`, `builtin-user_todo_restore`, `builtin-user_todo_reorder` | list/item、提醒、重复、父项、分页、OCC；成功返回完整实体、previous、restoreWith，`TODO_CONFLICT` 返回 bounded `current/currentUpdatedAt` | 读 L，写/软删 M，删清单 H；`TODO_*` |
| Agent 内部任务 | `builtin-todo_init`, `builtin-todo_get`, `builtin-todo_add`, `builtin-todo_update` | 当前 agent 任务清单；返回步骤状态 | 由 executor 定义；`TODO_*`，不等同用户待办 |
| 笔记 | `builtin-note_list`, `builtin-note_search`, `builtin-note_read`, `builtin-note_create`, `builtin-note_append`, `builtin-note_replace`, `builtin-note_set`, `builtin-note_delete`, `builtin-note_update_tags` | note/folder、正文、范围、OCC；返回正文、previous、资源位置 | 读 L，create/append/delete/tags M，replace/set H；`NOTE_*`, `WORKBENCH_UNAVAILABLE` |
| 思维导图 | `builtin-mindmap_create`, `builtin-mindmap_update`, `builtin-mindmap_delete`, `builtin-mindmap_edit_nodes`, `builtin-mindmap_versions`, `builtin-mindmap_diff_versions` | mindmap、节点 ops、版本；返回持久化回执/差异 | 读 L，写按 executor；`MINDMAP_*`, `RESULT_UNKNOWN` |
| 翻译 | `builtin-translate_text`, `builtin-translation_save` | 文本、语言、术语、结果 ID/folder；返回完整译文或 VFS translation/resource ID | translate L，save M；`TRANSLATION_*` |
| 教材/PDF | `builtin-textbook_bookmarks`, `builtin-textbook_highlights`, `builtin-pdf_page_image` | action、教材 ID、页码、批注矩形/颜色、OCC；返回书签/划线或 bounded 图片 | get/image L，批注写 M；`TEXTBOOK_*`, `PDF_*` |
| RAG 索引 | `builtin-index_status`, `builtin-index_rebuild` | status 可选 `resource_id` 与 `page/page_size<=20`，返回全局 summary；指定资源时还返回 `indexState`、分页 Unit、OCR/提取文本与 2000 字符预览截断标记。rebuild 必填 `resource_id`、可选 `folder_id`，返回 `status=indexed`、chunks、embeddingDim、durationMs，并发出 `vfs-index-progress` | status L，rebuild H；`INVALID_ARGUMENT`, `RESOURCE_NOT_FOUND`, `DEPENDENCY_UNAVAILABLE`, `CANCELLED`, `INDEX_STATUS_FAILED`, `INDEX_REBUILD_FAILED` |
| 网页存档 | `builtin-webpage_save` | 必填完整 `url/content`，可选 title/content_type/folder_id；正文最多 1,000,000 字符且归档 Markdown 最多 4 MiB。返回 fileId/resourceId/blobHash、unitsCreated、`indexQueued`、`deduplicated`，并发 DSTU 事件 | M；`INVALID_ARGUMENT`, `INCOMPLETE_WEB_FETCH`, `FOLDER_NOT_FOUND`, `DEPENDENCY_UNAVAILABLE`, `CANCELLED`, `WEBPAGE_SAVE_FAILED` |
| 学习总览 | `builtin-learning_overview`, `builtin-pomodoro_today_stats`, `builtin-pomodoro_daily_stats` | 严格日期范围或 days、分页；activityTotals/focusTotals/daily 按区间，题库、FSRS、SM-2 为调用时当前快照；返回 `partial/sourceErrors` | 全部 L；`INVALID_ARGUMENT`, `SOURCE_UNAVAILABLE`, `POMODORO_QUERY_FAILED` |
| 复习计划 | `builtin-review_get_due`, `builtin-review_stats`, `builtin-review_plan_generate`, `builtin-review_schedule`, `builtin-review_submit`, `builtin-review_suspend`, `builtin-review_resume`, `builtin-review_delete` | plan/question、筛选、评分结果、版本；返回计划/统计 | 读 L，写 M，delete H；`REVIEW_*` |
| 记忆/画像 | `builtin-memory_list`, `builtin-memory_read`, `builtin-memory_write`, `builtin-memory_write_batch`, `builtin-memory_write_smart`, `builtin-memory_update_by_id`, `builtin-memory_delete`, `builtin-memory_batch_move`, `builtin-memory_add_relation`, `builtin-memory_remove_relation`, `builtin-memory_update_tags`, `builtin-memory_export_all`, `builtin-learner_profile_get`, `builtin-learner_profile_update` | 新增工具分别接收最多 20 条 `note_ids + target_folder_path + expected_updated_at_by_id`、双端 note ID/双 OCC、note ID/tags/OCC，或 `page/page_size<=20`；返回逐项结果、当前双向关系、写后标签/版本、精确 inverse，或每条正文最多 2000 字符的分页全量导出 | write/update/delete/move/relation/tags M，export H。delete 是 VFS 软删，可从 VFS 回收站恢复，但没有 Agent restore，且删除时清理的入向关系标签不会自动重建；新增工具错误为 `MEMORY_INVALID_ARGS`, `MEMORY_CONFLICT`, `MEMORY_NOT_FOUND`, `MEMORY_OPERATION_FAILED` |
| 作文 | `builtin-essay_grade`, `builtin-essay_grade_status`, `builtin-essay_grade_wait`, `builtin-essay_get_result`, `builtin-essay_list_sessions`, `builtin-essay_list_results`, `builtin-essay_list_modes` | grade 必填 text，可选来自 list_modes 的内置/自定义/覆盖 `mode_id`、已启用非嵌入 `model_config_id`、topic/session 等；返回 task/session/round，wait/status/get 返回真实异步终态或完整批改。图片作文走 attachment_stage -> dstu_upload_file -> document_parse/status -> resource_read -> grade | grade M，其余 L；未知模式稳定返回 `ESSAY_MODE_NOT_FOUND`（Validation details 含 messageKey/fallback），其他失败当前尚无跨操作统一 code |
| Automation | `builtin-automation_propose`, `builtin-automation_list`, `builtin-automation_set_enabled`, `builtin-automation_update`, `builtin-automation_delete`, `builtin-automation_run_now`, `builtin-automation_runs`, `builtin-automation_retry_run`, `builtin-automation_cancel_run` | schedule/prompt/type/id；set_enabled/update/delete/run_now 必须携带 list 返回的正整数 `expected_version`；runs 使用 `page/page_size<=20`；成功返回最新定义、删除前快照或真实 run 状态；缺版本返回 OCC required，版本冲突返回当前值，必须重新 list 后再规划 | list/runs L，enable/update/run/retry/cancel M，propose/delete H；`AUTOMATION_OCC_REQUIRED`, `AUTOMATION_VERSION_CONFLICT`, `AUTOMATION_*` |
| 设置 | `builtin-settings_get`, `builtin-settings_set`, `builtin-model_assignments_get`, `builtin-model_assignments_set` | 白名单 key/prefix、公开值、模型分配；返回脱敏值与事件回执 | get L，set M；`SETTINGS_*`, `MODEL_ASSIGNMENTS_*` |
| 用量 | `builtin-llm_usage_query` | action、日期、粒度、分页；返回 summary/trends/by-model/by-caller/recent | L；`USAGE_*`, `INVALID_ARGUMENT` |
| 数据治理 | `builtin-backup_status`, `builtin-backup_job_status`, `builtin-sync_status`, `builtin-backup_create`, `builtin-sync_run` | 分页、job ID、同步方向/冲突策略；返回本地目录、真实终态、partial | status L，create/run H；`BACKUP_*`, `SYNC_*` |
| 学术检索 | `builtin-arxiv_search`, `builtin-scholar_search`, `builtin-paper_save`, `builtin-cite_format` | query/DOI/arXiv/url、论文元数据、格式；返回来源、VFS file ID、引文 | 搜索/格式 L，save 写入按 executor；`PAPER_*`, `SEARCH_*` |
| RAG/网络 | `builtin-unified_search`, `builtin-web_search`, `builtin-web_fetch` | query/url、top-k；返回编号来源、readResourceId、双语 citation guide 或网页正文 | L；`RETRIEVAL_*`, `FETCH_*` |
| 附件/解析 | `builtin-attachment_list`, `builtin-attachment_read`, `builtin-attachment_stage`, `builtin-document_parse`, `builtin-document_parse_status` | attachment/路径、文档 ID；返回 staged ID、文本或异步解析状态 | 读 L，stage M；`ATTACHMENT_*`, `DOCUMENT_*` |
| DOCX | `builtin-docx_create`, `builtin-docx_read_structured`, `builtin-docx_get_metadata`, `builtin-docx_extract_tables`, `builtin-docx_replace_text`, `builtin-docx_to_spec` | staged/path、spec、替换项；返回结构、表格、产物 | 读 L，写按 executor；`DOCX_*` |
| PPTX | `builtin-pptx_create`, `builtin-pptx_read_structured`, `builtin-pptx_get_metadata`, `builtin-pptx_extract_tables`, `builtin-pptx_replace_text`, `builtin-pptx_to_spec` | staged/path、spec、替换项；返回结构、表格、产物 | 读 L，写按 executor；`PPTX_*` |
| XLSX | `builtin-xlsx_create`, `builtin-xlsx_read_structured`, `builtin-xlsx_get_metadata`, `builtin-xlsx_extract_tables`, `builtin-xlsx_edit_cells`, `builtin-xlsx_replace_text`, `builtin-xlsx_to_spec` | staged/path、sheet/range/cells/spec；返回结构、表格、产物 | 读 L，写按 executor；`XLSX_*` |
| 图片生成 | `builtin-image_generate` | prompt/尺寸/参考图；返回生成资产 | 写入/外部调用按 executor；`IMAGE_*` |
| 模板 | `builtin-template_list`, `builtin-template_get`, `builtin-template_preview`, `builtin-template_validate`, `builtin-template_create`, `builtin-template_fork`, `builtin-template_update`, `builtin-template_delete` | template/spec/ID；返回模板、预览、校验/变更 | 读 L，写 M，delete H/不可恢复；`TEMPLATE_*` |
| Workbench | `builtin-workbench_get_capabilities`, `builtin-workbench_list_windows`, `builtin-workbench_observe`, `builtin-workbench_query_state`, `builtin-workbench_wait_for`, `builtin-workbench_open_app`, `builtin-workbench_app_command`, `builtin-workbench_act`, `builtin-workbench_act_high`, `builtin-workbench_close_window`, `builtin-workbench_undo` | app/window、动作批次、期望条件、undoToken；返回权威 observation、done/undone、一次性 token | 读 L，普通 act M，高危/close/undo H；`WORKBENCH_*`, `UNDO_*`, `RESULT_UNKNOWN` |
| Browser | `builtin-browser_open`, `builtin-browser_navigate`, `builtin-browser_back`, `builtin-browser_snapshot`, `builtin-browser_click`, `builtin-browser_type`, `builtin-browser_scroll`, `builtin-browser_close` | page/selector/text/方向；返回浏览器状态/截图语义 | 平台与动作分级见 schema；`BROWSER_*` |
| Workspace/子代理 | `builtin-workspace_create`, `builtin-workspace_create_agent`, `builtin-subagent_call`, `builtin-workspace_get_context`, `builtin-workspace_set_context`, `builtin-workspace_query`, `builtin-workspace_send`, `builtin-workspace_read_document`, `builtin-workspace_update_document`, `builtin-workspace_file_list`, `builtin-workspace_file_read`, `builtin-workspace_file_write`, `builtin-workspace_file_move`, `builtin-workspace_file_delete`, `builtin-workspace_artifact_write`, `builtin-workspace_change_revert`, `builtin-coordinator_sleep`, `builtin-local_shell_preflight`, `builtin-local_shell_execute`, `builtin-skill_scan`, `builtin-skill_install`, `builtin-runtime_root_request`, `builtin-tool_pack` | subagent_call 单 Task 委托：必填 task，可选 workspace_id（缺省后端自动创建工作区并注册当前会话为 coordinator，返回 auto_created_workspace=true）、profile（自由字符串：worker 默认/explorer 只读检索面/default，或 {appData}/workspaces/agents/*.md 自定义 profile 的 name，未知值报错并列出可用 profile）、resume_agent_session_id（续跑：传首次返回的 agent_session_id 并配合其 workspace_id，向同一子代理会话追问，返回 resumed=true/false）、skill_id（legacy 别名）、model、JSON context、wait（默认 true 阻塞等待，750s 预算内直接返回 output/output_truncated；wait=false 立即返回 ids 供 fan-out）；返回 workspace_id、agent_session_id、task_message_id、run_id、status、output、profile_id、resumed、token_usage（可能为 null 的 camelCase TokenUsage 对象，与 workspace_agent_completion 事件一致）等；其唯一生产 schema 是 workspace skill 的 TypeScript embeddedTools，不保留 Rust 重复 schema。coordinator_sleep 定位为等待 wait=false 派发的子代理，不再是创建 Worker 后的必需步骤。其余工具返回消息、文件变更、审批计划、任务状态 | subagent_call M；当前失败为自然语言错误、无稳定 code。Workspace FS 的 list/read L，artifact_write/file_write M，file_move/file_delete/change_revert H；其他工具按各自 executor 与动态参数分级；`WORKSPACE_*`, `SHELL_*`, `ROOT_*` |
| ChatAnki 制卡闭环 | `builtin-chatanki_run`, `builtin-chatanki_start`, `builtin-chatanki_status`, `builtin-chatanki_wait`, `builtin-chatanki_get_cards`, `builtin-chatanki_analyze`, `builtin-chatanki_list_templates`, `builtin-chatanki_import_apkg`, `builtin-chatanki_add_cards`, `builtin-chatanki_update_card`, `builtin-chatanki_delete_card`, `builtin-chatanki_retemplate`, `builtin-chatanki_control`, `builtin-chatanki_check_anki_connect`, `builtin-chatanki_export`, `builtin-chatanki_sync` | 输入材料/附件或 APKG、document/card/version、模板映射、控制动作与导出格式；返回真实异步任务、分页完整卡片、版本化变更、APKG/JSON 产物或 AnkiConnect 终态。写卡后必须 get_cards 复核；导出/同步/入队须用户明确意图 | import/export/sync M，其余当前 executor 为 L；兼容 CardForge 的 `builtin-anki_export_cards` 同按数据外发定为 M；返回 `status/error/output`，细分错误见 `docs/anki-agent-tools.md`，没有跨工具统一稳定 code |
| ChatAnki 卡库/FSRS | `builtin-chatanki_list_library_cards`, `builtin-chatanki_update_library_card`, `builtin-chatanki_delete_library_card`, `builtin-chatanki_enqueue_library_review`, `builtin-chatanki_set_library_suspended`, `builtin-chatanki_undo_library_last_review`, `builtin-chatanki_enqueue_review`, `builtin-chatanki_review_stats`, `builtin-chatanki_undo_last_review`, `builtin-chatanki_set_suspended` | library scope 跨会话访问完整 live 卡片库；list 支持 search/templateId/schedule/filter 与最多 20 条/页。内容写使用字符串 `version`，复习写使用独立整数 `reviewVersion`，delete 同时校验二者且未入队显式传 null；library enqueue 对 1..100 张卡全批 CAS。返回明确 `ratingAvailableToAgent=false` | list/update/enqueue_review/stats L；library enqueue、两种 suspend M；两种 undo 与 library delete H。冲突为 `version_conflict` / `review_state_conflict`；不存在/blocked 语义见 `docs/anki-agent-tools.md`，评分工具不暴露 |
| 自检/扩展 | `builtin-self_inspect`, `builtin-mcp_server_propose`, `builtin-skill_workshop_propose`, `builtin-skill_workshop_apply` | capability/server/skill draft；返回脱敏自检、提案或应用结果 | inspect L，提案/应用按 executor；`SELF_INSPECT_*`, `SKILL_*`, `MCP_*` |
| 技能生命周期 | `builtin-skill_set_enabled`, `builtin-skill_remove`, `builtin-skill_trust_request` | skill_id、enabled 方向；trust 先 `action=inspect`（只读现扫，返回整包 SHA-256 + 风险信号）再 `action=grant`（原样携带 `expected_package_sha256`/`declared_risk_level`）；返回启停回执、删除回执（连带清理 provenance/信任记录）或绑定指纹的信任授予 | set_enabled M（scope 绑定 skill+方向，可 remember）；remove、trust grant H、never-remember；trust inspect L。指纹失配 fail-closed；builtin 技能可停用不可删除。executor：`skill_lifecycle_executor.rs` |
| MCP 管理 | `builtin-mcp_server_update`, `builtin-mcp_server_set_enabled`, `builtin-mcp_server_remove` | server_id（id 或名称）、要改的字段（凭据红线：拒绝 `env` 明文，`env_required` 只收变量名）、enabled 方向、remove 必填 `expected_transport`（执行期复核）；返回脱敏 server 摘要、自动连测/失败回滚结果；写入后经 `chat_v2://settings_changed` 通知前端重连 | set_enabled M（scope 绑定 server+方向，可 remember）；update/remove H、never-remember；`MCP_*`。executor：`mcp_manage_executor.rs`，与 Settings 开关共享 `mcp.tools.list` 的 `enabled` 字段 |
| 子代理 persona | `builtin-custom_agent_list`, `builtin-custom_agent_get`, `builtin-custom_agent_propose`, `builtin-custom_agent_apply`, `builtin-custom_agent_remove` | file_name（`<小写字母/数字/连字符>.md`）、persona 完整 Markdown（frontmatter 需合法 name，≤64KB）；propose 写 `{workspaces}/agents-pending/` 提案区，apply 原样携带 `proposal_id + expected_content_sha256 + expected_proposal_revision` 后原子落盘 `workspaces/agents/`；返回提案/落盘/删除回执，persona 每次 `subagent_call` 现扫即时生效 | list/get L，propose M，apply/remove H、never-remember；三重指纹 + TOCTOU 复核 fail-closed。executor：`custom_agent_executor.rs` |

本地 shell 敏感度（`builtin-local_shell_execute`，按 `command` 动态分级）：

| 级别 | 条件 | Craft+宽松默认 |
| --- | --- | --- |
| Medium | 结构化纯只读家族（如 `ls`/`cat`/`rg`/`git status|log|diff|show`、PowerShell `Get-ChildItem` 等），且无管道/重定向/脚本包装，且命令守卫为 Allow | 静默执行 |
| High | 写文件、网络、管道/操作符、脚本解释器、未知可执行文件，或命令守卫 Ask/Deny | 逐项审批 |

名称级默认仍为 High（无参数时 fail-closed）。`local_shell_preflight` 为 Low。

本地 shell 平台可用性（`builtin-local_shell_preflight` / `builtin-local_shell_execute`）：

| 平台 | 沙箱后端 | 说明 |
| --- | --- | --- |
| macOS | `macos_seatbelt`（`/usr/bin/sandbox-exec` + `/bin/sh -c`） | 系统自带，无需安装 |
| Windows | `windows_appcontainer_job`（System32 Windows PowerShell，`-NoProfile -NonInteractive`） | 系统自带 |
| Linux 桌面 | `linux_bwrap`（bubblewrap `bwrap` + `/bin/sh -c`；全盘 `--ro-bind` 只读 + 可写根重挂 + protected roots 遮蔽 + 默认 `--unshare-net` 断网） | 需系统安装 `bubblewrap` 包；未安装时 preflight 标记 blocked 并给出安装指引，execute fail-closed 拒绝无沙箱执行 |
| Android/iOS | `unavailable` | 不支持本地 shell（Android 的 target_os 是 `android`，不会误入 linux 分支） |

Danger Full Access 使用独立后端：macOS/Linux 为带进程组与 rlimit 的
`danger_unsandboxed`，Windows 为保留 Job Object 限制的 `danger_unsandboxed_job`；
二者均不宣称文件系统或网络隔离。Full Access 绝不回退到该后端，平台沙箱不可用时
fail-closed。

事实源：`src-tauri/src/chat_v2/tools/shell_sandbox.rs`、`context.rs` 的
`local_shell_contract_for_platform`、`local_shell_preflight_executor.rs` 的运行时 capability 探测。

## 3. 撤销覆盖矩阵

`reversible=true` 只表示回执给出当前可执行的精确逆操作；不能把“理论上可手工改回”描述为
自动撤销。所有逆操作仍服从自身敏感度、OCC 和审批。

| 领域/操作 | 当前可逆性 | 撤销方式 | 结论 |
| --- | --- | --- | --- |
| Workbench `act` | 有条件可逆 | 原样消费一次性 `undoToken`；High；可能为 persistent/session durability | 已接入统一 undo；冲突不得覆盖用户新修改 |
| Workspace 文件写/移动/删除 | 有条件可逆 | `builtin-workspace_change_revert` 使用回执 change ID/备份 | 已有领域撤销；受 root、备份生命周期和 OCC 限制 |
| DSTU 软删 | 可逆 | `builtin-dstu_restore`，使用软删实体 ID | 已闭环 |
| DSTU purge | 不可逆 | 无 | High，必须明确确认 |
| 会话 archive | 可逆 | `builtin-session_restore` | 已闭环；会话硬删不开放 |
| 用户待办 item/list 软删 | 可逆 | 回执 `restoreWith` -> `builtin-user_todo_restore` | 已闭环；删除清单本身为 High |
| 用户待办 create item | 可逆 | 回执 `restoreWith` 指向带最新版本的软删 | 已闭环 |
| 用户待办 update/complete | 非自动可逆 | 回执含 `previous`；部分字段三态/状态工具不能无损重放 | 不接统一 undo，禁止宣称一键恢复 |
| 题库 toggle favorite | 可逆 | 回执精确反向 toggle + 最新 OCC | 已闭环 |
| 题库 create/update | 有限 | create 的逆操作是 High 批删；update 用 `previous` 人工构造且需最新 OCC | 不等于无条件 undo |
| 题库批删/reset/import/作答 | 当前不可自动撤销 | 无 Agent 恢复或会影响历史统计 | 文档明确不可撤 |
| 笔记/思维导图经 Workbench | 有条件可逆 | ACR ledger/undoToken | 使用统一 undo；直接 VFS 回落不自动等价 |
| 翻译保存 | 可逆到软删 | 回执指向 `builtin-dstu_delete`，之后可从回收站恢复 | 两段式可恢复 |
| 教材书签/划线更新 | 非一键 | OCC + previous/最新 metadata 可人工反向写 | 尚未接 Workbench undo |
| Automation 定义修改/删除/run_now | 当前不可自动撤销 | 无；run_now 的通知/任务副作用不可收回 | 删除 High，运行前说明副作用 |
| 设置/模型分配 | 当前不可自动撤销 | 重新读取后显式写回旧值 | 未接统一 undo |
| 备份/同步 | 不可自动撤销 | 无；恢复备份不开放 | High；以真实终态为准 |
| 记忆移动/关系/标签 | 可逆 | batch_move 对每个成功项返回带写后 OCC 的反向 move；add/remove_relation 返回反向关系调用；update_tags 返回旧用户标签和写后 OCC | 已有领域级 inverse；必须逐项使用最新版本，尚未接 Workbench undo |
| 记忆删除/画像/导出 | 删除可人工恢复但非无损 Agent undo | 删除是 VFS 软删，可走 VFS 回收站恢复；没有 Agent restore，且已清理的入向关系标签不会自动重建。画像可重新读取后显式修正；export 只读但会暴露大量隐私 | delete 为 M，profile 暂不接统一 undo；export 每页 H 审批 |
| Anki/FSRS | 专用有限撤销 | `chatanki_undo_last_review` 按当前会话所有权撤销；`chatanki_undo_library_last_review` 按全库作用域撤销。两者都要求最新 `reviewVersion + logId` | 详见 `docs/anki-agent-tools.md`；撤销不是重评，Agent 评分仍不开放 |

### 3.1 接入 `workbench_undo` 的评估

适合接入的下一批领域是教材批注、普通设置与具备稳定 `previous + OCC` 的题库/待办更新。
前提是 inverse 能跨窗口持久化、验证目标未被用户后改，并与领域事件刷新共用同一事务终态。
不应接入的操作包括网络通知、模型调用、同步、永久删除、导入外部文件和用户评分；这些副作用
无法由 UI ledger 完整补偿。

## 4. 明确不暴露的操作

本轮（2026-07-20）起，以下操作已改为经审批工具正门暴露，不再属于本清单：技能停用/删除/信任
（`skill_set_enabled`/`skill_remove`/`skill_trust_request`）、MCP server 修改/启停/删除
（`mcp_server_update`/`mcp_server_set_enabled`/`mcp_server_remove`）、自定义子代理 persona 写入/删除
（`custom_agent_propose` → `custom_agent_apply`/`custom_agent_remove`）。三族的删除/信任/落盘均为
High + never-remember，详见第 2 节领域索引。仍然不暴露的操作如下：

| 操作 | 原因与用户路径 |
| --- | --- |
| API key、OAuth token、WebDAV/S3/FTP 凭据读写 | 密钥不得进入模型上下文；仅 Settings 安全输入/secure store。MCP 工具的凭据红线同源：`env` 明文一律拒绝，`env_required` 只收变量名，值由用户在 Settings > MCP 工具填写 |
| `purge_all_data`、备份恢复、ZIP 导入 | 可整体覆盖或清空本地数据；保持人工 Settings 流程 |
| 会话硬删除、消息底层变体编辑 | 破坏历史且当前恢复/观测契约不足；仅 archive/restore |
| FSRS `rate` 评分及平行 `builtin-fsrs_*` 工具组 | Again/Hard/Good/Easy 必须由用户本人选择；Agent 可用 ChatAnki 会话级与全库 list/update/enqueue/受约束撤销、暂停、删除能力，但不开放 `fsrs_rate` 或任何 library rate/score 工具，也不再造一套重复工具 |
| 未经用户提供的题目答案、模拟考代答 | `agentCanAnswer=false`；Agent 只汇总用户在题库 UI 的真实作答 |
| 任意系统设置键、内部 OAuth key | `settings_set` 使用显式低风险白名单；内部键在 Rust 层硬拒绝。MCP 配置（`mcp.tools.list`）的增改启停删只准走 `mcp_server_*` 正门工具，禁止经 `settings_set`/shell/文件工具直改 |
| 绕过正门直写技能/persona 目录 | `~/.deep-student/skills/` 与 `workspaces/agents/` 只准经各自的提案+审批工具写入；shell 侧门由 deny 规则封死 |
| 未授权本地目录、任意 shell | Cautious/Relaxed/Full Access 由 runtime root、平台沙箱与命令守卫共同约束；Danger Full Access 经二次确认后显式取消文件/网络隔离，但不取消不可覆盖命令守卫 |
| 浏览器凭据导出 | 浏览器工具只操作当前受控页面，不回传密码、cookie 或 session secret |

## 5. 学习总览契约

`builtin-learning_overview` 默认读取本地今天在内的 7 天；显式
`start_date/end_date` 必须同时提供、严格 `YYYY-MM-DD`、结束不晚于今天、最多 90 天。
日明细最多 20 条/页；`activityTotals/focusTotals` 覆盖完整请求范围，不受 `daily` 分页影响。
`questionBank/fsrsReview/sm2Review` 是调用时的当前库存/调度快照，不是该日期范围内的历史增量；
回答“这周”类问题时不得把这些快照描述成仅发生在本周的数据。

热力图、番茄钟、题库、FSRS 或 SM-2 任一来源失败时返回 `partial=true` 和
`sourceErrors[]`。调用方必须点名缺失来源，不能把不可用数据当作零。番茄钟时长单位为秒。

## 6. 本阶段死代码处理

- 删除 `testApi.ts` 的 `getMistakeDetails/updateMistake/runtimeAutosaveCommit` 存根，以及只为它们
  存在且全仓无入口的 `ChatSaveTestPanel`/`chat-save-tests` 开发套件。
- 仅删除 `commands.rs` 中恒定返回全零且无调用方的 `calculate_review_analysis_stats`；保留
  `review_analyses` 表、迁移和真实趋势查询 `calculate_review_trend`。
- `AnkiConnectSettingsSection` 不与现有设置 UI 重复：此前只有配置加载/保存，没有可见控制面；
  现已接入 Settings > General。
- 删除未使用的 `get_coordinator_sleep_tool_schema` 函数/导出；保留
  `CoordinatorSleepExecutor` 和 workspace skill 中的正式 schema。

## 7. 后续工作

以下项目明确挂起，不在本任务书实施：

1. macOS/Linux 浏览器控制桥；当前结果型 browser 工具仍受平台实现约束。
2. 窗口/桌面截图工具及可靠的跨窗口视觉定位。
3. 音频/视频转写与媒体时间轴引用；附件工具当前只承诺 image/document。
4. 剪贴板读写工具及敏感内容防泄漏策略。
5. 需要题库 UI 深度状态协作的持久化模拟考会话、计时恢复和跨重启续考。
6. 将更多稳定 `previous + OCC` 领域接入持久化 `workbench_undo`，并提供统一变更摘要 UI。
7. 为所有存量 executor 的非 `message` 用户提示（hint/undoReason/progress）逐步补齐相同双语契约。
