//! Atomic publication: questions are authoritative; preview and stats are projections.

use super::*;
use crate::llm_structurer::StructuredQuestion;
use crate::vfs::repos::{Question as PersistedQuestion, UpdateQuestionParams};
use rusqlite::{params, Connection, TransactionBehavior};

/// The only document-import publication boundary. No model calls and no raw-text
/// promotion are allowed here. BEGIN IMMEDIATE serializes source-id lookups with
/// inserts, including tombstones; the RAII transaction rolls back on every error.
#[allow(clippy::too_many_arguments)]
pub(super) fn publish_candidates(
    vfs_db: &VfsDatabase,
    session_id: &str,
    qbank_name: &str,
    candidates: &[StructuredQuestion],
    degenerate: usize,
    pages: &[PageSlice],
    report: Value,
    progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
) -> Result<ImportResult, AppError> {
    publish_candidates_with_groups(
        vfs_db,
        session_id,
        qbank_name,
        candidates,
        degenerate,
        pages,
        report,
        progress_tx,
        None,
    )
}

/// Publication entry point that can receive typed group/material blocks captured
/// *before* answer merging. Groups, materials and their child questions are
/// published inside the same transaction as standalone questions.
#[allow(clippy::too_many_arguments)]
pub(super) fn publish_candidates_with_groups(
    vfs_db: &VfsDatabase,
    session_id: &str,
    qbank_name: &str,
    candidates: &[StructuredQuestion],
    degenerate: usize,
    pages: &[PageSlice],
    mut report: Value,
    progress_tx: Option<&UnboundedSender<QuestionImportProgress>>,
    typed_blocks: Option<Vec<super::groups::TypedBlock>>,
) -> Result<ImportResult, AppError> {
    if !report.is_object() {
        report = json!({"input_report": report});
    }
    let document_id = report["document_id"].as_str().unwrap_or("").to_string();
    let (structured, outcome, merge_items) =
        QuestionImportService::merge_answer_sections(candidates);
    // Group/material blocks are extracted from the typed blocks *before* the
    // publisher filters out non-question candidates. Standalone publication must
    // never see a material pretending to be a question, and a group child must
    // only be published through its group's transaction.
    let group_key =
        super::groups::GroupImportKey::new_session(session_id, &document_id, session_id);
    let group_blocks = {
        let mut blocks = match typed_blocks {
            Some(blocks) => blocks,
            None => super::groups::blocks_from_merge_items(&merge_items),
        };
        super::groups::enrich_with_structured(&mut blocks, &structured);
        blocks
    };
    let group_proposals = super::groups::proposals_from_hints(&group_blocks);
    let group_assembly =
        super::groups::assemble_groups(&group_key, &group_blocks, &group_proposals);
    let group_reserved: HashSet<String> = group_assembly
        .planned_child_sources
        .iter()
        .chain(group_assembly.blocked_child_sources.iter())
        .cloned()
        .collect();
    for field in [
        "quarantined_entries",
        "conflicts",
        "identity_risks",
        "matched_entries",
        "excluded_entries",
        "safe_fill_entries",
        "preserved_edits",
    ] {
        ensure_array(&mut report, field);
    }
    for (field, entries) in [
        (
            "quarantined_entries",
            serde_json::to_value(&outcome.quarantined),
        ),
        ("conflicts", serde_json::to_value(&outcome.conflicts)),
        ("matched_entries", serde_json::to_value(&outcome.matched)),
        ("excluded_entries", serde_json::to_value(&outcome.excluded)),
    ] {
        let entries =
            entries.map_err(|e| AppError::validation(format!("序列化导入审计失败: {e}")))?;
        for mut entry in entries.as_array().cloned().unwrap_or_default() {
            if let Some(sq) = candidates.iter().find(|sq| {
                entry["source_id"].as_str() == Some(sq.source.merged.question.source_id.as_str())
            }) {
                entry["source"] = source_audit(sq, &document_id, pages);
            }
            push_audit(&mut report, field, entry);
        }
    }
    report["answers_matched"] = json!(outcome.answers_matched);
    report["merge_outcome"] = serde_json::to_value(&outcome)
        .map_err(|e| AppError::validation(format!("序列化答案合并报告失败: {e}")))?;

    let mut conn = vfs_db
        .get_conn_safe()
        .map_err(|e| AppError::database(format!("获取导入数据库连接失败: {e}")))?;
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|e| AppError::database(format!("创建发布事务失败: {e}")))?;
    let (metadata_raw, preview_raw, checkpoint): (String, String, Option<String>) = tx.query_row(
        "SELECT metadata_json,preview_json,import_state_json FROM exam_sheets WHERE id=?1 AND deleted_at IS NULL",
        [session_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).map_err(|e| AppError::database(format!("读取导入会话失败: {e}")))?;
    let mut metadata: Value = serde_json::from_str(&metadata_raw)
        .map_err(|e| AppError::validation(format!("会话 metadata 无效，不能覆盖: {e}")))?;
    if !metadata.is_object() {
        metadata = json!({"legacy_metadata": metadata});
    }
    let old_document_id = metadata["import_report"]["document_id"]
        .as_str()
        .unwrap_or("")
        .to_string();
    // An append must not erase the previous document's durable source audit.
    if !old_document_id.is_empty() && old_document_id != document_id {
        let previous = metadata["import_report"].clone();
        if !metadata["import_report_history"].is_object() {
            let old_history = metadata["import_report_history"].take();
            metadata["import_report_history"] = json!({});
            if !old_history.is_null() {
                metadata["legacy_import_report_history"] = old_history;
            }
        }
        metadata["import_report_history"][&old_document_id] = previous;
    } else if old_document_id == document_id {
        // Resume may only contain failed batches. Keep already matched/excluded
        // evidence and durable safe-fill audit, but do not carry resolved failures
        // into the new quality gate.
        for key in ["matched_entries", "excluded_entries", "safe_fill_entries"] {
            if let Some(entries) = metadata["import_report"][key].as_array() {
                for entry in entries {
                    push_audit(&mut report, key, entry.clone());
                }
            }
        }
    }
    let mut preview = PreviewProjection::new(&preview_raw, session_id, qbank_name)?;
    let page_mapping =
        preview.add_document_pages(pages, &document_id, &old_document_id, &report)?;
    let base_order: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM questions WHERE exam_id=?1 AND deleted_at IS NULL",
            [session_id],
            |r| r.get(0),
        )
        .map_err(|e| AppError::database(format!("读取题目总数失败: {e}")))?;

    // Identical replayed source blocks are one identity. Differing payloads with
    // the same source id are ambiguous, not a reason to delete ordinary stems.
    let mut by_source: HashMap<&str, &StructuredQuestion> = HashMap::new();
    let mut ambiguous = HashSet::new();
    for sq in &structured {
        let block = &sq.source.merged.question;
        if let Some(previous) = by_source.insert(block.source_id.as_str(), sq) {
            let prior = &previous.source.merged.question;
            if previous.json != sq.json
                || prior.label != block.label
                || prior.section != block.section
                || prior.raw_text != block.raw_text
                || prior.block_kind != block.block_kind
                || prior.bbox != block.bbox
                || previous.source.merged.page_indices != sq.source.merged.page_indices
                || previous.source.merged.source_spans != sq.source.merged.source_spans
                || previous.source.images != sq.source.images
            {
                ambiguous.insert(block.source_id.as_str());
            }
        }
    }
    let donor_audits = donor_audit(&outcome, candidates);
    let originals: HashMap<&str, &StructuredQuestion> = candidates
        .iter()
        .map(|sq| (sq.source.merged.question.source_id.as_str(), sq))
        .collect();
    let mut group_outcome = super::groups::GroupPublishOutcome::default();
    if group_assembly.has_plans() {
        let mut planned_params: HashMap<String, (String, CreateQuestionParams)> = HashMap::new();
        for (index, sq) in structured.iter().enumerate() {
            let source_id = sq.source.merged.question.source_id.clone();
            if !group_assembly.planned_child_sources.contains(&source_id) {
                continue;
            }
            let card_id = stable_card_id(session_id, &document_id, &source_id);
            let Ok(mut params) = validated_params(sq, session_id, &card_id, index) else {
                continue;
            };
            params.source_ref = Some(
                make_source_ref(
                    sq,
                    &document_id,
                    pages,
                    &page_mapping,
                    base_order as usize + index + 1,
                )
                .to_string(),
            );
            params.question_label = Some(sq.source.merged.question.label.clone());
            params.images = Some(sq.source.images.clone());
            planned_params.insert(source_id, (card_id, params));
        }
        group_outcome =
            super::groups::publish_assembled_groups(&tx, &group_assembly, |_, child| {
                planned_params.get(&child.source_id).cloned()
            })
            .map_err(|error| AppError::database(format!("题组原子发布失败: {error}")))?;
    }
    let group_child_count = group_outcome.published_child_sources.len();
    let mut seen = HashSet::new();
    let mut published_ids = HashSet::new();
    let mut inserted = 0usize;
    let mut user_excluded_sources = 0usize;
    let mut safe_answer_fills = 0usize;
    let mut safe_explanation_fills = 0usize;
    let mut changed_question_ids: Vec<String> = group_outcome
        .receipts
        .iter()
        .filter(|receipt| !receipt.reused)
        .flat_map(|receipt| receipt.child_question_ids.iter().cloned())
        .collect();
    let mut committed_events = Vec::new();
    for (index, sq) in structured.iter().enumerate() {
        let block = &sq.source.merged.question;
        let identity_problem = if document_id.trim().is_empty() {
            Some("missing_document_id")
        } else if block.source_id.trim().is_empty() {
            Some("missing_source_id")
        } else if ambiguous.contains(block.source_id.as_str()) {
            Some("conflicting_source_identity")
        } else {
            None
        };
        if let Some(reason) = identity_problem {
            let entry = quarantine(sq, &document_id, pages, reason);
            push_audit(&mut report, "identity_risks", entry.clone());
            push_audit(&mut report, "quarantined_entries", entry);
            continue;
        }
        if group_reserved.contains(block.source_id.as_str())
            || group_outcome
                .published_child_sources
                .contains(block.source_id.as_str())
            || group_outcome
                .skipped_child_sources
                .contains(block.source_id.as_str())
        {
            // Published (or deliberately blocked) as a group child; publishing
            // it again standalone would duplicate the question and lose context.
            continue;
        }
        if block.block_kind != ImportBlockKind::Question {
            push_audit(
                &mut report,
                "quarantined_entries",
                quarantine(sq, &document_id, pages, "non_question_block"),
            );
            continue;
        }
        if !seen.insert(block.source_id.as_str()) {
            push_audit(
                &mut report,
                "excluded_entries",
                quarantine(sq, &document_id, pages, "identical_source_replay"),
            );
            continue;
        }
        let card_id = stable_card_id(session_id, &document_id, &block.source_id);
        let mut params = match validated_params(sq, session_id, &card_id, index) {
            Ok(params) => params,
            Err(reason) => {
                push_audit(
                    &mut report,
                    "quarantined_entries",
                    quarantine(sq, &document_id, pages, reason),
                );
                continue;
            }
        };
        let mut needs_question_review = false;
        let empty_donor_audit = DonorAudit::default();
        let donor_audit = donor_audits
            .get(block.source_id.as_str())
            .unwrap_or(&empty_donor_audit);
        let invalid_fields = invalid_answer(&mut params, sq, &donor_audit.unverified);
        if !invalid_fields.is_empty() {
            needs_question_review = true;
        }
        for (field, reason) in invalid_fields {
            let mut entry = quarantine(sq, &document_id, pages, reason);
            entry["card_id"] = json!(card_id);
            entry["field"] = json!(field);
            push_audit(&mut report, "quarantined_entries", entry.clone());
            push_audit(&mut report, "conflicts", entry);
            // Keep the valid question, never an unverified grading key or explanation.
        }
        let mut source_ref = make_source_ref(
            sq,
            &document_id,
            pages,
            &page_mapping,
            base_order as usize + index + 1,
        );
        if !pages.is_empty()
            && (sq.source.merged.page_indices.is_empty()
                || sq
                    .source
                    .merged
                    .page_indices
                    .iter()
                    .any(|p| !page_mapping.contains_key(p)))
        {
            push_audit(
                &mut report,
                "identity_risks",
                quarantine(sq, &document_id, pages, "unmapped_physical_source_page"),
            );
            needs_question_review = true;
        }
        if let Some(virtual_index) = preview.virtual_page_for(&document_id) {
            if pages.is_empty() {
                source_ref["virtual_page_index"] = json!(virtual_index);
            }
        }
        params.source_ref = Some(source_ref.to_string());
        params.question_label = Some(block.label.clone());
        params.images = Some(sq.source.images.clone());
        // Repository card lookup filters tombstones. This lookup intentionally
        // does not: a retry must not resurrect a user's excluded question.
        let existing: Vec<(String, Option<String>)> = {
            let mut stmt = tx
                .prepare("SELECT id,deleted_at FROM questions WHERE exam_id=?1 AND card_id=?2")
                .map_err(|e| AppError::database(format!("查找来源身份失败: {e}")))?;
            let rows = stmt
                .query_map(params![session_id, card_id], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| AppError::database(format!("查询已有题目失败: {e}")))?;
            rows.collect::<Result<_, _>>()
                .map_err(|e| AppError::database(format!("读取已有题目失败: {e}")))?
        };
        if existing.len() > 1 {
            push_audit(
                &mut report,
                "identity_risks",
                quarantine(sq, &document_id, pages, "duplicate_persisted_card_id"),
            );
            continue;
        }
        if let Some((id, deleted_at)) = existing.first() {
            if deleted_at.is_some() {
                user_excluded_sources += 1;
                let mut entry = quarantine(sq, &document_id, pages, "previously_soft_deleted");
                entry["question_id"] = json!(id);
                entry["card_id"] = json!(card_id);
                push_audit(&mut report, "excluded_entries", entry);
                continue;
            }
            let existing = VfsQuestionRepo::get_question_with_conn(&tx, id)
                .map_err(|e| AppError::database(format!("读取已有题目详情失败: {e}")))?
                .ok_or_else(|| AppError::database("已有题目在发布事务内消失".to_string()))?;
            let safe_fills = preserve_existing(
                &tx,
                &existing,
                &params,
                sq,
                &document_id,
                pages,
                &donor_audit.unverified,
                &donor_audit.provenance,
                originals.get(block.source_id.as_str()).copied(),
                &mut report,
            )?;
            safe_answer_fills += safe_fills.answer;
            safe_explanation_fills += safe_fills.explanation;
            if safe_fills.answer + safe_fills.explanation > 0 {
                changed_question_ids.push(existing.id.clone());
            }
            published_ids.insert(card_id);
            continue;
        }
        let mut question = VfsQuestionRepo::create_question_with_conn(&tx, &params)
            .map_err(|e| AppError::database(format!("写入导入题目失败: {e}")))?;
        needs_question_review |= outcome
            .conflicts
            .iter()
            .any(|conflict| conflict.target_source_id == block.source_id);
        if needs_question_review {
            question = VfsQuestionRepo::update_question_with_conn(
                &tx,
                &question.id,
                &UpdateQuestionParams {
                    status: Some(crate::vfs::repos::QuestionStatus::Review),
                    ..Default::default()
                },
            )
            .map_err(|e| AppError::database(format!("标记待审题目失败: {e}")))?;
        }
        inserted += 1;
        changed_question_ids.push(question.id.clone());
        published_ids.insert(card_id.clone());
        // Record what this successful import wrote, so a later re-import can
        // distinguish representation changes from real user edits.
        let mut initial_baseline = serde_json::Map::new();
        initial_baseline.insert(
            "answer".to_string(),
            json!(question.answer.clone().unwrap_or_default()),
        );
        initial_baseline.insert(
            "explanation".to_string(),
            json!(question.explanation.clone().unwrap_or_default()),
        );
        initial_baseline.insert("content".to_string(), json!(question.content.clone()));
        initial_baseline.insert(
            "question_label".to_string(),
            json!(question.question_label.clone().unwrap_or_default()),
        );
        persist_import_baseline(&tx, &question, initial_baseline)?;
        let mut event = serde_json::to_value(&question)
            .map_err(|e| AppError::validation(format!("序列化已发布题目失败: {e}")))?;
        event["card_id"] = json!(card_id);
        committed_events.push(event);
    }

    // Publication and indexing are separate states. Enqueue the changed question
    // ids in the same transaction (crash-safe ledger, no lost task window) and let
    // the worker index after commit; a pure replay enqueues nothing.
    changed_question_ids.sort();
    changed_question_ids.dedup();
    if !changed_question_ids.is_empty() {
        crate::vfs::question_index_ledger::QuestionIndexLedger::enqueue_question_units_with_conn(
            &tx,
            &changed_question_ids,
            chrono::Utc::now().timestamp_millis(),
        )
        .map_err(|e| AppError::database(format!("入队题目索引任务失败: {e}")))?;
    }
    let questions = read_all_questions(&tx, session_id)?;
    preview.project(&questions)?;
    VfsExamRepo::update_preview_json_with_conn(&tx, session_id, preview.root)
        .map_err(|e| AppError::database(format!("更新统一预览失败: {e}")))?;
    let stats = VfsQuestionRepo::refresh_stats_with_conn(&tx, session_id)
        .map_err(|e| AppError::database(format!("刷新发布统计失败: {e}")))?;
    if stats.total_count as usize != questions.len() {
        return Err(AppError::database("正式题目与统计数量不一致".to_string()));
    }
    let failed_count = coverage_failures(&report);
    let coverage_review = report["needs_review"].as_bool().unwrap_or(false)
        || collection_len(&report["unconfirmed_empty_pages"]) > 0
        || ["page_coverage", "chunk_coverage"].iter().any(|key| {
            report[*key].as_array().is_some_and(|entries| {
                entries.iter().any(|entry| {
                    matches!(
                        entry["state"].as_str(),
                        Some("needs_review" | "pending" | "processing")
                    )
                })
            })
        });
    let blocking_findings = ["quarantined_entries", "conflicts", "identity_risks"]
        .iter()
        .map(|key| {
            report[*key]
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|entry| {
                            !is_non_blocking_reason(entry["reason"].as_str().unwrap_or_default())
                        })
                        .count()
                })
                .unwrap_or(0)
        })
        .sum::<usize>();
    // Rule-recognised page furniture and explicit user exclusions are warnings,
    // not blockers. Only blocking findings can force review.
    let unresolved = blocking_findings > 0;
    // Deferred cross-page continuations and figure failures are explicit
    // incompleteness signals from the orchestrator. They can never be published
    // as a completed import: keep the report and the resumable checkpoint.
    let figure_failures = collection_len(&report["figure_failures"]);
    let deferred_continuations = collection_len(&report["deferred_continuations"]);
    let integrity_review = figure_failures > 0 || deferred_continuations > 0;
    // Zero published sources is never "completed". The reason matters: truly empty
    // input, candidates that were all non-question blocks, and a user's explicit
    // exclusion of every source are different terminal states.
    let published_total = published_ids.len() + group_child_count;
    let empty_import_reason: Option<&'static str> = if published_total == 0 {
        if structured.is_empty() {
            if candidates.is_empty() {
                Some("no_questions")
            } else {
                Some("no_publishable_questions")
            }
        } else if user_excluded_sources > 0 && user_excluded_sources >= structured.len() {
            Some("all_sources_excluded_by_user")
        } else {
            Some("no_publishable_questions")
        }
    } else {
        None
    };
    let group_review = group_assembly.is_blocking() || group_outcome.is_partial();
    let status = if failed_count > 0 && published_total == 0 {
        "failed"
    } else if failed_count > 0 {
        "partial"
    } else if published_total == 0 {
        if empty_import_reason == Some("all_sources_excluded_by_user") {
            "excluded"
        } else {
            "needs_review"
        }
    } else if unresolved || degenerate > 0 || coverage_review || integrity_review || group_review {
        "needs_review"
    } else {
        "completed"
    };
    let checkpoint_exists = checkpoint
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty() && value.trim() != "null");
    // Report content itself is durable: strip duplicated source material before
    // storing it. Counts and reasons were computed above from the unredacted
    // structure (keys are removed, array lengths are preserved).
    redact_sensitive(&mut report);
    report["schema_version"] = json!(1);
    report["status"] = json!(status);
    report["needs_review"] = json!(
        status == "needs_review"
            || unresolved
            || coverage_review
            || integrity_review
            || degenerate > 0
    );
    report["empty_import_reason"] = json!(empty_import_reason);
    report["publishable_count"] = json!(published_total);
    report["excluded_by_user_count"] = json!(user_excluded_sources);
    report["incomplete_batch_count"] = json!(collection_len(&report["incomplete_batches"]));
    report["groups"] = group_assembly.report();
    report["group_publish"] = group_outcome.report();
    report["group_child_published_count"] = json!(group_child_count);
    report["definition_version"] = json!(super::normalize::REPRESENTATION_NORMALIZATION_VERSION);
    annotate_report_severity(&mut report, status);
    // A labelled action is only executable when the persisted checkpoint can
    // actually re-run that stage; otherwise it becomes a review/re-import action.
    let capabilities = super::session_guard::checkpoint_resume_capabilities(checkpoint.as_deref());
    let actions = super::session_guard::filter_executable_recovery_actions(
        super::session_guard::recovery_actions(&report),
        capabilities,
    );
    report["recovery_actions"] = json!(actions.clone());
    // Only concrete stage retries are executable resumes. Review-only actions
    // (ambiguity, continuation review, empty-result review) do not pretend a
    // resume button can fix the import.
    let resume_executable = actions.iter().any(|action| {
        matches!(
            action["action"].as_str(),
            Some("reidentify_page" | "retry_batches" | "retry_figures")
        )
    });
    // A boolean alone is not an actionable resume signal: only concrete steps
    // count, and a user's exclusion or a completed import has none.
    report["resume_available"] = json!(
        checkpoint_exists
            && status != "completed"
            && status != "excluded"
            && !group_review
            && resume_executable
    );
    report["figure_failure_count"] = json!(figure_failures);
    report["deferred_continuation_count"] = json!(deferred_continuations);
    report["candidate_count"] = json!(candidates.len());
    report["question_candidate_count"] = json!(structured.len());
    report["published_count"] = json!(published_total);
    report["inserted_count"] = json!(inserted);
    report["new_count"] = json!(inserted);
    report["imported_count"] = json!(inserted);
    report["total_questions"] = json!(questions.len());
    report["total_count"] = json!(questions.len());
    report["safe_answer_fills"] = json!(safe_answer_fills);
    report["safe_explanation_fills"] = json!(safe_explanation_fills);
    report["safe_fill_count"] = json!(collection_len(&report["safe_fill_entries"]));
    report["degenerate_count"] = json!(degenerate);
    report["failed_count"] = json!(failed_count);
    report["quarantined_count"] = json!(collection_len(&report["quarantined_entries"]));
    report["excluded_count"] = json!(collection_len(&report["excluded_entries"]));
    // Convenient compatibility aliases carry the same evidence, not a second
    // independently filtered set of source records.
    report["matched"] = report["matched_entries"].clone();
    report["excluded"] = report["excluded_entries"].clone();
    metadata["import_report"] = report;
    let affected = tx.execute(
        "UPDATE exam_sheets SET metadata_json=?1, status=?2, import_state_json=CASE WHEN ?2='completed' THEN NULL ELSE import_state_json END, updated_at=?3 WHERE id=?4 AND deleted_at IS NULL",
        params![metadata.to_string(),status,chrono::Utc::now().to_rfc3339(),session_id],
    ).map_err(|e| AppError::database(format!("保存发布报告和断点失败: {e}")))?;
    if affected != 1 {
        return Err(AppError::database("导入会话不存在或已删除".to_string()));
    }
    tx.commit()
        .map_err(|e| AppError::database(format!("提交发布事务失败: {e}")))?;

    // Nothing observable as a published question escapes an uncommitted run.
    if let Some(sender) = progress_tx {
        for (index, question) in committed_events.into_iter().enumerate() {
            let _ = sender.send(QuestionImportProgress::QuestionParsed {
                question,
                question_index: index,
                total_parsed: index + 1,
            });
        }
        let _ = sender.send(QuestionImportProgress::Completed {
            session_id: session_id.to_string(),
            name: qbank_name.to_string(),
            total_questions: questions.len(),
            partial: status != "completed",
            failed_count,
            degenerate_count: degenerate,
        });
    }
    Ok(ImportResult {
        session_id: session_id.to_string(),
        name: qbank_name.to_string(),
        imported_count: inserted,
        total_questions: questions.len(),
    })
}

