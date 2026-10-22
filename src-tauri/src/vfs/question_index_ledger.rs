//! 题目索引持久账本（复用现有 `vfs_index_units`，不新增表/迁移）
//!
//! 设计要点（对应 PR #490 方案 §4.7 的“发布完成与索引完成分离”）：
//!
//! - 一题一个持久索引任务 = `vfs_index_units(resource_id = question_id, unit_index = 0)`，
//!   已有唯一索引 `idx_vfs_index_units_unique(resource_id, unit_index)`，入队用
//!   `INSERT ... ON CONFLICT DO UPDATE` 幂等，可在业务事务内直接调用。
//! - 入队（`enqueue_*_with_conn`）必须与材料更新/发布在**同一事务**写入；事务提交后
//!   才由 worker 执行索引，避免“已提交但还没投任务就崩溃”的窗口。
//! - claim 使用 `updated_at` 作为代际戳（claim token）并 CAS 回写：
//!   `text_state='indexing' AND updated_at=token`。入队会 bump `updated_at`，因此
//!   “入队 pending → 别处 claim → 旧 worker 晚回写”不会清掉新任务。
//! - 崩溃只会留下 `indexing` 行，由 `recover_stale_claims_with_conn` 复位为 pending，
//!   不依赖进程内状态。
//! - 失败重试有界：重试元数据编码在 `text_error` 前缀（`#retry=n;next=ms;`），
//!   不需要加列；超过 `max_retries` 转为 `failed` 并在 UI 可见原因。
//! - `SQLITE_BUSY/LOCKED` 只有限退避，绝不放大为业务重发/重发布。

use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use tracing::{debug, warn};

use crate::vfs::database::VfsDatabase;
use crate::vfs::error::{VfsError, VfsResult};

/// 题目索引任务的固定 unit_index。
pub const QUESTION_UNIT_INDEX: i32 = 0;
/// 默认最大重试次数（与资源索引 `max_retries` 语义一致）。
pub const DEFAULT_QUESTION_INDEX_MAX_RETRIES: i32 = 3;
/// 超过该时长仍处于 `indexing` 的 claim 视为崩溃遗留。
pub const STALE_CLAIM_MS: i64 = 10 * 60 * 1000;

const RETRY_PREFIX: &str = "#retry=";
const BUSY_RETRY_DELAYS_MS: [u64; 3] = [50, 150, 400];

/// 已 claim 的题目索引任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingQuestionUnit {
    pub unit_id: String,
    pub question_id: String,
    pub exam_id: String,
    /// claim 代际戳；回写时必须原样带回。
    pub claim_token: i64,
    /// 已发生的失败次数（0 表示首次尝试）。
    pub attempt: i32,
}

/// 单次失败回写的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionIndexAttemptOutcome {
    /// 已按退避重新排队。
    RetryScheduled { attempt: i32, next_at_ms: i64 },
    /// 达到重试上限，标记 failed。
    Exhausted,
    /// claim 已过期/被新入队替代，本次回写被丢弃（新任务仍保留）。
    ClaimLost,
}

/// 一批题目索引执行的统计。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct QuestionIndexBatchReport {
    pub claimed: usize,
    pub indexed: usize,
    pub retried: usize,
    pub exhausted: usize,
    pub claim_lost: usize,
}

/// 题目索引账本入口。
pub struct QuestionIndexLedger;

impl QuestionIndexLedger {
    /// 在**业务事务内**把题目加入索引待办（幂等）。
    ///
    /// 只接受仍存在且未软删除的题目；同一 `(question_id, 0)` 已存在时重置为 pending
    /// 并 bump `updated_at`（新的代际戳），保证旧 claim 的迟到回写失效。
    ///
    /// 返回实际入队（新建或重置）的题目数。
    pub fn enqueue_question_units_with_conn(
        conn: &Connection,
        question_ids: &[String],
        now_ms: i64,
    ) -> VfsResult<usize> {
        let mut enqueued = 0usize;
        for question_id in question_ids {
            let id = format!("unit_{}", nanoid::nanoid!(10));
            let changed = conn.execute(
                r#"
                INSERT INTO vfs_index_units (
                    id, resource_id, unit_index, text_content, text_required, text_state,
                    mm_required, mm_state, created_at, updated_at
                )
                SELECT ?1, q.id, 0, '', 1, 'pending', 0, 'disabled', ?3, ?3
                FROM questions q
                WHERE q.id = ?2 AND q.deleted_at IS NULL
                ON CONFLICT(resource_id, unit_index) DO UPDATE SET
                    text_required = 1,
                    text_state = 'pending',
                    text_error = NULL,
                    -- 新任务必须比任何在途 claim 的代际戳更新，才能让迟到回写失效；
                    -- 非 indexing 状态直接按 now 立即可执行（清除未来的重试时间）。
                    updated_at = CASE
                        WHEN text_state = 'indexing'
                            THEN MAX(updated_at + 1, excluded.updated_at)
                        ELSE excluded.updated_at
                    END
                "#,
                params![id, question_id, now_ms],
            )?;
            enqueued += changed;
        }
        if enqueued > 0 {
            debug!(
                "[QuestionIndexLedger] enqueued {} question index unit(s)",
                enqueued
            );
        }
        Ok(enqueued)
    }

