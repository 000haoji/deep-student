# Learning Hub 标签页化改造方案

> 状态：设计阶段 | 创建日期：2026-02-18

## 一、背景与目标

学习资源管理器（Learning Hub）当前采用**单应用面板模型**：同一时刻只能打开一个资源，切换资源时旧组件完全卸载，所有内部状态丢失。这在以下场景中体验较差：

- 做题时想翻看教材 → 切走后计时器重置、做题进度丢失
- 写笔记时参考翻译 → 切走后编辑器销毁重建
- 频繁在多个 PDF 之间切换 → 每次重新加载数十 MB 文件

**改造目标**：引入标签页机制，允许用户同时打开多个资源，通过标签页切换时保持各资源的内部状态，同时控制内存占用。

---

## 二、现状分析

### 2.1 核心组件关系

```
LearningHubPage
├─ LearningHubSidebar（文件管理器，使用 useFinderStore）
├─ UnifiedAppPanel（统一应用面板，根据 type 渲染对应 ContentView）
│  ├─ NoteContentView（非懒加载，Crepe 富文本编辑器）
│  ├─ TextbookContentView（PDF/DOCX/XLSX/PPTX 预览）
│  ├─ ExamContentView（题目集，多种 viewMode）
│  ├─ TranslationContentView（翻译工作台）
│  ├─ EssayContentView（作文批改工作台）
│  ├─ ImageContentView（图片预览）
│  ├─ FileContentView（通用文件预览）
│  └─ MindMapContentView（思维导图，使用 useMindMapStore）
└─ DstuAppLauncher（移动端左侧应用入口）
```

### 2.2 当前状态模型

**LearningHubPage** 使用单值 `openApp: OpenApp | null` 表示当前打开的应用：

- 文件：`src/components/learning-hub/LearningHubPage.tsx`
- 接口：`{ type: ResourceType, id: string, title: string, dstuPath: string }`
- 历史栈：`appHistoryRef: useRef<OpenApp[]>([])`，仅存储 `{type, id, title, dstuPath}` 信息，不保存组件内部状态
- 打开资源时将旧 app push 进历史栈，关闭时 pop 恢复上一个
- **问题**：恢复时组件从零重建，所有内部状态丢失

**UnifiedAppPanel** 接收 `{type, resourceId, dstuPath}` 作为 props：

- 文件：`src/components/learning-hub/apps/UnifiedAppPanel.tsx`
- 内部通过 `dstu.get(resourceId)` 加载 DstuNode，然后用 switch-case 渲染对应 ContentView
- `NoteContentView` 特殊：非懒加载（`import` 而非 `lazy()`），因为 Crepe 编辑器初始化在 Suspense 下会卡住
- 其余 7 个 ContentView 均为 `lazy()` 懒加载
- 每个 ContentView 被 `AppContentErrorBoundary` 包裹

### 2.3 各 ContentView 内部状态详情

以下列出每个 ContentView 在切换时会**丢失**的关键内部状态：

#### NoteContentView
- **文件**：`src/components/learning-hub/apps/views/NoteContentView.tsx`
- 本地状态：`content`（笔记正文）、`title`、`isLoading`、`error`
- `loadingNoteIdRef` 防止竞态
- 编辑器状态：Crepe 编辑器的光标位置、选区、滚动位置、undo 栈
- **保存机制**：`NotesCrepeEditor` 内置 1500ms 防抖自动保存（`AUTO_SAVE_DEBOUNCE_MS`），并在 unmount 时通过 `flushNoteDraftRef.current()` 同步 flush 未保存的草稿。因此**内容不会因切走而丢失**
- **切换代价**：编辑器完全销毁重建（光标位置、选区、滚动位置、undo 栈丢失，但内容安全）

#### TextbookContentView
- **文件**：`src/components/learning-hub/apps/views/TextbookContentView.tsx`
- 包裹在 `PreviewProvider` 上下文中（管理 zoom/font scale）
- 本地状态：`selectedPages`（页面选择集合）、`bookmarks`、`readingProgress`、`fileContent`（base64）、`pdfFile`（Blob 对象，可达数十 MB）、`zoomScale`、`fontScale`、`previewType`
- PDF 通过 `usePdfLoader` 加载，大文件分片加载
- **保存机制**：阅读进度防抖 2 秒、书签防抖 1 秒，unmount 时有 `pendingProgressRef` / `pendingBookmarksRef` flush 机制
- **切换代价**：PDF Blob 重新加载（可能数十 MB），阅读页码位置丢失

#### ExamContentView
- **文件**：`src/components/learning-hub/apps/views/ExamContentView.tsx`（588 行）
- 本地状态极多：`viewMode`（6 种：list/practice/review/tags/upload/edit）、`practiceMode`、`selectedTag`、`elapsedTime` + `isTimerRunning`（练习计时器）、`focusMode`、`sessionDetail`（整卷会话详情）
- 通过 `useQuestionBankSession({ examId })` 获取题目数据，该 hook 内部读写**全局** `useQuestionBankStore`
- **切换代价**：计时器重置、viewMode 重置为 list、题目浏览进度丢失

#### TranslationContentView
- **文件**：`src/components/learning-hub/apps/views/TranslationContentView.tsx`
- 本地状态：`session`（翻译会话）、`isLoading`、`loadError`
- `currentNodeIdRef` 防止竞态
- 渲染 `TranslateWorkbench`（内部有自己的 `showPromptEditor` 等 UI 状态）
- **切换代价**：重新从后端加载会话，TranslateWorkbench 内部 UI 状态丢失

#### EssayContentView
- **文件**：`src/components/learning-hub/apps/views/EssayContentView.tsx`
- 本地状态：`session`（作文批改会话）、`rounds`（批改轮次）、`isLoading`、`error`
- 渲染 `EssayGradingWorkbench`（内部有 `inputText`、`gradingResult`、`showPromptEditor` 等状态）
- **切换代价**：重新从后端加载会话，Workbench 内部 UI 状态丢失

#### ImageContentView
- **文件**：`src/components/learning-hub/apps/views/ImageContentView.tsx`
- 本地状态：`zoom`、`rotation`、`imageData`、`loadingStage`、`error`、`fileSize`
- 支持渐进式加载大文件
- **切换代价**：图片重新加载，缩放/旋转重置

