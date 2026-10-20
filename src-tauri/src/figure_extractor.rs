//! Stage 4: Figure Extractor — 配图裁切、质量校验与精确关联
//!
//! 从页面图片中按 VLM 提供的 bbox 坐标裁切配图，
//! 存储到 VFS Blob 并创建 QuestionImage 关联记录。
//!
//! 页面图片按 blob_hash 从 VFS 按需读取，避免长期持有大量内存。

use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::cross_page_merger::MergedQuestion;
use crate::models::AppError;
use crate::page_rasterizer::{self, PageSlice};
use crate::vfs::database::VfsDatabase;
use crate::vfs::repos::{QuestionImage, VfsBlobRepo, VfsFileRepo};
use crate::vlm_grounding_service::VlmGroundingService;

const MIN_FIGURE_SIZE: u32 = 30;

/// 题目及其精确关联的配图列表
#[derive(Debug, Clone)]
pub struct QuestionWithFigures {
    pub merged: MergedQuestion,
    pub images: Vec<QuestionImage>,
}

/// 配图提取失败的可审计原因。失败记录只含来源标识，不含题目原文。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FigureExtractionFailureReason {
    /// `figures_with_page` 中的页面索引超出本次页面数组。
    PageOutOfRange,
    /// 页面图片 blob 查询或读取失败。
    PageImageLoadFailed,
    /// 按 bbox 裁切或编码裁切结果失败。
    BboxCropFailed,
    /// 裁切结果小于最小尺寸。
    FigureTooSmall { width: u32, height: u32 },
    /// 配图 blob 或 VFS 文件落库失败。
    StoreFailed,
}

/// 单张配图提取失败明细；只含来源标识、页索引和原因，不含原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FigureExtractionFailure {
    /// 题目稳定来源 ID（来自 `merged.question.source_id` 或 `source_spans`）。
    pub source_id: String,
    /// 页面索引，0-based，与 `MergedQuestion::figures_with_page` 一致。
    pub page_index: usize,
    pub reason: FigureExtractionFailureReason,
}

/// 配图提取的完整结果。调用方必须同时消费 `questions` 与 `failures`。
#[derive(Debug, Clone, Default)]
pub struct FigureExtractionOutcome {
    pub questions: Vec<QuestionWithFigures>,
    pub failures: Vec<FigureExtractionFailure>,
}

impl FigureExtractionOutcome {
    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }
}

/// 为合并后的题目列表批量裁切配图并关联到各题
///
/// 页面图片按需通过 `blob_hash` 从 VFS 读取，不要求 PageSlice 持有 image_bytes。
/// 返回的 `failures` 非空时，调用方不得把本次导入标记为 completed。
pub fn extract_figures(
    merged_questions: Vec<MergedQuestion>,
    pages: &[PageSlice],
    vfs_db: &VfsDatabase,
) -> FigureExtractionOutcome {
    let mut results = Vec::with_capacity(merged_questions.len());
    let mut failures = Vec::new();
    let mut total_extracted = 0usize;

    let mut page_image_cache = lru::LruCache::<usize, Vec<u8>>::new(
        std::num::NonZeroUsize::new(3).expect("nonzero page cache capacity"),
    );

    for mq in merged_questions {
        let source_id = question_source_id(&mq);
        let mut images: Vec<QuestionImage> = Vec::new();

        for (page_idx, figure) in &mq.figures_with_page {
            let page = match pages.get(*page_idx) {
                Some(p) => p,
                None => {
                    warn!(
                        "[FigureExtractor] 页面索引 {} 超出范围，跳过配图 '{}'",
                        page_idx, figure.fig_label
                    );
                    record_failure(
                        &mut failures,
                        &source_id,
                        *page_idx,
                        FigureExtractionFailureReason::PageOutOfRange,
                    );
                    continue;
                }
            };

            if !page_image_cache.contains(page_idx) {
                match page_rasterizer::load_page_image_bytes(vfs_db, &page.blob_hash) {
                    Ok(bytes) => {
                        page_image_cache.put(*page_idx, bytes);
                    }
                    Err(e) => {
                        warn!(
                            "[FigureExtractor] 加载页面 {} 图片失败: {}",
                            page_idx + 1,
                            e
                        );
                        record_failure(
                            &mut failures,
                            &source_id,
                            *page_idx,
                            FigureExtractionFailureReason::PageImageLoadFailed,
                        );
                        continue;
                    }
                }
            }
            let page_bytes = page_image_cache.get(page_idx).expect("page loaded above");

            let cropped_bytes =
                match VlmGroundingService::crop_figure_from_page(page_bytes, &figure.bbox) {
                    Ok(b) => b,
                    Err(e) => {
                        debug!(
                            "[FigureExtractor] 裁切失败 (页面{}, '{}'): {}",
                            page_idx + 1,
                            figure.fig_label,
                            e
                        );
                        record_failure(
                            &mut failures,
                            &source_id,
                            *page_idx,
                            FigureExtractionFailureReason::BboxCropFailed,
                        );
                        continue;
                    }
                };

            // ★ F1：仅读图片头获取尺寸（旧实现完整解码一遍裁切结果）
            let dims = image::io::Reader::new(std::io::Cursor::new(&cropped_bytes))
                .with_guessed_format()
                .ok()
                .and_then(|reader| reader.into_dimensions().ok());
            if let Some((w, h)) = dims {
                if w < MIN_FIGURE_SIZE || h < MIN_FIGURE_SIZE {
                    debug!(
                        "[FigureExtractor] 配图太小 ({}x{})，跳过: '{}'",
                        w, h, figure.fig_label
                    );
                    record_failure(
                        &mut failures,
                        &source_id,
                        *page_idx,
                        FigureExtractionFailureReason::FigureTooSmall {
                            width: w,
                            height: h,
                        },
                    );
                    continue;
                }
            }

            match store_figure(vfs_db, &cropped_bytes, &figure.fig_label, *page_idx) {
                Ok(qi) => {
                    images.push(qi);
                    total_extracted += 1;
                }
                Err(e) => {
                    warn!(
                        "[FigureExtractor] 存储配图失败 (页面{}, '{}'): {}",
                        page_idx + 1,
                        figure.fig_label,
                        e
                    );
                    record_failure(
                        &mut failures,
                        &source_id,
                        *page_idx,
                        FigureExtractionFailureReason::StoreFailed,
                    );
                }
            }
        }

        results.push(QuestionWithFigures { merged: mq, images });
    }

    info!(
        "[FigureExtractor] 完成: 提取 {} 张配图, 记录 {} 条失败",
        total_extracted,
        failures.len()
    );

    FigureExtractionOutcome {
        questions: results,
        failures,
    }
}

