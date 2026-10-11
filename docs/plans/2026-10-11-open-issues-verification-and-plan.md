# 开放 Issue 本地核实与分期解决计划（2026-10-11）

上游仓库 `helixnow/deep-student` 开放 issue 共 12 条（另有 4 个 PR）。本文对每条
issue 的代码断言逐条在本地 `origin/main`（工作副本 HEAD `26550e0fe` 分支
`feat/anki-latex-formula-contract`）上取证，并给出分期计划。

核实方法：`grep` 定位到具体文件行 + 只读阅读上下文；版本号/表格 schema 与
`src-tauri/migrations/`、`scripts/generate-version.mjs` 交叉核对。

---

## 一、核实结论总表

| # | 标题 | 类型 | 本地核实 | 严重度 |
|---|---|---|---|---|
| 468 | attachment_read 对 res_* 必然失败（r.content 列不存在） | bug | **属实（硬 bug，100% 复现）** | P0 |
| 465 | AI 写入的题目图片无法加载（res_* 不被读取侧接受） | bug | **属实** | P0 |
| 469 | 会话 meta_json 体量失控（before/after 重复、toolResults 重复、tail_limit=80） | bug | **属实** | P1 |
| 470 | 对话页渲染策略（原图当缩略图 / content-visibility 污染 / focus 全量翻可见） | bug | **基本属实**（「白屏=重解码」为推断） | P1 |
| 467 | AI 解析不写 explanation、且必须已有作答 | enhancement | **属实** | P2 |
| 466 | 缺只读「查看答案/解析」模式 | enhancement | **属实** | P2 |
| 464 | 检索命中页图未进入多模态上下文（retrieval_hit 无生产者） | enhancement | **属实** | P3 |
| 471 | Android 无法更新，与旧包冲突 | bug | **源码无法定论**，需设备/APK 取证 | P3 |
| 472 | Android 顶部功能键与通知栏干扰 | bug | **部分可核实**，需截图/设备 | P3 |
| 455 | Android 搜狗输入法非中文无法输入 | bug | **源码无法定论**，需机型复现 | P3 |
| 461 | 模板卡面公式只输出 MathML、依赖设备数学字体 | bug | 属实；维护者已答「下个版本修复」 | 已认领 |
| 462 | 制卡默认 prompt 缺公式契约 | bug | 已由 PR #463 修复（open, mergeable clean） | 已修 |
| 377 | 生成内容时异常卡顿 | bug | 维护者已回访（0.9.70/71 优化）；与 #469/#470 同源 | 并入 P1 |
| 57 / 46 | 云同步 / 安卓显示异常 | bug | 维护者已回访修复（0.9.73） | 跟踪 |

---

## 二、逐条取证

### #468 attachment_read 对 res_* 必然失败 — 属实，硬 bug

- 三处 SQL 写作 `COALESCE(r.content, '')`：
  `src-tauri/src/chat_v2/tools/attachment_executor.rs:522` / `:561` / `:583`。
- vfs 库 `resources` 表列名为 `data`，**没有 `content`**：
  `src-tauri/migrations/vfs/V20260130__init.sql`（`data TEXT, -- 内嵌内容（inline 模式）`）。
- 该函数用 `ctx.vfs_db` 连接（`read_context_ref_content`），**第一句就报
  `no such column: r.content`**，`res_*` 的 fallback 分支（`:555` 起）根本执行不到。
- 仓库其余调用点都写作 `COALESCE(r.data,'')`：`cmd/notes.rs:2052`、
  `vfs/repos/note_repo.rs:1202`、`chat_v2/tools/attachment_stage_executor.rs:1201`。
- 附带：`attachment_executor.rs:619` 的 `raw_content.starts_with("data:")` 分支在现
  schema 下永不命中（`resources.data` 存的是 refs JSON）。

结论：**属实**。仅把 `r.content` 改成 `r.data` 不足以修好图片——图片资源
`resources.data` 存的是 `{"refs":[…]}`，字节要走
`resources.source_id → files.blob_hash → blobs`。

### #465 AI 写入的题目图片无法加载 — 属实