#### FileContentView
- **文件**：`src/components/learning-hub/apps/views/FileContentView.tsx`
- 与 TextbookContentView 类似，支持 PDF/媒体文件预览
- 本地状态：`selectedPages`、`fileContent`、预览设置
- **切换代价**：文件内容重新加载

#### MindMapContentView
- **文件**：`src/components/mindmap/MindMapContentView.tsx`（744 行）
- 使用**全局** `useMindMapStore`（Zustand + immer）
- store 状态：`document`（完整思维导图文档树）、`history`（past/future undo 栈）、`isDirty`、`isSaving`、`currentView`、`focusedNodeId`、`editingNodeId`、`selection`、`reciteMode`、`searchQuery/Results`、布局/样式配置
- 已有 localStorage 草稿机制：`mindmap:draft:{mindmapId}`，saveDraftSync() 同步写入
- `loadMindMap(mindmapId)` 加载时会自动检测并恢复草稿
- **切换代价**：store 被重置/覆盖，undo 栈清空（但有草稿可恢复文档内容）

### 2.4 全局 Store 冲突分析

以下两个全局 Zustand store 在多标签页场景下存在**实例冲突**问题：

#### useQuestionBankStore
- **文件**：`src/stores/questionBankStore.ts`（1654 行）
- **冲突字段**：`currentExamId`、`questions`（Map）、`questionOrder`、`currentQuestionId`、`stats`、`pagination`、`filters`、`practiceMode`（影响 `goToNextQuestion` 导航逻辑）
- **原因**：`loadQuestions(examId)` 会将结果写入全局 store 的 `questions`/`questionOrder`。如果两个 ExamContentView 实例同时挂载（display:none 保活），它们通过 `useQuestionBankSession` 共享同一个 store，后加载的 examId 会覆盖先加载的数据
- **可保留为全局**的字段：`showSettingsPanel`、`focusMode`（UI 偏好）、CSV 导入导出等功能性 actions
- **注意 `practiceMode`**：`ExamContentView` L77 从全局 store 读取 `practiceMode`，而 `goToNextQuestion`（store L1527）依据 `practiceMode` 决定顺序/随机/只错题导航。两个 exam tab 会共享同一个 `practiceMode`，必须本地化
- **useQuestionBankSession hook**（`src/hooks/useQuestionBankSession.ts`，262 行）是桥接层，从全局 store 读取 `questions/stats/currentQuestionId` 并做类型转换

#### useMindMapStore
- **文件**：`src/components/mindmap/store/mindmapStore.ts`（1498 行）
- **冲突字段**：`mindmapId`、`document`、`history`、`isDirty`、`isSaving`、`focusedNodeId`、`editingNodeId`、`selection`、`reciteMode`、布局/样式配置等（几乎所有字段）
- **原因**：`loadMindMap(mindmapId)` 会完全重置 store 状态
- **利好**：已有 localStorage 草稿机制（`DRAFT_KEY_PREFIX = 'mindmap:draft:'`），每个 mindmapId 独立持久化，`loadMindMap` 时自动恢复草稿

#### 不受影响的 Store
- `useFinderStore`（`src/components/learning-hub/stores/finderStore.ts`）：管理文件夹导航，不与 ContentView 关联
- `useHistoryStore`（`src/components/mindmap/store/historyStore.ts`）：独立的 undo/redo 栈，但实际 MindMap 使用的是 `mindmapStore` 内嵌的 history

### 2.5 Window 事件系统

所有与标签页相关的自定义事件分为三类：

#### A. "打开资源" 事件 → 流入 LearningHubPage

通过 `useLearningHubEvents` hook 统一监听（文件：`src/components/learning-hub/hooks/useLearningHubEvents.ts`）：

| 事件名 | 发送方 | detail 字段 | 当前处理 |
|---|---|---|---|
| `learningHubOpenExam` | App.tsx, ChatV2Page | `{ sessionId }` | `setOpenApp({ type:'exam', ... })` |
| `learningHubOpenTranslation` | App.tsx, ChatV2Page | `{ translationId, title? }` | `setOpenApp({ type:'translation', ... })` |
| `learningHubOpenEssay` | App.tsx, ChatV2Page | `{ essayId, title? }` | `setOpenApp({ type:'essay', ... })` |
| `learningHubOpenNote` | ChatV2Page, MemoryView | `{ noteId, source? }` | `setOpenApp({ type:'note', ... })` |
| `learningHubOpenResource` | various | `{ dstuPath }` | 动态 import openResource() |
| `LEARNING_OPEN_TRANSLATE` | CommandPalette | 无 | `handleCreateAndOpen('translation')` |
| `LEARNING_OPEN_ESSAY_GRADING` | CommandPalette | 无 | `handleCreateAndOpen('essay')` |
| `learningHubNavigateToKnowledge` | TauriAdapter (chat) | `{ preferTab?, documentId?, fileName?, resourceType?, memoryId? }` | 导航到知识库或打开文档 |

**改造影响**：所有这些处理函数中的 `setOpenApp(...)` 需要改为 `openTab(...)` 逻辑（查找已有 tab → 激活 / 新建 tab）。

#### B. "定向广播" 事件 → 流入 ContentView 内部

| 事件名 | 发送方 | 接收方 | 当前是否携带目标 ID |
|---|---|---|---|
| `translation:openSettings` | LearningHubPage 移动端按钮 | TranslateWorkbench | ❌ 无 |
| `essay:openSettings` | LearningHubPage 移动端按钮 | EssayGradingWorkbench | ❌ 无 |
| `exam:openSettings` | LearningHubPage 移动端按钮 | 通过 `useQuestionBankStore.toggleSettingsPanel()` | 无需路由（全局 store） |
| `LEARNING_GRADE_ESSAY` | CommandPalette (`learning.commands.ts`) | EssayGradingWorkbench | ❌ 无 |
| `LEARNING_ESSAY_SUGGESTIONS` | CommandPalette (`learning.commands.ts`) | EssayGradingWorkbench | ❌ 无 |

