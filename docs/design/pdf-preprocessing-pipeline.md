# 媒体预处理流水线方案设计（PDF + 图片）

> 版本: v2.2 (强制预处理压缩)
> 日期: 2026-02-02
> 状态: ✅ 实现完成

## v2.2 更新说明 - P0 架构改造

**核心变更：发送时不再压缩，完全依赖预处理阶段的压缩结果**

### 背景问题

原有架构中存在两套压缩机制：
1. **预处理压缩**：上传后异步执行，仅压缩 >1MB 的图片
2. **发送时压缩**：每次发送前调用 `adjust_image_quality_base64`，实时压缩

这导致：
- 用户感知发送时有 30+ 秒延迟（实际是在压缩）
- "预处理"名不副实，实际压缩发生在发送时

### 架构改造内容

1. **后端改动**
   - `pdf_processing_service.rs`：
     - 修改 `stage_image_compression()`：移除大小阈值检查，对所有图片强制使用 `low` 质量压缩
     - 新增 `stage_pdf_page_compression()`：为 PDF 每页生成压缩版本
     - 新增 `check_pdf_pages_need_compression()`：检查 PDF 页面是否需要压缩
   - `model2_pipeline.rs`：移除发送时的 `adjust_image_quality_base64` 调用
   - `file_repo.rs`：
     - 添加 `compressed_blob_hash` 字段到所有 SQL 查询
     - 修改 `get_content_with_conn()` 优先读取压缩版本
   - `ref_handlers.rs`：PDF 页面读取时优先使用 `compressedBlobHash`
   - `types.rs`：
     - `VfsFile` 添加 `compressed_blob_hash` 字段
     - `PdfPagePreview` 添加 `compressed_blob_hash` 字段

2. **前端改动**
   - `pdfProcessingStore.ts`：修改 `areAllModesReady()`，图片必须等待预处理完成才能发送
   - `InputBarUI.tsx`：附件列表对 PDF/图片显示阶段进度（页码 + 百分比），提升用户心智一致性

3. **数据流变更**
   ```
   【旧流程】
   上传 → (可选压缩) → 发送时压缩 → LLM

   【新流程】
   上传 → 强制预处理压缩 → 等待完成 → 发送（直接使用压缩版本）→ LLM
   ```

4. **发送时机控制**
   - 预处理未完成时，发送按钮禁用
   - 前端 `hasProcessingMedia` 检查 + `areAllModesReady` 函数确保只有预处理完成后才能发送

### 批判性检查发现的问题（v2.2.1 修复）

1. **`ready_modes` 过早包含 `image`**
   - **问题**：图片/PDF 流水线初始化时就把 `image` 加入 `ready_modes`，导致前端误判为可发送
   - **修复**：
     - `run_image_pipeline_internal()`：初始 `ready_modes` 为空，压缩完成后才添加 `image`
     - `run_pdf_pipeline_internal()`：初始 `ready_modes` 不包含 `image`，页面压缩完成后才添加
     - `vfs_upload_attachment`：返回的初始 `ready_modes` 不包含 `image`

2. **前端默认值不一致**
   - **问题**：前端默认 `readyModes: ['image']`，与后端改造后的逻辑冲突
   - **修复**：更新 `InputBarUI.tsx` 中的默认值（PDF: `['text']`，图片: `[]`）

3. **压缩失败导致无法发送**
   - **问题**：如果压缩失败，`image` 永远不会加入 `ready_modes`
   - **修复**：压缩失败时使用原图回退，仍然标记 `image` 就绪

4. **图片 `media_type` 不正确**
   - **问题**：使用压缩版本时，`media_type` 仍然是原文件扩展名推断的值
   - **修复**：`vfs_resolver.rs` 中检查 `compressed_blob_hash`，使用压缩版本时返回 `image/jpeg`

### 边缘情况处理

| 场景 | 处理方式 |
|------|----------|
| 压缩失败 | 回退到原图，仍然标记 `image` 就绪 |
| 复用已有附件 | 从数据库读取实际的 `processing_status` 和 `ready_modes` |
| 预处理事件未到达 | `areAllModesReady` 返回 `false`，禁止发送 |
| PDF 页面已有压缩版本 | 跳过压缩，直接标记 `image` 就绪 |
| 无 blob_hash | 直接标记 `image` 就绪（小文件 inline 存储） |
| 压缩功能被禁用 | 直接标记 `image` 就绪（使用原图） |

### v2.2.2 修复（OCR 格式兼容 & 状态一致性）

1. **OCR JSON 兼容解析**
   - `ocr_pages_json` 统一使用 `parse_ocr_pages_json` 解析（支持 `OcrPagesJson` 与旧格式）
   - 附件/文件的 Unit 构建与索引按页解析同步支持新格式

2. **索引重置的一致性**
   - `reset_all_index_state` 同步清理 `resources` 与业务表的 `mm_index_state/mm_indexed_pages_json`
   - 避免“多模态显示已索引但向量已清空”的状态漂移

### v2.2.3 修复（状态视图一致性）

1. **题目集多模态维度/模式显示**
   - 状态页读取题目集的 `mm_embedding_dim/mm_indexing_mode` 从 `exam_sheets.mm_indexed_pages_json` 取值
   - 避免题目集显示“已索引但无维度/模式”

2. **文件 OCR 状态识别增强**
   - `file` 类型的 OCR 状态同时识别 `extracted_text`、`ocr_pages_json` 与 `resources.ocr_text`
   - 解决扫描 PDF 已 OCR 但状态页仍显示“无文本”的问题

3. **批量索引失败状态恢复**
   - 文本批量索引失败时自动回滚前端进度状态，防止 UI 卡住

## v2.1 更新说明

添加媒体缓存管理功能：

1. **后端命令** (`src-tauri/src/vfs/handlers.rs`)
   - `vfs_get_media_cache_stats` - 获取缓存统计
   - `vfs_clear_media_cache` - 清理缓存并重置状态

2. **前端组件** (`src/components/settings/MediaCacheSection.tsx`)
   - 缓存统计卡片显示
   - 选择性清理不同类型缓存
   - 确认对话框和进度反馈

3. **设置页面集成**
   - 在「数据治理」页面添加「缓存」Tab

## v2.0 更新说明

本版本将原有的 PDF 预处理流水线扩展为 PDF + 图片通用的媒体预处理流水线：

### 主要改动

1. **后端扩展** (`src-tauri/src/vfs/pdf_processing_service.rs`)
   - 添加 `MediaType` 枚举（`Pdf` | `Image`）
   - 添加 `ImageCompression` 处理阶段
   - 实现 `run_image_pipeline_internal()` 图片处理流水线
   - 实现 `stage_image_compression()` 图片压缩（可选，大于阈值才压缩）
   - 实现 `stage_image_ocr()` 图片 OCR（复用 LLM OCR API）
   - 扩展事件系统支持 `media-processing-*` 统一事件

2. **上传入口改造** (`src-tauri/src/vfs/handlers.rs`)
   - `vfs_upload_attachment` 和 `vfs_upload_file` 现支持图片自动触发流水线

3. **前端状态管理** (`src/stores/pdfProcessingStore.ts`)
   - 扩展 `ProcessingStage` 类型支持 `image_compression`
   - 扩展 `PdfProcessingStatus` 添加 `mediaType` 字段

