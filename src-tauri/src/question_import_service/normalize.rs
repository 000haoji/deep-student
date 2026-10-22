//! 版本化、受限的表示规范化（representation canonicalization）与导入基线比较。
//!
//! 设计边界（见 docs/plans/2026-10-21-pr490-issue491-492-improvement-plan.md §4.2）：
//!
//! * 只处理**白名单**内的表示差异：Unicode NFKC、全角/半角标点、空白、LaTeX 数学
//!   定界符 `$`/`\(`/`\[`、`^`/`_` 上下标分组写法、少量无显示语义的排版命令
//!   （`\left`/`\right`/`\mathrm{...}`/`\text{...}`/`\,` 等）。
//! * **上下标角色永不丢失**：Unicode `²`/`₂` 先被编码成 `^{2}`/`_{2}` 再做 NFKC，
//!   因此 `x²`、`x^{2}`、`x^2` 等价，但 `x²`、`x₂`、`x2` 两两不同；
//!   `Ca²⁺`、`$Ca^{2+}$` 等价，`$Ca^{3+}$` 不相等；`m²` 与 `m2`、`$10^{23}$` 与 `1023` 不相等。
//! * 花括号是参数分组，保留为括号：`\frac{1}{2}` 与 `\frac{12}` 不相等；
//!   `x^{12}`（上标 12）与 `x^12`（上标 1 后接普通 2）不相等。LaTeX 命令名必须有词边界，
//!   `\leftarrow` 不会被 `\left` 吞掉。
//! * **不做**语义/代数等价判断：不解方程、不换算单位、不合并不同电荷/指数/系数。
//!   规则未覆盖的写法原样保留，交给复核。
//! * 规范化键带版本号。任何规则变化都必须提升 [`REPRESENTATION_NORMALIZATION_VERSION`]，
//!   旧基线按版本不匹配走保守分支，不能被新算法静默重新解释。
//! * 本模块是纯函数，不依赖数据库、不写用户数据、不读环境。LLM 语义猜测不参与裁决。
//!
//! 该模块同时向 `persistence` 提供字段级三方比较（当前库值 / 上次导入基线 / 本次提案）
//! 的纯函数判定，以及题目类型枚举的严格解析（未知枚举返回 [`QuestionTypeParse::Unsupported`]，
//! 绝不静默映射为 `Other`）。任何跨模块接线由导入服务持有方完成，本模块不反向依赖服务。

use unicode_normalization::UnicodeNormalization;

/// 表示规范化规则版本。
///
/// 与字段基线一起持久化；比较前必须一致，否则按“无可用基线”的保守分支处理。
pub const REPRESENTATION_NORMALIZATION_VERSION: u32 = 1;

/// 带版本的规范化键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalText {
    pub version: u32,
    pub key: String,
}

impl CanonicalText {
    pub fn is_same_representation(&self, other: &CanonicalText) -> bool {
        self.version == other.version && self.key == other.key
    }
}

/// 计算带版本的规范化键。
pub fn canonical(text: &str) -> CanonicalText {
    CanonicalText {
        version: REPRESENTATION_NORMALIZATION_VERSION,
        key: canonical_key(text),
    }
}

/// 计算规范化键字符串（供简单比较/去重使用）。
///
/// 关键顺序：**先把 Unicode 上下标编码成带角色的结构标记，再做 NFKC**。
/// 反过来（先 NFKC）会把 `x²`、`x₂`、`x2` 全部折成 `x2`，抹掉指数/下标/电荷结构，
/// 那属于“用规范化吞掉事实”，本模块明确禁止。
///
/// 结构保留规则：
/// * `^` / `_` 角色始终保留，写成 `^(…)` / `_(…)`；`{}` 是参数分组，写成 `(…)`，
///   因此 `x^{12}`（上标 12）与 `x^12`（上标 1 后面跟普通 2）不相同。
/// * `\frac{1}{2}` 与 `\frac{12}` 因分组不同而不相等。
/// * `\leftarrow` 不能被 `\left` 前缀吞掉（命令名必须有词边界）。
/// * 规则未覆盖的写法原样保留，不猜测。
pub fn canonical_key(text: &str) -> String {
    // 1. 上下标角色先编码（必须在 NFKC 之前）。
    let with_scripts = encode_unicode_scripts(text);
    // 2. NFKC：折叠兼容字符（全角字母/标点、㎏→kg 等），上标字符已被替换。
    let nfkc: String = with_scripts.nfkc().collect();
    let chars: Vec<char> = nfkc.chars().collect();
    // 3. `$` 只在“成对且内容像数学”时才是定界符；价格等不明确上下文原样保留。
    let chars = strip_math_dollar_pairs(&dedupe_dollars(&chars));
    let mut out = String::with_capacity(chars.len());
    canonicalize_chars(&mut out, &chars);
    out
}

