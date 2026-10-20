//! 题目导入服务 — Visual-First 统一管线
//!
//! 以 VLM 视觉分析为核心的导入架构：
//! 1. Stage 1 (PageRasterizer): 所有文档 → 高清页面图片
//! 2. Stage 2 (VlmAnalyzer): VLM 逐页分析 → 题目文本 + 配图 bbox
//! 3. Stage 3 (CrossPageMerger): 跨页题目检测与合并
//! 4. Stage 4 (FigureExtractor): 按 bbox 裁切配图 → VFS 存储 → 精确关联
//! 5. Stage 5 (LlmStructurer): VLM raw_text → 标准题目 JSON
//! 6. Stage 6 (Persistence): SAVEPOINT 事务写入
//!
//! 另保留独立的 CSV 导入功能（CsvImportService）。

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Read};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, LazyLock,
};
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

/// VLM 逐页分析的并发页数（视觉模型单页常需数十秒，串行导入多页试卷过慢）
const VLM_PAGE_CONCURRENCY: usize = 3;

// The page fan-out alone does not bound image/model buffers across concurrent imports.
static IMPORT_MEMORY_BUDGET: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(1));

/// 匹配 CSV 选项串中的 "A. 内容" 结构（每行多次调用，预编译）
static CSV_OPTION_ITEM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z])[\.、\s]\s*(.+)$").unwrap());

use crate::cross_page_merger;
use crate::document_parser::DocumentParser;
use crate::figure_extractor;
use crate::file_manager::FileManager;
use crate::llm_manager::LLMManager;
use crate::llm_structurer::LlmStructurer;
use crate::models::{
    AppError, ExamCardPreview, ExamSheetPreviewPage, ExamSheetPreviewResult, QuestionStatus,
    SourceType,
};
use crate::page_rasterizer::{PageRasterizer, PageSlice, RasterizerResult};
use crate::vfs::database::VfsDatabase;
use crate::vfs::repos::{
    CreateQuestionParams, QuestionFilters, QuestionImage, VfsBlobRepo, VfsExamRepo, VfsQuestionRepo,
};
use crate::vfs::types::VfsCreateExamSheetParams;
use crate::vlm_grounding_service::{ImportBlockKind, VlmGroundingService, VlmPageAnalysis};

mod non_visual;
mod persistence;

use persistence::publish_candidates;

// ============================================================================
// 公共类型
// ============================================================================

/// 流式导入进度事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum QuestionImportProgress {
    /// 文档预处理进度（解码、解析、图片提取等 — session 创建之前的阶段）
    Preprocessing {
        stage: String,
        message: String,
        percent: usize,
    },
    /// 页面渲染进度
    RenderingPages { current: usize, total: usize },
    /// 单张图片 OCR/VLM 完成（兼容旧前端事件名）
    OcrImageCompleted {
        image_index: usize,
        total_images: usize,
    },
    /// OCR/VLM 阶段完成
    OcrPhaseCompleted {
        total_images: usize,
        total_chars: usize,
    },
    /// 导入会话已创建
    SessionCreated {
        session_id: String,
        name: String,
        total_chunks: usize,
    },
    /// 配图提取进度
    ExtractingFigures { current: usize, total: usize },
    /// 题目结构化进度
    StructuringQuestion { current: usize, total: usize },
    /// 单道题目解析完成
    QuestionParsed {
        question: Value,
        question_index: usize,
        total_parsed: usize,
    },
    /// 分块完成（兼容旧前端）
    ChunkStart {
        chunk_index: usize,
        total_chunks: usize,
    },
    ChunkCompleted {
        chunk_index: usize,
        total_chunks: usize,
        questions_in_chunk: usize,
        total_parsed: usize,
    },
    /// 导入完成
    Completed {
        session_id: String,
        name: String,
        total_questions: usize,
        /// 导入过程"可能缺题"时为 true：VLM 直提中途失败但已保存部分题，
        /// 或流式导入中存在写库失败的题目。前端据此提示用户。
        /// serde 默认 false 以兼容旧前端反序列化。
        #[serde(default)]
        partial: bool,
        /// 解析成功但写库失败的题目数（P1 修复：失败不再静默计入 total_questions）。
        /// serde 默认 0 以兼容旧前端反序列化。
        #[serde(default)]
        failed_count: usize,
        /// ★ P2 修复：未完成 LLM 结构化的题目数（本地兜底 + 原文回退）。
        /// 旧实现只在日志里写「结构化完成 N/N」，降级对用户完全不可见。
        /// serde 默认 0 以兼容旧前端反序列化。
        #[serde(default)]
        degenerate_count: usize,
    },
    /// 导入失败
    Failed {
        session_id: Option<String>,
        error: String,
        total_parsed: usize,
    },
}

/// Cancellation state for a streaming question-bank import.
///
/// A plain cancellation token is not enough here: a cancel request can race a
/// completed import while the command is still cleaning up its registration.
/// The terminal state ensures that exactly one side wins, so callers never
/// report a successful cancellation after the import has already completed.
pub struct QuestionImportCancellation {
    token: CancellationToken,
    // 0 = running, 1 = cancellation accepted, 2 = import completed
    state: AtomicU8,
}

impl Default for QuestionImportCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl QuestionImportCancellation {
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            state: AtomicU8::new(0),
        }
    }

    /// Returns true only when this request transitioned the import from
    /// running to cancelling. Completion and a prior cancellation both win
    /// the race over later requests.
    pub fn request_cancel(&self) -> bool {
        if self
            .state
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            self.token.cancel();
            true
        } else {
            false
        }
    }

    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Marks a normal import return as complete. Returns false when an
    /// accepted cancellation won first, in which case the caller must discard
    /// any racey successful result and report cancellation instead.
    pub fn finish_or_cancelled(&self) -> bool {
        self.state
            .compare_exchange(0, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// 导入请求参数（兼容现有 commands.rs 接口）
#[derive(Debug, Clone)]
pub struct ImportRequest {
    pub content: String,
    pub format: String,
    pub name: Option<String>,
    pub session_id: Option<String>,
    pub folder_id: Option<String>,
    pub model_config_id: Option<String>,
    /// 保留兼容性（Visual-First 架构下忽略此字段）
    pub pdf_prefer_ocr: Option<bool>,
}

/// 导入结果
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub session_id: String,
    pub name: String,
    pub imported_count: usize,
    pub total_questions: usize,
}

/// PDF 文本检测结果（保留向后兼容，新架构下不再需要前端调用）
#[derive(Debug, Clone, Serialize)]
pub struct PdfTextInspection {
    pub valid_char_count: usize,
    pub total_char_count: usize,
    pub preview_text: String,
    pub recommendation: String,
}

/// 管线阶段标识（用于断点续导的状态机）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ImportStage {
    #[default]
    Rasterized,
    VlmAnalyzing,
    VlmCompleted,
    FiguresExtracted,
    Structuring,
    Completed,
    // DOCX 原生路径阶段（旧 text+marker 方案）
    DocxExtracted,
    DocxVlmDescribing,
    DocxVlmDone,
    DocxStructuring,
    // DOCX VLM 直提路径阶段
    DocxVlmDirect,
    DocxVlmDirectDone,
}

/// 断点续导状态（分阶段 checkpoint 状态机）
///
/// 每完成一个 Stage 的关键进度就持久化，恢复时跳到正确阶段：
/// - `VlmAnalyzing` + `vlm_pages_completed` → 从断点页继续 VLM
/// - `VlmCompleted` → 跳过 VLM，从 Stage 3 开始
/// - `Structuring` + `structuring_batches_completed` → 从断点批次继续 LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportCheckpointState {
    pub qbank_name: String,
    pub model_config_id: Option<String>,
    pub source_format: String,
    #[serde(default)]
    pub current_stage: ImportStage,
    /// 页面图片的 VFS blob hash 列表
    pub page_blob_hashes: Vec<String>,

    // Stage 2: VLM 分析
    #[serde(default)]
    pub vlm_pages_completed: usize,
    #[serde(default)]
    pub vlm_page_results: Vec<String>,

    // Stage 5: LLM 结构化
    #[serde(default)]
    pub structuring_batches_completed: usize,
    /// 已完成批次的 LLM 结构化结果（每个元素为一批题目的 JSON 数组字符串）
    #[serde(default)]
    pub structured_batch_results: Vec<String>,

    // Stage 4: 页面尺寸（恢复时避免解码图片）
    #[serde(default)]
    pub page_dimensions: Vec<(u32, u32)>,

    // 向后兼容旧 checkpoint
    #[serde(default)]
    pub source_image_hashes: Vec<String>,
    #[serde(default)]
    pub import_mode: String,
    #[serde(default)]
    pub text_content: String,
    #[serde(default)]
    pub chunks_total: usize,
    #[serde(default)]
    pub chunks_completed: usize,

    // DOCX 原生路径专用（旧 text+marker）
    #[serde(default)]
    pub docx_image_entries: Vec<DocxImageEntrySerde>,
    #[serde(default)]
    pub docx_vlm_descriptions: HashMap<String, String>,
    #[serde(default)]
    pub docx_vlm_images_completed: usize,
    #[serde(default)]
    pub docx_enriched_text: String,
    #[serde(default)]
    pub docx_marker_positions: Vec<(usize, usize)>,

    // DOCX VLM 直提路径专用
    #[serde(default)]
    pub docx_vlm_direct_blob_hashes: Vec<String>,
    #[serde(default)]
    pub docx_vlm_direct_mimes: Vec<String>,
    #[serde(default)]
    pub docx_vlm_direct_exts: Vec<String>,
    #[serde(default)]
    pub docx_vlm_direct_text: String,
    #[serde(default)]
    pub docx_vlm_direct_saved: usize,
    #[serde(default)]
    pub source_document_id: String,
    #[serde(default)]
    pub candidate_results: Vec<String>,
    #[serde(default)]
    pub failed_chunks: Vec<usize>,
    #[serde(default)]
    pub failed_pages: Vec<usize>,
}

// ============================================================================
// 导入服务
// ============================================================================

pub struct QuestionImportService {
    llm_manager: Arc<LLMManager>,
    file_manager: Option<Arc<FileManager>>,
}

impl QuestionImportService {
    pub fn new(llm_manager: Arc<LLMManager>, file_manager: Arc<FileManager>) -> Self {
        Self {
            llm_manager,
            file_manager: Some(file_manager),
        }
    }

    pub fn new_without_file_manager(llm_manager: Arc<LLMManager>) -> Self {
        Self {
            llm_manager,
            file_manager: None,
        }
    }

    /// 保留向后兼容
    pub fn inspect_pdf_text(&self, base64_content: &str) -> Result<PdfTextInspection, AppError> {
        let parser = DocumentParser::new();
        let extracted = parser
            .extract_text_from_base64("document.pdf", base64_content)
            .map_err(|e| AppError::validation(format!("文档解析失败: {}", e)))?;

        let normalized = extracted.trim();
        let valid_char_count = count_valid_chars(normalized);
        let total_char_count = normalized.chars().count();
        let recommendation = if valid_char_count < 100 {
            "auto_ocr"
        } else if valid_char_count < 500 {
            "manual_decision"
        } else {
            "use_text"
        }
        .to_string();

        Ok(PdfTextInspection {
            valid_char_count,
            total_char_count,
            preview_text: normalized.chars().take(800).collect(),
            recommendation,
        })
    }

    /// 非流式导入入口
    pub async fn import_document(
        &self,
        vfs_db: &Arc<VfsDatabase>,
        request: ImportRequest,
    ) -> Result<ImportResult, AppError> {
        self.import_document_stream(vfs_db, request, None).await
    }