**改造影响**：在多标签页场景下，这些事件会被**所有挂载的同类型** ContentView 收到。需要在 detail 中加入 `targetResourceId` 字段，接收方检查是否匹配自身的 resourceId。

#### C. PDF 页码引用事件 → ContentView ↔ Chat InputBar 双向

| 事件名 | 方向 | 发送方 → 接收方 | 是否携带 sourceId |
|---|---|---|---|
| `pdf-page-refs:update` | OUT | TextbookContentView/FileContentView → `usePdfPageRefs` | ✅ 有 `sourceId` |
| `pdf-page-refs:clear` | IN | `usePdfPageRefs` → TextbookContentView/FileContentView | ❌ 无 |
| `pdf-page-refs:remove` | IN | `usePdfPageRefs` → TextbookContentView/FileContentView | ❌ 无 |

**改造影响**：`pdf-page-refs:clear` 和 `pdf-page-refs:remove` 需要加入 `sourceId` 字段，让多个 PDF tab 只清除对应的选择状态。

#### D. PDF 页码跳转事件 → Chat 引用跳转到 ContentView

| 事件名 | 方向 | 发送方 → 接收方 | 是否已有目标过滤 |
|---|---|---|---|
| `pdf-ref:focus` | IN | Chat 引用按钮 → `usePdfFocusListener` (TextbookContentView/FileContentView) | ✅ 已有 sourceId 匹配（`matchesSource \|\| matchesPath`）|

**无需改造**：`usePdfFocusListener`（`src/components/learning-hub/apps/views/usePdfFocusListener.ts`）已通过 `sourceId` 和 `path` 匹配目标节点，多标签页场景下天然安全。

#### E. Tauri 事件：ExamSheet OCR 进度

| 事件名 | 监听方 | 是否有 session 过滤 |
|---|---|---|
| `exam_sheet_progress` | `useExamSheetProgress` (via `useTauriEventListener`) | ❌ **无 session 过滤** |

**问题**：`useExamSheetProgress`（`src/hooks/useExamSheetProgress.ts`）监听全局 Tauri 事件 `exam_sheet_progress`，但 `handleProgress` 未检查 `detail.summary.id` 是否匹配当前 session。如果两个 ExamContentView tab 同时处于 upload viewMode，两者都会收到对方的进度事件。

**改造影响**：`useExamSheetProgress` 需要接受 `sessionId` 参数，在 `handleProgress` 中检查 `payload.detail?.summary?.id === sessionId`，不匹配则忽略。

#### F. 导航状态同步事件

| 事件名 | 发送方 | 接收方 | 影响 |
|---|---|---|---|
| `LEARNING_HUB_NAV_STATE_CHANGED` | LearningHubNavigationContext | App.tsx | 无需改造（与 tab 无关） |

### 2.6 OpenResource 注册机制

- **文件**：使用 `registerOpenResourceHandler(handler, 'learning-hub')`
- 注册位置：`LearningHubPage.tsx` 的 `useEffect` 中
- `handler.openInPanel(path, node, mode)` 当前调用 `setOpenApp(prev => { if (prev) appHistoryRef.push(prev); return ... })`
- **改造影响**：改为调用 `openTab(...)` 逻辑

### 2.7 移动端三屏布局

- 文件：`LearningHubPage.tsx` L756~872
- 三屏结构：左侧 `DstuAppLauncher`（sidebarWidth） + 中间 `LearningHubSidebar`（100vw） + 右侧应用内容（100vw）
- 通过 `screenPosition` state（'left' | 'center' | 'right'）和 CSS `transform: translateX()` 实现滑动切换
- 触摸手势支持轴向锁定（`stateRef.current.axisLocked`），防止与竖直滚动冲突
- 右侧目前渲染 `{openApp ? <UnifiedAppPanel.../> : <empty/>}`
- `useMobileHeader` 根据 `screenPosition` + `openApp` 动态设置标题和返回按钮

### 2.8 桌面端分栏布局

- 文件：`LearningHubPage.tsx` L876~957
- 使用 `react-resizable-panels`：`PanelGroup direction="horizontal" autoSaveId="learning-hub-layout"`
- 左侧 Panel：sidebar，defaultSize=25, minSize=15
- 右侧 Panel：app，defaultSize=75, minSize=40, collapsible=true, collapsedSize=0
- 通过 `appPanelRef.current.expand()` / `.collapse()` 控制应用面板展开/折叠
- 打开应用时自动收缩侧边栏（`setLocalSidebarCollapsed(true)`）

---

## 三、改造方案

### 3.1 总体策略

采用 **`display:none` 保活**策略：所有已打开的 tab 同时渲染其 ContentView 组件，非活跃 tab 通过 `display:none` 隐藏。这是最简单、状态保持最完整的方案。

配合以下机制控制副作用：
- **标签页数量上限**（建议 8 个）+ LRU 淘汰最旧的非固定 tab
- **isActive prop** 传递给需要感知活跃状态的 ContentView（MindMap 和未来可能的其他需要全局 store 的类型）
- **全局 store 隔离**分阶段解决

### 3.2 数据模型变更

#### 新增 Tab 数据结构

新建文件 `src/components/learning-hub/types/tabs.ts`：

- `OpenTab` 接口：在现有 `OpenApp` 基础上增加 `tabId`（nanoid 生成的唯一标识符）、`isPinned`（是否固定）、`openedAt`（打开时间戳，用于 LRU 淘汰）
- `MAX_TABS` 常量：标签页数量上限，建议 8
- `createTab()` 工厂函数：自动生成 `tabId` 和 `openedAt`

#### 替换 LearningHubPage 核心状态

在 `LearningHubPage.tsx` 中：

- **移除**：`openApp` state、`appHistoryRef` ref
- **新增**：`tabs: OpenTab[]` state、`activeTabId: string | null` state
- **派生**：`activeTab = tabs.find(t => t.tabId === activeTabId)`、`hasOpenApp = tabs.length > 0`

#### Tab 操作函数

在 `LearningHubPage.tsx` 中新增以下 useCallback 函数：