4. **前端事件监听** (`src/hooks/usePdfProcessingProgress.ts`)
   - 监听新的 `media-processing-*` 统一事件
   - 保持旧的 `pdf-processing-*` 事件兼容

5. **数据库迁移** (`src-tauri/migrations/vfs/V20260205__add_compressed_blob_hash.sql`)
   - 添加 `compressed_blob_hash` 字段存储压缩后的图片引用

## 1. 背景与目标

### 1.1 当前痛点

| 问题 | PDF | 图片 | 影响 |
|------|-----|------|------|
| **重复渲染** | 预渲染 + OCR渲染分开 | N/A | 浪费资源 |
| **现场处理** | 选择模式时才处理 | 无 OCR 支持 | 发送卡顿 |
| **无状态反馈** | ✅ 已解决 | ❌ 无进度 | 用户体验差 |
| **手动索引** | ✅ 已自动化 | ❌ 需手动 | 使用不便 |
| **无压缩** | N/A | ❌ 大图片占用空间 | 存储浪费 |

### 1.2 目标

1. **上传即处理**：PDF 和图片上传后自动执行预处理流水线
2. **统一架构**：PDF 和图片共享相同的处理架构和事件系统
3. **状态可见**：前端实时显示处理进度（PDF 和图片统一）
4. **智能拦截**：未完成对应处理时，禁止选择该注入模式发送
5. **图片优化**：自动压缩、OCR、向量索引

---

## 2. 架构设计

### 2.1 流水线架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                    PDF 预处理流水线（上传后自动触发）              │
└─────────────────────────────────────────────────────────────────┘

PDF 上传（附件上传 / 学习资源管理器 / 教材导入）
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 1: 文本提取 (text_extraction)                            │
│  ├─ 使用 pdf-extract 库提取文本                                 │
│  ├─ 结果存入 files.extracted_text                               │
│  └─ 完成后：text 注入模式就绪                                    │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 2: 页面图片化 (page_rendering)                            │
│  ├─ DPI=150 渲染所有页面为 PNG                                  │
│  ├─ 使用 Triangle 滤波器（速度优先）                             │
│  ├─ 存入 vfs_blobs（基于内容哈希去重）                           │
│  └─ 更新 files.preview_json（记录所有页的 blob_hash）            │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 2.5: 页面压缩 (page_compression) 【v2.2 新增】            │
│  ├─ 使用 low 质量压缩所有页面（JPEG 格式）                       │
│  ├─ 存入 vfs_blobs（压缩后的 blob）                              │
│  ├─ 更新 preview_json 中每页的 compressed_blob_hash              │
│  └─ 完成后：image 注入模式真正就绪                               │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 3: OCR 识别 (ocr_processing)                             │
│  ├─ 复用 Stage 2 的图片（不再重新渲染）                          │
│  ├─ 并发调用 LLM OCR API（最多 4 并发）                         │
│  ├─ 结果存入 files.ocr_pages_json                               │
│  └─ 完成后：ocr 注入模式就绪                                     │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 4: 向量索引 (vector_indexing)                             │
│  ├─ 文本向量化：extracted_text + ocr_pages                      │
│  ├─ 多模态向量化：页面图片 embeddings                           │
│  ├─ 存入 LanceDB                                                 │
│  └─ 完成后：知识库检索就绪                                       │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
处理完成 → 前端可选择任意注入模式，知识库检索可用
```

### 2.1.1 图片预处理流水线（v2.0 新增）

```
┌─────────────────────────────────────────────────────────────────┐
│                图片预处理流水线（上传后自动触发）                  │
└─────────────────────────────────────────────────────────────────┘

图片上传（附件上传 / 学习资源管理器）
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 1: 图片压缩 (image_compression)                          │
│  ├─ 检测原始尺寸和文件大小                                       │
│  ├─ 大于 2MP 或 > 1MB 自动压缩                                   │
│  ├─ 使用 Triangle 滤波器（速度优先）                             │
│  ├─ 输出 JPEG 格式（质量 75-85）                                 │
│  ├─ 压缩版存入 vfs_blobs（通过 compressed_blob_hash 引用）       │
│  └─ 完成后：image 注入模式就绪                                   │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 2: OCR 识别 (ocr_processing)                             │
│  ├─ 调用 LLM OCR API（单张图片）                                │
│  ├─ 结果存入 files.ocr_text                                     │
│  └─ 完成后：ocr 注入模式就绪                                     │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
┌─────────────────────────────────────────────────────────────────┐
│  Stage 3: 向量索引 (vector_indexing)                             │
│  ├─ 文本向量化：ocr_text                                         │
│  ├─ 多模态向量化：压缩后图片 embedding                           │
│  ├─ 存入 LanceDB                                                 │
│  └─ 完成后：知识库检索就绪                                       │
└─────────────────────────────────────────────────────────────────┘
    │
    ▼
处理完成 → 前端可选择任意注入模式，知识库检索可用
```

### 2.1.2 媒体类型处理对比

| 特性 | PDF | 图片 |
|------|-----|------|
| 处理阶段 | 4 阶段 | 3 阶段 |
| 文本提取 | ✅ pdf-extract | ❌ 无原生文本 |
| 页面渲染 | ✅ 多页渲染 | ❌ 不需要（本身是图片）|
| 图片压缩 | ❌ 渲染时已控制 | ✅ 自动压缩优化 |
| OCR | ✅ 多页并发 | ✅ 单张处理 |
| 向量索引 | ✅ 文本+多模态 | ✅ 文本+多模态 |

### 2.2 状态机

#### 2.2.1 PDF 状态机

```
         ┌─────────┐
         │ pending │ ← 初始状态（上传完成）
         └────┬────┘
              │ start_pipeline()
              ▼
    ┌──────────────────┐
    │ text_extraction  │ → files.extracted_text
    └────────┬─────────┘
             │
             ▼
    ┌──────────────────┐
    │ page_rendering   │ → files.preview_json (all pages)
    └────────┬─────────┘
             │
             ▼
    ┌──────────────────┐
    │ ocr_processing   │ → files.ocr_pages_json
    └────────┬─────────┘
             │
             ▼
    ┌──────────────────┐
    │ vector_indexing  │ → LanceDB
    └────────┬─────────┘
             │
             ▼
       ┌───────────┐
       │ completed │
       └───────────┘

任何阶段失败 → error 状态（记录错误信息，支持重试）
```

#### 2.2.2 图片状态机（v2.0 新增）

```
         ┌─────────┐
         │ pending │ ← 初始状态（上传完成）
         └────┬────┘
              │ start_pipeline()
              ▼
    ┌──────────────────────┐
    │ image_compression    │ → files.compressed_blob_hash (可选)
    └────────┬─────────────┘
             │ image 模式就绪
             ▼
    ┌──────────────────┐
    │ ocr_processing   │ → files.ocr_text
    └────────┬─────────┘
             │ ocr 模式就绪
             ▼
    ┌──────────────────┐
    │ vector_indexing  │ → LanceDB
    └────────┬─────────┘
             │
             ▼
       ┌───────────┐
       │ completed │
       └───────────┘