/// 把 Unicode 上标/下标连续片段编码成 `^{...}` / `_{...}`（ASCII 内容）。
fn encode_unicode_scripts(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(mapped) = superscript_ascii(c) {
            out.push_str("^{");
            out.push(mapped);
            while let Some(&next) = chars.peek() {
                match superscript_ascii(next) {
                    Some(mapped) => {
                        out.push(mapped);
                        chars.next();
                    }
                    None => break,
                }
            }
            out.push('}');
            continue;
        }
        if let Some(mapped) = subscript_ascii(c) {
            out.push_str("_{");
            out.push(mapped);
            while let Some(&next) = chars.peek() {
                match subscript_ascii(next) {
                    Some(mapped) => {
                        out.push(mapped);
                        chars.next();
                    }
                    None => break,
                }
            }
            out.push('}');
            continue;
        }
        out.push(c);
    }
    out
}

fn superscript_ascii(c: char) -> Option<char> {
    Some(match c {
        '⁰' => '0',
        '¹' => '1',
        '²' => '2',
        '³' => '3',
        '⁴' => '4',
        '⁵' => '5',
        '⁶' => '6',
        '⁷' => '7',
        '⁸' => '8',
        '⁹' => '9',
        '⁺' => '+',
        '⁻' => '-',
        '⁼' => '=',
        '⁽' => '(',
        '⁾' => ')',
        'ⁿ' => 'n',
        'ⁱ' => 'i',
        _ => return None,
    })
}

fn subscript_ascii(c: char) -> Option<char> {
    Some(match c {
        '₀' => '0',
        '₁' => '1',
        '₂' => '2',
        '₃' => '3',
        '₄' => '4',
        '₅' => '5',
        '₆' => '6',
        '₇' => '7',
        '₈' => '8',
        '₉' => '9',
        '₊' => '+',
        '₋' => '-',
        '₌' => '=',
        '₍' => '(',
        '₎' => ')',
        _ => return None,
    })
}

/// `$$`（显示公式）与 `$`（行内公式）在表示上等价：先把连续 `$` 折成一个定界符。
fn dedupe_dollars(chars: &[char]) -> Vec<char> {
    let mut out = Vec::with_capacity(chars.len());
    let mut prev_dollar = false;
    for &c in chars {
        if c == '$' {
            if prev_dollar {
                continue;
            }
            prev_dollar = true;
        } else {
            prev_dollar = false;
        }
        out.push(c);
    }
    out
}

/// 只丢弃「成对且内容像数学」的 `$`；否则保留字面 `$`。
///
/// 目的：`$Ca^{2+}$` 与 `Ca²⁺` 等价，但价格文本 `售价 $5`、`$5 和 $10`
/// 不能被规范化抹成 `售价 5`、`5 和 10`（那是事实改变）。规则未覆盖时保留原样。
fn strip_math_dollar_pairs(chars: &[char]) -> Vec<char> {
    let positions: Vec<usize> = chars
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == '$')
        .map(|(index, _)| index)
        .collect();
    let mut drop = vec![false; chars.len()];
    let mut idx = 0usize;
    while idx + 1 < positions.len() {
        let (open, close) = (positions[idx], positions[idx + 1]);
        if math_like(&chars[open + 1..close]) {
            drop[open] = true;
            drop[close] = true;
            idx += 2;
        } else {
            // 当前 `$` 不像定界符：让下一个 `$` 作为开定界符再试。
            idx += 1;
        }
    }
    chars
        .iter()
        .enumerate()
        .filter(|(index, _)| !drop[*index])
        .map(|(_, c)| *c)
        .collect()
}

/// 数学内容的保守判定：只认 LaTeX 结构信号，或无空白的字母标识符。
///
/// 价格/金额、自然语言片段（`$5-$10`、`$5$`、`$2+3$`、`$5 和 $10`）一律**不**当公式定界，
/// 宁可少合并一种表示，也不把金额这类事实抹平；规则未覆盖时保留原样交给复核。
fn math_like(inner: &[char]) -> bool {
    if inner.is_empty() {
        return false;
    }
    const LATEX_OR_SCRIPT_SIGNALS: &str = "\\^_{}";
    if inner.iter().any(|c| LATEX_OR_SCRIPT_SIGNALS.contains(*c)) {
        return true;
    }
    // 裸标识符（`x` / `AB` / `x2`）算公式；含空白即视为自然语言片段。
    !inner.iter().any(|c| c.is_whitespace()) && inner.iter().any(|c| c.is_ascii_alphabetic())
}

