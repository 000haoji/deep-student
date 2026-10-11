//! 内置附件工具执行器
//!
//! 解决 P0 断裂点：用户上传的附件无法通过工具主动读取
//!
//! 执行两个内置附件工具：
//! - `builtin-attachment_list` - 列出会话中的附件
//! - `builtin-attachment_read` - 读取指定附件内容
//!
//! ## 设计说明
//! 该执行器通过 ChatDatabase 访问历史消息中的附件，
//! 为 LLM 提供主动读取用户上传附件的能力。

use std::collections::HashSet;
use std::time::Instant;

use async_trait::async_trait;
use rusqlite::OptionalExtension;
use serde_json::{json, Value};

use super::arg_utils::{ensure_localized_error, with_localized_message};
use super::executor::{ExecutionContext, ToolConcurrency, ToolExecutor, ToolSensitivity};
use super::strip_tool_namespace;
use crate::chat_v2::repo::ChatV2Repo;
use crate::chat_v2::resource_types::ContextRef;
use crate::chat_v2::types::{ToolCall, ToolResultInfo};
use crate::document_parser::DocumentParser;
use crate::vfs::repos::attachment_repo::VfsAttachmentRepo;

pub(crate) fn attachment_error(
    code: &str,
    zh_cn: impl Into<String>,
    en_us: impl Into<String>,
    hint_zh: impl Into<String>,
    hint_en: impl Into<String>,
    retryable: bool,
) -> String {
    let zh_cn = zh_cn.into();
    let en_us = en_us.into();
    let hint_zh = hint_zh.into();
    let hint_en = hint_en.into();
    with_localized_message(
        json!({
            "code": code,
            "hint": hint_zh,
            "hintFallback": {
                "zh-CN": hint_zh,
                "en-US": hint_en,
            },
            "retryable": retryable,
        }),
        "chat.tools.attachment.error",
        json!({ "code": code, "detail": zh_cn.clone() }),
        zh_cn,
        en_us,
    )
    .to_string()
}

fn schema_unavailable_error() -> String {
    attachment_error(
        "ATTACHMENT_STORE_SCHEMA_UNAVAILABLE",
        "附件存储表不可用，请先完成数据库迁移",
        "The attachment store schema is unavailable. Run database migrations first.",
        "完成数据库迁移后再调用 attachment_list/read；不要改试 attachment_stage",
        "Run database migrations, then retry attachment_list/read. Do not retry with attachment_stage.",
        false,
    )
}

fn invalid_attachment_args_error(param: &str) -> String {
    attachment_error(
        "ATTACHMENT_INVALID_ARGS",
        format!("缺少非空参数 {param}"),
        format!("Missing non-empty '{param}' parameter"),
        "必须提供非空 message_id 与 attachment_id。已有 <attachment_metadata> 时直接使用 rootId/relativePath/objectHandle；历史附件先 attachment_list",
        "Provide non-empty message_id and attachment_id. If <attachment_metadata> is present, use rootId/relativePath/objectHandle directly; otherwise call attachment_list first.",
        false,
    )
}

fn attachment_not_found_error(detail: impl Into<String>) -> String {
    let detail = detail.into();
    attachment_error(
        "ATTACHMENT_NOT_FOUND",
        detail.clone(),
        detail,
        "用 attachment_list 或已有 <attachment_metadata> 取得真实 message_id/attachment_id；不要用 attachment_stage 搜索附件",
        "Use attachment_list or existing <attachment_metadata> to get a real message_id/attachment_id. Do not use attachment_stage to search attachments.",
        false,
    )
}

fn attachment_source_unavailable_error(detail: impl Into<String>) -> String {
    let detail = detail.into();
    attachment_error(
        "ATTACHMENT_SOURCE_UNAVAILABLE",
        detail.clone(),
        detail,
        "附件记录仍存在，但原始内容已不可用；attachment_list/stage 无法修复，请让用户重新附加该文件",
        "The attachment record exists but its source bytes are unavailable. Listing or staging cannot repair it; ask the user to attach the file again.",
        false,
    )
}

fn staged_file_not_found_error(detail: impl Into<String>) -> String {
    let detail = detail.into();
    attachment_error(
        "ATTACHMENT_STAGED_FILE_NOT_FOUND",
        detail.clone(),
        detail,
        "暂存文件已失效；重新调用 attachment_stage，并把新返回的 root_id/relative_path 传给 attachment_extract",
        "The staged file expired. Call attachment_stage again and pass its new root_id/relative_path to attachment_extract.",
        true,
    )
}

