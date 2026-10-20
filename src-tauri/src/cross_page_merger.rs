//! Conservative cross-page continuation with typed source provenance.
//! Missing/failed pages are boundaries, not evidence of an adjacent continuation.

use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::vlm_grounding_service::{ImportBlockKind, VlmFigure, VlmPageAnalysis, VlmQuestion};

/// One original fragment; every merged page/bbox remains auditable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSpan {
    pub page_index: usize,
    pub bbox: [f64; 4],
    pub source_id: String,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct MergedQuestion {
    pub question: VlmQuestion,
    pub page_indices: Vec<usize>,
    pub figures_with_page: Vec<(usize, VlmFigure)>,
    pub source_spans: Vec<SourceSpan>,
}

fn valid_scope(scope: &str) -> Option<&str> {
    let scope = scope.trim();
    // A section is semantic metadata, not a guessed relationship between numbers.
    scope
        .chars()
        .any(|c| c.is_alphabetic() && !"一二三四五六七八九十百千万零〇".contains(c))
        .then_some(scope)
}

fn compatible_labels(a: &str, b: &str) -> bool {
    a.trim().is_empty() || b.trim().is_empty() || a.trim() == b.trim()
}

pub fn merge_pages(page_analyses: &[Option<VlmPageAnalysis>]) -> Vec<MergedQuestion> {
    let mut result: Vec<MergedQuestion> = Vec::new();
    let mut active_section = String::new();

    for (page_idx, analysis_opt) in page_analyses.iter().enumerate() {
        let Some(analysis) = analysis_opt else {
            active_section.clear();
            continue;
        };
        for (block_idx, source) in analysis.questions.iter().enumerate() {
            let mut question = source.clone();
            question.normalize_legacy_kind();
            if question.source_id.trim().is_empty() {
                question.source_id = format!("page:{page_idx}:block:{block_idx}");
            }
            if let Some(scope) = valid_scope(&question.section).map(str::to_string) {
                question.section = scope.clone();
                active_section = scope;
            } else if question.block_kind == ImportBlockKind::Heading {
                // An unclassified heading may start a new chapter. Do not leak the
                // previous chapter's identity through that boundary.
                question.section.clear();
                active_section.clear();
            } else {
                let crossed_answer_boundary = result.last().is_some_and(|last| {
                    (last.question.block_kind == ImportBlockKind::Question
                        && question.block_kind.is_answer())
                        || (last.question.block_kind.is_answer()
                            && question.block_kind == ImportBlockKind::Question)
                });
                if crossed_answer_boundary || question.block_kind == ImportBlockKind::Unknown {
                    active_section.clear();
                }
                question.section = active_section.clone();
            }

            let can_merge = question.continues_from_previous
                && matches!(
                    question.block_kind,
                    ImportBlockKind::Question
                        | ImportBlockKind::Answer
                        | ImportBlockKind::Explanation
                        | ImportBlockKind::Material
                )
                && result.last().is_some_and(|last| {
                    last.question.continues_to_next
                        && last.question.block_kind == question.block_kind
                        && last.question.section == question.section
                        && compatible_labels(&last.question.label, &question.label)
                        && last
                            .page_indices
                            .last()
                            .is_some_and(|&last_page| page_idx.saturating_sub(last_page) <= 1)
                });

            let span = SourceSpan {
                page_index: page_idx,
                bbox: question.bbox,
                source_id: question.source_id.clone(),
                label: question.label.clone(),
            };
            if can_merge {
                let last = result.last_mut().expect("checked above");
                debug!(
                    "[CrossPageMerger] 页面 {} 来源 '{}' 续接 '{}'",
                    page_idx + 1,
                    question.source_id,
                    last.question.source_id
                );
                if !question.raw_text.is_empty() {
                    if !last.question.raw_text.is_empty() {
                        last.question.raw_text.push('\n');
                    }
                    last.question.raw_text.push_str(&question.raw_text);
                }
                if last.page_indices.last() != Some(&page_idx) {
                    last.page_indices.push(page_idx);
                }
                last.source_spans.push(span);
                for figure in &question.figures {
                    last.figures_with_page.push((page_idx, figure.clone()));
                    last.question.figures.push(figure.clone());
                }
                last.question.continues_to_next = question.continues_to_next;
            } else {
                result.push(MergedQuestion {
                    figures_with_page: question
                        .figures
                        .iter()
                        .map(|figure| (page_idx, figure.clone()))
                        .collect(),
                    question,
                    page_indices: vec![page_idx],
                    source_spans: vec![span],
                });
            }
        }
    }
    info!(
        "[CrossPageMerger] 合并完成: {} 个文档块, {} 个跨页块",
        result.len(),
        result.iter().filter(|q| q.page_indices.len() > 1).count()
    );
    result
}

