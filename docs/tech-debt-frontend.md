# 前端技术债清单

> 生成时间: 2026-02-21 | 基于代码审计的完整清单，按优先级排列

---

## P0 — 死代码清理（立即可做） ✅ 已完成

### 1. 死文件删除 ✅

已删除 9 个死文件（含 `BackendTest.tsx`）。

### 2. 生产路由中的测试/Demo 组件 ✅

已用 `import.meta.env.DEV` 包裹 `lazyComponents.tsx`、`App.tsx` 中的 3 个开发组件（TreeDragTest/CrepeDemoPage/ChatV2IntegrationTest）。
`BackendTest.tsx` 已在 P0-1 中删除。

### 3. `mcp-debug/` 目录（4,573 行） ✅

已在 `vite.config.ts` 添加 `exclude-mcp-debug` 插件，生产构建时将 `mcp-debug` 动态 import 替换为空实现。

---

## P1 — UI 组件统一化

### 4. Shadcn 组件混用全景

**总量**: 130 处 Shadcn 导入 / 64 个文件

#### 4.1 重灾区文件（≥5 个 Shadcn 导入）

| 文件 | Shadcn 导入 | 具体组件 |
|------|------------|---------|
| `src/components/practice/MockExamMode.tsx` (L14-20) | 7 | Card, Progress, Badge, Input, Label, Switch, Slider |
| `src/components/practice/PaperGenerator.tsx` (L14-20) | 7 | Card, Badge, Input, Label, Switch, Slider, Checkbox |
| `src/components/SyncConflictDialog.tsx` (L15-22) | 7 | Badge, Label, Separator, Card, Tabs, Alert, Checkbox |
| `src/components/ConflictResolutionDialog.tsx` (L15-21) | 6 | Badge, Label, Separator, Card, Tabs, Alert |
| `src/components/practice/DailyPracticeMode.tsx` (L13-17) | 5 | Card, Progress, Badge, Input, Label |
| `src/components/practice/TimedPracticeMode.tsx` (L14-18) | 5 | Card, Progress, Badge, Input, Label |

#### 4.2 按 Shadcn 组件分类的引用清单

**Card** (16 处) — 无 Notion 替代：
- `practice/MockExamMode.tsx`, `practice/PaperGenerator.tsx`, `practice/DailyPracticeMode.tsx`
- `practice/TimedPracticeMode.tsx`, `practice/PracticeModeSelector.tsx`
- `SyncConflictDialog.tsx`, `ConflictResolutionDialog.tsx`, `ReviewSession.tsx`
- `ReviewCalendarView.tsx`, `ReviewPlanView.tsx`, `QuestionFavoritesView.tsx`
- `anki/panels/MaterialQueuePanel.tsx`
- `chat-v2/components/CompletionCard.tsx`, `chat-v2/components/ToolApprovalCard.tsx`
- `chat-v2/components/panels/OcrResultCard.tsx`

**Badge** (27+ 处) — 无 Notion 替代，最广泛使用的 Shadcn 组件：
- practice/ 模块全部 5 个文件
- 题库模块: `QuestionBankListView`, `QuestionFavoritesView`, `QuestionHistoryView`, `QuestionInlineEditor`, `ReviewQuestionsView`, `VirtualQuestionList`, `TagNavigationView`
- 复习模块: `ReviewPlanView`, `ReviewCalendarView`, `ReviewSession`
- 对话模块: `SyncConflictDialog`, `anki/MaterialQueuePanel`
- chat-v2: `ModelPanel`, `MultiSelectModelPanel`, `OcrResultCard`, `ToolApprovalCard`, `AdvancedPanel`, `ChatAnkiProgressCompact`
- 其他: `PracticeLauncher`, `CsvFieldMapper`, `UnifiedModelSelector`, `essay-grading/SettingsDrawer`

**Input** (21 处) — 无 Notion 替代：
- practice/ 模块: MockExamMode, PaperGenerator, DailyPracticeMode, TimedPracticeMode
- 题库模块: QuestionBankListView, QuestionBankManageView, QuestionInlineEditor, TagNavigationView
- learning-hub: LearningHubSidebar, LearningHubToolbar, FolderTreeItem, FolderTreeView, FinderSearchBar, FinderQuickAccess, DesktopView
- chat-v2: GroupEditorDialog, ModelPanel
- 其他: ShortcutSettings, UnifiedModelSelector, UnifiedSidebar
- notes: FindReplacePanel