    /// Visual-First 统一导入管线（流式）
    pub async fn import_document_stream(
        &self,
        vfs_db: &Arc<VfsDatabase>,
        request: ImportRequest,
        progress_tx: Option<UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        let _memory_permit = IMPORT_MEMORY_BUDGET
            .acquire()
            .await
            .map_err(|_| AppError::internal("导入资源队列已关闭"))?;
        let format = request.format.as_str();

        // JSON 直接导入（无需 VLM）
        if format == "json" {
            return self.import_json_directly(vfs_db, &request).await;
        }

        // 纯文本 & 表格格式：走 LLM 结构化（无图片，VLM 无意义）
        if matches!(format, "txt" | "md" | "markdown" | "csv" | "xlsx" | "xls") {
            return self
                .import_text_via_llm(vfs_db, &request, progress_tx.as_ref())
                .await;
        }

        // ====== DOCX：原生提取 + VLM 直提（无需 LibreOffice） ======
        if format == "docx" {
            // 快速检测是否含嵌入图片
            let has_images = match decode_base64_content(&request.content) {
                Ok(ref docx_bytes) => {
                    let parser = DocumentParser::new();
                    match parser.extract_docx_with_images(docx_bytes) {
                        Ok((_, images)) => !images.is_empty(),
                        Err(_) => false,
                    }
                }
                Err(_) => false,
            };

            if has_images {
                // 有图片 DOCX → VLM 直接提取（VLM 看到真实图片，自行判断图文关联）
                log::info!("[QuestionImport] DOCX 含嵌入图片，走 VLM 直提路径");
                return self
                    .import_docx_via_vlm(vfs_db, &request, progress_tx.as_ref())
                    .await;
            } else {
                // 纯文本 DOCX → LLM 结构化
                log::info!("[QuestionImport] DOCX 无嵌入图片，走纯文本路径");
                return self
                    .import_text_via_llm(vfs_db, &request, progress_tx.as_ref())
                    .await;
            }
        }

        // ====== Visual-First 管线：PDF / Image ======

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "rasterizing".to_string(),
                message: "正在渲染文档页面...".to_string(),
                percent: 2,
            });
        }

        // Stage 1: 渲染为页面图片
        let pages = match self
            .stage1_rasterize(vfs_db, &request, progress_tx.as_ref())
            .await
        {
            Ok(p) if !p.is_empty() => p,
            Ok(_) => {
                return Err(AppError::validation("文档渲染后没有生成任何页面"));
            }
            Err(e) => return Err(e),
        };

        let qbank_name = request
            .name
            .clone()
            .unwrap_or_else(|| "导入的题目集".to_string());

        let session_id = self.create_import_session(vfs_db, &request, &qbank_name, &pages)?;

        // 初始 checkpoint（Stage 1 完成）
        let mut checkpoint = ImportCheckpointState {
            qbank_name: qbank_name.clone(),
            model_config_id: request.model_config_id.clone(),
            source_format: request.format.clone(),
            current_stage: ImportStage::Rasterized,
            page_blob_hashes: pages.iter().map(|p| p.blob_hash.clone()).collect(),
            vlm_pages_completed: 0,
            vlm_page_results: Vec::new(),
            structuring_batches_completed: 0,
            structured_batch_results: Vec::new(),
            page_dimensions: pages.iter().map(|p| (p.width, p.height)).collect(),
            source_image_hashes: Vec::new(),
            import_mode: String::new(),
            text_content: String::new(),
            chunks_total: 0,
            chunks_completed: 0,
            docx_image_entries: Vec::new(),
            docx_vlm_descriptions: HashMap::new(),
            docx_vlm_images_completed: 0,
            docx_enriched_text: String::new(),
            docx_marker_positions: Vec::new(),
            docx_vlm_direct_blob_hashes: Vec::new(),
            docx_vlm_direct_mimes: Vec::new(),
            docx_vlm_direct_exts: Vec::new(),
            docx_vlm_direct_text: String::new(),
            docx_vlm_direct_saved: 0,
            source_document_id: format!("{:x}", Sha256::digest(request.content.as_bytes())),
            candidate_results: Vec::new(),
            failed_chunks: Vec::new(),
            failed_pages: Vec::new(),
        };
        persist_checkpoint(vfs_db, &session_id, &checkpoint)?;

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::SessionCreated {
                session_id: session_id.clone(),
                name: qbank_name.clone(),
                total_chunks: pages.len(),
            });
        }

        // Stage 2-6: 共享流程
        self.run_visual_pipeline_from_stage2(
            vfs_db,
            &session_id,
            &qbank_name,
            &pages,
            &mut checkpoint,
            0, // vlm 从 0 开始
            progress_tx.as_ref(),
        )
        .await
    }

    /// 恢复中断的导入（分阶段 checkpoint 状态机）
    pub async fn resume_import(
        &self,
        vfs_db: &VfsDatabase,
        session_id: &str,
        progress_tx: Option<UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        let _memory_permit = IMPORT_MEMORY_BUDGET
            .acquire()
            .await
            .map_err(|_| AppError::internal("导入资源队列已关闭"))?;
        let state_str = VfsExamRepo::get_import_state(vfs_db, session_id)
            .map_err(|e| AppError::database(format!("读取导入状态失败: {}", e)))?
            .ok_or_else(|| AppError::not_found("未找到可恢复的导入状态"))?;

        let mut checkpoint: ImportCheckpointState = serde_json::from_str(&state_str)
            .map_err(|e| AppError::validation(format!("解析导入状态失败: {}", e)))?;

        if checkpoint.source_document_id.is_empty() {
            let existing = VfsQuestionRepo::list_questions(
                vfs_db,
                session_id,
                &QuestionFilters::default(),
                1,
                1,
            )
            .map_err(|e| AppError::database(format!("检查旧导入状态失败: {}", e)))?;
            if existing.total > 0 {
                return Err(AppError::validation("此旧版导入已产生无稳定来源的题目，无法安全自动续导；请保留原题目集并重新导入到新题目集核对，避免重复或覆盖作答"));
            }
        }

        log::info!(
            "[QuestionImport] 恢复导入: session={}, stage={:?}, vlm={}/{}, struct_batches={}",
            session_id,
            checkpoint.current_stage,
            checkpoint.vlm_pages_completed,
            checkpoint.page_blob_hashes.len(),
            checkpoint.structuring_batches_completed,
        );

        // DOCX 原生路径：通过 stage 前缀 Docx* 检测
        let is_docx_native = matches!(
            checkpoint.current_stage,
            ImportStage::DocxExtracted
                | ImportStage::DocxVlmDescribing
                | ImportStage::DocxVlmDone
                | ImportStage::DocxStructuring
        );

        // DOCX VLM 直提路径恢复
        let is_docx_vlm_direct = matches!(
            checkpoint.current_stage,
            ImportStage::DocxVlmDirect | ImportStage::DocxVlmDirectDone
        );

        if is_docx_vlm_direct {
            let blob_hashes = checkpoint.docx_vlm_direct_blob_hashes.clone();
            let mime_list = checkpoint.docx_vlm_direct_mimes.clone();
            let ext_list = checkpoint.docx_vlm_direct_exts.clone();
            let full_text = checkpoint.docx_vlm_direct_text.clone();
            let qbank_name = checkpoint.qbank_name.clone();
            if checkpoint.source_document_id.is_empty() {
                checkpoint.source_document_id =
                    format!("{:x}", Sha256::digest(full_text.as_bytes()));
            }
            return self
                .run_docx_import_candidates(
                    vfs_db,
                    session_id,
                    &qbank_name,
                    &full_text,
                    &blob_hashes,
                    &mime_list,
                    &ext_list,
                    &mut checkpoint,
                    progress_tx.as_ref(),
                )
                .await;
        }

        if is_docx_native {
            let image_map: Vec<DocxImageEntry> = checkpoint
                .docx_image_entries
                .iter()
                .map(DocxImageEntry::from)
                .collect();
            let full_text = if checkpoint.docx_enriched_text.is_empty() {
                checkpoint.text_content.clone()
            } else {
                checkpoint.docx_enriched_text.clone()
            };
            let hashes: Vec<String> = image_map.iter().map(|i| i.blob_hash.clone()).collect();
            let mimes: Vec<String> = image_map.iter().map(|i| i.mime.clone()).collect();
            let exts: Vec<String> = image_map.iter().map(|i| i.ext.clone()).collect();
            let qbank_name = checkpoint.qbank_name.clone();
            return self
                .run_docx_import_candidates(
                    vfs_db,
                    session_id,
                    &qbank_name,
                    &full_text,
                    &hashes,
                    &mimes,
                    &exts,
                    &mut checkpoint,
                    progress_tx.as_ref(),
                )
                .await;
        }

        if checkpoint.import_mode == "text" && !checkpoint.text_content.is_empty() {
            let text_content = checkpoint.text_content.clone();
            let qbank_name = checkpoint.qbank_name.clone();
            if checkpoint.source_document_id.is_empty() {
                checkpoint.source_document_id =
                    format!("{:x}", Sha256::digest(text_content.as_bytes()));
            }
            let chunks = non_visual::text_chunks(&text_content);
            return self
                .run_text_import_chunks(
                    vfs_db,
                    session_id,
                    &qbank_name,
                    &chunks,
                    &mut checkpoint,
                    progress_tx.as_ref(),
                )
                .await;
        }

        // Visual-First 管线恢复
        let pages = rebuild_pages_from_checkpoint(&checkpoint, vfs_db)?;
        let vlm_start = checkpoint.vlm_pages_completed;
        let qbank_name = checkpoint.qbank_name.clone();

        self.run_visual_pipeline_from_stage2(
            vfs_db,
            session_id,
            &qbank_name,
            &pages,
            &mut checkpoint,
            vlm_start,
            progress_tx.as_ref(),
        )
        .await
    }

    /// Stage 2 ~ Stage 6 共享流程（首次导入和恢复复用同一逻辑）
    async fn run_visual_pipeline_from_stage2(
        &self,
        vfs_db: &VfsDatabase,
        session_id: &str,
        qbank_name: &str,
        pages: &[PageSlice],
        checkpoint: &mut ImportCheckpointState,
        _vlm_start_page: usize,
        progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        checkpoint.current_stage = ImportStage::VlmAnalyzing;
        if checkpoint.source_document_id.is_empty() {
            checkpoint.source_document_id = format!(
                "{:x}",
                Sha256::digest(checkpoint.page_blob_hashes.join(":").as_bytes())
            );
        }
        let vlm_service = VlmGroundingService::new(self.llm_manager.clone());
        let mut page_analyses: Vec<Option<VlmPageAnalysis>> = pages
            .iter()
            .enumerate()
            .map(|(idx, _)| {
                checkpoint
                    .vlm_page_results
                    .get(idx)
                    .and_then(|s| serde_json::from_str(s).ok())
            })
            .collect();
        let pending: Vec<usize> = page_analyses
            .iter()
            .enumerate()
            .filter_map(|(idx, analysis)| analysis.is_none().then_some(idx))
            .collect();
        if !pending.is_empty() {
            checkpoint.structuring_batches_completed = 0;
            checkpoint.structured_batch_results.clear();
        }
        checkpoint.failed_pages.clear();
        persist_checkpoint(vfs_db, session_id, checkpoint)?;
        use futures::StreamExt;
        let vlm_service = &vlm_service;
        let mut first_failure: Option<String> = None;
        let mut page_results = futures::stream::iter(pending)
            .map(|idx| async move {
                (
                    idx,
                    vlm_service.analyze_page_by_blob(vfs_db, &pages[idx]).await,
                )
            })
            .buffered(VLM_PAGE_CONCURRENCY);
        while let Some((idx, page_result)) = page_results.next().await {
            match page_result {
                Ok(mut analysis) => {
                    for (block_idx, block) in analysis.questions.iter_mut().enumerate() {
                        block.source_id = format!("page:{}:block:{}", idx, block_idx);
                    }
                    log::info!(
                        "[QuestionImport] 页面 {}/{}: {} 个文档块",
                        idx + 1,
                        pages.len(),
                        analysis.questions.len()
                    );
                    while checkpoint.vlm_page_results.len() <= idx {
                        checkpoint.vlm_page_results.push(String::new());
                    }
                    checkpoint.vlm_page_results[idx] = serde_json::to_string(&analysis)
                        .map_err(|e| AppError::validation(format!("保存页面结果失败: {}", e)))?;
                    page_analyses[idx] = Some(analysis);
                }
                Err(e) => {
                    log::warn!("[QuestionImport] 页面 {} 失败: {}", idx + 1, e);
                    first_failure.get_or_insert_with(|| e.to_string());
                    checkpoint.failed_pages.push(idx);
                }
            }
            checkpoint.vlm_pages_completed =
                page_analyses.iter().take_while(|p| p.is_some()).count();
            persist_checkpoint(vfs_db, session_id, checkpoint)?;
            if let Some(tx) = progress_tx {
                let _ = tx.send(QuestionImportProgress::OcrImageCompleted {
                    image_index: idx,
                    total_images: pages.len(),
                });
            }
        }
        checkpoint.current_stage = ImportStage::VlmCompleted;
        persist_checkpoint(vfs_db, session_id, checkpoint)?;
        let total_ocr_chars = page_analyses
            .iter()
            .flatten()
            .flat_map(|a| &a.questions)
            .map(|q| q.raw_text.len())
            .sum();
        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::OcrPhaseCompleted {
                total_images: pages.len(),
                total_chars: total_ocr_chars,
            });
        }

        // ===== Stage 3: 跨页合并 =====
        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "merging".to_string(),
                message: "正在跨页合并题目...".to_string(),
                percent: 42,
            });
        }
        let all_blocks = cross_page_merger::merge_pages(&page_analyses);
        let no_blocks = all_blocks.is_empty();
        let (merged, deferred) = cross_page_merger::partition_complete_blocks(all_blocks);
        let deferred_continuations: Vec<Value> = deferred
            .iter()
            .map(|block| {
                json!({
                    "source_id": block.question.source_id,
                    "page_spans": block.source_spans,
                    "continues_from_previous": block.question.continues_from_previous,
                    "continues_to_next": block.question.continues_to_next,
                    "reason": "unresolved_continuation",
                })
            })
            .collect();
        if !deferred_continuations.is_empty() {
            log::warn!(
                "[QuestionImport] {} 个不完整跨页块暂不发布，等待来源补齐",
                deferred_continuations.len()
            );
        }

        if no_blocks {
            let error_msg = no_questions_error("未能识别出题目", first_failure.as_deref());
            if let Some(tx) = progress_tx {
                let _ = tx.send(QuestionImportProgress::Failed {
                    session_id: Some(session_id.to_string()),
                    error: error_msg.clone(),
                    total_parsed: 0,
                });
            }
            let report = json!({"document_id": checkpoint.source_document_id, "failed_pages": checkpoint.failed_pages,
                "needs_review": true, "resume_available": true, "error": error_msg});
            self.publish_import_candidates(
                vfs_db,
                session_id,
                qbank_name,
                &[],
                0,
                pages,
                report,
                None,
            )?;
            return Err(AppError::validation(error_msg));
        }

        // ===== Stage 4: 配图裁切与关联 =====
        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::ExtractingFigures {
                current: 0,
                total: merged.len(),
            });
        }
        let figure_outcome = figure_extractor::extract_figures(merged, pages, vfs_db);
        let questions_with_figures = figure_outcome.questions;
        let figure_failures = figure_outcome.failures;

        checkpoint.current_stage = ImportStage::FiguresExtracted;
        persist_checkpoint(vfs_db, session_id, checkpoint)?;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::ExtractingFigures {
                current: questions_with_figures.len(),
                total: questions_with_figures.len(),
            });
        }

        // ===== Stage 5: LLM 结构化（带逐批 checkpoint） =====
        checkpoint.current_stage = ImportStage::Structuring;
        persist_checkpoint(vfs_db, session_id, checkpoint)?;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::StructuringQuestion {
                current: checkpoint.structuring_batches_completed,
                total: questions_with_figures.len(),
            });
        }

        let structurer = LlmStructurer::new(self.llm_manager.clone());
        let model_config_id = checkpoint.model_config_id.clone();
        let prior_results = checkpoint.structured_batch_results.clone();
        let mut checkpoint_error = None;
        let outcome = structurer
            .structure_questions_with_checkpoint(
                &questions_with_figures,
                model_config_id.as_deref(),
                checkpoint.structuring_batches_completed,
                &prior_results,
                |completed, results| {
                    checkpoint.structuring_batches_completed = completed;
                    checkpoint.structured_batch_results = results.to_vec();
                    if let Err(e) = persist_checkpoint(vfs_db, session_id, checkpoint) {
                        checkpoint_error = Some(e);
                    }
                },
            )
            .await?;
        if let Some(e) = checkpoint_error {
            return Err(e);
        }

        checkpoint.structuring_batches_completed = outcome.batches_completed;
        checkpoint.structured_batch_results = outcome.batch_results.clone();
        persist_checkpoint(vfs_db, session_id, checkpoint)?;

        if outcome.is_degraded() {
            log::warn!(
                "[QuestionImport] 结构化降级: 共 {} 题, LLM {} 题, 本地拆选项 {} 题, 原文回退 {} 题",
                outcome.total,
                outcome.llm_structured,
                outcome.local_split,
                outcome.raw_fallback
            );
        }

        let coverage: Vec<Value> = page_analyses
            .iter()
            .enumerate()
            .map(|(idx, analysis)| {
                json!({"page_index": idx, "state": match analysis {
                None => "failed",
                Some(a) if a.questions.is_empty() => "needs_review",
                Some(_) => "succeeded",
            }, "blocks": analysis.as_ref().map(|a| a.questions.len()).unwrap_or(0)})
            })
            .collect();
        let empty_pages: Vec<usize> = page_analyses
            .iter()
            .enumerate()
            .filter_map(|(idx, a)| {
                a.as_ref()
                    .is_some_and(|a| a.questions.is_empty())
                    .then_some(idx)
            })
            .collect();
        let publishable_questions: Vec<_> = outcome
            .questions
            .iter()
            .filter(|question| {
                !figure_failures
                    .iter()
                    .any(|failure| failure.source_id == question.source.merged.question.source_id)
            })
            .cloned()
            .collect();
        let report = json!({
            "document_id": checkpoint.source_document_id,
            "source_kind": "visual",
            "failed_pages": checkpoint.failed_pages,
            "page_coverage": coverage,
            "unconfirmed_empty_pages": empty_pages,
            "deferred_continuations": deferred_continuations,
            "figure_failures": figure_failures,
            "needs_review": !empty_pages.is_empty() || !deferred_continuations.is_empty() || !figure_failures.is_empty() || outcome.is_degraded(),
            "incomplete_batches": outcome.incomplete_batches,
            "resume_available": true,
        });
        self.publish_import_candidates(
            vfs_db,
            session_id,
            qbank_name,
            &publishable_questions,
            outcome.degenerate(),
            pages,
            report,
            progress_tx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn publish_import_candidates(
        &self,
        vfs_db: &VfsDatabase,
        session_id: &str,
        qbank_name: &str,
        candidates: &[crate::llm_structurer::StructuredQuestion],
        degenerate: usize,
        pages: &[PageSlice],
        report: Value,
        progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        publish_candidates(
            vfs_db,
            session_id,
            qbank_name,
            candidates,
            degenerate,
            pages,
            report,
            progress_tx,
        )
    }

    /// 对结构化结果做答案区合并与同题干去重（纯函数，便于单测）
    fn merge_answer_sections(
        source: &[crate::llm_structurer::StructuredQuestion],
    ) -> (
        Vec<crate::llm_structurer::StructuredQuestion>,
        crate::answer_merge::MergeOutcome,
    ) {
        use crate::answer_merge::{merge_answer_sections, MergeItem};

        let mut items: Vec<MergeItem> = source
            .iter()
            .map(|sq| {
                let block = &sq.source.merged.question;
                MergeItem::new(block.label.clone(), sq.json.clone())
                    .with_source(
                        block.block_kind,
                        block.section.clone(),
                        block.source_id.clone(),
                    )
                    .with_raw_text(block.raw_text.clone())
            })
            .collect();

        let outcome = merge_answer_sections(&mut items);

        let merged = source
            .iter()
            .zip(items.into_iter())
            .filter(|(_, item)| !item.removed)
            .map(|(sq, item)| crate::llm_structurer::StructuredQuestion {
                json: item.json,
                source: sq.source.clone(),
                origin: sq.origin,
            })
            .collect();

        (merged, outcome)
    }

    // ====== 内部方法 ======

    /// Stage 1: 渲染文档为页面图片
    ///
    /// DOCX 转换失败时返回 `Err(ImportResult)` 表示已通过文本模式完成导入。
    async fn stage1_rasterize(
        &self,
        vfs_db: &Arc<VfsDatabase>,
        request: &ImportRequest,
        progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    ) -> Result<Vec<PageSlice>, AppError> {
        let format = request.format.as_str();

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::RenderingPages {
                current: 0,
                total: 1,
            });
        }

        // CPU 密集渲染必须在 spawn_blocking 中执行，避免阻塞 Tokio worker。
        let content = request.content.clone();
        let format_owned = format.to_string();
        let vfs_db_arc = vfs_db.clone();

        let result: RasterizerResult =
            tokio::task::spawn_blocking(move || match format_owned.as_str() {
                "pdf" => PageRasterizer::rasterize_pdf(&content, &vfs_db_arc),
                "docx" => PageRasterizer::rasterize_docx(&content, &vfs_db_arc),
                fmt if is_image_format(fmt) => {
                    PageRasterizer::rasterize_images(&content, &vfs_db_arc)
                }
                _ => Err(AppError::validation(format!(
                    "不支持的导入格式: {}",
                    format_owned
                ))),
            })
            .await
            .map_err(|e| AppError::internal(format!("渲染任务调度失败: {}", e)))??;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::RenderingPages {
                current: result.pages.len(),
                total: result.pages.len(),
            });
        }

        Ok(result.pages)
    }

    /// DOCX 保留图片锚点，按有界批次暂存后统一关联答案与发布。
    async fn import_docx_via_vlm(
        &self,
        vfs_db: &VfsDatabase,
        request: &ImportRequest,
        progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        // ===== Stage 1: 提取文本 + 嵌入图片 =====
        log::info!(
            "[QuestionImport] DOCX VLM 直提路径开始: content 长度={} 字符",
            request.content.len()
        );

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "decoding".to_string(),
                message: "正在解码文档...".to_string(),
                percent: 2,
            });
        }

        let docx_bytes = decode_base64_content(&request.content)?;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "extracting".to_string(),
                message: "正在提取文本和图片...".to_string(),
                percent: 5,
            });
        }

        let parser = DocumentParser::new();
        let (extracted_text, image_bytes_list) = parser
            .extract_docx_with_images(&docx_bytes)
            .map_err(|e| AppError::validation(format!("DOCX 解析失败: {}", e)))?;

        let total_images = image_bytes_list.len();
        log::info!(
            "[QuestionImport] DOCX 提取完成: {} 字符, {} 张图片",
            extracted_text.len(),
            total_images
        );

        // ===== 图片存 VFS blob =====
        let mut blob_hashes: Vec<String> = Vec::with_capacity(total_images);
        let mut mime_list: Vec<String> = Vec::with_capacity(total_images);
        let mut ext_list: Vec<String> = Vec::with_capacity(total_images);

        for (idx, img_bytes) in image_bytes_list.iter().enumerate() {
            if img_bytes.is_empty() {
                return Err(AppError::validation(format!(
                    "DOCX 图片 {} 内容为空，无法保证图片索引完整",
                    idx
                )));
            }

            if let Some(tx) = progress_tx {
                let _ = tx.send(QuestionImportProgress::Preprocessing {
                    stage: "storing_images".to_string(),
                    message: format!("正在存储图片 {}/{}...", idx + 1, total_images),
                    percent: 5 + (idx * 10) / total_images.max(1),
                });
            }

            let (mime, ext) = detect_image_format(img_bytes);
            let blob = VfsBlobRepo::store_blob(vfs_db, img_bytes, Some(mime), Some(ext))
                .map_err(|e| AppError::database(format!("图片 {} Blob 存储失败: {}", idx, e)))?;

            blob_hashes.push(blob.hash);
            mime_list.push(mime.to_string());
            ext_list.push(ext.to_string());
        }

        let clean_text = extracted_text;

        // ===== 创建会话 =====
        let qbank_name = request
            .name
            .clone()
            .unwrap_or_else(|| "导入的题目集".to_string());

        let session_id = if let Some(sid) = &request.session_id {
            sid.clone()
        } else {
            let temp_id = uuid::Uuid::new_v4().to_string();
            let preview = ExamSheetPreviewResult {
                temp_id: temp_id.clone(),
                exam_name: Some(qbank_name.clone()),
                pages: vec![ExamSheetPreviewPage {
                    page_index: 0,
                    cards: Vec::new(),
                    blob_hash: None,
                    width: None,
                    height: None,
                    original_image_path: String::new(),
                    raw_ocr_text: None,
                    ocr_completed: false,
                    parse_completed: false,
                }],
                raw_model_response: None,
                instructions: None,
                session_id: Some(temp_id.clone()),
            };
            let preview_json = serde_json::to_value(&preview)
                .map_err(|e| AppError::validation(format!("序列化失败: {}", e)))?;

            let params = VfsCreateExamSheetParams {
                exam_name: Some(qbank_name.clone()),
                temp_id,
                metadata_json: json!({}),
                preview_json,
                status: "importing".to_string(),
                folder_id: request.folder_id.clone(),
            };
            VfsExamRepo::create_exam_sheet(vfs_db, params)
                .map_err(|e| AppError::database(format!("创建题目集失败: {}", e)))?
                .id
        };

        // ===== 保存 checkpoint: DocxVlmDirect（图片已提取，准备 VLM 调用） =====
        let mut checkpoint = ImportCheckpointState {
            qbank_name: qbank_name.clone(),
            model_config_id: request.model_config_id.clone(),
            source_format: "docx".to_string(),
            current_stage: ImportStage::DocxVlmDirect,
            page_blob_hashes: Vec::new(),
            vlm_pages_completed: 0,
            vlm_page_results: Vec::new(),
            structuring_batches_completed: 0,
            structured_batch_results: Vec::new(),
            page_dimensions: Vec::new(),
            source_image_hashes: Vec::new(),
            import_mode: String::new(),
            text_content: String::new(),
            chunks_total: 0,
            chunks_completed: 0,
            docx_image_entries: Vec::new(),
            docx_vlm_descriptions: HashMap::new(),
            docx_vlm_images_completed: 0,
            docx_enriched_text: String::new(),
            docx_marker_positions: Vec::new(),
            docx_vlm_direct_blob_hashes: blob_hashes.clone(),
            docx_vlm_direct_mimes: mime_list.clone(),
            docx_vlm_direct_exts: ext_list.clone(),
            docx_vlm_direct_text: clean_text.clone(),
            docx_vlm_direct_saved: 0,
            source_document_id: format!("{:x}", Sha256::digest(request.content.as_bytes())),
            candidate_results: Vec::new(),
            failed_chunks: Vec::new(),
            failed_pages: Vec::new(),
        };
        persist_checkpoint(vfs_db, &session_id, &checkpoint)?;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::SessionCreated {
                session_id: session_id.clone(),
                name: qbank_name.clone(),
                total_chunks: 1,
            });
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "vlm_extracting".to_string(),
                message: format!(
                    "正在用 VLM 分析 {} 张图片和文本，提取题目...",
                    blob_hashes.len()
                ),
                percent: 20,
            });
        }

        self.run_docx_import_candidates(
            vfs_db,
            &session_id,
            &qbank_name,
            &clean_text,
            &blob_hashes,
            &mime_list,
            &ext_list,
            &mut checkpoint,
            progress_tx,
        )
        .await
    }

    /// 纯文本格式的 LLM 直接解析路径
    ///
    /// 支持 txt/md/csv/xlsx/xls（纯文本解码）以及 DOCX 回退（二进制文档解析）。
    async fn import_text_via_llm(
        &self,
        vfs_db: &VfsDatabase,
        request: &ImportRequest,
        progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    ) -> Result<ImportResult, AppError> {
        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "decoding".to_string(),
                message: "正在解析文档内容...".to_string(),
                percent: 5,
            });
        }

        let text_content = if matches!(request.format.as_str(), "docx" | "xlsx" | "xls") {
            let parser = DocumentParser::new();
            let file_name = format!("document.{}", request.format);
            parser
                .extract_text_from_base64(&file_name, &request.content)
                .map_err(|e| AppError::validation(format!("文档解析失败: {}", e)))?
        } else {
            self.decode_text_content(&request.content)?
        };

        if text_content.trim().is_empty() {
            return Err(AppError::validation("文档内容为空"));
        }

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::Preprocessing {
                stage: "chunking".to_string(),
                message: format!(
                    "文档解析完成，共 {} 字符，正在分块...",
                    text_content.chars().count()
                ),
                percent: 15,
            });
        }

        let qbank_name = request
            .name
            .clone()
            .unwrap_or_else(|| "导入的题目集".to_string());

        let chunks = non_visual::text_chunks(&text_content);

        let total_chunks = chunks.len();

        // 创建会话
        let session_id = if let Some(sid) = &request.session_id {
            sid.clone()
        } else {
            let temp_id = uuid::Uuid::new_v4().to_string();
            let preview = ExamSheetPreviewResult {
                temp_id: temp_id.clone(),
                exam_name: Some(qbank_name.clone()),
                pages: vec![ExamSheetPreviewPage {
                    page_index: 0,
                    cards: Vec::new(),
                    blob_hash: None,
                    width: None,
                    height: None,
                    original_image_path: String::new(),
                    raw_ocr_text: None,
                    ocr_completed: false,
                    parse_completed: false,
                }],
                raw_model_response: None,
                instructions: None,
                session_id: Some(temp_id.clone()),
            };
            let preview_json = serde_json::to_value(&preview)
                .map_err(|e| AppError::validation(format!("序列化失败: {}", e)))?;

            let params = VfsCreateExamSheetParams {
                exam_name: Some(qbank_name.clone()),
                temp_id,
                metadata_json: json!({}),
                preview_json,
                status: "importing".to_string(),
                folder_id: request.folder_id.clone(),
            };
            let exam_sheet = VfsExamRepo::create_exam_sheet(vfs_db, params)
                .map_err(|e| AppError::database(format!("创建题目集失败: {}", e)))?;
            exam_sheet.id
        };

        // 初始 checkpoint（文本路径）
        let mut checkpoint = ImportCheckpointState {
            qbank_name: qbank_name.clone(),
            model_config_id: request.model_config_id.clone(),
            source_format: request.format.clone(),
            current_stage: ImportStage::Structuring,
            page_blob_hashes: Vec::new(),
            vlm_pages_completed: 0,
            vlm_page_results: Vec::new(),
            structuring_batches_completed: 0,
            structured_batch_results: Vec::new(),
            page_dimensions: Vec::new(),
            source_image_hashes: Vec::new(),
            import_mode: "text".to_string(),
            text_content: text_content.clone(),
            chunks_total: total_chunks,
            chunks_completed: 0,
            docx_image_entries: Vec::new(),
            docx_vlm_descriptions: HashMap::new(),
            docx_vlm_images_completed: 0,
            docx_enriched_text: String::new(),
            docx_marker_positions: Vec::new(),
            docx_vlm_direct_blob_hashes: Vec::new(),
            docx_vlm_direct_mimes: Vec::new(),
            docx_vlm_direct_exts: Vec::new(),
            docx_vlm_direct_text: String::new(),
            docx_vlm_direct_saved: 0,
            source_document_id: format!("{:x}", Sha256::digest(request.content.as_bytes())),
            candidate_results: Vec::new(),
            failed_chunks: Vec::new(),
            failed_pages: Vec::new(),
        };
        persist_checkpoint(vfs_db, &session_id, &checkpoint)?;

        if let Some(tx) = progress_tx {
            let _ = tx.send(QuestionImportProgress::SessionCreated {
                session_id: session_id.clone(),
                name: qbank_name.clone(),
                total_chunks,
            });
        }

        self.run_text_import_chunks(
            vfs_db,
            &session_id,
            &qbank_name,
            &chunks,
            &mut checkpoint,
            progress_tx,
        )
        .await
    }

    /// 创建导入会话记录
    fn create_import_session(
        &self,
        vfs_db: &VfsDatabase,
        request: &ImportRequest,
        qbank_name: &str,
        pages: &[PageSlice],
    ) -> Result<String, AppError> {
        if let Some(sid) = &request.session_id {
            return Ok(sid.clone());
        }

        let temp_id = uuid::Uuid::new_v4().to_string();

        let preview_pages: Vec<ExamSheetPreviewPage> = pages
            .iter()
            .map(|p| ExamSheetPreviewPage {
                page_index: p.page_index,
                cards: Vec::new(),
                blob_hash: Some(p.blob_hash.clone()),
                width: Some(p.width),
                height: Some(p.height),
                original_image_path: String::new(),
                raw_ocr_text: None,
                ocr_completed: true,
                parse_completed: false,
            })
            .collect();

        let preview = ExamSheetPreviewResult {
            temp_id: temp_id.clone(),
            exam_name: Some(qbank_name.to_string()),
            pages: preview_pages,
            raw_model_response: None,
            instructions: None,
            session_id: Some(temp_id.clone()),
        };

        let preview_json = serde_json::to_value(&preview)
            .map_err(|e| AppError::validation(format!("序列化失败: {}", e)))?;

        let params = VfsCreateExamSheetParams {
            exam_name: Some(qbank_name.to_string()),
            temp_id,
            metadata_json: json!({}),
            preview_json,
            status: "importing".to_string(),
            folder_id: request.folder_id.clone(),
        };

        let exam_sheet = VfsExamRepo::create_exam_sheet(vfs_db, params)
            .map_err(|e| AppError::database(format!("创建题目集失败: {}", e)))?;

        Ok(exam_sheet.id)
    }

    /// ★ P2 修复：就地修复"未结构化"的题目（只补选项拆分，不改题干语义）
    ///
    /// 历史导入中，LLM 失败的批次以 `question_type=other` + 选项留在 `content`
    /// 的形式落库，前端不会按选择题渲染，用户看到的就是「明明 20 道选择题只识别出
    /// 8 道」。这里复用导入兜底同一套本地拆分规则原地修复：
    /// - 不调用 LLM，零成本；
    /// - 幂等：已有 options 的题目直接跳过，可反复执行；
    /// - 只改 `content` / `options_json` / `question_type`，不动答案与用户作答。
    pub fn repair_unstructured_questions(
        &self,
        vfs_db: &VfsDatabase,
        exam_id: &str,
    ) -> Result<usize, AppError> {
        use crate::vfs::repos::{QuestionOption, QuestionType, UpdateQuestionParams};

        let conn = vfs_db
            .get_conn_safe()
            .map_err(|e| AppError::database(format!("获取数据库连接失败: {}", e)))?;

        let rows: Vec<(String, String, Option<String>)> = {
            let mut stmt = conn
                .prepare(
                    "SELECT id, content, options_json FROM questions \
                     WHERE exam_id = ?1 AND deleted_at IS NULL",
                )
                .map_err(|e| AppError::database(format!("查询题目失败: {}", e)))?;
            let mapped = stmt
                .query_map([exam_id], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|e| AppError::database(format!("查询题目失败: {}", e)))?;
            mapped.filter_map(|row| row.ok()).collect()
        };

        let mut repaired = 0usize;
        for (question_id, content, options_json) in rows {
            let has_options = options_json
                .as_deref()
                .map(|raw| {
                    let trimmed = raw.trim();
                    !trimmed.is_empty() && trimmed != "[]" && trimmed != "null"
                })
                .unwrap_or(false);
            if has_options {
                continue;
            }

            let Some((stem, options)) =
                crate::llm_structurer::split_options_from_raw_text(&content)
            else {
                continue;
            };

            let options: Vec<QuestionOption> = options
                .iter()
                .filter_map(|opt| {
                    let key = opt.get("key").and_then(|v| v.as_str())?.to_string();
                    let text = opt.get("content").and_then(|v| v.as_str())?.to_string();
                    Some(QuestionOption { key, content: text })
                })
                .collect();
            if options.len() < 2 {
                continue;
            }

            let params = UpdateQuestionParams {
                content: Some(stem),
                options: Some(options),
                question_type: Some(QuestionType::SingleChoice),
                ..Default::default()
            };
            crate::vfs::repos::VfsQuestionRepo::update_question_with_conn(
                &conn,
                &question_id,
                &params,
            )
            .map_err(|e| AppError::database(format!("更新题目 {} 失败: {}", question_id, e)))?;
            repaired += 1;
        }

        if repaired > 0 {
            if let Err(e) =
                crate::vfs::repos::VfsQuestionRepo::refresh_stats_with_conn(&conn, exam_id)
            {
                log::warn!("[QuestionImport] 修复后统计刷新失败: {}", e);
            }
            log::info!(
                "[QuestionImport] 本地修复完成: exam={}, 修复 {} 道未结构化选择题",
                exam_id,
                repaired
            );
        }

        Ok(repaired)
    }

    /// JSON 直接导入
    async fn import_json_directly(
        &self,
        vfs_db: &VfsDatabase,
        request: &ImportRequest,
    ) -> Result<ImportResult, AppError> {
        let json_content = self
            .decode_text_content(&request.content)
            .unwrap_or_else(|_| request.content.clone());

        let data: Value = serde_json::from_str(&json_content)
            .map_err(|e| AppError::validation(format!("JSON 解析失败: {}", e)))?;

        let questions = data
            .get("questions")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AppError::validation("JSON 格式错误：需要 questions 数组"))?
            .clone();

        if questions.is_empty() {
            return Err(AppError::validation("题目列表为空"));
        }

        let qbank_name = request
            .name
            .clone()
            .unwrap_or_else(|| "导入的题目集".to_string());

        let session_id = self.create_import_session(vfs_db, request, &qbank_name, &[])?;
        let candidates: Vec<crate::llm_structurer::StructuredQuestion> = questions
            .iter()
            .enumerate()
            .map(|(idx, q)| {
                let label = q
                    .get("label")
                    .or_else(|| q.get("question_label"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("Q{}", idx + 1));
                let kind = q
                    .get("block_kind")
                    .cloned()
                    .and_then(|v| serde_json::from_value(v).ok())
                    .unwrap_or_default();
                let source_id = format!("json:{}", idx);
                let block = crate::vlm_grounding_service::VlmQuestion {
                    label,
                    section: q
                        .get("section")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    block_kind: kind,
                    source_id: source_id.clone(),
                    raw_text: q
                        .get("raw_text")
                        .or_else(|| q.get("content"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string(),
                    ..Default::default()
                };
                crate::llm_structurer::StructuredQuestion {
                    json: q.clone(),
                    origin: crate::llm_structurer::StructureSource::Llm,
                    source: figure_extractor::QuestionWithFigures {
                        merged: cross_page_merger::MergedQuestion {
                            question: block,
                            page_indices: Vec::new(),
                            figures_with_page: Vec::new(),
                            source_spans: Vec::new(),
                        },
                        images: Vec::new(),
                    },
                }
            })
            .collect();
        let report = json!({"document_id": format!("{:x}", Sha256::digest(json_content.as_bytes())),
            "source_kind": "json", "resume_available": false});
        self.publish_import_candidates(
            vfs_db,
            &session_id,
            &qbank_name,
            &candidates,
            0,
            &[],
            report,
            None,
        )
    }

    /// 启动时检查可恢复的导入会话（供 lib.rs 启动流程调用）
    pub async fn recover_importing_sessions(
        &self,
        vfs_db: &VfsDatabase,
    ) -> Result<Vec<crate::vfs::repos::ImportingSession>, AppError> {
        VfsExamRepo::list_importing_sessions(vfs_db)
            .map_err(|e| AppError::database(format!("查询中断会话失败: {}", e)))
    }

    fn decode_text_content(&self, base64_content: &str) -> Result<String, AppError> {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64_content)
            .map_err(|e| AppError::validation(format!("Base64 解码失败: {}", e)))?;

        if let Ok(text) = String::from_utf8(bytes.clone()) {
            let text = if text.starts_with('\u{FEFF}') {
                text[3..].to_string()
            } else {
                text
            };
            return Ok(text);
        }

        let (decoded, _, had_errors) = encoding_rs::GBK.decode(&bytes);
        if !had_errors {
            return Ok(decoded.to_string());
        }

        let (decoded, _, _) = encoding_rs::GB18030.decode(&bytes);
        Ok(decoded.to_string())
    }
}

