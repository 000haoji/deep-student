//! 保守的选项边界解析（带源文本证据区间）。
//!
//! 契约（见 docs/plans/2026-10-21-pr490-issue491-492-improvement-plan.md §4.3）：
//!
//! * 同一个解析结果同时服务本地兜底与模型结果校验：本地错误的三项结果不能反过来
//!   否决模型正确的四项；模型也不能凭数量多就改写原文事实。
//! * 标记必须同时满足：标记形态（`A．`/`A.`/`A、`/`A)`/`（A）`）+ A→B→C 连续序列
//!   + 至少两项 + 每项内容非空 + 题干非空 + 位置边界。
//! * 边界分强弱：行首/空白/收尾标点（`）`、`】`、`。` …）为强边界；CJK 文字紧邻
//!   （`A．升高B．降低`）为弱边界，可用但整个结果会被标记为 `ambiguous`，
//!   让调用方走“不以数量作真值”的复核路径，而不是静默采信。
//! * `DNA`、`A.B`、`$x=A+B$`、`A、B、C 三点共线` 等正文写法不拆成选项：
//!   字母后必须紧跟选项分隔符，且枚举串本身不会形成 A→B→C 的连续项。
//! * 输出每个选项的标记区间与内容区间（字节偏移），供报告/复核定位，不复制原文。

use crate::question_import_service::normalize::strip_question_number;

/// 源文本字节区间 `[start, end)`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn slice<'a>(&self, raw: &'a str) -> &'a str {
        raw.get(self.start..self.end).unwrap_or("")
    }

    pub fn as_pair(self) -> (usize, usize) {
        (self.start, self.end)
    }
}

/// 单个已识别的选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOption {
    pub key: char,
    pub content: String,
    /// 标记（字母+分隔符，或整个括号形式）在原文中的区间。
    pub marker_span: Span,
    /// 选项正文在原文中的区间（已去除首尾空白）。
    pub content_span: Span,
    /// 标记前的边界是否为强边界（行首/空白/收尾标点）。
    pub strong_boundary: bool,
}

/// 选项排布形态（仅作诊断信息，不参与接受/拒绝）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionLayout {
    /// 每个标记都独占行首。
    Line,
    /// 同一行内联（`A．甲 B．乙`）或混排。
    Inline,
}

/// 解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedOptions {
    /// 去题号后的题干文本。
    pub stem: String,
    /// 题干在原文中的区间。
    pub stem_span: Span,
    pub options: Vec<ParsedOption>,
    pub layout: OptionLayout,
    /// 存在弱边界、被跳过的空内容项或序列断裂：数量不能作为真值来源。
    pub ambiguous: bool,
    /// 未采用/弱采用的选项字母，供复核报告定位（不含正文）。
    pub flagged_keys: Vec<char>,
}

impl ParsedOptions {
    pub fn keys(&self) -> Vec<char> {
        self.options.iter().map(|option| option.key).collect()
    }

    pub fn key_contents(&self) -> Vec<(char, &str)> {
        self.options
            .iter()
            .map(|option| (option.key, option.content.as_str()))
            .collect()
    }