pub(crate) fn required_attachment_id(args: &Value, key: &str) -> Result<String, String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid_attachment_args_error(key))
}

fn is_schema_unavailable(error: &str) -> bool {
    error.to_ascii_lowercase().contains("no such table")
}

fn is_attachment_not_found(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.starts_with("attachment not found:") || lower.starts_with("message not found:")
}

fn is_attachment_source_unavailable(error: &str) -> bool {
    let lower = error.to_ascii_lowercase();
    lower.starts_with("attachment source not found in vfs:")
        || lower.starts_with("resource not found in vfs:")
        || lower.contains("has no raw content available")
}

fn is_staged_file_not_found(error: &str) -> bool {
    error
        .to_ascii_lowercase()
        .starts_with("staged file not found:")
}

pub(crate) fn localized_attachment_failure(error: impl Into<String>) -> String {
    let raw = error.into();
    let mapped = if serde_json::from_str::<Value>(&raw)
        .ok()
        .as_ref()
        .is_some_and(Value::is_object)
    {
        raw
    } else if is_schema_unavailable(&raw) {
        schema_unavailable_error()
    } else if is_staged_file_not_found(&raw) {
        staged_file_not_found_error(raw)
    } else if is_attachment_source_unavailable(&raw) {
        attachment_source_unavailable_error(raw)
    } else if is_attachment_not_found(&raw) {
        attachment_not_found_error(raw)
    } else {
        raw
    };
    ensure_localized_error(
        mapped,
        "ATTACHMENT_OPERATION_FAILED",
        "chat.tools.attachment.error",
        "附件操作失败",
        "The attachment operation failed.",
    )
}

// ============================================================================
// 常量
// ============================================================================

/// 默认列表数量
const DEFAULT_LIST_LIMIT: u32 = 20;

// ============================================================================
// 内置附件工具执行器
// ============================================================================

/// 内置附件工具执行器
///
/// 处理以 `builtin-` 开头的附件工具：
/// - `builtin-attachment_list` - 列出会话附件
/// - `builtin-attachment_read` - 读取附件内容
pub struct AttachmentToolExecutor;

impl AttachmentToolExecutor {
    /// 创建新的附件工具执行器
    pub fn new() -> Self {
        Self
    }

    /// 执行附件列表
    async fn execute_list(&self, call: &ToolCall, ctx: &ExecutionContext) -> Result<Value, String> {
        let chat_v2_db = ctx
            .chat_v2_db
            .as_ref()
            .ok_or("Chat V2 database not available")?;

        // P0-01 安全修复：验证 session_id 参数，防止跨会话访问
        if let Some(param_session_id) = call.arguments.get("session_id").and_then(|v| v.as_str()) {
            if param_session_id != ctx.session_id {
                log::warn!(
                    "[AttachmentToolExecutor] Ignore mismatched session_id parameter: expected={}, got={}",
                    ctx.session_id,
                    param_session_id
                );
            }
        }

        // 解析参数（始终使用当前会话 ID）
        let session_id = ctx.session_id.clone();
        let type_filter = call
            .arguments
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("all");
        let limit = call
            .arguments
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_LIST_LIMIT as u64) as u32;

        log::debug!(
            "[AttachmentToolExecutor] attachment_list: session_id={}, type={}, limit={}",
            session_id,
            type_filter,
            limit
        );

        let start_time = Instant::now();

        // 查询会话中的消息
        let messages = ChatV2Repo::get_session_messages_v2(chat_v2_db, &session_id)
            .map_err(|e| localized_attachment_failure(format!("Failed to get messages: {}", e)))?;

        // 收集所有附件（兼容 legacy attachments + context_snapshot.user_refs）
        let mut attachments: Vec<Value> = Vec::new();
        let mut seen_keys: HashSet<String> = HashSet::new();
        for message in &messages {
            if let Some(ref atts) = message.attachments {
                for att in atts {
                    // 类型过滤
                    if type_filter != "all" && att.r#type != type_filter {
                        continue;
                    }

                    let dedupe_key = format!("{}::{}", message.id, att.id);
                    if !seen_keys.insert(dedupe_key) {
                        continue;
                    }

                    attachments.push(json!({
                        "attachment_id": att.id,
                        "message_id": message.id,
                        "name": att.name,
                        "type": att.r#type,
                        "mime_type": att.mime_type,
                        "size": att.size,
                        "status": att.status,
                        "timestamp": message.timestamp,
                    }));

                    if attachments.len() >= limit as usize {
                        break;
                    }
                }
            }

