//! 答案区合并与同题干去重
//!
//! 很多习题册/答案册是「题目区 + 答案区」两段式排版：
//! - 题目区：题干（可能带选项），没有答案；
//! - 答案区：成片的「题号 + 答案正文」，VLM 会把它们当成新题目输出。
//!
//! 如果直接落库，会出现两个问题：
//! 1. 答案区条目因为「没有题干」被当成空题目丢弃（静默丢题）；
//! 2. 答案正文与题干重复入库（同一道名词解释出现两遍，一份没答案、一份有答案）。
//!
//! 这里做两件事，且**只在证据充分时动手**：
//! - 同题干去重：归一化后相同/互相包含的条目合并为一条，答案向keeper 回填；
//! - 答案块回填：连续的「content 为空 + 有 answer」条目，当且仅当数量与某个题型
//!   的题目总数完全相等时，按顺序回填答案（只填空白，不覆盖已有答案）。
//!
//! 任何不满足守卫的条目都保持原样（不会被删），由上游决定如何展示。

use serde_json::Value;

/// 合并条目：label + 结构化 JSON + 是否已被合并掉
#[derive(Debug, Clone)]
pub struct MergeItem {
    pub label: String,
    pub json: Value,
    pub removed: bool,
}

impl MergeItem {
    pub fn new(label: String, json: Value) -> Self {
        Self {
            label,
            json,
            removed: false,
        }
    }

    fn content(&self) -> &str {
        self.json
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    fn answer(&self) -> &str {
        self.json
            .get("answer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    fn explanation(&self) -> &str {
        self.json
            .get("explanation")
            .and_then(|v| v.as_str())
            .unwrap_or("")
    }

    fn question_type(&self) -> &str {
        self.json
            .get("question_type")
            .and_then(|v| v.as_str())
            .unwrap_or("other")
    }

    fn has_options(&self) -> bool {
        self.json
            .get("options")
            .and_then(|v| v.as_array())
            .map(|arr| !arr.is_empty())
            .unwrap_or(false)
    }
}

/// 合并统计
#[derive(Debug, Default, Clone)]
pub struct MergeOutcome {
    /// 同题干去重掉的条目数
    pub duplicates: usize,
    /// 通过去重回填的答案数
    pub answers_from_duplicates: usize,
    /// 答案块按顺序回填到题目的数量
    pub answers_from_block: usize,
    /// 无法回填、保持原样的答案区条目数（不会被删除）
    pub unmatched_answer_entries: usize,
}

/// 归一化题干：去空白、去标点、小写，
/// 便于「试述 XXX」与「XXX（7 分）」这类同一题的不同写法互相匹配。
fn normalize(text: &str) -> String {
    const PUNCT: &str = "，。；：、（）()【】[]{}?!！？,.;:\"'“”‘’·—–-_/\\|";
    text.chars()
        .filter(|c| !c.is_whitespace() && !PUNCT.contains(*c))
        .collect::<String>()
        .to_lowercase()
}

fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// 两个条目是否指向同一道题
///
/// 只做**保守**合并，宁可漏合并也不误合并：
/// - 归一化后完全相同 → 合并；
/// - 互相包含时，要求长度比 ≥ 0.8 **且** 恰好只有一方有答案
///   （典型的「题目区无答案 / 答案区有答案」成对出现）。
///   两条都没有答案的近似题干（如「简述 X 的不良反应」vs「简述 X 的不良反应与禁忌证」）
///   不会被吞并。
fn same_question(a: &MergeItem, b: &MergeItem) -> bool {
    let na = normalize(a.content());
    let nb = normalize(b.content());
    if na.is_empty() || nb.is_empty() {
        return false;
    }
    if na == nb {
        return true;
    }

    let (short, long) = if na.len() <= nb.len() {
        (na.as_str(), nb.as_str())
    } else {
        (nb.as_str(), na.as_str())
    };
    let ratio = short.len() as f32 / long.len() as f32;
    let only_one_has_answer = is_blank(a.answer()) != is_blank(b.answer());

    short.len() >= 6 && long.contains(short) && ratio >= 0.8 && only_one_has_answer
}

fn merge_missing_fields(keeper: &mut Value, donor: &Value) {
    for field in ["answer", "explanation", "difficulty"] {
        let keeper_blank = keeper
            .get(field)
            .map(|v| v.is_null() || v.as_str().map(is_blank).unwrap_or(false))
            .unwrap_or(true);
        if !keeper_blank {
            continue;
        }
        if let Some(value) = donor.get(field) {
            if !value.is_null() {
                keeper[field] = value.clone();
            }
        }
    }

    // 选项：keeper 没有选项而 donor 有时直接继承
    let keeper_has_options = keeper
        .get("options")
        .and_then(|v| v.as_array())
        .map(|a| !a.is_empty())
        .unwrap_or(false);
    if !keeper_has_options {
        if let Some(options) = donor.get("options") {
            if options.as_array().map(|a| !a.is_empty()).unwrap_or(false) {
                keeper["options"] = options.clone();
            }
        }
    }
}

/// 答案区条目：没有题干、没有选项，但有答案正文
fn is_answer_only_entry(item: &MergeItem) -> bool {
    !item.removed && is_blank(item.content()) && !is_blank(item.answer()) && !item.has_options()
}

/// 执行去重 + 答案块回填
pub fn merge_answer_sections(items: &mut [MergeItem]) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();

    // ── 第一遍：同题干去重（保留最靠前的题目区条目，答案从答案区回填） ──
    for i in 0..items.len() {
        if items[i].removed || is_blank(items[i].content()) {
            continue;
        }
        for j in (i + 1)..items.len() {
            if items[j].removed || is_blank(items[j].content()) {
                continue;
            }
            if !same_question(&items[i], &items[j]) {
                continue;
            }

            let had_answer = !is_blank(items[i].answer());
            let donor_answer = items[j].answer().to_string();
            let donor_explanation = items[j].explanation().to_string();
            let donor = items[j].json.clone();
            merge_missing_fields(&mut items[i].json, &donor);
            items[j].removed = true;
            outcome.duplicates += 1;

            if !had_answer && !is_blank(&donor_answer) && !is_blank(items[i].answer()) {
                outcome.answers_from_duplicates += 1;
            }
            if is_blank(&donor_explanation) {
                // donor 没有解析属正常情况，无需处理
            }
        }
    }

    // ── 第二遍：答案块按顺序回填 ──
    let answer_entries: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| is_answer_only_entry(item))
        .map(|(idx, _)| idx)
        .collect();

