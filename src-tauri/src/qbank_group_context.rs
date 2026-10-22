//! #491 题组上下文投影层（读取复用模型仓库，不重复 SQL）。
//!
//! 数据读取与可见性（manifest / publish_state / group_version）完全交给
//! `VfsQuestionGroupRepo::get_context_for_question_with_conn`；本模块只做两件事：
//! 1. 按用途裁剪：练习 / AI 工具不携带兄弟题学习状态；编辑 / 导出保留 status。
//!    兄弟题本身已由仓库限定为 `QuestionSiblingBrief`（无 answer / explanation /
//!    structured_data / user_answer / is_correct），这里不会再引入答案字段。
//! 2. 渲染成判题 / 变式题 / AI 工具共用 prompt 段落。
//!
//! 组头不是可答题，不计题数；进度按子题聚合（仓库已算好）。

use rusqlite::Connection;
use serde_json::{json, Value};

use crate::vfs::repos::{QuestionGroupContext, VfsQuestionGroupRepo};

/// 上下文用途：决定是否保留兄弟题学习状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextPurpose {
    Practice,
    AiTool,
    Editor,
    Export,
}

impl ContextPurpose {
    pub fn keeps_study_state(self) -> bool {
        matches!(self, Self::Editor | Self::Export)
    }
}

/// 读取单题上下文；组不可见、题目无组或读取失败时返回 None。
pub fn load_question_group_context(
    conn: &Connection,
    question_id: &str,
) -> Option<QuestionGroupContext> {
    match VfsQuestionGroupRepo::get_context_for_question_with_conn(conn, question_id) {
        Ok(context) => context,
        Err(error) => {
            log::warn!(
                "[qbank_group_context] load context failed for {}: {}",
                question_id,
                error
            );
            None
        }
    }
}

/// 序列化成前端/AI 工具共用载荷；练习与 AI 工具裁掉兄弟题 status。
pub fn context_to_value(
    context: &QuestionGroupContext,
    purpose: ContextPurpose,
    exclude_question_id: Option<&str>,
) -> Option<Value> {
    let mut value = serde_json::to_value(context).ok()?;
    if let Some(ids) = value.get_mut("siblings").and_then(Value::as_array_mut) {
        if let Some(exclude) = exclude_question_id {
            ids.retain(|sibling| sibling.get("id").and_then(Value::as_str) != Some(exclude));
        }
    }
    if !purpose.keeps_study_state() {
        if let Some(siblings) = value.get_mut("siblings").and_then(Value::as_array_mut) {
            for sibling in siblings.iter_mut() {
                if let Some(object) = sibling.as_object_mut() {
                    object.remove("status");
                }
            }
        }
    }
    Some(value)
}

/// 题目对象上已附着的上下文（仓库批量注入）序列化。
pub fn attached_context_to_value(
    context: Option<&QuestionGroupContext>,
    purpose: ContextPurpose,
    exclude_question_id: Option<&str>,
) -> Value {
    context
        .and_then(|context| context_to_value(context, purpose, exclude_question_id))
        .unwrap_or(Value::Null)
}

/// 渲染判题 / 变式题 / AI 工具 prompt 段落：共享材料只出现一次，兄弟题只有题干与选项。
pub fn render_context_prompt(
    context: &QuestionGroupContext,
    exclude_question_id: Option<&str>,
) -> String {
    let mut prompt = String::new();
    if !context.materials.is_empty() {
        prompt.push_str("## 共享材料（本题组共用，标准答案不在此处）\n");
        for material in &context.materials {
            prompt.push_str(&format!(
                "[材料 {} · {}]\n",
                material.order_index,
                material.content_type.as_str()
            ));
            if let Some(title) = material
                .title
                .as_deref()
                .filter(|title| !title.trim().is_empty())
            {
                prompt.push_str(&format!("{title}\n"));
            }
            if !material.text.trim().is_empty() {
                prompt.push_str(material.text.trim());
                prompt.push('\n');
            }
            if !material.images.is_empty() {
                prompt.push_str(&format!("[配图 ×{}]\n", material.images.len()));
            }
            prompt.push('\n');
        }
    }
    let siblings: Vec<_> = context
        .siblings
        .iter()
        .filter(|sibling| Some(sibling.id.as_str()) != exclude_question_id)
        .collect();
    if !siblings.is_empty() {
        prompt.push_str("## 同组其他小题（仅题干与选项；不得假设或泄露其答案）\n");
        for sibling in siblings {
            let label = sibling
                .sub_label
                .as_deref()
                .or(sibling.question_label.as_deref())
                .unwrap_or(&sibling.id);
            prompt.push_str(&format!("{label}. {}\n", sibling.content));
            for option in sibling.options.iter().flatten() {
                prompt.push_str(&format!("   {}. {}\n", option.key, option.content));
            }
        }
        prompt.push('\n');
    }
    prompt
}