fn canonicalize_chars(out: &mut String, chars: &[char]) {
    let mut i = 0usize;
    while i < chars.len() {
        match chars[i] {
            '\\' => match latex_command(&chars[i + 1..]) {
                Some((consumed, CommandAction::Drop)) => i += 1 + consumed,
                Some((consumed, CommandAction::Literal(text))) => {
                    out.push_str(text);
                    i += 1 + consumed;
                }
                Some((consumed, CommandAction::Wrapper)) => {
                    i += 1 + consumed;
                    if i < chars.len() && chars[i] == '{' {
                        // 包装命令只去掉外壳，组内内容按同一规则规范化（不吞事实）。
                        let (inner_start, inner_end, next) = balanced_group(chars, i);
                        canonicalize_chars(out, &chars[inner_start..inner_end]);
                        i = next;
                    }
                }
                None => {
                    // 未知命令：保留反斜杠与命令名本体（两边写法一致时仍可比较）。
                    out.push('\\');
                    i += 1;
                }
            },
            '^' | '_' => {
                // 角色必须保留：`x²`、`x^{2}`、`x^2` 同为上标 2；`x2` 是普通数字。
                out.push(chars[i]);
                out.push('(');
                i += 1;
                if i < chars.len() && chars[i] == '{' {
                    let (inner_start, inner_end, next) = balanced_group(chars, i);
                    canonicalize_chars(out, &chars[inner_start..inner_end]);
                    i = next;
                } else if i < chars.len() {
                    // 单 token 上下标：一个字符或一条命令
                    let end = token_end(chars, i);
                    canonicalize_chars(out, &chars[i..end]);
                    i = end;
                }
                out.push(')');
            }
            // 裸花括号在正文里是字面字符：不能折叠成圆括号
            // （否则 `x{y}` 会与 `x(y)` 相等，改变事实）。只有命令参数组与
            // 上下标分组（上面两个分支）才按结构折叠为括号。
            '{' | '}' => {
                out.push(chars[i]);
                i += 1;
            }
            c => {
                push_canonical_char(out, c);
                i += 1;
            }
        }
    }
}

/// 单 token 的结束位置（一个字符，或一条 `\command`）。
fn token_end(chars: &[char], start: usize) -> usize {
    if chars[start] != '\\' {
        return (start + 1).min(chars.len());
    }
    let mut idx = start + 1;
    while idx < chars.len() && chars[idx].is_ascii_alphabetic() {
        idx += 1;
    }
    if idx == start + 1 {
        idx = (start + 2).min(chars.len());
    }
    idx
}

/// `chars[start] == '{'`：返回 (组内起点, 组内终点, 闭合后的下一个位置)。
fn balanced_group(chars: &[char], start: usize) -> (usize, usize, usize) {
    let mut depth = 0usize;
    let mut idx = start;
    while idx < chars.len() {
        match chars[idx] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return (start + 1, idx, idx + 1);
                }
            }
            _ => {}
        }
        idx += 1;
    }
    // 未闭合：按到末尾处理（不猜测补全）。
    (start + 1, chars.len(), chars.len())
}

fn push_canonical_char(out: &mut String, c: char) {
    let folded = fold_punctuation(c);
    if !folded.is_whitespace() {
        out.push(folded);
    }
}

/// 白名单标点折叠：只把“同一标点的不同编码”折成同一形式。
fn fold_punctuation(c: char) -> char {
    match c {
        '，' => ',',
        '。' => '.',
        '；' => ';',
        '：' => ':',
        '（' => '(',
        '）' => ')',
        '【' => '[',
        '】' => ']',
        '、' => ',',
        '－' | '—' | '–' | '−' | '﹣' => '-',
        '～' => '~',
        '“' | '”' => '"',
        '‘' | '’' => '\'',
        _ => c,
    }
}