fn question_source_id(mq: &MergedQuestion) -> String {
    let direct = mq.question.source_id.trim();
    if !direct.is_empty() {
        return direct.to_string();
    }

    if let Some(source_id) = mq
        .source_spans
        .iter()
        .map(|span| span.source_id.trim())
        .find(|source_id| !source_id.is_empty())
    {
        return source_id.to_string();
    }

    mq.page_indices
        .first()
        .map(|page_index| format!("page:{page_index}:unknown-source"))
        .unwrap_or_else(|| "unknown-source".to_string())
}

fn record_failure(
    failures: &mut Vec<FigureExtractionFailure>,
    source_id: &str,
    page_index: usize,
    reason: FigureExtractionFailureReason,
) {
    failures.push(FigureExtractionFailure {
        source_id: source_id.to_string(),
        page_index,
        reason,
    });
}

fn store_figure(
    vfs_db: &VfsDatabase,
    image_bytes: &[u8],
    fig_label: &str,
    page_idx: usize,
) -> Result<QuestionImage, AppError> {
    let blob = VfsBlobRepo::store_blob(vfs_db, image_bytes, Some("image/png"), Some("png"))
        .map_err(|e| AppError::database(format!("配图 Blob 存储失败: {}", e)))?;

    let file_name = format!("fig_p{}_{}.png", page_idx, sanitize_label(fig_label));

    let vfs_file = VfsFileRepo::create_file(
        vfs_db,
        &blob.hash,
        &file_name,
        image_bytes.len() as i64,
        "image",
        Some("image/png"),
        Some(&blob.hash),
        None,
    )
    .map_err(|e| AppError::database(format!("创建配图 VFS 文件失败: {}", e)))?;

    Ok(QuestionImage {
        id: vfs_file.id,
        name: file_name,
        mime: "image/png".to_string(),
        hash: blob.hash,
    })
}

