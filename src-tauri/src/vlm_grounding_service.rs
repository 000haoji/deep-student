//! Stage 2: VLM Grounding Service — VLM 一体化页面分析
//!
//! Visual-First 管线的核心阶段：使用视觉语言模型对试卷页面图片进行一体化分析。
//!
//! 功能：
//! 1. 题目切分 — 识别页面中所有题目的边界
//! 2. OCR — 提取每道题目的完整文本（含选项、LaTeX 公式）
//! 3. 图文关联 — 检测配图/插图并语义关联到对应题目
//! 4. 跨页续接 — 标记跨页题目的续接关系
//!
//! 所有信息来自单次 VLM 调用，天然对齐，无需跨模型匹配。

use base64::Engine;
use image::GenericImageView;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{debug, info, warn};

use crate::llm_manager::single_shot_stream::SingleShotOptions;
use crate::llm_manager::LLMManager;
use crate::models::AppError;
use crate::page_rasterizer::PageSlice;

// ============================================================================
// 数据类型
// ============================================================================

/// Typed source block, independent of whether the model produced a nonempty stem.
/// Old checkpoints default to Question; explicit unsupported kinds stay Unknown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportBlockKind {
    #[default]
    Question,
    Answer,
    Explanation,
    Heading,
    Material,
    Figure,
    #[serde(other)]
    Unknown,
}

impl ImportBlockKind {
    pub fn is_answer(self) -> bool {
        matches!(self, Self::Answer | Self::Explanation)
    }
}

/// VLM 单页分析结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VlmPageAnalysis {
    pub questions: Vec<VlmQuestion>,
}

/// VLM 识别出的文档块（包括题目、答案、标题等）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VlmQuestion {
    #[serde(default)]
    pub block_kind: ImportBlockKind,
    /// Semantic scope, shared by questions and their answers across pages.
    #[serde(default)]
    pub section: String,
    /// Stable source identity supplied by the caller when absent in model output.
    #[serde(default)]
    pub source_id: String,
    /// 题号标签（"1", "2", "5-8" 等）
    #[serde(default)]
    pub label: String,
    /// 题目区域归一化边界框 [x1, y1, x2, y2]，值域 0.0-1.0
    #[serde(default = "default_bbox")]
    pub bbox: [f64; 4],
    /// VLM OCR 的完整题目文本（含选项、LaTeX 公式）
    #[serde(default)]
    pub raw_text: String,
    /// 是否为共享配图的题组
    #[serde(default)]
    pub is_group: bool,
    /// 题组子题号
    #[serde(default)]
    pub sub_questions: Vec<String>,
    /// 该题关联的所有图片/配图
    #[serde(default)]
    pub figures: Vec<VlmFigure>,
    /// 是否接续上一页未完成的题目
    #[serde(default)]
    pub continues_from_previous: bool,
    /// 是否在本页未完成、续接到下一页
    #[serde(default)]
    pub continues_to_next: bool,
}

/// VLM 识别出的图片/配图
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmFigure {
    /// 图片区域归一化边界框 [x1, y1, x2, y2]，值域 0.0-1.0
    #[serde(default = "default_bbox")]
    pub bbox: [f64; 4],
    /// 图片标签 ("图1", "配图", "选项图" 等)
    #[serde(default)]
    pub fig_label: String,
}

fn default_bbox() -> [f64; 4] {
    [0.0, 0.0, 0.0, 0.0]
}

impl VlmQuestion {
    /// Upgrade the old textual marker to authoritative typed metadata once.
    pub fn normalize_legacy_kind(&mut self) {
        if self.raw_text.trim_start().starts_with("[答案区]") {
            self.block_kind = ImportBlockKind::Answer;
        }
        self.section = self.section.trim().to_string();
        self.label = self.label.trim().to_string();
    }
}

fn deserialize_nullable_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

fn validate_complete_response(
    finish_reason: Option<&str>,
    interrupted: Option<&str>,
) -> Result<(), AppError> {
    if let Some(reason) = interrupted {
        return Err(AppError::llm(format!(
            "VLM 响应中断，不能作为完整提取结果: {}",
            reason
        )));
    }
    match finish_reason
        .map(|reason| reason.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("stop" | "end_turn" | "stop_sequence" | "completed") => Ok(()),
        reason => Err(AppError::llm(format!(
            "VLM 响应未完整结束: {}",
            reason.unwrap_or("missing_completion_signal")
        ))),
    }
}

/// Only remove a complete outer Markdown fence. Never slice by the first/last
/// bracket: a nested options/figures array is not a complete page response.
fn parse_complete_json(content: &str) -> Result<Value, AppError> {
    let trimmed = content.trim();
    let json = if let Some(fenced) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
    {
        fenced
            .trim()
            .strip_suffix("```")
            .ok_or_else(|| AppError::llm("VLM JSON 代码块未完整闭合"))?
            .trim()
    } else {
        trimmed
    };
    serde_json::from_str(json)
        .map_err(|e| AppError::llm(format!("VLM JSON 响应不完整或无效: {}", e)))
}

fn validate_docx_input_budget(image_count: usize, text: &str) -> Result<(), AppError> {
    if image_count > DOCX_VLM_MAX_IMAGES || text.chars().count() > DOCX_VLM_MAX_TEXT_CHARS {
        return Err(AppError::validation(format!(
            "DOCX 提取批次超过完整性预算（最多 {} 张图片/{} 字符），请分批处理，不能截断后导入",
            DOCX_VLM_MAX_IMAGES, DOCX_VLM_MAX_TEXT_CHARS
        )));
    }
    Ok(())
}

// ============================================================================
// ★ P1-3 修复：DOCX 直提路径的图片请求预算
//
// 此前 extract_docx_questions_stream 把全部原始图片（不压缩、无上限）
// 塞进单次 VLM 请求，几十张手机照片即产生数十 MB 请求体（413/超时），
// 且每次重试完整重传。以下限制对齐考卷分割路径
// （llm_manager::EXAM_SEGMENT_MAX_IMAGE_BYTES / EXAM_SEGMENT_MAX_DIMENSION /
//  EXAM_SEGMENT_MAX_PAGES，私有常量故在此镜像）。
// ============================================================================

/// 单图字节上限（超过则缩放 + JPEG 重编码），对齐 EXAM_SEGMENT_MAX_IMAGE_BYTES
const DOCX_VLM_MAX_IMAGE_BYTES: usize = 1_500_000;
/// 单张源图片最大解码字节数，避免超大 data URL 在图片库内造成资源耗尽
const DOCX_VLM_MAX_SOURCE_IMAGE_BYTES: usize = 50 * 1024 * 1024;
/// 单张源图片最大像素数，限制解码后的最坏内存占用
const DOCX_VLM_MAX_SOURCE_PIXELS: u64 = 64_000_000;
/// 压缩时的最长边上限，对齐 EXAM_SEGMENT_MAX_DIMENSION
const DOCX_VLM_MAX_DIMENSION: u32 = 1_600;
/// 单次请求最多附带的图片数，对齐 EXAM_SEGMENT_MAX_PAGES
const DOCX_VLM_MAX_IMAGES: usize = 36;
const DOCX_VLM_MAX_TEXT_CHARS: usize = 8_000;
/// DOCX VLM SSE 响应的原始字节硬上限。32K 输出 token 的正常响应远低于此值；
/// 该预算用于阻断异常上游无限发送 SSE 注释、空事件或超长 JSON。
const DOCX_VLM_MAX_STREAM_BYTES: usize = 16 * 1024 * 1024;
/// 流连接建立后，连续无任何字节到达的最长等待时间。
const DOCX_VLM_STREAM_IDLE_TIMEOUT_SECS: u64 = 120;