    pub fn options_span(&self) -> Span {
        match (self.options.first(), self.options.last()) {
            (Some(first), Some(last)) => Span::new(first.marker_span.start, last.content_span.end),
            _ => Span::new(self.stem_span.end, self.stem_span.end),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Marker {
    key: char,
    start: usize,
    content_start: usize,
    strong: bool,
}

/// 解析 `raw` 中的选项列表；无法保守确认时返回 `None`。
///
/// 会依次尝试每个可能的 `A` 标记，取“选项最多、歧义最少”的结果
/// （正文里先出现的 `A．` 可能是题干中的引用）。
pub fn parse_options(raw: &str) -> Option<ParsedOptions> {
    let mut best: Option<ParsedOptions> = None;
    let mut search = 0usize;
    while let Some(first) = find_marker(raw, search, 'A') {
        if let Some(parsed) = build_sequence(raw, first) {
            let better = match &best {
                None => true,
                Some(current) => {
                    (parsed.options.len(), !parsed.ambiguous)
                        > (current.options.len(), !current.ambiguous)
                }
            };
            if better {
                best = Some(parsed);
            }
        }
        let advance = next_char_boundary(raw, first.start);
        if advance <= search {
            break;
        }
        search = advance;
    }
    best
}

fn next_char_boundary(raw: &str, idx: usize) -> usize {
    match raw[idx..].chars().next() {
        Some(c) => idx + c.len_utf8(),
        None => idx + 1,
    }
}

fn build_sequence(raw: &str, first: Marker) -> Option<ParsedOptions> {
    let mut markers = vec![first];
    let mut ambiguous = !first.strong;
    let mut flagged_keys: Vec<char> = if first.strong {
        Vec::new()
    } else {
        vec![first.key]
    };

    while let Some(expected) = next_letter(markers.last()?.key) {
        let last = *markers.last()?;
        let mut search = last.content_start;
        let mut found: Option<Marker> = None;
        while let Some(candidate) = find_marker(raw, search, expected) {
            let preceding_content = raw
                .get(last.content_start..candidate.start)
                .unwrap_or("")
                .trim();
            if candidate.strong || !preceding_content.is_empty() {
                found = Some(candidate);
                break;
            }
            let advance = next_char_boundary(raw, candidate.start);
            if advance <= search {
                break;
            }
            search = advance;
        }
        match found {
            Some(marker) => {
                if !marker.strong {
                    ambiguous = true;
                    flagged_keys.push(marker.key);
                }
                markers.push(marker);
            }
            None => break,
        }
    }

    let mut options: Vec<ParsedOption> = Vec::with_capacity(markers.len());
    for (index, marker) in markers.iter().enumerate() {
        let next_start = markers.get(index + 1).map(|next| next.start);
        // 选项内容只能到“下一个合法标记”为止，即使那个标记的字母跳号：
        // 否则 `A．甲 B．乙 D．丁` 会把 `D．丁` 吞进 B 的内容里（既丢证据又造出假冲突）。
        let stray = find_any_marker(raw, marker.content_start);
        let end = match (next_start, stray) {
            (Some(next), Some(stray)) => next.min(stray.start),
            (Some(next), None) => next,
            (None, Some(stray)) => stray.start,
            (None, None) => raw.len(),
        };
        if let Some(stray) = stray {
            // 跳号/断序列：内容被截断，本轮解析不完整，数量不能当真值。
            if next_start != Some(stray.start) {
                ambiguous = true;
                if !flagged_keys.contains(&stray.key) {
                    flagged_keys.push(stray.key);
                }
            }
        }
        let (content, content_span) = trimmed_span(raw, marker.content_start, end);
        if content.is_empty() {
            // 空内容说明这不是真正的选项边界（或原文缺字）：标记歧义，交由复核。
            ambiguous = true;
            flagged_keys.push(marker.key);
            continue;
        }
        options.push(ParsedOption {
            key: marker.key,
            content,
            marker_span: Span::new(marker.start, marker.content_start),
            content_span,
            strong_boundary: marker.strong,
        });
    }

    if options.len() < 2 {
        return None;
    }
    let stem_span_raw = Span::new(0, markers.first()?.start);
    let (stem_raw, stem_trimmed_span) = trimmed_span(raw, stem_span_raw.start, stem_span_raw.end);
    let stem = strip_question_number(&stem_raw);
    if stem.trim().is_empty() {
        return None;
    }
    // 题干文本在原文中的精确区间（去题号后）
    let stem_span = Span::new(
        stem_trimmed_span.end.saturating_sub(stem.len()),
        stem_trimmed_span.end,
    );
    let layout = if markers
        .iter()
        .all(|marker| at_line_start(raw, marker.start))
    {
        OptionLayout::Line
    } else {
        OptionLayout::Inline
    };
    Some(ParsedOptions {
        stem,
        stem_span,
        options,
        layout,
        ambiguous,
        flagged_keys,
    })
}

fn trimmed_span(raw: &str, start: usize, end: usize) -> (String, Span) {
    let segment = raw.get(start..end).unwrap_or("");
    let leading = segment.len() - segment.trim_start().len();
    let trailing = segment.len() - segment.trim_end().len();
    let inner_start = start + leading;
    let inner_end = end.saturating_sub(trailing).max(inner_start);
    (
        raw.get(inner_start..inner_end).unwrap_or("").to_string(),
        Span::new(inner_start, inner_end),
    )
}

fn at_line_start(raw: &str, idx: usize) -> bool {
    let line_start = raw[..idx].rfind('\n').map(|pos| pos + 1).unwrap_or(0);
    raw[line_start..idx].trim().is_empty()
}

fn next_letter(letter: char) -> Option<char> {
    if letter < 'H' {
        char::from_u32(letter as u32 + 1)
    } else {
        None
    }
}

fn normalized_letter(c: char) -> Option<char> {
    if c.is_ascii_uppercase() {
        return Some(c);
    }
    // 全角字母
    if ('Ａ'..='Ｚ').contains(&c) {
        return char::from_u32(c as u32 - 'Ａ' as u32 + 'A' as u32);
    }
    None
}

/// 选项分隔符（半角/全角）。`A)` / `A）` 也是合法标记。
fn is_separator(c: char) -> bool {
    matches!(c, '.' | '．' | '、' | ')' | '）' | ':' | '：')
}

/// 强边界的“收尾标点”：允许 `C．……）D．……` 这类紧邻写法。
fn is_closing_punctuation(c: char) -> bool {
    matches!(
        c,
        '）' | ')'
            | '】'
            | ']'
            | '」'
            | '』'
            | '”'
            | '’'
            | '。'
            | '！'
            | '？'
            | '!'
            | '?'
            | '…'
            | '；'
            | ';'
            | '，'
            | ','
    )
}

/// 弱边界：CJK 文字紧邻（VLM 常见 `甲B．乙` 连写）。需要额外复核。
fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

fn boundary_kind(raw: &str, idx: usize) -> Option<bool> {
    match raw.get(..idx).and_then(|prefix| prefix.chars().next_back()) {
        None => Some(true),
        Some(c) if c.is_whitespace() => Some(true),
        Some(c) if is_closing_punctuation(c) => Some(true),
        Some(c) if is_cjk(c) => Some(false),
        _ => None,
    }
}

fn marker_at(raw: &str, idx: usize) -> Option<(char, usize)> {
    let first = raw.get(idx..)?.chars().next()?;
    if let Some(letter) = normalized_letter(first) {
        let after_letter = idx + first.len_utf8();
        let separator = raw.get(after_letter..)?.chars().next()?;
        if is_separator(separator) {
            return Some((letter, after_letter + separator.len_utf8()));
        }
        return None;
    }
    if first == '（' || first == '(' {
        let after_open = idx + first.len_utf8();
        let letter = normalized_letter(raw.get(after_open..)?.chars().next()?)?;
        let after_letter = after_open + raw[after_open..].chars().next()?.len_utf8();
        let close = raw.get(after_letter..)?.chars().next()?;
        if close == '）' || close == ')' {
            return Some((letter, after_letter + close.len_utf8()));
        }
    }
    None
}

/// 找 `from` 之后最早的任意选项字母标记（A–H），用于切断上一项的内容区间。
fn find_any_marker(raw: &str, from: usize) -> Option<Marker> {
    if from > raw.len() {
        return None;
    }
    for (offset, _) in raw[from..].char_indices() {
        let idx = from + offset;
        let Some(strong) = boundary_kind(raw, idx) else {
            continue;
        };
        if let Some((key, content_start)) = marker_at(raw, idx) {
            if !strong && raw.get(from..idx).unwrap_or("").trim().is_empty() {
                continue;
            }
            return Some(Marker {
                key,
                start: idx,
                content_start,
                strong,
            });
        }
    }
    None
}

fn find_marker(raw: &str, from: usize, letter: char) -> Option<Marker> {
    if from > raw.len() {
        return None;
    }
    for (offset, _) in raw[from..].char_indices() {
        let idx = from + offset;
        let Some(strong) = boundary_kind(raw, idx) else {
            continue;
        };
        if let Some((key, content_start)) = marker_at(raw, idx) {
            if key == letter {
                return Some(Marker {
                    key,
                    start: idx,
                    content_start,
                    strong,
                });
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_closing_paren_before_marker_is_split_with_evidence() {
        // 计划 §4.3 的复现样本：C 的选项内容以 `）` 结束，D 紧跟其后且前面没有空白。
        let raw = "12．以下关于 ATP 的叙述正确的是\nA．甲\nB．乙\nC．丙（含ATP）D．丁";
        let parsed = parse_options(raw).expect("应拆出四项");
        assert_eq!(parsed.keys(), vec!['A', 'B', 'C', 'D']);
        assert_eq!(parsed.options[2].content, "丙（含ATP）");
        assert_eq!(parsed.options[3].content, "丁");
        assert!(!parsed.ambiguous, "强边界序列不应标记歧义");
        assert!(parsed.flagged_keys.is_empty());
        // 证据区间必须精确指向原文。
        assert_eq!(parsed.options[3].content_span.slice(raw), "丁");
        assert_eq!(parsed.options[2].content_span.slice(raw), "丙（含ATP）");
        assert_eq!(parsed.options[3].marker_span.slice(raw), "D．");
        assert_eq!(parsed.stem_span.slice(raw), "以下关于 ATP 的叙述正确的是");
        // C/D 紧邻跨行混排：D 不在行首，因此是 Inline 混排（布局只作诊断）。
        assert_eq!(parsed.layout, OptionLayout::Inline);
    }

    #[test]
    fn inline_space_separated_markers_split_four() {
        let raw = "1．下列哪项正确 A．甲 B．乙 C．丙 D．丁";
        let parsed = parse_options(raw).expect("应能拆分内联选项");
        assert_eq!(parsed.options.len(), 4);
        assert_eq!(parsed.options[1].content, "乙");
        assert_eq!(parsed.layout, OptionLayout::Inline);
        assert!(!parsed.ambiguous);
        assert!(parsed.stem.contains("下列哪项正确"));
    }

    #[test]
    fn dna_dotted_prose_and_formulas_are_not_split() {
        // DNA：字母后不是分隔符
        assert!(parse_options("2．关于DNA复制方式的叙述，正确的是").is_none());
        // 公式变量：字母前是运算符
        let parsed = parse_options("1．若 $x=A+B$ 且 $a,b>0$，判断下列结论\nA．正确\nB．错误")
            .expect("应识别真正的两项选项");
        assert_eq!(parsed.options.len(), 2);
        assert!(parsed.stem.contains("$x=A+B$"), "stem={}", parsed.stem);
        assert!(!parsed.ambiguous);
        // 正文里的 A.B 引用不能变成选项
        let parsed = parse_options("3．已知集合A.B的关系，判断下列选项\nA．成立\nB．不成立")
            .expect("应识别真正的两项选项");
        assert_eq!(parsed.keys(), vec!['A', 'B']);
        assert!(parsed.stem.contains("A.B"), "stem={}", parsed.stem);
    }

    #[test]
    fn enumerated_letters_in_stem_are_not_options() {
        assert!(parse_options("已知 A、B、C 三点共线，求 AB 长度").is_none());
        assert!(parse_options("名词解释：有效不应期").is_none());
    }

    #[test]
    fn broken_sequence_flags_ambiguity_instead_of_trusting_count() {
        let raw = "7．以下正确的是\nA．甲\nB．乙\nD．丁";
        let parsed = parse_options(raw).expect("仍应给出可复核的两项");
        assert_eq!(parsed.keys(), vec!['A', 'B']);
        assert!(parsed.ambiguous);
        assert!(parsed.flagged_keys.contains(&'D'));
        // 跳号标记不能把 `D．丁` 吞进 B 的内容（否则会造出假冲突、丢证据）。
        assert_eq!(parsed.options[1].content, "乙");
        assert_eq!(parsed.options[1].content_span.slice(raw), "乙");
    }

    #[test]
    fn empty_option_content_flags_ambiguity() {
        let raw = "8．以下正确的是\nA．甲\nB．\nC．丙";
        let parsed = parse_options(raw).expect("应保留有内容的两项");
        assert_eq!(parsed.keys(), vec!['A', 'C']);
        assert!(parsed.ambiguous);
        assert!(parsed.flagged_keys.contains(&'B'));
    }

    #[test]
    fn weak_cjk_boundary_splits_but_marks_ambiguous() {
        // VLM 常见连写：`A．升高B．降低`
        let raw = "9．反应速率变化\nA．升高B．降低C．不变";
        let parsed = parse_options(raw).expect("应拆出三项");
        assert_eq!(parsed.keys(), vec!['A', 'B', 'C']);
        assert!(parsed.ambiguous, "弱边界必须标记为歧义");
        assert_eq!(parsed.options[1].content, "降低");
        assert!(!parsed.options[1].strong_boundary);
    }

    #[test]
    fn requires_two_options_and_a_stem() {
        assert!(parse_options("1．下列哪项正确\nA．甲").is_none());
        assert!(parse_options("A．甲 B．乙").is_none());
        // 只有标记没有正文
        assert!(parse_options("1．题干\nA． B．").is_none());
    }

    #[test]
    fn fullwidth_letters_and_parenthesized_markers() {
        let raw = "4．下列各项\n（Ａ）甲\n（Ｂ）乙";
        let parsed = parse_options(raw).expect("应识别全角括号标记");
        assert_eq!(parsed.keys(), vec!['A', 'B']);
        assert_eq!(parsed.options[0].content, "甲");
        assert!(parsed.options[0].marker_span.slice(raw).starts_with('（'));
    }

    #[test]
    fn letters_beyond_h_are_not_option_markers() {
        assert!(parse_options("5．题干\nI．甲\nJ．乙").is_none());
    }

    #[test]
    fn option_span_is_contiguous_evidence() {
        let raw = "10．题干 A．甲 B．乙";
        let parsed = parse_options(raw).unwrap();
        let span = parsed.options_span();
        assert!(span.slice(raw).starts_with("A．"));
        assert!(span.slice(raw).ends_with("乙"));
        assert!(span.start >= parsed.stem_span.end);
    }
}