// ============================================================================
// 辅助函数
// ============================================================================

fn is_image_format(format: &str) -> bool {
    matches!(
        format,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tiff" | "image" | "heic" | "heif"
    )
}

fn count_valid_chars(text: &str) -> usize {
    text.chars()
        .filter(|c| {
            c.is_alphanumeric()
                || ('\u{4E00}'..='\u{9FFF}').contains(c)
                || ('\u{3400}'..='\u{4DBF}').contains(c)
        })
        .count()
}

fn detect_image_format(data: &[u8]) -> (&'static str, &'static str) {
    if data.starts_with(b"\x89PNG") {
        ("image/png", "png")
    } else if data.starts_with(b"\xFF\xD8\xFF") {
        ("image/jpeg", "jpg")
    } else if data.starts_with(b"RIFF") && data.len() > 12 && &data[8..12] == b"WEBP" {
        ("image/webp", "webp")
    } else {
        ("image/png", "png")
    }
}

/// 检查文本中是否存在选项标记结构：字母 `letter` 前为空白（或行首）、后跟选项分隔符。
/// 用于区分真正的选项列表（"B. 错误"）与题干中的字母引用（"A、B、C 三点共线"）。
fn contains_option_marker(text: &str, letter: char) -> bool {
    let mut prev: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == letter {
            let prev_ok = prev.is_none_or(|p| p.is_whitespace());
            if prev_ok {
                if let Some(&next) = chars.peek() {
                    if matches!(next, '.' | '、' | '．' | ':' | '：') {
                        return true;
                    }
                }
            }
        }
        prev = Some(c);
    }
    false
}