fn account_docx_vlm_stream_bytes(total: &mut usize, incoming: usize) -> Result<(), AppError> {
    *total = total
        .checked_add(incoming)
        .filter(|next| *next <= DOCX_VLM_MAX_STREAM_BYTES)
        .ok_or_else(|| {
            AppError::llm(format!(
                "VLM DOCX 流式响应超过 {}MB 安全上限",
                DOCX_VLM_MAX_STREAM_BYTES / (1024 * 1024)
            ))
        })?;
    Ok(())
}
/// 单次请求中图片原始字节总预算（base64 后约 32MB）
const DOCX_VLM_MAX_TOTAL_IMAGE_BYTES: usize = 24 * 1024 * 1024;

#[derive(Debug)]
struct PreparedDocxImage {
    data_url: String,
    source_index: usize,
    byte_len: usize,
}

#[derive(Debug, Default)]
struct PreparedDocxImages {
    images: Vec<PreparedDocxImage>,
    rejected: Vec<(usize, String)>,
    omitted_by_count: usize,
    total_bytes: usize,
}

fn base64_decoded_len_upper_bound(encoded_len: usize) -> Option<usize> {
    encoded_len.checked_add(3)?.checked_div(4)?.checked_mul(3)
}

/// 验证并压缩单张 data URL。任何检查或压缩失败都返回错误，调用方会记录并省略该图，
/// 不允许把无法确认大小/格式的原始内容继续放入 VLM 请求。
fn prepare_docx_image(data_url: String, source_index: usize) -> Result<PreparedDocxImage, String> {
    let (header, payload) = data_url
        .split_once(',')
        .ok_or_else(|| "缺少 data URL 分隔符".to_string())?;
    if !header.starts_with("data:image/") || !header.contains(";base64") {
        return Err("不是受支持的 base64 图片 data URL".to_string());
    }

    let payload = payload.trim();
    let decoded_upper_bound = base64_decoded_len_upper_bound(payload.len())
        .ok_or_else(|| "base64 长度溢出".to_string())?;
    if decoded_upper_bound > DOCX_VLM_MAX_SOURCE_IMAGE_BYTES {
        return Err(format!(
            "源图片可能超过 {}MB 上限",
            DOCX_VLM_MAX_SOURCE_IMAGE_BYTES / (1024 * 1024)
        ));
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|e| format!("base64 解码失败: {}", e))?;
    if bytes.is_empty() {
        return Err("图片内容为空".to_string());
    }
    if bytes.len() > DOCX_VLM_MAX_SOURCE_IMAGE_BYTES {
        return Err(format!(
            "源图片超过 {}MB 上限",
            DOCX_VLM_MAX_SOURCE_IMAGE_BYTES / (1024 * 1024)
        ));
    }

    let (width, height) = image::io::Reader::new(std::io::Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| format!("无法识别图片格式: {}", e))?
        .into_dimensions()
        .map_err(|e| format!("无法读取图片尺寸: {}", e))?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "图片像素数溢出".to_string())?;
    if width == 0 || height == 0 || pixels > DOCX_VLM_MAX_SOURCE_PIXELS {
        return Err(format!("图片尺寸 {}x{} 超出安全上限", width, height));
    }

    if bytes.len() <= DOCX_VLM_MAX_IMAGE_BYTES
        && width <= DOCX_VLM_MAX_DIMENSION
        && height <= DOCX_VLM_MAX_DIMENSION
    {
        return Ok(PreparedDocxImage {
            data_url,
            source_index,
            byte_len: bytes.len(),
        });
    }

    let image = image::load_from_memory(&bytes).map_err(|e| format!("图片解码失败: {}", e))?;
    let resized = image.resize(
        DOCX_VLM_MAX_DIMENSION,
        DOCX_VLM_MAX_DIMENSION,
        image::imageops::FilterType::Triangle,
    );

    for quality in [85, 75, 65, 55] {
        let mut cursor = std::io::Cursor::new(Vec::new());
        resized
            .write_to(&mut cursor, image::ImageOutputFormat::Jpeg(quality))
            .map_err(|e| format!("JPEG 压缩失败: {}", e))?;
        let out = cursor.into_inner();
        if out.len() <= DOCX_VLM_MAX_IMAGE_BYTES {
            info!(
                "[VLM-Grounding] DOCX 图片压缩: {:.1}MB -> {:.1}MB (quality={})",
                bytes.len() as f64 / (1024.0 * 1024.0),
                out.len() as f64 / (1024.0 * 1024.0),
                quality
            );
            return Ok(PreparedDocxImage {
                data_url: format!(
                    "data:image/jpeg;base64,{}",
                    base64::engine::general_purpose::STANDARD.encode(&out)
                ),
                source_index,
                byte_len: out.len(),
            });
        }
    }

    Err(format!(
        "压缩后仍超过 {}KB 上限",
        DOCX_VLM_MAX_IMAGE_BYTES / 1024
    ))
}

#[cfg(test)]
fn prepare_docx_images(
    candidate_urls: Vec<String>,
    total_input_count: usize,
) -> PreparedDocxImages {
    let mut prepared = PreparedDocxImages {
        omitted_by_count: total_input_count.saturating_sub(candidate_urls.len()),
        ..PreparedDocxImages::default()
    };

    for (source_index, data_url) in candidate_urls.into_iter().enumerate() {
        match prepare_docx_image(data_url, source_index) {
            Ok(image) => {
                let Some(next_total) = prepared.total_bytes.checked_add(image.byte_len) else {
                    prepared
                        .rejected
                        .push((source_index, "图片总字节数溢出".to_string()));
                    continue;
                };
                if next_total > DOCX_VLM_MAX_TOTAL_IMAGE_BYTES {
                    prepared.rejected.push((
                        source_index,
                        format!(
                            "加入后超过整批 {}MB 图片预算",
                            DOCX_VLM_MAX_TOTAL_IMAGE_BYTES / (1024 * 1024)
                        ),
                    ));
                    continue;
                }
                prepared.total_bytes = next_total;
                prepared.images.push(image);
            }
            Err(reason) => prepared.rejected.push((source_index, reason)),
        }
    }

    prepared
}

fn remap_docx_question_image_indices(
    question: &mut VlmExtractedQuestion,
    source_indices: &[usize],
) {
    question.image_indices = question
        .image_indices
        .iter()
        .filter_map(|prepared_index| source_indices.get(*prepared_index).copied())
        .collect();
}

// ============================================================================
// DOCX 图文混合 VLM 直提类型
// ============================================================================