pub fn partition_complete_blocks(
    blocks: Vec<MergedQuestion>,
) -> (Vec<MergedQuestion>, Vec<MergedQuestion>) {
    // Publishing a fragment before its continuation is available changes its
    // identity or freezes truncated content when a failed page is retried.
    blocks.into_iter().partition(|block| {
        !block.question.continues_from_previous && !block.question.continues_to_next
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn block(
        kind: ImportBlockKind,
        section: &str,
        label: &str,
        from: bool,
        to: bool,
    ) -> VlmQuestion {
        VlmQuestion {
            block_kind: kind,
            section: section.into(),
            label: label.into(),
            raw_text: format!("{label}原文"),
            bbox: [0.1, 0.2, 0.8, 0.9],
            continues_from_previous: from,
            continues_to_next: to,
            ..Default::default()
        }
    }
    fn page(block: VlmQuestion) -> Option<VlmPageAnalysis> {
        Some(VlmPageAnalysis {
            questions: vec![block],
        })
    }

    #[test]
    fn continuation_keeps_every_page_bbox_and_source_identity() {
        let pages = vec![
            page(block(
                ImportBlockKind::Question,
                "single_choice",
                "1",
                false,
                true,
            )),
            page(block(ImportBlockKind::Question, "", "1", true, false)),
        ];
        let result = merge_pages(&pages);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].page_indices, vec![0, 1]);
        assert_eq!(result[0].source_spans.len(), 2);
        assert_eq!(result[0].source_spans[1].bbox, [0.1, 0.2, 0.8, 0.9]);
        assert_ne!(
            result[0].source_spans[0].source_id,
            result[0].source_spans[1].source_id
        );
        assert_eq!(result[0].question.section, "single_choice");
    }

    #[test]
    fn never_merges_questions_with_answers_or_conflicting_sections_or_labels() {
        for (kind, scope, label) in [
            (ImportBlockKind::Answer, "single_choice", "1"),
            (ImportBlockKind::Question, "term_definition", "1"),
            (ImportBlockKind::Question, "single_choice", "2"),
        ] {
            let pages = vec![
                page(block(
                    ImportBlockKind::Question,
                    "single_choice",
                    "1",
                    false,
                    true,
                )),
                page(block(kind, scope, label, true, false)),
            ];
            assert_eq!(merge_pages(&pages).len(), 2);
        }
    }

    #[test]
    fn missing_page_prevents_continuation_and_scope_inheritance() {
        let pages = vec![
            page(block(
                ImportBlockKind::Question,
                "single_choice",
                "1",
                false,
                true,
            )),
            None,
            page(block(ImportBlockKind::Question, "", "1", true, false)),
        ];
        let result = merge_pages(&pages);
        assert_eq!(result.len(), 2);
        assert!(result[1].question.section.is_empty());
    }

    #[test]
    fn continuation_requires_two_sided_evidence() {
        let pages = vec![
            page(block(
                ImportBlockKind::Question,
                "single_choice",
                "1",
                false,
                false,
            )),
            page(block(
                ImportBlockKind::Question,
                "single_choice",
                "1",
                true,
                false,
            )),
        ];
        assert_eq!(merge_pages(&pages).len(), 2);
    }

    #[test]
    fn heading_scope_can_continue_across_pages_without_number_guessing() {
        let pages = vec![
            page(block(
                ImportBlockKind::Heading,
                "term_definition",
                "",
                false,
                false,
            )),
            page(block(ImportBlockKind::Answer, "", "1", false, false)),
        ];
        let result = merge_pages(&pages);
        assert_eq!(result[1].question.section, "term_definition");
        let numeric = vec![
            page(block(ImportBlockKind::Heading, "2", "", false, false)),
            page(block(ImportBlockKind::Answer, "", "1", false, false)),
        ];
        assert!(merge_pages(&numeric)[1].question.section.is_empty());
    }

    #[test]
    fn answer_region_without_scope_does_not_inherit_last_question_type() {
        let pages = vec![
            page(block(
                ImportBlockKind::Question,
                "short_answer",
                "8",
                false,
                false,
            )),
            page(block(ImportBlockKind::Answer, "", "1", false, false)),
        ];
        let result = merge_pages(&pages);
        assert!(result[1].question.section.is_empty());
    }

    #[test]
    fn failed_neighbor_defers_both_sides_until_a_complete_identity_exists() {
        let first = block(
            ImportBlockKind::Question,
            "synthetic_section",
            "1",
            false,
            true,
        );
        let second = block(
            ImportBlockKind::Question,
            "synthetic_section",
            "1",
            true,
            false,
        );
        for incomplete in [
            vec![page(first.clone()), None],
            vec![None, page(second.clone())],
        ] {
            let (ready, deferred) = partition_complete_blocks(merge_pages(&incomplete));
            assert!(ready.is_empty());
            assert_eq!(deferred.len(), 1);
        }
        let (ready, deferred) =
            partition_complete_blocks(merge_pages(&[page(first), page(second)]));
        assert!(deferred.is_empty());
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].question.source_id, "page:0:block:0");
        assert_eq!(ready[0].source_spans.len(), 2);
        assert_eq!(ready[0].page_indices, vec![0, 1]);
    }

    #[test]
    fn unresolved_or_conflicting_continuations_never_become_practice_questions() {
        let (ready, deferred) = partition_complete_blocks(merge_pages(&[
            page(block(
                ImportBlockKind::Question,
                "scope_a",
                "1",
                false,
                true,
            )),
            page(block(
                ImportBlockKind::Question,
                "scope_b",
                "1",
                true,
                false,
            )),
            page(block(
                ImportBlockKind::Question,
                "scope_b",
                "2",
                false,
                false,
            )),
        ]));
        assert_eq!(deferred.len(), 2);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].question.label, "2");
    }

    #[test]
    fn missing_head_stays_deferred_after_a_later_fragment_merges() {
        let (ready, deferred) = partition_complete_blocks(merge_pages(&[
            None,
            page(block(
                ImportBlockKind::Question,
                "synthetic_section",
                "1",
                true,
                true,
            )),
            page(block(
                ImportBlockKind::Question,
                "synthetic_section",
                "1",
                true,
                false,
            )),
        ]));
        assert!(ready.is_empty());
        assert_eq!(deferred.len(), 1);
        assert_eq!(deferred[0].source_spans.len(), 2);
        assert!(deferred[0].question.continues_from_previous);
    }

    #[test]
    fn legacy_answer_marker_is_upgraded_before_continuation() {
        let mut answer = block(ImportBlockKind::Question, "single_choice", "1", true, false);
        answer.raw_text = "[答案区] 答案：B".into();
        let pages = vec![
            page(block(
                ImportBlockKind::Question,
                "single_choice",
                "1",
                false,
                true,
            )),
            page(answer),
        ];
        let result = merge_pages(&pages);
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].question.block_kind, ImportBlockKind::Answer);
    }
}