任何阶段失败 → error 状态（记录错误信息，支持重试）
```

#### 2.2.3 统一处理阶段枚举

```rust
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStage {
    // PDF 专用阶段
    TextExtraction,    // PDF 文本提取
    PageRendering,     // PDF 页面渲染

    // 图片专用阶段
    ImageCompression,  // 图片压缩

    // 共享阶段
    OcrProcessing,     // OCR 处理（PDF 多页 / 图片单张）
    VectorIndexing,    // 向量索引

    // 终态
    Completed,
}
```

---

## 3. 数据模型改造

### 3.1 数据库表改造

```sql
-- files 表新增字段（已实现）
ALTER TABLE files ADD COLUMN processing_status TEXT DEFAULT 'pending';
-- PDF 可选值: pending | text_extraction | page_rendering | page_compression | ocr_processing | vector_indexing | completed | error
-- 图片可选值: pending | image_compression | ocr_processing | vector_indexing | completed | error

ALTER TABLE files ADD COLUMN processing_progress TEXT;
-- JSON 格式: {
--   "stage": "page_rendering",           -- 或 "page_compression" / "image_compression"
--   "current_page": 10,                  -- PDF 专用
--   "total_pages": 50,                   -- PDF 专用
--   "percent": 20.0,
--   "ready_modes": ["text"],             -- PDF: ["text", "image", "ocr"]
--                                        -- 图片: ["image", "ocr"]
--   "media_type": "pdf"                  -- 新增：pdf | image
-- }

-- 图片压缩专用字段（v2.0 新增）
ALTER TABLE files ADD COLUMN compressed_blob_hash TEXT;
-- 压缩后图片的 blob_hash，如果不需要压缩则为 NULL

ALTER TABLE files ADD COLUMN processing_error TEXT;
-- 错误信息（error 状态时填充）

ALTER TABLE files ADD COLUMN processing_started_at INTEGER;
-- 处理开始时间戳

ALTER TABLE files ADD COLUMN processing_completed_at INTEGER;
-- 处理完成时间戳
```

### 3.2 preview_json 扩展

```json
{
  "pages": [
    {
      "page_index": 0,
      "blob_hash": "abc123...",
      "width": 1200,
      "height": 1600,
      "mime_type": "image/jpeg"
    }
  ],
  "render_dpi": 150,
  "total_pages": 100,
  "rendered_pages": 100,
  "rendered_at": "2026-02-02T08:00:00Z",
  "render_mode": "full"
}
```

### 3.2.1 ocr_pages_json 格式

`ocr_pages_json` 作为 PDF 页级 OCR 的统一存储字段，已支持以下格式并保持向后兼容：

- **新格式（推荐）**：结构化 JSON，包含页索引与 OCR 块

```json
{
  "total_pages": 3,
  "pages": [
    {
      "page_index": 0,
      "blocks": [{ "text": "第一页文本", "bbox": [0, 0, 100, 20] }]
    },
    {
      "page_index": 1,
      "blocks": []
    },
    {
      "page_index": 2,
      "blocks": [{ "text": "第三页文本" }]
    }
  ],
  "completed_at": "2026-02-02T08:00:00Z"
}
```

- **旧格式（兼容）**：按页文本数组

```json
["第一页文本", null, "第三页文本"]
```

- **旧格式（兼容）**：纯字符串数组

```json
["第一页文本", "第二页文本"]
```

**读取约定**：
- 读取时需自动兼容上述格式，并忽略空页/空文本。
- 组装全文时建议按页插入分隔符（如 `--- 第 N 页 ---`）。
- 统计页数时需兼容对象/数组两类格式：新格式读取 `pages` 数组长度，旧格式直接取数组长度。

### 3.3 处理进度结构

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingProgress {
    /// 当前阶段
    pub stage: ProcessingStage,
    /// 当前处理的页码（PDF 渲染/OCR 时使用）
    pub current_page: Option<usize>,
    /// 总页数（PDF 专用，图片始终为 1）
    pub total_pages: Option<usize>,
    /// 总进度百分比 (0-100)
    pub percent: f32,
    /// 已就绪的注入模式
    /// - PDF: ["text", "image", "ocr"]
    /// - 图片: ["image", "ocr"]
    pub ready_modes: Vec<String>,
    /// 媒体类型（v2.0 新增）
    pub media_type: MediaType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaType {
    Pdf,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingStage {
    Pending,
    // PDF 专用
    TextExtraction,
    PageRendering,
    // 图片专用
    ImageCompression,
    // 共享阶段
    OcrProcessing,
    VectorIndexing,
    Completed,
    Error,
}
```

> `processing_progress.stage` 存储为小写 snake_case（如 `completed`），读取时需大小写兼容。

---

## 4. 后端改造

### 4.1 服务模块（扩展为通用媒体处理）

```
src-tauri/src/vfs/
├── media_processing_service.rs  # 🔄 重命名：通用媒体预处理服务（原 pdf_processing_service.rs）
└── ...
```

> 实际实现保持 `pdf_processing_service.rs` 文件名不变，但内部支持图片处理。

### 4.2 核心接口

```rust
// src-tauri/src/vfs/pdf_processing_service.rs
// 重命名为 MediaProcessingService 但保持文件名兼容

pub struct PdfProcessingService {  // 对外名称保持兼容
    db: Arc<VfsDatabase>,
    blob_repo: Arc<VfsBlobRepo>,
    file_manager: Arc<FileManager>,    // 新增：用于图片压缩
    llm_manager: Arc<LLMManager>,      // 用于 OCR
    index_service: Arc<VfsIndexService>,
    full_indexing_service: Arc<VfsFullIndexingService>,
    // 运行中的任务追踪
    running_tasks: DashMap<String, CancellationToken>,
}

impl PdfProcessingService {
    /// 启动预处理流水线（上传后自动调用）
    /// 异步执行，立即返回
    /// - PDF: text_extraction → page_rendering → ocr → vector_indexing
    /// - 图片: image_compression → ocr → vector_indexing
    pub async fn start_pipeline(
        &self, 
        file_id: &str, 
        start_stage: ProcessingStage  // 可指定起始阶段
    ) -> Result<()>;
    
    /// 获取处理状态
    pub fn get_status(&self, file_id: &str) -> Result<ProcessingStatus>;
    
    /// 取消处理
    pub fn cancel(&self, file_id: &str) -> Result<()>;
    
    /// 重试失败的处理
    pub async fn retry(&self, file_id: &str) -> Result<()>;
    
    // === 图片专用方法（v2.0 新增）===
    
    /// Stage 1: 图片压缩
    async fn stage_image_compression(&self, file_id: &str) -> Result<Option<String>>;
    
    /// Stage 2: 图片 OCR（复用 PDF OCR 能力）
    async fn stage_image_ocr(&self, file_id: &str) -> Result<String>;
}
```

### 4.3 上传入口改造

需要改造的入口（v2.0 扩展）：

| 入口 | 文件位置 | 改造内容 |
|------|----------|----------|
| 附件上传 | `vfs/repos/attachment_repo.rs::upload_with_conn` | PDF/图片上传后触发 pipeline |
| 文件上传 | `vfs/handlers.rs::vfs_upload_file` | PDF/图片上传后触发 pipeline |
| 教材导入 | `cmd/textbooks.rs::textbooks_add` | 复用 pipeline |