            if let Some(meta) = &message.meta {
                if let Some(snapshot) = &meta.context_snapshot {
                    for context_ref in &snapshot.user_refs {
                        if !matches!(context_ref.type_id.as_str(), "file" | "image" | "folder") {
                            continue;
                        }

                        let mapped_type = map_context_ref_to_attachment_type(&context_ref.type_id);
                        if type_filter != "all" && mapped_type != type_filter {
                            continue;
                        }

                        let dedupe_key = format!("{}::{}", message.id, context_ref.resource_id);
                        if !seen_keys.insert(dedupe_key) {
                            continue;
                        }

                        attachments.push(json!({
                            "attachment_id": context_ref.resource_id,
                            "message_id": message.id,
                            "name": context_ref.display_name.clone().unwrap_or_else(|| context_ref.resource_id.clone()),
                            "type": mapped_type,
                            "mime_type": map_context_ref_to_mime(&context_ref.type_id),
                            "size": Value::Null,
                            "status": "context_ref",
                            "timestamp": message.timestamp,
                            "source": "context_snapshot",
                        }));

                        if attachments.len() >= limit as usize {
                            break;
                        }
                    }
                }
            }

            if attachments.len() >= limit as usize {
                break;
            }
        }

        let duration = start_time.elapsed().as_millis() as u64;

        log::debug!(
            "[AttachmentToolExecutor] attachment_list completed: {} attachments in {}ms",
            attachments.len(),
            duration
        );

        Ok(json!({
            "success": true,
            "session_id": session_id,
            "attachments": attachments,
            "count": attachments.len(),
            "durationMs": duration,
        }))
    }

    /// 执行附件读取
    ///
    /// 工具描述指引：本工具只返回解析后的文本/base64，不提供磁盘路径。
    /// 二进制或大文件（xlsx/zip/图片等）应改用 `attachment_stage` 物化到
    /// temp root 拿到 root_id + relative_path 后，再用 workspace/shell 工具处理。
    async fn execute_read(&self, call: &ToolCall, ctx: &ExecutionContext) -> Result<Value, String> {
        let chat_v2_db = ctx
            .chat_v2_db
            .as_ref()
            .ok_or("Chat V2 database not available")?;

        let message_id = required_attachment_id(&call.arguments, "message_id")?;
        let attachment_id = required_attachment_id(&call.arguments, "attachment_id")?;
        let parse_content = call
            .arguments
            .get("parse_content")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        log::debug!(
            "[AttachmentToolExecutor] attachment_read: message_id={}, attachment_id={}, parse_content={}",
            message_id, attachment_id, parse_content
        );

        let start_time = Instant::now();

        // 获取消息
        let message = ChatV2Repo::get_message_v2(chat_v2_db, &message_id)
            .map_err(|e| localized_attachment_failure(format!("Failed to get message: {}", e)))?
            .ok_or_else(|| {
                attachment_not_found_error(format!("Message not found: {}", message_id))
            })?;

        // P0-01 安全修复：验证消息所属会话，防止跨会话访问
        if message.session_id != ctx.session_id {
            return Err("Unauthorized: Cannot access attachments from other sessions".to_string());
        }

        if let Some(attachment) = message
            .attachments
            .as_ref()
            .and_then(|atts| atts.iter().find(|a| a.id == attachment_id.as_str()))
        {
            // 从 preview_url 提取内容
            let content = if let Some(preview_url) = &attachment.preview_url {
                if preview_url.starts_with("data:") {
                    // 解析 data URL: data:mime_type;base64,content
                    let parts: Vec<&str> = preview_url.splitn(2, ",").collect();
                    if parts.len() == 2 {
                        let base64_content = parts[1];

                        // 判断是否为文本类型
                        let is_text_type = attachment.mime_type.starts_with("text/")
                            || attachment.mime_type == "application/json"
                            || attachment.mime_type == "application/xml"
                            || attachment.mime_type == "application/javascript";

                        if is_text_type {
                            // 文本类型：base64 解码
                            use base64::Engine;
                            let decoded = base64::engine::general_purpose::STANDARD
                                .decode(base64_content)
                                .map_err(|e| format!("Failed to decode base64: {}", e))?;
                            String::from_utf8(decoded)
                                .map_err(|e| format!("Invalid UTF-8: {}", e))?
                        } else if attachment.r#type == "image" {
                            // 图片类型：返回 base64（让多模态模型处理）
                            base64_content.to_string()
                        } else if parse_content {
                            // 文档类型：尝试使用 DocumentParser 解析
                            let parser = DocumentParser::new();
                            match parser.extract_text_from_base64(&attachment.name, base64_content)
                            {
                                Ok(text) => text,
                                Err(e) => {
                                    log::warn!(
                                        "[AttachmentToolExecutor] Failed to parse document {}: {}",
                                        attachment.name,
                                        e
                                    );
                                    format!("[文档: {}] (解析失败: {})", attachment.name, e)
                                }
                            }
                        } else {
                            // 不解析，返回原始 base64
                            base64_content.to_string()
                        }
                    } else {
                        return Err("Invalid data URL format".to_string());
                    }
                } else {
                    return Err("Attachment content not available (no data URL)".to_string());
                }
            } else {
                return Err("Attachment has no preview_url".to_string());
            };

            let duration = start_time.elapsed().as_millis() as u64;

            log::debug!(
                "[AttachmentToolExecutor] attachment_read completed: id={}, content_len={}, {}ms",
                attachment_id,
                content.len(),
                duration
            );

            return Ok(json!({
                "success": true,
                "attachment_id": attachment_id,
                "message_id": message_id,
                "name": attachment.name,
                "type": attachment.r#type,
                "mime_type": attachment.mime_type,
                "content": content,
                "contentLength": content.len(),
                "durationMs": duration,
            }));
        }

        // 统一引用模式兼容：支持读取 context_snapshot.user_refs 中的 file_/tb_/att_
        let context_ref = message
            .meta
            .as_ref()
            .and_then(|meta| meta.context_snapshot.as_ref())
            .and_then(|snapshot| {
                snapshot
                    .user_refs
                    .iter()
                    .find(|r| r.resource_id == attachment_id)
            })
            .ok_or_else(|| {
                attachment_not_found_error(format!(
                    "Attachment not found: {} in message {}",
                    attachment_id, message_id
                ))
            })?;

        let (name, mime_type, content) = read_context_ref_content(ctx, context_ref, parse_content)?;
        let duration = start_time.elapsed().as_millis() as u64;

        Ok(json!({
            "success": true,
            "attachment_id": attachment_id,
            "message_id": message_id,
            "name": name,
            "type": map_context_ref_to_attachment_type(&context_ref.type_id),
            "mime_type": mime_type,
            "content": content,
            "contentLength": content.len(),
            "source": "context_snapshot",
            "durationMs": duration,
        }))
    }
}