- 前端：`src/components/QuestionBankEditor.tsx:1029-1067` 用 `img.id` 调
  `vfs_get_attachment_content`；`Promise.allSettled` 的 rejected 分支被忽略
  （`:1051-1053`），而 `'error'` 哨兵语义（`:1030-1035`）无任何写入方 ⇒ 永久转圈。
- 读取侧白名单：`src-tauri/src/vfs/handlers.rs:1813-1824` 只接受
  `att_/file_/tb_/img_`，其它前缀 `Err("Invalid attachment ID format: …")`。
- 写入侧：`src-tauri/src/chat_v2/tools/qbank_executor.rs:1077+`
  `parse_question_images` 只校验 `id` 是非空字符串，**不校验前缀、不校验 hash**。
- 对照（其它写入方都做对了）：
  - 裁剪入库建 `file_` 条目 + 真 blob hash，注释明写「vfs_get_attachment_content
    只接受 att_/file_/tb_ 前缀」——`src-tauri/src/commands.rs:8434-8455`；
  - 文档导入 `ensure_vfs_file_for_blob()`——`src-tauri/src/question_import_service.rs:3407`。

结论：**属实**。数据没丢（`resources.source_id → files.id → blob_hash` 可解析），
只是 ID 空间不匹配。

### #469 会话 meta_json 体量失控 — 属实

- before/after 由**同一次调用**产生，逐字节相同：
  `pipeline/persistence.rs:1336-1337`（两行都是
  `build_replay_skill_payload_snapshot(&ctx.options)`）；
  `pipeline/compaction.rs:2306-2307`（`Some(snapshot.clone())` / `Some(snapshot)`）。
- `MessageMeta` 确实同时含 `tool_results`（`types.rs:1873`）与
  `skill_runtime_before/after`，整体以 `_meta` 序列化（`types.rs:1640`）。
- 全量加载：`src/features/chat/adapters/tauri/requestHelpers.ts:7`
  `LOAD_SESSION_TAIL_LIMIT = 80`；`handlers/load_session.rs:79` 仅按 `tail_limit` 取尾页。
- 加载期额外 CPU：`core/store/restoreActions.ts:79-134`
  `getReplayRuntimeSnapshots()`（每条消息最多 4 份快照）+ `remapLegacyBuiltinToolName()`
  对**每个** `mcp_` 块遍历快照 × `mcpToolSchemas` × `skillEmbeddedTools`
  （内部含 3 次正则归一化）⇒ O(块数 × 快照数 × schema 数)。

结论：**属实**。

### #470 对话页渲染策略 — 基本属实

- **A 原图当缩略图**：`hooks/useImagePreviewsFromRefs.ts:100-271` →
  `context/imagePayload.ts:86` `buildImageDataUrl`；
  `vfs/ref_handlers.rs:154+` `vfs_resolve_resource_refs` 只有 `refs` + `vfs_db`
  两个参数，无 resize/quality/thumbnail；
  `components/ContextRefsDisplay.tsx:141`（`CONTEXT_REFS_VISIBLE_COUNT = 8`）、
  `:213-239`（`w-16 h-16` + `object-cover`）。属实。
- **B 虚拟滚动与 content-visibility 互扰**：
  `MessageList.tsx:77` `DIRECT_RENDER_MAX_CONTENT_LENGTH = 200_000`；
  `:56` `DEFAULT_ESTIMATED_ITEM_SIZE = 120`；`:603` `estimateSize`；`:610` `measureElement`；
  `:458-463` / `:864-890` `observedTop` 账本只认 `writeScroll`/`followBottom`；
  `renderers/StreamingBlockRenderer.tsx:53-56`
  `contentVisibility:'auto'` + `containIntrinsicSize:'auto 96px'`；
  `overflow-anchor: none` 仅落在 workbench 作用域
  `features/workbench/apps/chat/ChatSessionSurface.css:29`，全屏 chat 页未关。属实。
- **C focus 全量翻可见**：`StreamingBlockRenderer.tsx:64-82`
  `installContentVisibilityRefresh()` **同时**监听 `visibilitychange` 与
  `window.focus`，无平台判断，`querySelectorAll('.stream-block[style*="content-visibility"]')`
  全部先 `visible` 再下一帧 `auto`。属实。
