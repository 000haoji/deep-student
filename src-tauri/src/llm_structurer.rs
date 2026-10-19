//! Stage 5: LLM Structurer — 将 VLM raw_text 结构化为标准题目 JSON
//!
//! VLM 提供的 raw_text 是题目的原始文本（含题号、选项等），
//! 本阶段使用普通 LLM（非 VLM）将其结构化为标准字段。
//! 图片已在 Stage 4 精确关联，此阶段仅处理文本。
//!
//! 对齐策略：通过 `_source_label` 字段要求 LLM 在输出中回传题号，
//! 然后按 label 匹配（而非位置索引）将结构化结果关联回源题目，
//! 防止 LLM 乱序或拆分/合并题目导致的错位。
//!
//! 降级策略（P0/P1 修复）：
//! - 每批最多 [`BATCH_SIZE`] 题（reasoning 模型下 8 题一批极易触发输出截断）
//! - 解析失败不再直接放弃：先尝试从截断的 JSON 中抢救完整对象，再换更严格的
//!   提示词重试一次
//! - 仍然失败时先做本地选项拆分（`A．xxx B．xxx`），只有连选项都拆不出来才
//!   回退为 `question_type=other` 的原文题目，并在汇总里单独计数

use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};

use crate::figure_extractor::QuestionWithFigures;
use crate::llm_manager::LLMManager;
use crate::models::AppError;

/// 结构化结果来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StructureSource {
    /// LLM 正常返回
    #[default]
    Llm,
    /// LLM 不可用，但本地按选项标记拆分成功（题型/选项可用）
    LocalSplit,
    /// 完全回退为原文（`question_type=other`，选项留在 content 里）
    RawFallback,
}

/// 结构化后的题目
#[derive(Debug, Clone)]
pub struct StructuredQuestion {
    pub json: Value,
    pub source: QuestionWithFigures,
    pub origin: StructureSource,
}

/// 结构化整体结果
#[derive(Debug, Clone, Default)]
pub struct StructureOutcome {
    /// 成功批次的连续前缀长度（断点恢复用）
    pub batches_completed: usize,
    /// 各批次的 LLM 原始结果（失败批次写哨兵）
    pub batch_results: Vec<String>,
    /// 与输入题目一一对应的结构化结果
    pub questions: Vec<StructuredQuestion>,
    pub total: usize,
    /// LLM 结构化成功的题数
    pub llm_structured: usize,
    /// LLM 失败但本地拆分选项成功的题数
    pub local_split: usize,
    /// 只能保留原文的题数
    pub raw_fallback: usize,
}

impl StructureOutcome {
    pub fn degenerate(&self) -> usize {
        self.local_split + self.raw_fallback
    }

    pub fn is_degraded(&self) -> bool {
        self.degenerate() > 0
    }

    fn push_llm(&mut self, qwf: &QuestionWithFigures, value: Value) {
        self.llm_structured += 1;
        self.questions.push(StructuredQuestion {
            json: value,
            source: qwf.clone(),
            origin: StructureSource::Llm,
        });
    }

    fn push_fallback(&mut self, qwf: &QuestionWithFigures) {
        let (value, origin) =
            fallback_json(&qwf.merged.question.raw_text, &qwf.merged.question.label);
        match origin {
            StructureSource::LocalSplit => self.local_split += 1,
            StructureSource::RawFallback => self.raw_fallback += 1,
            StructureSource::Llm => {}
        }
        self.questions.push(StructuredQuestion {
            json: value,
            source: qwf.clone(),
            origin,
        });
    }
}

/// 每批题目数。
///
/// 旧值 8 在 reasoning 模型（如 tree-deepseek-v4.1-flash）下会让单批输出
/// 逼近/超过 max_tokens，生产日志里表现为 `EOF while parsing an object`。
const BATCH_SIZE: usize = 4;

/// 单批最多尝试次数（首次 + 严格提示词重试一次）
const MAX_BATCH_ATTEMPTS: usize = 2;