fn map_context_ref_to_attachment_type(type_id: &str) -> &'static str {
    match type_id {
        "image" => "image",
        _ => "document",
    }
}

fn map_context_ref_to_mime(type_id: &str) -> &'static str {
    match type_id {
        "image" => "image/*",
        "folder" => "inode/directory",
        _ => "application/octet-stream",
    }
}

/// 一条可读取的 VFS 文件行（`files` + 其内联资源 `resources.data`）。
///
/// 注意列名是 `resources.data` 而非 `content`——vfs schema 里没有 `content` 列，
/// 历史实现写成 `COALESCE(r.content, '')` 会让每次 `res_*` 读取都以
/// `no such column: r.content` 失败（issue #468）。
struct ContextRefFileRow {
    file_id: String,
    name: String,
    mime_type: String,
    /// `resources.data`：inline 模式下可能是 base64 内容，也可能是引用 JSON（图片资源）。
    data: String,
}

/// 把 context_ref 的 `resource_id` 解析成一条 `files` 行。
///
/// ID → `files.id` 的解析走 `VfsAttachmentRepo::resolve_to_file_id`（SSOT），
/// 与 `vfs_get_attachment_content`、题库图片写入校验同一口径：`file_/att_/tb_/img_`
/// 直接命中，`res_*` 经 `files.resource_id` / `resources.source_id` 间接解析。
fn resolve_context_ref_file_row(
    conn: &rusqlite::Connection,
    resource_id: &str,
) -> Result<Option<ContextRefFileRow>, String> {
    let file_id =
        VfsAttachmentRepo::resolve_to_file_id(conn, resource_id).map_err(|e| e.to_string())?;
    let Some(file_id) = file_id else {
        return Ok(None);
    };

    conn.query_row(
        "SELECT f.id, COALESCE(f.file_name, f.id), COALESCE(f.mime_type, ''), COALESCE(r.data, '') \
         FROM files f \
         LEFT JOIN resources r ON f.resource_id = r.id \
         WHERE f.id = ?1 \
           AND f.deleted_at IS NULL \
           AND (r.deleted_at IS NULL OR r.id IS NULL)",
        rusqlite::params![file_id.as_str()],
        |row| {
            Ok(ContextRefFileRow {
                file_id: row.get(0)?,
                name: row.get(1)?,
                mime_type: row.get(2)?,
                data: row.get(3)?,
            })
        },
    )
    .optional()
    .map_err(|e| e.to_string())
}