- 报告自陈「最小化恢复白屏 = 纹理被丢弃需重新解码上传」属**推断**，未做运行时 profile。

结论：**基本属实**（一条为推断）。

### #467 AI 解析不写 explanation、且需已有作答 — 属实

- `src-tauri/src/qbank_grading/pipeline.rs:348` 注释「Analyze 模式只写 ai_feedback」；
  `persist_draft_feedback` 只更新 `ai_feedback`，不动 `explanation`。
- `src-tauri/src/chat_v2/tools/qbank_executor.rs:2939-2952` 强制要求
  `submission_id`（缺失即 `INVALID_ARGS 缺少 submission_id`）；
  `pipeline.rs:641` Analyze 分支读 `current_submission.is_correct`。
- `answer` / `explanation` / `ai_feedback` 确为独立字段。

结论：**属实**（功能缺口）。

### #466 缺只读「查看答案/解析」模式 — 属实

- `src/features/learning-hub/apps/views/ExamContentView.tsx:70` ViewMode 全集
  `list|manage|stats|favorites|practice|upload|review|sm2|tags|launcher|csvImport`，
  无 browse/answers；
- `:1250-1252` `handleQuestionClick` → `requestViewMode('practice', …)`；
  `:1269-1271` `handleViewQuestionDetail` → `handleOpenQuestion` → 同上。

结论：**属实**。

### #464 检索命中页图未进入多模态上下文 — 属实

- `pipeline/context_compiler/images.rs:21` 字段存在、`:38` 参与排序；
- 所有构造点硬编码 `false`：`types.rs:4639/4650`、`model_selection.rs:203`、
  `multi_variant.rs:3580`、`context_compiler.rs:286/312`、`user_message_builder.rs:470`、
  `images.rs:538`；全仓 `retrieval_hit: true` **0 命中**；
- 检索侧契约注释存在：`tools/builtin_retrieval_executor.rs:~398`；
- 文本模型兜底存在：`context_compiler.rs:668 compile_images_for_text_model`；
- `tools/textbook_pdf_executor.rs:~463` 把整图以 `data:` URL 塞进 JSON 返回。

结论：**属实**（能力缺口：契约有注释、无生产者）。

### #471 Android 无法更新、与旧包冲突 — 源码无法定论

- `identifier` 稳定：各 tag 均为 `com.deepstudent.app`，排除包名变更。
- versionCode 在正常路径下**单调递增**（`scripts/generate-version.mjs`
  `semverOrdinal = major*10^7 + minor*10^5 + patch`，基准 `0.9.55`→14641）：
  0.9.61=14647 → 0.10.5=114591。
- **可疑点**：`src-tauri/tauri.conf.json` 的静态 `bundle.android.versionCode = 14641`
  **小于** 0.9.61 生成值 14647。`reusable-build-android.yml:414` 注释说明
  `tauri android build` 会先用静态值重写 `gen/android/app/tauri.properties`，再靠
  `beforeBuildCommand`（`generate-version.mjs`）写回生成值。**若某次构建该 hook 未生效**，
  APK 会带 14641，从 0.9.61（14647）升级即 versionCode 回退 ⇒ Android 拒装。
- 另一常见成因：**签名不一致**（不同渠道/密钥签名的 APK 无法覆盖安装）。

结论：**无法从源码确认**。取证步骤：对已发布的 0.10.5 与 0.9.61 APK 执行
`aapt dump badging`（比对 versionCode）与 `apksigner verify --print-certs`
（比对签名指纹）。

### #472 Android 顶部功能键与通知栏干扰 — 部分可核实

- `src-tauri/mobile/android/MainActivity.kt:63-78` 已 `enableEdgeToEdge()` 并监听
  `systemBars() or displayCutout()`，换算 CSS px 注入 `__DEEP_STUDENT_SET_SAFE_AREA__`
  （`:180-193`）；`src/utils/platform.ts:135-138` 写入 `--android-safe-area-*`。
- 未逐一核对「顶部功能键」是否消费 `--android-safe-area-top`；截图宽高比
  （2797×593）为平板横屏，横屏 cutout 的 left/right 是否被顶栏消费是嫌疑点。