#[derive(Debug, Clone, Copy)]
enum CommandAction {
    /// 纯排版/定界符，直接丢弃。
    Drop,
    /// 只去掉外壳，后续分组内容保留。
    Wrapper,
    /// 等价别名。
    Literal(&'static str),
}

/// 识别白名单内的 LaTeX 命令。
///
/// 返回 `(命令名占用的字符数, 动作)`；未知命令返回 `None`（原样保留）。
/// 字母命令必须有词边界：`\leftarrow` 不能被 `\left` 吞掉，`\textbf` 也不能被 `\text` 吞掉。
fn latex_command(rest: &[char]) -> Option<(usize, CommandAction)> {
    const ALPHA_COMMANDS: [(&str, CommandAction); 13] = [
        ("operatorname", CommandAction::Wrapper),
        ("mathrm", CommandAction::Wrapper),
        ("textrm", CommandAction::Wrapper),
        ("text", CommandAction::Wrapper),
        ("qquad", CommandAction::Drop),
        ("quad", CommandAction::Drop),
        ("left", CommandAction::Drop),
        ("right", CommandAction::Drop),
        ("bigg", CommandAction::Drop),
        ("bigl", CommandAction::Drop),
        ("bigr", CommandAction::Drop),
        ("dfrac", CommandAction::Literal("\\frac")),
        ("tfrac", CommandAction::Literal("\\frac")),
    ];
    for (name, action) in ALPHA_COMMANDS {
        let name_chars: Vec<char> = name.chars().collect();
        if rest.len() < name_chars.len() || rest[..name_chars.len()] != name_chars[..] {
            continue;
        }
        // 词边界：命令名后面不能再跟 ASCII 字母。
        if rest
            .get(name_chars.len())
            .is_some_and(|c| c.is_ascii_alphabetic())
        {
            continue;
        }
        return Some((name_chars.len(), action));
    }
    // 非字母命令：`\,` `\;` `\:` `\!`（排版空白）与数学定界符 `\(` `\)` `\[` `\]`。
    match rest.first() {
        Some(',') | Some(';') | Some(':') | Some('!') | Some('(') | Some(')') | Some('[')
        | Some(']') => Some((1, CommandAction::Drop)),
        // `\\`（换行）与 `\ `（强制空白）同样是纯排版命令。
        Some('\\') | Some(' ') => Some((1, CommandAction::Drop)),
        _ => None,
    }
}

/// 去掉题干前的题号前缀（`9．` / `12.` / `3、`）。
///
/// 只接受 1~3 位数字加明确分隔符；其他写法原样返回，不猜。
pub fn strip_question_number(text: &str) -> String {
    let trimmed = text.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    let mut idx = 0usize;
    while idx < chars.len() && idx < 3 && chars[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == 0 || idx >= chars.len() {
        return trimmed.to_string();
    }
    let separator = chars[idx];
    if !matches!(separator, '．' | '.' | '、' | ')' | '）' | ':' | '：') {
        return trimmed.to_string();
    }
    let rest: String = chars[idx + 1..].iter().collect();
    let rest = rest.trim_start();
    if rest.is_empty() {
        trimmed.to_string()
    } else {
        rest.to_string()
    }
}

/// 去掉明确的“（考点：…）”导入器元数据，不删除一般括号/负号/子句。
pub fn strip_topic_metadata(text: &str) -> String {
    let mut stem = text.to_string();
    for (prefix, close) in [
        ("（考点：", '）'),
        ("（考点:", '）'),
        ("(考点：", ')'),
        ("(考点:", ')'),
    ] {
        while let Some(start) = stem.find(prefix) {
            let Some(offset) = stem[start + prefix.len()..].find(close) else {
                break;
            };
            stem.replace_range(start..start + prefix.len() + offset + close.len_utf8(), "");
        }
    }
    stem
}

/// 题干规范化键：题号 / 考点元数据 / 表示差异都不算变更，正文事实仍逐字保留。
pub fn canonical_stem_key(text: &str) -> String {
    canonical_key(&strip_topic_metadata(&strip_question_number(text)))
}

// ============================================================================
// 题目类型枚举的严格解析
// ============================================================================

/// 与导入契约一致的题型集合（对应 `crate::vfs::repos::QuestionType`）。
///
/// 保持独立定义，避免本模块依赖数据库层；接线的服务负责一对一映射。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalQuestionType {
    SingleChoice,
    MultipleChoice,
    IndefiniteChoice,
    FillBlank,
    ShortAnswer,
    Essay,
    Calculation,
    Proof,
    TrueFalse,
    Matching,
    Ordering,
    Numeric,
    Other,
}

impl CanonicalQuestionType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleChoice => "single_choice",
            Self::MultipleChoice => "multiple_choice",
            Self::IndefiniteChoice => "indefinite_choice",
            Self::FillBlank => "fill_blank",
            Self::ShortAnswer => "short_answer",
            Self::Essay => "essay",
            Self::Calculation => "calculation",
            Self::Proof => "proof",
            Self::TrueFalse => "true_false",
            Self::Matching => "matching",
            Self::Ordering => "ordering",
            Self::Numeric => "numeric",
            Self::Other => "other",
        }
    }

    pub fn is_choice(self) -> bool {
        matches!(
            self,
            Self::SingleChoice | Self::MultipleChoice | Self::IndefiniteChoice
        )
    }
}

/// 题型解析结果。
///
/// 关键契约：`Unsupported` 表示模型/数据给出的枚举值不在契约内，
/// **不能**再当作 `Other` 静默接受（那会把类型错配藏进“合法 Other”）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuestionTypeParse {
    Known(CanonicalQuestionType),
    /// 字段缺失或空白：调用方必须自己显式决定策略（不得静默当 Other）。
    Missing,
    /// 非空但不在契约内；`raw` 已截断，供报告定位使用（不含答案正文）。
    Unsupported {
        raw: String,
    },
}