**Label** (12 处)：
- practice/: MockExamMode, PaperGenerator, DailyPracticeMode, TimedPracticeMode
- ConflictResolutionDialog, SyncConflictDialog, CsvImportDialog
- QuestionBankExportDialog, QuestionInlineEditor
- chat-v2: AdvancedPanel, AgentOutputDrawer, CreateAgentDialog

**Progress** (11 处)：
- practice/: MockExamMode, DailyPracticeMode, TimedPracticeMode
- ReviewSession, ReviewPlanView, CsvImportDialog, ExamSheetUploader
- ImportProgressModal, IndexStatusView, MindMapContentView
- chat-v2: ChatAnkiProgressCompact

**Skeleton** (9 处)：
- FolderSelector, SessionBrowser, CroppedExamCardImage, ExamPageImage
- ReviewPlanView, FolderTreeView
- stats: KnowledgeRadar, LearningHeatmapChart, LearningTrendChart

**Checkbox** (6 处)：
- SyncConflictDialog, QuestionBankExportDialog, QuestionBankManageView
- PaperGenerator, MaterialQueuePanel
- chat-v2: GroupEditorDialog

**Sheet** (5 处)：
- AnkiPanelHost, UnifiedSourcePanel, QuestionHistoryView
- unified-sidebar: SidebarDrawer, SidebarSheet

**Textarea** (5 处)：
- QuestionInlineEditor
- chat-v2: GroupEditorDialog, OcrResultCard, AgentOutputDrawer, CreateAgentDialog

**Switch** (3 处): MockExamMode, PaperGenerator, AdvancedPanel

**Slider** (2 处): MockExamMode, PaperGenerator （另有 `SnappySlider` 在 AdvancedPanel, RagPanel 使用）

**Alert** (3 处): SyncConflictDialog, ConflictResolutionDialog, CsvImportDialog

**Tabs** (2 处): SyncConflictDialog, ConflictResolutionDialog

**Separator** (3 处): SyncConflictDialog, ConflictResolutionDialog, ModernSidebar

**Popover** (2 处): ChatV2Page, UnifiedModelSelector

**Table** (2 处): CsvFieldMapper, QuestionBankManageView

**Collapsible** (1 处): QuestionBankExportDialog

**注**: Card/Badge/Input/Label/Checkbox/Switch/Slider/Tabs 的 Shadcn 底层组件已做过 Notion 化样式修改（`shadow-none`、`border-border/40`、`bg-transparent` 等），**视觉上已基本统一**。技术债主要是双轨体系的维护负担，而非视觉不一致。

### 5. CSS 动画变量命名不统一

- `src/styles/notion-animations.css` 定义 `--shadow-notion`, `.notion-transition` 等类
- **5 处组件引用**了 `--shadow-notion` 变量:
  - `src/components/learning-hub/components/finder/FinderFileList.tsx`
  - `src/components/learning-hub/components/finder/FinderFileItem.tsx`
  - `src/components/QuestionBankListView.tsx`
  - `src/components/UnifiedNotification.css`
  - `src/components/practice/PracticeLauncher.tsx`
- `src/styles/ds-animations.css` 定义了等价的 `--shadow-ds`, `.ds-transition`（**0 处引用**，P0 已标记删除）

---

## P2 — 超大文件拆分

### 6. 前端 God Files（>2000 行）