fn stable_card_id(session_id: &str, document_id: &str, source_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(session_id.as_bytes());
    hash.update(document_id.as_bytes());
    hash.update(source_id.as_bytes());
    format!("card_{:x}", hash.finalize())
}

fn ensure_array(report: &mut Value, key: &str) {
    if !report[key].is_array() {
        let old = report[key].take();
        report[key] = if old.is_null() {
            json!([])
        } else {
            json!([old])
        };
    }
}

fn push_audit(report: &mut Value, key: &str, entry: Value) {
    ensure_array(report, key);
    let array = report[key].as_array_mut().expect("ensured array");
    if !array.contains(&entry) {
        array.push(entry);
    }
}

fn collection_len(value: &Value) -> usize {
    match value {
        Value::Array(values) => values.len(),
        Value::Object(values) => values.len(),
        Value::Number(number) => number.as_u64().unwrap_or(0) as usize,
        Value::Bool(value) => usize::from(*value),
        _ => 0,
    }
}

fn coverage_failures(report: &Value) -> usize {
    // The same batch id can appear in failed_batches and incomplete_batches;
    // count distinct evidence, not array lengths.
    let mut explicit = std::collections::BTreeSet::new();
    for key in [
        "failed_pages",
        "failed_chunks",
        "failed_batches",
        "incomplete_batches",
    ] {
        if let Some(items) = report[key].as_array() {
            for item in items {
                explicit.insert(item.to_string());
            }
        }
    }
    let explicit = explicit.len();
    let coverage = ["page_coverage", "chunk_coverage"]
        .iter()
        .map(|key| {
            report[*key]
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|entry| entry["state"].as_str() == Some("failed"))
                        .count()
                })
                .unwrap_or(0)
        })
        .sum::<usize>();
    explicit
        .max(coverage)
        .max(usize::from(report["partial"].as_bool().unwrap_or(false)))
}

fn sha256_hex(text: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(text.as_bytes());
    format!("{:x}", hash.finalize())
}

/// Import reports are durable and can be backed up or synced. They may describe
/// where a fact came from, but must not duplicate user material (raw source text,
/// answers, model payloads) that already lives in the protected source store.
fn redact_sensitive(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let mut replacements = Vec::new();
            for key in [
                "raw_text",
                "value",
                "existing_value",
                "incoming_value",
                "existing",
                "incoming",
                "evidence",
                "json",
            ] {
                if let Some(original) = map.get(key) {
                    let text = original
                        .as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| original.to_string());
                    replacements.push((key.to_string(), text));
                }
            }
            for (key, text) in replacements {
                map.remove(&key);
                map.insert(format!("{key}_sha256"), json!(sha256_hex(&text)));
                map.insert(format!("{key}_chars"), json!(text.chars().count()));
            }
            for value in map.values_mut() {
                redact_sensitive(value);
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sensitive(item);
            }
        }
        _ => {}
    }
}

fn is_non_blocking_reason(reason: &str) -> bool {
    // Page furniture recognised by an auditable rule, and entries the user
    // explicitly excluded, are warnings. Everything else blocks completion.
    matches!(
        reason,
        "classified_non_question"
            | "previously_soft_deleted"
            | "identical_source_replay"
            | "user_edit_preserved"
    )
}

/// Completion is derived from classified, severity-tagged findings rather than
/// "any non-empty report array".
fn annotate_report_severity(report: &mut Value, status: &str) {
    for key in [
        "quarantined_entries",
        "conflicts",
        "identity_risks",
        "excluded_entries",
        "preserved_edits",
    ] {
        if let Some(entries) = report[key].as_array_mut() {
            for entry in entries {
                let reason = entry["reason"].as_str().unwrap_or_default();
                entry["severity"] = json!(if is_non_blocking_reason(reason) {
                    "warning"
                } else {
                    "blocking"
                });
                entry["category"] = json!(key);
            }
        }
    }
    for key in ["figure_failures", "deferred_continuations"] {
        if let Some(entries) = report[key].as_array_mut() {
            for entry in entries {
                if entry.is_object() {
                    entry["severity"] = json!("blocking");
                    entry["category"] = json!(key);
                }
            }
        }
    }
    // Count severities per entry; array lengths alone would mix warnings with
    // blockers (e.g. a recognised heading next to an ambiguous source).
    let mut blocking = collection_len(&report["failed_pages"]);
    let mut warnings = 0usize;
    for key in [
        "quarantined_entries",
        "conflicts",
        "identity_risks",
        "excluded_entries",
        "preserved_edits",
        "figure_failures",
        "deferred_continuations",
    ] {
        if let Some(entries) = report[key].as_array() {
            for entry in entries {
                match entry["severity"].as_str() {
                    Some("warning") => warnings += 1,
                    Some("blocking") => blocking += 1,
                    _ => {}
                }
            }
        }
    }
    let categories = json!({
        "failed_pages": collection_len(&report["failed_pages"]),
        "failed_batches": collection_len(&report["failed_batches"]),
        "incomplete_batches": collection_len(&report["incomplete_batches"]),
        "figure_failures": collection_len(&report["figure_failures"]),
        "deferred_continuations": collection_len(&report["deferred_continuations"]),
        "quarantined": collection_len(&report["quarantined_entries"]),
        "conflicts": collection_len(&report["conflicts"]),
        "identity_risks": collection_len(&report["identity_risks"]),
        "excluded_by_rule_or_user": warnings,
    });
    report["completion"] = json!({
        "status": status,
        "blocking_count": blocking,
        "warning_count": warnings,
        "categories": categories,
    });
}