impl QuestionTypeParse {
    pub fn known(self) -> Option<CanonicalQuestionType> {
        match self {
            Self::Known(kind) => Some(kind),
            _ => None,
        }
    }
}

/// 契约内的题型枚举/中文同义词。
pub fn parse_question_type(raw: Option<&str>) -> QuestionTypeParse {
    let Some(raw) = raw else {
        return QuestionTypeParse::Missing;
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return QuestionTypeParse::Missing;
    }
    let lowered = trimmed.to_lowercase();
    let kind = match lowered.as_str() {
        "single_choice" | "单选" | "单选题" => CanonicalQuestionType::SingleChoice,
        "multiple_choice" | "多选" | "多选题" => CanonicalQuestionType::MultipleChoice,
        "indefinite_choice" | "不定项" | "不定项选择" => {
            CanonicalQuestionType::IndefiniteChoice
        }
        "fill_blank" | "填空" | "填空题" => CanonicalQuestionType::FillBlank,
        "short_answer" | "简答" | "简答题" => CanonicalQuestionType::ShortAnswer,
        "essay" | "论述" | "论述题" => CanonicalQuestionType::Essay,
        "calculation" | "计算" | "计算题" => CanonicalQuestionType::Calculation,
        "proof" | "证明" | "证明题" => CanonicalQuestionType::Proof,
        "true_false" | "判断" | "判断题" | "judgment" => CanonicalQuestionType::TrueFalse,
        "matching" | "匹配" | "匹配题" | "连线" | "连线题" => {
            CanonicalQuestionType::Matching
        }
        "ordering" | "排序" | "排序题" => CanonicalQuestionType::Ordering,
        "numeric" | "数值" | "数值题" => CanonicalQuestionType::Numeric,
        "other" | "其他" | "其他题型" => CanonicalQuestionType::Other,
        _ => {
            let mut raw = trimmed.to_string();
            if raw.chars().count() > 64 {
                raw = raw.chars().take(64).collect();
            }
            return QuestionTypeParse::Unsupported { raw };
        }
    };
    QuestionTypeParse::Known(kind)
}

// ============================================================================
// 字段级三方比较（当前库值 / 上次导入基线 / 本次提案）
// ============================================================================

/// 字段级更新决策。调用方（持久层）据此决定写入、保留或进入复核，
/// 不得再把“原字符串不同”当作“用户改过”。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldUpdateDecision {
    /// 规范化后与提案一致：无需写入。
    NoChange,
    /// 当前值与基线一致（用户未编辑）：可安全采用本次提案。
    ApplyProposal,
    /// 提案与基线一致、当前值不同：用户已编辑，保留用户值。
    PreserveUserEdit,
    /// 提案为空、当前值非空：不因重导清空已有内容，保留现值和理由。
    PreserveOnEmptyProposal,
    /// 当前值与提案都偏离基线：显式冲突，进入复核。
    ReviewConflict,
    /// 没有可用基线且当前值非空：保守分支，不猜测是否用户编辑，进入复核。
    ReviewLegacyNoBaseline,
}

impl FieldUpdateDecision {
    pub fn writes_proposal(self) -> bool {
        matches!(self, Self::ApplyProposal)
    }

    pub fn needs_review(self) -> bool {
        matches!(self, Self::ReviewConflict | Self::ReviewLegacyNoBaseline)
    }

    /// 归因标签：用于报告/审计，不含字段正文。
    pub fn reason(self) -> &'static str {
        match self {
            Self::NoChange => "canonical_equal",
            Self::ApplyProposal => "user_unchanged_since_baseline",
            Self::PreserveUserEdit => "user_edit_preserved",
            Self::PreserveOnEmptyProposal => "empty_proposal_kept_existing",
            Self::ReviewConflict => "baseline_conflict",
            Self::ReviewLegacyNoBaseline => "legacy_record_without_baseline",
        }
    }
}

/// 单字段三方比较输入。
#[derive(Debug, Clone, Copy)]
pub struct FieldBaseline<'a> {
    /// 字段标识（`answer` / `explanation` / `content` …），只用于报告。
    pub field: &'a str,
    /// 上次导入成功时该字段的规范化前的原值；None 表示老记录没有基线。
    pub baseline: Option<&'a str>,
    /// 当前数据库里的值。
    pub current: &'a str,
    /// 本次导入提案的值。
    pub proposal: &'a str,
}

