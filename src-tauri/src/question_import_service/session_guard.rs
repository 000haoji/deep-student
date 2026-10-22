//! Import session and checkpoint safety boundaries.
//!
//! A caller-supplied session id, a stale checkpoint or two concurrent resumes must
//! never let one import silently overwrite another attempt's evidence. This module
//! owns the explicit operation-mode matrix, the per-session lease, checkpoint
//! schema migration and the compare-and-swap checkpoint writer.

use super::ImportCheckpointState;
use crate::models::AppError;
use crate::vfs::database::VfsDatabase;

use rusqlite::{params, OptionalExtension};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

/// Bump when the persisted checkpoint contract changes in a way that old readers
/// cannot interpret. Unknown newer values are rejected, never reset silently.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 2;
/// Version of the extraction pipeline that produced cached page/batch results.
pub const PIPELINE_VERSION: u32 = 1;
/// Versions of the stages whose output is cached in a checkpoint. A cached value
/// produced by a different version must never be silently reused.
pub const PARSER_VERSION: u32 = 1;
pub const PROMPT_VERSION: u32 = 1;
pub const STRUCTURER_VERSION: u32 = 1;

/// An import operation must declare what it intends to do with an existing
/// session. The status matrix is per operation, not a blanket `status=importing`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionOperation {
    /// Add another document to an existing question bank.
    AppendDocument,
    /// Continue an interrupted import from its checkpoint.
    Resume,
}