fn read_context_ref_content(
    ctx: &ExecutionContext,
    context_ref: &ContextRef,
    parse_content: bool,
) -> Result<(String, String, String), String> {
    let vfs_db = ctx
        .vfs_db
        .as_ref()
        .ok_or("VFS database not available for context ref read")?;
    let conn = vfs_db.get_conn_safe().map_err(|e| e.to_string())?;

    if context_ref.resource_id.starts_with("fld_") {
        return Err("Folder context reference is not readable via attachment_read".to_string());
    }

    let row = resolve_context_ref_file_row(&conn, &context_ref.resource_id)?.ok_or_else(|| {
        format!(
            "Resource not found in VFS: {}（可尝试用 resource_list 确认资源，或重新上传附件）",
            context_ref.resource_id
        )
    })?;

    log::debug!(
        "[AttachmentToolExecutor] read_context_ref_content: resource_id={}, file_id={}, mime_type={}, data_len={}",
        context_ref.resource_id,
        row.file_id,
        row.mime_type,
        row.data.len()
    );

    let is_image_ref = context_ref.type_id == "image" || row.mime_type.starts_with("image/");

    // 图片的字节必须走 blob / 内联 base64，而不是 `resources.data`——后者对图片资源
    // 存的是引用 JSON（`{"refs":[…]}`），直接当内容返回会让模型拿到一串 JSON。
    // `VfsAttachmentRepo::get_content_with_conn` 是 UI 命令同款解析器：
    // 内联 base64 → resources.data，外部文件 → original_path / blob_hash。
    let raw_content = if is_image_ref {
        match VfsAttachmentRepo::get_content_with_conn(&conn, vfs_db.blobs_dir(), &row.file_id) {
            Ok(Some(content)) if !content.is_empty() => content,
            Ok(_) => {
                log::warn!(
                    "[AttachmentToolExecutor] 图片内容为空: file_id={}, resource_id={}",
                    row.file_id,
                    context_ref.resource_id
                );
                return Err(attachment_error(
                    "ATTACHMENT_BLOB_MISSING",
                    format!(
                        "图片内容缺失：{}（file_id={}）",
                        context_ref.resource_id, row.file_id
                    ),
                    format!(
                        "Image content missing: {} (file_id={})",
                        context_ref.resource_id, row.file_id
                    ),
                    "该图片的 blob 已丢失，请重新上传；不要反复重试同一附件",
                    "The image blob is missing; re-upload it instead of retrying",
                    false,
                ));
            }
            Err(e) => {
                log::warn!(
                    "[AttachmentToolExecutor] 图片内容读取失败: file_id={}, error={}",
                    row.file_id,
                    e
                );
                return Err(attachment_error(
                    "ATTACHMENT_BLOB_MISSING",
                    format!("图片内容读取失败：{}（{}）", context_ref.resource_id, e),
                    format!(
                        "Failed to read image content: {} ({})",
                        context_ref.resource_id, e
                    ),
                    "该图片的 blob 已丢失，请重新上传；不要反复重试同一附件",
                    "The image blob is missing; re-upload it instead of retrying",
                    false,
                ));
            }
        }
    } else {
        row.data.clone()
    };

    // 兼容历史数据：部分图片内容以 `data:` URL 形式内联，剥离前缀只留 base64。
    let content = if is_image_ref && raw_content.starts_with("data:") {
        raw_content
            .split_once(',')
            .map(|(_, right)| right.to_string())
            .unwrap_or_default()
    } else {
        raw_content
    };

    let _ = parse_content; // 图片始终返回 base64；文本内容已由上方分支决定
    Ok((row.name, row.mime_type, content))
}

impl Default for AttachmentToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ToolExecutor for AttachmentToolExecutor {
    fn can_handle(&self, tool_name: &str) -> bool {
        let stripped = strip_tool_namespace(tool_name);
        matches!(stripped, "attachment_list" | "attachment_read")
    }