- 维护者已回复：vivo 未开放相应 API，暂建议用工作台模式。

结论：**部分可核实**，需原图与设备复现。

### #455 Android 搜狗输入法非中文无法输入 — 源码无法定论

- 清单已注入 `android:windowSoftInputMode="adjustResize"`
  （`reusable-build-android.yml:275-279`）。
- 现象（非中文输入无回显、输入法抖动、切回中文也失效需重启）指向 WebView IME
  composition 在特定 ROM/输入法下的问题，源码侧无直接可判定点。维护者仅追问机型，
  无后续。

### #461 模板卡面公式字体 — 已认领

- 事实成立：模板卡面走 `renderCardFaceLatexHtml(text, 'mathml')`
  （`src/components/anki/AnkiTemplateCardFace.tsx:146` →
  `src/components/anki/utils/cardFaceLatex.ts:102`），只输出 MathML；卡面沙箱 CSP
  为 `font-src data:`（`src/components/previews/htmlSandboxPolicy.ts:102`）且
  `sanitizeCssForPreview()` 删除所有 `@font-face`（同文件 `:50`）⇒ 字形只能来自设备。
- 维护者 2026-10-11 回复「ok，下个版本修复」。
- 注：本项目此前决定不做鸿蒙/卓易通适配；但报告人已把该条重构为**平台无关的卡面
  渲染健壮性**问题（内联 KaTeX 字体子集），与鸿蒙适配无关，可独立实现。

### #462 制卡公式契约 — 已由 PR #463 修复

PR #463（`feat/anki-latex-formula-contract`，open，mergeable_state=clean）已统一
`LATEX_RULES`/`LATEX_OUTPUT_BRIEF` 到 `src-tauri/src/prompt_contracts.rs`，新增
`bare_latex` 质检并联动 7 处契约。

---

## 三、分期解决计划

原则：先修「数据读不出来」的硬 bug（用户数据已在库、影响面确定），再治体感性能，
最后补功能与平台问题。每期独立可发布，且每期都必须先跑测试再构建（RULES #8）。

### P0 — 附件 ID 空间与解析（#468 + #465，1 个 PR）

**目标**：AI 写入/引用的图片与附件能被模型和 UI 正确读回。

1. 收敛「附件 ID → 内容」为**单一解析函数**（issue #468 也建议），统一
   `attachment_list` / `attachment_read` / `vfs_get_attachment_content` 三处口径：
   - 修正三处 `COALESCE(r.content,'')` → `r.data`；
   - 图片资源：`resources.source_id → files.blob_hash → blobs` 取字节 → base64，
     mime 取 `files.mime_type`；
   - 找不到 blob 时返回明确错误码（如 `BLOB_MISSING`），不吞成
     `ATTACHMENT_OPERATION_FAILED`。
2. 写入侧归一化：`parse_question_images`（`qbank_executor.rs:1077+`）对 `res_*` 解析为
   可加载的 `file_*`（复用 `ensure_vfs_file_for_blob` 语义），无法归一化则显式报错，
   不落库。
3. 读取侧容错：`vfs_get_attachment_content`（`handlers.rs:1813`）接受 `res_*`。
4. 前端可见化：`QuestionBankEditor.tsx:1051` rejected 分支写 `'error'` 哨兵 +
   失败占位，消除永久 spinner。
5. 回归测试：以 `res_*` 图片引用为输入断言返回 base64；断言 `r.content` 不再出现。

**验收**：issue #465 / #468 的复现步骤不再复现；`cargo test` 相关模块全绿。

### P1 — 会话加载与渲染性能（#469 + #470，建议拆 2 个 PR）

**目标**：打开含图/长会话不再卡顿、上滚不回弹。

1. **先做 #469 的「去冗余 + 按需加载」**（issue #469 评论指出这会让 #470 的回弹
   大概率一同好转）：
   - `before`/`after` 相同时只存一份（或存 hash 引用）；
   - `toolResults` 不再进 `meta_json`（`blocks.tool_output_json` 已有内容），或只存
     block id 列表；
   - `chat_v2_load_session` 按用途裁剪 `_meta`：首屏只给
     `modelId/chatParams/usage/contextSnapshot`，技能回放快照改为重放/重试时按消息拉取；
   - `remapLegacyBuiltinToolName` 预建「工具名 → 是否本地内置」索引，消除
     O(块×快照×schema) 的重复扫描。