/// ★ P1-2 修复：批次失败哨兵值。
///
/// 此前 LLM 调用失败的批次被持久化为 `"[]"` 且推进 `completed`，
/// 与"成功但空结果"无法区分，断点恢复永远不再重试，
/// 全批题目静默降级为 minimal JSON。现在失败批次记录此哨兵，
/// 且 `completed`（成功前缀长度）不越过失败批次，恢复时会重试。
const BATCH_FAILED_SENTINEL: &str = "__LLM_BATCH_FAILED__";

/// 判断持久化的批次结果是否为"可复用的成功结果"
/// （非空、非失败哨兵、可解析为非空 JSON 数组）
fn parse_reusable_batch_result(s: &str) -> Option<Vec<Value>> {
    if s.is_empty() || s == BATCH_FAILED_SENTINEL {
        return None;
    }
    match serde_json::from_str::<Vec<Value>>(s) {
        Ok(arr) if !arr.is_empty() => Some(arr),
        _ => None,
    }
}

pub struct LlmStructurer {
    llm_manager: Arc<LLMManager>,
}

impl LlmStructurer {
    pub fn new(llm_manager: Arc<LLMManager>) -> Self {
        Self { llm_manager }
    }

    /// 批量结构化题目列表，每批最多 [`BATCH_SIZE`] 道。
    ///
    /// `prior_batch_results` 为 checkpoint 中已持久化的前 N 批 LLM 结果（JSON 数组字符串）。
    /// 恢复时用真实结果而非 fallback。
    pub async fn structure_questions(
        &self,
        questions: &[QuestionWithFigures],
        model_config_id: Option<&str>,
        batches_completed: usize,
        prior_batch_results: &[String],
    ) -> Result<StructureOutcome, AppError> {
        if questions.is_empty() {
            return Ok(StructureOutcome::default());
        }

        let total = questions.len();
        let mut outcome = StructureOutcome {
            total,
            ..Default::default()
        };
        let mut batch_result_jsons: Vec<String> = prior_batch_results.to_vec();

        let all_batches: Vec<&[QuestionWithFigures]> = questions.chunks(BATCH_SIZE).collect();
        let total_batches = all_batches.len();

        info!(
            "[LlmStructurer] 开始结构化 {} 道题目 ({} 批, 每批 {} 题, 跳过前 {} 批)",
            total, total_batches, BATCH_SIZE, batches_completed
        );

        // 恢复已完成批次：使用持久化的真实 LLM 结果
        for batch_idx in 0..batches_completed.min(total_batches) {
            let batch = all_batches[batch_idx];

            let restored_parsed: Vec<Value> = prior_batch_results
                .get(batch_idx)
                .and_then(|s| parse_reusable_batch_result(s))
                .unwrap_or_default();

            let labels = Self::labels_of(batch);
            let matched = Self::align_by_label(&labels, &restored_parsed);
            for (i, qwf) in batch.iter().enumerate() {
                match matched.get(i).and_then(|m| m.clone()) {
                    Some(value) => outcome.push_llm(qwf, value),
                    None => outcome.push_fallback(qwf),
                }
            }
        }

        let mut completed = batches_completed;
        // ★ P1-2 修复：completed 语义 = 「成功批次的连续前缀长度」。
        // 失败批次不推进 completed（后续成功批次的结果仍持久化，恢复时可直接复用），
        // 保证断点恢复会重试失败批次而非永久降级。
        let mut prefix_all_ok = true;

        for batch_idx in batches_completed..total_batches {
            let batch = all_batches[batch_idx];
            let labels = Self::labels_of(batch);

            // 恢复场景：该批次此前已成功（在失败批次之后被持久化过），直接复用，不再调 LLM
            if let Some(prior_parsed) = prior_batch_results
                .get(batch_idx)
                .and_then(|s| parse_reusable_batch_result(s))
            {
                info!(
                    "[LlmStructurer] 批次 {}/{}: 复用已持久化的成功结果（{} 项）",
                    batch_idx + 1,
                    total_batches,
                    prior_parsed.len()
                );
                let matched = Self::align_by_label(&labels, &prior_parsed);
                for (i, qwf) in batch.iter().enumerate() {
                    match matched.get(i).and_then(|m| m.clone()) {
                        Some(value) => outcome.push_llm(qwf, value),
                        None => outcome.push_fallback(qwf),
                    }
                }
                if prefix_all_ok {
                    completed = batch_idx + 1;
                }
                continue;
            }

            let (parsed, batch_json_str, batch_ok) = self
                .run_batch(batch, model_config_id, batch_idx, total_batches)
                .await;

            let matched = Self::align_by_label(&labels, &parsed);
            let matched_count = matched.iter().filter(|m| m.is_some()).count();

            info!(
                "[LlmStructurer] 批次 {}/{}: LLM 返回 {} 项, 匹配 {} 项",
                batch_idx + 1,
                total_batches,
                parsed.len(),
                matched_count
            );
            if batch_ok && matched_count < batch.len() {
                warn!(
                    "[LlmStructurer] 批次 {}/{} 有 {} 道题目未匹配到结构化结果，改用本地兜底",
                    batch_idx + 1,
                    total_batches,
                    batch.len() - matched_count
                );
            }

            for (i, qwf) in batch.iter().enumerate() {
                match matched.get(i).and_then(|m| m.clone()) {
                    Some(value) => outcome.push_llm(qwf, value),
                    None => outcome.push_fallback(qwf),
                }
            }

            // 持久化此批次的 LLM 原始结果（失败批次持久化哨兵，供恢复时识别并重试）
            while batch_result_jsons.len() <= batch_idx {
                batch_result_jsons.push(String::new());
            }
            batch_result_jsons[batch_idx] = batch_json_str;

            if batch_ok {
                if prefix_all_ok {
                    completed = batch_idx + 1;
                }
            } else {
                prefix_all_ok = false;
            }
        }

        outcome.batches_completed = completed;
        outcome.batch_results = batch_result_jsons;

        info!(
            "[LlmStructurer] 结构化完成: {}/{} 道题目（LLM {}，本地拆选项 {}，原文回退 {}）",
            outcome.questions.len(),
            total,
            outcome.llm_structured,
            outcome.local_split,
            outcome.raw_fallback
        );
        if outcome.degenerate() > 0 {
            warn!(
                "[LlmStructurer] {} 道题目未完成 LLM 结构化（本地兜底 {} 道，原文回退 {} 道，占 {:.0}%）",
                outcome.degenerate(),
                outcome.local_split,
                outcome.raw_fallback,
                outcome.degenerate() as f64 * 100.0 / total as f64
            );
        }

        Ok(outcome)
    }

