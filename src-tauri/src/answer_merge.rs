//! Link typed answer/explanation blocks to uniquely identified source questions.
//!
//! Stems, option counts and block lengths are deliberately not identities. Ordinary
//! questions are never deduplicated here. Non-question source blocks are excluded
//! from publication, with matched evidence or a serializable quarantine record.
//!
//! Labeled answer tables (`1  2  3` over `B  C  A`, or `1. B`) are parsed from row
//! and column coordinates. Every accepted cell carries its source id, section,
//! row/col and byte spans. A label set such as `4、5` is reported as a set but is
//! never expanded unless the source gives one explicit cell per label; compact
//! strings and ragged rows become located `unsupported_answer_layout` /
//! `ambiguous_multi_label_answer` records instead of guesses. Call
//! [`collect_non_question_candidates`] before merging to hand materials, figures
//! and headings to the typed assembly step; nothing is promoted to a question.

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

    /// Classification without mutation: pre-merge consumers (the typed assembly
    /// step for shared materials) must see the same kinds the merge will use.
    pub fn effective_kind(&self) -> ImportBlockKind {
        // A split/incomplete source cannot be promoted by an old text marker or
        // by a caller-provided kind. Recheck at merge time as metadata is public.
        if self.requires_review() {
            return ImportBlockKind::Unknown;
        }
        if self.has_legacy_answer_marker() {
            return ImportBlockKind::Answer;
        }
        self.kind
    }

    fn has_legacy_answer_marker(&self) -> bool {
        self.raw_text.trim_start().starts_with("[答案区]")
            || self.raw_text.trim_start().starts_with("【答案区】")
            || self
                .json
                .get("content")
                .and_then(Value::as_str)
                .is_some_and(|s| {
                    s.trim_start().starts_with("[答案区]")
                        || s.trim_start().starts_with("【答案区】")
                })
    }

    fn upgrade_legacy_marker(&mut self) {
        self.kind = self.effective_kind();
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
    /// Row/column/byte-span evidence when the entry came from an answer table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<AnswerLayoutIssue>,
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
            layout: None,
        }
    }

    fn from_item_layout(item: &MergeItem, reason: &str, layout: Option<AnswerLayoutIssue>) -> Self {
        let mut entry = Self::from_item(item, reason);
        entry.layout = layout;
        entry
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
    /// Explicit (label, value) answer-table cells that carried their own evidence.
    pub answer_table_cells: usize,
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

fn find_target_by_label(
    label: &str,
    section: &str,
    items: &[MergeItem],
) -> Result<usize, &'static str> {
    let label = normalized_label(label);
    if label.is_empty() {
        return Err("missing_original_label");
    }
    let section = normalized_section(section);
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

fn find_target(item: &MergeItem, items: &[MergeItem]) -> Result<usize, &'static str> {
    if normalized_label(&item.label).is_empty() {
        return Err("missing_original_label");
    }
    if !item.json.is_object() {
        return Err("invalid_answer_payload");
    }
    find_target_by_label(&item.label, &item.section, items)
}

// ---------------------------------------------------------------------------
// Deterministic labeled answer tables: row/column coordinates plus explicit
// labels. A single answer cell may only be used when the source itself states
// the label→value correspondence; equal counts are never treated as evidence.
// ---------------------------------------------------------------------------

/// A cell longer than this is prose, not a table cell.
const MAX_ANSWER_CELL_CHARS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerTableMapping {
    /// `1 B` / `1. B`: the label and its value are adjacent in one cell or row.
    CellPair,
    /// A label row (`1  2  3`) followed by a value row aligned by column.
    ColumnAlignment,
    /// One enumerated label cell (`4、5`) with one distinct value cell per label.
    LabelRun,
}

/// One answer cell. Text is never copied into reports; spans and hashes are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnswerCellEvidence {
    pub source_id: String,
    pub section: String,
    /// 0-based line index inside the answer block's raw text.
    pub row: usize,
    /// 0-based cell index inside that line.
    pub col: usize,
    /// Byte range of the label in raw text.
    pub label_span: [usize; 2],
    /// Byte range of the value in raw text.
    pub value_span: [usize; 2],
    pub label_sha256: String,
    pub value_sha256: String,
    pub mapping: AnswerTableMapping,
}

/// Typed, located layout failure. `evidence_sha256` hashes the offending cell so
/// diagnostics stay useful without duplicating answer text into reports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnswerLayoutIssue {
    pub reason: &'static str,
    pub source_id: String,
    pub section: String,
    pub row: usize,
    pub col: Option<usize>,
    pub span: [usize; 2],
    pub evidence_sha256: String,
    pub parsed_labels: Vec<String>,
}

fn sha256_hex(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[derive(Debug, Clone)]
struct TableCell<'a> {
    text: &'a str,
    /// Absolute byte range inside the answer block's raw text.
    span: [usize; 2],
}

fn push_cell<'a>(
    cells: &mut Vec<TableCell<'a>>,
    line: &'a str,
    start: usize,
    end: usize,
    base: usize,
) {
    let raw = &line[start..end];
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return;
    }
    let offset = raw.len() - raw.trim_start().len();
    let begin = base + start + offset;
    cells.push(TableCell {
        text: trimmed,
        span: [begin, begin + trimmed.len()],
    });
}