| 文件 | 行数 | 拆分建议 |
|------|------|---------|
| `src/components/Settings.tsx` | **4,856** | 按 Tab 拆分为 GeneralTab/ModelsTab/ParamsTab/AppTab/AboutTab（部分已拆，但主文件仍含大量逻辑） |
| `src/chat-v2/core/store/createChatStore.ts` | **4,013** | 按功能拆分：sessionSlice, messageSlice, streamSlice, toolSlice |
| `src/utils/tauriApi.ts` | **3,832** | 按模块拆分：chatApi, vfsApi, ankiApi, settingsApi。已有 **36 处** ★废弃标记待清理 |
| `src/chat-v2/pages/ChatV2Page.tsx` | **3,418** | 提取 hook：useSessionLifecycle, useChatKeyboard, useChatLayout |
| `src/chat-v2/adapters/TauriAdapter.ts` | **3,237** | 按 concern 拆分：messageAdapter, toolAdapter, contextAdapter |
| `src/components/crepe/CrepeEditor.tsx` | **2,856** | 提取 features 到独立文件（部分已在 `features/` 目录） |
| `src/chat-v2/components/input-bar/InputBarUI.tsx` | **2,541** | 提取子组件：AttachmentBar, FeatureChips, ModelSelector |
| `src/components/learning-hub/LearningHubSidebar.tsx` | **2,523** | 提取 TreeSection, SearchSection, QuickAccess |
| `src/components/QuestionBankEditor.tsx` | **2,453** | 提取 QuestionForm, OptionEditor, ExplanationEditor |
| `src/components/settings/McpToolsSection.tsx` | **2,239** | 提取 ServerList, ToolList, ConfigEditor |
| `src/components/DataImportExport.tsx` | **2,174** | 拆分 ImportPanel, ExportPanel |

### 7. CSS God File

| 文件 | 行数 | 建议 |
|------|------|------|
| `src/App.css` | **11,543** | 急需拆分。按模块提取到 `styles/` 子目录或改用 Tailwind/CSS Modules |
| `src/DeepStudent.css` | **2,975** | 与 App.css 职责重叠，合并或按模块拆分 |

---

## P3 — 类型安全与代码卫生

### 8. `any` 类型重灾区

**总量**: 821 处 / 142 个文件

| 文件 | `any` 数量 | 说明 |
|------|-----------|------|
| `src/utils/tauriApi.ts` | **70** | Tauri invoke 返回值缺类型 |
| `src/mcp/mcpService.ts` | **61** | MCP 协议动态类型 |
| `src/components/Settings.tsx` | **55** | 配置项动态类型 |
| `src/stores/researchStore.ts` | **33** | 研究模块 store |
| `src/utils/templateDowngrader.ts` | **31** | 模板降级转换 |
| `src/utils/debugLogger.ts` | **27** | 调试日志 |
| `src/App.tsx` | **24** | 主入口 |
| `src/types/index.ts` | **21** | 全局类型定义 |

### 9. 调试代码未隔离

**总量**: ~46,400 行（占前端 12.5%）

| 目录 | 文件数 | 行数 | 加载方式 |
|------|--------|------|---------|
| `src/debug-panel/` | 52 | **28,108** | `App.tsx` 中 `React.lazy` 加载 `GlobalDebugPanel` |
| `src/chat-v2/debug/` | 12 | **9,840** | 被 debug-panel plugins 引用 |
| `src/components/dev/` | 17 | **3,873** | `lazyComponents.tsx` 注册 |
| `src/mcp-debug/` | ~15 | **4,573** | `main.tsx` 动态 import（仅 dev） |

debug-panel 包含 **45 个调试插件**（AnkiGeneration, ChatAnkiParse, CrepeDragDrop, DeepSeekOcr, ExamSheetProcessing, StreamResponse, ...），其中许多是一次性调试工具。

### 10. ★ 废弃注释堆积

**总量**: 215 处 / 71 个文件

| 文件 | 数量 | 示例 |
|------|------|------|
| `src/utils/tauriApi.ts` | **36** | `// ★ xxx_api 已废弃` 等已删除函数的注释 |
| `src/App.tsx` | **18** | `// ★ 分析模式已废弃`, `// ★ MistakeItem 类型导入已废弃` 等 |
| `src/debug-panel/DebugPanelHost.tsx` | **12** | 调试面板的历史功能注释 |
| `src/chat-v2/context/definitions/index.ts` | **10** | 上下文定义中的清理记录 |
| `src/types/index.ts` | **9** | 类型定义中的历史注释 |
| `src/utils/notesApi.ts` | **7** | 笔记 API 的清理记录 |
| `src/lazyComponents.tsx` | **4** | L46, L73, L90-91 |