/// Source metadata is model-provided. Reports must use an allowlist rather than
/// trying to blacklist every possible free-text key it may invent.
fn safe_source_metadata(sq: &StructuredQuestion) -> Value {
    const ALLOWED: [&str; 7] = [
        "text_span",
        "answer_evidence_verified",
        "explanation_evidence_verified",
        "requires_review",
        "source_kind",
        "image_indices",
        "page_span",
    ];
    let Some(metadata) = sq.json.get("source_metadata").and_then(Value::as_object) else {
        return json!({});
    };
    let mut safe = serde_json::Map::new();
    for key in ALLOWED {
        if let Some(value) = metadata.get(key) {
            safe.insert(key.to_string(), value.clone());
        }
    }
    Value::Object(safe)
}

fn source_audit(sq: &StructuredQuestion, document_id: &str, pages: &[PageSlice]) -> Value {
    let source = &sq.source.merged;
    json!({"document_id":document_id,"source_id":source.question.source_id,
        "block_kind":source.question.block_kind,"section":source.question.section,
        "label":source.question.label,
        "raw_text_sha256":sha256_hex(&source.question.raw_text),
        "raw_text_chars":source.question.raw_text.chars().count(),
        "json_sha256":sha256_hex(&sq.json.to_string()),
        "json_keys":sq.json.as_object().map(|json| {
            let mut keys: Vec<&String> = json.keys().collect();
            keys.sort();
            json!(keys)
        }).unwrap_or(Value::Null),
        "page_indices":if pages.is_empty(){json!([])}else{json!(source.page_indices)},
        "page_spans":if pages.is_empty(){json!([])}else{json!(source.source_spans)},
        "source_metadata":safe_source_metadata(sq)})
}

fn quarantine(
    sq: &StructuredQuestion,
    document_id: &str,
    pages: &[PageSlice],
    reason: &str,
) -> Value {
    let mut entry = source_audit(sq, document_id, pages);
    entry["reason"] = json!(reason);
    entry
}

fn make_source_ref(
    sq: &StructuredQuestion,
    document_id: &str,
    pages: &[PageSlice],
    mapping: &HashMap<usize, usize>,
    display_order: usize,
) -> Value {
    let source = &sq.source.merged;
    let local_pages = if pages.is_empty() {
        vec![]
    } else {
        source.page_indices.clone()
    };
    let preview_pages: Vec<usize> = local_pages
        .iter()
        .filter_map(|p| mapping.get(p).copied())
        .collect();
    json!({"schema_version":1,"kind":"document_import","document_id":document_id,
        "block_id":source.question.source_id,"section":source.question.section,"label":source.question.label,
        "page_indices":local_pages,"page_spans":if pages.is_empty(){json!([])}else{json!(source.source_spans)},
        "preview_page_indices":preview_pages,
        "page_offset":local_pages.first().and_then(|p|mapping.get(p).map(|mapped|mapped.saturating_sub(*p))),
        "display_order":display_order,"source_metadata":sq.json.get("source_metadata").cloned().unwrap_or(Value::Null),
        // Exact inline quote supporting each extracted field. Kept even when the
        // field is rejected by the explicit verification gate, so the rejected
        // payload stays auditable instead of disappearing with the value.
        "answer_evidence":inline_evidence(sq,"answer"),
        "explanation_evidence":inline_evidence(sq,"explanation")})
}

fn validated_params(
    sq: &StructuredQuestion,
    session: &str,
    card: &str,
    index: usize,
) -> Result<CreateQuestionParams, &'static str> {
    if !sq.json.is_object() {
        return Err("invalid_question_schema");
    }
    let content = sq.json["content"]
        .as_str()
        .ok_or("missing_string_content")?
        .trim();
    if content.is_empty()
        || matches!(
            content.to_ascii_lowercase().as_str(),
            "null" | "undefined" | "none"
        )
        || content
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
    {
        return Err("empty_or_invalid_content");
    }
    if content.starts_with("[答案区]")
        || sq
            .source
            .merged
            .question
            .raw_text
            .trim_start()
            .starts_with("[答案区]")
    {
        return Err("legacy_answer_marker");
    }
    if let Some(kind) = sq.json.get("block_kind") {
        if kind.as_str().is_some_and(|kind| kind != "question") {
            return Err("contradictory_block_kind");
        }
    }
    // An unknown model-provided enum is not the same as a legitimate "other".
    // Mapping it silently to Other would let malformed choice payloads bypass the
    // option gate, so it enters review with an explicit reason.
    if let Some(kind) = sq.json.get("question_type").and_then(Value::as_str) {
        let normalized = kind.trim().to_ascii_lowercase();
        if !normalized.is_empty() && !KNOWN_QUESTION_TYPE_ENUMS.contains(&normalized.as_str()) {
            return Err("unknown_question_type_enum");
        }
    }
    let mut params = super::json_to_question_params(&sq.json, session, card, index);
    // Do not inherit the legacy adapter's text rewriting or synthetic Q{n} label.
    params.content = content.to_string();
    params.answer = params.answer.filter(|answer| !answer.trim().is_empty());
    let options_value = sq.json.get("options").cloned().unwrap_or(Value::Null);
    if is_choice(params.question_type.as_ref()) {
        let options = options_value.as_array().ok_or("choice_missing_options")?;
        if options.len() < 2 {
            return Err("choice_requires_two_unique_options");
        }
        let mut keys = HashSet::new();
        let mut values = HashSet::new();
        let mut validated = Vec::with_capacity(options.len());
        for option in options {
            let key = option["key"].as_str().ok_or("invalid_option_key")?.trim();
            let value = option["content"]
                .as_str()
                .ok_or("invalid_option_content")?
                .trim();
            if key.is_empty()
                || key.chars().count() > 16
                || !key.chars().all(char::is_alphanumeric)
                || value.is_empty()
                || value
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            {
                return Err("invalid_choice_option");
            }
            let canonical = key.to_uppercase();
            let normalized_value = value
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            if !keys.insert(canonical.clone()) || !values.insert(normalized_value) {
                return Err("duplicate_choice_option");
            }
            validated.push(crate::vfs::repos::QuestionOption {
                key: canonical,
                content: value.to_string(),
            });
        }
        params.options = Some(validated);
    } else if !options_value.is_null() {
        // Unknown/non-choice types may still carry options. Empty arrays are
        // dropped; any non-empty payload must satisfy the same structure rules
        // instead of bypassing validation through the type field.
        match &options_value {
            Value::Array(options) if options.is_empty() => params.options = None,
            Value::Array(options) => {
                params.options = Some(validated_non_choice_options(options)?);
            }
            _ => return Err("invalid_option_structure"),
        }
    }
    Ok(params)
}

/// Structural rules for options that are present on a non-choice payload.
/// Model-written keys/content must never smuggle malformed data into storage.
fn validated_non_choice_options(
    options: &[Value],
) -> Result<Vec<crate::vfs::repos::QuestionOption>, &'static str> {
    let mut keys = HashSet::new();
    let mut values = HashSet::new();
    let mut validated = Vec::with_capacity(options.len());
    for option in options {
        let key = option["key"]
            .as_str()
            .ok_or("invalid_option_structure")?
            .trim();
        let value = option["content"]
            .as_str()
            .ok_or("invalid_option_structure")?
            .trim();
        if key.is_empty()
            || key.chars().count() > 16
            || !key.chars().all(char::is_alphanumeric)
            || value.is_empty()
            || value
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
        {
            return Err("invalid_option_structure");
        }
        let canonical = key.to_uppercase();
        let normalized_value = value
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !keys.insert(canonical.clone()) || !values.insert(normalized_value) {
            return Err("invalid_option_structure");
        }
        validated.push(crate::vfs::repos::QuestionOption {
            key: canonical,
            content: value.to_string(),
        });
    }
    Ok(validated)
}

const KNOWN_QUESTION_TYPE_ENUMS: [&str; 39] = [
    "single_choice",
    "单选",
    "单选题",
    "multiple_choice",
    "多选",
    "多选题",
    "indefinite_choice",
    "不定项",
    "fill_blank",
    "填空",
    "填空题",
    "short_answer",
    "简答",
    "简答题",
    "essay",
    "论述",
    "论述题",
    "calculation",
    "计算",
    "计算题",
    "proof",
    "证明",
    "证明题",
    "true_false",
    "判断",
    "判断题",
    "judgment",
    "matching",
    "匹配",
    "匹配题",
    "连线",
    "连线题",
    "ordering",
    "排序",
    "排序题",
    "numeric",
    "数值",
    "数值题",
    "other",
];

fn is_choice(kind: Option<&crate::vfs::repos::QuestionType>) -> bool {
    matches!(
        kind,
        Some(
            crate::vfs::repos::QuestionType::SingleChoice
                | crate::vfs::repos::QuestionType::MultipleChoice
                | crate::vfs::repos::QuestionType::IndefiniteChoice
        )
    )
}

const EVIDENCE_FIELDS: [&str; 2] = ["answer", "explanation"];

fn evidence_field(field: &str) -> Option<&'static str> {
    match field {
        "answer" => Some("answer"),
        "explanation" => Some("explanation"),
        _ => None,
    }
}

fn inline_evidence(sq: &StructuredQuestion, field: &str) -> Value {
    sq.json
        .get(&format!("{field}_evidence"))
        .cloned()
        .unwrap_or(Value::Null)
}

/// Only an explicit `false` is a hard gate. Absence remains compatible with the
/// visual path, which verifies its own quote before producing the candidate.
/// An unverified answer source also makes its explanation untrusted unless the
/// explanation field was verified explicitly on its own key.
fn evidence_unverified(value: &Value, field: &str) -> bool {
    let pointer = format!("/source_metadata/{field}_evidence_verified");
    match value.pointer(&pointer).and_then(Value::as_bool) {
        Some(verified) => !verified,
        None if field == "explanation" => {
            value
                .pointer("/source_metadata/answer_evidence_verified")
                .and_then(Value::as_bool)
                == Some(false)
        }
        None => false,
    }
}

#[derive(Default, Clone)]
struct FieldProvenance {
    source_id: String,
    evidence: Value,
    evidence_kind: &'static str,
}

impl FieldProvenance {
    fn from_source(sq: &StructuredQuestion, field: &str) -> Self {
        let inline = inline_evidence(sq, field);
        let quoted = inline
            .as_str()
            .map(str::trim)
            .is_some_and(|quote| !quote.is_empty());
        Self {
            source_id: sq.source.merged.question.source_id.clone(),
            evidence: if quoted {
                inline
            } else {
                json!(sq.source.merged.question.raw_text)
            },
            evidence_kind: if quoted { "inline" } else { "raw_text" },
        }
    }
}

#[derive(Default)]
struct DonorAudit {
    unverified: HashSet<&'static str>,
    provenance: HashMap<&'static str, FieldProvenance>,
}

/// Per target field: whether the matched donor explicitly failed verification,
/// and which donor supplied a verified value. A matched donor cannot launder a
/// field that its own source marked unverified.
fn donor_audit(
    outcome: &crate::answer_merge::MergeOutcome,
    candidates: &[StructuredQuestion],
) -> HashMap<String, DonorAudit> {
    let mut audits: HashMap<String, DonorAudit> = HashMap::new();
    for relation in &outcome.matched {
        let (Some(target), Some(donor_id)) = (
            relation["target_source_id"].as_str(),
            relation["source_id"].as_str(),
        ) else {
            continue;
        };
        let Some(fields) = relation["matched_fields"].as_array() else {
            continue;
        };
        let donors: Vec<&StructuredQuestion> = candidates
            .iter()
            .filter(|sq| sq.source.merged.question.source_id == donor_id)
            .collect();
        for field in fields
            .iter()
            .filter_map(Value::as_str)
            .filter_map(evidence_field)
        {
            let audit = audits.entry(target.to_string()).or_default();
            if donors
                .iter()
                .any(|donor| evidence_unverified(&donor.json, field))
            {
                audit.unverified.insert(field);
                audit.provenance.remove(field);
            } else if !audit.unverified.contains(field) {
                if let Some(donor) = donors.first() {
                    audit
                        .provenance
                        .entry(field)
                        .or_insert_with(|| FieldProvenance::from_source(donor, field));
                }
            }
        }
    }
    audits
}

fn invalid_answer(
    params: &mut CreateQuestionParams,
    sq: &StructuredQuestion,
    unverified_donors: &HashSet<&'static str>,
) -> Vec<(&'static str, &'static str)> {
    let mut invalid = Vec::new();
    for field in EVIDENCE_FIELDS {
        let present = match field {
            "answer" => params
                .answer
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
            _ => params
                .explanation
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()),
        };
        if !present {
            continue;
        }
        if evidence_unverified(&sq.json, field) || unverified_donors.contains(field) {
            invalid.push((
                field,
                if field == "answer" {
                    "answer_evidence_not_verified"
                } else {
                    "explanation_evidence_not_verified"
                },
            ));
            match field {
                "answer" => params.answer = None,
                _ => params.explanation = None,
            }
        }
    }
    if sq
        .json
        .get("answer")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        invalid.push(("answer", "invalid_answer_schema"));
        params.answer = None;
    }
    if sq
        .json
        .get("explanation")
        .is_some_and(|value| !value.is_null() && !value.is_string())
    {
        invalid.push(("explanation", "invalid_explanation_schema"));
        params.explanation = None;
    }
    if let Some(reason) = invalid_choice_answer(params) {
        invalid.push(("answer", reason));
        params.answer = None;
    }
    invalid
}

fn invalid_choice_answer(params: &mut CreateQuestionParams) -> Option<&'static str> {
    if !is_choice(params.question_type.as_ref()) {
        return None;
    }
    let answer = params.answer.as_deref()?.trim().to_uppercase();
    let options = params.options.as_ref()?;
    if options.iter().any(|option| option.key == answer) {
        params.answer = Some(answer);
        return None;
    }
    let stripped = answer
        .trim_start_matches("答案")
        .trim()
        .trim_start_matches([':', '：'])
        .trim();
    let tokens: Vec<String> = stripped
        .split(|c: char| {
            c.is_whitespace()
                || matches!(
                    c,
                    ',' | '，'
                        | '、'
                        | ';'
                        | '；'
                        | '/'
                        | '|'
                        | '('
                        | ')'
                        | '（'
                        | '）'
                        | '['
                        | ']'
                        | '.'
                        | '。'
                )
        })
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect();
    let tokens = if tokens.len() == 1
        && options.iter().all(|option| option.key.len() == 1)
        && tokens[0].chars().all(|c| c.is_ascii_uppercase())
    {
        tokens[0].chars().map(|c| c.to_string()).collect::<Vec<_>>()
    } else {
        tokens
    };
    if tokens.is_empty()
        || tokens
            .iter()
            .any(|token| !options.iter().any(|option| option.key == *token))
    {
        return Some("answer_references_missing_or_unverifiable_option");
    }
    if params.question_type == Some(crate::vfs::repos::QuestionType::SingleChoice)
        && tokens.len() != 1
    {
        return Some("single_choice_has_multiple_answers");
    }
    let mut keys = HashSet::new();
    if tokens.iter().any(|key| !keys.insert(key)) {
        return Some("duplicate_answer_option");
    }
    // Downstream grading expects keys, not an OCR phrase such as "答案：b".
    // The exact extracted value remains in the source audit.
    params.answer = Some(tokens.join(""));
    None
}

#[derive(Default)]
struct SafeFillStats {
    answer: usize,
    explanation: usize,
}

fn persisted_field<'a>(question: &'a PersistedQuestion, field: &str) -> &'a str {
    match field {
        "answer" => question.answer.as_deref().unwrap_or(""),
        _ => question.explanation.as_deref().unwrap_or(""),
    }
}