**MIME 类型判断**：
```rust
fn get_media_type(mime_type: &str) -> Option<MediaType> {
    if mime_type == "application/pdf" {
        Some(MediaType::Pdf)
    } else if mime_type.starts_with("image/") {
        Some(MediaType::Image)
    } else {
        None  // 其他类型不触发 pipeline
    }
}
```

### 4.4 图片压缩策略

```rust
/// 图片压缩配置
pub struct ImageCompressionConfig {
    /// 是否启用压缩（默认 true）
    pub enabled: bool,
    /// 压缩阈值：超过此大小才压缩（默认 1MB）
    pub size_threshold: usize,
    /// 像素阈值：超过此像素才压缩（默认 2 百万像素 = 2MP）
    pub pixel_threshold: usize,
    /// 压缩质量（默认 "medium"）
    pub quality: String,  // "low" | "medium" | "high" | "auto"
}

impl Default for ImageCompressionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            size_threshold: 1 * 1024 * 1024,  // 1MB
            pixel_threshold: 2_000_000,        // 2MP
            quality: "medium".to_string(),
        }
    }
}
```

### 4.5 OCR 执行策略

- 由设置项控制是否执行 OCR（后端流水线读取设置）：
  - `ocr.enabled`: 总开关
  - `ocr.images`: 是否对图片启用 OCR
  - `ocr.scanned_pdf`: 是否对扫描版 PDF 启用 OCR
  - `ocr.pdf_text_threshold`: PDF 文本阈值（低于阈值触发 OCR）
  - `ocr.skip_for_multimodal`: 是否在多模态场景跳过 OCR
- 当 OCR 被跳过时，`ready_modes` 不会包含 `ocr`，前端应阻止选择 OCR 注入模式发送

### 4.6 事件发射

```rust
// 进度事件
window.emit("pdf-processing-progress", json!({
    "file_id": file_id,
    "status": {
        "stage": "page_rendering",
        "current_page": 10,
        "total_pages": 50,
        "percent": 20.0,
        "ready_modes": ["text"]
    }
}));

// 完成事件
window.emit("pdf-processing-completed", json!({
    "file_id": file_id,
    "ready_modes": ["text", "image", "ocr"]
}));

// 错误事件
window.emit("pdf-processing-error", json!({
    "file_id": file_id,
    "error": "OCR API 调用失败",
    "stage": "ocr_processing"
}));
```

> 注意：OCR/向量索引失败在新实现中**不再触发全局 error 事件**（流水线继续完成），
> 是否可注入由 `ready_modes` 决定；仅致命错误才会进入 `error` 状态。

---

## 5. 前端改造

### 5.1 状态管理（v2.0 扩展为通用媒体处理）

```typescript
// src/stores/mediaProcessingStore.ts（原 pdfProcessingStore.ts 扩展）

type MediaType = 'pdf' | 'image';
type ProcessingStage = 
  | 'pending' 
  | 'text_extraction'      // PDF 专用
  | 'page_rendering'       // PDF 专用
  | 'page_compression'     // PDF 专用
  | 'image_compression'    // 图片专用
  | 'ocr_processing' 
  | 'vector_indexing' 
  | 'completed' 
  | 'error';

interface MediaProcessingStatus {
  mediaType: MediaType;
  stage: ProcessingStage;
  currentPage?: number;    // PDF: 当前页；图片: 始终为 1
  totalPages?: number;     // PDF: 总页数；图片: 始终为 1
  percent: number;
  readyModes: Array<'text' | 'ocr' | 'image'>;  // PDF 有 text，图片没有
  error?: string;
}

// 全局状态 Map: fileId -> MediaProcessingStatus
// 统一管理 PDF 和图片的处理状态
const mediaProcessingStatus = new Map<string, MediaProcessingStatus>();

// 兼容旧 API
export const usePdfProcessingStore = useMediaProcessingStore;
```

### 5.2 事件监听（v2.0 统一事件）

```typescript
// src/hooks/useMediaProcessingProgress.ts（原 usePdfProcessingProgress.ts 扩展）

export function useMediaProcessingProgress() {
  useEffect(() => {
    // 统一事件监听：支持 PDF 和图片
    const unlistenProgress = listen('media-processing-progress', (event) => {
      const { fileId, status } = event.payload;
      mediaProcessingStore.update(fileId, status);
    });
    
    const unlistenCompleted = listen('media-processing-completed', (event) => {
      const { fileId, readyModes, mediaType } = event.payload;
      mediaProcessingStore.setCompleted(fileId, readyModes, mediaType);
    });
    
    const unlistenError = listen('media-processing-error', (event) => {
      const { fileId, error, stage, mediaType } = event.payload;
      mediaProcessingStore.setError(fileId, error, stage, mediaType);
    });
    
    // 兼容旧事件（PDF 专用，渐进迁移）
    const unlistenPdfProgress = listen('pdf-processing-progress', (event) => {
      const { fileId, status } = event.payload;
      mediaProcessingStore.update(fileId, { ...status, mediaType: 'pdf' });
    });
    
    return () => {
      unlistenProgress();
      unlistenCompleted();
      unlistenError();
      unlistenPdfProgress();
    };
  }, []);
}

// 兼容旧 Hook
export const usePdfProcessingProgress = useMediaProcessingProgress;
```

### 5.3 UI 改造

#### 5.3.1 注入模式选择器（v2.0 支持图片）

```tsx
// AttachmentInjectModeSelector.tsx

// 统一处理 PDF 和图片的模式选择
function MediaModeSelector({ attachment, processingStatus }) {
  const mediaType = processingStatus?.mediaType || 
    (attachment.mimeType === 'application/pdf' ? 'pdf' : 'image');
  
  if (mediaType === 'pdf') {
    return <PdfModeSelector attachment={attachment} processingStatus={processingStatus} />;
  } else {
    return <ImageModeSelector attachment={attachment} processingStatus={processingStatus} />;
  }
}

// PDF 模式选择器（保持不变）
function PdfModeSelector({ attachment, processingStatus }) {
  const isTextReady = processingStatus?.readyModes.includes('text') ?? true;
  const isOcrReady = processingStatus?.readyModes.includes('ocr');
  const isImageReady = processingStatus?.readyModes.includes('image');
  
  return (
    <div className="pdf-mode-selector">
      <ModeButton mode="text" enabled={isTextReady} />
      {isOcrReady ? (
        <ModeButton mode="ocr" enabled={true} />
      ) : (
        <ProcessingIndicator 
          label="OCR 处理中"
          progress={processingStatus?.percent}
          currentPage={processingStatus?.currentPage}
          totalPages={processingStatus?.totalPages}
        />
      )}
      {isImageReady ? (
        <ModeButton mode="image" enabled={true} />
      ) : (
        <ProcessingIndicator label="图片渲染中" progress={processingStatus?.percent} />
      )}
    </div>
  );
}

// 图片模式选择器（v2.0 新增）
function ImageModeSelector({ attachment, processingStatus }) {
  const isImageReady = processingStatus?.readyModes.includes('image') ?? true;
  const isOcrReady = processingStatus?.readyModes.includes('ocr');
  const isCompressing = processingStatus?.stage === 'image_compression';
  const isOcrProcessing = processingStatus?.stage === 'ocr_processing';
  
  return (
    <div className="image-mode-selector">
      {/* 图片模式 - 压缩完成后就绪 */}
      {isImageReady ? (
        <ModeButton mode="image" enabled={true} />
      ) : isCompressing ? (
        <ProcessingIndicator 
          label="图片压缩中"
          progress={processingStatus?.percent}
        />
      ) : (
        <ModeButton mode="image" enabled={false} />
      )}
      
      {/* OCR 模式 */}
      {isOcrReady ? (
        <ModeButton mode="ocr" enabled={true} />
      ) : isOcrProcessing ? (
        <ProcessingIndicator 
          label="OCR 处理中"
          progress={processingStatus?.percent}
        />
      ) : (
        <ModeButton mode="ocr" enabled={false} label="OCR 等待中" />
      )}
    </div>
  );
}
```