/// 题目内容去重 key：去除全部空白后取 SHA-256 前 16 字节。
/// 供 CSV 导入去重与 DOCX VLM 直提断点恢复去重共用。
fn content_dedup_key(content: &str) -> String {
    let normalized = content.trim().replace([' ', '\t', '\r', '\n'], "");
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    hex::encode(&hasher.finalize()[..16])
}

fn json_to_question_params(
    q: &Value,
    exam_id: &str,
    card_id: &str,
    index: usize,
) -> CreateQuestionParams {
    let mut content = q
        .get("content")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let options: Option<Vec<crate::vfs::repos::QuestionOption>> =
        q.get("options").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|opt| {
                    let key = opt
                        .get("key")
                        .and_then(|k| k.as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = opt
                        .get("content")
                        .and_then(|c| c.as_str())
                        .unwrap_or("")
                        .to_string();
                    if key.is_empty() && content.is_empty() {
                        None
                    } else {
                        Some(crate::vfs::repos::QuestionOption { key, content })
                    }
                })
                .collect()
        });

    // 后处理：选择题如果 options 有实际内容，移除 content 末尾重复的选项文本
    if let Some(ref opts) = options {
        if opts.len() >= 2 && opts.iter().any(|o| !o.content.is_empty()) {
            // 找到 content 末尾形如 "A. xxx B. xxx" 或 "A．xxx" 的选项文本并移除
            // 匹配模式：从最后一个 "\nA" 开始到末尾
            let patterns = ["\nA.", "\nA．", "\nA、", "\nA "];
            let mut best_cut = content.len();
            for pat in &patterns {
                if let Some(pos) = content.rfind(pat) {
                    // 确认后面确实有 B/C 的选项结构（字母前为空白、后跟分隔符），
                    // 避免把 "A、B、C 三点共线" 这类题干文本误判为选项列表
                    let after = &content[pos..];
                    if contains_option_marker(after, 'B') && contains_option_marker(after, 'C') {
                        best_cut = best_cut.min(pos);
                    }
                }
            }
            if best_cut < content.len() {
                content = content[..best_cut].trim_end().to_string();
            }
        }
    }

    let question_type =
        q.get("question_type")
            .and_then(|v| v.as_str())
            .map(|t| match t.to_lowercase().as_str() {
                "single_choice" | "单选" | "单选题" => {
                    crate::vfs::repos::QuestionType::SingleChoice
                }
                "multiple_choice" | "多选" | "多选题" => {
                    crate::vfs::repos::QuestionType::MultipleChoice
                }
                "indefinite_choice" | "不定项" => {
                    crate::vfs::repos::QuestionType::IndefiniteChoice
                }
                "fill_blank" | "填空" | "填空题" => crate::vfs::repos::QuestionType::FillBlank,
                "short_answer" | "简答" | "简答题" => {
                    crate::vfs::repos::QuestionType::ShortAnswer
                }
                "essay" | "论述" | "论述题" => crate::vfs::repos::QuestionType::Essay,
                "calculation" | "计算" | "计算题" => {
                    crate::vfs::repos::QuestionType::Calculation
                }
                "proof" | "证明" | "证明题" => crate::vfs::repos::QuestionType::Proof,
                "true_false" | "判断" | "判断题" | "judgment" => {
                    crate::vfs::repos::QuestionType::TrueFalse
                }
                "matching" | "匹配" | "匹配题" | "连线" | "连线题" => {
                    crate::vfs::repos::QuestionType::Matching
                }
                "ordering" | "排序" | "排序题" => crate::vfs::repos::QuestionType::Ordering,
                "numeric" | "数值" | "数值题" => crate::vfs::repos::QuestionType::Numeric,
                _ => crate::vfs::repos::QuestionType::Other,
            });

    let difficulty = q.get("difficulty").and_then(|v| v.as_str()).and_then(|d| {
        match d.to_lowercase().as_str() {
            "easy" | "简单" => Some(crate::vfs::repos::Difficulty::Easy),
            "medium" | "中等" => Some(crate::vfs::repos::Difficulty::Medium),
            "hard" | "困难" => Some(crate::vfs::repos::Difficulty::Hard),
            "very_hard" | "极难" => Some(crate::vfs::repos::Difficulty::VeryHard),
            _ => None,
        }
    });

    let tags = q.get("tags").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|t| t.as_str().map(String::from))
            .collect()
    });

    // 新题型契约：JSON 里携带的 structured_data 对象原样透传（matching/ordering/
    // numeric 缺少它无法自动判分）；非对象值丢弃，避免把 "null"/字符串写进列。
    let structured_data = q.get("structured_data").filter(|v| v.is_object()).cloned();

    CreateQuestionParams {
        exam_id: exam_id.to_string(),
        card_id: Some(card_id.to_string()),
        question_label: q
            .get("label")
            .or_else(|| q.get("question_label"))
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
            .or_else(|| Some(format!("Q{}", index + 1))),
        content,
        options,
        answer: q.get("answer").and_then(|v| v.as_str()).map(String::from),
        explanation: q
            .get("explanation")
            .and_then(|v| v.as_str())
            .map(String::from),
        question_type,
        difficulty,
        tags,
        source_type: Some(crate::vfs::repos::SourceType::Imported),
        source_ref: None,
        images: None,
        parent_id: None,
        structured_data,
    }
}