- **openTab(app)**：查找是否已有相同 `resourceId` 的 tab → 有则激活；无则新建。如果超出 MAX_TABS，LRU 淘汰最旧的非固定 tab。
- **closeTab(tabId)**：移除 tab，如果是当前活跃 tab 则激活相邻 tab（优先右侧、次选左侧）。如果关闭后 tabs 为空，在移动端切回 center 屏。
- **switchTab(tabId)**：设置 `activeTabId`。
- **updateTabTitle(tabId, title)**：更新指定 tab 的标题。
- **closeOtherTabs(tabId)**：关闭除指定 tab 外的所有非固定 tab。
- **closeRightTabs(tabId)**：关闭指定 tab 右侧的所有非固定 tab。

### 3.3 所有 setOpenApp 调用点改造

以下函数中所有 `setOpenApp(...)` 调用改为 `openTab(...)`：

| 函数 | 位置 | 改造要点 |
|---|---|---|
| `handleOpenApp` | L593 | 接收 `ResourceListItem`，构建 tab 参数传给 `openTab` |
| `handleOpenExamEvent` | L452 | `openTab({ type:'exam', resourceId: sessionId, ... })` |
| `handleOpenTranslationEvent` | L467 | `openTab({ type:'translation', resourceId: translationId, ... })` |
| `handleOpenEssayEvent` | L483 | `openTab({ type:'essay', resourceId: essayId, ... })` |
| `handleOpenNoteEvent` | L499 | `openTab({ type:'note', resourceId: noteId, ... })` |
| `handleOpenResourceEvent` | L515 | 通过 `openResource()` 最终走 `registerOpenResourceHandler` |
| `handleNavigateToKnowledgeEvent` | L535 | 当有 `documentId` 时调用 `openTab(...)` |
| `handleCreateAndOpen` | L631 | 创建新资源后 `openTab(...)` |
| `registerOpenResourceHandler.openInPanel` | L414 | 改为 `openTab(...)` |

`handleCloseApp` 改为：关闭 `activeTab`（即 `closeTab(activeTabId)`），移动端额外 `setScreenPosition('center')`。

`handleTitleChange` 改为：`updateTabTitle(activeTabId, title)`。

### 3.4 TabBar 组件

新建文件 `src/components/learning-hub/components/TabBar.tsx`。

#### 接口设计

Props：`tabs`、`activeTabId`、`onSwitch`、`onClose`、`onCloseOthers`、`onCloseRight`。

#### UI 规格

- 高度 36px，`border-b`，`bg-muted/30`，横向溢出时 `overflow-x-auto scrollbar-none`
- 每个 tab item：类型图标（使用现有 `getAppIcon`）+ 资源名称（truncate，max-w-48）+ 关闭按钮（hover 显示）
- 活跃 tab：`bg-background text-foreground`，底部 2px primary 色条
- 非活跃 tab：`text-muted-foreground hover:bg-muted/50`
- 右键菜单：关闭、关闭其他、关闭右侧（使用项目已有的 ContextMenu 组件）
- tab 项之间 `border-r border-border/30` 分隔
- Hover 时 tooltip 显示完整路径（`tab.dstuPath`）

#### 拖拽排序（Phase 4）

使用项目已有的 `@dnd-kit/core` 依赖实现拖拽排序，具体实现延后。

### 3.5 TabPanelContainer 组件

新建文件 `src/components/learning-hub/apps/TabPanelContainer.tsx`。

#### 职责

替代当前 `{openApp && <UnifiedAppPanel .../>}` 的直接渲染方式，为每个 tab 渲染一个 `UnifiedAppPanel` 实例并用 `display:none` 保活。

#### 关键实现细节

- 遍历 `tabs` 数组，为每个 tab 渲染一个 `div`，用 `style={{ display: tab.tabId === activeTabId ? 'flex' : 'none' }}`
- **key 策略**：使用 `tab.tabId` 作为 React key（不是 `tab.resourceId`），保证组件实例与 tab 一一对应
- 每个 div 内渲染 `<UnifiedAppPanel>` 并传入 `isActive={tab.tabId === activeTabId}`
- 每个 `UnifiedAppPanel` 外套 `<Suspense>` 以支持懒加载

### 3.6 UnifiedAppPanel 和 ContentViewProps 扩展

#### UnifiedAppPanelProps 新增

- `isActive?: boolean` — 表示当前 panel 是否为用户可见的活跃面板

#### ContentViewProps 新增

- `isActive?: boolean` — 透传给所有 ContentView，让需要的组件感知活跃状态

#### 哪些 ContentView 需要使用 isActive

| ContentView | 是否需要 isActive | 原因 |
|---|---|---|
| NoteContentView | 可能需要 | 验证 Crepe 编辑器在 display:none 下是否正常工作；如有问题，在 inactive 时暂停编辑器 |
| TextbookContentView | 不需要 | display:none 下 PDF 渲染暂停，恢复后正常 |
| ExamContentView | 不需要 | 计时器用 `setInterval` / `useState`，display:none 下仍运行（这是期望行为） |
| TranslationContentView | 不需要 | 纯数据展示 |
| EssayContentView | 不需要 | 纯数据展示 |
| ImageContentView | 不需要 | 静态图片 |
| FileContentView | 不需要 | 同 TextbookContentView |
| MindMapContentView | **必须** | 全局 store 冲突，需要在 inactive → active 切换时 saveDraftSync / loadMindMap |

### 3.7 桌面端布局改造

在 `LearningHubPage.tsx` 桌面端渲染部分（L876~957）：

#### 右侧 Panel 内容改造

- 原：直接渲染 `{openApp && <UnifiedAppPanel .../>}`
- 新：渲染 `<TabBar>` + `<TabPanelContainer>`，TabBar 在顶部、TabPanelContainer 占据剩余空间
- 当 `tabs.length === 0` 时，不渲染任何内容（Panel 处于 collapsed 状态）

#### Panel 展开/折叠逻辑

- 依赖条件从 `hasOpenApp`（基于 `openApp !== null`）改为 `tabs.length > 0`
- `appPanelRef.current.expand()` 在 `tabs.length > 0` 时执行
- `appPanelRef.current.collapse()` 在 `tabs.length === 0` 时执行

#### PanelResizeHandle 显示条件