    /// 原子 claim 一批到期的 pending 题目单元（pending → indexing）。
    ///
    /// `updated_at` 对题目单元同时表示“最早可执行时间”：失败退避写入未来时间戳，
    /// 由 SQL 直接过滤，避免前方大量未来重试行把后面的到期行饿死。
    pub fn claim_pending_question_units_with_conn(
        conn: &Connection,
        now_ms: i64,
        limit: u32,
        max_retries: i32,
    ) -> VfsResult<Vec<PendingQuestionUnit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        let candidates = {
            let mut stmt = conn.prepare(
                r#"
                SELECT u.id, u.resource_id, q.exam_id, u.updated_at, u.text_error
                FROM vfs_index_units u
                JOIN questions q ON q.id = u.resource_id AND q.deleted_at IS NULL
                WHERE u.unit_index = 0
                  AND u.text_required = 1
                  AND u.text_state = 'pending'
                  AND u.updated_at <= ?1
                ORDER BY u.updated_at ASC
                LIMIT ?2
                "#,
            )?;
            let rows = stmt.query_map(params![now_ms, limit as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3).unwrap_or(0),
                    row.get::<_, Option<String>>(4).ok().flatten(),
                ))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };

        let mut claimed = Vec::new();
        for (unit_id, question_id, exam_id, updated_at, error) in candidates {
            if claimed.len() >= limit as usize {
                break;
            }
            let (attempt, _, _) = parse_retry(error.as_deref());
            if attempt >= max_retries.max(1) {
                continue;
            }
            let token = next_stamp(now_ms, updated_at);
            let changed = conn.execute(
                r#"
                UPDATE vfs_index_units
                SET text_state = 'indexing', updated_at = ?3
                WHERE id = ?1 AND text_state = 'pending' AND updated_at = ?2
                "#,
                params![unit_id, updated_at, token],
            )?;
            if changed == 1 {
                claimed.push(PendingQuestionUnit {
                    unit_id,
                    question_id,
                    exam_id,
                    claim_token: token,
                    attempt,
                });
            }
        }

        Ok(claimed)
    }

    /// 成功回写（CAS）。返回 false 表示 claim 已失效（被新入队替代）。
    pub fn mark_question_unit_indexed_with_conn(
        conn: &Connection,
        unit_id: &str,
        claim_token: i64,
        chunk_count: i32,
        embedding_dim: i64,
        now_ms: i64,
    ) -> VfsResult<bool> {
        let changed = conn.execute(
            r#"
            UPDATE vfs_index_units
            SET text_state = 'indexed',
                text_error = NULL,
                text_indexed_at = ?3,
                text_chunk_count = ?4,
                text_embedding_dim = ?5,
                updated_at = ?3
            WHERE id = ?1 AND text_state = 'indexing' AND updated_at = ?2
            "#,
            params![unit_id, claim_token, now_ms, chunk_count, embedding_dim],
        )?;
        Ok(changed == 1)
    }

    /// 失败回写（CAS + 有界退避）。claim 失效时不消耗新任务的尝试次数。
    pub fn mark_question_unit_failed_with_conn(
        conn: &Connection,
        unit_id: &str,
        claim_token: i64,
        error: &str,
        now_ms: i64,
        max_retries: i32,
    ) -> VfsResult<QuestionIndexAttemptOutcome> {
        // 先确认 claim 仍有效，并读取当前尝试次数。
        let current: Option<Option<String>> = conn
            .query_row(
                "SELECT text_error FROM vfs_index_units WHERE id = ?1 AND text_state = 'indexing' AND updated_at = ?2",
                params![unit_id, claim_token],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        let Some(stored_error) = current else {
            return Ok(QuestionIndexAttemptOutcome::ClaimLost);
        };
        let (attempt, _, _) = parse_retry(stored_error.as_deref());
        let next_attempt = attempt + 1;
        let stamp = next_stamp(now_ms, claim_token);
        let safe_error = sanitize_ledger_error(error);

        if next_attempt >= max_retries.max(1) {
            conn.execute(
                r#"
                UPDATE vfs_index_units
                SET text_state = 'failed', text_error = ?3, updated_at = ?4
                WHERE id = ?1 AND text_state = 'indexing' AND updated_at = ?2
                "#,
                params![
                    unit_id,
                    claim_token,
                    encode_retry(next_attempt, None, &safe_error),
                    stamp
                ],
            )?;
            return Ok(QuestionIndexAttemptOutcome::Exhausted);
        }

        let delay = backoff_ms(next_attempt);
        let next_at = now_ms.saturating_add(delay);
        // pending 行的 updated_at 即“最早可执行时间”：由 SQL 过滤，保证公平。
        conn.execute(
            r#"
            UPDATE vfs_index_units
            SET text_state = 'pending', text_error = ?3, updated_at = ?4
            WHERE id = ?1 AND text_state = 'indexing' AND updated_at = ?2
            "#,
            params![
                unit_id,
                claim_token,
                encode_retry(next_attempt, Some(next_at), &safe_error),
                next_at
            ],
        )?;
        Ok(QuestionIndexAttemptOutcome::RetryScheduled {
            attempt: next_attempt,
            next_at_ms: next_at,
        })
    }

    /// 崩溃补偿：把超时仍处于 `indexing` 的任务复位为 pending。
    pub fn recover_stale_claims_with_conn(
        conn: &Connection,
        now_ms: i64,
        stale_ms: i64,
    ) -> VfsResult<usize> {
        let cutoff = now_ms.saturating_sub(stale_ms.max(0));
        let recovered = conn.execute(
            r#"
            UPDATE vfs_index_units
            SET text_state = 'pending', updated_at = ?1
            WHERE unit_index = 0
              AND text_state = 'indexing'
              AND updated_at <= ?2
              AND resource_id IN (SELECT id FROM questions WHERE deleted_at IS NULL)
            "#,
            params![next_stamp(now_ms, cutoff), cutoff],
        )?;
        if recovered > 0 {
            warn!(
                "[QuestionIndexLedger] recovered {} stale indexing claim(s)",
                recovered
            );
        }
        Ok(recovered)
    }

    /// 当前待处理的题目索引任务数（诊断/测试用）。
    pub fn pending_question_unit_count_with_conn(conn: &Connection) -> VfsResult<i64> {
        let count = conn.query_row(
            r#"
            SELECT COUNT(*) FROM vfs_index_units u
            JOIN questions q ON q.id = u.resource_id AND q.deleted_at IS NULL
            WHERE u.unit_index = 0 AND u.text_required = 1
              AND u.text_state IN ('pending', 'indexing', 'failed')
            "#,
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }
}

/// 题目索引执行器：由 worker 实现（复用 `VfsFullIndexingService::index_question`），
/// 单测用假实现即可验证账本编排。
#[allow(async_fn_in_trait)]
pub trait QuestionIndexExecutor {
    /// `claim_token` 必须传入单题索引的最终账本回写，使旧内容写入无法覆盖新任务
    /// （新入队会推进 `updated_at`，旧 token 的 CAS 失败）。
    async fn index_question_for_ledger(
        &self,
        question_id: &str,
        exam_id: &str,
        claim_token: i64,
    ) -> VfsResult<(usize, usize)>;
}

/// 执行一批题目索引：claim → 调用执行器 → CAS 回写。
///
/// 本模块不直接触碰 LanceDB；`executor` 由 `VfsFullIndexingService` 实现。
pub async fn run_pending_question_index_batch<E: QuestionIndexExecutor>(
    db: &VfsDatabase,
    executor: &E,
    limit: u32,
    max_retries: i32,
    now_ms: i64,
) -> VfsResult<QuestionIndexBatchReport> {
    let mut report = QuestionIndexBatchReport::default();

    // 1) 崩溃补偿 + claim
    let claimed = {
        let conn = db.get_conn_safe()?;
        let _ = QuestionIndexLedger::recover_stale_claims_with_conn(&conn, now_ms, STALE_CLAIM_MS)?;
        with_busy_retry(|| {
            QuestionIndexLedger::claim_pending_question_units_with_conn(
                &conn,
                now_ms,
                limit,
                max_retries,
            )
        })?
    };
    report.claimed = claimed.len();
    if claimed.is_empty() {
        return Ok(report);
    }

    for unit in claimed {
        let outcome = executor
            .index_question_for_ledger(&unit.question_id, &unit.exam_id, unit.claim_token)
            .await;
        let conn = db.get_conn_safe()?;
        match outcome {
            Ok((chunk_count, dim)) => {
                let applied = with_busy_retry(|| {
                    QuestionIndexLedger::mark_question_unit_indexed_with_conn(
                        &conn,
                        &unit.unit_id,
                        unit.claim_token,
                        chunk_count as i32,
                        dim as i64,
                        now_ms,
                    )
                })?;
                if applied {
                    report.indexed += 1;
                    continue;
                }
                // CAS 失败：要么 index_question 自己已把行写成 indexed（内容已就绪），
                // 要么期间有新的入队把行重置为 pending（新任务已保留）。
                let state: Option<String> = conn
                    .query_row(
                        "SELECT text_state FROM vfs_index_units WHERE id = ?1",
                        params![unit.unit_id],
                        |row| row.get(0),
                    )
                    .optional()?;
                match state.as_deref() {
                    Some("indexed") => report.indexed += 1,
                    Some("pending") | Some("indexing") => report.claim_lost += 1,
                    _ => report.claim_lost += 1,
                }
            }
            Err(error) => {
                let result = with_busy_retry(|| {
                    QuestionIndexLedger::mark_question_unit_failed_with_conn(
                        &conn,
                        &unit.unit_id,
                        unit.claim_token,
                        &error.to_string(),
                        now_ms,
                        max_retries,
                    )
                })?;
                match result {
                    QuestionIndexAttemptOutcome::RetryScheduled { .. } => report.retried += 1,
                    QuestionIndexAttemptOutcome::Exhausted => report.exhausted += 1,
                    QuestionIndexAttemptOutcome::ClaimLost => report.claim_lost += 1,
                }
            }
        }
    }

    debug!(
        "[QuestionIndexLedger] batch: claimed={}, indexed={}, retried={}, exhausted={}, claim_lost={}",
        report.claimed, report.indexed, report.retried, report.exhausted, report.claim_lost
    );
    Ok(report)
}

/// 题组材料上下文 + 题目字段 → 索引文本。
///
/// 材料必须进入**每个子题**的索引文本，保证只取单题时也能检索到共享条件；
/// 权威正文仍只存材料表一份（调用方传入已读取的文本）。
pub fn compose_question_index_text(
    content: &str,
    options: &[(String, String)],
    answer: Option<&str>,
    explanation: Option<&str>,
    tags: &[String],
    user_note: Option<&str>,
    group_material: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(material) = group_material.map(str::trim).filter(|m| !m.is_empty()) {
        parts.push(format!("材料: {material}"));
    }
    if !content.trim().is_empty() {
        parts.push(format!("题目: {}", content.trim()));
    }
    for (key, value) in options {
        if !value.trim().is_empty() {
            parts.push(format!("选项{key}: {}", value.trim()));
        }
    }
    if let Some(answer) = answer.map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("答案: {answer}"));
    }
    if let Some(explanation) = explanation.map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("解析: {explanation}"));
    }
    if !tags.is_empty() {
        parts.push(format!("标签: {}", tags.join(", ")));
    }
    if let Some(note) = user_note.map(str::trim).filter(|v| !v.is_empty()) {
        parts.push(format!("笔记: {note}"));
    }
    parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// 内部工具