    /// 调用 LLM 并解析单批结果；失败或结果不完整时用更严格的提示词重试一次。
    async fn run_batch(
        &self,
        batch: &[QuestionWithFigures],
        model_config_id: Option<&str>,
        batch_idx: usize,
        total_batches: usize,
    ) -> (Vec<Value>, String, bool) {
        // 保存"比上一次更好"的不完整结果：重试仍不完整时不至于丢掉已解析的部分
        let mut best: Option<(Vec<Value>, String)> = None;

        for attempt in 0..MAX_BATCH_ATTEMPTS {
            let strict = attempt > 0;
            let prompt = Self::build_batch_prompt(batch, strict);
            match self
                .llm_manager
                .call_llm_for_question_parsing_with_model(&prompt, model_config_id)
                .await
            {
                Ok(response) => {
                    let parsed = Self::parse_llm_response(&response);
                    if parsed.is_empty() {
                        warn!(
                            "[LlmStructurer] 批次 {}/{} 第 {}/{} 次尝试：响应无法解析出题目（{} 字符）",
                            batch_idx + 1,
                            total_batches,
                            attempt + 1,
                            MAX_BATCH_ATTEMPTS,
                            response.chars().count()
                        );
                    } else {
                        let json_str = serde_json::to_string(&parsed).unwrap_or_default();
                        let is_better = best
                            .as_ref()
                            .map(|(previous, _)| parsed.len() > previous.len())
                            .unwrap_or(true);
                        if is_better {
                            best = Some((parsed.clone(), json_str.clone()));
                        }

                        if parsed.len() >= batch.len() {
                            if strict {
                                info!(
                                    "[LlmStructurer] 批次 {}/{} 第 {} 次尝试成功（{} 项）",
                                    batch_idx + 1,
                                    total_batches,
                                    attempt + 1,
                                    parsed.len()
                                );
                            }
                            return (parsed, json_str, true);
                        }

                        // ★ P0 修复：模型只回了部分题目（如 8 题只回 4 项）也重试一次，
                        // 旧实现直接把这半批当成功，剩下 4 题静默走本地兜底。
                        warn!(
                            "[LlmStructurer] 批次 {}/{} 第 {}/{} 次尝试只返回 {} / {} 项，重试一次",
                            batch_idx + 1,
                            total_batches,
                            attempt + 1,
                            MAX_BATCH_ATTEMPTS,
                            parsed.len(),
                            batch.len()
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "[LlmStructurer] 批次 {}/{} 第 {}/{} 次尝试调用失败: {}",
                        batch_idx + 1,
                        total_batches,
                        attempt + 1,
                        MAX_BATCH_ATTEMPTS,
                        e
                    );
                }
            }

            if attempt + 1 < MAX_BATCH_ATTEMPTS {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }

        if let Some((parsed, json_str)) = best {
            warn!(
                "[LlmStructurer] 批次 {}/{} 重试后仍不完整，采用最佳结果（{} / {} 项），其余走本地兜底",
                batch_idx + 1,
                total_batches,
                parsed.len(),
                batch.len()
            );
            return (parsed, json_str, true);
        }

        warn!(
            "[LlmStructurer] 批次 {}/{} 重试后仍失败，本轮回退为本地兜底（checkpoint 不标记完成，恢复时重试）",
            batch_idx + 1,
            total_batches
        );
        (Vec::new(), BATCH_FAILED_SENTINEL.to_string(), false)
    }

    fn labels_of(batch: &[QuestionWithFigures]) -> Vec<String> {
        batch
            .iter()
            .map(|qwf| qwf.merged.question.label.clone())
            .collect()
    }

    /// 按 label 匹配 LLM 输出与源题目（而非位置索引）
    ///
    /// 返回与 `labels` 等长的 `Vec<Option<Value>>`。
    ///
    /// P1 修复：LLM 少输出题目（8 题只回 4 项）时，旧实现因"数量不等"整批丢弃。
    /// 现在若一个 label 都没对上，就按顺序回退，至少把已经解析出来的前 N 题救回来。
    fn align_by_label(labels: &[String], parsed: &[Value]) -> Vec<Option<Value>> {
        let mut result: Vec<Option<Value>> = vec![None; labels.len()];
        // 标记已被消费的 parsed 项：位置回退只取尚未消费的项，避免把已按 label
        // 匹配走的 parsed 重复分配给另一题（label 错位 + 数量恰好相等的场景）。
        let mut used: Vec<bool> = vec![false; parsed.len()];

        // 如果数量完全一致且无 _source_label，按位置回退
        let has_labels = parsed
            .iter()
            .any(|p| p.get("_source_label").and_then(|v| v.as_str()).is_some());

        if !has_labels && parsed.len() == labels.len() {
            for (i, p) in parsed.iter().enumerate() {
                result[i] = Some(p.clone());
            }
            return result;
        }

        // 按 _source_label 匹配
        for (p_idx, p) in parsed.iter().enumerate() {
            let p_label = p
                .get("_source_label")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();

            if p_label.is_empty() {
                continue;
            }

            // 在 batch 中找到匹配的源题目
            for (i, label) in labels.iter().enumerate() {
                if result[i].is_some() {
                    continue;
                }
                if label.trim() == p_label {
                    result[i] = Some(p.clone());
                    used[p_idx] = true;
                    break;
                }
            }
        }

        // ★ P1 修复：一个 label 都没对上（模型改写了题号/丢了 _source_label）时，
        // 旧实现只在"数量相等"时按序回退，数量不等就整批丢弃。这里放宽为：
        // 完全无匹配则按顺序填充，能救几题救几题，其余交给本地兜底。
        let matched_any = result.iter().any(|slot| slot.is_some());
        if !matched_any {
            for (i, slot) in result.iter_mut().enumerate() {
                match parsed.get(i) {
                    Some(p) => *slot = Some(p.clone()),
                    None => break,
                }
            }
            return result;
        }

        // 部分匹配且数量一致：未匹配题槽用「尚未被 label 消费的 parsed 项」依序回退
        if parsed.len() == labels.len() {
            let mut next_unused = 0usize;
            for slot in result.iter_mut() {
                if slot.is_some() {
                    continue;
                }
                while next_unused < parsed.len() && used[next_unused] {
                    next_unused += 1;
                }
                if next_unused >= parsed.len() {
                    break;
                }
                *slot = Some(parsed[next_unused].clone());
                used[next_unused] = true;
                next_unused += 1;
            }
        }

        result
    }

    fn build_batch_prompt(batch: &[QuestionWithFigures], strict: bool) -> String {
        let mut questions_text = String::new();

        for (i, qwf) in batch.iter().enumerate() {
            questions_text.push_str(&format!(
                "--- 题目 {} (label=\"{}\") ---\n{}\n\n",
                i + 1,
                qwf.merged.question.label,
                qwf.merged.question.raw_text
            ));
        }

        let strict_rules = if strict {
            "\n**重要（上一次输出无法解析，请严格遵守）**：\n\
             9. 只输出 JSON 数组本身：不要 markdown 代码块、不要解释、不要前后缀文字\n\
             10. 每个题目对象必须完整闭合后再输出下一个；宁可精简 tags/explanation，也不要中途截断\n\
             11. `options` 必须是对象数组 {\"key\":\"A\",\"content\":\"...\"}，不要输出字符串数组\n\
             12. 数组长度必须严格等于输入题目数量，不要遗漏、不要合并题目\n"
        } else {
            ""
        };

        format!(
            r#"请将以下 {} 道题目的原始文本结构化为标准 JSON 格式。不需要处理图片。

{}
**输出要求**：
输出一个 JSON 数组，每个元素对应一道题目（只输出 JSON，不要其他内容）：

```json
[
  {{
    "_source_label": "1",
    "content": "题干内容（不含选项文本）",
    "question_type": "single_choice|multiple_choice|indefinite_choice|fill_blank|short_answer|essay|calculation|proof|other",
    "options": [
      {{"key": "A", "content": "选项A内容"}},
      {{"key": "B", "content": "选项B内容"}}
    ],
    "answer": "A",
    "explanation": "解析（如有）",
    "difficulty": "easy|medium|hard|very_hard",
    "tags": ["知识点标签"]
  }}
]
```

**规则**：
1. `_source_label` 必须填写原始题号标签（括号中 label= 的值），用于对齐
2. 选择题必须将选项拆分到 options 数组，content 只保留题干
3. 题型: single_choice=单选, multiple_choice=多选, fill_blank=填空, short_answer=简答
4. 公式格式：{latex_brief}
5. 没有答案/解析的字段输出 null
6. difficulty 按下方判定标准逐题判断
7. 输出数组长度必须等于输入题目数量 ({})
8. 若某条原始文本是「答案/解析区」条目（以 `[答案区]` 开头，或只有题号+答案而没有题干/选项），把 `content` 输出为空字符串 ""、`answer` 输出答案正文、`question_type` 输出 "other"，**不要编造题干**
{}{difficulty_rubric}"#,
            batch.len(),
            questions_text,
            batch.len(),
            strict_rules,
            latex_brief = crate::prompt_contracts::LATEX_OUTPUT_BRIEF,
            difficulty_rubric = crate::prompt_contracts::DIFFICULTY_RUBRIC
        )
    }

    fn parse_llm_response(response: &str) -> Vec<Value> {
        let trimmed = response.trim();
        let slice = match (trimmed.find('['), trimmed.rfind(']')) {
            (Some(start), Some(end)) if end > start => &trimmed[start..=end],
            _ => trimmed,
        };

        match serde_json::from_str::<Vec<Value>>(slice) {
            Ok(arr) => arr,
            Err(e) => {
                // ★ P0 修复：输出被 max_tokens 截断时，旧实现整批丢弃。
                // 这里改为从截断文本里逐个抢救已闭合的题目对象。
                let salvaged = salvage_json_objects(slice);
                if salvaged.is_empty() {
                    warn!("[LlmStructurer] 解析 LLM 响应失败: {}", e);
                } else {
                    warn!(
                        "[LlmStructurer] 解析 LLM 响应失败（{}），已从截断内容中抢救出 {} 个题目对象",
                        e,
                        salvaged.len()
                    );
                }
                salvaged
            }
        }
    }
}

/// 从可能被截断的 JSON 文本中逐个抠出完整对象
fn salvage_json_objects(text: &str) -> Vec<Value> {
    let chars: Vec<char> = text.chars().collect();
    let mut salvaged = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i] != '{' {
            i += 1;
            continue;
        }

        let start = i;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        let mut end = None;
        let mut j = i;

        while j < chars.len() {
            let c = chars[j];
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
            } else if c == '"' {
                in_string = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    end = Some(j);
                    break;
                }
            }
            j += 1;
        }

        match end {
            Some(end_idx) => {
                let candidate: String = chars[start..=end_idx].iter().collect();
                if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
                    salvaged.push(value);
                }
                i = end_idx + 1;
            }
            // 未闭合：后面的内容都不可信，停止
            None => break,
        }
    }

    salvaged
}