2. **再做 #470**：
   - 缩略图通道：给 `vfs_resolve_resource_refs` 加 `purpose`（`preview`/`model`）与目标
     尺寸，或写入 `files.preview_json`（列已存在，`migrations/vfs/V20260130__init.sql`）；
   - 前端按 `resourceId` 共享预览缓存，优先 `blob:` URL 而非 data URL；
   - `installContentVisibilityRefresh` 限定平台（仅 WKWebView）或只对视口内块生效；
   - 测量行高时临时禁用块级 `content-visibility`，或给 `estimateSize` 内容长度上界；
     把 virtualizer 的滚动补偿纳入 `observedTop` 账本；
   - 全屏 chat 页也关闭原生 scroll anchoring（`overflow-anchor: none`）。

**验收**：构造 issue 所述形状的会话（26 条消息 / 227 块 / 9 张照片），测量首屏 IPC
体量与上滚回弹；#469 报告人给的只读测量 SQL 可直接复用。

### P2 — 题库功能补齐（#467 + #466，1 个 PR）

1. **#467**：让 `qbank_ai_grade` 的 `mode: analyze` 允许省略 `submission_id`（仅按题干/
   选项生成解析），并把结果写入 `questions.explanation`；或新增
   `qbank_generate_explanation` 工具；若保留 `ai_feedback` 轨，则提供显式「采纳为解析」
   动作。保持 `answer`/`explanation`/`ai_feedback` 三者互不隐式覆盖。
2. **#466**：题目行增加只读「查看答案」入口（题干 + 选项 + 正确项高亮 + 解析，不可编辑、
   不进练习），或新增 `browse`/`answers` 视图模式；对无答案/无解析给明确空态
   （复用 `editor.noAnswerOrExplanation`）。

**验收**：未作答的题也能生成解析；题库能只读核对答案。

### P3 — 平台与大特性（按取证结果排期）

1. **#471（先取证再动手）**：
   - 对已发布的 0.10.5 / 0.9.61 APK 跑 `aapt dump badging` + `apksigner verify --print-certs`；
   - 若确认为 versionCode 回退：修构建注入链路，并**加 CI 守卫**断言
     `tauri.properties` 的 versionCode == 生成器值（防止静态 14641 泄漏）；
   - 若确认为签名不一致：核对 keystore 历史，必要时提供「先卸载再装」的用户指引。
2. **#472**：核对顶栏对 `--android-safe-area-top` 的消费；横屏 cutout 的 left/right；
   vivo 侧 API 限制需产品取舍（工作台模式是现规避）。
3. **#455**：需机型 + 输入法复现；可能需 Android WebView IME composition 兼容层。
4. **#464**：多模态检索接线，两条可独立落地——
   - L1 用户指定：引用面板「把这张图交给模型」，引用粒度从资源级扩到 page/blob 级
     （`chat_v2/resource_types.rs:316` `SendContextRef` 目前只有 `resource_id/hash/type_id`）；
   - L3 模型按需取图：给工具结果加图片通道（协议层改动），或让 `pdf_page_image` 返回
     图像内容而非 JSON 里的 data URL。
   - 共同约束：只对 VL 模型直读、注入发生在当前工具循环内（不改已冻结历史）、
     以 canonical 引用参与（base64 不落库）、受既有 image budget 限制。
5. **#461**：给模板卡面内联 KaTeX 字体子集（`KaTeX_Main-Regular/Italic` +
   `KaTeX_Math-Italic` + `KaTeX_Size1-4` ≈77KB woff2 转 data URI，CSP 已允许
   `font-src data:`）。维护者已认领「下个版本修复」，实现前建议先与其对齐归属。

---

## 四、建议的落地顺序（一句话）

P0（#468+#465 一个 PR）→ P1-a（#469 去冗余+按需加载）→ P1-b（#470 图片通道+滚动）
→ P2（#467+#466）→ P3 按取证结果（#471 取证优先，#464/#461 需与维护者对齐归属）。