/// 判定单字段是否应写入/保留/复核。
///
/// 顺序即优先级：完全等价 → 空提案保护 → 无基线保守分支 → 基线比较。
pub fn decide_field_update(cmp: &FieldBaseline<'_>) -> FieldUpdateDecision {
    let current = canonical_key(cmp.current);
    let proposal = canonical_key(cmp.proposal);
    if current == proposal {
        return FieldUpdateDecision::NoChange;
    }
    if cmp.proposal.trim().is_empty() {
        return FieldUpdateDecision::PreserveOnEmptyProposal;
    }
    match cmp.baseline {
        None => {
            if cmp.current.trim().is_empty() {
                // 空字段没有可被覆盖的用户内容，补全不算语义猜测。
                FieldUpdateDecision::ApplyProposal
            } else {
                FieldUpdateDecision::ReviewLegacyNoBaseline
            }
        }
        Some(baseline) => {
            let baseline = canonical_key(baseline);
            if current == baseline {
                FieldUpdateDecision::ApplyProposal
            } else if proposal == baseline {
                FieldUpdateDecision::PreserveUserEdit
            } else {
                FieldUpdateDecision::ReviewConflict
            }
        }
    }
}

/// 基线版本门禁：缺失或版本不一致都不能与当前算法比较。
pub fn baseline_version_usable(stored: Option<u32>) -> bool {
    stored == Some(REPRESENTATION_NORMALIZATION_VERSION)
}

/// 选项列表的规范化比较（key 大写、content 表示规范化）。
///
/// 仅用于“是否同一表示”的判断；数量/键集不同会同时体现在结果里，由调用方决策。
pub fn canonical_options(options: &[(String, String)]) -> Vec<(String, String)> {
    options
        .iter()
        .map(|(key, content)| (key.trim().to_uppercase(), canonical_key(content)))
        .collect()
}