- 从 `hasOpenApp` 改为 `tabs.length > 0`

### 3.8 移动端布局适配

#### 保持三屏滑动结构不变

移动端不显示 TabBar（屏幕空间不足），右侧屏幕始终显示 `activeTab` 对应的 panel。

#### 右侧屏幕渲染

- 原：`{openApp ? <UnifiedAppPanel .../> : <empty/>}`
- 新：`{activeTab ? <UnifiedAppPanel type={activeTab.type} .../> : <empty/>}`
- 移动端不需要 TabPanelContainer 的 display:none 保活（一次只看一个）
- 但为了切回时不丢状态，移动端也可以用 TabPanelContainer，只是不显示 TabBar

#### 移动端极简 Tab 切换（可选，Phase 4）

在右侧屏幕顶部增加一个极简的 tab 指示器（小圆点或横向滑动条），让用户知道还有其他打开的 tab 可以切换。

#### useMobileHeader 改造

- 所有 `openApp` 引用改为 `activeTab`
- 标题显示 `activeTab.title`
- 设置按钮事件发送需携带 `targetResourceId: activeTab.resourceId`

### 3.9 导航上下文改造

`LearningHubNavigationContext` 中的 `hasOpenApp` 和 `registerCloseAppCallback`：

- `hasOpenApp` 改为 `tabs.length > 0`
- `registerCloseAppCallback` 传入的 `handleCloseApp` 改为关闭 activeTab

---

## 四、全局 Store 隔离方案

### 4.1 useQuestionBankStore 隔离

**目标**：让每个 ExamContentView 实例拥有独立的题目数据，互不影响。

**方案**：改造 `useQuestionBankSession` hook，将 exam-specific 状态从全局 store 提取到 hook 内部的本地 state。

#### 需要本地化的状态

从全局 store 中提取到 hook 本地：
- `questions`（Map） → hook 内 `useState<Question[]>`
- `questionOrder` → hook 内维护
- `currentQuestionId` / `currentIndex` → hook 内 `useState<number>`
- `stats` → hook 内 `useState<Stats | null>`
- `pagination` → hook 内 `useState`
- `isLoading` / `isSubmitting` / `error` → hook 内 `useState`

#### 需要保留在全局 store 的状态

- `showSettingsPanel` / `toggleSettingsPanel()` — 移动端设置按钮需要跨组件访问
- `focusMode` — 全局 UI 偏好
- CSV 导入导出、同步等功能性 API actions（它们不在 ContentView 的热路径上）

#### ❗ 额外需要本地化的状态

- **`practiceMode`**：当前由 `ExamContentView` L77 从全局 store 读取，并被 `goToNextQuestion`（store L1527）用于导航逻辑（sequential/random/wrong-only）。必须本地化到 `useQuestionBankSession` 内部，否则两个 exam tab 的练习模式互相干扰
- **导航 actions**：`goToNextQuestion`、`goToPrevQuestion`、`goToQuestion` 当前读取全局 store 的 `questionOrder`/`currentQuestionId`/`practiceMode`，本地化后这些导航逻辑也需要移入 hook 内部
- **`checkSyncStatus`**：`ExamContentView` L76 在 mount 时调用，可能修改全局 store 的 `isSyncing` 等字段，需评估是否会影响其他 tab

#### 改造方式

`useQuestionBankSession` 内部不再通过 `useQuestionBankStore(useShallow(...))` 订阅全局 store 的数据切片，而是：

1. 用 `useState` 管理 `questions`、`stats`、`currentIndex` 等
2. `loadQuestions` 直接调用 `invoke('qbank_list_questions', { examId, ... })` 并用 `setState` 更新本地数据
3. `submitAnswer` 直接调用 `invoke('qbank_submit_answer', ...)` 并用返回的 `updated_question` / `updated_stats` 本地更新
4. `toggleFavorite`、`deleteQuestion` 等同理直接 invoke 后本地更新
5. 全局 store 的 actions（`loadQuestions`、`submitAnswer` 等）仍可保留，但不再被 `useQuestionBankSession` 使用

#### 回归测试要点

- ExamContentView 的所有 viewMode（list/practice/review/tags/upload）功能正常
- 同时打开两个不同 exam 的 tab，切换后各自的题目数据和做题进度独立
- QuestionBankEditor 的 navigate、submitAnswer、markCorrect 功能正常
- 计时器在 display:none 下继续运行

### 4.2 useMindMapStore 隔离

**目标**：多个 MindMapContentView tab 不互相覆盖全局 store。

**方案**：利用已有的 localStorage 草稿机制实现**"假保活"**—— 不改造 store 为多实例，而是在 tab 切换时触发 saveDraft / loadMindMap 循环。

#### 实现方式

在 `MindMapContentView` 中监听 `isActive` prop 变化：

- **active → inactive**（用户切到另一个 tab）：调用 `saveDraftSync()` 同步保存草稿到 localStorage
- **inactive → active**（用户切回此 tab）：调用 `loadMindMap(resourceId)`，该方法内部已有草稿恢复逻辑（比较草稿时间戳 vs 服务器 updatedAt，优先使用较新的）

#### 用户体验

- 从用户角度看，切换回 MindMap tab 时有极短暂的加载（从 localStorage 读取 + JSON.parse），但草稿恢复使文档内容和布局配置完全保留
- undo/redo 栈会丢失（`loadMindMap` 会 reset history），这是已知妥协
- 如果需要完全保活 undo 栈，则需要在 Phase 3+ 改造 store 为实例化模式

#### 不需要改造 store 的原因

MindMap 的草稿机制已经做得很完善：`MindMapDraftPayload` 包含 `document`、`currentView`、`focusedNodeId`、`layoutId`、`layoutDirection`、`styleId`、`edgeType`，恢复精度很高。

---

## 五、事件路由改造

### 5.1 定向广播事件加 targetResourceId

以下事件需要在 detail 中添加 `targetResourceId` 字段：