    let contiguous = answer_entries.windows(2).all(|pair| pair[1] == pair[0] + 1);

    if !answer_entries.is_empty() && contiguous {
        // 按题型分组（只保留有题干、未合并的题）
        let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
        for (idx, item) in items.iter().enumerate() {
            if item.removed || is_blank(item.content()) || item.has_options() {
                continue;
            }
            let qtype = item.question_type().to_string();
            match groups.iter_mut().find(|(t, _)| *t == qtype) {
                Some((_, list)) => list.push(idx),
                None => groups.push((qtype, vec![idx])),
            }
        }
        // 也允许「有选项」的题型（选择题答案块）：单独再扫一遍
        for (idx, item) in items.iter().enumerate() {
            if item.removed || is_blank(item.content()) || !item.has_options() {
                continue;
            }
            let qtype = item.question_type().to_string();
            match groups.iter_mut().find(|(t, _)| *t == qtype) {
                Some((_, list)) => list.push(idx),
                None => groups.push((qtype, vec![idx])),
            }
        }
        for (_, list) in groups.iter_mut() {
            list.sort_unstable();
        }

        // 守卫：答案块长度必须与某个题型的题目总数完全相等，否则不猜
        let matched_group = groups
            .iter()
            .find(|(_, list)| list.len() == answer_entries.len())
            .cloned();

        if let Some((qtype, targets)) = matched_group {
            let mut filled = 0usize;
            for (entry_idx, target_idx) in answer_entries.iter().zip(targets.iter()) {
                let mut used = false;
                if !is_blank(items[*entry_idx].answer()) && is_blank(items[*target_idx].answer()) {
                    let answer = items[*entry_idx].answer().to_string();
                    items[*target_idx].json["answer"] = Value::String(answer);
                    filled += 1;
                    used = true;
                }
                // 解析同理
                if !is_blank(items[*entry_idx].explanation())
                    && is_blank(items[*target_idx].explanation())
                {
                    let explanation = items[*entry_idx].explanation().to_string();
                    items[*target_idx].json["explanation"] = Value::String(explanation);
                    used = true;
                }
                // 只有真正被吸收的条目才移除，否则保留（不丢数据）
                if used {
                    items[*entry_idx].removed = true;
                }
            }
            outcome.answers_from_block = filled;
            if filled > 0 {
                tracing::info!(
                    "[AnswerMerge] 答案块回填: {} 条 → {} 题型 {} 题",
                    filled,
                    qtype,
                    targets.len()
                );
            }
        }
    }