/// VLM 从图文混合 DOCX 直接提取的 typed 文档块。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VlmExtractedQuestion {
    #[serde(default)]
    pub block_kind: ImportBlockKind,
    #[serde(default)]
    pub section: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub source_id: String,
    #[serde(default)]
    pub raw_text: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub question_type: String,
    #[serde(default)]
    pub options: Vec<VlmExtractedOption>,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub answer: String,
    #[serde(default, deserialize_with = "deserialize_nullable_string")]
    pub explanation: String,
    /// 关联的图片索引（对应输入图片数组的下标）
    #[serde(default)]
    pub image_indices: Vec<usize>,
    #[serde(default)]
    pub difficulty: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VlmExtractedOption {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub content: String,
}

// ============================================================================
// Service
// ============================================================================

pub struct VlmGroundingService {
    llm_manager: Arc<LLMManager>,
}

impl VlmGroundingService {
    pub fn new(llm_manager: Arc<LLMManager>) -> Self {
        Self { llm_manager }
    }

    /// 分析单个页面（通过 blob_hash 从 VFS 加载图片）
    ///
    /// 当前管线以串行方式逐页调用（保证 checkpoint 顺序一致性）。
    /// 未来如需并发，应在调用方使用 `Semaphore` 控制并发数，
    /// 并改造 checkpoint 逻辑以支持乱序完成。
    pub async fn analyze_page_by_blob(
        &self,
        vfs_db: &crate::vfs::database::VfsDatabase,
        page: &PageSlice,
    ) -> Result<VlmPageAnalysis, AppError> {
        let image_bytes = crate::page_rasterizer::load_page_image_bytes(vfs_db, &page.blob_hash)?;
        let (mime, _) = detect_image_format(&image_bytes);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&image_bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);