#### 5.3.2 发送按钮禁用逻辑

```tsx
// InputBarUI.tsx

const canSend = useMemo(() => {
  // 检查所有 PDF 附件的选中模式是否就绪
  return attachments.every(att => {
    if (!isPdf(att)) return true;
    
    const selectedModes = att.injectModes?.pdf || ['text'];
    const status = pdfProcessingStore.get(att.fileId);
    const readyModes = status?.readyModes || ['text']; // 默认文本模式就绪
    
    return selectedModes.every(mode => readyModes.includes(mode));
  });
}, [attachments, pdfProcessingStatus]);

// 显示提示信息
{!canSend && (
  <Tooltip content="部分附件正在处理中，请等待完成或切换注入模式">
    <Button disabled>发送</Button>
  </Tooltip>
)}
```

---

## 6. 性能考量

### 6.1 渲染优化

| 优化项 | 方法 | 预期效果 |
|--------|------|----------|
| 快速滤波器 | Triangle 替代 Lanczos3 | 提速 3-5 倍 |
| JPEG 格式 | PNG → JPEG (quality=75) | 文件大小减少 60% |
| 并行渲染 | 多线程渲染（4 线程） | 提速 3-4 倍 |
| 内存控制 | 流式处理，及时释放 | 避免 OOM |

### 6.2 OCR 优化

| 优化项 | 方法 | 预期效果 |
|--------|------|----------|
| 图片复用 | 直接使用预渲染图片 | 节省 50% 时间 |
| 并发调用 | 4 并发 API 调用 | 提速 4 倍 |
| 结果缓存 | 基于 blob_hash 缓存 | 重复 PDF 秒级返回 |

### 6.3 预估处理时间

以 50 页 A4 PDF 为例：

| 阶段 | 优化前 | 优化后 |
|------|--------|--------|
| 文本提取 | 2s | 2s |
| 页面渲染 | 25s | 8s |
| OCR 处理 | 60s | 15s |
| 向量索引 | 10s | 10s（后台） |
| **总计** | **97s** | **35s** |

---

## 7. 兼容性与迁移

### 7.1 向后兼容

- 已上传的 PDF 文件：保持 `processing_status = 'completed'`（假设已完成）
- 旧的 `preview_json` 格式：自动识别，按需重新渲染
- 旧的 OCR 缓存：保持可用，逐步迁移到新格式

### 7.2 数据迁移

```sql
-- 迁移脚本：为已有 PDF 文件设置默认状态
UPDATE files 
SET processing_status = 'completed',
    processing_progress = '{"stage":"completed","percent":100,"ready_modes":["text"]}'
WHERE mime_type = 'application/pdf' 
  AND processing_status IS NULL;
```

---

## 8. 调研结果确认

### 8.1 上传入口调研结果 ✅

| 问题 | 结论 |
|------|------|
| `vfs_upload_file` 和 `upload_with_conn` 的关系 | **独立**：`vfs_upload_file` 直接调用 `render_pdf_preview()`，未复用 `upload_with_conn`。建议统一到 `upload_with_conn` 或都触发 pipeline |
| 教材导入是否可以复用 pipeline | **可以**：`textbooks_add` 已支持进度回调（`render_pdf_preview_with_progress`），可改为触发 pipeline |
| 统一触发点 | **建议在 `upload_with_conn()` 触发**，三个入口都会调用（直接或间接） |

**三个入口对比**：

| 入口 | 文件位置 | 当前 PDF 处理 | 进度支持 |
|------|----------|--------------|----------|
| `vfs_upload_attachment` | `attachment_repo.rs:162` | `render_pdf_preview()` | ❌ |
| `vfs_upload_file` | `handlers.rs:1614` | `render_pdf_preview()` | ❌ |
| `textbooks_add` | `textbooks.rs:179` | `render_pdf_preview_with_progress()` | ✅ |

### 8.2 OCR 缓存和图片复用调研结果 ✅

| 特性 | 预渲染图片 (vfs_blobs) | OCR 图片 (pdf_ocr_images) |
|------|------------------------|--------------------------|
| 存储位置 | `vfs_blobs/{hash[0:2]}/{hash}.png` | `pdf_ocr_images/{session_id}/page_{:05}.jpg` |
| 格式 | **PNG** | **JPEG** |
| DPI | 150（默认） | 150（默认），最大 300 |
| 哈希算法 | SHA-256（图片内容） | SHA-256（PDF 文件） |
| 引用计数 | 有 | 无 |
| 清理策略 | 引用计数为 0 时清理 | 无自动清理（问题！） |

**图片复用可行性**：✅ **高**
- DPI 一致（默认 150）
- 渲染引擎一致（pdfium）
- 需要处理格式差异：建议统一使用 JPEG（更小），或 OCR API 支持 PNG

**改造方案**：
1. `PdfOcrService` 注入 `VfsDatabase`
2. 在渲染前检查 `preview_json` 中是否已有对应页面的 blob
3. 如有则直接使用，无则渲染新图片

### 8.3 前端状态管理调研结果 ✅

**当前问题**：
1. `AttachmentMeta.status` 缺少 `processing` 状态
2. 上传完成后立即设为 `ready`，但 PDF 可能仍在处理
3. 发送按钮仅检查 `uploading`，不检查处理中状态

**需要改造的类型**：

```typescript
// 改造前
status: 'pending' | 'uploading' | 'ready' | 'error';

// 改造后
status: 'pending' | 'uploading' | 'processing' | 'ready' | 'error';
processingStatus?: {
  stage?: 'text_extraction' | 'page_rendering' | 'ocr_processing' | 'vector_indexing';
  progress?: number;
  readyModes?: PdfInjectMode[];
  error?: string;
};
```

**需要改造的组件**：

| 文件 | 改造内容 | 优先级 |
|------|----------|--------|
| `src/chat-v2/core/types/common.ts` | 增加 `processing` 状态和 `processingStatus` | P0 |
| `src/chat-v2/components/input-bar/InputBarUI.tsx` | 上传完成后设为 `processing`，监听事件，更新 `disabledSend` | P0 |
| `src/hooks/usePdfProcessingProgress.ts` | 🆕 创建 Hook 监听事件 | P0 |
| `src/chat-v2/components/input-bar/AttachmentInjectModeSelector.tsx` | 显示进度，禁用未就绪模式 | P1 |