**建议**: 这些注释记录了 "什么被删除了"，但这些信息存在于 git history 中，不需要在代码中保留。批量删除匹配 `// ★.*已废弃|// ★.*已删除|// ★.*已移除` 的注释。

### 11. TODO/FIXME 需分类审计

**总量**: 346 处 / 75 个文件（排除字面量 TODO 工具名后约 ~240 处实际待办）

需审计的高优文件：
- `chat-v2/components/ActivityTimeline/ActivityTimeline.tsx` (55) — 55 处中大部分是 `TODO_TOOL_NAMES`/`TodoStep`/`todoBlocks` 等**变量名**，非技术债标记。实际 TODO 注释约 3-5 处
- `chat-v2/skills/builtin/index.ts` (23) — 技能定义中的待实现标记
- `mcp/builtinMcpServer.ts` (13) — 内置 MCP 工具待完善
- `chat-v2/context/vfsRefTypes.ts` (11) — 类型定义待完善
- `components/Settings.tsx` (9) — 设置页待优化

---

## 附录: Shadcn 组件当前样式状态

以下 Shadcn 基础组件**已完成 Notion 化样式改造**（确认无视觉债务）：

| 组件 | 改造内容 |
|------|---------|
| `shad/Card.tsx` | `shadow-none`, `border-border/40`, `bg-background` |
| `shad/Badge.tsx` | `bg-primary/5`, `bg-muted/10`, 移除 border |
| `shad/Input.tsx` | `border-transparent`, `bg-transparent`, `hover:bg-muted/30` |
| `shad/Textarea.tsx` | 同 Input 风格 |
| `shad/Checkbox.tsx` | `border-border/40`, `bg-transparent` |
| `shad/Switch.tsx` | `bg-muted/70`, `shadow-none`, `ring-1 ring-black/5` |
| `shad/Slider.tsx` | 需确认（可能仍有默认样式） |
| `shad/Tabs.tsx` | 需确认 |

---

# 二轮调查新发现

> 追加时间: 2026-02-21 第二轮深入调查

---

## P1-NEW — npm 依赖债务 ✅ 已完成

### 12. 疑似未使用的 dependencies ✅

已从 `package.json` 中移除 7 个未使用的包：`@antv/hierarchy`, `@emotion/is-prop-valid`, `@reactour/tour`, `remark-mermaidjs`, `remark-directive`, `react-zoom-pan-pinch`, `defuddle`。
**注**: `@sentry/browser` 保留 — `main.tsx` 中有 `await import('@sentry/browser')` 动态导入（用户开启错误报告时加载）。

### 13. devDependencies 错放到 dependencies ✅

已将 `@types/dompurify`, `@types/jsdom` 移到 devDependencies。

### 14. Radix UI 依赖分布异常 ✅

- 已将 `@radix-ui/react-checkbox`, `@radix-ui/react-slot`, `@radix-ui/react-switch` 从 devDependencies 移到 dependencies
- 已删除 `@radix-ui/react-toast`（随死代码 `shad/Toast.tsx` 一起清理）
- 已删除 `@radix-ui/react-select`（0 处引用）

### 15. 低使用率依赖（仅 1 处 import）

| 包名 | import 数 | 说明 |
|------|----------|------|
| `@zumer/snapdom` | 1 | DOM 快照工具 |
| `heic2any` | 1 | HEIC 图片转换 |
| `pptx-preview` | 1 | PPT 预览 |
| `docx-preview` | 1 | Word 预览 |
| `exceljs` | 1 | Excel 处理 |
| `diff` | 1 | 文本差异比较 |
| `yaml` | 1 | YAML 解析 |
| `immer` | 2 | 不可变数据（仅 2 处使用，zustand 的 immer 中间件可能已够用） |

这些不是"错误"，仅作为精简依赖时的参考。

---

## P2-NEW — console.log 泛滥

### 16. 生产代码中的 console.log

**总量**: **1,049 处** `console.log` 分布在生产代码中（不含测试文件）