        let mut analysis = self
            .analyze_single_page(&data_url, page.text_hint.as_deref())
            .await?;
        for (index, block) in analysis.questions.iter_mut().enumerate() {
            // Model-chosen IDs are not trustworthy or stable across retries.
            block.source_id = format!(
                "page:{}:{}:block:{}",
                page.page_index, page.blob_hash, index
            );
            block.normalize_legacy_kind();
        }
        Ok(analysis)
    }

    /// 分析单个页面图片
    ///
    /// `image_data_url` 为 `data:image/jpeg;base64,...` 格式。
    /// `text_hint` 为可选的机器提取文本层（辅助 VLM 识别模糊内容）。
    pub async fn analyze_single_page(
        &self,
        image_data_url: &str,
        text_hint: Option<&str>,
    ) -> Result<VlmPageAnalysis, AppError> {
        let candidates = self.get_vlm_config_candidates().await;
        if candidates.is_empty() {
            return Err(AppError::configuration(
                "未找到可用的 VLM 模型（需要 GLM-4.6V / Qwen-VL 等多模态模型），请在设置中配置",
            ));
        }

        let mut last_error: Option<AppError> = None;
        for config in &candidates {
            match self
                .analyze_single_page_with_config(config, image_data_url, text_hint)
                .await
            {
                Ok(analysis) => {
                    if last_error.is_some() {
                        info!(
                            "[VLM-Grounding] 切换到 {} ({}) 后成功",
                            config.model, config.name
                        );
                    }
                    return Ok(analysis);
                }
                Err(e) => {
                    warn!(
                        "[VLM-Grounding] 模型 {} ({}) 分析失败: {}，尝试下一个候选",
                        config.model, config.name, e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| AppError::configuration("所有 VLM 候选模型均失败，请检查模型配置")))
    }

    /// 使用指定配置执行单页分析（候选循环的内部实现）
    async fn analyze_single_page_with_config(
        &self,
        config: &crate::llm_manager::ApiConfig,
        image_data_url: &str,
        text_hint: Option<&str>,
    ) -> Result<VlmPageAnalysis, AppError> {
        let api_key = self
            .llm_manager
            .decrypt_api_key_if_needed(&config.api_key)?;

        let prompt = Self::build_analysis_prompt(text_hint);

        let messages = vec![json!({
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": image_data_url, "detail": "high" } },
                { "type": "text", "text": prompt }
            ]
        })];

        let max_tokens = crate::llm_manager::effective_max_tokens(
            config.max_output_tokens,
            config.max_tokens_limit,
        )
        .max(4096)
        .min(16384);

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "temperature": 0.1,
            "max_tokens": max_tokens,
        });

        // GLM-4.5+ 支持 thinking 参数；OCR/题目集默认关闭以降低延迟
        if crate::llm_manager::adapters::zhipu::ZhipuAdapter::supports_thinking_static(
            &config.model,
        ) {
            let enable = self.llm_manager.is_ocr_thinking_enabled();
            if let Some(obj) = request_body.as_object_mut() {
                obj.insert(
                    "thinking".to_string(),
                    json!({ "type": if enable { "enabled" } else { "disabled" } }),
                );
            }
        }

        // 流式传输 + 空闲超时：稠密试卷页输出数千 token（或推理模型先思考）时，
        // 非流式请求会被网关（Cloudflare ~100s → 524 / nginx 60s → 504）或固定
        // 总超时掐断后整页重来；流式只要还在出字节就不会被判超时。
        let opts = SingleShotOptions::new(format!("VLM-Grounding({})", config.model));
        info!(
            "[VLM-Grounding] 发送分析请求 (streaming): model={}, base_url={}",
            config.model,
            crate::llm_manager::sanitize_url_for_log(&config.base_url)
        );
        let completion = self
            .llm_manager
            .single_shot_completion(
                config,
                &request_body,
                Some(&api_key),
                None,
                "VLM 请求构建失败",
                &opts,
                None,
            )
            .await?;

        if completion.aborted {
            return Err(AppError::llm("VLM 响应已取消，不能作为完整提取结果"));
        }
        validate_complete_response(
            completion.finish_reason.as_deref(),
            completion.interrupted.as_deref(),
        )?;
        info!(
            "[VLM-Grounding] 收到响应: {} 字符 (attempts={}, stream={})",
            completion.content.len(),
            completion.attempts,
            completion.mode.is_stream()
        );

        Self::parse_vlm_response(&completion.content)
    }

    /// 描述单张图片内容（轻量 VLM 调用）
    ///
    /// 用于 DOCX 原生导入：对文档中嵌入的配图/示意图/图表进行文字描述，
    /// 描述会嵌入到题目文本中供 LLM 结构化时理解图片含义。
    pub async fn describe_image(&self, image_bytes: &[u8]) -> Result<String, AppError> {
        let (mime, _) = detect_image_format(image_bytes);
        let b64 = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        let data_url = format!("data:{};base64,{}", mime, b64);

        let candidates = self.get_vlm_config_candidates().await;
        if candidates.is_empty() {
            return Err(AppError::configuration(
                "未找到可用的 VLM 模型（需要 GLM-4.6V / Qwen-VL 等多模态模型），请在设置中配置",
            ));
        }

        let mut last_error: Option<AppError> = None;
        for config in &candidates {
            match self.describe_image_with_config(config, &data_url).await {
                Ok(description) => {
                    if last_error.is_some() {
                        info!(
                            "[VLM-Grounding] 图片描述切换到 {} ({}) 后成功",
                            config.model, config.name
                        );
                    }
                    return Ok(description);
                }
                Err(e) => {
                    warn!(
                        "[VLM-Grounding] 图片描述模型 {} ({}) 失败: {}，尝试下一个候选",
                        config.model, config.name, e
                    );
                    last_error = Some(e);
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| AppError::configuration("所有 VLM 候选模型均失败，请检查模型配置")))
    }

    /// 使用指定配置描述单张图片（候选循环的内部实现）
    async fn describe_image_with_config(
        &self,
        config: &crate::llm_manager::ApiConfig,
        data_url: &str,
    ) -> Result<String, AppError> {
        let api_key = self
            .llm_manager
            .decrypt_api_key_if_needed(&config.api_key)?;

        let prompt = format!(
            "{}\n\n{}",
            r#"请详细描述这张图片的内容。这是一份试题/学习材料中的配图。

要求：
1. 如果是数学/物理/化学等图形，精确描述图中的坐标、标注、数值、方程
2. 如果是表格，用文字或 Markdown 表格转录完整内容
3. 如果是示意图/流程图，描述各部分的关系和标注
4. 如果包含文字，完整转录；公式格式遵循下方的 LaTeX 公式契约
5. 只输出描述，不要其他多余内容"#,
            crate::prompt_contracts::LATEX_OUTPUT_BRIEF
        );

        let messages = vec![json!({
            "role": "user",
            "content": [
                { "type": "image_url", "image_url": { "url": data_url, "detail": "high" } },
                { "type": "text", "text": prompt }
            ]
        })];

        let max_tokens = crate::llm_manager::effective_max_tokens(
            config.max_output_tokens,
            config.max_tokens_limit,
        )
        .max(2048)
        .min(4096);

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "temperature": 0.1,
            "max_tokens": max_tokens,
        });

        if crate::llm_manager::adapters::zhipu::ZhipuAdapter::supports_thinking_static(
            &config.model,
        ) {
            let enable = self.llm_manager.is_ocr_thinking_enabled();
            if let Some(obj) = request_body.as_object_mut() {
                obj.insert(
                    "thinking".to_string(),
                    json!({ "type": if enable { "enabled" } else { "disabled" } }),
                );
            }
        }

        let opts =
            SingleShotOptions::new(format!("VLM-Describe({})", config.model)).with_max_retries(1);
        let completion = self
            .llm_manager
            .single_shot_completion(
                config,
                &request_body,
                Some(&api_key),
                None,
                "VLM 图片描述请求构建失败",
                &opts,
                None,
            )
            .await?;

        Ok(completion.content.trim().to_string())
    }

    /// 从 DOCX 图文混合文档中直接提取题目（流式回调）
    ///
    /// 调用方先按完整性预算分批，将本批图片和原文交给 VLM 分类。
    /// 模型建议的图文关系仍需调用方校验，不允许因请求预算省略源内容。
    ///
    /// HTTP 流式传输；整批 JSON 与结束原因验证通过后才逐块回调。
    /// 不完整响应不会向调用方交付可发布题目。
    pub async fn extract_docx_questions_stream(
        &self,
        image_data_urls: &[String],
        extracted_text: &str,
        mut on_question: impl FnMut(VlmExtractedQuestion) -> bool,
    ) -> Result<usize, AppError> {
        if image_data_urls.is_empty() {
            return Err(AppError::validation("图片列表为空"));
        }
        validate_docx_input_budget(image_data_urls.len(), extracted_text)?;

        let config = self.get_vlm_config().await?;
        let api_key = self
            .llm_manager
            .decrypt_api_key_if_needed(&config.api_key)?;

        // 按数量预算逐张进入 blocking 线程。每轮只克隆一张已通过编码长度预检的图片，
        // 避免先复制 36 个大 data URL 形成数 GB 瞬时峰值。
        let total_input_count = image_data_urls.len();
        let mut prepared = PreparedDocxImages {
            omitted_by_count: total_input_count.saturating_sub(DOCX_VLM_MAX_IMAGES),
            ..PreparedDocxImages::default()
        };
        for (source_index, data_url) in image_data_urls.iter().take(DOCX_VLM_MAX_IMAGES).enumerate()
        {
            let payload = match data_url.split_once(',') {
                Some((_, payload)) => payload.trim(),
                None => {
                    prepared
                        .rejected
                        .push((source_index, "缺少 data URL 分隔符".to_string()));
                    continue;
                }
            };
            let decoded_upper_bound = match base64_decoded_len_upper_bound(payload.len()) {
                Some(size) => size,
                None => {
                    prepared
                        .rejected
                        .push((source_index, "base64 长度溢出".to_string()));
                    continue;
                }
            };
            if decoded_upper_bound > DOCX_VLM_MAX_SOURCE_IMAGE_BYTES {
                prepared.rejected.push((
                    source_index,
                    format!(
                        "源图片可能超过 {}MB 上限",
                        DOCX_VLM_MAX_SOURCE_IMAGE_BYTES / (1024 * 1024)
                    ),
                ));
                continue;
            }

            let owned_data_url = data_url.clone();
            let result = tokio::task::spawn_blocking(move || {
                prepare_docx_image(owned_data_url, source_index)
            })
            .await
            .map_err(|e| AppError::internal(format!("图片压缩任务失败: {}", e)))?;
            match result {
                Ok(image) => {
                    let Some(next_total) = prepared.total_bytes.checked_add(image.byte_len) else {
                        prepared
                            .rejected
                            .push((source_index, "图片总字节数溢出".to_string()));
                        continue;
                    };
                    if next_total > DOCX_VLM_MAX_TOTAL_IMAGE_BYTES {
                        prepared.rejected.push((
                            source_index,
                            format!(
                                "加入后超过整批 {}MB 图片预算",
                                DOCX_VLM_MAX_TOTAL_IMAGE_BYTES / (1024 * 1024)
                            ),
                        ));
                        continue;
                    }
                    prepared.total_bytes = next_total;
                    prepared.images.push(image);
                }
                Err(reason) => prepared.rejected.push((source_index, reason)),
            }
        }

        for (source_index, reason) in &prepared.rejected {
            warn!(
                "[VLM-Grounding] 省略 DOCX 图片 {}: {}",
                source_index, reason
            );
        }
        if prepared.omitted_by_count > 0 {
            warn!(
                "[VLM-Grounding] DOCX 图片数 {} 超过单请求上限 {}，未处理后续 {} 张图片",
                total_input_count, DOCX_VLM_MAX_IMAGES, prepared.omitted_by_count
            );
        }
        if !prepared.rejected.is_empty() || prepared.omitted_by_count > 0 {
            return Err(AppError::validation(format!(
                "DOCX 提取批次有 {} 张图片未完整发送（安全检查或请求预算），请缩小批次或修复图片后重试",
                prepared.rejected.len() + prepared.omitted_by_count
            )));
        }
        if prepared.images.is_empty() {
            return Err(AppError::validation(
                "DOCX 中没有通过安全检查且可发送给 VLM 的图片",
            ));
        }

        let source_indices: Vec<usize> = prepared
            .images
            .iter()
            .map(|image| image.source_index)
            .collect();

        // 构建多图 + 文本的 content 数组
        let mut content_parts: Vec<Value> = Vec::with_capacity(prepared.images.len() * 2 + 2);

        for (idx, image) in prepared.images.iter().enumerate() {
            // 在每张图片前加文本标注，帮助 VLM 建立索引
            content_parts.push(json!({
                "type": "text",
                "text": format!("【图片{}】", idx)
            }));
            content_parts.push(json!({
                "type": "image_url",
                "image_url": { "url": image.data_url, "detail": "high" }
            }));
        }

        // 构建提取指令
        let prompt = Self::build_docx_extraction_prompt(prepared.images.len(), extracted_text);
        content_parts.push(json!({
            "type": "text",
            "text": prompt
        }));

        let messages = vec![json!({
            "role": "user",
            "content": content_parts
        })];

        let max_tokens = crate::llm_manager::effective_max_tokens(
            config.max_output_tokens,
            config.max_tokens_limit,
        )
        .max(8192)
        .min(32768);

        let mut request_body = json!({
            "model": config.model,
            "messages": messages,
            "temperature": 0.1,
            "max_tokens": max_tokens,
            "stream": true,
        });

        if crate::llm_manager::adapters::zhipu::ZhipuAdapter::supports_thinking_static(
            &config.model,
        ) {
            let enable = self.llm_manager.is_ocr_thinking_enabled();
            if let Some(obj) = request_body.as_object_mut() {
                obj.insert(
                    "thinking".to_string(),
                    json!({ "type": if enable { "enabled" } else { "disabled" } }),
                );
            }
        }

        // Keep HTTP transport streaming, but do not publish individual objects until
        // the whole batch and its provider finish reason pass integrity checks.
        let mut opts = SingleShotOptions::new(format!("VLM-DOCX({})", config.model));
        opts.max_stream_bytes = DOCX_VLM_MAX_STREAM_BYTES;
        opts.idle_timeout = std::time::Duration::from_secs(DOCX_VLM_STREAM_IDLE_TIMEOUT_SECS);
        let completion = self
            .llm_manager
            .single_shot_completion(
                &config,
                &request_body,
                Some(&api_key),
                None,
                "VLM DOCX 提取请求构建失败",
                &opts,
                None,
            )
            .await?;
        if completion.aborted {
            return Err(AppError::llm("VLM 响应已取消，不能作为完整提取结果"));
        }
        validate_complete_response(
            completion.finish_reason.as_deref(),
            completion.interrupted.as_deref(),
        )?;
        let mut response_bytes = 0;
        account_docx_vlm_stream_bytes(&mut response_bytes, completion.content.len())?;
        let questions = Self::parse_docx_extraction_response(&completion.content)?;
        let mut count = 0;
        for mut question in questions {
            remap_docx_question_image_indices(&mut question, &source_indices);
            count += 1;
            if !on_question(question) {
                break;
            }
        }
        Ok(count)
    }

    /// 构建 DOCX 图文提取的 VLM prompt
    fn build_docx_extraction_prompt(image_count: usize, extracted_text: &str) -> String {
        format!(
            r#"你是文档原文提取器，不是解题助手。本批次有 {image_count} 张图片（图片0起）及以下完整原文：
---
{extracted_text}
---
识别所有文档块，保留原文顺序；只输出完整 JSON 数组，不输出其他内容。
每个块的字段：
{{"block_kind":"question|answer|explanation|heading|material|figure|unknown",
  "section":"chapter16/single_choice", "label":"1", "source_id":"",
  "raw_text":"当前块的逐字原文，保留原题号、公式和数学符号",
  "content":"题干（非题目块可为空）", "question_type":"single_choice",
  "options":[{{"key":"A","content":"选项原文"}}],
  "answer":null, "explanation":null, "image_indices":[0],
  "difficulty":"medium", "tags":[]}}

强制规则：
1. block_kind 是语义分类而非看 content 是否为空。参考答案、评分要点及带题目摘要的答案仍是 answer；解析是 explanation；标题是 heading；公共材料是 material；独立图片是 figure；无法判断是 unknown。禁止将答案摘要生成新题。
2. section 必须来自实际标题/明确语义作用域，如 single_choice、term_definition、short_answer；有章节时使用 chapter16/single_choice 这样的路径。题目区与对应答案区使用相同作用域，跨页可继承有证据的标题；不得仅凭数字重号猜测，不确定就空字符串。
3. label 仅填写原题号/子题号，不得按本批次重新编号；source_id 若输入中已有则原样保留，否则空字符串由后端分配。raw_text 必须是来源原文，不能改写、总结或补充。
4. 严禁推理作答、补答案、补评分要点或从常识生成解析。仅转录当前块明确存在的答案/解析，题目块未见答案时 answer/explanation 必须 null。题号相同不代表可以把别处文字偷偷并入此块。
5. answer/explanation 块保留原文并填写相应字段；没有原题号或section也必须保留为相应类型，不得伪造题号或题干。
6. question_type: single_choice/multiple_choice/indefinite_choice/fill_blank/short_answer/calculation/proof/essay/other；只有 question 才填题型。选项放 options，不混入题干；其他块 options=[]。
7. image_indices 只能引用本次实际显示的图片，不得猜测未见图片。题组共享图片和材料必须保留为对应块，不得改写为独立题。
8. 不要遗漏或合并块；输出必须完整闭合。公式格式：{latex_brief}
{difficulty_rubric}"#,
            latex_brief = crate::prompt_contracts::LATEX_OUTPUT_BRIEF,
            difficulty_rubric = crate::prompt_contracts::DIFFICULTY_RUBRIC,
        )
    }

    /// 解析 DOCX 图文提取的 VLM 响应
    fn parse_docx_extraction_response(
        content: &str,
    ) -> Result<Vec<VlmExtractedQuestion>, AppError> {
        let value = parse_complete_json(content)?;
        let blocks = if value.is_array() {
            value
        } else {
            value
                .get("questions")
                .or_else(|| value.get("blocks"))
                .cloned()
                .ok_or_else(|| AppError::llm("VLM 响应缺少完整文档块数组"))?
        };
        let mut questions: Vec<VlmExtractedQuestion> = serde_json::from_value(blocks)
            .map_err(|e| AppError::llm(format!("VLM 文档块字段无法解析: {}", e)))?;
        for question in &mut questions {
            if question.raw_text.trim_start().starts_with("[答案区]")
                || question.content.trim_start().starts_with("[答案区]")
            {
                question.block_kind = ImportBlockKind::Answer;
            }
            question.section = question.section.trim().to_string();
            question.label = question.label.trim().to_string();
        }
        Ok(questions)
    }

    /// 检测是否有可用的 VLM 模型
    pub async fn is_available(llm_manager: &Arc<LLMManager>) -> bool {
        let configs = match llm_manager.get_api_configs().await {
            Ok(c) => c,
            Err(_) => return false,
        };
        configs.iter().any(|c| c.enabled && c.is_multimodal)
    }

    /// 从 VLM 响应文本中提取结构化分析结果
    fn parse_vlm_response(content: &str) -> Result<VlmPageAnalysis, AppError> {
        let value = parse_complete_json(content)?;
        let blocks = if value.is_array() {
            value
        } else {
            value
                .get("questions")
                .or_else(|| value.get("blocks"))
                .cloned()
                .ok_or_else(|| AppError::llm("VLM 响应缺少完整文档块数组"))?
        };
        let mut questions: Vec<VlmQuestion> = serde_json::from_value(blocks)
            .map_err(|e| AppError::llm(format!("VLM 页面块字段无法解析: {}", e)))?;
        for question in &mut questions {
            question.normalize_legacy_kind();
        }
        Ok(VlmPageAnalysis { questions })
    }

    /// 构建 VLM 分析 prompt
    ///
    /// 当 `text_hint` 可用时，在 prompt 中附加文本层作为参考，
    /// 提高小字体 / 模糊公式的识别准确率。
    fn build_analysis_prompt(text_hint: Option<&str>) -> String {
        let hint = text_hint
            .unwrap_or("")
            .chars()
            .take(1500)
            .collect::<String>();
        format!(
            r#"请逐字提取此页面的所有文档块及位置，不要解题或生成答案。
机器文本仅辅助识别，以图片原文为准：
---
{hint}
---
只输出完整 JSON 数组，每个元素表示原页面上的一个块：
[{{"block_kind":"question|answer|explanation|heading|material|figure|unknown",
   "section":"chapter16/single_choice", "label":"1", "source_id":"",
   "bbox":[0.05,0.02,0.95,0.17], "raw_text":"逐字原文（保留题号/选项/公式）",
   "is_group":false,"sub_questions":[],
   "figures":[{{"bbox":[0.60,0.03,0.92,0.15],"fig_label":"配图"}}],
   "continues_from_previous":false,"continues_to_next":false}}]

强制规则：
1. 明确区分 question（题目）、answer（参考答案/评分要点）、explanation（解析）、heading（标题）、material（公共材料）、figure（独立图）、unknown（无法识别）。答案即使重复题干摘要也仍是 answer，不能改成新题。旧 [答案区] 标记一律表示 answer。
2. section 来自实际章节与题型标题，如 chapter16/single_choice、chapter16/term_definition、chapter16/short_answer。没有章节时可用 single_choice/term_definition/short_answer。题目和对应答案使用同一语义作用域；缺少标题证据就留空，禁止只凭数字猜测跨页作用域。
3. label 是原题号/子题号，禁止重新编号；无题号留空。source_id 留空由后端绑定原页。标题、公共材料、答案必须单独输出，不能偷偷并入无关题目。
4. raw_text 逐字保留当前块原文，不得总结/改写、补漏或生成新内容。严禁推理补答案、凭常识作答、补解析；页面没写的答案不能出现在提取结果。
5. bbox 为当前块在本页的位置，[x1,y1,x2,y2]归一化0..1；figures只列真实配图而非页眉、水印。
6. 同一题跨页仅在原文确有连续关系时分别标 continues_to_next/continues_from_previous。答案区起始不自动视为上一题续接；跨kind不能续接。
7. 题组材料与子题分别保留原题号及内容，不得因共享图片而合并多道题。参考答案每条单独保留原题号、section与原文，后端只在身份唯一时关联。
8. 完整提取所有块，不能因为输出预算而省略后半页。公式格式：{latex_brief}"#,
            latex_brief = crate::prompt_contracts::LATEX_OUTPUT_BRIEF,
        )
    }

    /// 获取 VLM 模型配置
    ///
    /// 优先级（从高到低）：
    /// 1. GLM-4.6V+ / GLM-5.xV（大尺寸 VLM，非 Thinking 变体）
    /// 2. GLM-4.5V（非 Thinking 变体）
    /// 3. Qwen3-VL
    /// 4. Qwen2.5-VL
    /// 5. 其他 Qwen-VL
    /// 6. GLM-4.xV 兜底（包含 Thinking/小参数等）
    /// 7. 任意多模态模型
    /// 获取 VLM 候选配置列表（按优先级排序，供故障切换使用）
    ///
    /// 与旧 `get_vlm_config` 的选择逻辑保持一致，但不再只返回第一个匹配，
    /// 而是返回所有可用候选。调用方逐个尝试，失败时自动切换到下一个模型，
    /// 避免单一模型（如已停服的 GLM-4.6V）失败导致整体 OCR 不可用。
    pub(crate) async fn get_vlm_config_candidates(&self) -> Vec<crate::llm_manager::ApiConfig> {
        let configs = self.llm_manager.get_api_configs().await.unwrap_or_default();

        // 黑名单：GLM-4.1V / GLM-4.0V / GLM-4V- 质量差，即使在 OCR 引擎中也跳过
        let is_blacklisted = |model: &str| {
            let lower = model.to_lowercase();
            lower.contains("glm-4.1v") || lower.contains("glm-4.0v") || lower.contains("glm-4v-")
        };

        let mut result: Vec<crate::llm_manager::ApiConfig> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut push = |config: &crate::llm_manager::ApiConfig, tier: &str| {
            if is_blacklisted(&config.model) {
                info!(
                    "[VLM-Grounding] 跳过黑名单模型: {} ({})",
                    config.model, config.name
                );
                return;
            }
            if crate::llm_manager::is_discontinued_ocr_model(config) {
                info!(
                    "[VLM-Grounding] 跳过已停服模型: {} ({})",
                    config.model, config.name
                );
                return;
            }
            if !config.enabled || !config.is_multimodal {
                return;
            }
            if seen.insert(config.id.clone()) {
                info!(
                    "[VLM-Grounding] 候选 ({}): {} ({})",
                    tier, config.model, config.name
                );
                result.push(config.clone());
            }
        };

        // ===== Tier 0: 从 OCR 引擎配置中查找 GLM 视觉模型 =====
        // OCR 引擎（ocr.available_models）和普通模型列表是两套独立存储，
        // 一键分配会把 GLM-4.6V 加到 OCR 引擎但不一定在普通模型列表中。
        let ocr_models = self.llm_manager.get_available_ocr_models().await;
        // 按 priority 排序，优先使用主引擎
        let mut ocr_glm: Vec<&crate::llm_manager::OcrModelConfig> = ocr_models
            .iter()
            .filter(|m| m.enabled && m.engine_type == "glm4v_ocr")
            .collect();
        ocr_glm.sort_by_key(|m| m.priority);

        for ocr_cfg in &ocr_glm {
            if let Some(config) = configs.iter().find(|c| c.id == ocr_cfg.config_id) {
                push(config, "Tier0-GLM");
            }
        }

        // 也检查 OCR 引擎中的 Qwen-VL / GenericVlm（按 priority 排序）
        let mut ocr_vlm: Vec<&crate::llm_manager::OcrModelConfig> = ocr_models
            .iter()
            .filter(|m| {
                m.enabled
                    && matches!(
                        m.engine_type.as_str(),
                        "generic_vlm" | "paddle_ocr_vl" | "paddle_ocr_vl_v1"
                    )
            })
            .collect();
        ocr_vlm.sort_by_key(|m| m.priority);

        // ===== Tier 1-5: 从普通模型列表中按名称匹配 =====

        // GLM-4.6V+ 或 GLM-5.xV（排除 thinking 小模型）
        let glm_top_tier =
            regex::Regex::new(r"(?i)glm-(?:4\.[6-9]|4\.\d{2,}|5(?:\.\d+)?)v").unwrap();
        // GLM-4.5V（非 thinking）
        let glm_4_5 = regex::Regex::new(r"(?i)glm-4\.5v").unwrap();
        let is_thinking = |m: &str| m.contains("thinking");

        let vlm_model_priorities: Vec<Box<dyn Fn(&str) -> bool>> = vec![
            // Tier 1: GLM-4.6V+ / GLM-5.xV（非 thinking）
            Box::new(move |m: &str| glm_top_tier.is_match(m) && !is_thinking(m)),
            // Tier 2: GLM-4.5V（非 thinking）
            Box::new(move |m: &str| glm_4_5.is_match(m) && !is_thinking(m)),
            // Tier 3: Qwen3-VL
            Box::new(|m: &str| m.contains("qwen3-vl")),
            // Tier 4: Qwen2.5-VL
            Box::new(|m: &str| m.contains("qwen2.5-vl")),
            // Tier 5: 其他 Qwen-VL
            Box::new(|m: &str| m.contains("qwen-vl")),
        ];

        for matcher in &vlm_model_priorities {
            for config in configs.iter().filter(|c| matcher(&c.model.to_lowercase())) {
                push(config, "name-match");
            }
        }

        // ===== Tier 6: OCR 引擎中的通用 VLM 模型 =====
        for ocr_cfg in &ocr_vlm {
            if let Some(config) = configs.iter().find(|c| c.id == ocr_cfg.config_id) {
                push(config, "Tier6-OCR-VLM");
            }
        }

        // ===== Tier 7: 任意多模态模型 =====
        for config in configs.iter() {
            push(config, "Tier7-any-multimodal");
        }

        // ★ PR #490 §4.7：静态能力筛查 + 健康排序（冷却候选降到队尾，不删除）。
        // 健康状态只来自 single_shot_completion 的真实授权请求结果；本函数不发探活请求。
        let (result, _rejected) = crate::llm_manager::model_health::model_health()
            .prepare_candidates(result, std::time::Instant::now());

        debug!(
            "[VLM-Grounding] VLM 候选模型: {}",
            result
                .iter()
                .map(|c| format!("{}({})", c.model, c.name))
                .collect::<Vec<_>>()
                .join(" → ")
        );

        result
    }

    pub(crate) async fn get_vlm_config(&self) -> Result<crate::llm_manager::ApiConfig, AppError> {
        self.get_vlm_config_candidates()
            .await
            .into_iter()
            .next()
            .ok_or_else(|| {
                AppError::configuration(
                    "未找到可用的 VLM 模型（需要 GLM-4.6V / Qwen-VL 等多模态模型），请在设置中配置",
                )
            })
    }

    /// 按归一化坐标从页面图片中裁切配图区域
    pub fn crop_figure_from_page(
        page_image_bytes: &[u8],
        figure_bbox: &[f64; 4],
    ) -> Result<Vec<u8>, AppError> {
        let img = image::load_from_memory(page_image_bytes)
            .map_err(|e| AppError::internal(format!("加载页面图片失败: {}", e)))?;

        let (img_w, img_h) = img.dimensions();

        let x1 = figure_bbox[0].min(figure_bbox[2]).clamp(0.0, 1.0);
        let y1 = figure_bbox[1].min(figure_bbox[3]).clamp(0.0, 1.0);
        let x2 = figure_bbox[0].max(figure_bbox[2]).clamp(0.0, 1.0);
        let y2 = figure_bbox[1].max(figure_bbox[3]).clamp(0.0, 1.0);

        let px = (x1 * img_w as f64).round() as u32;
        let py = (y1 * img_h as f64).round() as u32;
        let pw = ((x2 - x1) * img_w as f64).round().max(1.0) as u32;
        let ph = ((y2 - y1) * img_h as f64).round().max(1.0) as u32;

        let px = px.min(img_w.saturating_sub(1));
        let py = py.min(img_h.saturating_sub(1));
        let pw = pw.min(img_w - px);
        let ph = ph.min(img_h - py);

        if pw == 0 || ph == 0 {
            return Err(AppError::validation("裁切区域无效：宽度或高度为 0"));
        }

        let cropped = image::imageops::crop_imm(&img, px, py, pw, ph).to_image();

        let mut buffer = std::io::Cursor::new(Vec::new());
        cropped
            .write_to(&mut buffer, image::ImageOutputFormat::Png)
            .map_err(|e| AppError::internal(format!("编码裁切图片失败: {}", e)))?;

        Ok(buffer.into_inner())
    }
}