| 事件名 | 发送方修改位置 | 接收方修改位置 |
|---|---|---|
| `translation:openSettings` | `LearningHubPage.tsx` L368 | `TranslateWorkbench.tsx` L128 |
| `essay:openSettings` | `LearningHubPage.tsx` L371 | `EssayGradingWorkbench.tsx` L75 |
| `LEARNING_GRADE_ESSAY` | `learning.commands.ts` L117 | `EssayGradingWorkbench.tsx` L482 |
| `LEARNING_ESSAY_SUGGESTIONS` | `learning.commands.ts` L131 | `EssayGradingWorkbench.tsx` L496 |

**发送方改造**：

- `LearningHubPage.tsx` 中的移动端设置按钮：`detail` 中加入 `{ targetResourceId: activeTab.resourceId }`
- `learning.commands.ts` 中的命令面板命令：需要获取当前 activeTab 的 resourceId。可以通过新增一个全局函数/store 暴露当前 `activeTabResourceId`，或改为通过 `useQuestionBankStore` 类似的模式

**接收方改造**：

- 每个 Workbench 在事件监听中检查 `event.detail?.targetResourceId`，只有匹配自身 resourceId 或无 targetResourceId（兼容旧代码）时才处理

### 5.2 PDF 页码引用事件加 sourceId

**需要改造的事件**：

- `pdf-page-refs:clear`：由 `usePdfPageRefs.ts` L73 发送 → 加入 `{ sourceId }`
- `pdf-page-refs:remove`：由 `usePdfPageRefs.ts` L88 发送 → 加入 `{ sourceId, page }`

**接收方改造**：

- `TextbookContentView.tsx` L112-127：`handleClear` 和 `handleRemove` 检查 `event.detail.sourceId` 是否匹配 `node.sourceId`
- `FileContentView.tsx` L168-184：同上

**`pdf-page-refs:update` 无需改造**：已有 `sourceId` 字段。

### 5.3 exam:openSettings 不需要改造

当前已通过 `useQuestionBankStore.getState().toggleSettingsPanel()` 而非 window event 实现，`showSettingsPanel` 是全局 UI 状态。在多 exam tab 场景下，这个行为是可以接受的（设置面板在活跃 tab 中响应即可）。

### 5.4 CommandPalette 获取活跃 Tab 信息

`learning.commands.ts` 中的 `LEARNING_GRADE_ESSAY` 和 `LEARNING_ESSAY_SUGGESTIONS` 需要知道当前活跃 tab 的 resourceId 来定向发送事件。

方案：新增一个轻量的全局 store 或导出函数，暴露 `getActiveTab(): OpenTab | null`。CommandPalette 在 execute 时调用此函数获取 targetResourceId。

---

## 六、分阶段实施计划

### Phase 1：核心标签页框架（预计 3 天）

**范围**：

1. 新增 `OpenTab` 类型定义和 `createTab` 工厂函数
2. `LearningHubPage` 状态模型从 `openApp` 改为 `tabs[]` + `activeTabId`
3. 实现 `openTab`、`closeTab`、`switchTab`、`updateTabTitle` 函数
4. 修改所有 `setOpenApp` 调用点
5. 新建 `TabBar` 组件（简单版，不含拖拽和右键菜单）
6. 新建 `TabPanelContainer` 组件，实现 `display:none` 保活
7. `UnifiedAppPanel` 和 `ContentViewProps` 增加 `isActive` prop
8. 桌面端布局改造（PanelGroup 内嵌 TabBar + TabPanelContainer）
9. 移动端布局适配（activeTab 替代 openApp，暂不显示 TabBar）
10. 导航上下文、Panel 折叠、sidebar 收缩等逻辑适配

**已知限制**：同一 resourceId 不重复开 tab（openTab 中有去重）。全局 store 冲突问题暂不修（避免同时开两个 exam 或两个 mindmap）。

**测试要点**：
- 桌面端：打开多个不同类型资源 tab，切换后内容保持
- 移动端：打开资源、返回列表、再打开另一个资源，切换正常
- 关闭 tab 后激活相邻 tab
- 超过 8 个 tab 时自动淘汰最旧的

### Phase 2：全局 Store 隔离（预计 3 天）

**范围**：

1. `useQuestionBankSession` 改为本地 state 管理，不再读写全局 store 的 questions/stats
2. `MindMapContentView` 增加 `isActive` 监听，实现 saveDraftSync / loadMindMap 循环
3. 验证同时打开两个不同 exam 的 tab 数据隔离
4. 验证两个 mindmap tab 切换时草稿恢复

**测试要点**：
- 打开 exam A，做 3 题 → 打开 exam B → 切回 exam A，做题进度保持
- 打开 mindmap A，编辑节点 → 打开 mindmap B → 切回 mindmap A，编辑内容保持

### Phase 3：事件路由（预计 1 天）

**范围**：

1. `translation:openSettings` / `essay:openSettings` 加 `targetResourceId`
2. `LEARNING_GRADE_ESSAY` / `LEARNING_ESSAY_SUGGESTIONS` 加 `targetResourceId`
3. `pdf-page-refs:clear` / `pdf-page-refs:remove` 加 `sourceId`
4. 新增 `getActiveTab()` 全局导出供 CommandPalette 使用
5. 各接收方增加 targetResourceId/sourceId 过滤

**测试要点**：
- 同时打开两个翻译 tab，移动端点设置按钮只影响活跃的那个
- 同时打开两个 PDF tab，chat 发送后只清除当前活跃 PDF 的选中页

### Phase 4：UX 优化（预计 2-3 天）

**范围**：

1. TabBar 右键菜单（关闭、关闭其他、关闭右侧）
2. TabBar 标签页拖拽排序（使用项目已有的 `@dnd-kit/core`）
3. Tab 固定（pin）功能
4. 移动端极简 tab 指示器（可选）
5. Tab 状态持久化到 localStorage（刷新后恢复已打开的 tabs）
6. Ctrl+W / Cmd+W 快捷键关闭当前 tab
7. Ctrl+Tab 快捷键切换 tab

---

## 七、风险与注意事项

### 7.1 内存占用

display:none 保活下，所有 tab 的 DOM 和 JS 状态都在内存中。主要风险来源：
- PDF Blob 对象：单个可达 50-100 MB
- MindMap document 树：取决于节点数（上限 10000 个）
- 富文本编辑器 Crepe 实例：每个实例包含 ProseMirror editor state