// ---------------------------------------------------------------------------

/// 脱敏并截断用于 ledger 的错误文本：只保留首行、稳定错误摘要和关键状态码，
/// 不写供应商原始响应体、URL 查询串、密钥或用户材料。
pub fn sanitize_ledger_error(message: &str) -> String {
    let raw_line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("index failed");
    // 供应商响应体/材料内容一律不进入账本，只保留稳定错误摘要。
    const BODY_MARKERS: &[&str] = &[
        "provider body",
        "response body",
        "raw_body",
        "response_body",
        "响应体",
        "返回体",
    ];
    let lowered = raw_line.to_ascii_lowercase();
    let line = BODY_MARKERS
        .iter()
        .filter_map(|marker| lowered.find(marker))
        .min()
        .map(|index| raw_line[..index].trim_end())
        .unwrap_or(raw_line);

    let mut out = String::with_capacity(line.len().min(200));
    let mut redact_next = false;
    for token in line.split_whitespace() {
        let lower = token.to_ascii_lowercase();
        let redact = redact_next
            || lower.starts_with("authorization:")
            || lower == "bearer"
            || lower.contains("api_key=")
            || lower.contains("apikey=")
            || lower.contains("access_token=")
            || lower.contains("signature=")
            || lower.contains("x-amz-signature=")
            || lower.contains("token=");
        if redact {
            out.push_str("[redacted] ");
            redact_next = lower.starts_with("authorization:") || lower == "bearer";
            continue;
        }
        redact_next = false;

        let url_candidate = ["url=", "uri=", "href="]
            .iter()
            .find_map(|prefix| token.strip_prefix(*prefix).map(|rest| (*prefix, rest)));
        if let Some((prefix, candidate)) = url_candidate {
            if candidate.starts_with("http://") || candidate.starts_with("https://") {
                match candidate.split_once('?') {
                    Some((base, _query)) => {
                        out.push_str(prefix);
                        out.push_str(base);
                        out.push_str("?[redacted] ");
                    }
                    None => {
                        out.push_str(token);
                        out.push(' ');
                    }
                }
                continue;
            }
        }
        out.push_str(token);
        out.push(' ');
    }
    let trimmed = out.trim_end();
    if trimmed.chars().count() > 200 {
        let mut clipped: String = trimmed.chars().take(200).collect();
        clipped.push('…');
        clipped
    } else {
        trimmed.to_string()
    }
}