/// 按字节上限截断字符串，并保证落在 UTF-8 字符边界上。
///
/// ★ 2026-06-12（代理 3 审阅 F3）：旧代码多处 `&body[..body.len().min(N)]`
/// 直接按字节切片，中文错误消息/响应在非字符边界会 panic。
#[cfg(test)]
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
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

#[cfg(test)]
mod tests {
    use super::*;

    fn png_data_url(width: u32, height: u32) -> String {
        let image = image::RgbImage::new(width, height);
        let mut cursor = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(image)
            .write_to(&mut cursor, image::ImageOutputFormat::Png)
            .unwrap();
        format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(cursor.into_inner())
        )
    }

    #[test]
    fn old_checkpoints_default_to_questions_but_legacy_answers_upgrade() {
        let legacy = VlmGroundingService::parse_vlm_response(
            r#"[{"label":"1","raw_text":"原题"},{"label":"1","raw_text":"[答案区] B"}]"#,
        )
        .unwrap();
        assert_eq!(legacy.questions[0].block_kind, ImportBlockKind::Question);
        assert_eq!(legacy.questions[1].block_kind, ImportBlockKind::Answer);
        assert_eq!(
            serde_json::to_value(ImportBlockKind::Answer).unwrap(),
            json!("answer")
        );
    }

    #[test]
    fn wrapped_page_response_keeps_nested_figure_arrays() {
        let response = r#"{"questions":[{"label":"1","block_kind":"question","section":"single_choice","raw_text":"原题","figures":[{"bbox":[0.1,0.2,0.3,0.4]}]}]}"#;
        let result = VlmGroundingService::parse_vlm_response(response).unwrap();
        assert_eq!(result.questions.len(), 1);
        assert_eq!(result.questions[0].figures.len(), 1);
        assert!(VlmGroundingService::parse_vlm_response(&response[..response.len() - 1]).is_err());
    }

    #[test]
    fn complete_json_gate_rejects_partial_envelopes_and_trailing_prose() {
        assert!(parse_complete_json(r#"[{"label":"1"}], omitted later items"#).is_err());
        assert!(parse_complete_json("```json\n[]").is_err());
        assert!(parse_complete_json("```json\n[]\n```").is_ok());
    }

    #[test]
    fn length_or_interrupted_responses_cannot_be_complete() {
        for reason in [
            "length",
            "max_tokens",
            "MAX_TOKENS",
            "max_output_tokens",
            "model_context_window_exceeded",
            "content_filter",
            "refusal",
            "tool_use",
            "tool_calls",
            "aborted",
            "unknown",
        ] {
            assert!(validate_complete_response(Some(reason), None).is_err());
        }
        assert!(validate_complete_response(None, None).is_err());
        assert!(validate_complete_response(Some("stop"), Some("socket closed")).is_err());
        for reason in ["stop", "end_turn", "stop_sequence", "completed", "STOP"] {
            assert!(validate_complete_response(Some(reason), None).is_ok());
        }
    }

    #[test]
    fn docx_budget_never_silently_truncates_text_or_images() {
        assert!(validate_docx_input_budget(36, &"字".repeat(8000)).is_ok());
        assert!(validate_docx_input_budget(37, "原文").is_err());
        assert!(validate_docx_input_budget(1, &"字".repeat(8001)).is_err());
    }

    #[test]
    fn typed_docx_answer_accepts_null_and_keeps_original_label_and_text() {
        let result = VlmGroundingService::parse_docx_extraction_response(
            r#"[{"block_kind":"answer","section":"term_definition","label":"3","source_id":"input:3","raw_text":"3. 原文评分要点","content":"摘要","answer":null,"explanation":null}]"#,
        ).unwrap();
        assert_eq!(result[0].block_kind, ImportBlockKind::Answer);
        assert_eq!(result[0].label, "3");
        assert_eq!(result[0].source_id, "input:3");
        assert_eq!(result[0].raw_text, "3. 原文评分要点");
        assert!(result[0].answer.is_empty());
        let unknown: VlmQuestion =
            serde_json::from_value(json!({"block_kind":"new_unsupported_kind"})).unwrap();
        assert_eq!(unknown.block_kind, ImportBlockKind::Unknown);
    }

    #[test]
    fn prompts_require_typed_source_blocks_and_forbid_generated_answers() {
        for prompt in [
            VlmGroundingService::build_analysis_prompt(None),
            VlmGroundingService::build_docx_extraction_prompt(1, "全部原文"),
        ] {
            assert!(prompt.contains("block_kind"));
            assert!(prompt.contains("term_definition"));
            assert!(prompt.contains("section"));
            assert!(prompt.contains("原题号"));
            assert!(prompt.contains("严禁推理"));
        }
    }

    #[test]
    fn test_truncate_utf8_ascii() {
        assert_eq!(truncate_utf8("hello", 10), "hello");
        assert_eq!(truncate_utf8("hello", 3), "hel");
        assert_eq!(truncate_utf8("", 5), "");
    }

    #[test]
    fn test_truncate_utf8_multibyte_boundary() {
        // "错" 为 3 字节（E9 94 99），截断点落在字符中间时应回退到边界
        let s = "错误信息"; // 12 字节
        assert_eq!(truncate_utf8(s, 12), "错误信息");
        assert_eq!(truncate_utf8(s, 11), "错误信"); // 回退到 9
        assert_eq!(truncate_utf8(s, 10), "错误信");
        assert_eq!(truncate_utf8(s, 9), "错误信");
        assert_eq!(truncate_utf8(s, 8), "错误"); // 回退到 6
        assert_eq!(truncate_utf8(s, 2), "");
    }

    #[test]
    fn test_docx_vlm_stream_budget_rejects_limit_and_integer_overflow() {
        let mut total = DOCX_VLM_MAX_STREAM_BYTES - 1;
        account_docx_vlm_stream_bytes(&mut total, 1).unwrap();
        assert_eq!(total, DOCX_VLM_MAX_STREAM_BYTES);
        assert!(account_docx_vlm_stream_bytes(&mut total, 1).is_err());

        let mut overflow = usize::MAX;
        assert!(account_docx_vlm_stream_bytes(&mut overflow, 1).is_err());
    }

    #[test]
    fn test_prepare_docx_image_fails_closed_for_malformed_input() {
        assert!(prepare_docx_image("not-a-data-url".to_string(), 0).is_err());
        assert!(prepare_docx_image("data:text/plain;base64,SGVsbG8=".to_string(), 0).is_err());
        assert!(prepare_docx_image("data:image/png;base64,!!!".to_string(), 0).is_err());
    }

    #[test]
    fn test_prepare_docx_image_resizes_and_enforces_hard_output_limit() {
        let prepared = prepare_docx_image(png_data_url(2_000, 100), 7).unwrap();
        assert_eq!(prepared.source_index, 7);
        assert!(prepared.byte_len <= DOCX_VLM_MAX_IMAGE_BYTES);
        assert!(prepared.data_url.starts_with("data:image/jpeg;base64,"));

        let (_, payload) = prepared.data_url.split_once(',').unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .unwrap();
        let (width, height) = image::io::Reader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .unwrap()
            .into_dimensions()
            .unwrap();
        assert!(width <= DOCX_VLM_MAX_DIMENSION);
        assert!(height <= DOCX_VLM_MAX_DIMENSION);
    }

    #[test]
    fn test_prepare_docx_images_applies_count_budget_before_processing() {
        let valid = png_data_url(1, 1);
        let candidates = vec![valid; DOCX_VLM_MAX_IMAGES];
        let prepared = prepare_docx_images(candidates, DOCX_VLM_MAX_IMAGES + 1);

        assert_eq!(prepared.images.len(), DOCX_VLM_MAX_IMAGES);
        assert_eq!(prepared.omitted_by_count, 1);
        assert!(prepared.rejected.is_empty());
        assert!(prepared.total_bytes <= DOCX_VLM_MAX_TOTAL_IMAGE_BYTES);
    }

    #[test]
    fn test_remap_docx_question_image_indices_drops_invalid_entries() {
        let mut question = VlmExtractedQuestion {
            content: String::new(),
            question_type: String::new(),
            options: Vec::new(),
            answer: String::new(),
            explanation: String::new(),
            image_indices: vec![0, 2, 99],
            difficulty: String::new(),
            tags: Vec::new(),
            ..Default::default()
        };

        remap_docx_question_image_indices(&mut question, &[3, 8, 12]);
        assert_eq!(question.image_indices, vec![3, 12]);
    }

    #[test]
    fn test_crop_figure_default_bbox_yields_minimal_crop() {
        // 默认 bbox [0,0,0,0] 应得到 1x1 裁切而非 panic
        let mut img = image::RgbImage::new(100, 80);
        img.put_pixel(0, 0, image::Rgb([255, 0, 0]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageOutputFormat::Png)
            .unwrap();
        let bytes = buf.into_inner();

        let result = VlmGroundingService::crop_figure_from_page(&bytes, &[0.0, 0.0, 0.0, 0.0]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_crop_figure_inverted_and_out_of_range_bbox() {
        let img = image::RgbImage::new(100, 80);
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageOutputFormat::Png)
            .unwrap();
        let bytes = buf.into_inner();

        // 颠倒坐标 + 超出 0-1 范围都应被归一化处理
        let result = VlmGroundingService::crop_figure_from_page(&bytes, &[0.9, 1.5, 0.1, -0.2]);
        assert!(result.is_ok());
    }
}