    async fn execute(
        &self,
        call: &ToolCall,
        ctx: &ExecutionContext,
    ) -> Result<ToolResultInfo, String> {
        let start_time = Instant::now();
        let tool_name = strip_tool_namespace(&call.name);

        log::debug!(
            "[AttachmentToolExecutor] Executing builtin tool: {} (full: {})",
            tool_name,
            call.name
        );

        // 发射工具调用开始事件
        ctx.emit_tool_call_start(&call.name, call.arguments.clone(), Some(&call.id));

        let result = match tool_name {
            "attachment_list" => self.execute_list(call, ctx).await,
            "attachment_read" => self.execute_read(call, ctx).await,
            _ => Err(format!("Unknown attachment tool: {}", tool_name)),
        };

        let duration = start_time.elapsed().as_millis() as u64;

        match result {
            Ok(output) => {
                // 发射工具调用结束事件
                ctx.emit_tool_call_end(Some(json!({
                    "result": output,
                    "durationMs": duration,
                })));

                let result = ToolResultInfo::success(
                    Some(call.id.clone()),
                    Some(ctx.block_id.clone()),
                    call.name.clone(),
                    call.arguments.clone(),
                    output,
                    duration,
                );

                // SSOT: 后端立即保存工具块（防闪退）
                if let Err(e) = ctx.save_tool_block(&result) {
                    log::warn!("[AttachmentToolExecutor] Failed to save tool block: {}", e);
                }

                Ok(result)
            }
            Err(e) => {
                let e = localized_attachment_failure(e);
                ctx.emit_tool_call_error(&e);

                let result = ToolResultInfo::failure(
                    Some(call.id.clone()),
                    Some(ctx.block_id.clone()),
                    call.name.clone(),
                    call.arguments.clone(),
                    e,
                    duration,
                );

                // SSOT: 后端立即保存工具块（防闪退）
                if let Err(e) = ctx.save_tool_block(&result) {
                    log::warn!("[AttachmentToolExecutor] Failed to save tool block: {}", e);
                }

                Ok(result)
            }
        }
    }

    fn sensitivity_level(&self, _tool_name: &str) -> ToolSensitivity {
        // 附件工具是只读操作，低敏感
        ToolSensitivity::Low
    }

    fn concurrency_class(&self, _tool_name: &str) -> ToolConcurrency {
        // attachment_list / attachment_read 均为只读，可并行 + 自动重试
        ToolConcurrency::ReadOnly
    }

    fn name(&self) -> &'static str {
        "AttachmentToolExecutor"
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_v2::database::ChatV2Database;
    use crate::chat_v2::events::ChatV2EventEmitter;
    use crate::chat_v2::types::{AttachmentMeta, ChatMessage, ChatSession};
    use crate::data_governance::migration::coordinator::MigrationCoordinator;
    use crate::data_governance::schema_registry::DatabaseId;
    use crate::tools::ToolRegistry;
    use std::sync::Arc;

    fn setup_attachment_context() -> (tempfile::TempDir, ExecutionContext) {
        let temp_dir = tempfile::tempdir().expect("create ChatV2 test directory");
        let mut coordinator =
            MigrationCoordinator::new(temp_dir.path().to_path_buf()).with_audit_db(None);
        coordinator
            .migrate_single(DatabaseId::ChatV2)
            .expect("apply ChatV2 migrations");
        let chat_db =
            Arc::new(ChatV2Database::new(temp_dir.path()).expect("open migrated ChatV2 database"));

        let session_id = "sess_attachment_db".to_string();
        ChatV2Repo::create_session_v2(
            &chat_db,
            &ChatSession::new(session_id.clone(), "chat".to_string()),
        )
        .expect("persist session");
        let mut message = ChatMessage::new_user(session_id.clone(), Vec::new());
        message.id = "msg_attachment_db".to_string();
        message.attachments = Some(vec![AttachmentMeta {
            id: "att_text".to_string(),
            name: "note.txt".to_string(),
            r#type: "document".to_string(),
            mime_type: "text/plain".to_string(),
            size: 5,
            preview_url: Some("data:text/plain;base64,aGVsbG8=".to_string()),
            status: "ready".to_string(),
            error: None,
        }]);
        ChatV2Repo::create_message_v2(&chat_db, &message).expect("persist attachment message");

        let emitter = Arc::new(ChatV2EventEmitter::new_windowless_for_test(
            session_id.clone(),
        ));
        let ctx = ExecutionContext::new(
            session_id,
            message.id,
            "blk_attachment_db".to_string(),
            emitter,
            Arc::new(ToolRegistry::new()),
            None,
        )
        .with_chat_v2_db(Some(chat_db));
        (temp_dir, ctx)
    }