impl SessionOperation {
    fn label(self) -> &'static str {
        match self {
            SessionOperation::AppendDocument => "追加文档",
            SessionOperation::Resume => "续导",
        }
    }

    fn allows_status(self, status: &str) -> bool {
        match self {
            // Appending may target a live or finished bank, but never a session
            // that is being deleted, archived or already has an active
            // unfinished import (that would overwrite a recoverable attempt).
            SessionOperation::AppendDocument => {
                matches!(
                    status,
                    "importing"
                        | "parsing"
                        | "needs_review"
                        | "partial"
                        | "failed"
                        | "completed"
                        | "excluded"
                )
            }
            SessionOperation::Resume => {
                matches!(status, "importing" | "needs_review" | "partial" | "failed")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ImportSessionSnapshot {
    pub id: String,
    pub status: String,
    pub deleted: bool,
    pub import_mode: String,
    pub import_state: Option<String>,
}

pub fn load_import_session(
    db: &VfsDatabase,
    session_id: &str,
) -> Result<Option<ImportSessionSnapshot>, AppError> {
    let conn = db
        .get_conn_safe()
        .map_err(|e| AppError::database(format!("读取导入会话连接失败: {e}")))?;
    conn.query_row(
        "SELECT id,status,deleted_at IS NOT NULL,COALESCE(json_extract(metadata_json,'$.import_mode'),''),import_state_json \
         FROM exam_sheets WHERE id=?1",
        [session_id],
        |row| {
            Ok(ImportSessionSnapshot {
                id: row.get(0)?,
                status: row.get(1)?,
                deleted: row.get(2)?,
                import_mode: row.get(3)?,
                import_state: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(|e| AppError::database(format!("读取导入会话失败: {e}")))
}

/// Validates an existing session for the declared operation. Missing, soft
/// deleted, wrong-mode and non-resumable sessions are rejected before any model
/// call or expensive rasterization happens.
pub fn validate_session_for_operation(
    db: &VfsDatabase,
    session_id: &str,
    operation: SessionOperation,
    request_mode: &str,
) -> Result<ImportSessionSnapshot, AppError> {
    let snapshot = load_import_session(db, session_id)?
        .ok_or_else(|| AppError::not_found("导入会话不存在，不能追加或续导"))?;
    if snapshot.deleted {
        return Err(AppError::validation("导入会话已删除，不能在其上继续导入"));
    }
    if !operation.allows_status(&snapshot.status) {
        return Err(AppError::validation(format!(
            "会话状态 {} 不允许{}",
            snapshot.status,
            operation.label()
        )));
    }
    // Bank-level `import_mode` describes the last document parser, not the
    // parser of a new appended document (image + DOCX appends are legitimate).
    // Only an explicit resume compares the current document's mode.
    if operation == SessionOperation::Resume
        && !snapshot.import_mode.is_empty()
        && !request_mode.is_empty()
        && snapshot.import_mode != request_mode
    {
        return Err(AppError::validation(format!(
            "导入会话模式 {} 与本次续导请求 {} 不一致，请新建题目集",
            snapshot.import_mode, request_mode
        )));
    }
    if operation == SessionOperation::AppendDocument
        && snapshot
            .import_state
            .as_deref()
            .is_some_and(|state| !state.trim().is_empty() && state.trim() != "null")
    {
        return Err(AppError::validation(
            "该题目集存在未完成的导入断点，请先续导或取消后再追加新文档",
        ));
    }
    if operation == SessionOperation::Resume {
        let state = snapshot
            .import_state
            .as_deref()
            .map(str::trim)
            .filter(|state| !state.is_empty() && *state != "null")
            .ok_or_else(|| AppError::validation("该会话没有可恢复的导入断点"))?;
        if state.starts_with('{') && state.len() < 8 {
            return Err(AppError::validation("导入断点内容无效，不能续导"));
        }
    }
    Ok(snapshot)
}

/// In-process per-session mutual exclusion. The app is single-user; this prevents
/// a resume and an append from interleaving checkpoint writes or publishing twice.
static ACTIVE_SESSIONS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub struct SessionLease {
    session_id: String,
    released: bool,
}

impl SessionLease {
    pub fn try_acquire(session_id: &str) -> Result<Self, AppError> {
        let mut active = ACTIVE_SESSIONS
            .lock()
            .map_err(|_| AppError::internal("导入会话锁已中毒"))?;
        if !active.insert(session_id.to_string()) {
            return Err(AppError::validation(
                "该题目集已有导入或续导在运行，请等待其结束或先取消",
            ));
        }
        Ok(Self {
            session_id: session_id.to_string(),
            released: false,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn release(&mut self) {
        if self.released {
            return;
        }
        if let Ok(mut active) = ACTIVE_SESSIONS.lock() {
            active.remove(&self.session_id);
        }
        self.released = true;
    }
}

impl Drop for SessionLease {
    fn drop(&mut self) {
        if !self.released {
            if let Ok(mut active) = ACTIVE_SESSIONS.lock() {
                active.remove(&self.session_id);
            }
            self.released = true;
        }
    }
}

/// Validate, migrate and stamp a checkpoint before it is reused.
///
/// Legacy checkpoints (schema_version 0) without cached page/batch results are
/// safe to upgrade. Legacy checkpoints that already contain cached typed results
/// cannot be reinterpreted: their answer-region markers and block kinds predate
/// the typed contract, so resuming them could merge, reclassify or publish the
/// wrong blocks. They must be reviewed/re-imported explicitly.
pub fn migrate_checkpoint(
    mut checkpoint: ImportCheckpointState,
) -> Result<ImportCheckpointState, AppError> {
    if checkpoint.schema_version > CHECKPOINT_SCHEMA_VERSION {
        return Err(AppError::validation(format!(
            "该导入断点由更新版本（schema {}）创建，本版本无法安全读取；请升级应用或新建题目集重新导入",
            checkpoint.schema_version
        )));
    }
    let has_page_cache = checkpoint
        .vlm_page_results
        .iter()
        .any(|result| !result.trim().is_empty());
    let has_structure_cache = checkpoint
        .structured_batch_results
        .iter()
        .any(|result| !result.trim().is_empty())
        || checkpoint
            .candidate_results
            .iter()
            .any(|result| !result.trim().is_empty());
    let has_cached_results = has_page_cache || has_structure_cache;
    if checkpoint.schema_version < CHECKPOINT_SCHEMA_VERSION {
        if has_cached_results {
            return Err(AppError::validation(
                "旧版导入断点含有未版本化的页面/结构化缓存，类型解释不一致，不能安全自动续导；请保留原题目集并把原文档重新导入到新题目集核对",
            ));
        }
        checkpoint.schema_version = CHECKPOINT_SCHEMA_VERSION;
    }
    // Version gates are real gates, not decorative fields. Cached stage output
    // produced by an unknown/different version is never reused: fail closed with
    // the exact stage reason so the UI can ask for review/re-import.
    if has_cached_results {
        if let Some((stage, stored, current)) = cached_stage_version_mismatch(
            has_page_cache,
            has_structure_cache,
            checkpoint.pipeline_version,
            checkpoint.parser_version,
            checkpoint.prompt_version,
            checkpoint.structurer_version,
        ) {
            return Err(AppError::validation(format!(
                "导入断点 {stage} 版本 {stored} 与当前 {current} 不一致，缓存不能复用；请重新导入或复核"
            )));
        }
    } else {
        // Nothing cached: stamp the current contract before any reuse happens.
        checkpoint.pipeline_version = PIPELINE_VERSION;
        checkpoint.parser_version = PARSER_VERSION;
        checkpoint.prompt_version = PROMPT_VERSION;
        checkpoint.structurer_version = STRUCTURER_VERSION;
    }
    if checkpoint.input_fingerprint.trim().is_empty() {
        checkpoint.input_fingerprint = checkpoint_fingerprint(&checkpoint);
    } else {
        let current = checkpoint_fingerprint(&checkpoint);
        if current != checkpoint.input_fingerprint {
            return Err(AppError::validation(
                "导入断点记录的实际输入指纹与当前输入不一致，拒绝复用该断点；请重新导入或复核",
            ));
        }
    }
    if checkpoint.attempt_id.trim().is_empty() {
        checkpoint.attempt_id = uuid::Uuid::new_v4().to_string();
    }
    checkpoint.revision = checkpoint.revision.saturating_add(1);
    Ok(checkpoint)
}

/// A stable fingerprint of the imported input, independent of page numbering.
pub fn checkpoint_fingerprint(checkpoint: &ImportCheckpointState) -> String {
    fingerprint_from_parts(
        &checkpoint.source_document_id,
        &checkpoint.page_blob_hashes,
        &checkpoint.text_content,
        &checkpoint.docx_vlm_direct_blob_hashes,
        &checkpoint.docx_vlm_direct_text,
    )
}

fn fingerprint_from_parts(
    source_document_id: &str,
    page_blob_hashes: &[String],
    text_content: &str,
    docx_blob_hashes: &[String],
    docx_text: &str,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    if !source_document_id.is_empty() {
        hash.update(source_document_id.as_bytes());
    }
    for page in page_blob_hashes {
        hash.update(page.as_bytes());
    }
    if !text_content.is_empty() {
        hash.update(text_content.as_bytes());
    }
    for blob in docx_blob_hashes {
        hash.update(blob.as_bytes());
    }
    if !docx_text.is_empty() {
        hash.update(docx_text.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

/// The single pure stage-version rule shared by migration and the read-only
/// resume-capability view. Returns `(stage, stored, current)` on mismatch.
fn cached_stage_version_mismatch(
    has_page_cache: bool,
    has_structure_cache: bool,
    pipeline_version: u32,
    parser_version: u32,
    prompt_version: u32,
    structurer_version: u32,
) -> Option<(&'static str, u32, u32)> {
    if !has_page_cache && !has_structure_cache {
        return None;
    }
    if pipeline_version != PIPELINE_VERSION {
        return Some(("pipeline", pipeline_version, PIPELINE_VERSION));
    }
    if has_page_cache && parser_version != PARSER_VERSION {
        return Some(("parser", parser_version, PARSER_VERSION));
    }
    if has_page_cache && prompt_version != PROMPT_VERSION {
        return Some(("prompt", prompt_version, PROMPT_VERSION));
    }
    if has_structure_cache && structurer_version != STRUCTURER_VERSION {
        return Some(("structurer", structurer_version, STRUCTURER_VERSION));
    }
    None
}

/// Compare-and-swap checkpoint write. `expected` is the exact JSON string that was
/// read (None means the session must currently have no checkpoint). A stale writer
/// gets `false` instead of overwriting a competing attempt.
pub fn persist_checkpoint_cas(
    db: &VfsDatabase,
    session_id: &str,
    checkpoint: &ImportCheckpointState,
    expected: Option<&str>,
) -> Result<bool, AppError> {
    let mut value = serde_json::to_value(checkpoint)
        .map_err(|e| AppError::validation(format!("序列化导入状态失败: {e}")))?;
    // Keep a monotonically increasing revision in the stored payload so readers
    // can distinguish an older attempt even after a process restart.
    value["revision"] = json!(checkpoint.revision);
    let state = value.to_string();
    let conn = db
        .get_conn_safe()
        .map_err(|e| AppError::database(format!("保存导入断点连接失败: {e}")))?;
    let affected = match expected {
        Some(expected) => conn.execute(
            "UPDATE exam_sheets SET import_state_json=?1, updated_at=?2 \
             WHERE id=?3 AND deleted_at IS NULL AND import_state_json IS ?4",
            params![state, chrono::Utc::now().to_rfc3339(), session_id, expected],
        ),
        None => conn.execute(
            "UPDATE exam_sheets SET import_state_json=?1, updated_at=?2 \
             WHERE id=?3 AND deleted_at IS NULL AND (import_state_json IS NULL OR trim(import_state_json)='')",
            params![state, chrono::Utc::now().to_rfc3339(), session_id],
        ),
    }
    .map_err(|e| AppError::database(format!("保存导入断点失败: {e}")))?;
    Ok(affected == 1)
}

/// What a persisted checkpoint can actually re-execute. A report action may only
/// be advertised as an executable resume when the checkpoint supports that stage;
/// otherwise the UI must ask for a review/re-import instead.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumeCapabilities {
    pub reidentify_pages: bool,
    pub retry_batches: bool,
    pub retry_figures: bool,
}

impl ResumeCapabilities {
    pub fn supports(&self, action: &str) -> bool {
        match action {
            "reidentify_page" => self.reidentify_pages,
            "retry_batches" => self.retry_batches,
            "retry_figures" => self.retry_figures,
            _ => false,
        }
    }
}

pub fn checkpoint_resume_capabilities(raw: Option<&str>) -> ResumeCapabilities {
    let Some(raw) = raw
        .map(str::trim)
        .filter(|raw| !raw.is_empty() && *raw != "null")
    else {
        return ResumeCapabilities::default();
    };
    let Ok(checkpoint) = serde_json::from_str::<Value>(raw) else {
        return ResumeCapabilities::default();
    };
    let version = checkpoint["schema_version"].as_u64().unwrap_or(0) as u32;
    let has_cached_results = [
        "vlm_page_results",
        "structured_batch_results",
        "candidate_results",
    ]
    .iter()
    .any(|key| {
        checkpoint[*key].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| !item.as_str().unwrap_or("").trim().is_empty())
        })
    });
    // Legacy caches cannot be reinterpreted, and newer schemas are unknown.
    if (version == 0 && has_cached_results) || version > CHECKPOINT_SCHEMA_VERSION {
        return ResumeCapabilities::default();
    }
    let has_page_cache = checkpoint["vlm_page_results"]
        .as_array()
        .is_some_and(|items| {
            items
                .iter()
                .any(|item| !item.as_str().unwrap_or("").trim().is_empty())
        });
    let has_structure_cache = ["structured_batch_results", "candidate_results"]
        .iter()
        .any(|key| {
            checkpoint[*key].as_array().is_some_and(|items| {
                items
                    .iter()
                    .any(|item| !item.as_str().unwrap_or("").trim().is_empty())
            })
        });
    let stage_u32 = |key: &str| checkpoint[key].as_u64().unwrap_or(0) as u32;
    if cached_stage_version_mismatch(
        has_page_cache,
        has_structure_cache,
        stage_u32("pipeline_version"),
        stage_u32("parser_version"),
        stage_u32("prompt_version"),
        stage_u32("structurer_version"),
    )
    .is_some()
    {
        return ResumeCapabilities::default();
    }
    let strings = |key: &str| -> Vec<String> {
        checkpoint[key]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    let stored_fingerprint = checkpoint["input_fingerprint"].as_str().unwrap_or("");
    if !stored_fingerprint.trim().is_empty() {
        let current = fingerprint_from_parts(
            checkpoint["source_document_id"].as_str().unwrap_or(""),
            &strings("page_blob_hashes"),
            checkpoint["text_content"].as_str().unwrap_or(""),
            &strings("docx_vlm_direct_blob_hashes"),
            checkpoint["docx_vlm_direct_text"].as_str().unwrap_or(""),
        );
        if current != stored_fingerprint {
            return ResumeCapabilities::default();
        }
    }
    let visual = checkpoint["page_blob_hashes"]
        .as_array()
        .is_some_and(|pages| !pages.is_empty());
    let text = checkpoint["import_mode"].as_str() == Some("text")
        || !checkpoint["text_content"].as_str().unwrap_or("").is_empty();
    let docx = !checkpoint["docx_vlm_direct_blob_hashes"]
        .as_array()
        .map(|items| items.is_empty())
        .unwrap_or(true)
        || !checkpoint["docx_enriched_text"]
            .as_str()
            .unwrap_or("")
            .is_empty();
    ResumeCapabilities {
        reidentify_pages: visual,
        retry_batches: visual || text || docx,
        retry_figures: visual,
    }
}

/// Filter report actions by executable checkpoint capabilities. Dropped blocking
/// actions are replaced by an explicit review/re-import action so the frontend
/// never shows a resume button that cannot fix the reported problem.
pub fn filter_executable_recovery_actions(
    actions: Vec<Value>,
    capabilities: ResumeCapabilities,
) -> Vec<Value> {
    let mut filtered = Vec::with_capacity(actions.len());
    let mut dropped_blocking = false;
    for action in actions {
        let name = action["action"].as_str().unwrap_or_default();
        if is_executable_action(name) {
            if capabilities.supports(name) {
                filtered.push(action);
            } else {
                dropped_blocking |= action["severity"].as_str() == Some("blocking");
            }
        } else {
            filtered.push(action);
        }
    }
    if dropped_blocking {
        filtered.push(json!({
            "action": "reimport_or_review_invalid_checkpoint",
            "reason": "checkpoint_incompatible_with_recovery",
            "severity": "blocking",
        }));
    }
    filtered
}

pub fn is_executable_action(action: &str) -> bool {
    matches!(
        action,
        "reidentify_page" | "retry_batches" | "retry_figures"
    )
}

/// Unconditional checkpoint write for one owned attempt, but still refuses to
/// write into a missing or soft-deleted session.
pub fn persist_checkpoint_live(
    db: &VfsDatabase,
    session_id: &str,
    checkpoint: &ImportCheckpointState,
) -> Result<(), AppError> {
    let value = serde_json::to_value(checkpoint)
        .map_err(|error| AppError::validation(format!("序列化导入状态失败: {error}")))?;
    let conn = db
        .get_conn_safe()
        .map_err(|error| AppError::database(format!("保存导入状态连接失败: {error}")))?;
    let affected = conn
        .execute(
            "UPDATE exam_sheets SET import_state_json=?1, updated_at=?2 \
             WHERE id=?3 AND deleted_at IS NULL",
            params![
                value.to_string(),
                chrono::Utc::now().to_rfc3339(),
                session_id
            ],
        )
        .map_err(|error| AppError::database(format!("保存导入状态失败: {error}")))?;
    if affected != 1 {
        return Err(AppError::database(
            "导入会话不存在或已删除，拒绝写入断点".to_string(),
        ));
    }
    Ok(())
}

/// Report-facing recovery plan. A boolean "resume_available" is not actionable;
/// each entry names a concrete step and the evidence that triggered it.
pub fn recovery_actions(report: &Value) -> Vec<Value> {
    let mut actions = Vec::new();
    let pages: Vec<Value> = report["failed_pages"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if !pages.is_empty() {
        actions.push(json!({
            "action": "reidentify_page",
            "pages": pages,
            "reason": "page_recognition_failed",
            "severity": "blocking",
        }));
    }
    let mut batches = Vec::new();
    for key in ["failed_batches", "incomplete_batches", "failed_chunks"] {
        if let Some(items) = report[key].as_array() {
            for item in items {
                if !batches.contains(item) {
                    batches.push(item.clone());
                }
            }
        }
    }
    if !batches.is_empty() {
        actions.push(json!({
            "action": "retry_batches",
            "batch_ids": batches,
            "reason": "structure_batch_incomplete",
            "severity": "blocking",
        }));
    }
    if let Some(figures) = report["figure_failures"]
        .as_array()
        .filter(|f| !f.is_empty())
    {
        actions.push(json!({
            "action": "retry_figures",
            "source_ids": figures.iter().filter_map(|f| f["source_id"].as_str()).collect::<Vec<_>>(),
            "reason": "figure_extraction_failed",
            "severity": "blocking",
        }));
    }
    if let Some(deferred) = report["deferred_continuations"]
        .as_array()
        .filter(|d| !d.is_empty())
    {
        // A cached page whose VLM blocks still claim an unresolved continuation
        // must be re-recognised (plus adjacent pages), not merely reviewed: the
        // boundary itself can be wrong. Without page evidence, keep it review-only.
        let mut pages: Vec<Value> = Vec::new();
        for entry in deferred {
            if let Some(spans) = entry["page_spans"].as_array() {
                for span in spans {
                    if let Some(page) = span["page_index"].as_u64() {
                        if !pages.contains(&json!(page)) {
                            pages.push(json!(page));
                        }
                    }
                }
            }
        }
        let source_ids: Vec<&str> = deferred
            .iter()
            .filter_map(|entry| entry["source_id"].as_str())
            .collect();
        if pages.is_empty() {
            actions.push(json!({
                "action": "review_continuation",
                "source_ids": source_ids,
                "reason": "unresolved_page_continuation",
                "severity": "blocking",
            }));
        } else {
            actions.push(json!({
                "action": "reidentify_page",
                "pages": pages,
                "source_ids": source_ids,
                "reason": "unresolved_page_continuation",
                "severity": "blocking",
            }));
        }
    }
    let mut review_sources = Vec::new();
    for key in ["quarantined_entries", "conflicts", "identity_risks"] {
        if let Some(entries) = report[key].as_array() {
            for entry in entries {
                if let Some(source) = entry["source_id"].as_str() {
                    if !review_sources.contains(&json!(source)) {
                        review_sources.push(json!(source));
                    }
                }
            }
        }
    }
    if !review_sources.is_empty() {
        actions.push(json!({
            "action": "review_ambiguity",
            "source_ids": review_sources,
            "reason": "source_evidence_needs_review",
            "severity": "blocking",
        }));
    }
    if report["groups"]["blocking_conflict_count"]
        .as_u64()
        .is_some_and(|count| count > 0)
    {
        actions.push(json!({
            "action": "review_group_conflict",
            "reason": "group_or_material_conflict",
            "severity": "blocking",
        }));
    }
    if let Some(reason) = report["empty_import_reason"].as_str() {
        if reason != "all_sources_excluded_by_user" {
            actions.push(json!({
                "action": "reimport_or_review_empty_result",
                "reason": reason,
                "severity": "blocking",
            }));
        }
    }
    let _ = report; // keep the signature stable for future stage-specific fields
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::database::setup_migrated_test_db;
    use crate::vfs::repos::VfsExamRepo;
    use crate::vfs::types::VfsCreateExamSheetParams;

    fn exam(db: &VfsDatabase, status: &str, checkpoint: Option<Value>) -> String {
        let sheet = VfsExamRepo::create_exam_sheet(
            db,
            VfsCreateExamSheetParams {
                exam_name: Some("会话门禁合成".into()),
                temp_id: "guard-synthetic".into(),
                metadata_json: json!({"import_mode":"visual"}),
                preview_json: json!({"temp_id":"guard-synthetic","exam_name":"会话门禁合成","pages":[]}),
                status: status.into(),
                folder_id: None,
            },
        )
        .unwrap();
        if let Some(checkpoint) = checkpoint {
            VfsExamRepo::update_import_state(db, &sheet.id, &checkpoint).unwrap();
        }
        sheet.id
    }

    fn checkpoint(schema_version: u32, cached: bool) -> ImportCheckpointState {
        ImportCheckpointState {
            qbank_name: "会话门禁合成".into(),
            model_config_id: None,
            source_format: "pdf".into(),
            current_stage: super::super::ImportStage::VlmAnalyzing,
            page_blob_hashes: vec!["page-a".into()],
            vlm_pages_completed: usize::from(cached),
            vlm_page_results: if cached {
                vec![json!({"questions":[{"block_kind":"question"}]}).to_string()]
            } else {
                vec![String::new()]
            },
            structuring_batches_completed: 0,
            structured_batch_results: vec![],
            page_dimensions: vec![(100, 100)],
            source_image_hashes: vec![],
            import_mode: "visual".into(),
            text_content: String::new(),
            chunks_total: 0,
            chunks_completed: 0,
            docx_image_entries: vec![],
            docx_vlm_descriptions: Default::default(),
            docx_vlm_images_completed: 0,
            docx_enriched_text: String::new(),
            docx_marker_positions: vec![],
            docx_vlm_direct_blob_hashes: vec![],
            docx_vlm_direct_mimes: vec![],
            docx_vlm_direct_exts: vec![],
            docx_vlm_direct_text: String::new(),
            docx_vlm_direct_saved: 0,
            source_document_id: "doc-synthetic".into(),
            candidate_results: vec![],
            failed_chunks: vec![],
            failed_pages: vec![],
            schema_version,
            pipeline_version: 0,
            input_fingerprint: String::new(),
            attempt_id: String::new(),
            revision: 0,
            parser_version: 0,
            prompt_version: 0,
            structurer_version: 0,
        }
    }

    #[test]
    fn legacy_checkpoint_with_cached_results_is_rejected_not_silently_rerun() {
        let error = migrate_checkpoint(checkpoint(0, true))
            .unwrap_err()
            .to_string();
        assert!(error.contains("旧版导入断点"), "{error}");
    }

    #[test]
    fn fresh_legacy_checkpoint_is_migrated_with_versions_and_fingerprint() {
        let migrated = migrate_checkpoint(checkpoint(0, false)).unwrap();
        assert_eq!(migrated.schema_version, CHECKPOINT_SCHEMA_VERSION);
        assert_eq!(migrated.pipeline_version, PIPELINE_VERSION);
        assert!(!migrated.input_fingerprint.is_empty());
        assert!(!migrated.attempt_id.is_empty());
        assert_eq!(migrated.revision, 1);
        assert_eq!(migrated.parser_version, PARSER_VERSION);
        assert_eq!(migrated.prompt_version, PROMPT_VERSION);
        assert_eq!(migrated.structurer_version, STRUCTURER_VERSION);
    }

    fn current_versions(cp: &mut ImportCheckpointState) {
        cp.pipeline_version = PIPELINE_VERSION;
        cp.parser_version = PARSER_VERSION;
        cp.prompt_version = PROMPT_VERSION;
        cp.structurer_version = STRUCTURER_VERSION;
    }

    #[test]
    fn cached_checkpoint_with_matching_versions_is_reused() {
        let mut cp = checkpoint(2, true);
        current_versions(&mut cp);
        let migrated = migrate_checkpoint(cp).expect("matching cached versions are reusable");
        assert_eq!(migrated.pipeline_version, PIPELINE_VERSION);
    }

    #[test]
    fn unversioned_page_cache_is_never_silently_reused() {
        let mut cp = checkpoint(2, true);
        current_versions(&mut cp);
        cp.prompt_version = 0;
        let error = migrate_checkpoint(cp).unwrap_err().to_string();
        assert!(error.contains("prompt 版本"), "{error}");

        let mut cp = checkpoint(2, true);
        current_versions(&mut cp);
        cp.pipeline_version = 0;
        let error = migrate_checkpoint(cp).unwrap_err().to_string();
        assert!(error.contains("pipeline 版本"), "{error}");

        let mut cp = checkpoint(2, true);
        current_versions(&mut cp);
        cp.parser_version = 0;
        let error = migrate_checkpoint(cp).unwrap_err().to_string();
        assert!(error.contains("parser 版本"), "{error}");
    }

    #[test]
    fn structure_cache_requires_matching_structurer_version() {
        let mut cp = checkpoint(2, false);
        current_versions(&mut cp);
        cp.structured_batch_results = vec![json!([{"content":"synthetic"}]).to_string()];
        cp.structurer_version = 0;
        let error = migrate_checkpoint(cp).unwrap_err().to_string();
        assert!(error.contains("structurer 版本"), "{error}");
    }

    #[test]
    fn stored_input_fingerprint_must_match_current_input() {
        let mut cp = checkpoint(2, false);
        current_versions(&mut cp);
        cp.input_fingerprint = "deadbeef".to_string();
        let error = migrate_checkpoint(cp).unwrap_err().to_string();
        assert!(error.contains("指纹"), "{error}");
    }

    #[test]
    fn newer_checkpoint_schema_is_fail_closed() {
        let error = migrate_checkpoint(checkpoint(CHECKPOINT_SCHEMA_VERSION + 1, false))
            .unwrap_err()
            .to_string();
        assert!(error.contains("更新版本"), "{error}");
    }

    #[test]
    fn deleted_or_missing_sessions_are_rejected_before_work() {
        let (_temp, db) = setup_migrated_test_db();
        assert!(validate_session_for_operation(
            &db,
            "missing-session",
            SessionOperation::Resume,
            "visual"
        )
        .is_err());
        let id = exam(&db, "needs_review", None);
        {
            let conn = db.get_conn_safe().unwrap();
            conn.execute(
                "UPDATE exam_sheets SET deleted_at=?1 WHERE id=?2",
                params!["2026-10-21T00:00:00Z", id],
            )
            .unwrap();
        }
        let error =
            validate_session_for_operation(&db, &id, SessionOperation::AppendDocument, "visual")
                .unwrap_err()
                .to_string();
        assert!(error.contains("已删除"), "{error}");
    }

    #[test]
    fn resume_requires_a_checkpoint_and_a_resumable_status() {
        let (_temp, db) = setup_migrated_test_db();
        let no_checkpoint = exam(&db, "needs_review", None);
        let error =
            validate_session_for_operation(&db, &no_checkpoint, SessionOperation::Resume, "visual")
                .unwrap_err()
                .to_string();
        assert!(error.contains("没有可恢复"), "{error}");

        let completed = exam(&db, "completed", Some(json!({"schema_version":2})));
        let error =
            validate_session_for_operation(&db, &completed, SessionOperation::Resume, "visual")
                .unwrap_err()
                .to_string();
        assert!(error.contains("不允许续导"), "{error}");

        let recoverable = exam(&db, "needs_review", Some(json!({"schema_version":2})));
        let snapshot =
            validate_session_for_operation(&db, &recoverable, SessionOperation::Resume, "visual")
                .unwrap();
        assert_eq!(snapshot.status, "needs_review");
        // Appending a new document to a finished bank without an active
        // checkpoint is allowed; an unfinished checkpoint must be resumed first.
        let finished = exam(&db, "completed", None);
        assert!(validate_session_for_operation(
            &db,
            &finished,
            SessionOperation::AppendDocument,
            ""
        )
        .is_ok());
        let error =
            validate_session_for_operation(&db, &recoverable, SessionOperation::AppendDocument, "")
                .unwrap_err()
                .to_string();
        assert!(error.contains("未完成的导入断点"), "{error}");
    }

    #[test]
    fn checkpoint_cas_rejects_a_stale_writer_and_deleted_session() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(
            &db,
            "importing",
            Some(json!({"schema_version":2,"revision":1})),
        );
        let mut current = checkpoint(2, false);
        current.revision = 2;
        let first = persist_checkpoint_cas(
            &db,
            &id,
            &current,
            Some("{\"schema_version\":2,\"revision\":1}"),
        )
        .unwrap();
        assert!(first);
        // The same expectation no longer matches: a competing attempt cannot
        // overwrite the checkpoint it did not read.
        current.revision = 3;
        assert!(!persist_checkpoint_cas(
            &db,
            &id,
            &current,
            Some("{\"schema_version\":2,\"revision\":1}")
        )
        .unwrap());
        {
            let conn = db.get_conn_safe().unwrap();
            conn.execute(
                "UPDATE exam_sheets SET deleted_at=?1 WHERE id=?2",
                params!["2026-10-21T00:00:00Z", id],
            )
            .unwrap();
        }
        assert!(!persist_checkpoint_cas(&db, &id, &current, None).unwrap());
    }

    #[test]
    fn session_lease_rejects_concurrent_imports_and_releases_on_drop() {
        let first = SessionLease::try_acquire("synthetic-session").unwrap();
        assert!(SessionLease::try_acquire("synthetic-session").is_err());
        assert!(SessionLease::try_acquire("another-session").is_ok());
        drop(first);
        assert!(SessionLease::try_acquire("synthetic-session").is_ok());
    }

    #[test]
    fn resume_capabilities_follow_the_checkpoint_not_the_report_label() {
        let visual = json!({
            "schema_version": CHECKPOINT_SCHEMA_VERSION,
            "page_blob_hashes": ["page-a"],
            "vlm_page_results": [""],
        })
        .to_string();
        let caps = checkpoint_resume_capabilities(Some(&visual));
        assert!(caps.reidentify_pages && caps.retry_batches && caps.retry_figures);

        let text = json!({
            "schema_version": CHECKPOINT_SCHEMA_VERSION,
            "import_mode": "text",
            "text_content": "synthetic",
        })
        .to_string();
        let caps = checkpoint_resume_capabilities(Some(&text));
        assert!(!caps.reidentify_pages && caps.retry_batches && !caps.retry_figures);

        let legacy_cached = json!({
            "schema_version": 0,
            "page_blob_hashes": ["page-a"],
            "vlm_page_results": ["{\"questions\":[]}"],
        })
        .to_string();
        assert_eq!(
            checkpoint_resume_capabilities(Some(&legacy_cached)),
            ResumeCapabilities::default()
        );
        assert_eq!(
            checkpoint_resume_capabilities(Some(
                &json!({"schema_version": CHECKPOINT_SCHEMA_VERSION + 1}).to_string()
            )),
            ResumeCapabilities::default()
        );
    }

    #[test]
    fn capabilities_reject_stage_version_and_fingerprint_mismatch() {
        let fingerprint = fingerprint_from_parts("doc-cap", &["page-a".to_string()], "", &[], "");
        let base = json!({
            "schema_version": CHECKPOINT_SCHEMA_VERSION,
            "pipeline_version": PIPELINE_VERSION,
            "parser_version": PARSER_VERSION,
            "prompt_version": PROMPT_VERSION,
            "structurer_version": STRUCTURER_VERSION,
            "page_blob_hashes": ["page-a"],
            "vlm_page_results": ["{\"questions\":[]}"],
            "source_document_id": "doc-cap",
            "input_fingerprint": fingerprint,
        });
        let caps = checkpoint_resume_capabilities(Some(&base.to_string()));
        assert!(caps.reidentify_pages && caps.retry_batches && caps.retry_figures);

        let mut stale_prompt = base.clone();
        stale_prompt["prompt_version"] = json!(0);
        assert_eq!(
            checkpoint_resume_capabilities(Some(&stale_prompt.to_string())),
            ResumeCapabilities::default(),
            "a stale prompt cache must not offer an executable resume"
        );

        let mut changed_input = base.clone();
        changed_input["input_fingerprint"] = json!("deadbeef");
        assert_eq!(
            checkpoint_resume_capabilities(Some(&changed_input.to_string())),
            ResumeCapabilities::default()
        );
    }

    #[test]
    fn actions_are_dropped_when_the_checkpoint_cannot_execute_them() {
        let actions = vec![
            json!({"action":"reidentify_page","pages":[2],"severity":"blocking"}),
            json!({"action":"review_ambiguity","severity":"blocking"}),
        ];
        let filtered = filter_executable_recovery_actions(actions, ResumeCapabilities::default());
        assert!(!filtered.iter().any(|a| a["action"] == "reidentify_page"));
        assert!(filtered
            .iter()
            .any(|a| a["action"] == "reimport_or_review_invalid_checkpoint"));
        assert!(filtered.iter().any(|a| a["action"] == "review_ambiguity"));
    }

    #[test]
    fn recovery_actions_name_concrete_steps_and_skip_user_exclusions() {
        let report = json!({
            "failed_pages": [2],
            "incomplete_batches": [1],
            "figure_failures": [{"source_id":"q7"}],
            "deferred_continuations": [{"source_id":"q8"}],
            "quarantined_entries": [{"source_id":"q9","reason":"ambiguous_question_identity"}],
            "empty_import_reason": Value::Null,
        });
        let actions = recovery_actions(&report);
        let names: Vec<_> = actions
            .iter()
            .filter_map(|action| action["action"].as_str())
            .collect();
        assert!(names.contains(&"reidentify_page"));
        assert!(names.contains(&"retry_batches"));
        assert!(names.contains(&"retry_figures"));
        assert!(names.contains(&"review_continuation"));
        assert!(names.contains(&"review_ambiguity"));
        let continuation_pages = recovery_actions(&json!({
            "deferred_continuations": [{
                "source_id": "q10",
                "page_spans": [{"page_index": 3}, {"page_index": 4}],
                "reason": "unresolved_continuation"
            }]
        }));
        let reidentify = continuation_pages
            .iter()
            .find(|action| action["action"] == "reidentify_page")
            .expect("cached continuation pages need re-recognition");
        assert_eq!(reidentify["pages"], json!([3, 4]));
        assert_eq!(reidentify["reason"], "unresolved_page_continuation");

        let excluded = recovery_actions(&json!({
            "empty_import_reason": "all_sources_excluded_by_user"
        }));
        assert!(excluded.is_empty(), "user exclusion is a terminal choice");
    }
}