重灾区：
| 文件 | 数量 | 说明 |
|------|------|------|
| `chat-v2/adapters/TauriAdapter.ts` | **91** | 适配器层大量调试日志 |
| `chat-v2/core/store/createChatStore.ts` | **71** | Store 操作日志 |
| `utils/tauriApi.ts` | **60** | API 调用日志 |
| `chat-v2/pages/ChatV2Page.tsx` | **43** | 页面生命周期日志 |
| `anki/cardforge/engines/TaskController.examples.ts` | **39** | 示例代码中的日志 |
| `components/notes/NotesContext.tsx` | **30** | 笔记上下文日志 |
| `api/vfsRagApi.ts` | **29** | RAG API 日志 |
| `services/resourceSyncService.ts` | **23** | 同步服务日志 |
| `chat-v2/context/vfsRefApi.ts` | **16** | VFS 引用 API 日志 |
| `chat-v2/workspace/events.ts` | **15** | 工作区事件日志 |

**建议**:
1. 统一使用项目已有的 `debugLogger` 工具替代裸 `console.log`
2. 或创建 `logger` 工具，生产环境自动过滤 debug 级别
3. 批量清理明确的开发调试日志（如 `console.log('[ChatStore]'...)`）

---

## P2-NEW — 前端性能债务

### 17. React.memo 使用率极低

- **仅 21 个组件**使用了 `React.memo`（占总组件数 <5%）
- 已 memo 的组件集中在：chat-v2 消息渲染（MessageItem, BlockRenderer, MessageList, MarkdownRenderer）和 learning-hub 列表项
- **未 memo 但高频渲染的候选组件**：
  - `practice/` 模块（MockExamMode, DailyPracticeMode 等）— 含大量表单状态更新
  - `Settings.tsx` — 4856 行的巨型组件，任何状态变化导致全部重渲染
  - `InputBarUI.tsx` — 聊天输入栏，每次按键触发
  - `LearningHubSidebar.tsx` — 树形结构，节点展开收起触发

### 18. practice/ 模块结构重复

6 个练习模式组件（2,055 行）共享极其相似的结构：
- 相同的 Card + Badge + Input + Label 导入
- 相同的 stats 展示模式（正确率/总数/进度条）
- 相同的 "设置面板 → 开始练习 → 结果展示" 三阶段状态机

| 文件 | 行数 |
|------|------|
| `MockExamMode.tsx` | 415 |
| `PaperGenerator.tsx` | 427 |
| `DailyPracticeMode.tsx` | 377 |
| `TimedPracticeMode.tsx` | 327 |
| `PracticeLauncher.tsx` | 335 |
| `PracticeModeSelector.tsx` | 174 |

**建议**: 提取共享的 `PracticeConfigCard`, `PracticeStatsPanel`, `usePracticeMode` hook，减少 ~40% 重复代码。

---

## P3-NEW — i18n 硬编码中文

### 19. 中文字符串硬编码分布

虽然项目使用了 i18next，仍有大量中文硬编码在 TS/TSX 文件中：

| 文件 | 中文字符行数 | 说明 |
|------|-------------|------|
| `Settings.tsx` | **4,032** | 多数是注释，但含配置项 label |
| `createChatStore.ts` | **2,876** | Store 中的中文日志和默认值 |
| `ChatV2Page.tsx` | **2,759** | 页面中的中文注释和 fallback |
| `tauriApi.ts` | **2,637** | API 层的中文注释 |
| `TauriAdapter.ts` | **2,258** | 适配器中的中文注释和日志 |
| `toolDisplayName.ts` | **176** | 工具中文显示名**硬编码**（非 i18n） |

**注**: 多数中文出现在注释和日志中（可以接受），但 `toolDisplayName.ts` 的 176 行中文工具名是硬编码的 UI 文本，应迁移到 i18n。

---

## 附录 B: 操作优先级更新

### 新增 P0 项 ✅
- ✅ 删除 `@radix-ui/react-select`和 `@radix-ui/react-toast`

### 新增 P1 项 ✅
- ✅ 移动 `@radix-ui/react-checkbox`, `@radix-ui/react-slot`, `@radix-ui/react-switch` 到 dependencies
- ✅ 移动 `@types/dompurify`, `@types/jsdom` 到 devDependencies
- ✅ 清理 8 个未使用的 dependencies

### 新增 P2 项
- 统一 console.log → debugLogger（1,049 处）
- practice/ 模块提取公共组件和 hook