    #[test]
    fn test_can_handle() {
        let executor = AttachmentToolExecutor::new();

        // 处理附件工具
        assert!(executor.can_handle("builtin-attachment_list"));
        assert!(executor.can_handle("builtin-attachment_read"));

        // 不处理其他 builtin 工具
        assert!(!executor.can_handle("builtin-rag_search"));
        assert!(!executor.can_handle("builtin-resource_read"));
    }

    #[test]
    fn test_strip_namespace() {
        assert_eq!(
            strip_tool_namespace("builtin-attachment_list"),
            "attachment_list"
        );
        assert_eq!(strip_tool_namespace("attachment_read"), "attachment_read");
    }

    #[test]
    fn test_sensitivity_level() {
        let executor = AttachmentToolExecutor::new();
        assert_eq!(
            executor.sensitivity_level("builtin-attachment_list"),
            ToolSensitivity::Low
        );
        assert_eq!(
            executor.sensitivity_level("builtin-attachment_read"),
            ToolSensitivity::Low
        );
        assert_eq!(
            executor.concurrency_class("builtin-attachment_list"),
            ToolConcurrency::ReadOnly
        );
        assert_eq!(
            executor.concurrency_class("builtin-attachment_read"),
            ToolConcurrency::ReadOnly
        );
        assert!(!executor.can_handle("builtin-attachment_stage"));
    }