fn incoming_field<'a>(params: &'a CreateQuestionParams, field: &str) -> &'a str {
    match field {
        "answer" => params.answer.as_deref().unwrap_or(""),
        _ => params.explanation.as_deref().unwrap_or(""),
    }
}

#[allow(clippy::too_many_arguments)]
fn option_pairs(options: Option<&Vec<crate::vfs::repos::QuestionOption>>) -> Vec<(String, String)> {
    options
        .map(|options| {
            options
                .iter()
                .map(|option| (option.key.clone(), option.content.clone()))
                .collect()
        })
        .unwrap_or_default()
}

/// Persisted import baseline (`source_ref.import_baseline`) for one question.
/// Only values written by the import pipeline are recorded; user edits never
/// become the baseline, so a later re-import can tell them apart.
fn persisted_baseline(existing: &PersistedQuestion) -> Option<Value> {
    existing
        .source_ref
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|source| source.get("import_baseline").cloned())
}

fn baseline_field<'a>(baseline: &'a Option<Value>, field: &str) -> Option<&'a str> {
    baseline
        .as_ref()?
        .get("fields")?
        .get(field)?
        .as_str()
        .filter(|value| !value.is_empty())
}

/// Store the field-level baseline after a successful publish. Existing evidence
/// fields on `source_ref` are preserved; only the baseline node is replaced.
fn persist_import_baseline(
    conn: &Connection,
    existing: &PersistedQuestion,
    fields: serde_json::Map<String, Value>,
) -> Result<(), AppError> {
    let mut source = existing
        .source_ref
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .unwrap_or_else(|| json!({}));
    if !source.is_object() {
        source = json!({"legacy_source_ref": source});
    }
    source["import_baseline"] = json!({
        "version": super::normalize::REPRESENTATION_NORMALIZATION_VERSION,
        "fields": fields,
    });
    conn.execute(
        "UPDATE questions SET source_ref=?1 WHERE id=?2",
        rusqlite::params![source.to_string(), existing.id],
    )
    .map_err(|e| AppError::database(format!("保存导入字段基线失败: {e}")))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn preserve_existing(
    conn: &Connection,
    existing: &PersistedQuestion,
    incoming: &CreateQuestionParams,
    sq: &StructuredQuestion,
    document_id: &str,
    pages: &[PageSlice],
    unverified_donors: &HashSet<&'static str>,
    provenance: &HashMap<&'static str, FieldProvenance>,
    original: Option<&StructuredQuestion>,
    report: &mut Value,
) -> Result<SafeFillStats, AppError> {
    // Representation-safe structural comparison: whitespace, Unicode width and
    // LaTeX sub/superscript spelling are not user edits; real fact changes are.
    let same_structure = super::normalize::canonical_stem_key(&existing.content)
        == super::normalize::canonical_stem_key(&incoming.content)
        && super::normalize::options_equivalent(
            &option_pairs(existing.options.as_ref()),
            &option_pairs(incoming.options.as_ref()),
        )
        && Some(&existing.question_type) == incoming.question_type.as_ref()
        && existing.question_label == incoming.question_label
        && existing.structured_data == incoming.structured_data
        && existing.images == incoming.images.clone().unwrap_or_default();
    let baseline = persisted_baseline(existing);
    let baseline_version = baseline
        .as_ref()
        .and_then(|baseline| baseline.get("version"))
        .and_then(Value::as_u64)
        .map(|version| version as u32);
    let baseline = if super::normalize::baseline_version_usable(baseline_version) {
        baseline
    } else {
        None
    };
    // A replay with an existing usable baseline must be a pure no-op unless it
    // actually writes a field. Otherwise merely re-importing would rewrite the
    // stored baseline (empty fields, shapes) and make previews non-idempotent.
    let needs_baseline_write = baseline.is_none();
    let mut conflicts: Vec<(&'static str, &'static str, String, String)> = Vec::new();
    if !same_structure {
        conflicts.push((
            "existing_question_edited",
            "question",
            existing.content.clone(),
            incoming.content.clone(),
        ));
    }
    if !conflicts.is_empty() {
        for (reason, field, existing_value, incoming_value) in conflicts {
            let mut entry = quarantine(sq, document_id, pages, reason);
            entry["question_id"] = json!(existing.id);
            entry["card_id"] = json!(existing.card_id);
            entry["field"] = json!(field);
            entry["existing_value"] = json!(existing_value);
            entry["incoming_value"] = json!(incoming_value);
            push_audit(report, "conflicts", entry);
        }
        return Ok(SafeFillStats::default());
    }
    let mut fills: Vec<(&'static str, String, Option<FieldProvenance>)> = Vec::new();
    let mut baseline_fields = serde_json::Map::new();
    for field in EVIDENCE_FIELDS {
        let existing_value = persisted_field(existing, field).trim();
        let incoming_value = incoming_field(incoming, field).trim();
        if existing_value.is_empty() && incoming_value.is_empty() {
            continue;
        }
        // Defense in depth: invalid_answer already clears these values, but a
        // safe backfill must never reintroduce an unverified grading key or
        // explanation even if call ordering changes.
        if evidence_unverified(&sq.json, field) || unverified_donors.contains(field) {
            let mut entry = quarantine(
                sq,
                document_id,
                pages,
                if field == "answer" {
                    "answer_evidence_not_verified"
                } else {
                    "explanation_evidence_not_verified"
                },
            );
            entry["question_id"] = json!(existing.id);
            entry["card_id"] = json!(existing.card_id);
            entry["field"] = json!(field);
            entry["incoming_value"] = json!(incoming_value);
            push_audit(report, "conflicts", entry);
            continue;
        }
        let decision = super::normalize::decide_field_update(&super::normalize::FieldBaseline {
            field,
            baseline: baseline_field(&baseline, field),
            current: existing_value,
            proposal: incoming_value,
        });
        if decision.writes_proposal() {
            let own_value_before_merge = original.is_some_and(|original| {
                original
                    .json
                    .get(field)
                    .and_then(Value::as_str)
                    .is_some_and(|value| !value.trim().is_empty())
            });
            let donor = if own_value_before_merge {
                None
            } else {
                provenance.get(field).cloned()
            };
            fills.push((field, incoming_value.to_string(), donor));
            baseline_fields.insert(field.to_string(), json!(incoming_value));
        } else if decision.needs_review() {
            let mut entry = quarantine(sq, document_id, pages, decision.reason());
            entry["question_id"] = json!(existing.id);
            entry["card_id"] = json!(existing.card_id);
            entry["field"] = json!(field);
            push_audit(report, "conflicts", entry);
        } else {
            // The user's value (or an equal representation) is kept. The
            // import result, not the user's edit, stays the field baseline.
            let kept = match decision {
                super::normalize::FieldUpdateDecision::PreserveUserEdit => {
                    incoming_value.to_string()
                }
                super::normalize::FieldUpdateDecision::PreserveOnEmptyProposal => {
                    baseline_field(&baseline, field)
                        .unwrap_or(existing_value)
                        .to_string()
                }
                _ => existing_value.to_string(),
            };
            if !kept.is_empty() {
                baseline_fields.insert(field.to_string(), json!(kept));
            }
            if decision == super::normalize::FieldUpdateDecision::PreserveUserEdit {
                let mut entry = quarantine(sq, document_id, pages, decision.reason());
                entry["question_id"] = json!(existing.id);
                entry["card_id"] = json!(existing.card_id);
                entry["field"] = json!(field);
                push_audit(report, "preserved_edits", entry);
            }
        }
    }
    if !fills.is_empty() {
        let answer = fills
            .iter()
            .find(|(field, _, _)| *field == "answer")
            .map(|(_, value, _)| value.clone());
        let explanation = fills
            .iter()
            .find(|(field, _, _)| *field == "explanation")
            .map(|(_, value, _)| value.clone());
        VfsQuestionRepo::update_question_with_conn(
            conn,
            &existing.id,
            &UpdateQuestionParams {
                answer,
                explanation,
                expected_updated_at: Some(existing.updated_at.clone()),
                ..Default::default()
            },
        )
        .map_err(|e| AppError::database(format!("安全回填已有题目答案失败: {e}")))?;
    }
    // Content/label baselines let a later import distinguish a representation
    // change from a user edit without comparing raw strings.
    baseline_fields.insert("content".to_string(), json!(existing.content));
    baseline_fields.insert(
        "question_label".to_string(),
        json!(existing.question_label.clone().unwrap_or_default()),
    );
    if needs_baseline_write || !fills.is_empty() {
        persist_import_baseline(conn, existing, baseline_fields)?;
    }
    let mut stats = SafeFillStats::default();
    for (field, value, donor) in fills {
        let source = donor.unwrap_or_else(|| FieldProvenance::from_source(sq, field));
        push_audit(
            report,
            "safe_fill_entries",
            json!({
                "question_id": existing.id,
                "card_id": existing.card_id,
                "document_id": document_id,
                "source_id": sq.source.merged.question.source_id,
                "field": field,
                "value": value,
                "evidence_source_id": source.source_id,
                "evidence_kind": source.evidence_kind,
                "evidence": source.evidence,
            }),
        );
        match field {
            "answer" => stats.answer += 1,
            _ => stats.explanation += 1,
        }
    }
    Ok(stats)
}

fn read_all_questions(
    conn: &Connection,
    session_id: &str,
) -> Result<Vec<PersistedQuestion>, AppError> {
    let mut all = Vec::new();
    let mut page = 1;
    loop {
        let batch = VfsQuestionRepo::list_questions_with_conn(
            conn,
            session_id,
            &QuestionFilters::default(),
            page,
            100,
        )
        .map_err(|e| AppError::database(format!("分页读取正式题目失败: {e}")))?;
        if batch.questions.is_empty() && batch.has_more {
            return Err(AppError::database("题目分页未推进".to_string()));
        }
        all.extend(batch.questions);
        if !batch.has_more {
            if all.len() != batch.total as usize {
                return Err(AppError::database("题目分页结果不完整".to_string()));
            }
            break;
        }
        page += 1;
    }
    Ok(all)
}

fn question_card_id(question: &PersistedQuestion) -> String {
    question
        .card_id
        .as_ref()
        .filter(|id| !id.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| question.id.clone())
}

/// JSON projection intentionally retains unknown page/card metadata, old image
/// paths and per-document page identifiers instead of round-tripping a lossy DTO.
struct PreviewProjection {
    root: Value,
    old_cards: HashMap<String, Value>,
}

impl PreviewProjection {
    fn new(raw: &str, session: &str, name: &str) -> Result<Self, AppError> {
        let mut root: Value = serde_json::from_str(raw)
            .map_err(|e| AppError::validation(format!("已有预览无效，不能覆盖: {e}")))?;
        if !root.is_object() {
            return Err(AppError::validation("已有预览不是对象".to_string()));
        }
        if !root["pages"].is_array() {
            root["pages"] = json!([]);
        }
        root["session_id"] = json!(session);
        if root["temp_id"].as_str().unwrap_or("").is_empty() {
            root["temp_id"] = json!(session);
        }
        root["exam_name"] = json!(name);
        let mut old_cards = HashMap::new();
        let mut seen_pages = HashSet::new();
        for (index, page) in root["pages"]
            .as_array_mut()
            .expect("pages array")
            .iter_mut()
            .enumerate()
        {
            if !page.is_object() {
                return Err(AppError::validation("已有预览页不是对象".to_string()));
            }
            let page_index = page["page_index"]
                .as_u64()
                .map(|p| p as usize)
                .unwrap_or(index);
            if !seen_pages.insert(page_index) {
                return Err(AppError::validation("已有预览页索引重复".to_string()));
            }
            page["page_index"] = json!(page_index);
            if let Some(cards) = page["cards"].as_array() {
                for card in cards {
                    if let Some(id) = card["card_id"].as_str().filter(|id| !id.is_empty()) {
                        let mut card = card.clone();
                        card["page_index"] = json!(page_index);
                        old_cards.entry(id.to_string()).or_insert(card);
                    }
                }
            }
            page["cards"] = json!([]);
        }
        Ok(Self { root, old_cards })
    }

    fn add_document_pages(
        &mut self,
        pages: &[PageSlice],
        document: &str,
        old_document: &str,
        report: &Value,
    ) -> Result<HashMap<usize, usize>, AppError> {
        let stored = self.root["pages"].as_array_mut().expect("pages array");
        let mut next = stored
            .iter()
            .filter_map(|page| page["page_index"].as_u64())
            .max()
            .map(|p| p as usize + 1)
            .unwrap_or(0);
        let mut mapping = HashMap::new();
        if pages.is_empty() {
            if !stored.iter().any(|page| {
                page["is_virtual"].as_bool() == Some(true)
                    && page["document_id"].as_str() == Some(document)
            }) {
                stored.push(json!({"page_index":next,"document_id":document,"is_virtual":true,
                    "cards":[],"original_image_path":"","ocr_completed":true,"parse_completed":true}));
            }
            return Ok(mapping);
        }
        let mut input_seen = HashSet::new();
        for page in pages {
            if !input_seen.insert(page.page_index) {
                return Err(AppError::validation("本次来源页索引重复".to_string()));
            }
            let existing = stored.iter().position(|stored| {
                stored["document_id"].as_str() == Some(document)
                    && stored["source_page_index"].as_u64() == Some(page.page_index as u64)
            });
            let legacy: Vec<usize> = if old_document.is_empty() || old_document == document {
                stored
                    .iter()
                    .enumerate()
                    .filter(|(_, stored)| {
                        stored["document_id"].as_str().unwrap_or("").is_empty()
                            && stored["blob_hash"].as_str() == Some(page.blob_hash.as_str())
                            && !page.blob_hash.is_empty()
                    })
                    .map(|(index, _)| index)
                    .collect()
            } else {
                vec![]
            };
            let position = match existing.or_else(|| {
                if legacy.len() == 1 {
                    Some(legacy[0])
                } else {
                    None
                }
            }) {
                Some(position) => position,
                None => {
                    stored.push(json!({"page_index":next,"cards":[],"original_image_path":""}));
                    next += 1;
                    stored.len() - 1
                }
            };
            let stored_page = &mut stored[position];
            if stored_page["document_id"].as_str() == Some(document)
                && stored_page["blob_hash"]
                    .as_str()
                    .is_some_and(|hash| !hash.is_empty() && hash != page.blob_hash)
            {
                return Err(AppError::validation(
                    "同一文档版本来源页图片发生变化".to_string(),
                ));
            }
            stored_page["document_id"] = json!(document);
            stored_page["source_page_index"] = json!(page.page_index);
            stored_page["blob_hash"] = json!(page.blob_hash);
            stored_page["width"] = json!(page.width);
            stored_page["height"] = json!(page.height);
            let coverage = report["page_coverage"]
                .as_array()
                .and_then(|entries| {
                    entries
                        .iter()
                        .find(|entry| entry["page_index"].as_u64() == Some(page.page_index as u64))
                })
                .and_then(|entry| entry["state"].as_str());
            stored_page["ocr_completed"] = json!(coverage != Some("failed"));
            stored_page["parse_completed"] = json!(!matches!(
                coverage,
                Some("failed" | "needs_review" | "pending" | "processing")
            ));
            mapping.insert(
                page.page_index,
                stored_page["page_index"]
                    .as_u64()
                    .expect("assigned page index") as usize,
            );
        }
        Ok(mapping)
    }

    fn virtual_page_for(&self, document: &str) -> Option<usize> {
        self.root["pages"].as_array()?.iter().find(|page| {
            page["is_virtual"].as_bool() == Some(true)
                && page["document_id"].as_str() == Some(document)
        })?["page_index"]
            .as_u64()
            .map(|p| p as usize)
    }

    fn ensure_virtual_page(&mut self, document: &str) -> usize {
        if let Some(index) = self.virtual_page_for(document) {
            return index;
        }
        let pages = self.root["pages"].as_array_mut().expect("pages array");
        let index = pages
            .iter()
            .filter_map(|page| page["page_index"].as_u64())
            .max()
            .map(|p| p as usize + 1)
            .unwrap_or(0);
        pages.push(
            json!({"page_index":index,"document_id":document,"is_virtual":true,"cards":[],
            "original_image_path":"","ocr_completed":true,"parse_completed":true}),
        );
        index
    }

    fn project(&mut self, questions: &[PersistedQuestion]) -> Result<(), AppError> {
        let mut seen = HashSet::new();
        for question in questions {
            let id = question_card_id(question);
            if !seen.insert(id.clone()) {
                return Err(AppError::database(
                    "正式题目 card_id 重复，不能构建唯一预览".to_string(),
                ));
            }
            let old = self.old_cards.get(&id).cloned();
            let mut card = old
                .clone()
                .filter(Value::is_object)
                .unwrap_or_else(|| json!({}));
            let source = question
                .source_ref
                .as_deref()
                .and_then(|raw| serde_json::from_str::<Value>(raw).ok());
            let document = source
                .as_ref()
                .and_then(|source| source["document_id"].as_str())
                .unwrap_or("");
            let physical = source
                .as_ref()
                .and_then(|source| source["page_indices"].as_array())
                .is_some_and(|pages| !pages.is_empty());
            let mut target = None;
            if physical {
                if let Some(source) = source.as_ref() {
                    let local = source["page_indices"][0].as_u64();
                    target = self.root["pages"]
                        .as_array()
                        .expect("pages array")
                        .iter()
                        .find(|page| {
                            page["document_id"].as_str() == Some(document)
                                && page["source_page_index"].as_u64() == local
                        })
                        .and_then(|page| page["page_index"].as_u64())
                        .map(|p| p as usize);
                }
            } else if let Some(index) = source
                .as_ref()
                .and_then(|source| source["virtual_page_index"].as_u64())
            {
                if self.root["pages"]
                    .as_array()
                    .expect("pages array")
                    .iter()
                    .any(|page| {
                        page["page_index"].as_u64() == Some(index)
                            && page["is_virtual"].as_bool() == Some(true)
                            && page["document_id"].as_str() == Some(document)
                    })
                {
                    target = Some(index as usize);
                }
            }
            // A legacy card can retain its exact old id/page/bbox, never a guessed
            // correspondence by stem, ordinal or Q label.
            if source.is_none() {
                target = old
                    .as_ref()
                    .and_then(|card| card["page_index"].as_u64())
                    .map(|p| p as usize);
            }
            let page_index = target.unwrap_or_else(|| self.ensure_virtual_page(document));
            let page = self.root["pages"]
                .as_array_mut()
                .expect("pages array")
                .iter_mut()
                .find(|page| page["page_index"].as_u64() == Some(page_index as u64))
                .expect("target page exists");
            card["card_id"] = json!(id);
            card["question_id"] = json!(question.id);
            card["page_index"] = json!(page_index);
            card["question_label"] = json!(question.question_label.clone().unwrap_or_default());
            card["ocr_text"] = json!(question.content);
            card["content"] = json!(question.content);
            card["options"] = json!(question.options);
            card["answer"] = json!(question.answer);
            card["explanation"] = json!(question.explanation);
            card["structured_data"] = json!(question.structured_data);
            card["question_type"] = json!(question.question_type);
            card["difficulty"] = json!(question.difficulty);
            card["tags"] = json!(question.tags);
            card["status"] = json!(question.status);
            card["user_answer"] = json!(question.user_answer);
            card["is_correct"] = json!(question.is_correct);
            card["attempt_count"] = json!(question.attempt_count);
            card["correct_count"] = json!(question.correct_count);
            card["last_attempt_at"] = json!(question.last_attempt_at);
            card["user_note"] = json!(question.user_note);
            card["is_favorite"] = json!(question.is_favorite);
            card["is_bookmarked"] = json!(question.is_bookmarked);
            card["images"] = json!(question.images);
            card["source_type"] = json!(match question.source_type {
                crate::vfs::repos::SourceType::Manual => "manual_create",
                crate::vfs::repos::SourceType::AiGenerated => "ai_generated",
                crate::vfs::repos::SourceType::Imported => "import_file",
                crate::vfs::repos::SourceType::Ocr => "ocr_image",
            });
            if let Some(source) = source.as_ref() {
                card["source_info"] = source.clone();
            }
            if source.is_some() {
                card["bbox"] = json!({"x":0.0,"y":0.0,"width":0.0,"height":0.0});
                card.as_object_mut()
                    .expect("card object")
                    .remove("resolved_bbox");
                card["bbox_available"] = json!(false);
                if physical && page["is_virtual"].as_bool() != Some(true) {
                    let source = source.as_ref().expect("present");
                    let anchor = source["page_indices"][0].as_u64();
                    if let Some(bbox) = source["page_spans"]
                        .as_array()
                        .and_then(|spans| {
                            spans
                                .iter()
                                .find(|span| span["page_index"].as_u64() == anchor)
                        })
                        .and_then(|span| span["bbox"].as_array())
                    {
                        if bbox.len() == 4 {
                            let values: Option<Vec<f64>> = bbox.iter().map(Value::as_f64).collect();
                            if let Some(v) = values.filter(|v| {
                                v.iter().all(|n| n.is_finite() && (0.0..=1.0).contains(n))
                                    && v[2] > v[0]
                                    && v[3] > v[1]
                            }) {
                                card["bbox"] =
                                    json!({"x":v[0],"y":v[1],"width":v[2]-v[0],"height":v[3]-v[1]});
                                card["bbox_available"] = json!(true);
                                if let (Some(width), Some(height)) = (
                                    page["width"].as_u64().filter(|n| *n > 0),
                                    page["height"].as_u64().filter(|n| *n > 0),
                                ) {
                                    card["resolved_bbox"] = json!({"x":v[0]*width as f64,"y":v[1]*height as f64,
                                        "width":(v[2]-v[0])*width as f64,"height":(v[3]-v[1])*height as f64});
                                }
                            }
                        }
                    }
                }
            } else if card.get("bbox").is_none() {
                card["bbox"] = json!({"x":0.0,"y":0.0,"width":0.0,"height":0.0});
            }
            page["cards"]
                .as_array_mut()
                .expect("page cards array")
                .push(card);
        }
        let cards: Vec<&Value> = self.root["pages"]
            .as_array()
            .expect("pages array")
            .iter()
            .flat_map(|page| page["cards"].as_array().expect("page cards array"))
            .collect();
        let projected: HashSet<&str> = cards
            .iter()
            .filter_map(|card| card["card_id"].as_str())
            .collect();
        if cards.len() != questions.len()
            || projected.len() != seen.len()
            || !seen.iter().all(|id| projected.contains(id.as_str()))
        {
            return Err(AppError::database(
                "正式题目与预览身份集合不一致".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_page_merger::{MergedQuestion, SourceSpan};
    use crate::figure_extractor::QuestionWithFigures;
    use crate::llm_structurer::{StructureSource, StructuredQuestion};
    use crate::vfs::database::setup_migrated_test_db;
    use crate::vlm_grounding_service::VlmQuestion;

    fn pages(count: usize) -> Vec<PageSlice> {
        (0..count)
            .map(|page_index| PageSlice {
                page_index,
                blob_hash: format!("synthetic-page-{page_index}"),
                text_hint: None,
                width: 1000,
                height: 1400,
            })
            .collect()
    }

    fn exam(db: &VfsDatabase, pages: &[PageSlice]) -> String {
        let preview_pages: Vec<Value> = pages
            .iter()
            .map(|p| {
                json!({
                    "page_index": p.page_index, "blob_hash": p.blob_hash,
                    "width": p.width, "height": p.height, "cards": [],
                    "original_image_path": "", "ocr_completed": true, "parse_completed": false,
                })
            })
            .collect();
        let sheet = VfsExamRepo::create_exam_sheet(
            db,
            VfsCreateExamSheetParams {
                exam_name: Some("合成导入回归".into()),
                temp_id: "synthetic-import".into(),
                metadata_json: json!({"preserve_me": {"raw": true}, "file_name": "synthetic.txt"}),
                preview_json: json!({"temp_id":"synthetic-import", "exam_name":"合成导入回归",
                "pages": preview_pages, "instructions":"preserve instructions"}),
                status: "importing".into(),
                folder_id: None,
            },
        )
        .expect("create synthetic exam and resource");
        // A realistic versioned visual checkpoint: recovery actions are only
        // advertised when the persisted state can actually re-run that stage.
        VfsExamRepo::update_import_state(
            db,
            &sheet.id,
            &json!({
                "schema_version": super::super::session_guard::CHECKPOINT_SCHEMA_VERSION,
                "pipeline_version": super::super::session_guard::PIPELINE_VERSION,
                "current_stage": "vlm_analyzing",
                "page_blob_hashes": ["synthetic-checkpoint-page"],
                "vlm_page_results": [],
                "import_mode": "visual",
                "source_document_id": "synthetic-document-v1",
            }),
        )
        .expect("checkpoint");
        sheet.id
    }

    fn block(
        kind: ImportBlockKind,
        section: &str,
        label: &str,
        source_id: &str,
        page: Option<usize>,
        answer: Option<&str>,
    ) -> StructuredQuestion {
        let bbox = [0.1, 0.2, 0.8, 0.6];
        StructuredQuestion {
            json: json!({
                "content": if kind == ImportBlockKind::Answer { "非空答案摘要不应发布".to_string() }
                    else { format!("合成题 {section}/{label}") },
                "question_type":"short_answer", "answer":answer,
                "source_metadata":{"synthetic":true,"text_span":{"start":12,"end":34}},
            }),
            source: QuestionWithFigures {
                merged: MergedQuestion {
                    question: VlmQuestion {
                        block_kind: kind,
                        section: section.into(),
                        label: label.into(),
                        source_id: source_id.into(),
                        raw_text: format!("合成原文 {section}/{label}"),
                        bbox,
                        ..Default::default()
                    },
                    page_indices: page.into_iter().collect(),
                    source_spans: page
                        .map(|page_index| SourceSpan {
                            page_index,
                            bbox,
                            source_id: source_id.into(),
                            label: label.into(),
                        })
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
                images: vec![],
            },
            origin: StructureSource::Llm,
        }
    }

    fn report() -> Value {
        json!({"document_id":"synthetic-document-v1","failed_pages":[],"failed_chunks":[]})
    }

    fn state(db: &VfsDatabase, id: &str) -> (Value, Value, String, Option<String>) {
        let conn = db.get_conn_safe().unwrap();
        let (metadata, preview, status, checkpoint): (String,String,String,Option<String>) = conn.query_row(
            "SELECT metadata_json,preview_json,status,import_state_json FROM exam_sheets WHERE id=?1",
            [id], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
        ).unwrap();
        (
            serde_json::from_str(&metadata).unwrap(),
            serde_json::from_str(&preview).unwrap(),
            status,
            checkpoint,
        )
    }

    fn questions(db: &VfsDatabase, id: &str) -> Vec<crate::vfs::repos::Question> {
        let conn = db.get_conn_safe().unwrap();
        read_all_questions(&conn, id).unwrap()
    }

    fn assert_projection(db: &VfsDatabase, id: &str, expected: usize) {
        let rows = questions(db, id);
        let (_, preview, _, _) = state(db, id);
        let cards: Vec<&Value> = preview["pages"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|page| page["cards"].as_array().unwrap())
            .collect();
        let ids: HashSet<String> = rows.iter().map(question_card_id).collect();
        let preview_ids: HashSet<String> = cards
            .iter()
            .map(|card| card["card_id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(rows.len(), expected);
        assert_eq!(
            cards.len(),
            expected,
            "preview must have one card per live question"
        );
        assert_eq!(
            ids, preview_ids,
            "preview uses the authoritative identities"
        );
        let conn = db.get_conn_safe().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT total_count FROM question_bank_stats WHERE exam_id=?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, expected as i64);
        let resource: String=conn.query_row("SELECT r.data FROM resources r JOIN exam_sheets e ON e.resource_id=r.id WHERE e.id=?1",[id],|r|r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&resource).unwrap(), preview);
    }

    // Negative control uses the real legacy adapter and a synthetic post-merge
    // migration fixture: its raw fallback yields 71 rows while its preview sees
    // only 45 nonempty stems. The new gate below would fail on the old publisher.
    #[test]
    fn legacy_71_rows_45_cards_fixture_is_published_as_38_questions() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut input: Vec<_> = (1..=38)
            .map(|n| {
                block(
                    ImportBlockKind::Question,
                    "scope",
                    &n.to_string(),
                    &format!("q{n}"),
                    None,
                    None,
                )
            })
            .collect();
        for n in 1..=33 {
            let mut a = block(
                ImportBlockKind::Answer,
                "scope",
                &n.to_string(),
                &format!("a{n}"),
                None,
                Some("合成答案"),
            );
            if n <= 26 {
                a.json["content"] = json!("");
            }
            input.push(a);
        }
        let legacy_preview = input
            .iter()
            .filter(|sq| {
                sq.json["content"]
                    .as_str()
                    .is_some_and(|s| !s.trim().is_empty())
            })
            .count();
        let legacy_rows: Vec<_> = input
            .iter()
            .enumerate()
            .map(|(n, sq)| {
                let mut value = sq.json.clone();
                if value["content"].as_str().unwrap_or("").is_empty() {
                    value["content"] = json!(sq.source.merged.question.raw_text);
                }
                super::super::json_to_question_params(&value, &id, &format!("legacy-{n}"), n)
            })
            .collect();
        assert_eq!((legacy_rows.len(), legacy_preview), (71, 45));
        assert!(legacy_rows.iter().all(|q| q.source_ref.is_none()));
        let result =
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!(result.imported_count, 38);
        assert_projection(&db, &id, 38);
        assert!(!questions(&db, &id)
            .iter()
            .any(|q| q.content.contains("答案摘要")));
    }

    #[test]
    fn publishes_38_scoped_questions_with_all_38_answers_and_real_provenance() {
        let (_temp, db) = setup_migrated_test_db();
        let source_pages = pages(8);
        let id = exam(&db, &source_pages);
        let mut stems = Vec::new();
        let mut answers = Vec::new();
        for (scope, count, kind) in [
            ("single_choice", 20, "single_choice"),
            ("term_definition", 10, "short_answer"),
            ("short_answer", 8, "short_answer"),
        ] {
            for number in 1..=count {
                let global = stems.len();
                let page = if global < 8 {
                    0
                } else if global < 19 {
                    1
                } else {
                    2
                };
                let label = number.to_string();
                let mut question = block(
                    ImportBlockKind::Question,
                    scope,
                    &label,
                    &format!("q:{scope}:{number}"),
                    Some(page),
                    None,
                );
                question.json["question_type"] = json!(kind);
                let answer = if kind == "single_choice" {
                    "B".to_string()
                } else {
                    format!("合成答案 {scope}/{number}")
                };
                if kind == "single_choice" {
                    question.json["options"] =
                        json!([{"key":"A","content":"合成甲"},{"key":"B","content":"合成乙"}]);
                }
                stems.push(question);
                answers.push(block(
                    ImportBlockKind::Answer,
                    scope,
                    &label,
                    &format!("a:{scope}:{number}"),
                    Some(3 + global % 5),
                    Some(&answer),
                ));
            }
        }
        // Reordered answers and resetting labels deliberately defeat positional
        // or global-number matching; only their semantic scopes identify targets.
        answers.reverse();
        stems.extend(answers);
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let result = publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &stems,
            0,
            &source_pages,
            report(),
            Some(&sender),
        )
        .unwrap();
        assert_eq!((result.imported_count, result.total_questions), (38, 38));
        assert_projection(&db, &id, 38);
        for question in questions(&db, &id) {
            let source: Value =
                serde_json::from_str(question.source_ref.as_deref().unwrap()).unwrap();
            let label = question.question_label.as_deref().unwrap();
            assert_eq!(source["schema_version"], 1);
            assert_eq!(source["kind"], "document_import");
            assert_eq!(source["document_id"], "synthetic-document-v1");
            assert_eq!(source["label"], label);
            assert_eq!(
                source["block_id"],
                format!("q:{}/{}", source["section"].as_str().unwrap(), label).replace('/', ":")
            );
            assert_eq!(source["source_metadata"]["text_span"]["start"], 12);
            assert_eq!(source["page_spans"][0]["bbox"], json!([0.1, 0.2, 0.8, 0.6]));
            assert_eq!(
                question.card_id.as_deref(),
                Some(
                    stable_card_id(
                        &id,
                        "synthetic-document-v1",
                        source["block_id"].as_str().unwrap()
                    )
                    .as_str()
                )
            );
            let scope = source["section"].as_str().unwrap();
            let expected = if scope == "single_choice" {
                "B".to_string()
            } else {
                format!("合成答案 {scope}/{label}")
            };
            assert_eq!(question.answer.as_deref(), Some(expected.as_str()));
        }
        let (metadata, preview, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "completed");
        assert!(checkpoint.is_none());
        assert_eq!(metadata["preserve_me"]["raw"], true);
        assert_eq!(metadata["import_report"]["published_count"], 38);
        assert_eq!(metadata["import_report"]["inserted_count"], 38);
        assert_eq!(metadata["import_report"]["answers_matched"], 38);
        assert_eq!(
            metadata["import_report"]["matched_entries"]
                .as_array()
                .unwrap()
                .len(),
            38
        );
        assert!(
            metadata["import_report"]["matched_entries"][0]["raw_text_sha256"]
                .as_str()
                .is_some_and(|hash| hash.len() == 64)
        );
        assert_eq!(
            metadata["import_report"]["matched_entries"][0]["raw_text_chars"],
            "合成原文 single_choice/1".chars().count()
        );
        assert!(metadata["import_report"]["matched_entries"][0]["source"]["page_spans"].is_array());
        assert_eq!(metadata["import_report"]["resume_available"], false);
        assert!(metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap()
            .is_empty());
        let counts: Vec<_> = preview["pages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["cards"].as_array().unwrap().len())
            .collect();
        assert_eq!(counts, vec![8, 11, 19, 0, 0, 0, 0, 0]);
        let card = &preview["pages"][0]["cards"][0];
        assert_eq!(card["bbox"]["x"], 0.1);
        assert!((card["bbox"]["width"].as_f64().unwrap() - 0.7).abs() < 1e-9);
        assert_eq!(card["resolved_bbox"]["x"], 100.0);
        assert_eq!(preview["instructions"], "preserve instructions");
        let mut parsed = 0;
        while let Ok(event) = receiver.try_recv() {
            match event {
                QuestionImportProgress::QuestionParsed { .. } => {
                    // Reopen a different connection on receipt: publication was
                    // committed before any progress escaped the transaction.
                    assert_eq!(questions(&db, &id).len(), 38);
                    parsed += 1;
                }
                QuestionImportProgress::Completed {
                    total_questions,
                    partial,
                    ..
                } => {
                    assert_eq!(total_questions, 38);
                    assert!(!partial);
                }
                _ => panic!("unexpected publication progress"),
            }
        }
        assert_eq!(parsed, 38);
    }

    #[test]
    fn all_250_questions_are_projected_and_nonvisual_sources_do_not_fake_pages() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input: Vec<_> = (0..250)
            .map(|n| {
                let mut q = block(
                    ImportBlockKind::Question,
                    "long",
                    &n.to_string(),
                    &format!("text:{n}"),
                    Some(0),
                    None,
                );
                // Text spans are not physical page coordinates, even when an old
                // adapter fabricated a SourceSpan with page_index=0.
                q.source.merged.source_spans[0].bbox = [0.0; 4];
                q
            })
            .collect();
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 250);
        let (_, preview, _, _) = state(&db, &id);
        assert_eq!(preview["pages"].as_array().unwrap().len(), 1);
        assert_eq!(preview["pages"][0]["page_index"], 0);
        assert_eq!(preview["pages"][0]["is_virtual"], true);
        assert!(preview["pages"][0].get("blob_hash").is_none());
        for question in questions(&db, &id) {
            let source: Value =
                serde_json::from_str(question.source_ref.as_deref().unwrap()).unwrap();
            assert_eq!(source["page_indices"], json!([]));
            assert_eq!(source["page_spans"], json!([]));
            assert_eq!(
                source["source_metadata"]["text_span"],
                json!({"start":12,"end":34})
            );
        }
        assert_eq!(preview["pages"][0]["cards"][0]["bbox_available"], false);
        assert!(preview["pages"][0]["cards"][0]
            .get("resolved_bbox")
            .is_none());
    }

    #[test]
    fn replay_preserves_ids_edits_answers_notes_and_attempt_history() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![
            block(
                ImportBlockKind::Question,
                "s",
                "原号-1",
                "q1",
                None,
                Some("原答案"),
            ),
            block(ImportBlockKind::Question, "s", "原号-2", "q2", None, None),
        ];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let before = questions(&db, &id);
        let q1 = before
            .iter()
            .find(|q| q.question_label.as_deref() == Some("原号-1"))
            .unwrap();
        VfsQuestionRepo::update_question(
            &db,
            &q1.id,
            &UpdateQuestionParams {
                content: Some("用户编辑后的题干".into()),
                answer: Some("用户维护答案".into()),
                user_note: Some("用户笔记不能消失".into()),
                is_favorite: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        VfsQuestionRepo::submit_answer(&db, &q1.id, "我的作答", Some(false), false).unwrap();
        let submission = VfsQuestionRepo::insert_submission(
            &db,
            &q1.id,
            "我的作答",
            Some(false),
            "synthetic",
            None,
        )
        .unwrap();
        let q2 = before
            .iter()
            .find(|q| q.question_label.as_deref() == Some("原号-2"))
            .unwrap();
        VfsQuestionRepo::update_question(
            &db,
            &q2.id,
            &UpdateQuestionParams {
                user_note: Some("可安全补答但保留笔记".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut retry = input.clone();
        retry[1].json["answer"] = json!("补充的原文答案");
        retry.reverse();
        let snapshot = VfsQuestionRepo::get_question(&db, &q1.id).unwrap().unwrap();
        let result =
            publish_candidates(&db, &id, "合成导入回归", &retry, 0, &[], report(), None).unwrap();
        assert_eq!((result.imported_count, result.total_questions), (0, 2));
        let after = questions(&db, &id);
        let old_ids: HashSet<_> = before.iter().map(|q| (&q.id, &q.card_id)).collect();
        let new_ids: HashSet<_> = after.iter().map(|q| (&q.id, &q.card_id)).collect();
        assert_eq!(old_ids, new_ids);
        let preserved = VfsQuestionRepo::get_question(&db, &q1.id).unwrap().unwrap();
        assert_eq!(
            serde_json::to_value(&preserved).unwrap(),
            serde_json::to_value(&snapshot).unwrap()
        );
        let filled = VfsQuestionRepo::get_question(&db, &q2.id).unwrap().unwrap();
        assert_eq!(filled.answer.as_deref(), Some("补充的原文答案"));
        assert_eq!(filled.user_note.as_deref(), Some("可安全补答但保留笔记"));
        let conn = db.get_conn_safe().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM answer_submissions WHERE id=?1",
                [submission],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
        drop(conn);
        let (metadata, preview, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert_eq!(metadata["import_report"]["safe_answer_fills"], 1);
        assert!(!metadata["import_report"]["conflicts"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(metadata["import_report"]["published_count"], 2);
        assert_eq!(metadata["import_report"]["inserted_count"], 0);
        assert_eq!(
            preview["pages"].as_array().unwrap().len(),
            1,
            "retry must reuse virtual page"
        );
        assert_projection(&db, &id, 2);
    }

    #[test]
    fn soft_deleted_identity_is_not_resurrected_on_retry() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(ImportBlockKind::Question, "s", "1", "q1", None, None)];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let row = questions(&db, &id).pop().unwrap();
        VfsQuestionRepo::delete_question(&db, &row.id).unwrap();
        let result =
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!((result.imported_count, result.total_questions), (0, 0));
        assert_projection(&db, &id, 0);
        let conn = db.get_conn_safe().unwrap();
        let (count, deleted): (i64, i64) = conn
            .query_row(
                "SELECT COUNT(*),SUM(deleted_at IS NOT NULL) FROM questions WHERE exam_id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((count, deleted), (1, 1));
        drop(conn);
        let (metadata, _, _, _) = state(&db, &id);
        assert!(metadata["import_report"]["excluded_entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["reason"] == "previously_soft_deleted"));
    }

    #[test]
    fn publication_failure_rolls_back_questions_preview_stats_resource_checkpoint_and_events() {
        // Fail both an intermediate and the last SQL write. Neither may leave
        // progress, an early preview/resource write, stats or questions behind.
        for trigger in [
            "CREATE TRIGGER fail_publication BEFORE INSERT ON questions WHEN NEW.question_label='2' BEGIN SELECT RAISE(ABORT,'synthetic question failure'); END;",
            "CREATE TRIGGER fail_publication BEFORE UPDATE OF metadata_json ON exam_sheets BEGIN SELECT RAISE(ABORT,'synthetic final write failure'); END;",
        ] {
            let (_temp,db)=setup_migrated_test_db();
            let id=exam(&db,&[]);
            let initial=state(&db,&id);
            let resource=VfsExamRepo::get_exam_sheet_content(&db,&id).unwrap();
            {
                let conn=db.get_conn_safe().unwrap();
                conn.execute_batch(trigger).unwrap();
            }
            let input=vec![block(ImportBlockKind::Question,"s","1","q1",None,None),block(ImportBlockKind::Question,"s","2","q2",None,None)];
            let (sender,mut receiver)=tokio::sync::mpsc::unbounded_channel();
            assert!(publish_candidates(&db,&id,"合成导入回归",&input,0,&[],report(),Some(&sender)).is_err());
            assert!(questions(&db,&id).is_empty());
            assert_eq!(state(&db,&id),initial);
            assert_eq!(VfsExamRepo::get_exam_sheet_content(&db,&id).unwrap(),resource);
            let conn=db.get_conn_safe().unwrap();
            let count:i64=conn.query_row("SELECT COUNT(*) FROM question_bank_stats WHERE exam_id=?1",[&id],|r|r.get(0)).unwrap();
            assert_eq!(count,0);
            assert!(receiver.try_recv().is_err(),"rolled-back imports must not emit published progress");
        }
    }

    #[test]
    fn partial_and_failed_coverage_keep_checkpoint_review_stays_durable() {
        let (_temp, db) = setup_migrated_test_db();
        let source_pages = pages(2);
        let id = exam(&db, &source_pages);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q1",
            Some(0),
            None,
        )];
        let mut partial = report();
        partial["failed_pages"] = json!([1]);
        partial["page_coverage"] =
            json!([{"page_index":0,"state":"succeeded"},{"page_index":1,"state":"failed"}]);
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &input,
            0,
            &source_pages,
            partial,
            None,
        )
        .unwrap();
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "partial");
        assert!(checkpoint.is_some());
        assert_eq!(metadata["import_report"]["resume_available"], true);
        assert_projection(&db, &id, 1);
        let failed_id = exam(&db, &source_pages);
        publish_candidates(
            &db,
            &failed_id,
            "合成导入回归",
            &[],
            0,
            &source_pages,
            json!({"document_id":"all-failed","failed_pages":[0,1]}),
            None,
        )
        .unwrap();
        let (metadata, _, status, checkpoint) = state(&db, &failed_id);
        assert_eq!(status, "failed");
        assert!(checkpoint.is_some());
        assert_eq!(metadata["import_report"]["resume_available"], true);
        let review_id = exam(&db, &[]);
        let mut unknown = block(
            ImportBlockKind::Unknown,
            "s",
            "?",
            "unknown",
            None,
            Some("非题目"),
        );
        unknown.json["content"] = json!("非空 unknown 也不能发布");
        publish_candidates(
            &db,
            &review_id,
            "合成导入回归",
            &[unknown],
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, status, checkpoint) = state(&db, &review_id);
        assert_eq!(status, "needs_review");
        assert!(checkpoint.is_some());
        assert_eq!(
            metadata["import_report"]["quarantined_entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_projection(&db, &review_id, 0);
        let empty_id = exam(&db, &[]);
        publish_candidates(
            &db,
            &empty_id,
            "合成导入回归",
            &input,
            0,
            &[],
            json!({"document_id":"unconfirmed-empty","unconfirmed_empty_pages":[0]}),
            None,
        )
        .unwrap();
        assert_eq!(state(&db, &empty_id).2, "needs_review");
    }

    #[test]
    fn validates_choice_options_and_never_publishes_an_invalid_grading_key() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut invalid_key = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "choice1",
            None,
            Some("C"),
        );
        invalid_key.json["question_type"] = json!("single_choice");
        invalid_key.json["options"] =
            json!([{"key":"A","content":"合成甲"},{"key":"B","content":"合成乙"}]);
        let mut duplicate = invalid_key.clone();
        duplicate.source.merged.question.source_id = "duplicate-option".into();
        duplicate.json["options"][1]["key"] = json!("a");
        let mut empty = invalid_key.clone();
        empty.source.merged.question.source_id = "empty-stem".into();
        empty.json["content"] = json!("");
        let mut identity_missing = invalid_key.clone();
        identity_missing.source.merged.question.source_id.clear();
        let input = vec![invalid_key, duplicate, empty, identity_missing];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 1);
        let rows = questions(&db, &id);
        assert!(rows[0].answer.is_none());
        assert_eq!(rows[0].status, crate::vfs::repos::QuestionStatus::Review);
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let reasons: HashSet<_> = metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["reason"].as_str())
            .collect();
        assert!(reasons.contains("answer_references_missing_or_unverifiable_option"));
        assert!(reasons.contains("duplicate_choice_option"));
        assert!(reasons.contains("empty_or_invalid_content"));
        assert!(reasons.contains("missing_source_id"));
        assert_eq!(
            metadata["import_report"]["identity_risks"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn append_isolates_document_page_zero_and_reuses_mapping_on_retry() {
        let (_temp, db) = setup_migrated_test_db();
        let original_pages = pages(1);
        let id = exam(&db, &original_pages);
        let first = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q1",
            Some(0),
            None,
        )];
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &first,
            0,
            &original_pages,
            report(),
            None,
        )
        .unwrap();
        let mut second_pages = pages(1);
        second_pages[0].blob_hash = "second-document-page-zero".into();
        let second = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q1",
            Some(0),
            None,
        )];
        let second_report = json!({"document_id":"synthetic-document-v2"});
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &second,
            0,
            &second_pages,
            second_report.clone(),
            None,
        )
        .unwrap();
        assert_projection(&db, &id, 2);
        let (_, preview, _, _) = state(&db, &id);
        assert_eq!(preview["pages"].as_array().unwrap().len(), 2);
        assert_eq!(preview["pages"][0]["blob_hash"], "synthetic-page-0");
        assert_eq!(
            preview["pages"][1]["blob_hash"],
            "second-document-page-zero"
        );
        assert_eq!(preview["pages"][0]["cards"].as_array().unwrap().len(), 1);
        assert_eq!(preview["pages"][1]["cards"].as_array().unwrap().len(), 1);
        assert_eq!(
            preview["pages"][1]["cards"][0]["source_info"]["page_indices"],
            json!([0])
        );
        assert_eq!(
            preview["pages"][1]["cards"][0]["source_info"]["preview_page_indices"],
            json!([1])
        );
        let result = publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &second,
            0,
            &second_pages,
            second_report,
            None,
        )
        .unwrap();
        assert_eq!((result.imported_count, result.total_questions), (0, 2));
        let (_, replayed, _, _) = state(&db, &id);
        assert_eq!(replayed["pages"].as_array().unwrap().len(), 2);
        assert_eq!(preview, replayed);
    }

    #[test]
    fn ordinary_same_stem_questions_are_not_deduplicated_and_source_collisions_need_review() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut input: Vec<_> = (0..4)
            .map(|n| {
                block(
                    ImportBlockKind::Question,
                    "distinct-sources",
                    &format!("{n}"),
                    &format!("source-{n}"),
                    None,
                    None,
                )
            })
            .collect();
        input[0].json["content"] = json!("同一个题干");
        input[1].json["content"] = json!("同一个题干");
        input[2].json["content"] = json!("求-1与1/2的值");
        input[3].json["content"] = json!("求1与12的值");
        let mut collision_a = block(ImportBlockKind::Question, "s", "5", "collision", None, None);
        let mut collision_b = collision_a.clone();
        collision_a.json["content"] = json!("冲突源的第一个版本");
        collision_b.json["content"] = json!("冲突源的第二个版本");
        input.extend([collision_a, collision_b]);
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 4);
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert_eq!(
            metadata["import_report"]["identity_risks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap()
            .iter()
            .all(|entry| entry["reason"] == "conflicting_source_identity"));
    }

    #[test]
    fn review_without_checkpoint_cannot_claim_resume_and_answer_only_is_never_a_question() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        VfsExamRepo::clear_import_state(&db, &id).unwrap();
        let input = vec![block(
            ImportBlockKind::Answer,
            "s",
            "1",
            "unmatched-answer",
            None,
            Some("B"),
        )];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 0);
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert!(checkpoint.is_none());
        assert_eq!(metadata["import_report"]["resume_available"], false);
        let quarantine = metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap();
        assert_eq!(quarantine.len(), 1);
        assert_eq!(quarantine[0]["block_kind"], "answer");
        assert!(quarantine[0]["json_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64));
        assert!(quarantine[0]["source"]["json_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key == "content"));
        assert!(quarantine[0].get("json").is_none());
        assert_eq!(quarantine[0]["source"]["page_indices"], json!([]));
    }

    #[test]
    fn explicitly_unverified_answers_never_publish_or_fill_an_attempted_question() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut initial = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "unverified",
            None,
            Some("伪造的答案"),
        );
        initial.json["source_metadata"]["answer_evidence_verified"] = json!(false);
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &[initial.clone()],
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        assert_projection(&db, &id, 1);
        let question = questions(&db, &id).pop().unwrap();
        assert!(question.answer.is_none());
        VfsQuestionRepo::submit_answer(&db, &question.id, "已有作答", None, true).unwrap();
        VfsQuestionRepo::update_question(
            &db,
            &question.id,
            &UpdateQuestionParams {
                user_note: Some("作答笔记".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let before = VfsQuestionRepo::get_question(&db, &question.id)
            .unwrap()
            .unwrap();
        publish_candidates(&db, &id, "合成导入回归", &[initial], 0, &[], report(), None).unwrap();
        let after = VfsQuestionRepo::get_question(&db, &question.id)
            .unwrap()
            .unwrap();
        assert_eq!(
            serde_json::to_value(before).unwrap(),
            serde_json::to_value(after).unwrap()
        );
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert_eq!(metadata["import_report"]["safe_answer_fills"], 0);
        assert!(metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["reason"] == "answer_evidence_not_verified"));

        // A matched answer donor cannot bypass the flag on its own source.
        let donor_id = exam(&db, &[]);
        let target = block(ImportBlockKind::Question, "s", "2", "target", None, None);
        let mut donor = block(
            ImportBlockKind::Answer,
            "s",
            "2",
            "donor",
            None,
            Some("未证实的匹配答案"),
        );
        donor.json["source_metadata"]["answer_evidence_verified"] = json!(false);
        publish_candidates(
            &db,
            &donor_id,
            "合成导入回归",
            &[target, donor],
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let target = questions(&db, &donor_id).pop().unwrap();
        assert!(target.answer.is_none());
        assert_eq!(state(&db, &donor_id).2, "needs_review");
    }

    #[test]
    fn legacy_questions_without_source_are_not_mapped_by_stem_or_label() {
        let (_temp, db) = setup_migrated_test_db();
        let source_pages = pages(1);
        let id = exam(&db, &source_pages);
        let mut params = super::super::json_to_question_params(
            &json!({"content":"同题干","question_type":"short_answer"}),
            &id,
            "old-card",
            0,
        );
        params.card_id = None;
        let old = VfsQuestionRepo::create_question(&db, &params).unwrap();
        let preview = json!({"pages":[{"page_index":0,"blob_hash":"synthetic-page-0","cards":[{
            "card_id":old.id,"question_label":"old","ocr_text":"同题干",
            "bbox":{"x":0.2,"y":0.3,"width":0.4,"height":0.2},"private_card_field":"preserved"
        }]}]});
        VfsExamRepo::update_preview_json(&db, &id, preview).unwrap();
        let mut input = block(ImportBlockKind::Question, "s", "Q1", "new", Some(0), None);
        input.json["content"] = json!("同题干");
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &[input],
            0,
            &source_pages,
            report(),
            None,
        )
        .unwrap();
        assert_projection(&db, &id, 2);
        let rows = questions(&db, &id);
        assert!(rows
            .iter()
            .find(|q| q.id == old.id)
            .unwrap()
            .source_ref
            .is_none());
        let (_, preview, _, _) = state(&db, &id);
        let card = preview["pages"][0]["cards"]
            .as_array()
            .unwrap()
            .iter()
            .find(|card| card["card_id"] == old.id)
            .unwrap();
        assert_eq!(card["bbox"]["x"], 0.2);
        assert_eq!(card["private_card_field"], "preserved");
    }

    #[test]
    fn source_ref_keeps_inline_evidence_while_explicit_false_blocks_both_fields() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut input = block(
            ImportBlockKind::Question,
            "scope",
            "1",
            "evidence-q",
            None,
            Some("B"),
        );
        input.json["answer_evidence"] = json!("合成原文：答案 B");
        input.json["explanation"] = json!("合成解析");
        input.json["explanation_evidence"] = json!("合成原文：解析");
        input.json["source_metadata"]["answer_evidence_verified"] = json!(false);
        input.json["source_metadata"]["explanation_evidence_verified"] = json!(false);
        publish_candidates(&db, &id, "合成导入回归", &[input], 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 1);
        let question = questions(&db, &id).pop().unwrap();
        assert!(question.answer.is_none());
        assert!(question.explanation.is_none());
        let source: Value = serde_json::from_str(question.source_ref.as_deref().unwrap()).unwrap();
        assert_eq!(source["answer_evidence"], "合成原文：答案 B");
        assert_eq!(source["explanation_evidence"], "合成原文：解析");
        assert_eq!(source["source_metadata"]["answer_evidence_verified"], false);
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let conflicts = metadata["import_report"]["conflicts"].as_array().unwrap();
        assert!(conflicts
            .iter()
            .any(|entry| entry["field"] == "answer"
                && entry["reason"] == "answer_evidence_not_verified"));
        assert!(conflicts.iter().any(|entry| entry["field"] == "explanation"
            && entry["reason"] == "explanation_evidence_not_verified"));
        assert_eq!(metadata["import_report"]["safe_fill_entries"], json!([]));
    }

    #[test]
    fn safe_fill_audits_each_field_with_inline_donor_provenance() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let target = block(
            ImportBlockKind::Question,
            "scope",
            "1",
            "q-fill",
            None,
            None,
        );
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&target),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let mut answer_donor = block(
            ImportBlockKind::Answer,
            "scope",
            "1",
            "a-fill",
            None,
            Some("合成答案"),
        );
        answer_donor.json["answer_evidence"] = json!("合成原文：答案");
        let mut explanation_donor = block(
            ImportBlockKind::Explanation,
            "scope",
            "1",
            "e-fill",
            None,
            None,
        );
        explanation_donor.json["explanation"] = json!("合成解析");
        explanation_donor.json["explanation_evidence"] = json!("合成原文：解析");
        let input = vec![target, answer_donor, explanation_donor];
        let result =
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!((result.imported_count, result.total_questions), (0, 1));
        let question = questions(&db, &id).pop().unwrap();
        assert_eq!(question.answer.as_deref(), Some("合成答案"));
        assert_eq!(question.explanation.as_deref(), Some("合成解析"));
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        assert_eq!(metadata["import_report"]["safe_answer_fills"], 1);
        assert_eq!(metadata["import_report"]["safe_explanation_fills"], 1);
        assert_eq!(metadata["import_report"]["safe_fill_count"], 2);
        let entries = metadata["import_report"]["safe_fill_entries"]
            .as_array()
            .unwrap();
        assert_eq!(entries.len(), 2);
        let answer = entries
            .iter()
            .find(|entry| entry["field"] == "answer")
            .unwrap();
        assert_eq!(answer["question_id"], question.id.as_str());
        assert_eq!(answer["source_id"], "q-fill");
        assert!(answer["value_sha256"].is_string());
        assert!(answer["evidence_sha256"].is_string());
        assert!(answer.get("value").is_none());
        assert!(answer.get("evidence").is_none());
        assert_eq!(answer["evidence_source_id"], "a-fill");
        assert_eq!(answer["evidence_kind"], "inline");
        let explanation = entries
            .iter()
            .find(|entry| entry["field"] == "explanation")
            .unwrap();
        assert_eq!(explanation["source_id"], "q-fill");
        assert!(explanation["value_sha256"].is_string());
        assert!(explanation["evidence_sha256"].is_string());
        assert_eq!(explanation["evidence_source_id"], "e-fill");
        assert!(metadata["import_report"]["conflicts"]
            .as_array()
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unverified_donor_cannot_safe_fill_an_existing_empty_answer_or_explanation() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let target = block(ImportBlockKind::Question, "scope", "1", "q-old", None, None);
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&target),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let mut answer_donor = block(
            ImportBlockKind::Answer,
            "scope",
            "1",
            "a-old",
            None,
            Some("未证实答案"),
        );
        answer_donor.json["source_metadata"]["answer_evidence_verified"] = json!(false);
        let mut explanation_donor = block(
            ImportBlockKind::Explanation,
            "scope",
            "1",
            "e-old",
            None,
            None,
        );
        explanation_donor.json["explanation"] = json!("未证实解析");
        explanation_donor.json["source_metadata"]["answer_evidence_verified"] = json!(false);
        let input = vec![target, answer_donor, explanation_donor];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let question = questions(&db, &id).pop().unwrap();
        assert!(question.answer.is_none());
        assert!(question.explanation.is_none());
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert_eq!(metadata["import_report"]["safe_answer_fills"], 0);
        assert_eq!(metadata["import_report"]["safe_explanation_fills"], 0);
        assert_eq!(metadata["import_report"]["safe_fill_entries"], json!([]));
        let quarantined = metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap();
        assert!(quarantined
            .iter()
            .any(|entry| entry["reason"] == "answer_evidence_not_verified"));
        assert!(quarantined
            .iter()
            .any(|entry| entry["reason"] == "explanation_evidence_not_verified"));
    }

    #[test]
    fn figure_or_deferred_failures_never_allow_completed_and_keep_resume() {
        for (key, count_key) in [
            ("figure_failures", "figure_failure_count"),
            ("deferred_continuations", "deferred_continuation_count"),
        ] {
            let (_temp, db) = setup_migrated_test_db();
            let id = exam(&db, &[]);
            let mut report = report();
            report[key] = json!([{"source_id":"synthetic-source","reason":"合成失败"}]);
            let input = vec![block(
                ImportBlockKind::Question,
                "s",
                "1",
                "q-gate",
                None,
                Some("合成答案"),
            )];
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report, None).unwrap();
            assert_projection(&db, &id, 1);
            let (metadata, _, status, checkpoint) = state(&db, &id);
            assert_eq!(status, "needs_review", "{key} must not complete");
            assert!(checkpoint.is_some(), "{key} must keep the checkpoint");
            // Only stage-retry actions are executable resumes; a deferred
            // continuation is review-only until adjacent pages are supplied.
            let expected_resume = key == "figure_failures";
            assert_eq!(
                metadata["import_report"]["resume_available"], expected_resume,
                "{key}"
            );
            assert_eq!(metadata["import_report"]["needs_review"], true);
            assert_eq!(metadata["import_report"][count_key], 1);
            assert_eq!(metadata["import_report"][key].as_array().unwrap().len(), 1);
        }
    }

    #[test]
    fn cached_unresolved_continuation_pages_are_reidentified_not_just_reviewed() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-cont",
            None,
            Some("合成答案"),
        )];
        let mut report = report();
        report["deferred_continuations"] = json!([{
            "source_id": "q-cont",
            "page_spans": [{"page_index": 0}, {"page_index": 1}],
            "reason": "unresolved_continuation",
        }]);
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report, None).unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let report = &metadata["import_report"];
        assert_eq!(report["resume_available"], true);
        let action = report["recovery_actions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|action| action["action"] == "reidentify_page")
            .expect("continuation pages must be re-recognisable");
        assert_eq!(action["pages"], json!([0, 1]));
        assert_eq!(action["reason"], "unresolved_page_continuation");
    }

    #[test]
    fn heading_or_material_only_candidates_never_complete_an_empty_import() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut heading = block(ImportBlockKind::Heading, "", "一、选择题", "h1", None, None);
        heading.json["content"] = json!("一、选择题");
        let mut material = block(ImportBlockKind::Material, "reading", "", "m1", None, None);
        material.json["content"] = json!("阅读理解材料正文");
        let input = vec![heading, material];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 0);
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert!(
            checkpoint.is_some(),
            "an empty publication must keep the checkpoint for review"
        );
        assert_eq!(
            metadata["import_report"]["empty_import_reason"],
            "no_publishable_questions"
        );
        assert_eq!(metadata["import_report"]["publishable_count"], 0);
        assert_eq!(metadata["import_report"]["needs_review"], true);
    }

    #[test]
    fn truly_empty_input_is_distinguished_from_discarded_candidates() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        publish_candidates(&db, &id, "合成导入回归", &[], 0, &[], report(), None).unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert_eq!(
            metadata["import_report"]["empty_import_reason"],
            "no_questions"
        );
    }

    #[test]
    fn user_excluded_sources_have_a_distinct_terminal_state_without_review() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(ImportBlockKind::Question, "s", "1", "q1", None, None)];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let row = questions(&db, &id).pop().unwrap();
        VfsQuestionRepo::delete_question(&db, &row.id).unwrap();
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "excluded");
        assert_eq!(
            metadata["import_report"]["empty_import_reason"],
            "all_sources_excluded_by_user"
        );
        assert_eq!(metadata["import_report"]["excluded_by_user_count"], 1);
        assert_eq!(metadata["import_report"]["needs_review"], false);
        assert_eq!(metadata["import_report"]["resume_available"], false);
    }

    #[test]
    fn replay_of_existing_sources_is_effective_publication_coverage() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-replay",
            None,
            Some("合成答案"),
        )];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let result =
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!(result.imported_count, 0);
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        assert_eq!(metadata["import_report"]["published_count"], 1);
        assert_eq!(metadata["import_report"]["inserted_count"], 0);
        assert!(metadata["import_report"]["empty_import_reason"].is_null());
    }

    #[test]
    fn publication_enqueues_index_units_for_changed_questions_in_the_same_transaction() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-index",
            None,
            Some("A"),
        )];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let row = questions(&db, &id).pop().unwrap();
        let units = |question_id: &str| -> i64 {
            let conn = db.get_conn_safe().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM vfs_index_units WHERE resource_id=?1 AND unit_index=0",
                [question_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        let unit_updated_at = |question_id: &str| -> i64 {
            let conn = db.get_conn_safe().unwrap();
            conn.query_row(
                "SELECT updated_at FROM vfs_index_units WHERE resource_id=?1 AND unit_index=0",
                [question_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(units(&row.id), 1);
        let before = unit_updated_at(&row.id);
        std::thread::sleep(std::time::Duration::from_millis(5));
        // A pure replay changes nothing and must not bump the index generation.
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!(units(&row.id), 1);
        assert_eq!(unit_updated_at(&row.id), before);
    }

    #[test]
    fn incomplete_batches_are_failures_not_a_completed_import() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-batch",
            None,
            Some("合成答案"),
        )];
        let mut report = report();
        report["incomplete_batches"] = json!([3]);
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report, None).unwrap();
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "partial");
        assert!(checkpoint.is_some());
        assert_eq!(metadata["import_report"]["incomplete_batch_count"], 1);
        assert_eq!(metadata["import_report"]["failed_count"], 1);
        assert_eq!(metadata["import_report"]["resume_available"], true);
    }

    #[test]
    fn resume_keeps_safe_fill_audit_entries() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let target = block(ImportBlockKind::Question, "s", "1", "q-fill", None, None);
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&target),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let donor = block(
            ImportBlockKind::Answer,
            "s",
            "1",
            "a-fill",
            None,
            Some("合成答案"),
        );
        let input = vec![target.clone(), donor];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        let (filled, _, _, _) = state(&db, &id);
        assert_eq!(
            filled["import_report"]["safe_fill_entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // A later resume of the same document must not erase durable backfill audit.
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&target),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (resumed, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        assert_eq!(
            resumed["import_report"]["safe_fill_entries"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn report_audit_redacts_material_text_and_model_payload() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut question = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-secret",
            None,
            Some("SECRET-ANSWER-TEXT"),
        );
        question.source.merged.question.raw_text = "SECRET-RAW-STUDENT-MATERIAL".into();
        question.json["content"] = json!("SECRET-QUESTION-CONTENT");
        let unmatched = block(
            ImportBlockKind::Answer,
            "s",
            "99",
            "a-secret",
            None,
            Some("SECRET-UNMATCHED-ANSWER"),
        );
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &[question, unmatched],
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, _, _) = state(&db, &id);
        let serialized = metadata["import_report"].to_string();
        for secret in [
            "SECRET-ANSWER-TEXT",
            "SECRET-RAW-STUDENT-MATERIAL",
            "SECRET-QUESTION-CONTENT",
            "SECRET-UNMATCHED-ANSWER",
        ] {
            assert!(
                !serialized.contains(secret),
                "import report must not duplicate source material: {secret}"
            );
        }
        let quarantine = metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap();
        assert_eq!(quarantine.len(), 1);
        assert!(quarantine[0].get("raw_text").is_none());
        assert!(quarantine[0].get("json").is_none());
        assert!(quarantine[0]["raw_text_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64));
        assert!(quarantine[0]["json_sha256"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64));
        assert!(quarantine[0]["source"]["raw_text_sha256"].is_string());
    }

    #[test]
    fn options_on_unexpected_question_types_are_validated_and_unknown_enums_reviewed() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut malformed = block(ImportBlockKind::Question, "s", "1", "opt-dup", None, None);
        malformed.json["question_type"] = json!("essay");
        malformed.json["options"] = json!([
            {"key":"A","content":"合成甲"},
            {"key":"A","content":"合成乙"}
        ]);
        let mut unknown_enum = block(ImportBlockKind::Question, "s", "2", "enum", None, None);
        unknown_enum.json["question_type"] = json!("quantum_choice");
        let good = block(ImportBlockKind::Question, "s", "3", "good", None, None);
        let input = vec![malformed, unknown_enum, good];
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_projection(&db, &id, 1);
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let reasons: HashSet<_> = metadata["import_report"]["quarantined_entries"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["reason"].as_str())
            .collect();
        assert!(reasons.contains("invalid_option_structure"));
        assert!(reasons.contains("unknown_question_type_enum"));
    }

    #[test]
    fn report_exposes_severity_tagged_actions_instead_of_a_bare_resume_flag() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-action",
            None,
            Some("合成答案"),
        )];
        let mut report = report();
        report["failed_pages"] = json!([2]);
        report["figure_failures"] = json!([{"source_id":"q-action","reason":"合成图失败"}]);
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report, None).unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "partial");
        let report = &metadata["import_report"];
        let actions = report["recovery_actions"].as_array().unwrap();
        let names: HashSet<_> = actions
            .iter()
            .filter_map(|action| action["action"].as_str())
            .collect();
        assert!(names.contains("reidentify_page"));
        assert!(names.contains("retry_figures"));
        assert_eq!(report["resume_available"], true);
        assert_eq!(report["completion"]["blocking_count"], 2);
        assert_eq!(report["figure_failures"][0]["severity"], "blocking");
    }

    #[test]
    fn classified_non_question_entries_are_warnings_not_blockers() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let question = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q1",
            None,
            Some("合成答案"),
        );
        let mut heading = block(ImportBlockKind::Heading, "", "一、单选", "h1", None, None);
        heading.json["content"] = json!("一、单选");
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            &[question, heading],
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        let report = &metadata["import_report"];
        let excluded = report["excluded_entries"].as_array().unwrap();
        assert!(excluded.iter().all(|entry| entry["severity"] == "warning"));
        assert_eq!(report["completion"]["blocking_count"], 0);
        assert_eq!(report["completion"]["warning_count"], 1);
        assert!(report["recovery_actions"].as_array().unwrap().is_empty());
        assert_eq!(report["resume_available"], false);
    }

    #[test]
    fn representation_equivalent_stem_is_not_a_user_edit() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut first = block(ImportBlockKind::Question, "chem", "1", "chem-1", None, None);
        first.json["content"] = json!("Ca²⁺浓度对平衡的影响");
        first.source.merged.question.raw_text = "1. Ca²⁺浓度对平衡的影响".into();
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&first),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let mut replay = first.clone();
        replay.json["content"] = json!("$Ca^{2+}$浓度对平衡的影响");
        replay.source.merged.question.raw_text = "1. $Ca^{2+}$浓度对平衡的影响".into();
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&replay),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        assert!(metadata["import_report"]["conflicts"]
            .as_array()
            .unwrap()
            .is_empty());
        assert_eq!(questions(&db, &id).len(), 1);
    }

    #[test]
    fn different_charge_is_a_real_representation_conflict() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut first = block(ImportBlockKind::Question, "chem", "1", "chem-2", None, None);
        first.json["content"] = json!("Ca²⁺浓度对平衡的影响");
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&first),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let mut replay = first.clone();
        replay.json["content"] = json!("$Ca^{3+}$浓度对平衡的影响");
        replay.source.merged.question.raw_text = "1. $Ca^{3+}$浓度对平衡的影响".into();
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&replay),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let conflicts = metadata["import_report"]["conflicts"].as_array().unwrap();
        assert!(conflicts
            .iter()
            .any(|entry| entry["reason"] == "existing_question_edited"));
        assert_eq!(questions(&db, &id)[0].content, "Ca²⁺浓度对平衡的影响");
    }

    #[test]
    fn user_edit_is_preserved_through_three_way_baseline() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let q = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "baseline-q",
            None,
            Some("A"),
        );
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&q),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        // Second import records the field baseline ("A").
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&q),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let row = questions(&db, &id).pop().unwrap();
        VfsQuestionRepo::update_question(
            &db,
            &row.id,
            &UpdateQuestionParams {
                answer: Some("用户答案".into()),
                ..Default::default()
            },
        )
        .unwrap();
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&q),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let after = questions(&db, &id).pop().unwrap();
        assert_eq!(after.answer.as_deref(), Some("用户答案"));
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "completed");
        assert!(metadata["import_report"]["conflicts"]
            .as_array()
            .unwrap()
            .is_empty());
        assert!(metadata["import_report"]["preserved_edits"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["reason"] == "user_edit_preserved"));
    }

    #[test]
    fn legacy_record_without_baseline_is_reviewed_not_blamed_on_the_user() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let q = block(
            ImportBlockKind::Question,
            "s",
            "1",
            "legacy-q",
            None,
            Some("A"),
        );
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&q),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let row = questions(&db, &id).pop().unwrap();
        // Emulate a record created before field baselines existed.
        {
            let conn = db.get_conn_safe().unwrap();
            let source: String = conn
                .query_row(
                    "SELECT source_ref FROM questions WHERE id=?1",
                    [&row.id],
                    |r| r.get(0),
                )
                .unwrap();
            let mut source: Value = serde_json::from_str(&source).unwrap();
            source.as_object_mut().unwrap().remove("import_baseline");
            conn.execute(
                "UPDATE questions SET source_ref=?1 WHERE id=?2",
                rusqlite::params![source.to_string(), row.id],
            )
            .unwrap();
        }
        VfsQuestionRepo::update_question(
            &db,
            &row.id,
            &UpdateQuestionParams {
                answer: Some("用户答案".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let mut replay = q.clone();
        replay.json["answer"] = json!("B");
        publish_candidates(
            &db,
            &id,
            "合成导入回归",
            std::slice::from_ref(&replay),
            0,
            &[],
            report(),
            None,
        )
        .unwrap();
        let (metadata, _, status, _) = state(&db, &id);
        assert_eq!(status, "needs_review");
        let conflicts = metadata["import_report"]["conflicts"].as_array().unwrap();
        assert!(conflicts
            .iter()
            .any(|entry| entry["reason"] == "legacy_record_without_baseline"));
        assert_eq!(questions(&db, &id)[0].answer.as_deref(), Some("用户答案"));
    }

    #[test]
    fn group_materials_and_children_publish_in_one_transaction_without_standalone_duplicates() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let mut material = block(
            ImportBlockKind::Material,
            "reading",
            "",
            "material-1",
            None,
            None,
        );
        material.json["content"] = json!("阅读材料：合成短文");
        material.source.merged.question.raw_text = "阅读材料：合成短文".into();
        material.source.merged.question.is_group = true;
        material.source.merged.question.sub_questions = vec!["1".into(), "2".into()];
        let mut first = block(
            ImportBlockKind::Question,
            "reading",
            "1",
            "child-1",
            None,
            None,
        );
        first.json["content"] = json!("子题一");
        let mut second = block(
            ImportBlockKind::Question,
            "reading",
            "2",
            "child-2",
            None,
            None,
        );
        second.json["content"] = json!("子题二");
        let input = vec![material, first, second];
        let sources: Vec<_> = input.iter().map(|sq| sq.source.clone()).collect();
        let typed_blocks =
            crate::question_import_service::groups::blocks_before_answer_merge(&sources);
        publish_candidates_with_groups(
            &db,
            &id,
            "合成导入回归",
            &input,
            0,
            &[],
            report(),
            None,
            Some(typed_blocks),
        )
        .unwrap();
        // Only the two children are answerable questions; the group header and
        // material never become a question, and nothing is published twice.
        assert_eq!(questions(&db, &id).len(), 2);
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "completed");
        assert!(checkpoint.is_none());
        let report = &metadata["import_report"];
        assert_eq!(report["groups"]["group_count"], 1);
        assert_eq!(report["groups"]["group_child_count"], 2);
        assert_eq!(report["group_child_published_count"], 2);
        assert_eq!(report["groups"]["unconsumed_material_count"], 0);
        assert_eq!(report["publishable_count"], 2);
    }

    #[test]
    fn deferred_continuation_blocks_completion_then_replay_completes_idempotently() {
        let (_temp, db) = setup_migrated_test_db();
        let id = exam(&db, &[]);
        let input = vec![block(
            ImportBlockKind::Question,
            "s",
            "1",
            "q-continuation",
            None,
            Some("合成答案"),
        )];
        let mut deferred = report();
        deferred["deferred_continuations"] =
            json!([{"source_id":"q-continuation","reason":"合成跨页未闭合"}]);
        publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], deferred, None).unwrap();
        assert_projection(&db, &id, 1);
        let first_card = questions(&db, &id)[0].card_id.clone().unwrap();
        assert_eq!(
            first_card,
            stable_card_id(&id, "synthetic-document-v1", "q-continuation")
        );
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "needs_review");
        assert!(checkpoint.is_some());
        assert_eq!(metadata["import_report"]["deferred_continuation_count"], 1);

        let result =
            publish_candidates(&db, &id, "合成导入回归", &input, 0, &[], report(), None).unwrap();
        assert_eq!((result.imported_count, result.total_questions), (0, 1));
        let second_card = questions(&db, &id)[0].card_id.clone().unwrap();
        assert_eq!(first_card, second_card);
        let (metadata, _, status, checkpoint) = state(&db, &id);
        assert_eq!(status, "completed");
        assert!(checkpoint.is_none());
        assert_eq!(metadata["import_report"]["deferred_continuation_count"], 0);
        assert_eq!(metadata["import_report"]["resume_available"], false);
        assert_projection(&db, &id, 1);
    }
}