/// 组进度摘要（不泄露答案，仅用于日志/工具回执）。
pub fn progress_value(context: &QuestionGroupContext) -> Value {
    json!({
        "total": context.group_progress.total,
        "new": context.group_progress.new,
        "in_progress": context.group_progress.in_progress,
        "mastered": context.group_progress.mastered,
        "review": context.group_progress.review,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::repos::{
        GroupProgress, GroupPublishState, MaterialContentType, QuestionGroup,
        QuestionGroupMaterial, QuestionSiblingBrief, QuestionStatus, QuestionType, SourceType,
    };

    fn sample_context() -> QuestionGroupContext {
        QuestionGroupContext {
            group: QuestionGroup {
                id: "g1".to_string(),
                exam_id: "exam_1".to_string(),
                group_type: crate::vfs::repos::QuestionGroupType::SharedCondition,
                title: Some("Shared".to_string()),
                source_type: SourceType::Imported,
                source_ref: None,
                import_baseline: None,
                version: 4,
                publish_state: GroupPublishState::Complete,
                expected_child_ids: vec!["q1".into(), "q2".into()],
                expected_material_ids: vec!["m1".into()],
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
                deleted_at: None,
            },
            materials: vec![QuestionGroupMaterial {
                id: "m1".to_string(),
                group_id: "g1".to_string(),
                exam_id: "exam_1".to_string(),
                order_index: 1,
                content_type: MaterialContentType::Text,
                title: Some("Passage".to_string()),
                text: "SHARED CONDITION TEXT".to_string(),
                images: vec![],
                source_ref: None,
                page_index: Some(1),
                bbox: None,
                group_version: 4,
                created_at: "now".to_string(),
                updated_at: "now".to_string(),
                deleted_at: None,
            }],
            siblings: vec![QuestionSiblingBrief {
                id: "q2".to_string(),
                sub_label: Some("(2)".to_string()),
                question_label: Some("2".to_string()),
                group_order: Some(2),
                question_type: QuestionType::ShortAnswer,
                status: QuestionStatus::Review,
                content: "SIBLING STEM".to_string(),
                options: None,
            }],
            sibling_count: 2,
            group_progress: GroupProgress {
                total: 2,
                new: 1,
                in_progress: 0,
                mastered: 0,
                review: 1,
            },
        }
    }

    #[test]
    fn practice_context_drops_sibling_study_state() {
        let value =
            context_to_value(&sample_context(), ContextPurpose::Practice, Some("q1")).unwrap();
        let sibling = &value["siblings"][0];
        assert!(sibling.get("status").is_none());
        assert!(sibling.get("answer").is_none());
        assert!(sibling.get("explanation").is_none());
        assert_eq!(sibling["content"], "SIBLING STEM");
        assert_eq!(value["group"]["version"], 4);
        assert_eq!(value["materials"][0]["order_index"], 1);
    }

    #[test]
    fn editor_context_keeps_status_but_never_answers() {
        let value =
            context_to_value(&sample_context(), ContextPurpose::Editor, Some("q1")).unwrap();
        assert_eq!(value["siblings"][0]["status"], "review");
        assert!(value["siblings"][0].get("answer").is_none());
    }

    #[test]
    fn projection_excludes_current_question_even_if_repo_returns_it() {
        let mut context = sample_context();
        let mut self_sibling = context.siblings[0].clone();
        self_sibling.id = "q1".to_string();
        self_sibling.content = "CURRENT STEM".to_string();
        context.siblings.insert(0, self_sibling);

        let value = context_to_value(&context, ContextPurpose::Practice, Some("q1")).unwrap();
        let siblings = value["siblings"].as_array().unwrap();
        assert_eq!(siblings.len(), 1);
        assert_eq!(siblings[0]["id"], "q2");

        let prompt = render_context_prompt(&context, Some("q1"));
        assert!(!prompt.contains("CURRENT STEM"));
        assert!(prompt.contains("SIBLING STEM"));
    }

    #[test]
    fn prompt_contains_materials_and_stems_only() {
        let prompt = render_context_prompt(&sample_context(), Some("q1"));
        assert!(prompt.contains("SHARED CONDITION TEXT"));
        assert!(prompt.contains("SIBLING STEM"));
        assert!(!prompt.contains("SECRET"));
        // 兄弟题状态不进 prompt
        assert!(!prompt.contains("review"));
    }

    #[test]
    fn progress_is_exposed_without_answers() {
        let progress = progress_value(&sample_context());
        assert_eq!(progress["total"], 2);
        assert_eq!(progress["review"], 1);
    }
}