fn sanitize_label(label: &str) -> String {
    label
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .take(20)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_page_merger::MergedQuestion;
    use crate::vfs::database::setup_migrated_test_db;
    use crate::vfs::repos::VfsBlobRepo;
    use crate::vlm_grounding_service::{ImportBlockKind, VlmFigure, VlmQuestion};
    use std::io::Cursor;

    fn figure(bbox: [f64; 4]) -> VlmFigure {
        VlmFigure {
            bbox,
            fig_label: "fig".to_string(),
        }
    }

    fn merged_question(source_id: &str, figures: Vec<(usize, VlmFigure)>) -> MergedQuestion {
        MergedQuestion {
            question: VlmQuestion {
                source_id: source_id.to_string(),
                block_kind: ImportBlockKind::Question,
                ..Default::default()
            },
            page_indices: figures.iter().map(|(page, _)| *page).collect(),
            figures_with_page: figures,
            source_spans: Vec::new(),
        }
    }

    fn page(page_index: usize, blob_hash: &str) -> PageSlice {
        PageSlice {
            page_index,
            blob_hash: blob_hash.to_string(),
            text_hint: None,
            width: 100,
            height: 100,
        }
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([255, 255, 255, 255]));
        let mut buffer = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut buffer, image::ImageOutputFormat::Png)
            .expect("encode test png");
        buffer.into_inner()
    }

    fn store_page_blob(db: &VfsDatabase, bytes: &[u8]) -> String {
        VfsBlobRepo::store_blob(db, bytes, Some("image/png"), Some("png"))
            .expect("store page blob")
            .hash
    }

    fn assert_single_failure(
        outcome: &FigureExtractionOutcome,
        source_id: &str,
        page_index: usize,
        reason: FigureExtractionFailureReason,
    ) {
        assert_eq!(outcome.questions.len(), 1);
        assert!(outcome.questions[0].images.is_empty());
        assert_eq!(outcome.failures.len(), 1, "expected exactly one failure");
        assert_eq!(outcome.failures[0].source_id, source_id);
        assert_eq!(outcome.failures[0].page_index, page_index);
        assert_eq!(outcome.failures[0].reason, reason);
    }

    #[test]
    fn records_page_load_crop_and_size_failures_without_dropping_question() {
        let (_temp_dir, db) = setup_migrated_test_db();

        let out_of_range = extract_figures(
            vec![merged_question(
                "src-page",
                vec![(3, figure([0.0, 0.0, 1.0, 1.0]))],
            )],
            &[],
            &db,
        );
        assert_single_failure(
            &out_of_range,
            "src-page",
            3,
            FigureExtractionFailureReason::PageOutOfRange,
        );

        let load_failed = extract_figures(
            vec![merged_question(
                "src-load",
                vec![(0, figure([0.0, 0.0, 1.0, 1.0]))],
            )],
            &[page(0, "missing-blob-hash")],
            &db,
        );
        assert_single_failure(
            &load_failed,
            "src-load",
            0,
            FigureExtractionFailureReason::PageImageLoadFailed,
        );

        let invalid_page_hash = store_page_blob(&db, b"not an image");
        let crop_failed = extract_figures(
            vec![merged_question(
                "src-crop",
                vec![(0, figure([0.0, 0.0, 1.0, 1.0]))],
            )],
            &[page(0, &invalid_page_hash)],
            &db,
        );
        assert_single_failure(
            &crop_failed,
            "src-crop",
            0,
            FigureExtractionFailureReason::BboxCropFailed,
        );

        let small_size = MIN_FIGURE_SIZE - 10;
        let small_page_hash = store_page_blob(&db, &png_bytes(small_size, small_size));
        let too_small = extract_figures(
            vec![merged_question(
                "src-small",
                vec![(0, figure([0.0, 0.0, 1.0, 1.0]))],
            )],
            &[page(0, &small_page_hash)],
            &db,
        );
        assert_single_failure(
            &too_small,
            "src-small",
            0,
            FigureExtractionFailureReason::FigureTooSmall {
                width: small_size,
                height: small_size,
            },
        );
    }

    #[test]
    fn records_store_failure_without_dropping_question() {
        let (_temp_dir, db) = setup_migrated_test_db();
        let page_hash = store_page_blob(&db, &png_bytes(100, 100));

        // 让 files 的 INSERT 必定失败，覆盖 store_figure 的落库失败分支。
        db.get_conn_safe()
            .expect("connection")
            .execute_batch(
                "CREATE TRIGGER fail_figure_file_insert BEFORE INSERT ON files
                 BEGIN
                   SELECT RAISE(ABORT, 'forced figure store failure');
                 END;",
            )
            .expect("create failure trigger");

        let outcome = extract_figures(
            vec![merged_question(
                "src-store",
                vec![(0, figure([0.0, 0.0, 1.0, 1.0]))],
            )],
            &[page(0, &page_hash)],
            &db,
        );
        assert_single_failure(
            &outcome,
            "src-store",
            0,
            FigureExtractionFailureReason::StoreFailed,
        );
    }

    #[test]
    fn stores_valid_figure_without_failure() {
        let (_temp_dir, db) = setup_migrated_test_db();
        let page_hash = store_page_blob(&db, &png_bytes(100, 100));

        let outcome = extract_figures(
            vec![merged_question(
                "src-ok",
                vec![(0, figure([0.0, 0.0, 0.5, 0.5]))],
            )],
            &[page(0, &page_hash)],
            &db,
        );

        assert!(outcome.failures.is_empty());
        assert_eq!(outcome.questions.len(), 1);
        assert_eq!(outcome.questions[0].images.len(), 1);
    }
}