### 8.4 VFS 索引和召回体系调研结果 ✅

#### 8.4.1 索引服务架构

**核心服务**：`VfsIndexService`（`src-tauri/src/vfs/index_service.rs`）

**三层存储结构**：
| 层级 | 存储 | 内容 |
|------|------|------|
| Unit 元数据 | SQLite `vfs_index_units` | 资源分块信息、索引状态 |
| Segment 元数据 | SQLite `vfs_index_segments` | 向量块信息、lance_row_id |
| 向量数据 | LanceDB `vfs_emb_{modality}_{dim}` | 实际向量（如 `vfs_emb_text_768`） |

**双模态支持**：
- `text_state`: 文本向量索引状态（pending → indexing → indexed）
- `mm_state`: 多模态向量索引状态

**关键接口**：
```rust
// 同步 Units（资源创建/更新后调用）
sync_resource_units(input: UnitBuildInput) -> Vec<UnitIndexStatus>

// 批量索引待处理 Units
process_pending_batch(mode: "text"|"mm"|"both", batch_size) -> BatchIndexResult

// 检索
search_with_resource_info(params: VfsSearchParams) -> Vec<VfsSearchResult>
```

#### 8.4.2 当前索引触发时机

| 场景 | 触发方式 | 位置 |
|------|----------|------|
| 资源创建/更新 | `sync_resource_units()` 创建 Units（状态 pending） | 各资源 repo |
| 手动批量索引 | `vfs_unified_batch_index` 命令 | 前端 IndexStatusView |
| 题目集识别完成 | 异步触发多模态索引 | `useExamSheetProgress.ts` |

**问题**：当前无后台自动索引任务，依赖前端主动调用

#### 8.4.3 索引流程

```
资源创建/更新
    ↓
sync_resource_units() → 生成 Units（状态: pending）
    ↓
[手动/自动触发批量索引]
    ↓
VfsFullIndexingService::process_pending_batch()
    ↓
├─ 文本索引:
│   ├─ 提取文本 → 分块 → Embedding → LanceDB
│   └─ 状态: indexed
│
└─ 多模态索引:
    ├─ 准备图片 → VL-Embedding → LanceDB
    └─ 状态: indexed
```

#### 8.4.4 检索流程

**Tauri 命令**：`vfs_rag_search`

**检索模式**：
- 纯向量检索：`vector_search()`
- 混合检索：`hybrid_search()`（FTS + 向量 + RRF 融合）
- 多模态检索：`multimodal_service.search_full()`

**距离类型**：`DistanceType::Cosine`（余弦相似度）

**重排序**：可选，调用 `LLMManager::call_reranker_api`

#### 8.4.5 前端集成点

| 组件 | 功能 |
|------|------|
| `IndexStatusView.tsx` | 索引状态总览、批量操作、进度监听 |
| `RagPanel.tsx` | RAG 参数配置（Top-K、Rerank、多模态开关） |
| `SourcePanelV2.tsx` | 检索结果展示 |
| `TauriAdapter.ts` | 发送消息时传递 RAG 参数 |

**事件监听**：
- `vfs-index-progress`: 文本索引进度
- `mm_index_progress`: 多模态索引进度（含多阶段）

#### 8.4.6 PDF 预处理与索引集成方案

**Stage 4 改造要点**：

1. **复用 `sync_resource_units()`**：
   - 在 Stage 1 完成后调用（文本提取）
   - 在 Stage 3 完成后再次调用（OCR 文本）
   - 输入：`extracted_text`、`ocr_pages_json`、`preview_json`

2. **自动触发索引**：
   - Stage 4 主动调用 `process_pending_batch()`
   - 无需等待用户手动触发

3. **进度事件整合**：
   - 复用 `vfs-index-progress` 和 `mm_index_progress` 事件
   - 前端 `processingStatus.stage = 'vector_indexing'` 时监听

4. **跳过条件**：
   - 如果用户未启用自动索引，Stage 4 可跳过
   - 检查 `files.processing_status` 避免重复索引

### 8.5 风险评估更新

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| 大 PDF 内存占用 | 中 | 流式处理（当前 pdfium 支持），限制并发渲染页数 |
| OCR API 限流 | 中 | 已有 4 并发限制（`pdf_ocr_service.rs`） |
| 断点续传 | 低 | 基于 blob_hash 天然幂等，重启后继续处理即可 |
| 存储空间 | 中 | JPEG 格式 + 引用计数清理 + 可配置最大页数 |
| 格式差异 (PNG/JPEG) | 低 | 统一使用 JPEG，或 OCR 支持 PNG |
| 索引延迟 | 低 | Stage 4 自动触发，无需用户干预 |
| 索引失败 | 低 | 复用现有重试机制，失败后可手动重索引 |

---

## 9. 实现计划

| 阶段 | 任务 | 工作量 | 优先级 |
|------|------|--------|--------|
| Phase 1 | 数据库迁移 + 后端 Service 框架 | 2h | P0 |
| Phase 1 | 文本提取 + 页面渲染阶段 | 3h | P0 |
| Phase 1 | 上传入口触发 pipeline | 1h | P0 |
| Phase 2 | OCR 处理阶段（复用图片） | 2h | P0 |
| Phase 2 | 前端状态监听 + 进度显示 | 2h | P0 |
| Phase 2 | 发送按钮禁用逻辑 | 1h | P0 |
| Phase 3 | 向量索引集成 | 2h | P1 |
| Phase 3 | 取消/重试机制 | 1h | P1 |
| Phase 4 | 教材导入复用 pipeline | 2h | P2 |
| Phase 4 | 测试 + 文档 | 2h | P2 |

**总工作量估计**: 18h

---

## 附录

### A. 相关文件清单（详细）

```
后端改造：
├── src-tauri/src/vfs/
│   ├── pdf_processing_service.rs    # 🆕 核心服务（待创建）
│   ├── repos/
│   │   ├── attachment_repo.rs       # 改造：upload_with_conn() 触发 pipeline (line 151-180)
│   │   ├── pdf_preview.rs           # 复用：render_pdf_preview_with_progress()
│   │   └── blob_repo.rs             # 复用：store_blob_with_conn()
│   └── handlers.rs                  # 改造：vfs_upload_file() 统一触发 (line 1606-1631)
├── src-tauri/src/pdf_ocr_service.rs # 改造：复用预渲染图片 (line 275-722)
├── src-tauri/src/vfs/index_service.rs # 复用：sync_resource_units()
└── src-tauri/src/cmd/textbooks.rs   # 改造：复用 pipeline (line 154-196)

前端改造：
├── src/chat-v2/core/types/common.ts                                  # 改造：AttachmentMeta 增加字段 (line 225-242)
├── src/chat-v2/components/input-bar/InputBarUI.tsx                   # 改造：
│   │                                                                 #   - 上传完成设 processing (line 496-500)
│   │                                                                 #   - 监听处理事件
│   │                                                                 #   - disabledSend 增加检查 (line 712-727)
├── src/chat-v2/components/input-bar/AttachmentInjectModeSelector.tsx # 改造：显示进度，禁用未就绪模式 (line 173-223)
├── src/hooks/usePdfProcessingProgress.ts                             # 🆕 进度监听 Hook（待创建）
└── src/components/learning-hub/LearningHubSidebar.tsx                # 复用：已有进度显示逻辑 (line 608-634)
```

