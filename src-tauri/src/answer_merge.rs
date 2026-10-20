//! Link typed answer/explanation blocks to uniquely identified source questions.
//!
//! Stems, option counts and block lengths are deliberately not identities. Ordinary
//! questions are never deduplicated here. Non-question source blocks are excluded
//! from publication, with matched evidence or a serializable quarantine record.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

use crate::vlm_grounding_service::ImportBlockKind;

#[derive(Debug, Clone)]
pub struct MergeItem {
    pub label: String,
    pub json: Value,
    /// Excluded from questions, not permission to discard source evidence.
    pub removed: bool,
    pub kind: ImportBlockKind,
    pub section: String,
    pub source_id: String,
    pub raw_text: String,
}

impl MergeItem {
    pub fn new(label: String, json: Value) -> Self {
        let kind = json
            .get("block_kind")
            .map(|v| serde_json::from_value(v.clone()).unwrap_or(ImportBlockKind::Unknown))
            .unwrap_or_default();
        let string = |key| {
            json.get(key)
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string()
        };
        let mut item = Self {
            label,
            removed: false,
            kind,
            section: string("section"),
            source_id: string("source_id"),
            raw_text: string("raw_text"),
            json,
        };
        item.upgrade_legacy_marker();
        item
    }

    pub fn with_source(
        mut self,
        kind: ImportBlockKind,
        section: impl Into<String>,
        source_id: impl Into<String>,
    ) -> Self {
        self.kind = kind;
        self.section = section.into();
        self.source_id = source_id.into();
        self.upgrade_legacy_marker();
        self
    }

    pub fn with_raw_text(mut self, raw_text: impl Into<String>) -> Self {
        self.raw_text = raw_text.into();
        self.upgrade_legacy_marker();
        self
    }

    fn requires_review(&self) -> bool {
        self.json
            .pointer("/source_metadata/requires_review")
            .and_then(Value::as_bool)
            == Some(true)
            || self.json.get("requires_review").and_then(Value::as_bool) == Some(true)
    }