/// 选项列表是否等价（顺序敏感：选项位置本身是事实）。
pub fn options_equivalent(left: &[(String, String)], right: &[(String, String)]) -> bool {
    canonical_options(left) == canonical_options(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_key_reports_version() {
        let key = canonical("Ca²⁺");
        assert_eq!(key.version, REPRESENTATION_NORMALIZATION_VERSION);
        assert_eq!(canonical_key(" 1 ． 甲 "), "1.甲");
    }

    #[test]
    fn unicode_superscript_equals_latex_superscript_group() {
        // 计划 §4.2 明确要求：Ca²⁺ 与 $Ca^{2+}$ 是同一表示的两种写法。
        assert_eq!(canonical_key("Ca²⁺"), canonical_key("$Ca^{2+}$"));
        assert_eq!(canonical_key("Ca²⁺"), canonical_key("Ca^{2+}"));
        assert_eq!(canonical_key("Fe³⁺"), canonical_key("$\\mathrm{Fe}^{3+}$"));
        assert_eq!(canonical_key("H₂O"), canonical_key("H_{2}O"));
        assert_eq!(canonical_key("\\(x^2\\)"), canonical_key("$x^2$"));
        assert_eq!(canonical_key("x²"), canonical_key("x^{2}"));
        assert_eq!(canonical_key("x²"), canonical_key("x^2"));
    }

    #[test]
    fn script_role_is_never_collapsed_by_nfkc() {
        // 回归：先 NFKC 再删 `^`/`_` 会把 x²、x₂、x2 变成同一个键（事实碰撞）。
        assert_ne!(canonical_key("x²"), canonical_key("x2"));
        assert_ne!(canonical_key("x₂"), canonical_key("x2"));
        assert_ne!(canonical_key("x²"), canonical_key("x₂"));
        assert_ne!(canonical_key("m²"), canonical_key("m2"));
        assert_ne!(canonical_key("$10^{23}$"), canonical_key("1023"));
        assert_ne!(canonical_key("H₂O"), canonical_key("H2O"));
        // 分组歧义：x^{12} 与 x^12 不是同一上标。
        assert_ne!(canonical_key("$x^{12}$"), canonical_key("$x^12$"));
        assert_ne!(canonical_key("$x^{12}$"), canonical_key("$x^{1}2$"));
    }

    #[test]
    fn brace_grouping_is_preserved_for_argument_commands() {
        assert_ne!(canonical_key("\\frac{1}{2}"), canonical_key("\\frac{12}"));
        assert_eq!(
            canonical_key("\\frac{1}{2}"),
            canonical_key("\\dfrac{1}{2}")
        );
        assert_eq!(
            canonical_key("\\frac{1}{2}"),
            canonical_key("$\\frac{1}{2}$")
        );
    }

    #[test]
    fn latex_command_requires_word_boundary() {
        // \leftarrow 不能被 \left 吞掉。
        assert_eq!(canonical_key("$\\leftarrow$"), "\\leftarrow");
        assert_ne!(
            canonical_key("$\\leftarrow$"),
            canonical_key("$\\rightarrow$")
        );
        assert_eq!(canonical_key("$\\left(x\\right)$"), "(x)");
        // \textbf 也不能被 \text 吞掉（未知命令原样保留）。
        assert_ne!(canonical_key("$\\textbf{A}$"), canonical_key("$\\text{A}$"));
        assert_eq!(canonical_key("$\\text{甲}$"), "甲");
        // 未覆盖命令保持原样。
        assert_eq!(canonical_key("$\\alpha\\beta$"), "\\alpha\\beta");
    }

    #[test]
    fn different_charge_and_exponent_facts_are_never_merged() {
        assert_ne!(canonical_key("Ca²⁺"), canonical_key("$Ca^{3+}$"));
        assert_ne!(canonical_key("$Ca^{2+}$"), canonical_key("$Ca^{+}$"));
        assert_ne!(canonical_key("$10^{-3}$"), canonical_key("$10^{-2}$"));
        assert_ne!(canonical_key("x_1"), canonical_key("x_2"));
        assert_ne!(canonical_key("x^{12}"), canonical_key("x^{21}"));
    }

    #[test]
    fn literal_braces_are_not_folded_into_parentheses() {
        // 回归：无条件把 `{}` 折成 `()` 会让 x{y} 与 x(y) 相等（事实碰撞）。
        assert_ne!(canonical_key("x{y}"), canonical_key("x(y)"));
        assert_ne!(canonical_key("{1,2}"), canonical_key("(1,2)"));
        // 裸花括号按字面保留
        assert_eq!(canonical_key("x{y}"), "x{y}");
        assert_eq!(canonical_key("{1,2}"), "{1,2}");
        // 命令参数/上下标分组仍按结构折叠（不吞事实）
        assert_eq!(
            canonical_key("$\\frac{1}{2}$"),
            canonical_key("\\dfrac{1}{2}")
        );
        assert_ne!(canonical_key("\\frac{1}{2}"), canonical_key("\\frac{12}"));
        assert_eq!(canonical_key("Ca²⁺"), canonical_key("$Ca^{2+}$"));
    }

    #[test]
    fn dollar_delimiter_only_when_paired_and_math_like() {
        // 价格文本：单个 `$` 不成对，保留字面，不得抹平事实。
        assert_ne!(canonical_key("售价 $5"), canonical_key("售价 5"));
        // 两个 `$` 但内容是自然语言：不满足“像数学”，同样保留。
        assert_ne!(canonical_key("$5 和 $10"), canonical_key("5 和 10"));
        assert_ne!(
            canonical_key("costs $5 and $6"),
            canonical_key("costs 5 and 6")
        );
        // 纯数字/运算符片段不当公式定界（宁可少合并，也不抹平金额）
        assert_ne!(canonical_key("售价 $5-$10"), canonical_key("售价 5-10"));
        assert_ne!(canonical_key("$5$"), canonical_key("5"));
        assert_ne!(canonical_key("$2+3$"), canonical_key("2+3"));
        assert_eq!(canonical_key("$2\\,\\mathrm{kg}$"), canonical_key("2kg"));
        // 真正的数学定界符：成对且像数学，按同一表示比较。
        assert_eq!(canonical_key("$x$"), canonical_key("x"));
        assert_eq!(canonical_key("$$x^2$$"), canonical_key("$x^2$"));
        assert_eq!(canonical_key("当 $x>0$ 时"), canonical_key("当 x>0 时"));
        assert_ne!(canonical_key("当 $x>0$ 时"), canonical_key("当 x>1 时"));
    }

    #[test]
    fn units_are_never_swallowed() {
        assert_ne!(canonical_key("5 cm"), canonical_key("5 m"));
        assert_ne!(canonical_key("10 mmol/L"), canonical_key("10 mol/L"));
        assert_ne!(
            canonical_key("$2\\,\\mathrm{kg}$"),
            canonical_key("$2\\,\\mathrm{g}$")
        );
        // 排版命令本身不影响事实。
        assert_eq!(canonical_key("$2\\,\\mathrm{kg}$"), canonical_key("2kg"));
    }

    #[test]
    fn punctuation_and_whitespace_variants_are_equivalent() {
        assert_eq!(canonical_key("A．甲"), canonical_key("A. 甲"));
        assert_eq!(canonical_key("甲，乙"), canonical_key("甲,乙"));
        assert_eq!(canonical_key("（一）"), canonical_key("(一)"));
        assert_eq!(canonical_key("a\tb\n c"), canonical_key("a b c"));
        // 负号/减号的表示差异不影响数值事实。
        assert_eq!(canonical_key("−1"), canonical_key("-1"));
        assert_ne!(canonical_key("-1"), canonical_key("1"));
    }

    #[test]
    fn canonicalization_keeps_letters_and_case() {
        // 不能把化学式/选项字母的差异吞掉。
        assert_ne!(canonical_key("Co"), canonical_key("CO"));
        assert_ne!(canonical_key("A"), canonical_key("B"));
    }

    #[test]
    fn stem_key_ignores_number_and_topic_metadata_only() {
        assert_eq!(
            canonical_stem_key("9．奎尼丁与地高辛合用时正确的处理是 （考点：药物相互作用）"),
            canonical_stem_key("奎尼丁与地高辛合用时正确的处理是")
        );
        // 一般括号、负号、子句都不是元数据。
        assert_ne!(canonical_stem_key("(A) 甲"), canonical_stem_key("甲"),);
        assert_ne!(
            canonical_stem_key("求 -1 的值"),
            canonical_stem_key("求 1 的值")
        );
        // 规则未覆盖的写法保持原样。
        assert_eq!(strip_question_number("第9题 甲"), "第9题 甲");
    }

    #[test]
    fn question_type_unknown_enum_is_not_silently_other() {
        assert_eq!(parse_question_type(None), QuestionTypeParse::Missing);
        assert_eq!(parse_question_type(Some("   ")), QuestionTypeParse::Missing);
        assert_eq!(
            parse_question_type(Some("single_choice")),
            QuestionTypeParse::Known(CanonicalQuestionType::SingleChoice)
        );
        assert_eq!(
            parse_question_type(Some("单选题")),
            QuestionTypeParse::Known(CanonicalQuestionType::SingleChoice)
        );
        assert_eq!(
            parse_question_type(Some("other")),
            QuestionTypeParse::Known(CanonicalQuestionType::Other)
        );
        for unsupported in [
            "multiple",
            "choice",
            "single-select",
            "unknown",
            "Other_Choice",
        ] {
            match parse_question_type(Some(unsupported)) {
                QuestionTypeParse::Unsupported { raw } => assert_eq!(raw, unsupported),
                other => panic!("{unsupported} 必须返回 Unsupported，实际 {other:?}"),
            }
        }
        assert!(CanonicalQuestionType::SingleChoice.is_choice());
        assert!(!CanonicalQuestionType::Other.is_choice());
    }

    #[test]
    fn three_way_baseline_decisions() {
        let base =
            |baseline: Option<&'static str>, current: &'static str, proposal: &'static str| {
                decide_field_update(&FieldBaseline {
                    field: "answer",
                    baseline,
                    current,
                    proposal,
                })
            };
        // 只是表示不同：不算变更。
        assert_eq!(
            base(Some("Ca²⁺"), "Ca²⁺", "$Ca^{2+}$"),
            FieldUpdateDecision::NoChange
        );
        // 用户没改：采用提案。
        assert_eq!(
            base(Some("A"), "A", "B"),
            FieldUpdateDecision::ApplyProposal
        );
        // 用户改过：保留用户值。
        assert_eq!(
            base(Some("A"), "C", "A"),
            FieldUpdateDecision::PreserveUserEdit
        );
        // 双方都偏离基线：冲突。
        assert_eq!(
            base(Some("A"), "C", "B"),
            FieldUpdateDecision::ReviewConflict
        );
        // 空提案不清空已有内容。
        assert_eq!(
            base(Some("A"), "A", ""),
            FieldUpdateDecision::PreserveOnEmptyProposal
        );
        // 老记录没有基线：空字段可补全，非空字段只进复核、不当作“用户已改”。
        assert_eq!(base(None, "", "B"), FieldUpdateDecision::ApplyProposal);
        assert_eq!(
            base(None, "A", "B"),
            FieldUpdateDecision::ReviewLegacyNoBaseline
        );
        assert!(!FieldUpdateDecision::ReviewLegacyNoBaseline.writes_proposal());
        assert!(FieldUpdateDecision::ReviewConflict.needs_review());
        assert_eq!(
            FieldUpdateDecision::ReviewLegacyNoBaseline.reason(),
            "legacy_record_without_baseline"
        );
    }

    #[test]
    fn baseline_version_must_match() {
        assert!(baseline_version_usable(Some(
            REPRESENTATION_NORMALIZATION_VERSION
        )));
        assert!(!baseline_version_usable(None));
        assert!(!baseline_version_usable(Some(
            REPRESENTATION_NORMALIZATION_VERSION + 1
        )));
    }

    #[test]
    fn option_equivalence_uses_representation_keys_but_keeps_order() {
        let left = vec![
            ("A".to_string(), "Ca²⁺".to_string()),
            ("B".to_string(), "甲".to_string()),
        ];
        let right = vec![
            ("a".to_string(), "$Ca^{2+}$".to_string()),
            ("b".to_string(), "甲 ".to_string()),
        ];
        assert!(options_equivalent(&left, &right));
        let reordered = vec![
            ("B".to_string(), "甲".to_string()),
            ("A".to_string(), "Ca²⁺".to_string()),
        ];
        assert!(!options_equivalent(&left, &reordered));
        let different = vec![
            ("A".to_string(), "$Ca^{3+}$".to_string()),
            ("B".to_string(), "甲".to_string()),
        ];
        assert!(!options_equivalent(&left, &different));
    }
}