**缓解措施**：8 个 tab 上限 + LRU 淘汰。未来可考虑 Phase 5 进阶方案：对超过 N 个 tab 的非活跃 panel 执行"休眠"（卸载组件但保存序列化状态到 sessionStorage）。

### 7.2 NoteContentView 的 Crepe 编辑器

NoteContentView 是**唯一非懒加载**的 ContentView（避免 Suspense 导致 Crepe init 卡住）。在 display:none 下，Crepe/ProseMirror 编辑器可能出现：
- 光标定位异常（display:none 下无法计算 DOM 布局）
- 快捷键冲突（多个编辑器实例同时监听 keyboard events）

**验证清单**：
- [ ] display:none 下 Crepe 编辑器是否正常
- [ ] 切回后光标和滚动位置是否恢复
- [ ] 多个笔记 tab 的快捷键是否冲突

如果有问题，需要在 NoteContentView 中使用 `isActive` prop 来 blur/disable 非活跃编辑器。

### 7.3 计时器行为

ExamContentView 的练习计时器（`elapsedTime` + `isTimerRunning`）在 display:none 下 `setInterval` 仍然运行。这是**期望行为**（用户切走时计时继续），但需在 UI 上明确告知。

### 7.4 ExamSheetUploader 的流式处理

如果 ExamContentView 处于 upload viewMode 且正在进行 OCR 流式处理，切到另一个 tab 后处理仍在后台继续。这是正确行为，但需确保 progress 事件仍能正确更新 UI（display:none 下 setState 正常工作，切回后立刻反映最新进度）。

### 7.5 向后兼容

- `appHistoryRef` 的"后退到上一个资源"UX 被 tab 切换替代，后退按钮改为"关闭当前 tab"
- 对外部调用者（App.tsx, ChatV2Page 等）透明：它们仍通过 window event 发送 `learningHubOpenXxx`，LearningHubPage 内部从 `setOpenApp` 改为 `openTab`，外部无需感知

---

## 八、涉及文件清单

### 新增文件
| 文件 | 说明 |
|---|---|
| `src/components/learning-hub/types/tabs.ts` | OpenTab 类型、MAX_TABS、createTab |
| `src/components/learning-hub/components/TabBar.tsx` | 标签页栏组件 |
| `src/components/learning-hub/apps/TabPanelContainer.tsx` | 保活容器组件 |

### 修改文件
| 文件 | Phase | 改动概要 |
|---|---|---|
| `src/components/learning-hub/LearningHubPage.tsx` | 1 | 核心状态模型替换、渲染层改造、所有 setOpenApp 替换 |
| `src/components/learning-hub/apps/UnifiedAppPanel.tsx` | 1 | 新增 isActive prop 透传 |
| `src/components/learning-hub/LearningHubNavigationContext.tsx` | 1 | hasOpenApp 和 closeApp 回调适配 |
| `src/components/learning-hub/types.ts` | 1 | 可选：导出新类型 |
| `src/hooks/useQuestionBankSession.ts` | 2 | 全局 store 读取改为本地 state |
| `src/components/mindmap/MindMapContentView.tsx` | 2 | isActive 监听 + saveDraftSync/loadMindMap |
| `src/components/TranslateWorkbench.tsx` | 3 | 事件监听加 targetResourceId 过滤 |
| `src/components/EssayGradingWorkbench.tsx` | 3 | 事件监听加 targetResourceId 过滤 |
| `src/components/learning-hub/apps/views/TextbookContentView.tsx` | 3 | pdf-page-refs 事件加 sourceId 过滤 |
| `src/components/learning-hub/apps/views/FileContentView.tsx` | 3 | pdf-page-refs 事件加 sourceId 过滤 |
| `src/chat-v2/components/input-bar/usePdfPageRefs.ts` | 3 | clear/remove 事件发送时加 sourceId |
| `src/command-palette/modules/learning.commands.ts` | 3 | 事件发送时获取 activeTab resourceId |
| `src/hooks/useExamSheetProgress.ts` | 2 | 新增 sessionId 参数，handleProgress 过滤非当前 session 的事件 |
| `src/components/learning-hub/LearningHubSidebar.tsx` | 1 | `activeFileId`、`hasOpenApp`、`onCloseApp` props 适配 |

---

## 附录：批判性检查报告

> 日期：2026-02-18 | 方法：逐项对照源码验证设计方案中的假设和遗漏

### A. 事实性错误（已修正）

#### A-1. NoteContentView 保存机制描述有误 ✅ 已修正

- **原文**："编辑器的 `onSave` 回调通过 `dstu.update()` 保存，但切走前如果用户未触发保存则内容丢失"
- **实际**：`NotesCrepeEditor` 内置 `AUTO_SAVE_DEBOUNCE_MS = 1500` 自动保存，且在 unmount 时通过 `flushNoteDraftRef.current()` 同步 flush（`NotesCrepeEditor.tsx` L389-407）。**内容不会丢失**，丢失的只是光标位置/选区/undo 栈
- **影响**：降低了 NoteContentView 在 LRU 淘汰场景下的风险等级

### B. 遗漏的问题（新增 P 级）

#### B-1. `practiceMode` 全局冲突 — **严重** ✅ 已补充到 §2.4 和 §4.1

- **位置**：`ExamContentView.tsx` L77 读取 `useQuestionBankStore(state => state.practiceMode)`
- **影响链**：`goToNextQuestion`（store L1527）内部用 `practiceMode` 决定 sequential / random / wrong-only 导航。两个 exam tab 共享同一个 `practiceMode`，切换一个 tab 的练习模式会影响另一个
- **解决**：Phase 2 中将 `practiceMode` 和导航 actions（`goToNextQuestion`/`goToPrevQuestion`/`goToQuestion`）一并本地化到 `useQuestionBankSession`

#### B-2. `useExamSheetProgress` Tauri 事件无 session 过滤 — **严重** ✅ 已补充到 §2.5E

- **位置**：`src/hooks/useExamSheetProgress.ts` L213-214
- **问题**：监听全局 Tauri 事件 `exam_sheet_progress`，`handleProgress` 未检查 `payload.detail?.summary?.id`。两个 ExamContentView tab 同时上传时，进度条会互相干扰
- **解决**：`useExamSheetProgress` 新增 `sessionId` 参数，在 `handleProgress` 入口处检查 `detail.summary.id === sessionId`