    fn upgrade_legacy_marker(&mut self) {
        // A split/incomplete source cannot be promoted by an old text marker or
        // by a caller-provided kind. Recheck at merge time as metadata is public.
        if self.requires_review() {
            self.kind = ImportBlockKind::Unknown;
            return;
        }
        if self.raw_text.trim_start().starts_with("[答案区]")
            || self
                .json
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|s| s.trim_start().starts_with("[答案区]"))
        {
            self.kind = ImportBlockKind::Answer;
        }
    }

    fn field(&self, field: &str) -> &str {
        self.json.get(field).and_then(Value::as_str).unwrap_or("")
    }

    fn payload(&self, field: &str) -> &str {
        let explicit = self.field(field).trim();
        if !explicit.is_empty() {
            return explicit;
        }
        let is_primary = (self.kind == ImportBlockKind::Answer && field == "answer")
            || (self.kind == ImportBlockKind::Explanation && field == "explanation");
        if !is_primary {
            return "";
        }
        // A typed answer may have an LLM-generated summary in content. Prefer its
        // original source text over that summary whenever the explicit field is absent.
        self.raw_text.trim().trim_start_matches("[答案区]").trim()
    }

    /// An explicit producer verdict is authoritative per field. `None` keeps
    /// compatibility with visual extraction, where evidence is verified locally
    /// before the candidate is built. A source marked unverified for answers is
    /// also untrusted for explanations unless that field was verified on its own.
    fn evidence_verified(&self, field: &str) -> bool {
        let pointer = format!("/source_metadata/{}_evidence_verified", field);
        match self.json.pointer(&pointer).and_then(Value::as_bool) {
            Some(verified) => verified,
            None if field == "explanation" => {
                self.json
                    .pointer("/source_metadata/answer_evidence_verified")
                    .and_then(Value::as_bool)
                    != Some(false)
            }
            None => true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct QuarantinedEntry {
    pub label: String,
    pub block_kind: ImportBlockKind,
    pub section: String,
    pub source_id: String,
    pub raw_text: String,
    pub json: Value,
    pub reason: String,
}

impl QuarantinedEntry {
    fn from_item(item: &MergeItem, reason: &str) -> Self {
        Self {
            label: item.label.clone(),
            block_kind: item.kind,
            section: item.section.clone(),
            source_id: item.source_id.clone(),
            raw_text: item.raw_text.clone(),
            json: item.json.clone(),
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MergeConflict {
    pub target_label: String,
    pub target_source_id: String,
    pub source_label: String,
    pub source_id: String,
    pub section: String,
    pub field: String,
    pub existing: String,
    pub incoming: String,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct MergeOutcome {
    // Retained for existing progress consumers. Stem deduplication and count-based
    // assignment are intentionally disabled, so these three counters stay zero.
    pub duplicates: usize,
    pub answers_from_duplicates: usize,
    pub answers_from_block: usize,
    pub unmatched_answer_entries: usize,
    pub answers_matched: usize,
    pub matched: Vec<Value>,
    pub quarantined: Vec<QuarantinedEntry>,
    pub excluded: Vec<QuarantinedEntry>,
    pub conflicts: Vec<MergeConflict>,
}

fn normalized_label(label: &str) -> String {
    let mut label = label.trim();
    if label.starts_with(['Q', 'q']) && label[1..].starts_with(|c: char| c.is_ascii_digit()) {
        label = &label[1..];
    }
    label = label.trim_start_matches('第').trim_end_matches('题').trim();
    label = label.trim_matches(|c| matches!(c, '(' | ')' | '（' | '）'));
    label = label.trim_end_matches(['.', '．', '、', ':', '：']).trim();
    match label.to_ascii_lowercase().as_str() {
        "" | "?" | "unknown" | "null" | "none" | "未知" => String::new(),
        _ => label.to_string(),
    }
}

fn normalized_section(section: &str) -> String {
    let scope = section.trim().to_ascii_lowercase();
    if !scope
        .chars()
        .any(|c| c.is_alphabetic() && !"一二三四五六七八九十百千万零〇".contains(c))
    {
        return String::new();
    }
    match scope.as_str() {
        "单选" | "单选题" | "单项选择题" => "single_choice".to_string(),
        "多选" | "多选题" | "多项选择题" => "multiple_choice".to_string(),
        "名词解释" | "名词解释题" => "term_definition".to_string(),
        "简答" | "简答题" => "short_answer".to_string(),
        "" | "unknown" | "未知" => String::new(),
        _ => scope,
    }
}

fn find_target(item: &MergeItem, items: &[MergeItem]) -> Result<usize, &'static str> {
    let label = normalized_label(&item.label);
    if label.is_empty() {
        return Err("missing_original_label");
    }
    if !item.json.is_object() {
        return Err("invalid_answer_payload");
    }
    let section = normalized_section(&item.section);
    let by_label: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, candidate)| {
            !candidate.removed
                && candidate.kind == ImportBlockKind::Question
                && candidate.json.is_object()
                && !candidate.field("content").trim().is_empty()
                && normalized_label(&candidate.label) == label
        })
        .map(|(index, _)| index)
        .collect();
    let exact: Vec<usize> = by_label
        .iter()
        .copied()
        .filter(|&index| normalized_section(&items[index].section) == section)
        .collect();
    // Unknown scope is usable only when the label is unique across the document.
    if !section.is_empty() {
        match exact.as_slice() {
            [target] => return Ok(*target),
            [] => {}
            _ => return Err("ambiguous_question_identity"),
        }
    }
    match by_label.as_slice() {
        [target]
            if section.is_empty() || normalized_section(&items[*target].section).is_empty() =>
        {
            Ok(*target)
        }
        [] => Err("question_not_found"),
        [_] => Err("section_mismatch"),
        _ => Err("ambiguous_question_identity"),
    }
}

/// Link by unique (semantic section, original label), never by stem or position.
/// Missing scope requires global label uniqueness. Conflicting values never
/// overwrite the original question, including a blank original with two donors.
pub fn merge_answer_sections(items: &mut [MergeItem]) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();
    let mut by_target: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for item in items.iter_mut() {
        item.upgrade_legacy_marker();
    }

    for index in 0..items.len() {
        if items[index].removed || items[index].kind == ImportBlockKind::Question {
            continue;
        }
        items[index].removed = true;
        match items[index].kind {
            ImportBlockKind::Heading | ImportBlockKind::Material | ImportBlockKind::Figure => {
                outcome.excluded.push(QuarantinedEntry::from_item(
                    &items[index],
                    "classified_non_question",
                ));
            }
            ImportBlockKind::Answer | ImportBlockKind::Explanation => {
                match find_target(&items[index], items) {
                    Ok(target) => by_target.entry(target).or_default().push(index),
                    Err(reason) => outcome
                        .quarantined
                        .push(QuarantinedEntry::from_item(&items[index], reason)),
                }
            }
            _ => outcome.quarantined.push(QuarantinedEntry::from_item(
                &items[index],
                if items[index].requires_review() {
                    "source_requires_review"
                } else {
                    "unknown_block_kind"
                },
            )),
        }
    }

    for (target, donors) in by_target {
        let mut matched_fields: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        let mut conflicting_donors = Vec::new();
        let mut rejected_donors: HashSet<(usize, &'static str)> = HashSet::new();
        let mut filled_answer = false;
        for field in ["answer", "explanation"] {
            let field: &'static str = field;
            let original = items[target].field(field).trim().to_string();
            let mut values: Vec<(usize, String)> = Vec::new();
            for index in donors.iter().copied() {
                let value = items[index].payload(field);
                if value.is_empty() {
                    continue;
                }
                // A donor that explicitly failed evidence verification never
                // transfers the field. Other verified fields from the same donor
                // may still be used.
                if !items[index].evidence_verified(field) {
                    if rejected_donors.insert((index, field)) {
                        outcome.quarantined.push(QuarantinedEntry::from_item(
                            &items[index],
                            if field == "answer" {
                                "answer_evidence_not_verified"
                            } else {
                                "explanation_evidence_not_verified"
                            },
                        ));
                    }
                    continue;
                }
                values.push((index, value.to_string()));
            }
            let Some((_, proposed)) = values.first() else {
                continue;
            };
            let reference = if original.is_empty() {
                proposed.as_str()
            } else {
                original.as_str()
            };
            let conflict = values.iter().any(|(_, value)| value != reference);
            if conflict {
                for (index, incoming) in &values {
                    if !conflicting_donors.contains(index) {
                        conflicting_donors.push(*index);
                    }
                    outcome.conflicts.push(MergeConflict {
                        target_label: items[target].label.clone(),
                        target_source_id: items[target].source_id.clone(),
                        source_label: items[*index].label.clone(),
                        source_id: items[*index].source_id.clone(),
                        section: items[*index].section.clone(),
                        field: field.to_string(),
                        existing: original.clone(),
                        incoming: incoming.clone(),
                    });
                }
                continue;
            }
            if original.is_empty() {
                items[target].json[field] = Value::String(proposed.clone());
                filled_answer |= field == "answer";
            }
            for (index, _) in values {
                matched_fields.entry(index).or_default().push(field);
            }
        }
        if filled_answer {
            outcome.answers_matched += 1;
        }
        for index in donors {
            if let Some(fields) = matched_fields.get(&index) {
                outcome.matched.push(json!({
                    "target_source_id": items[target].source_id,
                    "target_label": items[target].label,
                    "target_section": items[target].section,
                    "source_id": items[index].source_id,
                    "label": items[index].label,
                    "section": items[index].section,
                    "block_kind": items[index].kind,
                    "raw_text": items[index].raw_text,
                    "json": items[index].json,
                    "matched_fields": fields,
                }));
            }
            if conflicting_donors.contains(&index) {
                outcome.quarantined.push(QuarantinedEntry::from_item(
                    &items[index],
                    "conflicting_answer_or_explanation",
                ));
            } else if !matched_fields.contains_key(&index)
                && !rejected_donors.iter().any(|(donor, _)| *donor == index)
            {
                outcome.quarantined.push(QuarantinedEntry::from_item(
                    &items[index],
                    "empty_answer_payload",
                ));
            }
        }
    }
    outcome.unmatched_answer_entries = outcome
        .quarantined
        .iter()
        .filter(|entry| entry.block_kind.is_answer())
        .count();
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(section: &str, label: &str, content: &str) -> MergeItem {
        MergeItem::new(
            label.into(),
            json!({"content":content,"answer":null,"options":null}),
        )
        .with_source(
            ImportBlockKind::Question,
            section,
            format!("q:{section}:{label}"),
        )
    }
    fn answer(section: &str, label: &str, value: &str) -> MergeItem {
        MergeItem::new(
            label.into(),
            json!({"content":"摘要不应成为题目","answer":value}),
        )
        .with_source(
            ImportBlockKind::Answer,
            section,
            format!("a:{section}:{label}"),
        )
        .with_raw_text(format!("{label}. {value}"))
    }

    #[test]
    fn preserves_same_stem_questions_with_different_options() {
        let mut first = question("single_choice", "1", "下列说法正确的是");
        let mut second = question("single_choice", "2", "下列说法正确的是");
        first.json["options"] = json!([{"key":"A","content":"甲"},{"key":"B","content":"乙"}]);
        second.json["options"] = json!([{"key":"A","content":"丙"},{"key":"B","content":"丁"}]);
        let mut items = vec![first, second];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.duplicates, 0);
        assert!(items.iter().all(|item| !item.removed));
    }

    #[test]
    fn preserves_math_sign_differences() {
        let mut items = vec![
            question("", "1", "求-1的相反数"),
            question("", "2", "求1的相反数"),
        ];
        assert_eq!(merge_answer_sections(&mut items).duplicates, 0);
        assert!(items.iter().all(|item| !item.removed));
    }

    #[test]
    fn never_assigns_answers_by_matching_total_count() {
        let mut items = vec![
            question("single_choice", "1", "第一题"),
            answer("single_choice", "99", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_from_block, 0);
        assert!(items[0].json["answer"].is_null());
        assert_eq!(outcome.quarantined.len(), 1);
        assert!(items[1].removed);
    }

    #[test]
    fn synthetic_38_questions_and_38_answers_publish_only_questions() {
        let scopes = [
            ("single_choice", 20),
            ("term_definition", 10),
            ("short_answer", 8),
        ];
        let mut items = Vec::new();
        for (scope, count) in scopes {
            for n in 1..=count {
                items.push(question(scope, &n.to_string(), &format!("{scope}题{n}")));
            }
        }
        // Answers deliberately reversed, with numbering restarting in each section.
        for (scope, count) in scopes.into_iter().rev() {
            for n in (1..=count).rev() {
                items.push(answer(scope, &n.to_string(), &format!("{scope}答案{n}")));
            }
        }
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items.iter().filter(|item| !item.removed).count(), 38);
        assert_eq!(outcome.answers_matched, 38);
        assert_eq!(outcome.matched.len(), 38);
        assert!(outcome.quarantined.is_empty());
        assert!(outcome.conflicts.is_empty());
        assert_eq!(items[0].json["answer"], "single_choice答案1");
        assert_eq!(items[37].json["answer"], "short_answer答案8");
    }

    #[test]
    fn mixed_20_plus_6_answers_do_not_depend_on_total_count_or_order() {
        let mut items = Vec::new();
        for n in 1..=20 {
            items.push(question("single_choice", &n.to_string(), "选择题"));
        }
        for n in 1..=10 {
            items.push(question("term_definition", &n.to_string(), "名词解释"));
        }
        for n in (1..=6).rev() {
            items.push(answer("term_definition", &n.to_string(), "定义"));
        }
        for n in (1..=20).rev() {
            items.push(answer("single_choice", &n.to_string(), "B"));
        }
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 26);
        assert!(outcome.quarantined.is_empty());
        assert!(items[26].json["answer"].is_null());
    }

    #[test]
    fn chapter_numbering_and_missing_scope_are_not_guessed() {
        let mut items = vec![
            question("chapter1/single_choice", "1", "甲"),
            question("chapter2/single_choice", "1", "乙"),
            answer("", "1", "A"),
            answer("chapter2/single_choice", "1", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined[0].reason, "ambiguous_question_identity");
        assert!(items[0].json["answer"].is_null());
        assert_eq!(items[1].json["answer"], "B");
    }

    #[test]
    fn duplicate_identity_is_quarantined_even_when_section_is_known() {
        let mut items = vec![
            question("single_choice", "1", "甲"),
            question("single_choice", "1", "乙"),
            answer("single_choice", "1", "A"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined.len(), 1);
        assert!(items[..2]
            .iter()
            .all(|item| !item.removed && item.json["answer"].is_null()));
    }

    #[test]
    fn numeric_scopes_never_disambiguate_repeated_labels() {
        let mut items = vec![
            question("1", "1", "甲"),
            question("2", "1", "乙"),
            answer("1", "1", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined.len(), 1);
        assert!(items[..2].iter().all(|item| item.json["answer"].is_null()));
    }

    #[test]
    fn missing_original_label_is_quarantined_not_positionally_matched() {
        let mut items = vec![
            question("single_choice", "1", "题干"),
            answer("single_choice", "", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined[0].reason, "missing_original_label");
        assert!(items[0].json["answer"].is_null());
    }

    #[test]
    fn missing_answer_payload_does_not_use_generated_summary_as_evidence() {
        let mut donor = answer("single_choice", "1", "");
        donor.raw_text.clear();
        let mut items = vec![question("single_choice", "1", "题干"), donor];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined[0].reason, "empty_answer_payload");
        assert!(items[0].json["answer"].is_null());
    }

    #[test]
    fn malformed_explicit_kind_is_unknown_not_question() {
        let mut items = vec![MergeItem::new(
            "1".into(),
            json!({"block_kind":null,"content":"不确定的块"}),
        )];
        let outcome = merge_answer_sections(&mut items);
        assert!(items[0].removed);
        assert_eq!(outcome.quarantined[0].reason, "unknown_block_kind");
    }

    #[test]
    fn known_scope_mismatch_is_not_relaxed_even_for_unique_label() {
        let mut items = vec![
            question("term_definition", "1", "题干"),
            answer("single_choice", "1", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.quarantined[0].reason, "section_mismatch");
        assert!(items[0].json["answer"].is_null());
    }

    #[test]
    fn conflicting_original_answer_is_preserved_and_reported() {
        let mut original = question("single_choice", "1", "题干");
        original.json["answer"] = json!("D");
        let mut items = vec![original, answer("single_choice", "1", "B")];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "D");
        assert_eq!(outcome.conflicts.len(), 1);
        assert_eq!(outcome.quarantined.len(), 1);
        assert!(outcome.matched.is_empty());
    }

    #[test]
    fn conflicting_donors_leave_blank_original_unchanged() {
        let mut items = vec![
            question("single_choice", "1", "题干"),
            answer("single_choice", "1", "B"),
            answer("single_choice", "1", "C"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert!(items[0].json["answer"].is_null());
        assert_eq!(outcome.conflicts.len(), 2);
        assert_eq!(outcome.quarantined.len(), 2);
    }

    #[test]
    fn unscoped_globally_unique_label_can_match_without_overwriting() {
        let mut items = vec![question("", "Q12", "题干"), answer("", "12", "原文答案")];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "原文答案");
        assert_eq!(outcome.matched.len(), 1);
        assert_eq!(outcome.matched[0]["raw_text"], "12. 原文答案");
    }

    #[test]
    fn headings_are_excluded_but_unknown_blocks_need_review() {
        let mut items = vec![
            question("", "1", "题干"),
            MergeItem::new("".into(), json!({"content":"一、选择题"})).with_source(
                ImportBlockKind::Heading,
                "single_choice",
                "h",
            ),
            MergeItem::new("".into(), json!({"content":"无法识别"})).with_source(
                ImportBlockKind::Unknown,
                "",
                "u",
            ),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.excluded.len(), 1);
        assert_eq!(outcome.quarantined.len(), 1);
        assert_eq!(outcome.unmatched_answer_entries, 0);
        assert!(items[1].removed && items[2].removed);
    }

    #[test]
    fn legacy_marker_becomes_typed_and_is_never_published() {
        let mut items = vec![
            question("", "1", "题干"),
            MergeItem::new("1".into(), json!({"content":"答案摘要","answer":"B"}))
                .with_source(ImportBlockKind::Question, "", "old")
                .with_raw_text("[答案区] 1. 答案：B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[1].kind, ImportBlockKind::Answer);
        assert!(items[1].removed);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(outcome.matched.len(), 1);
    }

    #[test]
    fn review_required_legacy_answer_fragment_cannot_fill_a_question() {
        for kind in [ImportBlockKind::Unknown, ImportBlockKind::Answer] {
            let fragment = MergeItem::new(
                "1".into(),
                json!({
                    "block_kind":"unknown", "content":"[答案区] 答案片段",
                    "answer":"B", "source_metadata":{"requires_review":true}
                }),
            )
            .with_source(kind, "single_choice", "fragment:1")
            .with_raw_text("[答案区] 1. 答案片段");
            let mut items = vec![question("single_choice", "1", "完整题干"), fragment];
            let outcome = merge_answer_sections(&mut items);
            assert!(items[0].json["answer"].is_null());
            assert_eq!(items[1].kind, ImportBlockKind::Unknown);
            assert!(items[1].removed);
            assert!(outcome.matched.is_empty());
            assert_eq!(outcome.quarantined.len(), 1);
            assert_eq!(outcome.quarantined[0].reason, "source_requires_review");
        }
    }

    #[test]
    fn merge_entry_rechecks_review_metadata_added_after_construction() {
        let mut fragment = question("single_choice", "1", "截断题干");
        fragment.json["requires_review"] = json!(true);
        let mut items = vec![fragment, answer("single_choice", "1", "B")];
        let outcome = merge_answer_sections(&mut items);
        assert!(items.iter().all(|item| item.removed));
        assert!(outcome.matched.is_empty());
        assert_eq!(outcome.quarantined.len(), 2);
    }

    #[test]
    fn explicitly_unverified_donor_never_transfers_answer() {
        let mut donor = answer("single_choice", "1", "B");
        donor.json["source_metadata"] = json!({"answer_evidence_verified": false});
        let mut items = vec![question("single_choice", "1", "题干"), donor];
        let outcome = merge_answer_sections(&mut items);
        assert!(items[0].json["answer"].is_null());
        assert!(outcome.matched.is_empty());
        assert_eq!(outcome.answers_matched, 0);
        assert_eq!(outcome.quarantined.len(), 1);
        assert_eq!(
            outcome.quarantined[0].reason,
            "answer_evidence_not_verified"
        );
    }

    #[test]
    fn explicitly_unverified_explanation_never_transfers() {
        for metadata in [
            json!({"explanation_evidence_verified": false}),
            // A source that failed answer verification is not trusted to supply
            // an explanation unless that field was verified explicitly.
            json!({"answer_evidence_verified": false}),
        ] {
            let mut donor = answer("short_answer", "1", "");
            donor.kind = ImportBlockKind::Explanation;
            donor.json["explanation"] = json!("模型生成的解析");
            donor.json["source_metadata"] = metadata;
            let mut items = vec![question("short_answer", "1", "题干"), donor];
            let outcome = merge_answer_sections(&mut items);
            assert!(items[0].json["explanation"].is_null());
            assert!(outcome.matched.is_empty());
            assert_eq!(
                outcome.quarantined[0].reason,
                "explanation_evidence_not_verified"
            );
        }
    }

    #[test]
    fn donor_fields_are_gated_independently() {
        let mut donor = answer("short_answer", "1", "未证实的答案");
        donor.json["explanation"] = json!("已证实的解析");
        donor.json["source_metadata"] = json!({
            "answer_evidence_verified": false,
            "explanation_evidence_verified": true,
        });
        let mut items = vec![question("short_answer", "1", "题干"), donor];
        let outcome = merge_answer_sections(&mut items);
        assert!(items[0].json["answer"].is_null());
        assert_eq!(items[0].json["explanation"], "已证实的解析");
        assert_eq!(outcome.answers_matched, 0);
        assert_eq!(outcome.matched.len(), 1);
        assert_eq!(outcome.matched[0]["matched_fields"], json!(["explanation"]));
        assert_eq!(
            outcome.quarantined[0].reason,
            "answer_evidence_not_verified"
        );
    }

    #[test]
    fn explicitly_verified_donor_still_transfers() {
        let mut donor = answer("single_choice", "1", "B");
        donor.json["source_metadata"] = json!({
            "answer_evidence_verified": true,
            "explanation_evidence_verified": true,
        });
        let mut items = vec![question("single_choice", "1", "题干"), donor];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(outcome.answers_matched, 1);
        assert!(outcome.quarantined.is_empty());
    }
}