#[cfg(test)]
fn question_to_card(q: &Value, index: usize) -> ExamCardPreview {
    let content = q.get("content").and_then(|v| v.as_str()).unwrap_or("");
    let card_id = format!("card_{}", nanoid::nanoid!(12));

    ExamCardPreview {
        card_id,
        page_index: 0,
        question_label: format!("Q{}", index + 1),
        ocr_text: content.to_string(),
        tags: q
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        question_type: q
            .get("question_type")
            .and_then(|v| v.as_str())
            .and_then(|t| serde_json::from_str(&format!("\"{}\"", t)).ok()),
        answer: q.get("answer").and_then(|v| v.as_str()).map(String::from),
        explanation: q
            .get("explanation")
            .and_then(|v| v.as_str())
            .map(String::from),
        difficulty: q
            .get("difficulty")
            .and_then(|v| v.as_str())
            .and_then(|d| serde_json::from_str(&format!("\"{}\"", d)).ok()),
        status: QuestionStatus::New,
        source_type: SourceType::ImportFile,
        ..Default::default()
    }
}

/// 一道题都没提取到时给用户的错误：带上首个分块 / 页面的失败原因（如「对话模型未配置」
/// 「未找到可用的 VLM 模型」），否则用户只看到笼统的「未能提取到题目」无从下手。
fn no_questions_error(prefix: &str, first_failure: Option<&str>) -> String {
    match first_failure.map(str::trim) {
        Some(reason) if !reason.is_empty() => format!("{}：{}", prefix, reason),
        _ => prefix.to_string(),
    }
}

// ============================================================================
// DOCX 原生导入辅助类型与函数
// ============================================================================

/// DOCX 嵌入图片在 VFS 中的映射条目
#[derive(Debug, Clone)]
struct DocxImageEntry {
    /// 图片在原文中的 `<<IMG:N>>` 索引
    marker_index: usize,
    /// VFS blob hash
    blob_hash: String,
    /// MIME 类型
    mime: String,
    /// 文件扩展名
    ext: String,
}

/// DocxImageEntry 的可序列化版本（用于 checkpoint 持久化）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocxImageEntrySerde {
    pub marker_index: usize,
    pub blob_hash: String,
    pub mime: String,
    pub ext: String,
}

impl From<&DocxImageEntry> for DocxImageEntrySerde {
    fn from(e: &DocxImageEntry) -> Self {
        Self {
            marker_index: e.marker_index,
            blob_hash: e.blob_hash.clone(),
            mime: e.mime.clone(),
            ext: e.ext.clone(),
        }
    }
}

impl From<&DocxImageEntrySerde> for DocxImageEntry {
    fn from(e: &DocxImageEntrySerde) -> Self {
        Self {
            marker_index: e.marker_index,
            blob_hash: e.blob_hash.clone(),
            mime: e.mime.clone(),
            ext: e.ext.clone(),
        }
    }
}

/// 用 VLM 描述替换原文中的 `<<IMG:N>>` 标记，构建增强文本
fn decode_base64_content(content: &str) -> Result<Vec<u8>, AppError> {
    use base64::Engine;
    let base64_str = if content.starts_with("data:") {
        content
            .split(',')
            .nth(1)
            .ok_or_else(|| AppError::validation("无效的 Data URL 格式"))?
    } else {
        content
    };
    base64::engine::general_purpose::STANDARD
        .decode(base64_str)
        .map_err(|e| AppError::validation(format!("Base64 解码失败: {}", e)))
}