### B. 核心代码位置

| 功能 | 文件 | 行号 | 说明 |
|------|------|------|------|
| PDF 检测 | `attachment_repo.rs` | 153-154 | `is_pdf = mime_type == "application/pdf"` |
| 预渲染入口 | `attachment_repo.rs` | 162 | `render_pdf_preview()` |
| 预渲染实现 | `pdf_preview.rs` | 50-180 | `render_pdf_preview_with_progress()` |
| OCR 渲染 | `pdf_ocr_service.rs` | 394-432 | `run_backend_worker()` 中的渲染逻辑 |
| OCR 缓存检查 | `pdf_ocr_service.rs` | 1234-1254 | `load_cached_blocks()` |
| Blob 存储 | `blob_repo.rs` | 57-139 | `store_blob_with_conn()` |
| 附件状态 | `common.ts` | 225-242 | `AttachmentMeta` 类型定义 |
| 发送检查 | `InputBarUI.tsx` | 712-727 | `disabledSend` 计算逻辑 |

### C. 事件定义

```typescript
// 后端发射的事件
'pdf-processing-progress': {
  file_id: string;
  status: {
    stage: 'text_extraction' | 'page_rendering' | 'ocr_processing' | 'vector_indexing';
    current_page?: number;
    total_pages?: number;
    percent: number;
    ready_modes: string[];
  }
}

'pdf-processing-completed': {
  file_id: string;
  ready_modes: ['text', 'image', 'ocr'];
}

'pdf-processing-error': {
  file_id: string;
  error: string;
  stage: string;
}
```

### D. 数据库迁移脚本

```sql
-- Migration: Add PDF processing status fields
-- Version: 2026_02_02_add_pdf_processing_status

ALTER TABLE files ADD COLUMN processing_status TEXT DEFAULT 'pending';
ALTER TABLE files ADD COLUMN processing_progress TEXT;
ALTER TABLE files ADD COLUMN processing_error TEXT;
ALTER TABLE files ADD COLUMN processing_started_at INTEGER;
ALTER TABLE files ADD COLUMN processing_completed_at INTEGER;

-- Set existing PDFs as completed (backward compatibility)
UPDATE files 
SET processing_status = 'completed',
    processing_progress = '{"stage":"completed","percent":100,"ready_modes":["text"]}'
WHERE mime_type = 'application/pdf' 
  AND processing_status = 'pending';

-- Index for status queries
CREATE INDEX IF NOT EXISTS idx_files_processing_status ON files(processing_status);
```

### E. VFS 索引体系文件清单

```
后端索引服务：
├── src-tauri/src/vfs/
│   ├── index_service.rs          # VfsIndexService 核心服务
│   ├── index_handlers.rs         # Tauri 索引命令
│   ├── indexing.rs               # VfsFullIndexingService（批量索引）
│   ├── embedding_service.rs      # 文本 Embedding 服务
│   ├── multimodal_service.rs     # 多模态 Embedding 服务
│   ├── lance_store.rs            # LanceDB 向量存储
│   └── repos/
│       ├── index_unit_repo.rs    # Unit 表操作
│       └── index_segment_repo.rs # Segment 表操作
│
└── src-tauri/src/chat_v2/tools/
    └── builtin_retrieval_executor.rs  # RAG 内置工具

前端索引组件：
├── src/components/learning-hub/views/
│   ├── IndexStatusView.tsx       # 索引状态总览
│   └── IndexDiagnosticPanel.tsx  # 索引诊断
├── src/components/shared/
│   └── MultimodalIndexButton.tsx # 多模态索引按钮
├── src/api/
│   ├── vfsUnifiedIndexApi.ts     # 统一索引 API
│   └── vfsRagApi.ts              # RAG 检索 API
└── src/stores/
    └── unifiedIndexStore.ts      # 索引状态 Store
```

### F. 索引相关 Tauri 命令

| 命令 | 功能 | 参数 |
|------|------|------|
| `vfs_unified_index_status` | 获取索引状态总览 | 无 |
| `vfs_unified_batch_index` | 批量索引待处理 | `mode`, `batch_size` |
| `vfs_sync_resource_units` | 同步资源 Units | `resource_id`, `data`, `ocr_text`... |
| `vfs_reindex_unit` | 重新索引单个 Unit | `unit_id`, `mode` |
| `vfs_delete_resource_index` | 删除资源索引 | `resource_id` |
| `vfs_rag_search` | RAG 向量检索 | `query`, `top_k`, `folder_ids`... |
| `vfs_multimodal_index` | 多模态索引资源 | `source_type`, `source_id` |

### G. 缓存管理功能

**v2.1 新增**：在设置页面「数据治理 → 缓存」Tab 中添加媒体缓存管理功能。

#### 缓存类型

| 缓存类型 | 存储位置 | 说明 |
|---------|---------|------|
| PDF 预览图片 | `vfs_blobs/` | PDF 页面渲染后的 JPEG 图片 |
| 压缩图片缓存 | `vfs_blobs/` | 图片压缩后的缓存，通过 `files.compressed_blob_hash` 引用 |
| OCR 文本 | `files.ocr_text` / `files.ocr_pages_json` | OCR 识别结果 |
| 向量索引 | `lance/vfs/` | LanceDB 向量数据 |

#### 后端命令

| 命令 | 功能 |
|------|------|
| `vfs_get_media_cache_stats` | 获取缓存统计（数量、大小） |
| `vfs_clear_media_cache` | 清理指定类型的缓存并重置处理状态 |

#### 前端组件

- `src/components/settings/MediaCacheSection.tsx` - 缓存管理 UI
- 在 `DataGovernanceDashboard` 中添加 "缓存" Tab

#### 清理行为

1. **PDF 预览图片**：清理后重新打开 PDF 会重新渲染
2. **压缩图片缓存**：清理后发送消息时会重新压缩
3. **OCR 文本**：清理后使用 OCR 模式时会重新识别
4. **向量索引**：清理后需重新建立所有资源的向量索引（智能搜索和 RAG 功能失效）

清理后会自动重置 `files` 表的 `processing_status` 为 `'pending'`，允许重新处理。

### H. 调试工具

**媒体预处理调试插件** (`src/debug-panel/plugins/MediaProcessingDebugPlugin.tsx`)

在调试面板中提供完整的媒体预处理生命周期监控：

| 功能 | 说明 |
|------|------|
| 事件流监控 | 实时显示后端发送的 `media-processing-*` 和 `pdf-processing-*` 事件 |
| Store 状态 | 显示 `pdfProcessingStore` 中所有条目的实时状态 |
| 阶段追踪 | 可视化追踪文本提取 → 页面渲染 → OCR → 向量索引的完整流程 |
| 错误诊断 | 高亮显示错误事件，帮助快速定位问题 |
| 注入模式选择 | 监听用户点击选择/取消选择注入模式（text/ocr/image） |
| 实际注入内容 | 监听发送消息时实际注入的内容块（文本块/图片块数量） |