/// Split on `|`, tab, ideographic space and runs of two or more spaces. A single
/// space stays inside a cell so `1 B` remains one auditable cell.
fn split_cells(line: &str, base: usize) -> Vec<TableCell<'_>> {
    let mut cells = Vec::new();
    let mut cursor: Option<usize> = None;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        let separator = match ch {
            '\t' | '\u{3000}' | '|' => true,
            ' ' => {
                let mut run = 1;
                while let Some(&(_, ' ')) = chars.peek() {
                    chars.next();
                    run += 1;
                }
                run >= 2
            }
            _ => false,
        };
        if separator {
            if let Some(start) = cursor.take() {
                push_cell(&mut cells, line, start, idx, base);
            }
        } else if cursor.is_none() {
            cursor = Some(idx);
        }
    }
    if let Some(start) = cursor {
        push_cell(&mut cells, line, start, line.len(), base);
    }
    cells
}

fn cell_tokens<'a>(cell: &TableCell<'a>) -> Vec<TableCell<'a>> {
    let mut tokens = Vec::new();
    let mut cursor: Option<usize> = None;
    for (idx, ch) in cell.text.char_indices() {
        if ch.is_whitespace() {
            if let Some(start) = cursor.take() {
                tokens.push(TableCell {
                    text: &cell.text[start..idx],
                    span: [cell.span[0] + start, cell.span[0] + idx],
                });
            }
        } else if cursor.is_none() {
            cursor = Some(idx);
        }
    }
    if let Some(start) = cursor {
        tokens.push(TableCell {
            text: &cell.text[start..],
            span: [cell.span[0] + start, cell.span[0] + cell.text.len()],
        });
    }
    tokens
}

fn range_bounds(part: &str) -> Option<(u32, u32)> {
    for separator in ["-", "~", "～", "—", "–", "至"] {
        if let Some((low, high)) = part.split_once(separator) {
            return Some((low.trim().parse().ok()?, high.trim().parse().ok()?));
        }
    }
    None
}

/// Parse one label cell into its explicit label set. `4、5` and `1-5` are sets;
/// the set is reported as evidence even when the mapping itself is rejected.
fn label_set(text: &str) -> Option<Vec<String>> {
    let mut label = text.trim();
    if label.is_empty() {
        return None;
    }
    label = label.trim_start_matches(['(', '（', '[', '【', '〔']);
    label = label.trim_end_matches([
        ')', '）', ']', '】', '〕', '.', '．', '、', ':', '：', ',', '，',
    ]);
    label = label.trim();
    if let Some(rest) = label.strip_prefix('第') {
        label = rest;
    }
    if let Some(rest) = label.strip_suffix('题') {
        label = rest;
    }
    label = label.trim();
    let label = label
        .strip_prefix(['Q', 'q'])
        .filter(|rest| rest.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or(label);
    if label.is_empty() {
        return None;
    }
    let parts: Vec<&str> = label
        .split(['、', ',', '，', '/', '|'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return None;
    }
    let mut labels = Vec::new();
    for part in parts {
        if let Some((low, high)) = range_bounds(part) {
            if high < low || high - low > 60 {
                return None;
            }
            for number in low..=high {
                labels.push(number.to_string());
            }
        } else if part.chars().all(|c| c.is_ascii_digit()) && part.len() <= 4 {
            labels.push(part.to_string());
        } else {
            return None;
        }
    }
    (!labels.is_empty()).then_some(labels)
}

fn is_answer_value(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() || text.chars().count() > MAX_ANSWER_CELL_CHARS {
        return false;
    }
    if text.contains(['。', '；', ';', '|', '】']) {
        return false;
    }
    text.chars()
        .any(|c| c.is_alphanumeric() || "√×±°′″+-−=<>÷≈≤≥/".contains(c))
}

#[derive(Debug)]
enum RowShape<'a> {
    /// No cell could be read as a label or a value.
    Text,
    /// `1 B`, `1. B  2. C` — every pair is explicit in the source.
    Pairs(Vec<(usize, TableCell<'a>, TableCell<'a>)>),
    /// `4、5  B  D` — one enumerated label cell with one value cell per label.
    LabelRun {
        label: TableCell<'a>,
        labels: Vec<String>,
        values: Vec<TableCell<'a>>,
        start_col: usize,
    },
    /// `1  2  3`
    Header(Vec<TableCell<'a>>),
    /// Row of values under a label header, aligned by column.
    Values(Vec<TableCell<'a>>),
    /// Values without any label row in the same block.
    UnlabeledValues(Vec<TableCell<'a>>),
    Unsolvable {
        reason: &'static str,
        col: Option<usize>,
        span: [usize; 2],
        sha: String,
        labels: Vec<String>,
    },
}

fn classify_cells<'a>(
    cells: Vec<TableCell<'a>>,
    pending_header: bool,
    strong_columns: bool,
) -> RowShape<'a> {
    let cell_count = cells.len();
    let logical: Vec<TableCell<'a>> = cells
        .into_iter()
        .filter(|cell| label_set(cell.text).is_some() || is_answer_value(cell.text))
        .collect();
    if logical.is_empty() {
        return RowShape::Text;
    }
    if logical.len() != cell_count {
        // A long prose cell in a single-cell line is an answer, not a table row.
        if !strong_columns {
            return RowShape::Text;
        }
        // A prose cell next to numbered columns cannot be aligned deterministically.
        return RowShape::Unsolvable {
            reason: "unsupported_answer_layout",
            col: None,
            span: [logical[0].span[0], logical[logical.len() - 1].span[1]],
            sha: sha256_hex(logical[0].text),
            labels: Vec::new(),
        };
    }
    let all_labels = logical.iter().all(|cell| label_set(cell.text).is_some());
    if all_labels {
        return if pending_header {
            RowShape::Values(logical)
        } else {
            RowShape::Header(logical)
        };
    }
    // A row that starts with a value can only be a value row; anything else is
    // an unlabeled value list that cannot be attributed deterministically.
    if label_set(logical[0].text).is_none() {
        return if pending_header {
            RowShape::Values(logical)
        } else if strong_columns {
            RowShape::UnlabeledValues(logical)
        } else {
            // Whitespace-separated tokens without a label are usually prose.
            RowShape::Text
        };
    }
    let mut pairs = Vec::new();
    let mut run: Option<RowShape<'a>> = None;
    let mut index = 0;
    while index < logical.len() {
        let Some(labels) = label_set(logical[index].text) else {
            return RowShape::Unsolvable {
                reason: "unsupported_answer_layout",
                col: Some(index),
                span: logical[index].span,
                sha: sha256_hex(logical[index].text),
                labels: Vec::new(),
            };
        };
        let label = logical[index].clone();
        if labels.len() == 1 {
            let Some(value) = logical.get(index + 1) else {
                return RowShape::Unsolvable {
                    reason: "unsupported_answer_layout",
                    col: Some(index),
                    span: label.span,
                    sha: sha256_hex(label.text),
                    labels,
                };
            };
            if label_set(value.text).is_some() && !is_answer_value(value.text) {
                return RowShape::Unsolvable {
                    reason: "unsupported_answer_layout",
                    col: Some(index + 1),
                    span: value.span,
                    sha: sha256_hex(value.text),
                    labels,
                };
            }
            if !is_answer_value(value.text) {
                return RowShape::Unsolvable {
                    reason: "unsupported_answer_layout",
                    col: Some(index + 1),
                    span: value.span,
                    sha: sha256_hex(value.text),
                    labels,
                };
            }
            pairs.push((index, label, value.clone()));
            index += 2;
        } else {
            let values: Vec<TableCell<'a>> = logical
                .iter()
                .skip(index + 1)
                .take(labels.len())
                .cloned()
                .collect();
            if values.len() < labels.len() {
                return RowShape::Unsolvable {
                    reason: "ambiguous_multi_label_answer",
                    col: Some(index),
                    span: label.span,
                    sha: sha256_hex(label.text),
                    labels,
                };
            }
            if index + 1 + labels.len() != logical.len() {
                return RowShape::Unsolvable {
                    reason: "unsupported_answer_layout",
                    col: Some(index),
                    span: label.span,
                    sha: sha256_hex(label.text),
                    labels,
                };
            }
            if run.is_some() || !pairs.is_empty() {
                return RowShape::Unsolvable {
                    reason: "unsupported_answer_layout",
                    col: Some(index),
                    span: label.span,
                    sha: sha256_hex(label.text),
                    labels,
                };
            }
            let start_col = index + 1;
            run = Some(RowShape::LabelRun {
                label,
                labels,
                values,
                start_col,
            });
            index = logical.len();
        }
    }
    match run {
        Some(shape) => shape,
        None => RowShape::Pairs(pairs),
    }
}

fn strip_answer_marker(raw: &str) -> (&str, usize) {
    let lead = raw.len() - raw.trim_start().len();
    let rest = &raw[lead..];
    for marker in ["[答案区]", "【答案区】"] {
        if rest.starts_with(marker) {
            let consumed = lead + marker.len();
            return (&raw[consumed..], consumed);
        }
    }
    (raw, 0)
}

fn lines_with_offsets(text: &str) -> Vec<(usize, &str)> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            lines.push((start, &text[start..index]));
            start = index + 1;
        }
    }
    lines.push((start, &text[start..]));
    lines
}