/// 从 LLM 输出的单道题 JSON 中扫描 `<<IMG:N...>>` 标记，返回关联的 QuestionImage 列表
///
/// 同时匹配两种格式：
/// - `<<IMG:N>>` — 原始标记
/// - `<<IMG:N:[图片内容: ...]>>` — VLM 增强标记
// ============================================================================
// checkpoint 辅助函数
// ============================================================================

fn persist_checkpoint(
    vfs_db: &VfsDatabase,
    session_id: &str,
    cp: &ImportCheckpointState,
) -> Result<(), AppError> {
    let value = serde_json::to_value(cp)
        .map_err(|e| AppError::validation(format!("序列化导入状态失败: {}", e)))?;
    VfsExamRepo::update_import_state(vfs_db, session_id, &value)
        .map_err(|e| AppError::database(format!("保存导入状态失败: {}", e)))
}

fn rebuild_pages_from_checkpoint(
    checkpoint: &ImportCheckpointState,
    _vfs_db: &VfsDatabase,
) -> Result<Vec<PageSlice>, AppError> {
    let mut pages = Vec::with_capacity(checkpoint.page_blob_hashes.len());

    for (idx, hash) in checkpoint.page_blob_hashes.iter().enumerate() {
        let (width, height) = checkpoint
            .page_dimensions
            .get(idx)
            .copied()
            .unwrap_or((0, 0));

        pages.push(PageSlice {
            page_index: idx,
            blob_hash: hash.clone(),
            text_hint: None,
            width,
            height,
        });
    }

    Ok(pages)
}

/// 为已存储的 blob 创建 VFS files 表条目，使前端 `vfs_get_attachment_content` 可以按 file ID 加载图片。
/// 返回生成的 file_id（`file_` 前缀）。对同一 blob_hash 重复调用是安全的（INSERT OR IGNORE）。
fn ensure_vfs_file_for_blob(
    vfs_db: &VfsDatabase,
    blob_hash: &str,
    file_name: &str,
    mime: &str,
) -> Option<String> {
    use rusqlite::OptionalExtension;

    // 先查是否已有 file 条目引用此 blob_hash
    let conn = match vfs_db.get_conn_safe() {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[QuestionImport] ensure_vfs_file: 获取连接失败: {}", e);
            return None;
        }
    };

    // 查找已有的 file 条目
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM files WHERE blob_hash = ?1 AND deleted_at IS NULL LIMIT 1",
            rusqlite::params![blob_hash],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten();

    if let Some(id) = existing {
        return Some(id);
    }

    // 创建新的 file 条目
    let file_id = format!("file_{}", nanoid::nanoid!(12));
    let now = chrono::Utc::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    if let Err(e) = conn.execute(
        r#"INSERT OR IGNORE INTO files (id, blob_hash, file_name, original_path, size, "type", mime_type, tags_json, is_favorite, status, created_at, updated_at)
           VALUES (?1, ?2, ?3, '', 0, 'image', ?4, '[]', 0, 'active', ?5, ?6)"#,
        rusqlite::params![file_id, blob_hash, file_name, mime, now, now],
    ) {
        log::warn!(
            "[QuestionImport] ensure_vfs_file: 创建 file 条目失败: {}",
            e
        );
        return None;
    }

    Some(file_id)
}

// ============================================================================
// CSV 导入功能（保持不变）
// ============================================================================