/// 选项分隔符（半角/全角）
const OPTION_SEPARATORS: [char; 6] = ['．', '.', '、', ')', '）', '：'];

fn normalize_option_letter(c: char) -> Option<char> {
    let upper = c.to_ascii_uppercase();
    if ('A'..='H').contains(&upper) {
        Some(upper)
    } else {
        None
    }
}

fn next_option_letter(letter: char) -> Option<char> {
    if letter < 'H' {
        char::from_u32(letter as u32 + 1)
    } else {
        None
    }
}

/// 解析形如 `A．选项内容` / `（A）选项内容` 的单行
fn parse_option_line(line: &str) -> Option<(char, String)> {
    let trimmed = line.trim_start();
    let mut chars = trimmed.chars();

    let first = chars.next()?;
    if let Some(letter) = normalize_option_letter(first) {
        let rest = chars.as_str();
        let mut rest_chars = rest.chars();
        let separator = rest_chars.next()?;
        if OPTION_SEPARATORS.contains(&separator) {
            return Some((letter, rest_chars.as_str().trim().to_string()));
        }
        return None;
    }

    // 全角/半角括号包裹：`（A）xxx` / `(A) xxx`
    if first == '（' || first == '(' {
        let mut rest_chars = chars.as_str().chars();
        let letter = normalize_option_letter(rest_chars.next()?)?;
        let close = rest_chars.next()?;
        if close == '）' || close == ')' {
            let after = rest_chars.as_str();
            return Some((letter, after.trim().to_string()));
        }
    }

    None
}