    #[test]
    fn structured_errors_map_schema_args_and_not_found() {
        let schema: Value = serde_json::from_str(&localized_attachment_failure(
            "Failed to get messages: no such table: attachments",
        ))
        .expect("schema error");
        assert_eq!(schema["code"], "ATTACHMENT_STORE_SCHEMA_UNAVAILABLE");
        assert_eq!(schema["retryable"], false);
        assert!(schema["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("不要改试 attachment_stage")));

        let missing = required_attachment_id(&json!({}), "message_id").unwrap_err();
        let args: Value =
            serde_json::from_str(&localized_attachment_failure(missing)).expect("invalid args");
        assert_eq!(args["code"], "ATTACHMENT_INVALID_ARGS");
        assert_eq!(args["retryable"], false);
        assert!(args["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("attachment_list")));

        let empty =
            required_attachment_id(&json!({ "attachment_id": " " }), "attachment_id").unwrap_err();
        let empty_args: Value =
            serde_json::from_str(&localized_attachment_failure(empty)).expect("empty id");
        assert_eq!(empty_args["code"], "ATTACHMENT_INVALID_ARGS");

        let missing_attachment: Value = serde_json::from_str(&localized_attachment_failure(
            "Attachment not found: att_x in message msg_x",
        ))
        .expect("not found");
        assert_eq!(missing_attachment["code"], "ATTACHMENT_NOT_FOUND");
        assert_eq!(missing_attachment["retryable"], false);
        assert!(missing_attachment["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("attachment_list")));

        let missing_source: Value = serde_json::from_str(&localized_attachment_failure(
            "Resource not found in VFS: file_x",
        ))
        .expect("missing source");
        assert_eq!(missing_source["code"], "ATTACHMENT_SOURCE_UNAVAILABLE");
        assert!(missing_source["hint"]
            .as_str()
            .is_some_and(|hint| hint.contains("重新附加")));
    }

    #[tokio::test]
    async fn attachment_queries_use_chat_v2_database_without_main_database() {
        let (_temp_dir, ctx) = setup_attachment_context();
        let executor = AttachmentToolExecutor::new();

        let list = executor
            .execute_list(
                &ToolCall::new(
                    "call_list".to_string(),
                    "builtin-attachment_list".to_string(),
                    json!({}),
                ),
                &ctx,
            )
            .await
            .expect("list attachments from ChatV2 database");
        assert_eq!(list["count"], 1);
        assert_eq!(list["attachments"][0]["attachment_id"], "att_text");

        let read = executor
            .execute_read(
                &ToolCall::new(
                    "call_read".to_string(),
                    "builtin-attachment_read".to_string(),
                    json!({
                        "message_id": "msg_attachment_db",
                        "attachment_id": "att_text"
                    }),
                ),
                &ctx,
            )
            .await
            .expect("read attachment from ChatV2 database");
        assert_eq!(read["content"], "hello");
    }

    /// issue #468 回归：`res_*` 图片引用必须能读回 base64 内容。
    ///
    /// 历史实现把 SQL 写成 `COALESCE(r.content, '')`，而 vfs `resources` 表的列名是
    /// `data`——每次 `res_*` 读取都以 `no such column: r.content` 失败。这里同时验证
    /// 「res_* → files.resource_id → blob」的完整解析链。
    fn setup_res_image_ctx(
        res_id: &str,
        file_id: &str,
        link_via_source_id: bool,
    ) -> (tempfile::TempDir, ExecutionContext, String) {
        use crate::vfs::repos::VfsBlobRepo;

        let (temp_dir, vfs_db) = crate::vfs::database::setup_migrated_test_db();
        let vfs_db = Arc::new(vfs_db);

        let image_bytes: &[u8] = b"\x89PNG\r\n\x1a\nres-image-bytes";
        let expected_base64 = {
            use base64::Engine;
            base64::engine::general_purpose::STANDARD.encode(image_bytes)
        };

        {
            let conn = vfs_db.get_conn_safe().expect("get VFS connection");
            let blob = VfsBlobRepo::store_blob_with_conn(
                &conn,
                vfs_db.blobs_dir(),
                image_bytes,
                Some("image/png"),
                Some("png"),
            )
            .expect("store blob");

            // 引用资源：data 存的是引用 JSON（不是 base64），字节在 blob 里。
            // source_id 指向 files.id（link_via_source_id 时）或留空（经 files.resource_id 关联）。
            conn.execute(
                "INSERT INTO resources (id, hash, type, source_id, storage_mode, data, created_at, updated_at) \
                 VALUES (?1, ?2, 'image', ?3, 'inline', ?4, 1, 1)",
                rusqlite::params![
                    res_id,
                    format!("{res_id}_hash"),
                    if link_via_source_id { Some(file_id) } else { None },
                    "{\"refs\":[]}"
                ],
            )
            .expect("insert resource");

            conn.execute(
                "INSERT INTO files (id, resource_id, blob_hash, sha256, file_name, size, tags_json, status, created_at, updated_at, type, name, mime_type) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, '[]', 'active', '2026-09-04T00:00:00Z', '2026-09-04T00:00:00Z', 'image', ?5, 'image/png')",
                rusqlite::params![
                    file_id,
                    if link_via_source_id { None } else { Some(res_id) },
                    blob.hash,
                    format!("{file_id}_sha"),
                    "ref.png",
                    image_bytes.len() as i64
                ],
            )
            .expect("insert files row");
        }

        let session_id = format!("sess_{res_id}");
        let emitter = Arc::new(ChatV2EventEmitter::new_windowless_for_test(
            session_id.clone(),
        ));
        let ctx = ExecutionContext::new(
            session_id.clone(),
            format!("msg_{res_id}"),
            format!("blk_{res_id}"),
            emitter,
            Arc::new(ToolRegistry::new()),
            None,
        )
        .with_vfs_db(Some(vfs_db));

        (temp_dir, ctx, expected_base64)
    }

    #[test]
    fn reads_res_context_ref_image_linked_by_files_resource_id() {
        let (_temp, ctx, expected_base64) = setup_res_image_ctx("res_img_a", "file_img_a", false);
        let context_ref = ContextRef::new("res_img_a", "res_img_a_hash", "image");

        let (name, mime, content) = read_context_ref_content(&ctx, &context_ref, true)
            .expect("res_* image (files.resource_id) must be readable");

        assert_eq!(name, "ref.png");
        assert_eq!(mime, "image/png");
        assert_eq!(content, expected_base64);
    }

    #[test]
    fn reads_res_context_ref_image_linked_by_resources_source_id() {
        let (_temp, ctx, expected_base64) = setup_res_image_ctx("res_img_b", "file_img_b", true);
        let context_ref = ContextRef::new("res_img_b", "res_img_b_hash", "image");

        let (name, mime, content) = read_context_ref_content(&ctx, &context_ref, true)
            .expect("res_* image (resources.source_id → files.id) must be readable");

        assert_eq!(name, "ref.png");
        assert_eq!(mime, "image/png");
        assert_eq!(content, expected_base64);
    }

    #[test]
    fn unresolvable_res_context_ref_reports_not_found() {
        let (_temp, ctx, _expected) = setup_res_image_ctx("res_img_c", "file_img_c", false);
        let context_ref = ContextRef::new("res_missing", "res_missing_hash", "image");

        let error = read_context_ref_content(&ctx, &context_ref, true)
            .expect_err("missing res_* must fail");
        assert!(
            error.contains("Resource not found in VFS"),
            "unexpected error: {error}"
        );
    }
}