    outcome.unmatched_answer_entries = items
        .iter()
        .filter(|item| is_answer_only_entry(item))
        .count();

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(label: &str, content: &str, question_type: &str, answer: &str) -> MergeItem {
        MergeItem::new(
            label.to_string(),
            json!({
                "content": content,
                "question_type": question_type,
                "options": null,
                "answer": if answer.is_empty() { Value::Null } else { Value::String(answer.to_string()) },
                "explanation": null,
            }),
        )
    }

    fn item_with_options(label: &str, content: &str, answer: &str) -> MergeItem {
        MergeItem::new(
            label.to_string(),
            json!({
                "content": content,
                "question_type": "single_choice",
                "options": [
                    {"key": "A", "content": "甲"},
                    {"key": "B", "content": "乙"}
                ],
                "answer": if answer.is_empty() { Value::Null } else { Value::String(answer.to_string()) },
                "explanation": null,
            }),
        )
    }

    // ★ 真实样本：Q21 与 Q59 同为「致心律失常作用（proarrhythmia）」
    #[test]
    fn dedupes_same_stem_and_fills_answer() {
        let mut items = vec![
            item("Q21", "致心律失常作用（proarrhythmia）", "short_answer", ""),
            item(
                "Q59",
                "致心律失常作用（proarrhythmia）",
                "short_answer",
                "核心定义（2分）：抗心律失常药在治疗过程中诱发新的心律失常",
            ),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.duplicates, 1);
        assert_eq!(outcome.answers_from_duplicates, 1);
        assert!(items[1].removed);
        assert_eq!(
            items[0].json["answer"],
            "核心定义（2分）：抗心律失常药在治疗过程中诱发新的心律失常"
        );
    }

    // ★ 真实样本：Q31「试述…（7 分）」与 Q69「…（7 分）」
    #[test]
    fn dedupes_by_containment_with_prefix() {
        let mut items = vec![
            item(
                "Q31",
                "试述心律失常发生的电生理学机制。（7 分）",
                "essay",
                "",
            ),
            item(
                "Q69",
                "心律失常发生的电生理学机制（7 分）",
                "short_answer",
                "答案正文",
            ),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.duplicates, 1);
        assert_eq!(items[0].json["answer"], "答案正文");
    }

    // ★ 答案块：20 道选择题 + 20 条空题干答案，按顺序回填
    #[test]
    fn maps_answer_block_by_type_count() {
        let mut items = Vec::new();
        for i in 1..=3 {
            items.push(item_with_options(&format!("Q{i}"), &format!("第{i}题"), ""));
        }
        for letter in ["B", "C", "A"] {
            items.push(item("", "", "other", letter));
        }
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_from_block, 3);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(items[1].json["answer"], "C");
        assert_eq!(items[2].json["answer"], "A");
        assert!(items[3].removed && items[4].removed && items[5].removed);
    }

    // 守卫：答案条数与任何题型都对不上时不猜，保持原样
    #[test]
    fn does_not_guess_when_block_size_mismatches() {
        let mut items = vec![
            item_with_options("Q1", "第一题", ""),
            item_with_options("Q2", "第二题", ""),
            item("", "", "other", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_from_block, 0);
        assert!(!items[2].removed);
        assert_eq!(outcome.unmatched_answer_entries, 1);
    }

    // 已有答案不被覆盖
    #[test]
    fn does_not_overwrite_existing_answer() {
        let mut items = vec![
            item_with_options("Q1", "第一题", "D"),
            item("", "", "other", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "D");
        assert_eq!(outcome.answers_from_block, 0);
    }

    // 守卫：两条相似题干都没有答案时不得互相吞并
    #[test]
    fn does_not_merge_similar_stems_without_answers() {
        let mut items = vec![
            item("Q1", "简述奎尼丁的不良反应", "short_answer", ""),
            item("Q2", "简述奎尼丁的不良反应与禁忌证", "short_answer", ""),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.duplicates, 0);
        assert!(!items[1].removed);
    }
}