#[derive(Debug, Clone)]
struct AnswerAtom {
    label: String,
    value: String,
    evidence: AnswerCellEvidence,
}

#[derive(Debug, Default)]
struct ParsedAnswerTable {
    atoms: Vec<AnswerAtom>,
    issues: Vec<AnswerLayoutIssue>,
    /// Whether the block carries real column structure (or an explicit
    /// multi-label cell). Without it a single label/value line stays legacy.
    table_like: bool,
}

fn row_span(cells: &[TableCell<'_>]) -> [usize; 2] {
    [
        cells.first().map(|cell| cell.span[0]).unwrap_or(0),
        cells.last().map(|cell| cell.span[1]).unwrap_or(0),
    ]
}

#[allow(clippy::too_many_arguments)]
fn layout_issue(
    reason: &'static str,
    source_id: &str,
    section: &str,
    row: usize,
    col: Option<usize>,
    span: [usize; 2],
    evidence_sha256: String,
    parsed_labels: Vec<String>,
) -> AnswerLayoutIssue {
    AnswerLayoutIssue {
        reason,
        source_id: source_id.to_string(),
        section: section.to_string(),
        row,
        col,
        span,
        evidence_sha256,
        parsed_labels,
    }
}

fn cell_evidence(
    source_id: &str,
    section: &str,
    row: usize,
    col: usize,
    label: &TableCell<'_>,
    value: &TableCell<'_>,
    mapping: AnswerTableMapping,
) -> AnswerCellEvidence {
    AnswerCellEvidence {
        source_id: source_id.to_string(),
        section: section.to_string(),
        row,
        col,
        label_span: label.span,
        value_span: value.span,
        label_sha256: sha256_hex(label.text),
        value_sha256: sha256_hex(value.text),
        mapping,
    }
}

/// Deterministic parse of one answer block. Only explicit correspondences are
/// emitted: adjacent cell pairs, a value row under a label header, or one
/// enumerated label cell with one distinct value cell per label. Equal counts, a
/// single value for several labels and compact `1-5 BCADB` strings are reported
/// as located issues instead of being expanded by guesswork.
fn parse_answer_table(raw_text: &str, source_id: &str, section: &str) -> ParsedAnswerTable {
    let (text, base) = strip_answer_marker(raw_text);
    let mut rows: Vec<(usize, RowShape<'_>, bool)> = Vec::new();
    for (row, (offset, line)) in lines_with_offsets(text).into_iter().enumerate() {
        let cells = split_cells(line, base + offset);
        if cells.is_empty() {
            continue;
        }
        let strong_columns = cells.len() > 1;
        let shape = if cells.len() == 1 {
            let tokens = cell_tokens(&cells[0]);
            if tokens.len() < 2 {
                RowShape::Text
            } else {
                classify_cells(tokens, false, false)
            }
        } else {
            classify_cells(cells, false, true)
        };
        rows.push((row, shape, strong_columns));
    }

    let table_like = rows.iter().any(|(_, shape, strong_columns)| {
        *strong_columns
            || matches!(
                shape,
                RowShape::Pairs(_)
                    | RowShape::LabelRun { .. }
                    | RowShape::Unsolvable {
                        reason: "ambiguous_multi_label_answer",
                        ..
                    }
            )
    });
    if !table_like {
        // A plain answer such as `B C` or `1. 一段定义` must stay a plain answer.
        return ParsedAnswerTable::default();
    }

    let mut out = ParsedAnswerTable {
        table_like,
        ..ParsedAnswerTable::default()
    };
    let mut pending: Option<(Vec<TableCell<'_>>, usize)> = None;
    for (row, shape, _) in rows {
        let shape = match shape {
            RowShape::Header(cells) if pending.is_some() => RowShape::Values(cells),
            RowShape::UnlabeledValues(cells) if pending.is_some() => RowShape::Values(cells),
            other => other,
        };
        match shape {
            RowShape::Text => {}
            RowShape::Header(cells) => {
                if let Some((previous, previous_row)) = pending.take() {
                    out.issues.push(layout_issue(
                        "unsupported_answer_layout",
                        source_id,
                        section,
                        previous_row,
                        None,
                        row_span(&previous),
                        sha256_hex(previous[0].text),
                        Vec::new(),
                    ));
                }
                pending = Some((cells, row));
            }
            RowShape::Values(values) => match pending.take() {
                Some((header, _)) if header.len() == values.len() => {
                    for (col, (label_cell, value_cell)) in
                        header.iter().zip(values.iter()).enumerate()
                    {
                        let labels = label_set(label_cell.text).unwrap_or_default();
                        if labels.len() != 1 {
                            out.issues.push(layout_issue(
                                "ambiguous_multi_label_answer",
                                source_id,
                                section,
                                row,
                                Some(col),
                                value_cell.span,
                                sha256_hex(value_cell.text),
                                labels,
                            ));
                            continue;
                        }
                        out.atoms.push(AnswerAtom {
                            label: labels[0].clone(),
                            value: value_cell.text.to_string(),
                            evidence: cell_evidence(
                                source_id,
                                section,
                                row,
                                col,
                                label_cell,
                                value_cell,
                                AnswerTableMapping::ColumnAlignment,
                            ),
                        });
                    }
                }
                Some((header, header_row)) => out.issues.push(layout_issue(
                    "unsupported_answer_layout",
                    source_id,
                    section,
                    header_row,
                    None,
                    row_span(&header),
                    sha256_hex(header[0].text),
                    header
                        .iter()
                        .filter_map(|cell| label_set(cell.text))
                        .flatten()
                        .collect(),
                )),
                None => out.issues.push(layout_issue(
                    "unsupported_answer_layout",
                    source_id,
                    section,
                    row,
                    Some(0),
                    row_span(&values),
                    sha256_hex(values[0].text),
                    Vec::new(),
                )),
            },
            RowShape::Pairs(pairs) => {
                if let Some((header, header_row)) = pending.take() {
                    out.issues.push(layout_issue(
                        "unsupported_answer_layout",
                        source_id,
                        section,
                        header_row,
                        None,
                        row_span(&header),
                        sha256_hex(header[0].text),
                        Vec::new(),
                    ));
                }
                for (col, label_cell, value_cell) in pairs {
                    let labels = label_set(label_cell.text).unwrap_or_default();
                    if labels.len() != 1 {
                        out.issues.push(layout_issue(
                            "ambiguous_multi_label_answer",
                            source_id,
                            section,
                            row,
                            Some(col),
                            label_cell.span,
                            sha256_hex(label_cell.text),
                            labels,
                        ));
                        continue;
                    }
                    out.atoms.push(AnswerAtom {
                        label: labels[0].clone(),
                        value: value_cell.text.to_string(),
                        evidence: cell_evidence(
                            source_id,
                            section,
                            row,
                            col,
                            &label_cell,
                            &value_cell,
                            AnswerTableMapping::CellPair,
                        ),
                    });
                }
            }
            RowShape::LabelRun {
                label,
                labels,
                values,
                start_col,
            } => {
                if let Some((header, header_row)) = pending.take() {
                    out.issues.push(layout_issue(
                        "unsupported_answer_layout",
                        source_id,
                        section,
                        header_row,
                        None,
                        row_span(&header),
                        sha256_hex(header[0].text),
                        Vec::new(),
                    ));
                }
                for (offset, value_cell) in values.iter().enumerate() {
                    let Some(target_label) = labels.get(offset) else {
                        continue;
                    };
                    out.atoms.push(AnswerAtom {
                        label: target_label.clone(),
                        value: value_cell.text.to_string(),
                        evidence: cell_evidence(
                            source_id,
                            section,
                            row,
                            start_col + offset,
                            &label,
                            value_cell,
                            AnswerTableMapping::LabelRun,
                        ),
                    });
                }
            }
            RowShape::UnlabeledValues(values) => out.issues.push(layout_issue(
                "unsupported_answer_layout",
                source_id,
                section,
                row,
                Some(0),
                row_span(&values),
                sha256_hex(values[0].text),
                Vec::new(),
            )),
            RowShape::Unsolvable {
                reason,
                col,
                span,
                sha,
                labels,
            } => out.issues.push(layout_issue(
                reason, source_id, section, row, col, span, sha, labels,
            )),
        }
    }
    if let Some((header, header_row)) = pending {
        out.issues.push(layout_issue(
            "unsupported_answer_layout",
            source_id,
            section,
            header_row,
            None,
            row_span(&header),
            sha256_hex(header[0].text),
            Vec::new(),
        ));
    }
    out
}

#[derive(Debug)]
struct FieldProposal {
    donor: usize,
    field: &'static str,
    value: String,
    evidence: Option<AnswerCellEvidence>,
}

#[derive(Debug, Default)]
struct DonorExpansion {
    assignments: Vec<(usize, &'static str, String, Option<AnswerCellEvidence>)>,
    quarantine: Vec<(String, Option<AnswerLayoutIssue>)>,
    table_cells: usize,
}

fn expand_typed_fields(item: &MergeItem, items: &[MergeItem], expansion: &mut DonorExpansion) {
    let values: Vec<(&'static str, String)> = ["answer", "explanation"]
        .into_iter()
        .filter_map(|field| {
            let value = item.payload(field);
            (!value.is_empty()).then(|| (field, value.to_string()))
        })
        .collect();
    if values.is_empty() {
        expansion
            .quarantine
            .push(("empty_answer_payload".to_string(), None));
        return;
    }
    match find_target(item, items) {
        Ok(target) => {
            for (field, value) in values {
                expansion.assignments.push((target, field, value, None));
            }
        }
        Err(reason) => expansion.quarantine.push((reason.to_string(), None)),
    }
}

fn expand_donor(index: usize, items: &[MergeItem]) -> DonorExpansion {
    let item = &items[index];
    let mut expansion = DonorExpansion::default();
    if item.kind != ImportBlockKind::Answer {
        expand_typed_fields(item, items, &mut expansion);
        return expansion;
    }
    let parsed = parse_answer_table(&item.raw_text, &item.source_id, &item.section);
    let donor_label = normalized_label(&item.label);
    let single_explicit = parsed.issues.is_empty()
        && parsed.atoms.len() == 1
        && !donor_label.is_empty()
        && normalized_label(&parsed.atoms[0].label) == donor_label;
    if single_explicit || !parsed.table_like {
        expand_typed_fields(item, items, &mut expansion);
        if single_explicit {
            if let Some(atom) = parsed.atoms.into_iter().next() {
                for assignment in expansion.assignments.iter_mut() {
                    if assignment.1 == "answer" && assignment.3.is_none() {
                        assignment.3 = Some(atom.evidence.clone());
                    }
                }
            }
        }
        return expansion;
    }

    let ParsedAnswerTable { atoms, issues, .. } = parsed;
    let atom_labels: Vec<String> = atoms.iter().map(|atom| atom.label.clone()).collect();
    let verified = item.evidence_verified("answer");
    let mut unverified: Option<AnswerCellEvidence> = None;
    for atom in atoms {
        expansion.table_cells += 1;
        if !verified {
            if unverified.is_none() {
                unverified = Some(atom.evidence.clone());
            }
            continue;
        }
        match find_target_by_label(&atom.label, &item.section, items) {
            Ok(target) => {
                expansion
                    .assignments
                    .push((target, "answer", atom.value, Some(atom.evidence)))
            }
            Err(reason) => expansion.quarantine.push((
                reason.to_string(),
                Some(layout_issue(
                    reason,
                    &atom.evidence.source_id,
                    &atom.evidence.section,
                    atom.evidence.row,
                    Some(atom.evidence.col),
                    atom.evidence.value_span,
                    atom.evidence.value_sha256.clone(),
                    vec![atom.label.clone()],
                )),
            )),
        }
    }
    if let Some(evidence) = unverified {
        expansion.quarantine.push((
            "answer_evidence_not_verified".to_string(),
            Some(layout_issue(
                "answer_evidence_not_verified",
                &evidence.source_id,
                &evidence.section,
                evidence.row,
                Some(evidence.col),
                evidence.value_span,
                evidence.value_sha256,
                Vec::new(),
            )),
        ));
    }
    for issue in issues {
        expansion
            .quarantine
            .push((issue.reason.to_string(), Some(issue)));
    }
    // A block-level explanation is only usable when its own label matches an
    // explicit table cell; otherwise it stays review material.
    let explanation = item.field("explanation").trim();
    if !explanation.is_empty() {
        let exact = !donor_label.is_empty()
            && atom_labels
                .iter()
                .any(|label| normalized_label(label) == donor_label);
        if exact {
            if let Ok(target) = find_target_by_label(&donor_label, &item.section, items) {
                expansion
                    .assignments
                    .push((target, "explanation", explanation.to_string(), None));
            }
        } else {
            expansion
                .quarantine
                .push(("explanation_without_explicit_label".to_string(), None));
        }
    }
    expansion
}

#[derive(Debug, Clone, Serialize)]
pub struct NonQuestionCandidate {
    pub index: usize,
    pub label: String,
    pub block_kind: ImportBlockKind,
    pub section: String,
    pub source_id: String,
    pub raw_text: String,
    pub json: Value,
}

impl NonQuestionCandidate {
    pub fn is_material(&self) -> bool {
        self.block_kind == ImportBlockKind::Material
    }
}

/// Snapshot every non-question candidate before `merge_answer_sections` excludes
/// it, so shared materials, figures and headings can be handed to the typed
/// assembly step instead of being silently dropped. Pure: no mutation.
pub fn collect_non_question_candidates(items: &[MergeItem]) -> Vec<NonQuestionCandidate> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let kind = item.effective_kind();
            (kind != ImportBlockKind::Question).then(|| NonQuestionCandidate {
                index,
                label: item.label.clone(),
                block_kind: kind,
                section: item.section.clone(),
                source_id: item.source_id.clone(),
                raw_text: item.raw_text.clone(),
                json: item.json.clone(),
            })
        })
        .collect()
}

/// Missing scope requires global label uniqueness. Conflicting values never
/// overwrite the original question, including a blank original with two donors.
pub fn merge_answer_sections(items: &mut [MergeItem]) -> MergeOutcome {
    let mut outcome = MergeOutcome::default();
    let mut by_target: BTreeMap<usize, Vec<FieldProposal>> = BTreeMap::new();
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
                let expansion = expand_donor(index, items);
                outcome.answer_table_cells += expansion.table_cells;
                for (target, field, value, evidence) in expansion.assignments {
                    by_target.entry(target).or_default().push(FieldProposal {
                        donor: index,
                        field,
                        value,
                        evidence,
                    });
                }
                for (reason, layout) in expansion.quarantine {
                    outcome.quarantined.push(QuarantinedEntry::from_item_layout(
                        &items[index],
                        &reason,
                        layout,
                    ));
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

    for (target, proposals) in by_target {
        let mut matched_fields: BTreeMap<usize, Vec<&'static str>> = BTreeMap::new();
        let mut matched_evidence: BTreeMap<usize, Vec<AnswerCellEvidence>> = BTreeMap::new();
        let mut conflicting_donors: Vec<usize> = Vec::new();
        let mut rejected_donors: HashSet<(usize, &'static str)> = HashSet::new();
        let mut filled_answer = false;
        for field in ["answer", "explanation"] {
            let field: &'static str = field;
            let original = items[target].field(field).trim().to_string();
            let mut values: Vec<(usize, String, Option<AnswerCellEvidence>)> = Vec::new();
            for proposal in proposals.iter().filter(|proposal| proposal.field == field) {
                if proposal.value.is_empty() {
                    continue;
                }
                // A donor that explicitly failed evidence verification never
                // transfers the field. Other verified fields from the same donor
                // may still be used.
                if !items[proposal.donor].evidence_verified(field) {
                    if rejected_donors.insert((proposal.donor, field)) {
                        outcome.quarantined.push(QuarantinedEntry::from_item(
                            &items[proposal.donor],
                            if field == "answer" {
                                "answer_evidence_not_verified"
                            } else {
                                "explanation_evidence_not_verified"
                            },
                        ));
                    }
                    continue;
                }
                values.push((
                    proposal.donor,
                    proposal.value.clone(),
                    proposal.evidence.clone(),
                ));
            }
            let Some((_, proposed, _)) = values.first() else {
                continue;
            };
            let reference = if original.is_empty() {
                proposed.as_str()
            } else {
                original.as_str()
            };
            let conflict = values.iter().any(|(_, value, _)| value != reference);
            if conflict {
                for (index, incoming, _) in &values {
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
            for (index, _, evidence) in &values {
                matched_fields.entry(*index).or_default().push(field);
                if let Some(evidence) = evidence {
                    matched_evidence
                        .entry(*index)
                        .or_default()
                        .push(evidence.clone());
                }
            }
        }
        if filled_answer {
            outcome.answers_matched += 1;
        }
        let mut donors: Vec<usize> = proposals.iter().map(|proposal| proposal.donor).collect();
        donors.sort_unstable();
        donors.dedup();
        for index in donors {
            if let Some(fields) = matched_fields.get(&index) {
                let mut entry = json!({
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
                });
                if let Some(evidence) = matched_evidence.get(&index) {
                    entry["answer_evidence"] =
                        serde_json::to_value(evidence).unwrap_or(Value::Null);
                }
                outcome.matched.push(entry);
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

    fn table_answer(section: &str, label: &str, raw: &str) -> MergeItem {
        MergeItem::new(label.to_string(), json!({"content":"答案表","answer":null}))
            .with_source(
                ImportBlockKind::Answer,
                section,
                format!("a:table:{section}:{label}"),
            )
            .with_raw_text(raw)
    }

    #[test]
    fn answer_table_header_and_value_row_map_by_column() {
        let mut items: Vec<MergeItem> = (1..=5)
            .map(|n| question("single_choice", &n.to_string(), &format!("题干{n}")))
            .collect();
        items.push(table_answer(
            "single_choice",
            "",
            "参考答案\n1  2  3  4  5\nB  C  A  D  B",
        ));
        let outcome = merge_answer_sections(&mut items);
        assert!(outcome.quarantined.is_empty(), "{:?}", outcome.quarantined);
        assert!(outcome.conflicts.is_empty());
        assert_eq!(outcome.answer_table_cells, 5);
        assert_eq!(outcome.answers_matched, 5);
        assert_eq!(outcome.matched.len(), 5);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(items[2].json["answer"], "A");
        assert_eq!(items[4].json["answer"], "B");
        let evidence = &outcome.matched[0]["answer_evidence"][0];
        assert_eq!(evidence["row"], 2);
        assert_eq!(evidence["col"], 0);
        assert_eq!(evidence["section"], "single_choice");
        assert_eq!(evidence["mapping"], "column_alignment");
        assert!(evidence["label_span"][0].as_u64().unwrap() > 0);
        assert!(
            evidence["value_span"][1].as_u64().unwrap()
                > evidence["value_span"][0].as_u64().unwrap()
        );
        // Text is not copied into evidence; only spans and hashes are.
        assert!(evidence.get("value_text").is_none());
        assert_eq!(evidence["value_sha256"].as_str().map(str::len), Some(64));
    }

    /// Red case from the old production source: a whole answer table arriving
    /// as one block labelled `1` used to be written into question 1 verbatim.
    #[test]
    fn answer_table_block_with_single_vlm_label_still_uses_every_cell() {
        let mut items: Vec<MergeItem> = (1..=5)
            .map(|n| question("single_choice", &n.to_string(), &format!("题干{n}")))
            .collect();
        items.push(table_answer(
            "single_choice",
            "1",
            "1  2  3  4  5\nB  C  A  D  B",
        ));
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 5);
        assert_eq!(outcome.answer_table_cells, 5);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(items[1].json["answer"], "C");
        assert_eq!(items[2].json["answer"], "A");
        assert_eq!(items[3].json["answer"], "D");
        assert_eq!(items[4].json["answer"], "B");
        assert!(outcome.quarantined.is_empty(), "{:?}", outcome.quarantined);
    }

    #[test]
    fn answer_table_labeled_rows_keep_row_col_evidence() {
        let mut items = vec![
            question("short_answer", "1", "第一题"),
            question("short_answer", "2", "第二题"),
            table_answer("short_answer", "", "1. 甲\n2. 乙"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 2);
        assert_eq!(items[0].json["answer"], "甲");
        assert_eq!(items[1].json["answer"], "乙");
        assert_eq!(outcome.matched[0]["answer_evidence"][0]["row"], 0);
        assert_eq!(outcome.matched[1]["answer_evidence"][0]["row"], 1);
        assert_eq!(outcome.matched[1]["answer_evidence"][0]["col"], 0);
        assert_eq!(
            outcome.matched[0]["answer_evidence"][0]["mapping"],
            "cell_pair"
        );
    }

    #[test]
    fn multi_label_cell_with_single_value_is_not_duplicated() {
        let mut items = vec![
            question("single_choice", "4", "第四题"),
            question("single_choice", "5", "第五题"),
            table_answer("single_choice", "", "4、5  B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 0);
        assert_eq!(outcome.answer_table_cells, 0);
        assert!(items[0].json["answer"].is_null());
        assert!(items[1].json["answer"].is_null());
        assert_eq!(outcome.quarantined.len(), 1);
        assert_eq!(
            outcome.quarantined[0].reason,
            "ambiguous_multi_label_answer"
        );
        let layout = outcome.quarantined[0]
            .layout
            .as_ref()
            .expect("located issue");
        assert_eq!(layout.parsed_labels, vec!["4".to_string(), "5".to_string()]);
        assert_eq!(layout.row, 0);
        assert_eq!(layout.col, Some(0));
        assert!(!layout.evidence_sha256.is_empty());
        assert_eq!(layout.evidence_sha256.len(), 64);
    }

    #[test]
    fn compact_range_answer_string_is_not_count_matched() {
        let mut items: Vec<MergeItem> = (1..=5)
            .map(|n| question("single_choice", &n.to_string(), &format!("题干{n}")))
            .collect();
        items.push(table_answer("single_choice", "", "1-5 BCADB"));
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 0);
        assert!(items[..5].iter().all(|item| item.json["answer"].is_null()));
        assert_eq!(
            outcome.quarantined[0].reason,
            "ambiguous_multi_label_answer"
        );
        assert_eq!(
            outcome.quarantined[0]
                .layout
                .as_ref()
                .unwrap()
                .parsed_labels
                .len(),
            5
        );
    }

    #[test]
    fn ragged_answer_table_reports_unsupported_layout_with_location() {
        let mut items = vec![
            question("single_choice", "1", "第一题"),
            table_answer("single_choice", "", "1  B  C"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 0);
        assert!(items[0].json["answer"].is_null());
        assert_eq!(outcome.quarantined.len(), 1);
        assert_eq!(outcome.quarantined[0].reason, "unsupported_answer_layout");
        let layout = outcome.quarantined[0]
            .layout
            .as_ref()
            .expect("located issue");
        assert_eq!(layout.row, 0);
        assert_eq!(layout.col, Some(2));
        assert!(layout.span[1] > layout.span[0]);
        assert_eq!(layout.source_id, "a:table:single_choice:");
        assert_eq!(layout.section, "single_choice");
    }

    #[test]
    fn answer_table_never_crosses_sections() {
        let mut items = vec![
            question("term_definition", "1", "名词一"),
            question("term_definition", "2", "名词二"),
            table_answer("single_choice", "", "1  2\nB  C"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 0);
        assert!(items[..2].iter().all(|item| item.json["answer"].is_null()));
        assert_eq!(outcome.quarantined.len(), 2);
        assert!(outcome
            .quarantined
            .iter()
            .all(|entry| entry.reason == "section_mismatch"));
        assert!(outcome
            .quarantined
            .iter()
            .all(|entry| entry.layout.is_some()));
    }

    #[test]
    fn answer_table_duplicate_labels_across_sections_are_blocked() {
        let mut items = vec![
            question("chapter1/single_choice", "1", "第一章第一题"),
            question("chapter2/single_choice", "1", "第二章第一题"),
            table_answer("", "", "1  2\nB  C"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(outcome.answers_matched, 0);
        assert!(items[..2].iter().all(|item| item.json["answer"].is_null()));
        assert!(outcome
            .quarantined
            .iter()
            .any(|entry| entry.reason == "ambiguous_question_identity"));
        assert!(outcome
            .quarantined
            .iter()
            .any(|entry| entry.reason == "question_not_found"));
    }

    #[test]
    fn answer_table_does_not_overwrite_existing_answer() {
        let mut first = question("single_choice", "1", "第一题");
        first.json["answer"] = json!("D");
        let mut items = vec![
            first,
            question("single_choice", "2", "第二题"),
            table_answer("single_choice", "", "1  2\nB  C"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "D");
        assert_eq!(items[1].json["answer"], "C");
        assert_eq!(outcome.answers_matched, 1);
        assert_eq!(outcome.conflicts.len(), 1);
        assert!(outcome
            .quarantined
            .iter()
            .any(|entry| entry.reason == "conflicting_answer_or_explanation"));
    }

    #[test]
    fn answer_table_never_promotes_answer_or_material_to_question() {
        let mut items = vec![
            question("single_choice", "1", "第一题"),
            MergeItem::new("".into(), json!({"content":"共享材料正文"})).with_source(
                ImportBlockKind::Material,
                "single_choice",
                "m1",
            ),
            table_answer("single_choice", "", "1  2\nB  C"),
        ];
        let candidates = collect_non_question_candidates(&items);
        assert!(candidates.iter().any(|candidate| candidate.is_material()
            && candidate.source_id == "m1"
            && candidate.raw_text.is_empty()
            && candidate.json["content"] == "共享材料正文"));
        let outcome = merge_answer_sections(&mut items);
        assert!(outcome.excluded.iter().any(|entry| entry.source_id == "m1"));
        assert_eq!(items[1].kind, ImportBlockKind::Material);
        assert_eq!(items[2].kind, ImportBlockKind::Answer);
        assert!(items[1].removed && items[2].removed);
        assert_eq!(items.iter().filter(|item| !item.removed).count(), 1);
    }

    #[test]
    fn non_question_candidates_are_exposed_before_exclusion() {
        let mut items = vec![
            question("", "1", "第一题"),
            MergeItem::new("".into(), json!({"content":"一、选择题"})).with_source(
                ImportBlockKind::Heading,
                "",
                "h1",
            ),
            MergeItem::new("".into(), json!({"content":"无法识别"})).with_source(
                ImportBlockKind::Unknown,
                "",
                "u1",
            ),
            MergeItem::new("1".into(), json!({"content":"摘要","answer":"B"}))
                .with_source(ImportBlockKind::Question, "", "old")
                .with_raw_text("[答案区] 1. 答案：B"),
        ];
        let candidates = collect_non_question_candidates(&items);
        assert_eq!(candidates.len(), 3);
        assert!(items.iter().all(|item| !item.removed));
        assert!(candidates
            .iter()
            .all(|candidate| candidate.block_kind != ImportBlockKind::Question));
        assert!(candidates
            .iter()
            .any(|candidate| candidate.block_kind == ImportBlockKind::Unknown
                && candidate.source_id == "u1"));
        // A legacy marker block is reported as an answer candidate, never a question.
        assert!(candidates
            .iter()
            .any(|candidate| candidate.source_id == "old"
                && candidate.block_kind == ImportBlockKind::Answer));

        let outcome = merge_answer_sections(&mut items);
        // Unknown furniture is retained for review, not silently dropped.
        assert!(outcome
            .quarantined
            .iter()
            .any(|entry| entry.source_id == "u1" && entry.reason == "unknown_block_kind"));
        assert_eq!(items[0].json["answer"], "B");
    }

    #[test]
    fn single_label_answer_block_keeps_legacy_behavior() {
        let mut items = vec![
            question("single_choice", "1", "题干"),
            answer("single_choice", "1", "B"),
        ];
        let outcome = merge_answer_sections(&mut items);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(outcome.answers_matched, 1);
        assert_eq!(outcome.answer_table_cells, 0);
        assert_eq!(
            outcome.matched[0]["answer_evidence"][0]["mapping"],
            "cell_pair"
        );
    }

    #[test]
    fn rerun_after_table_merge_is_idempotent_and_keeps_user_edits() {
        let mut items: Vec<MergeItem> = (1..=3)
            .map(|n| question("single_choice", &n.to_string(), &format!("题干{n}")))
            .collect();
        items.push(table_answer("single_choice", "", "1  2  3\nB  C  A"));
        let first = merge_answer_sections(&mut items);
        assert_eq!(first.answers_matched, 3);
        items[1].json["answer"] = json!("用户改过");
        let second = merge_answer_sections(&mut items);
        assert_eq!(second.answers_matched, 0);
        assert_eq!(items[0].json["answer"], "B");
        assert_eq!(items[1].json["answer"], "用户改过");
        assert_eq!(items[2].json["answer"], "A");
        assert!(second.conflicts.is_empty());
    }
}