fn next_stamp(now_ms: i64, previous: i64) -> i64 {
    now_ms.max(previous.saturating_add(1))
}

fn backoff_ms(attempt: i32) -> i64 {
    let exp = attempt.clamp(1, 8) as u32;
    (30_000_i64.saturating_mul(1_i64 << exp)).min(15 * 60 * 1000)
}

fn encode_retry(attempt: i32, next_at_ms: Option<i64>, message: &str) -> String {
    match next_at_ms {
        Some(next) => format!("{RETRY_PREFIX}{attempt};next={next}; {message}"),
        None => format!("{RETRY_PREFIX}{attempt};next=-; {message}"),
    }
}

/// 解析 `#retry=n;next=ms; msg`；非本模块写入的错误按首次尝试处理。
fn parse_retry(error: Option<&str>) -> (i32, Option<i64>, String) {
    let Some(raw) = error else {
        return (0, None, String::new());
    };
    let Some(rest) = raw.strip_prefix(RETRY_PREFIX) else {
        return (0, None, raw.to_string());
    };
    let (meta, message) = match rest.split_once("; ") {
        Some((meta, message)) => (meta, message.to_string()),
        None => (rest, String::new()),
    };
    let mut attempt = 0;
    let mut next = None;
    for part in meta.split(';') {
        if let Some(value) = part.strip_prefix("next=") {
            next = value.parse::<i64>().ok();
        } else if let Ok(parsed) = part.parse::<i32>() {
            attempt = parsed;
        }
    }
    (attempt, next, message)
}