/// 把同一行里的 `A．xx B．xx C．xx` 拆成多行，便于复用逐行解析
fn split_inline_option_markers(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 16);
    let mut expected = 'A';
    let mut prev: Option<char> = None;
    let mut chars = raw.chars().peekable();

    while let Some(c) = chars.next() {
        if let Some(letter) = normalize_option_letter(c) {
            let preceded_by_space = prev.map(|p| p.is_whitespace()).unwrap_or(true);
            if letter == expected && preceded_by_space {
                if let Some(next) = chars.peek().copied() {
                    if OPTION_SEPARATORS.contains(&next) {
                        if !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push(c);
                        prev = Some(c);
                        expected = next_option_letter(letter).unwrap_or(letter);
                        continue;
                    }
                }
            }
        }
        out.push(c);
        prev = Some(c);
    }

    out
}

/// 去掉题干前的题号前缀（`9．` / `12.` / `3、`）
fn strip_question_number(text: &str) -> String {
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

/// 从 VLM 原文里本地拆分出题干与选项（不依赖 LLM）
///
/// 命中条件：从 `A` 开始，至少 2 个连续的选项行（或同一行内联标记）。
pub(crate) fn split_options_from_raw_text(raw: &str) -> Option<(String, Vec<Value>)> {
    let normalized = split_inline_option_markers(raw);
    let lines: Vec<&str> = normalized.lines().collect();

    let first_idx = lines
        .iter()
        .position(|line| parse_option_line(line).is_some())?;
    let (first_letter, _) = parse_option_line(lines[first_idx])?;
    if first_letter != 'A' {
        return None;
    }

    let stem_lines: Vec<String> = lines[..first_idx].iter().map(|s| s.to_string()).collect();
    let mut options: Vec<(char, String)> = Vec::new();
    let mut idx = first_idx;
    let mut expected = 'A';

    while idx < lines.len() {
        match parse_option_line(lines[idx]) {
            Some((letter, mut text)) if letter == expected => {
                idx += 1;
                // 选项内容可能换行，拼接到当前选项
                while idx < lines.len() {
                    let candidate = lines[idx];
                    if candidate.trim().is_empty() || parse_option_line(candidate).is_some() {
                        break;
                    }
                    if text.is_empty() {
                        text.push_str(candidate.trim());
                    } else {
                        text.push(' ');
                        text.push_str(candidate.trim());
                    }
                    idx += 1;
                }
                options.push((letter, text.trim().to_string()));
                expected = match next_option_letter(letter) {
                    Some(next) => next,
                    None => break,
                };
            }
            _ => break,
        }
    }

    if options.len() < 2 {
        return None;
    }

    let stem = strip_question_number(&stem_lines.join("\n"));
    if stem.trim().is_empty() {
        return None;
    }

    let options_json: Vec<Value> = options
        .iter()
        .map(|(key, content)| json!({ "key": key.to_string(), "content": content }))
        .collect();

    Some((stem, options_json))
}

fn raw_text_to_minimal_json(raw_text: &str, label: &str) -> Value {
    json!({
        "content": raw_text,
        "question_type": "other",
        "options": null,
        "answer": null,
        "explanation": null,
        "difficulty": "medium",
        "tags": [],
        "_source_label": label
    })
}

/// 兜底 JSON：优先本地拆分选项，拆不出来才整段回退
fn fallback_json(raw_text: &str, label: &str) -> (Value, StructureSource) {
    if let Some((stem, options)) = split_options_from_raw_text(raw_text) {
        let mut value = raw_text_to_minimal_json(raw_text, label);
        value["content"] = json!(stem);
        value["question_type"] = json!("single_choice");
        value["options"] = Value::Array(options);
        return (value, StructureSource::LocalSplit);
    }
    (
        raw_text_to_minimal_json(raw_text, label),
        StructureSource::RawFallback,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // ★ P1-2 回归：区分"成功结果 / 失败哨兵 / 空结果 / 损坏 JSON"
    #[test]
    fn reusable_batch_result_distinguishes_failure_from_success() {
        // 成功的非空数组可复用
        let ok = r#"[{"_source_label":"1","content":"题目"}]"#;
        assert!(parse_reusable_batch_result(ok).is_some());

        // 失败哨兵不可复用（恢复时触发重试）
        assert!(parse_reusable_batch_result(BATCH_FAILED_SENTINEL).is_none());

        // 旧版失败写入的 "[]" 不可复用
        assert!(parse_reusable_batch_result("[]").is_none());

        // 空字符串（占位）不可复用
        assert!(parse_reusable_batch_result("").is_none());

        // 损坏 JSON 不可复用
        assert!(parse_reusable_batch_result("{not json").is_none());
    }

    // ★ P0 回归：截断的 JSON 也要能把已闭合的题目对象抢救出来
    #[test]
    fn parse_llm_response_salvages_truncated_array() {
        let truncated = r#"[
            {"_source_label":"1","content":"第一题","question_type":"single_choice"},
            {"_source_label":"2","content":"第二题","question_type":"short_answer"},
            {"_source_label":"3","content":"第三题被截断""#;
        let parsed = LlmStructurer::parse_llm_response(truncated);
        assert_eq!(parsed.len(), 2, "应抢救出两个完整对象: {parsed:?}");
        assert_eq!(parsed[0]["content"], "第一题");
        assert_eq!(parsed[1]["content"], "第二题");
    }

    #[test]
    fn parse_llm_response_ignores_prose_without_json() {
        let parsed = LlmStructurer::parse_llm_response("抱歉，我无法完成这个请求。");
        assert!(parsed.is_empty());
    }

    // ★ P1 回归：LLM 少输出题目时按顺序救回已解析部分
    #[test]
    fn align_by_label_fills_unmatched_when_labels_mismatch() {
        let labels = vec![
            "9".to_string(),
            "10".to_string(),
            "11".to_string(),
            "12".to_string(),
        ];
        // 模型改写了题号（Q9 而非 9），一个都对不上
        let parsed = vec![
            json!({"_source_label": "Q9", "content": "题九"}),
            json!({"_source_label": "Q10", "content": "题十"}),
        ];
        let aligned = LlmStructurer::align_by_label(&labels, &parsed);
        assert_eq!(aligned.iter().filter(|s| s.is_some()).count(), 2);
        assert_eq!(aligned[0].as_ref().unwrap()["content"], "题九");
        assert_eq!(aligned[1].as_ref().unwrap()["content"], "题十");
        assert!(aligned[2].is_none());
    }

    #[test]
    fn align_by_label_prefers_label_match() {
        let labels = vec!["1".to_string(), "2".to_string()];
        let parsed = vec![
            json!({"_source_label": "2", "content": "第二题"}),
            json!({"_source_label": "1", "content": "第一题"}),
        ];
        let aligned = LlmStructurer::align_by_label(&labels, &parsed);
        assert_eq!(aligned[0].as_ref().unwrap()["content"], "第一题");
        assert_eq!(aligned[1].as_ref().unwrap()["content"], "第二题");
    }

    // ★ P1 回归：本地选项拆分（真实生产样本）
    #[test]
    fn split_options_line_based_real_sample() {
        let raw = "9．奎尼丁与地高辛合用时正确的处理是 （考点：药物相互作用）\nA．地高辛剂量加倍\nB．地高辛剂量必须减半\nC．两药无相互作用\nD．停用奎尼丁";
        let (stem, options) = split_options_from_raw_text(raw).expect("应能拆分");
        assert_eq!(
            stem,
            "奎尼丁与地高辛合用时正确的处理是 （考点：药物相互作用）"
        );
        assert_eq!(options.len(), 4);
        assert_eq!(options[0]["key"], "A");
        assert_eq!(options[0]["content"], "地高辛剂量加倍");
        assert_eq!(options[3]["key"], "D");
    }

    #[test]
    fn split_options_inline_single_line() {
        let raw = "1．下列哪项正确 A．甲 B．乙 C．丙 D．丁";
        let (stem, options) = split_options_from_raw_text(raw).expect("应能拆分内联选项");
        assert!(stem.contains("下列哪项正确"), "stem={stem}");
        assert_eq!(options.len(), 4);
        assert_eq!(options[1]["content"], "乙");
    }

    #[test]
    fn split_options_rejects_plain_prose() {
        assert!(split_options_from_raw_text("名词解释：有效不应期").is_none());
        // 题干里提到 A、B、C 不代表是选项
        assert!(split_options_from_raw_text("已知 A、B、C 三点共线，求 AB 长度").is_none());
    }

    #[test]
    fn fallback_json_uses_local_split_before_raw_fallback() {
        let raw = "12．利多卡因抗心律失常的电生理特点是\nA．延长 APD 和 ERP\nB．缩短 APD，相对延长 ERP\nC．重度阻滞钠通道\nD．阻滞钾通道为主";
        let (value, origin) = fallback_json(raw, "12");
        assert_eq!(origin, StructureSource::LocalSplit);
        assert_eq!(value["question_type"], "single_choice");
        assert_eq!(value["options"].as_array().unwrap().len(), 4);
        assert!(!value["content"].as_str().unwrap().contains("A．"));

        let (plain, plain_origin) = fallback_json("名词解释：奎尼丁晕厥", "28");
        assert_eq!(plain_origin, StructureSource::RawFallback);
        assert_eq!(plain["question_type"], "other");
    }
}