**事件类型**：
- 🔄 `progress` - 处理进度更新（含 Store 初始化、状态同步）
- ✅ `completed` - 处理完成
- ❌ `error` - 处理错误/移除/清理
- 🖱️ `mode_change` - 用户选择注入模式（紫色）
- 📤 `inject` - 实际注入内容（青色）

**完整生命周期监听**：
1. **上传阶段**：`processing_store_init` - Store 初始化
2. **处理阶段**：后端事件 `media-processing-*`
3. **状态同步**：`status_sync_progress/completed/error`
4. **用户交互**：`inject_mode_change` 注入模式选择
5. **重试操作**：`retry_processing_start`
6. **发送阶段**：`format_resource_done` 实际注入内容
7. **清理阶段**：`attachment_remove`、`processing_store_cleanup`

**使用方式**：打开调试面板 → 选择「媒体预处理调试」插件 → 上传 PDF/图片附件观察事件流。

**关键检查点**：
1. `fileId` 应为 `sourceId`（附件 ID，格式 `att_xxx`）
2. 事件更新的 key 应与 Store 查询的 key 一致
3. `readyModes` 应随阶段推进逐步增加
4. 注入模式选择应正确反映用户点击
5. 实际注入内容应与选择的模式一致
6. 移除/清理操作应正确触发 Store 清理

### I. 已知问题与修复历史

#### 2026-02-02: Store Key 不一致问题（P0 修复）

**问题描述**：前端进度显示卡在 0%，无法更新。

**根因**：
- 后端发送事件时使用 `file_id`（附件 ID）
- 前端初始化和查询 Store 时使用 `resourceId`（资源 ID）
- 两者不同导致事件无法匹配

**修复方案**：统一使用 `sourceId`（附件 ID）作为 `pdfProcessingStore` 的 key。

**涉及文件**：
- `src/chat-v2/components/input-bar/InputBarUI.tsx` - Store 初始化和查询
- `src/chat-v2/core/store/createChatStore.ts` - 清理逻辑
- `src/hooks/usePdfProcessingProgress.ts` - 事件监听（无需修改，已正确使用 `fileId`）

#### 2026-02-02: 复用附件不返回处理状态（P0 修复）

**问题描述**：复用已有附件时，前端无法获取处理状态。

**根因**：
- `attachment_repo.rs` 复用附件时返回 `processing_status: None`
- 前端以为不需要预处理，实际上可能还未完成

**修复方案**：添加 `get_processing_status_with_conn` 方法，查询并返回已有的处理状态。

**涉及文件**：
- `src-tauri/src/vfs/repos/attachment_repo.rs`

#### 2026-02-02: 事件通道竞态条件（P0 修复）

**问题描述**：消息发送后前端不显示，只有刷新才能看到。

**根因**：
- `stream_start` 通过 `chat_v2_session_{id}` 通道发送
- `thinking/start` 通过 `chat_v2_event_{id}` 通道发送
- 两个通道独立，可能产生竞态：`thinking/start` 先于 `stream_start` 到达
- 此时消息不存在，块被创建但不会添加到消息的 `blockIds`

**修复方案**：在 `createBlockInternal` 中，当消息不存在时自动创建占位消息。

**涉及文件**：
- `src/chat-v2/core/store/createChatStore.ts`

#### 2026-02-02: 图片重试阶段错误（P1 修复）

**问题描述**：图片处理失败后重试，从 OCR 阶段开始而非 ImageCompression。

**根因**：`retry()` 方法硬编码了 `ProcessingStage::OcrProcessing`。

**修复方案**：根据媒体类型选择正确的重试起始阶段。

**涉及文件**：
- `src-tauri/src/vfs/pdf_processing_service.rs`

### J. 图片压缩架构说明

#### 两套独立的压缩机制

系统存在**两套独立的图片压缩机制**，服务于不同目的：

| 机制 | 触发时机 | 条件 | 目的 | 结果存储 |
|------|----------|------|------|----------|
| **预处理压缩** | 上传后异步 | `size > 1MB` | 存储优化 | `compressed_blob_hash` |
| **发送时压缩** | LLM 调用前 | `vision_quality` | API 优化 | 不存储，临时使用 |

#### 预处理压缩

位置：`pdf_processing_service.rs::stage_image_compression`

```rust
// 仅对大于 1MB 的图片执行
if file_size > compression_config.size_threshold {
    // 压缩并存储到 compressed_blob_hash
}
```

#### 发送时压缩

位置：`model2_pipeline.rs` / `file_manager.rs::adjust_image_quality_base64`

```rust
// 每次发送时根据 vision_quality 策略执行
let adjusted = file_manager.adjust_image_quality_base64(image_base64, vision_quality);
```

**策略**：
- `low`: 768px, JPEG 60%（适用于 6+ 张图或 PDF）
- `medium`: 1024px, JPEG 75%（适用于 2-5 张图）
- `high`: 不压缩（适用于单图或 OCR 场景）
- `auto`: 根据图片数量自动选择

#### 为什么发送时压缩不是 Bug

1. **预处理压缩存储结果**，但发送时的 `vision_quality` 策略可能不同
2. **不同 LLM 模型对图片大小有不同限制**，发送时动态压缩更灵活
3. **存储多个压缩版本会增加磁盘占用**，按需压缩更经济

#### 优化建议（P2）

如果发送时压缩（30秒 / 45张图）影响用户体验，可考虑：
1. 预处理阶段生成多种质量版本（low, medium）并缓存
2. 发送时直接使用缓存版本
3. 需要修改数据库 schema 和 blob 存储逻辑

### K. 边缘情况分析

#### 已处理的边缘情况

| 场景 | 当前行为 | 说明 |
|------|----------|------|
| 新上传 | 启动 Pipeline | 正常流程 |
| 复用已完成 | 不启动，返回状态 | ✅ 正确 |
| 复用未完成 | 继续处理 | ✅ 正确 |
| 复用出错 | 不自动重试 | 需用户手动点击重试 |
| 预处理中发送 | 禁用发送按钮 | 除非切换到已就绪模式 |
| 多文件同时上传 | 独立处理 | OCR 并发限制 4 |
| 压缩效果不明显 | 使用原图回退 | `compressed_blob_hash` 可等于原图 hash，MIME 保持原格式 |
| 压缩 blob 缺失 | 回退原图 | 优先压缩版本，读取失败自动回退 |
| 删除/清理缓存 | 同步清理压缩 blob | 避免压缩页/压缩图泄漏或误删原图 |

#### 潜在问题

| 场景 | 问题 | 建议 |
|------|------|------|
| 多大 PDF 同时上传 | OCR 队列拥堵 | 添加全局 Pipeline 并发限制 |
| 会话切换 | Store 状态残留 | 按 fileId 隔离，影响小 |
| 长时间处理 | 无超时机制 | 添加超时和重试逻辑 |

### L. 参考资料

- [VFS 架构设计文档](./vfs-architecture.md)
- [PDF OCR 服务文档](./pdf-ocr-service.md)
- [向量索引系统文档](./vector-indexing.md)
- 参考实现：`src/hooks/useExamSheetProgress.ts`（题目集处理进度）
- 参考实现：`LearningHubSidebar.tsx:608-634`（教材导入进度）
- 参考实现：`IndexStatusView.tsx`（索引状态 UI）

---

*最后更新：2026-02-02*