fn is_busy_error(error: &rusqlite::Error) -> bool {
    matches!(
        error.sqlite_error_code(),
        Some(rusqlite::ErrorCode::DatabaseBusy) | Some(rusqlite::ErrorCode::DatabaseLocked)
    )
}

fn with_busy_retry<T>(mut op: impl FnMut() -> VfsResult<T>) -> VfsResult<T> {
    let mut last_error: Option<VfsError> = None;
    for (index, delay) in BUSY_RETRY_DELAYS_MS.iter().enumerate() {
        match op() {
            Ok(value) => return Ok(value),
            Err(error) => {
                let busy = match &error {
                    VfsError::Database(message) => {
                        message.contains("database is locked")
                            || message.contains("database table is locked")
                            || message.contains("SQLITE_BUSY")
                            || message.contains("SQLITE_LOCKED")
                    }
                    _ => false,
                };
                if !busy || index + 1 == BUSY_RETRY_DELAYS_MS.len() {
                    return Err(error);
                }
                warn!(
                    "[QuestionIndexLedger] transient SQLite lock, backing off {}ms: {}",
                    delay, error
                );
                std::thread::sleep(Duration::from_millis(*delay));
                last_error = Some(error);
            }
        }
    }
    Err(last_error.unwrap_or_else(|| VfsError::Other("busy retry exhausted".to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::database::setup_migrated_test_db;

    fn insert_exam(conn: &Connection, exam_id: &str) {
        conn.execute(
            r#"INSERT INTO exam_sheets (id, status, temp_id, metadata_json, preview_json, created_at, updated_at)
               VALUES (?1, 'completed', 't', '{}', '{}', '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z')"#,
            params![exam_id],
        )
        .unwrap();
    }

    fn insert_question(conn: &Connection, question_id: &str, exam_id: &str, deleted: bool) {
        conn.execute(
            r#"INSERT INTO questions (id, exam_id, content, created_at, updated_at, deleted_at)
               VALUES (?1, ?2, '题干', '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z', ?3)"#,
            params![
                question_id,
                exam_id,
                if deleted {
                    Some("2026-10-21T01:00:00Z")
                } else {
                    None
                }
            ],
        )
        .unwrap();
    }

    fn unit_state(conn: &Connection, question_id: &str) -> (String, i64, Option<String>) {
        conn.query_row(
            "SELECT text_state, updated_at, text_error FROM vfs_index_units WHERE resource_id = ?1 AND unit_index = 0",
            params![question_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .unwrap()
    }

    #[test]
    fn enqueue_is_idempotent_and_resets_existing_task() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_1", "exam_1", false);

        let ids = vec!["q_1".to_string()];
        assert_eq!(
            QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &ids, 1_000).unwrap(),
            1
        );
        let rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM vfs_index_units WHERE resource_id = 'q_1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1, "重复入队不能新增行");

        // 模拟已索引后再次入队：必须回到 pending 且 bump 代际戳。
        conn.execute(
            "UPDATE vfs_index_units SET text_state = 'indexed', updated_at = 1_500 WHERE resource_id = 'q_1'",
            [],
        )
        .unwrap();
        assert_eq!(
            QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &ids, 1_000).unwrap(),
            1
        );
        let (state, stamp, error) = unit_state(&conn, "q_1");
        assert_eq!(state, "pending");
        assert_eq!(
            stamp, 1_000,
            "非 indexing 状态重新入队按 now 立即执行（清除未来退避时间）"
        );
        assert!(error.is_none());
    }

    #[test]
    fn enqueue_skips_deleted_or_missing_questions() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_deleted", "exam_1", true);

        let enqueued = QuestionIndexLedger::enqueue_question_units_with_conn(
            &conn,
            &["q_deleted".to_string(), "q_missing".to_string()],
            1_000,
        )
        .unwrap();
        assert_eq!(enqueued, 0);
        assert_eq!(
            QuestionIndexLedger::pending_question_unit_count_with_conn(&conn).unwrap(),
            0
        );
    }

    #[test]
    fn claim_is_bounded_and_marks_indexing() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_1", "exam_1", false);
        insert_question(&conn, "q_2", "exam_1", false);
        QuestionIndexLedger::enqueue_question_units_with_conn(
            &conn,
            &["q_1".to_string(), "q_2".to_string()],
            1_000,
        )
        .unwrap();

        let claimed =
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, 1_100, 1, 3)
                .unwrap();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].exam_id, "exam_1");
        let claimed_state = unit_state(&conn, &claimed[0].question_id).0;
        let other = if claimed[0].question_id == "q_1" {
            "q_2"
        } else {
            "q_1"
        };
        assert_eq!(claimed_state, "indexing");
        assert_eq!(unit_state(&conn, other).0, "pending");
    }

    #[test]
    fn stale_claim_mark_does_not_clear_new_enqueue() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_1", "exam_1", false);
        let ids = vec!["q_1".to_string()];
        QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &ids, 1_000).unwrap();
        let claimed =
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, 1_100, 1, 3)
                .unwrap();
        let unit = &claimed[0];

        // 材料再次变更 → 新任务入队（pending，代际戳更新）
        QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &ids, 1_200).unwrap();

        // 旧 worker 晚到的成功回写必须被 CAS 拒绝，且不能清掉 pending。
        let applied = QuestionIndexLedger::mark_question_unit_indexed_with_conn(
            &conn,
            &unit.unit_id,
            unit.claim_token,
            2,
            1024,
            1_300,
        )
        .unwrap();
        assert!(!applied, "旧 claim 的迟到回写必须失败");
        assert_eq!(unit_state(&conn, "q_1").0, "pending");

        // 失败回写同样不能消耗新任务。
        let outcome = QuestionIndexLedger::mark_question_unit_failed_with_conn(
            &conn,
            &unit.unit_id,
            unit.claim_token,
            "boom",
            1_300,
            3,
        )
        .unwrap();
        assert_eq!(outcome, QuestionIndexAttemptOutcome::ClaimLost);
        assert_eq!(unit_state(&conn, "q_1").0, "pending");
    }

    #[test]
    fn failed_attempt_backs_off_then_exhausts() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_1", "exam_1", false);
        let ids = vec!["q_1".to_string()];
        QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &ids, 1_000).unwrap();

        let mut now = 1_100;
        let claimed =
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, now, 1, 2).unwrap();
        let unit = &claimed[0];
        let outcome = QuestionIndexLedger::mark_question_unit_failed_with_conn(
            &conn,
            &unit.unit_id,
            unit.claim_token,
            "embed failed",
            now,
            2,
        )
        .unwrap();
        let next_at = match outcome {
            QuestionIndexAttemptOutcome::RetryScheduled {
                attempt,
                next_at_ms,
            } => {
                assert_eq!(attempt, 1);
                next_at_ms
            }
            other => panic!("expected retry, got {other:?}"),
        };

        // 退避未到期：不可 claim。
        now += 1;
        assert!(
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, now, 1, 2)
                .unwrap()
                .is_empty()
        );

        // 到期后可再次 claim；第二次失败即达到上限。
        now = next_at;
        let claimed =
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, now, 1, 2).unwrap();
        assert_eq!(claimed[0].attempt, 1);
        let outcome = QuestionIndexLedger::mark_question_unit_failed_with_conn(
            &conn,
            &claimed[0].unit_id,
            claimed[0].claim_token,
            "embed failed again",
            now,
            2,
        )
        .unwrap();
        assert_eq!(outcome, QuestionIndexAttemptOutcome::Exhausted);
        let (state, _, error) = unit_state(&conn, "q_1");
        assert_eq!(state, "failed");
        assert!(error.unwrap().contains("embed failed again"));
    }

    #[test]
    fn crash_recovery_resets_stale_indexing() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_1", "exam_1", false);
        QuestionIndexLedger::enqueue_question_units_with_conn(&conn, &["q_1".into()], 1_000)
            .unwrap();
        QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, 1_100, 1, 3).unwrap();
        assert_eq!(unit_state(&conn, "q_1").0, "indexing");

        // 未超时：不回收
        assert_eq!(
            QuestionIndexLedger::recover_stale_claims_with_conn(&conn, 2_000, 5_000).unwrap(),
            0
        );
        // 超时（模拟崩溃后重启）：复位 pending
        assert_eq!(
            QuestionIndexLedger::recover_stale_claims_with_conn(&conn, 60_000, 1_000).unwrap(),
            1
        );
        assert_eq!(unit_state(&conn, "q_1").0, "pending");
    }

    #[test]
    fn batch_runner_indexes_and_retries_without_duplicating() {
        let (_tmp, db) = setup_migrated_test_db();
        {
            let conn = db.get_conn_safe().unwrap();
            insert_exam(&conn, "exam_1");
            insert_question(&conn, "q_ok", "exam_1", false);
            insert_question(&conn, "q_bad", "exam_1", false);
            QuestionIndexLedger::enqueue_question_units_with_conn(
                &conn,
                &["q_ok".to_string(), "q_bad".to_string()],
                1_000,
            )
            .unwrap();
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        struct FakeExecutor;
        impl QuestionIndexExecutor for FakeExecutor {
            async fn index_question_for_ledger(
                &self,
                question_id: &str,
                _exam_id: &str,
                _claim_token: i64,
            ) -> VfsResult<(usize, usize)> {
                if question_id == "q_bad" {
                    Err(VfsError::Other("provider down".to_string()))
                } else {
                    Ok((1, 1024))
                }
            }
        }
        let report = runtime
            .block_on(run_pending_question_index_batch(
                &db,
                &FakeExecutor,
                10,
                3,
                1_100,
            ))
            .unwrap();
        assert_eq!(report.claimed, 2);
        assert_eq!(report.indexed, 1);
        assert_eq!(report.retried, 1);

        let conn = db.get_conn_safe().unwrap();
        assert_eq!(unit_state(&conn, "q_ok").0, "indexed");
        let (bad_state, _, bad_error) = unit_state(&conn, "q_bad");
        assert_eq!(bad_state, "pending");
        assert!(bad_error.unwrap().contains("provider down"));
    }

    #[test]
    fn batch_runner_keeps_task_enqueued_during_indexing() {
        use std::sync::Arc;

        let (_tmp, db) = setup_migrated_test_db();
        let db = Arc::new(db);
        {
            let conn = db.get_conn_safe().unwrap();
            insert_exam(&conn, "exam_1");
            insert_question(&conn, "q_1", "exam_1", false);
            QuestionIndexLedger::enqueue_question_units_with_conn(
                &conn,
                &["q_1".to_string()],
                1_000,
            )
            .unwrap();
        }

        // 模拟索引执行期间材料再次变更（同一事务语义由 enqueue 单语句保证）。
        struct RacingExecutor(Arc<VfsDatabase>);
        impl QuestionIndexExecutor for RacingExecutor {
            async fn index_question_for_ledger(
                &self,
                question_id: &str,
                _exam_id: &str,
                _claim_token: i64,
            ) -> VfsResult<(usize, usize)> {
                let conn = self.0.get_conn_safe()?;
                QuestionIndexLedger::enqueue_question_units_with_conn(
                    &conn,
                    &[question_id.to_string()],
                    2_000,
                )?;
                Ok((1, 1024))
            }
        }

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let report = runtime
            .block_on(run_pending_question_index_batch(
                &db,
                &RacingExecutor(Arc::clone(&db)),
                10,
                3,
                1_100,
            ))
            .unwrap();
        assert_eq!(report.claimed, 1);
        assert_eq!(report.indexed, 0);
        assert_eq!(report.claim_lost, 1);
        let conn = db.get_conn_safe().unwrap();
        assert_eq!(
            unit_state(&conn, "q_1").0,
            "pending",
            "新任务不能被旧回写清掉"
        );
    }

    #[test]
    fn compose_question_index_text_includes_group_material() {
        let text = compose_question_index_text(
            "若 a=1，求 b。",
            &[
                ("A".to_string(), "1".to_string()),
                ("B".to_string(), "2".to_string()),
            ],
            Some("B"),
            Some("由条件推出"),
            &["代数".to_string()],
            None,
            Some("共享条件：a 为正整数，b=a+1。"),
        );
        let material_at = text
            .find("材料: 共享条件")
            .expect("material must be indexed");
        let question_at = text.find("题目: 若 a=1").expect("question must be indexed");
        assert!(material_at < question_at, "材料应在题目之前进入索引文本");
        assert!(text.contains("选项B: 2"));
        assert!(text.contains("答案: B"));
    }

    #[test]
    fn future_retry_does_not_starve_due_task() {
        let (_tmp, db) = setup_migrated_test_db();
        let conn = db.get_conn_safe().unwrap();
        insert_exam(&conn, "exam_1");
        insert_question(&conn, "q_a", "exam_1", false);
        insert_question(&conn, "q_b", "exam_1", false);
        QuestionIndexLedger::enqueue_question_units_with_conn(
            &conn,
            &["q_a".to_string(), "q_b".to_string()],
            1_000,
        )
        .unwrap();

        let first = QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, 1_100, 1, 3)
            .unwrap();
        assert_eq!(first.len(), 1);
        // q_b 仍在 pending，且到期。
        QuestionIndexLedger::mark_question_unit_failed_with_conn(
            &conn,
            &first[0].unit_id,
            first[0].claim_token,
            "temporary",
            1_100,
            3,
        )
        .unwrap();

        // 下一轮（退避未到期）必须取到 q_b，而不是被 q_a 的未来时间戳挡住。
        let second =
            QuestionIndexLedger::claim_pending_question_units_with_conn(&conn, 1_200, 1, 3)
                .unwrap();
        assert_eq!(second.len(), 1);
        assert_ne!(
            second[0].question_id, first[0].question_id,
            "退避中的题不能阻塞另一道到期题"
        );
    }

    #[test]
    fn ledger_error_is_redacted_and_truncated() {
        let sanitized = sanitize_ledger_error(
            "HTTP 500: upstream failed url=https://api.example.com/v1?api_key=sk-secret&x=1 Authorization: Bearer sk-abc provider body: secret material",
        );
        assert!(!sanitized.contains("sk-secret"));
        assert!(!sanitized.contains("sk-abc"));
        assert!(!sanitized.contains("api_key="));
        assert!(!sanitized.contains("secret material"));
        assert!(sanitized.contains("HTTP 500"));

        let long = "x".repeat(500);
        assert!(sanitize_ledger_error(&long).chars().count() <= 201);
    }

    #[test]
    fn busy_classification_matches_sqlite_codes() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(5),
            Some("database is locked".to_string()),
        );
        assert!(is_busy_error(&busy));
        let other = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(1),
            Some("syntax error".to_string()),
        );
        assert!(!is_busy_error(&other));
    }
}