/// CSV 导入去重策略
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CsvDuplicateStrategy {
    #[default]
    Skip,
    Overwrite,
    Merge,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvImportRequest {
    pub file_path: String,
    pub exam_id: String,
    pub field_mapping: HashMap<String, String>,
    #[serde(default)]
    pub duplicate_strategy: CsvDuplicateStrategy,
    pub folder_id: Option<String>,
    pub exam_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvImportResult {
    pub success_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub errors: Vec<CsvImportError>,
    pub exam_id: String,
    pub total_rows: usize,
    /// 用户请求取消时为 true。已写入的行保留，剩余行不会继续处理。
    #[serde(default)]
    pub cancelled: bool,
}

const CSV_IMPORT_ACTIVE: u8 = 0;
const CSV_IMPORT_CANCELLED: u8 = 1;
const CSV_IMPORT_COMPLETED: u8 = 2;

/// Coordinates a CSV import's terminal state with a cancellation request.
///
/// `request_cancel` and `finish_or_cancelled` race through a compare-and-swap
/// rather than independent loads. This means a cancellation is accepted only
/// while the import is still active, and an accepted cancellation cannot later
/// be reported as a successful completion.
#[derive(Debug)]
pub struct CsvImportCancellation {
    state: AtomicU8,
}

impl Default for CsvImportCancellation {
    fn default() -> Self {
        Self::new()
    }
}

impl CsvImportCancellation {
    pub fn new() -> Self {
        Self {
            state: AtomicU8::new(CSV_IMPORT_ACTIVE),
        }
    }

    pub fn request_cancel(&self) -> bool {
        self.state
            .compare_exchange(
                CSV_IMPORT_ACTIVE,
                CSV_IMPORT_CANCELLED,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn is_cancelled(&self) -> bool {
        self.state.load(Ordering::Acquire) == CSV_IMPORT_CANCELLED
    }

    /// Marks a successful import terminally complete. Returns `true` when a
    /// cancellation request won the race and the caller must report Cancelled.
    fn finish_or_cancelled(&self) -> bool {
        match self.state.compare_exchange(
            CSV_IMPORT_ACTIVE,
            CSV_IMPORT_COMPLETED,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => false,
            Err(CSV_IMPORT_CANCELLED) => true,
            Err(CSV_IMPORT_COMPLETED) => false,
            Err(state) => {
                debug_assert!(false, "unknown CSV import cancellation state: {state}");
                false
            }
        }
    }
}

/// Finalizes an in-flight token on every return path, including validation and
/// I/O errors. A late cancel then correctly reports that the import has ended.
struct CsvImportCompletionGuard<'a> {
    cancellation: Option<&'a CsvImportCancellation>,
}

impl<'a> CsvImportCompletionGuard<'a> {
    fn new(cancellation: Option<&'a CsvImportCancellation>) -> Self {
        Self { cancellation }
    }
}

impl Drop for CsvImportCompletionGuard<'_> {
    fn drop(&mut self) {
        if let Some(cancellation) = self.cancellation {
            let _ = cancellation.finish_or_cancelled();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvImportError {
    pub row: usize,
    pub message: String,
    pub raw_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum CsvImportProgress {
    Started {
        total_rows: usize,
        file_path: String,
        exam_id: String,
    },
    Progress {
        current: usize,
        total: usize,
        success: usize,
        skipped: usize,
        failed: usize,
        exam_id: String,
    },
    Completed(CsvImportResult),
    Cancelled(CsvImportResult),
    Failed {
        error: String,
        exam_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvPreviewResult {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: usize,
    pub encoding: String,
}

pub struct CsvImportService;

const CSV_IMPORT_TARGET_FIELDS: &[&str] = &[
    "question_label",
    "content",
    "options",
    "answer",
    "explanation",
    "question_type",
    "difficulty",
    "tags",
    "images",
];

impl CsvImportService {
    pub fn preview_csv(file_path: &str, preview_rows: usize) -> Result<CsvPreviewResult, AppError> {
        let (content, encoding) = Self::read_file_with_encoding(file_path)?;
        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(true)
            .from_reader(content.as_bytes());

        let headers: Vec<String> = reader
            .headers()
            .map_err(|e| AppError::validation(format!("读取 CSV 表头失败: {}", e)))?
            .iter()
            .map(|h| h.to_string())
            .collect();

        let mut rows = Vec::new();
        let mut total_rows = 0;
        for result in reader.records() {
            total_rows += 1;
            if rows.len() < preview_rows {
                if let Ok(record) = result {
                    rows.push(record.iter().map(|s| s.to_string()).collect());
                }
            }
        }

        Ok(CsvPreviewResult {
            headers,
            rows,
            total_rows,
            encoding,
        })
    }

    pub fn import_csv(
        vfs_db: &VfsDatabase,
        request: &CsvImportRequest,
        progress_tx: Option<UnboundedSender<CsvImportProgress>>,
        cancellation: Option<Arc<CsvImportCancellation>>,
    ) -> Result<CsvImportResult, AppError> {
        log::info!(
            "[CsvImport] 开始导入: {} -> exam_id={}",
            request.file_path,
            request.exam_id
        );
        let _completion_guard = CsvImportCompletionGuard::new(cancellation.as_deref());

        if Self::is_cancelled(cancellation.as_deref()) {
            return Ok(Self::cancelled_result(
                request.exam_id.clone(),
                0,
                0,
                0,
                0,
                Vec::new(),
                progress_tx.as_ref(),
            ));
        }

        let (content, encoding) = match Self::read_file_with_encoding(&request.file_path) {
            Ok(result) => result,
            Err(error) => {
                if Self::finish_or_cancelled(cancellation.as_deref()) {
                    return Ok(Self::cancelled_result(
                        request.exam_id.clone(),
                        0,
                        0,
                        0,
                        0,
                        Vec::new(),
                        progress_tx.as_ref(),
                    ));
                }
                return Err(error);
            }
        };
        log::info!("[CsvImport] 编码: {}", encoding);

        if Self::is_cancelled(cancellation.as_deref()) {
            return Ok(Self::cancelled_result(
                request.exam_id.clone(),
                0,
                0,
                0,
                0,
                Vec::new(),
                progress_tx.as_ref(),
            ));
        }

        let mut reader = csv::ReaderBuilder::new()
            .flexible(true)
            .has_headers(true)
            .from_reader(content.as_bytes());

        let headers: Vec<String> = match reader.headers() {
            Ok(headers) => headers.iter().map(|header| header.to_string()).collect(),
            Err(error) => {
                if Self::finish_or_cancelled(cancellation.as_deref()) {
                    return Ok(Self::cancelled_result(
                        request.exam_id.clone(),
                        0,
                        0,
                        0,
                        0,
                        Vec::new(),
                        progress_tx.as_ref(),
                    ));
                }
                return Err(AppError::validation(format!(
                    "读取 CSV 表头失败: {}",
                    error
                )));
            }
        };

        if let Err(error) = Self::validate_field_mapping(&headers, &request.field_mapping) {
            if Self::finish_or_cancelled(cancellation.as_deref()) {
                return Ok(Self::cancelled_result(
                    request.exam_id.clone(),
                    0,
                    0,
                    0,
                    0,
                    Vec::new(),
                    progress_tx.as_ref(),
                ));
            }
            return Err(error);
        }

        // Keep total-row progress while allowing a cancellation to stop a large
        // CSV during parsing instead of waiting for `collect()` to finish.
        let mut records = Vec::new();
        for record in reader.records() {
            if Self::is_cancelled(cancellation.as_deref()) {
                return Ok(Self::cancelled_result(
                    request.exam_id.clone(),
                    records.len(),
                    0,
                    0,
                    0,
                    Vec::new(),
                    progress_tx.as_ref(),
                ));
            }
            records.push(record);
        }
        let total_rows = records.len();

        // 在创建题目集或写入任意一行前处理已到达的取消请求，避免取消一个尚未开始的
        // 导入仍产生空题目集。
        if Self::is_cancelled(cancellation.as_deref()) {
            return Ok(Self::cancelled_result(
                request.exam_id.clone(),
                total_rows,
                0,
                0,
                0,
                Vec::new(),
                progress_tx.as_ref(),
            ));
        }

        let (exam_id, created_exam) = match Self::ensure_exam_exists(vfs_db, request) {
            Ok(result) => result,
            Err(error) => {
                if Self::finish_or_cancelled(cancellation.as_deref()) {
                    return Ok(Self::cancelled_result(
                        request.exam_id.clone(),
                        total_rows,
                        0,
                        0,
                        0,
                        Vec::new(),
                        progress_tx.as_ref(),
                    ));
                }
                return Err(error);
            }
        };
        if Self::is_cancelled(cancellation.as_deref()) {
            if created_exam {
                Self::discard_new_empty_exam(vfs_db, &exam_id);
            }
            return Ok(Self::cancelled_result(
                request.exam_id.clone(),
                total_rows,
                0,
                0,
                0,
                Vec::new(),
                progress_tx.as_ref(),
            ));
        }
        let mut existing_hashes = match Self::get_existing_content_hashes(vfs_db, &exam_id) {
            Ok(hashes) => hashes,
            Err(error) => {
                if Self::finish_or_cancelled(cancellation.as_deref()) {
                    if created_exam {
                        Self::discard_new_empty_exam(vfs_db, &exam_id);
                    }
                    return Ok(Self::cancelled_result(
                        exam_id,
                        total_rows,
                        0,
                        0,
                        0,
                        Vec::new(),
                        progress_tx.as_ref(),
                    ));
                }
                if created_exam {
                    Self::discard_new_empty_exam(vfs_db, &exam_id);
                }
                return Err(error);
            }
        };

        if Self::is_cancelled(cancellation.as_deref()) {
            if created_exam {
                Self::discard_new_empty_exam(vfs_db, &exam_id);
            }
            return Ok(Self::cancelled_result(
                request.exam_id.clone(),
                total_rows,
                0,
                0,
                0,
                Vec::new(),
                progress_tx.as_ref(),
            ));
        }

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(CsvImportProgress::Started {
                total_rows,
                file_path: request.file_path.clone(),
                exam_id: exam_id.clone(),
            });
        }

        let mut success_count = 0;
        let mut skipped_count = 0;
        let mut failed_count = 0;
        let mut errors = Vec::new();

        for (idx, result) in records.into_iter().enumerate() {
            // 取消在单行的数据库写入之间生效。这样不会回滚已完成的行，但能保证
            // 取消确认后不再开始后续写入。
            if Self::is_cancelled(cancellation.as_deref()) {
                if created_exam {
                    Self::discard_new_empty_exam(vfs_db, &exam_id);
                }
                if success_count > 0 || skipped_count > 0 || failed_count > 0 {
                    if let Err(e) = VfsQuestionRepo::refresh_stats(vfs_db, &exam_id) {
                        log::warn!("[CsvImport] 取消后刷新统计失败: {}", e);
                    }
                }
                return Ok(Self::cancelled_result(
                    exam_id,
                    total_rows,
                    success_count,
                    skipped_count,
                    failed_count,
                    errors,
                    progress_tx.as_ref(),
                ));
            }
            let row_num = idx + 2;
            match result {
                Ok(record) => {
                    match Self::process_csv_row(
                        vfs_db,
                        &exam_id,
                        &headers,
                        &record,
                        &request.field_mapping,
                        &request.duplicate_strategy,
                        &mut existing_hashes,
                        row_num,
                    ) {
                        Ok(CsvRowResult::Success) => success_count += 1,
                        Ok(CsvRowResult::Skipped) => skipped_count += 1,
                        Ok(CsvRowResult::Updated) => success_count += 1,
                        Err(e) => {
                            failed_count += 1;
                            errors.push(CsvImportError {
                                row: row_num,
                                message: e.to_string(),
                                raw_data: Some(record.iter().collect::<Vec<_>>().join(",")),
                            });
                        }
                    }
                }
                Err(e) => {
                    failed_count += 1;
                    errors.push(CsvImportError {
                        row: row_num,
                        message: format!("解析行失败: {}", e),
                        raw_data: None,
                    });
                }
            }

            if let Some(ref tx) = progress_tx {
                if (idx + 1) % 10 == 0 || idx + 1 == total_rows {
                    let _ = tx.send(CsvImportProgress::Progress {
                        current: idx + 1,
                        total: total_rows,
                        success: success_count,
                        skipped: skipped_count,
                        failed: failed_count,
                        exam_id: exam_id.clone(),
                    });
                }
            }
        }

        // This compare-and-swap is the terminal linearization point. A cancel
        // request either wins here and receives Cancelled, or loses and is told
        // that the import has already completed.
        if Self::finish_or_cancelled(cancellation.as_deref()) {
            if created_exam {
                Self::discard_new_empty_exam(vfs_db, &exam_id);
            }
            if success_count > 0 || skipped_count > 0 || failed_count > 0 {
                if let Err(e) = VfsQuestionRepo::refresh_stats(vfs_db, &exam_id) {
                    log::warn!("[CsvImport] 取消后刷新统计失败: {}", e);
                }
            }
            return Ok(Self::cancelled_result(
                exam_id,
                total_rows,
                success_count,
                skipped_count,
                failed_count,
                errors,
                progress_tx.as_ref(),
            ));
        }

        if let Err(e) = VfsQuestionRepo::refresh_stats(vfs_db, &exam_id) {
            log::warn!("[CsvImport] 统计刷新失败: {}", e);
        }

        let result = CsvImportResult {
            success_count,
            skipped_count,
            failed_count,
            errors,
            exam_id,
            total_rows,
            cancelled: false,
        };

        if let Some(ref tx) = progress_tx {
            let _ = tx.send(CsvImportProgress::Completed(result.clone()));
        }

        Ok(result)
    }

    fn is_cancelled(cancellation: Option<&CsvImportCancellation>) -> bool {
        cancellation.is_some_and(CsvImportCancellation::is_cancelled)
    }

    fn finish_or_cancelled(cancellation: Option<&CsvImportCancellation>) -> bool {
        cancellation.is_some_and(CsvImportCancellation::finish_or_cancelled)
    }

    fn cancelled_result(
        exam_id: String,
        total_rows: usize,
        success_count: usize,
        skipped_count: usize,
        failed_count: usize,
        errors: Vec<CsvImportError>,
        progress_tx: Option<&UnboundedSender<CsvImportProgress>>,
    ) -> CsvImportResult {
        let result = CsvImportResult {
            success_count,
            skipped_count,
            failed_count,
            errors,
            exam_id,
            total_rows,
            cancelled: true,
        };

        if let Some(tx) = progress_tx {
            let _ = tx.send(CsvImportProgress::Cancelled(result.clone()));
        }

        result
    }

    fn validate_file_path(path: &str) -> Result<(), AppError> {
        if std::path::Path::new(path).to_string_lossy().contains("..") {
            return Err(AppError::validation("路径不允许包含 '..' 目录遍历"));
        }
        Ok(())
    }

    fn read_file_with_encoding(file_path: &str) -> Result<(String, String), AppError> {
        Self::validate_file_path(file_path)?;
        let file = std::fs::File::open(file_path)
            .map_err(|e| AppError::internal(format!("打开文件失败: {}", e)))?;
        let mut reader = BufReader::new(file);
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| AppError::internal(format!("读取文件失败: {}", e)))?;

        if let Ok(content) = String::from_utf8(bytes.clone()) {
            let content = if content.starts_with('\u{FEFF}') {
                content[3..].to_string()
            } else {
                content
            };
            return Ok((content, "UTF-8".to_string()));
        }
        let (decoded, _, had_errors) = encoding_rs::GBK.decode(&bytes);
        if !had_errors {
            return Ok((decoded.to_string(), "GBK".to_string()));
        }
        let (decoded, _, _) = encoding_rs::GB18030.decode(&bytes);
        Ok((decoded.to_string(), "GB18030".to_string()))
    }

    fn validate_field_mapping(
        headers: &[String],
        field_mapping: &HashMap<String, String>,
    ) -> Result<(), AppError> {
        if !field_mapping.values().any(|v| v == "content") {
            return Err(AppError::validation("字段映射中必须包含 content 字段"));
        }
        for csv_col in field_mapping.keys() {
            if !headers.contains(csv_col) {
                return Err(AppError::validation(format!(
                    "CSV 文件中不存在列 '{}'",
                    csv_col
                )));
            }
        }
        for target_field in field_mapping.values() {
            if !CSV_IMPORT_TARGET_FIELDS.contains(&target_field.as_str()) {
                return Err(AppError::validation(format!(
                    "不支持的目标字段 '{}', 仅支持: {}",
                    target_field,
                    CSV_IMPORT_TARGET_FIELDS.join(", ")
                )));
            }
        }
        let mut seen_targets = HashSet::new();
        for target_field in field_mapping.values() {
            if !seen_targets.insert(target_field.as_str()) {
                return Err(AppError::validation(format!(
                    "目标字段 '{}' 被重复映射，请确保每个目标字段只映射一次",
                    target_field
                )));
            }
        }
        Ok(())
    }

    fn discard_new_empty_exam(vfs_db: &VfsDatabase, exam_id: &str) {
        use rusqlite::params;

        let conn = match vfs_db.get_conn_safe() {
            Ok(conn) => conn,
            Err(error) => {
                log::warn!(
                    "[CsvImport] 取消后检查新建题目集失败 ({}): {}",
                    exam_id,
                    error
                );
                return;
            }
        };
        let question_count = match conn.query_row(
            "SELECT COUNT(*) FROM questions WHERE exam_id = ?1 AND deleted_at IS NULL",
            params![exam_id],
            |row| row.get::<_, i64>(0),
        ) {
            Ok(count) => count,
            Err(error) => {
                log::warn!(
                    "[CsvImport] 取消后检查新建题目集失败 ({}): {}",
                    exam_id,
                    error
                );
                return;
            }
        };
        if question_count > 0 {
            return;
        }
        if let Err(error) = VfsExamRepo::purge_exam_sheet(vfs_db, exam_id) {
            log::warn!(
                "[CsvImport] 取消后清理新建空题目集失败 ({}): {}",
                exam_id,
                error
            );
        }
    }

    fn ensure_exam_exists(
        vfs_db: &VfsDatabase,
        request: &CsvImportRequest,
    ) -> Result<(String, bool), AppError> {
        if let Ok(Some(_)) = VfsExamRepo::get_exam_sheet(vfs_db, &request.exam_id) {
            return Ok((request.exam_id.clone(), false));
        }

        let exam_name = request.exam_name.clone().unwrap_or_else(|| {
            let file_name = std::path::Path::new(&request.file_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("CSV导入");
            format!("CSV导入 - {}", file_name)
        });

        let preview = ExamSheetPreviewResult {
            temp_id: request.exam_id.clone(),
            exam_name: Some(exam_name.clone()),
            pages: vec![ExamSheetPreviewPage {
                page_index: 0,
                cards: Vec::new(),
                blob_hash: None,
                width: None,
                height: None,
                original_image_path: String::new(),
                raw_ocr_text: None,
                ocr_completed: false,
                parse_completed: false,
            }],
            raw_model_response: None,
            instructions: None,
            session_id: Some(request.exam_id.clone()),
        };

        let preview_json = serde_json::to_value(&preview)
            .map_err(|e| AppError::validation(format!("序列化失败: {}", e)))?;

        let params = VfsCreateExamSheetParams {
            exam_name: Some(exam_name),
            temp_id: request.exam_id.clone(),
            metadata_json: json!({}),
            preview_json,
            status: "completed".to_string(),
            folder_id: request.folder_id.clone(),
        };

        // P1 修复：create_exam_sheet 内部会用 generate_id() 生成新 ID（request.exam_id 仅作 temp_id），
        // 必须返回实际生成的 ID，否则后续题目会挂到不存在的 exam_id 下成为孤儿数据。
        let exam_sheet = VfsExamRepo::create_exam_sheet(vfs_db, params)
            .map_err(|e| AppError::database(format!("创建题目集失败: {}", e)))?;
        Ok((exam_sheet.id, true))
    }

    fn get_existing_content_hashes(
        vfs_db: &VfsDatabase,
        exam_id: &str,
    ) -> Result<HashMap<String, String>, AppError> {
        use rusqlite::params;
        let conn = vfs_db
            .get_conn_safe()
            .map_err(|e| AppError::database(format!("获取连接失败: {}", e)))?;
        let mut stmt = conn
            .prepare("SELECT id, content FROM questions WHERE exam_id = ?1 AND deleted_at IS NULL")
            .map_err(|e| AppError::database(format!("准备查询失败: {}", e)))?;
        let rows = stmt
            .query_map(params![exam_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| AppError::database(format!("查询失败: {}", e)))?;

        let mut hashes = HashMap::new();
        for row in rows {
            let (id, content) =
                row.map_err(|e| AppError::database(format!("读取行失败: {}", e)))?;
            hashes.insert(Self::compute_content_hash(&content), id);
        }
        Ok(hashes)
    }

    fn compute_content_hash(content: &str) -> String {
        content_dedup_key(content)
    }

    fn process_csv_row(
        vfs_db: &VfsDatabase,
        exam_id: &str,
        headers: &[String],
        record: &csv::StringRecord,
        field_mapping: &HashMap<String, String>,
        duplicate_strategy: &CsvDuplicateStrategy,
        existing_hashes: &mut HashMap<String, String>,
        row_num: usize,
    ) -> Result<CsvRowResult, AppError> {
        let mut field_values: HashMap<String, String> = HashMap::new();
        for (csv_col, target_field) in field_mapping {
            if let Some(col_idx) = headers.iter().position(|h| h == csv_col) {
                if let Some(value) = record.get(col_idx) {
                    if !value.trim().is_empty() {
                        field_values.insert(target_field.clone(), value.to_string());
                    }
                }
            }
        }

        let content = field_values
            .get("content")
            .ok_or_else(|| AppError::validation(format!("第 {} 行: content 为空", row_num)))?;
        if content.trim().is_empty() {
            return Err(AppError::validation(format!(
                "第 {} 行: content 为空",
                row_num
            )));
        }

        let content_hash = Self::compute_content_hash(content);
        let parsed_images = match field_values.get("images") {
            Some(raw) => Self::parse_images(raw)
                .map_err(|msg| AppError::validation(format!("第 {} 行: {}", row_num, msg)))?,
            None => None,
        };

        if let Some(existing_id) = existing_hashes.get(&content_hash) {
            match duplicate_strategy {
                CsvDuplicateStrategy::Skip => return Ok(CsvRowResult::Skipped),
                CsvDuplicateStrategy::Overwrite => {
                    let params = Self::build_update_params(&field_values, parsed_images.clone());
                    VfsQuestionRepo::update_question(vfs_db, existing_id, &params)
                        .map_err(|e| AppError::database(format!("更新失败: {}", e)))?;
                    return Ok(CsvRowResult::Updated);
                }
                CsvDuplicateStrategy::Merge => {
                    if let Ok(Some(existing)) = VfsQuestionRepo::get_question(vfs_db, existing_id) {
                        let params = Self::build_merge_params(
                            &field_values,
                            &existing,
                            parsed_images.clone(),
                        );
                        VfsQuestionRepo::update_question(vfs_db, existing_id, &params)
                            .map_err(|e| AppError::database(format!("合并失败: {}", e)))?;
                        return Ok(CsvRowResult::Updated);
                    }
                    // 哈希表中存在但读取失败/已被删除：跳过而非创建重复题
                    log::warn!(
                        "[CsvImport] 第 {} 行: 合并目标题目 {} 不可读，按跳过处理",
                        row_num,
                        existing_id
                    );
                    return Ok(CsvRowResult::Skipped);
                }
            }
        }

        let params = Self::build_create_params(exam_id, &field_values, parsed_images, row_num);
        let new_q = VfsQuestionRepo::create_question(vfs_db, &params)
            .map_err(|e| AppError::database(format!("创建失败: {}", e)))?;
        existing_hashes.insert(content_hash, new_q.id);
        Ok(CsvRowResult::Success)
    }

    fn build_create_params(
        exam_id: &str,
        fv: &HashMap<String, String>,
        parsed_images: Option<Vec<QuestionImage>>,
        row_num: usize,
    ) -> CreateQuestionParams {
        CreateQuestionParams {
            exam_id: exam_id.to_string(),
            card_id: Some(format!("csv_{}", nanoid::nanoid!(10))),
            question_label: fv
                .get("question_label")
                .cloned()
                .or_else(|| Some(format!("Q{}", row_num - 1))),
            content: fv.get("content").cloned().unwrap_or_default(),
            options: fv
                .get("options")
                .and_then(|s| Self::parse_options_string(s)),
            answer: fv.get("answer").cloned(),
            explanation: fv.get("explanation").cloned(),
            question_type: fv
                .get("question_type")
                .and_then(|t| Self::parse_question_type(t)),
            difficulty: fv.get("difficulty").and_then(|d| Self::parse_difficulty(d)),
            tags: fv.get("tags").and_then(|t| Self::parse_tags(t)),
            source_type: Some(crate::vfs::repos::SourceType::Imported),
            source_ref: Some("csv".to_string()),
            images: parsed_images,
            parent_id: None,
            structured_data: None,
        }
    }

    fn build_update_params(
        fv: &HashMap<String, String>,
        parsed_images: Option<Vec<QuestionImage>>,
    ) -> crate::vfs::repos::UpdateQuestionParams {
        let mut params = crate::vfs::repos::UpdateQuestionParams::default();
        if let Some(v) = fv.get("content") {
            params.content = Some(v.clone());
        }
        if let Some(v) = fv.get("answer") {
            params.answer = Some(v.clone());
        }
        if let Some(v) = fv.get("explanation") {
            params.explanation = Some(v.clone());
        }
        if let Some(v) = fv.get("options") {
            params.options = Self::parse_options_string(v);
        }
        if let Some(v) = fv.get("question_type") {
            params.question_type = Self::parse_question_type(v);
        }
        if let Some(v) = fv.get("difficulty") {
            params.difficulty = Self::parse_difficulty(v);
        }
        if let Some(v) = fv.get("tags") {
            params.tags = Self::parse_tags(v);
        }
        if let Some(images) = parsed_images {
            params.images = Some(images);
        }
        params
    }

    fn build_merge_params(
        fv: &HashMap<String, String>,
        existing: &crate::vfs::repos::Question,
        parsed_images: Option<Vec<QuestionImage>>,
    ) -> crate::vfs::repos::UpdateQuestionParams {
        let mut params = crate::vfs::repos::UpdateQuestionParams::default();
        if existing.answer.is_none() {
            if let Some(v) = fv.get("answer") {
                params.answer = Some(v.clone());
            }
        }
        if existing.explanation.is_none() {
            if let Some(v) = fv.get("explanation") {
                params.explanation = Some(v.clone());
            }
        }
        if existing.options.is_none() {
            if let Some(v) = fv.get("options") {
                params.options = Self::parse_options_string(v);
            }
        }
        if existing.tags.is_empty() {
            if let Some(v) = fv.get("tags") {
                params.tags = Self::parse_tags(v);
            }
        }
        if existing.images.is_empty() {
            if let Some(images) = parsed_images {
                if !images.is_empty() {
                    params.images = Some(images);
                }
            }
        }
        params
    }

    fn parse_options_string(s: &str) -> Option<Vec<crate::vfs::repos::QuestionOption>> {
        let s = s.trim();
        if s.starts_with('[') {
            if let Ok(opts) = serde_json::from_str::<Vec<crate::vfs::repos::QuestionOption>>(s) {
                return Some(opts);
            }
        }
        for sep in [';', '\n', '|'] {
            let parts: Vec<&str> = s.split(sep).filter(|p| !p.trim().is_empty()).collect();
            if parts.len() >= 2 {
                let options: Vec<_> = parts
                    .iter()
                    .filter_map(|part| {
                        let part = part.trim();
                        let caps = CSV_OPTION_ITEM_RE.captures(part)?;
                        Some(crate::vfs::repos::QuestionOption {
                            key: caps.get(1)?.as_str().to_uppercase(),
                            content: caps.get(2)?.as_str().to_string(),
                        })
                    })
                    .collect();
                if !options.is_empty() {
                    return Some(options);
                }
            }
        }
        None
    }

    fn parse_question_type(s: &str) -> Option<crate::vfs::repos::QuestionType> {
        match s.trim().to_lowercase().as_str() {
            "single_choice" | "单选" | "单选题" => {
                Some(crate::vfs::repos::QuestionType::SingleChoice)
            }
            "multiple_choice" | "多选" | "多选题" => {
                Some(crate::vfs::repos::QuestionType::MultipleChoice)
            }
            "indefinite_choice" | "不定项" | "不定项选择题" => {
                Some(crate::vfs::repos::QuestionType::IndefiniteChoice)
            }
            "fill_blank" | "填空" | "填空题" => {
                Some(crate::vfs::repos::QuestionType::FillBlank)
            }
            "short_answer" | "简答" | "简答题" => {
                Some(crate::vfs::repos::QuestionType::ShortAnswer)
            }
            "essay" | "论述" | "论述题" => Some(crate::vfs::repos::QuestionType::Essay),
            "calculation" | "计算" | "计算题" => {
                Some(crate::vfs::repos::QuestionType::Calculation)
            }
            "proof" | "证明" | "证明题" => Some(crate::vfs::repos::QuestionType::Proof),
            "true_false" | "判断" | "判断题" | "judgment" => {
                Some(crate::vfs::repos::QuestionType::TrueFalse)
            }
            "matching" | "匹配" | "匹配题" | "连线" | "连线题" => {
                Some(crate::vfs::repos::QuestionType::Matching)
            }
            "ordering" | "排序" | "排序题" => Some(crate::vfs::repos::QuestionType::Ordering),
            "numeric" | "数值" | "数值题" => Some(crate::vfs::repos::QuestionType::Numeric),
            _ => Some(crate::vfs::repos::QuestionType::Other),
        }
    }

    fn parse_difficulty(s: &str) -> Option<crate::vfs::repos::Difficulty> {
        match s.trim().to_lowercase().as_str() {
            "easy" | "简单" | "1" => Some(crate::vfs::repos::Difficulty::Easy),
            "medium" | "中等" | "2" => Some(crate::vfs::repos::Difficulty::Medium),
            "hard" | "困难" | "难" | "3" => Some(crate::vfs::repos::Difficulty::Hard),
            "very_hard" | "极难" | "4" => Some(crate::vfs::repos::Difficulty::VeryHard),
            _ => None,
        }
    }

    fn parse_tags(s: &str) -> Option<Vec<String>> {
        let s = s.trim();
        if s.starts_with('[') {
            if let Ok(tags) = serde_json::from_str::<Vec<String>>(s) {
                return Some(tags);
            }
        }
        for sep in [',', ';', '|', '、'] {
            if s.contains(sep) {
                let tags: Vec<String> = s
                    .split(sep)
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect();
                if !tags.is_empty() {
                    return Some(tags);
                }
            }
        }
        if !s.is_empty() {
            Some(vec![s.to_string()])
        } else {
            None
        }
    }

    fn parse_images(s: &str) -> Result<Option<Vec<QuestionImage>>, String> {
        let input = s.trim();
        if input.is_empty() {
            return Ok(None);
        }
        if !input.starts_with('[') {
            return Err("images 字段必须是 JSON 数组格式".to_string());
        }

        let images: Vec<QuestionImage> =
            serde_json::from_str(input).map_err(|e| format!("images 字段解析失败: {}", e))?;
        Ok(Some(images))
    }
}

enum CsvRowResult {
    Success,
    Skipped,
    Updated,
}

#[cfg(test)]
mod csv_import_tests {
    use super::{CsvDuplicateStrategy, CsvImportCancellation, CsvImportRequest, CsvImportService};
    use std::collections::HashMap;
    use std::sync::Arc;

    #[test]
    fn cancelled_csv_import_does_not_create_questions_or_an_empty_exam() {
        let (temp_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let csv_path = temp_dir.path().join("cancelled.csv");
        std::fs::write(&csv_path, "question\nfirst row\nsecond row\n").expect("write CSV fixture");

        let request = CsvImportRequest {
            file_path: csv_path.to_string_lossy().into_owned(),
            exam_id: "exam_cancelled_before_write".to_string(),
            field_mapping: HashMap::from([("question".to_string(), "content".to_string())]),
            duplicate_strategy: CsvDuplicateStrategy::Skip,
            folder_id: None,
            exam_name: Some("Cancelled import".to_string()),
        };
        let cancellation = Arc::new(CsvImportCancellation::new());
        assert!(cancellation.request_cancel());

        let result = CsvImportService::import_csv(&db, &request, None, Some(cancellation))
            .expect("cancelled import returns its partial result");

        assert!(result.cancelled);
        assert_eq!(result.success_count, 0);
        let conn = db.get_conn_safe().expect("get VFS connection");
        let question_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM questions", [], |row| row.get(0))
            .expect("count questions");
        let exam_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM exam_sheets", [], |row| row.get(0))
            .expect("count exams");
        assert_eq!(
            question_count, 0,
            "no rows may be written after cancellation"
        );
        assert_eq!(exam_count, 0, "cancellation must not create an empty exam");
    }

    #[test]
    fn cancellation_and_completion_have_a_single_terminal_winner() {
        let cancelled_first = CsvImportCancellation::new();
        assert!(cancelled_first.request_cancel());
        assert!(cancelled_first.finish_or_cancelled());

        let completed_first = CsvImportCancellation::new();
        assert!(!completed_first.finish_or_cancelled());
        assert!(
            !completed_first.request_cancel(),
            "a completed import must not accept a late cancellation"
        );
    }
}

#[cfg(test)]
mod no_questions_error_tests {
    use super::no_questions_error;

    #[test]
    fn surfaces_the_first_failure_reason_when_nothing_was_extracted() {
        assert_eq!(
            no_questions_error(
                "未能识别出题目",
                Some(" 未找到可用的 VLM 模型，请在设置中配置 ")
            ),
            "未能识别出题目：未找到可用的 VLM 模型，请在设置中配置"
        );
    }

    #[test]
    fn keeps_the_plain_message_without_a_reason() {
        assert_eq!(no_questions_error("未能提取到题目", None), "未能提取到题目");
        assert_eq!(
            no_questions_error("未能提取到题目", Some("  ")),
            "未能提取到题目"
        );
    }
}