#### B-3. `pdf-ref:focus` 事件未文档化（但已安全）✅ 已补充到 §2.5D

- **位置**：`src/components/learning-hub/apps/views/usePdfFocusListener.ts`
- **结论**：`usePdfFocusListener` 已通过 `sourceId` + `path` 双重匹配过滤目标节点，多标签页下天然安全。**无需改造**，但之前文档中遗漏了这个事件

#### B-4. `LearningHubSidebar` props 未列入改造清单 ✅ 已补充到 §八

- **位置**：`LearningHubPage.tsx` L900 `activeFileId={openApp?.id}` 和 L901 `hasOpenApp={hasOpenApp}`
- **解决**：Phase 1 中将 `activeFileId` 改为 `activeTab?.resourceId`，`hasOpenApp` 改为 `tabs.length > 0`

### C. 架构风险（需关注）

#### C-1. `onTitleChange` 闭包稳定性

- **位置**：TabPanelContainer 中为每个 tab 创建 `onTitleChange={(title) => onTitleChange(tab.tabId, title)}`
- **风险**：这个箭头函数每次渲染都是新引用。`UnifiedAppPanel` 的 `useEffect`（L95-125）依赖 `[dstuPath, onTitleChange, resourceId, t, type]`，如果 `onTitleChange` 引用变化，会重新触发 `dstu.get()` 加载
- **解决**：TabPanelContainer 中必须用 `useCallback` 或 `useMemo` 为每个 tab 创建稳定的回调引用，或者 `UnifiedAppPanel` 内部用 `useRef` 持有 `onTitleChange` 避免它参与 effect 依赖

#### C-2. MindMapContentView 接口不同于 ContentViewProps

- **位置**：`UnifiedAppPanel.tsx` L187 `<MindMapContentView resourceId={node.id} onTitleChange={onTitleChange} className="h-full" />`
- **问题**：`MindMapContentView` 有自己独立的 props 接口，不使用 `ContentViewProps`（不接收 `node`/`onClose`/`readOnly`）。§3.6 中"ContentViewProps 新增 isActive"不会自动传递到 MindMapContentView
- **解决**：在 `UnifiedAppPanel` 的 `case 'mindmap':` 分支中手动传入 `isActive` 和必要的 tab 相关 props

#### C-3. 页面级导航导致 Tab 状态全部丢失

- **问题**：用户从 Learning Hub 导航到 Chat 页面再回来时，`LearningHubPage` 整个组件卸载重挂，所有 `tabs` state 丢失
- **现状**：当前单 app 模型也有此问题（`openApp` 也会丢失），但标签页化后用户预期更高（开了多个 tab）
- **Phase 4 部分解决**：将 `tabs` 和 `activeTabId` 持久化到 `sessionStorage`（不是 `localStorage`，刷新后应清空）。返回 Learning Hub 时恢复 tab 元数据，各 ContentView 组件重建并重新加载数据
- **注意**：这只恢复 tab 列表，不恢复各 ContentView 的内部状态（编辑器光标、做题进度等）。真正保持状态需要上层路由的 keep-alive 机制，超出本方案范围

#### C-4. LRU 淘汰时的数据安全

各 ContentView 在 LRU 淘汰（tab 被自动关闭导致组件卸载）时的数据安全性：

| ContentView | 淘汰时数据安全 | 机制 |
|---|---|---|
| NoteContentView | ✅ 安全 | `NotesCrepeEditor` unmount flush |
| TextbookContentView | ✅ 安全 | `pendingProgressRef`/`pendingBookmarksRef` unmount flush |
| ExamContentView | ⚠️ 部分安全 | 做题进度已通过 `submitAnswer` 实时提交；但 viewMode、计时器、未提交的答案输入框内容丢失 |
| TranslationContentView | ✅ 安全 | 会话数据在后端，前端只读 |
| EssayContentView | ✅ 安全 | 会话数据在后端，前端只读 |
| ImageContentView | ✅ 安全 | 无需持久化状态 |
| FileContentView | ✅ 安全 | 同 TextbookContentView |
| MindMapContentView | ✅ 安全 | `saveDraftSync()` 在 isActive→inactive 时触发；unmount 时 `reset()` 也会保存 |

**ExamContentView 特别注意**：如果用户正在做题页面输入了答案但尚未点击"提交"，此时被 LRU 淘汰，输入框内容丢失。建议在 LRU 淘汰前检查是否有未提交状态，给出提示或阻止淘汰。

#### C-5. `checkSyncStatus` 全局副作用

- **位置**：`ExamContentView.tsx` L76 `const checkSyncStatus = useQuestionBankStore(state => state.checkSyncStatus)`，L152-164 在 mount 时调用
- **问题**：`checkSyncStatus` 可能修改全局 store 的 `isSyncing`、`syncStatus` 等字段。多个 exam tab mount 时并发调用，可能产生竞态
- **严重性**：低（同步检查是幂等操作，竞态最多导致短暂的状态闪烁）
- **建议**：Phase 2 中评估是否需要本地化，或加防抖

### D. 设计验证清单更新

在 §7.2 验证清单基础上，补充以下验证项：

- [ ] `NotesCrepeEditor` 在 `display:none` 下：ProseMirror 的 `EditorView.update()` 是否报错
- [ ] 多个 `NotesCrepeEditor` 实例：keyboard shortcuts（Ctrl+B/I/K 等）是否只在活跃编辑器响应
- [ ] `ExamSheetUploader` 两个 tab 同时上传：进度条是否独立（需 B-2 修复后验证）
- [ ] `practiceMode` 切换：tab A 设为 random、tab B 设为 sequential，各自导航是否正确（需 B-1 修复后验证）
- [ ] `onTitleChange` 闭包：打开一个 tab 后观察 Network/console，确认 `dstu.get()` 没有重复触发（验证 C-1）
- [ ] 同时打开 2 个 MindMap tab：切换时草稿保存/恢复是否平滑，undo 栈丢失是否可接受
