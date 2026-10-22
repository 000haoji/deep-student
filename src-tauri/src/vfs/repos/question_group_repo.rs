//! VFS 题组（Question Group）与材料（Material）Repo —— Issue #491
//!
//! ## 模型
//! - `question_groups`: 一层平级题组（阅读 / 共享条件 / 其他）；组头不是 question，
//!   不产生作答记录、不计题数。
//! - `question_group_materials`: 组内多个**有序**材料；正文/配图/来源各存一份，
//!   不在子题里重复存储。
//! - `questions.group_id/sub_label/group_order/group_version`: nullable 关系列。
//!   `parent_id` 仍是变式题语义，与本模型无关。
//!
//! ## 事务契约
//! 所有 `*_with_conn` 方法**不自行开启事务**：调用方（typed 组装 / 导入发布 /
//! 命令层）持有 `BEGIN IMMEDIATE ... COMMIT`，以保证组、材料、子题、统计与
//! 预览一次提交。`publish_group_bundle` 是唯一自开事务的便捷入口，内部复用
//! `publish_group_bundle_with_conn`。
//!
//! ## 原子性
//! 发布顺序：组（staging）→ 材料 → 子题（关系随 INSERT 一次写入）→ 校验 →
//! 组置 complete。禁止“先发布 question 再回写 group_id”的写法。
//!
//! ## 可见性（RowSync 安全）
//! 仅 `publish_state='complete'` 不够：组行可能先于成员到达。统一谓词
//! [`visible_group_sql`] 要求 manifest 中所有 child/material id 已落地、存活、
//! `group_version` 与组 `version` 一致，且计数匹配。所有读取路径必须使用它。
//!
//! ## CAS
//! 编辑命令强制 `expected_version`（组版本，单调递增）。材料没有独立版本，
//! 任何材料/关系变更都在同一事务内 `group.version += 1` 并重新盖章成员
//! `group_version`。

use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Deserializer, Serialize};
use tracing::{debug, info, warn};

use crate::vfs::database::VfsDatabase;
use crate::vfs::error::{VfsError, VfsResult};
use crate::vfs::repos::question_repo::{
    CreateQuestionParams, Question, QuestionImage, QuestionOption, QuestionStatus, QuestionType,
    SourceType,
};

// ============================================================================
// 枚举
// ============================================================================

/// 组类型（首版固定三种，不做任意嵌套）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuestionGroupType {
    /// 阅读/完形等以材料为主体的题组
    Reading,
    /// 共享题干条件（数学 (1)(2)(3)）
    SharedCondition,
    #[default]
    Other,
}

impl QuestionGroupType {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuestionGroupType::Reading => "reading",
            QuestionGroupType::SharedCondition => "shared_condition",
            QuestionGroupType::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "reading" => QuestionGroupType::Reading,
            "shared_condition" => QuestionGroupType::SharedCondition,
            _ => QuestionGroupType::Other,
        }
    }
}

/// 发布状态：staging 对 UI 完全不可见
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GroupPublishState {
    Staging,
    #[default]
    Complete,
}

impl GroupPublishState {
    pub fn as_str(&self) -> &'static str {
        match self {
            GroupPublishState::Staging => "staging",
            GroupPublishState::Complete => "complete",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "staging" => GroupPublishState::Staging,
            _ => GroupPublishState::Complete,
        }
    }
}

/// 材料内容类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MaterialContentType {
    #[default]
    Text,
    Formula,
    /// 配图（图片引用在 `images` 中）
    Figure,
    Mixed,
}

impl MaterialContentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MaterialContentType::Text => "text",
            MaterialContentType::Formula => "formula",
            MaterialContentType::Figure => "figure",
            MaterialContentType::Mixed => "mixed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "formula" => MaterialContentType::Formula,
            "figure" | "image" => MaterialContentType::Figure,
            "mixed" => MaterialContentType::Mixed,
            _ => MaterialContentType::Text,
        }
    }
}

// ============================================================================
// 数据类型
// ============================================================================

/// 题组
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionGroup {
    pub id: String,
    pub exam_id: String,
    pub group_type: QuestionGroupType,
    pub title: Option<String>,
    pub source_type: SourceType,
    pub source_ref: Option<String>,
    /// 字段级导入基线（不对外承诺稳定 schema，仅导入器使用）
    #[serde(default)]
    pub import_baseline: Option<serde_json::Value>,
    /// 关系/材料变更版本（CAS 基线，单调递增）
    pub version: i64,
    pub publish_state: GroupPublishState,
    pub expected_child_ids: Vec<String>,
    pub expected_material_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 组材料
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionGroupMaterial {
    pub id: String,
    pub group_id: String,
    pub exam_id: String,
    /// 组内顺序，从 1 开始连续
    pub order_index: i32,
    pub content_type: MaterialContentType,
    pub title: Option<String>,
    pub text: String,
    pub images: Vec<QuestionImage>,
    pub source_ref: Option<String>,
    pub page_index: Option<i32>,
    pub bbox: Option<[f64; 4]>,
    /// 写入时的组版本（可见性校验用）
    pub group_version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

/// 同组兄弟题简介：**必须**包含必要题干与选项（数学 (2) 需要 (1) 的条件），
/// 但绝不包含答案/解析/结构化答案/作答结果，避免作答前泄露。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionSiblingBrief {
    pub id: String,
    pub sub_label: Option<String>,
    pub question_label: Option<String>,
    pub group_order: Option<i32>,
    pub question_type: QuestionType,
    pub status: QuestionStatus,
    pub content: String,
    #[serde(default)]
    pub options: Option<Vec<QuestionOption>>,
}

/// 组进度（按子题聚合）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupProgress {
    pub total: i64,
    /// 新题数量（前端合同字段名：new）
    pub new: i64,
    pub in_progress: i64,
    pub mastered: i64,
    pub review: i64,
}

/// 单题上下文：所属组 + 有序材料 + 兄弟题简介
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionGroupContext {
    pub group: QuestionGroup,
    pub materials: Vec<QuestionGroupMaterial>,
    pub siblings: Vec<QuestionSiblingBrief>,
    pub sibling_count: i64,
    pub group_progress: GroupProgress,
}

/// 组详情（组字段平铺 + 材料 + 子题简介）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionGroupBundle {
    #[serde(flatten)]
    pub group: QuestionGroup,
    pub materials: Vec<QuestionGroupMaterial>,
    pub siblings: Vec<QuestionSiblingBrief>,
}

/// 组列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionGroupListResult {
    pub groups: Vec<QuestionGroup>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub has_more: bool,
}

/// 新建组参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewQuestionGroup {
    pub exam_id: String,
    #[serde(default)]
    pub group_type: QuestionGroupType,
    pub title: Option<String>,
    #[serde(default)]
    pub source_type: Option<SourceType>,
    pub source_ref: Option<String>,
    #[serde(default)]
    pub import_baseline: Option<serde_json::Value>,
    /// 空组默认就是 complete；导入发布路径请使用 `publish_group_bundle`（内部 staging）
    #[serde(default)]
    pub publish_state: Option<GroupPublishState>,
}

/// 组更新（None = 不修改）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GroupUpdate {
    pub title: Option<String>,
    pub group_type: Option<QuestionGroupType>,
    pub source_ref: Option<String>,
    pub import_baseline: Option<serde_json::Value>,
}

/// 前端把可选文本传 `null` 时按空串处理（避免 String 反序列化失败）。
fn deserialize_string_or_null<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// 前端把 `images` 传 `null` 时按空数组处理。
fn deserialize_vec_or_null<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

/// bbox 容错：接受 null / [x0,y0,x1,y1] / {x0,y0,x1,y1}。
fn parse_bbox_value(value: &serde_json::Value) -> Option<[f64; 4]> {
    if let Some(array) = value.as_array() {
        if array.len() == 4 {
            let numbers: Option<Vec<f64>> = array.iter().map(|item| item.as_f64()).collect();
            if let Some(numbers) = numbers {
                return Some([numbers[0], numbers[1], numbers[2], numbers[3]]);
            }
        }
        return None;
    }
    let object = value.as_object()?;
    let read = |key: &str| object.get(key).and_then(|item| item.as_f64());
    match (read("x0"), read("y0"), read("x1"), read("y1")) {
        (Some(x0), Some(y0), Some(x1), Some(y1)) => Some([x0, y0, x1, y1]),
        _ => None,
    }
}

fn deserialize_bbox_or_null<'de, D>(deserializer: D) -> Result<Option<[f64; 4]>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.as_ref().and_then(parse_bbox_value))
}

/// 新建材料参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewMaterial {
    pub group_id: String,
    pub order_index: i32,
    #[serde(default)]
    pub content_type: MaterialContentType,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "deserialize_string_or_null")]
    pub text: String,
    #[serde(default, deserialize_with = "deserialize_vec_or_null")]
    pub images: Vec<QuestionImage>,
    pub source_ref: Option<String>,
    pub page_index: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_bbox_or_null")]
    pub bbox: Option<[f64; 4]>,
}

/// 材料更新（None = 不修改）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MaterialUpdate {
    pub order_index: Option<i32>,
    pub content_type: Option<MaterialContentType>,
    pub title: Option<String>,
    pub text: Option<String>,
    pub images: Option<Vec<QuestionImage>>,
    pub source_ref: Option<String>,
    pub page_index: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_bbox_or_null")]
    pub bbox: Option<[f64; 4]>,
}

/// 待发布的子题（关系随 INSERT 一次写入）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedChildQuestion {
    /// 调用方内引用（仅用于回执映射，不落库）
    #[serde(default)]
    pub client_ref: Option<String>,
    #[serde(flatten)]
    pub question: CreateQuestionParams,
    pub sub_label: Option<String>,
    pub group_order: i32,
}

/// 原子发布载荷
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedGroupBundle {
    pub group: NewQuestionGroup,
    #[serde(default)]
    pub materials: Vec<NewMaterial>,
    #[serde(default)]
    pub children: Vec<ProposedChildQuestion>,
}

/// 材料全文命中：定位到具体材料与可见子题（搜索派生，不复制权威正文）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMaterialHit {
    pub question_id: String,
    pub material_id: String,
    pub group_id: String,
    pub sub_label: Option<String>,
    pub question_label: Option<String>,
    pub group_order: Option<i32>,
    /// 材料正文匹配片段（带 <mark> 高亮）
    pub snippet: String,
}

/// 删除预览
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupDeletePreview {
    pub group_id: String,
    pub version: i64,
    pub material_ids: Vec<String>,
    pub child_questions: Vec<QuestionSiblingBrief>,
    pub total_children: i64,
}

/// 整组软删结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftDeleteSummary {
    pub group_id: String,
    pub materials_deleted: usize,
    pub children_deleted: usize,
    pub new_version: i64,
}

// ============================================================================
// 可见性谓词
// ============================================================================

/// 生成"组完整可见"的 SQL 谓词（组别名需在外部 FROM 中）。
///
/// 判定条件（全部满足）：
/// 1. 组未软删且 `publish_state='complete'`
/// 2. manifest 中每个 child/material id 均已落地、存活
/// 3. 存活成员计数 == manifest 长度（防止 manifest 尚未更新的旧成员让计数虚假匹配）
/// 4. 每个成员的 `group_version` 与组 `version` 一致（旧内容齐全 ≠ 新版本完整）
pub fn visible_group_sql(alias: &str) -> String {
    format!(
        "({a}.deleted_at IS NULL
          AND {a}.publish_state = 'complete'
          AND EXISTS (SELECT 1 FROM exam_sheets es WHERE es.id = {a}.exam_id AND es.deleted_at IS NULL)
          AND json_array_length({a}.expected_child_ids_json) > 0
          AND (SELECT COUNT(*) FROM questions q WHERE q.group_id = {a}.id AND q.deleted_at IS NULL)
              = json_array_length({a}.expected_child_ids_json)
          AND (SELECT COUNT(*) FROM question_group_materials m WHERE m.group_id = {a}.id AND m.deleted_at IS NULL)
              = json_array_length({a}.expected_material_ids_json)
          AND NOT EXISTS (
              SELECT 1 FROM json_each({a}.expected_child_ids_json) e
              WHERE NOT EXISTS (
                  SELECT 1 FROM questions q
                  WHERE q.id = e.value AND q.deleted_at IS NULL AND q.group_version = {a}.version))
          AND NOT EXISTS (
              SELECT 1 FROM json_each({a}.expected_material_ids_json) e
              WHERE NOT EXISTS (
                  SELECT 1 FROM question_group_materials m
                  WHERE m.id = e.value AND m.deleted_at IS NULL AND m.group_version = {a}.version))
        )",
        a = alias
    )
}

/// 子题读取路径的可见性过滤（questions 别名 q）
pub(crate) fn visible_child_question_sql(alias: &str) -> String {
    format!(
        "({a}.group_id IS NULL OR EXISTS (
            SELECT 1 FROM question_groups g
            WHERE g.id = {a}.group_id AND {vis}
        ))",
        a = alias,
        vis = visible_group_sql("g")
    )
}

// ============================================================================
// Repo
// ============================================================================

pub struct VfsQuestionGroupRepo;

impl VfsQuestionGroupRepo {
    // ------------------------------------------------------------------
    // 基础工具
    // ------------------------------------------------------------------

    /// RFC3339 毫秒时间，并保证严格大于 `prev`（避免同一毫秒内 CAS 失效）
    fn next_timestamp(prev: Option<&str>) -> String {
        let now = chrono::Utc::now();
        let candidate = now.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        match prev {
            Some(previous) if candidate.as_str() <= previous => {
                if let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(previous) {
                    let bumped = parsed + chrono::Duration::milliseconds(1);
                    bumped
                        .with_timezone(&chrono::Utc)
                        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                        .to_string()
                } else {
                    candidate
                }
            }
            _ => candidate,
        }
    }

    fn json_array_or_empty(value: Option<&String>) -> Vec<String> {
        value
            .and_then(|raw| serde_json::from_str::<Vec<String>>(raw).ok())
            .unwrap_or_default()
    }

    fn invalid(reason: &str) -> VfsError {
        VfsError::InvalidArgument {
            param: "question_group".to_string(),
            reason: reason.to_string(),
        }
    }

    fn conflict(expected: i64, actual: i64) -> VfsError {
        VfsError::Other(format!(
            "QBANK_CONFLICT: expected_version={}, actual_version={}",
            expected, actual
        ))
    }

    // ------------------------------------------------------------------
    // 读取：组
    // ------------------------------------------------------------------

    pub fn get_group(db: &VfsDatabase, group_id: &str) -> VfsResult<Option<QuestionGroup>> {
        let conn = db.get_conn_safe()?;
        Self::get_group_with_conn(&conn, group_id)
    }

    /// 读取组（**不做可见性过滤**，内部编排用；对外请用 `get_visible_group_with_conn`）
    pub fn get_group_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Option<QuestionGroup>> {
        let row = conn
            .query_row(
                &format!(
                    "SELECT {} FROM question_groups WHERE id = ?1",
                    Self::group_columns()
                ),
                params![group_id],
                Self::row_to_group,
            )
            .optional()?;
        Ok(row)
    }

    /// 读取可见组（staging / 不完整 / 已软删 → None）
    pub fn get_visible_group_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Option<QuestionGroup>> {
        let sql = format!(
            "SELECT {} FROM question_groups g WHERE g.id = ?1 AND {}",
            Self::group_columns_with_alias("g"),
            visible_group_sql("g")
        );
        let row = conn
            .query_row(&sql, params![group_id], Self::row_to_group)
            .optional()?;
        Ok(row)
    }

    pub fn list_groups_with_conn(
        conn: &Connection,
        exam_id: &str,
        page: u32,
        page_size: u32,
    ) -> VfsResult<QuestionGroupListResult> {
        let page = page.max(1);
        let page_size = page_size.clamp(1, 100);
        let offset = (page.saturating_sub(1)) * page_size;

        let count_sql = format!(
            "SELECT COUNT(*) FROM question_groups g WHERE g.exam_id = ?1 AND {}",
            visible_group_sql("g")
        );
        let total: i64 = conn.query_row(&count_sql, params![exam_id], |row| row.get(0))?;

        let sql = format!(
            "SELECT {} FROM question_groups g
             WHERE g.exam_id = ?1 AND {}
             ORDER BY g.created_at ASC, g.id ASC
             LIMIT ?2 OFFSET ?3",
            Self::group_columns_with_alias("g"),
            visible_group_sql("g")
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![exam_id, page_size, offset], Self::row_to_group)?;
        let groups: Vec<QuestionGroup> = rows.filter_map(Self::log_and_skip).collect();

        Ok(QuestionGroupListResult {
            groups,
            total,
            page,
            page_size,
            has_more: (offset as i64 + page_size as i64) < total,
        })
    }

    // ------------------------------------------------------------------
    // 读取：材料
    // ------------------------------------------------------------------

    pub fn get_materials_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Vec<QuestionGroupMaterial>> {
        let sql = format!(
            "SELECT {} FROM question_group_materials
             WHERE group_id = ?1 AND deleted_at IS NULL
             ORDER BY order_index ASC, id ASC",
            Self::material_columns()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![group_id], Self::row_to_material)?;
        Ok(rows.filter_map(Self::log_and_skip).collect())
    }

    pub fn get_material_with_conn(
        conn: &Connection,
        material_id: &str,
    ) -> VfsResult<Option<QuestionGroupMaterial>> {
        let sql = format!(
            "SELECT {} FROM question_group_materials WHERE id = ?1",
            Self::material_columns()
        );
        conn.query_row(&sql, params![material_id], Self::row_to_material)
            .optional()
            .map_err(Into::into)
    }

    // ------------------------------------------------------------------
    // 读取：子题简介
    // ------------------------------------------------------------------

    pub fn get_siblings_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Vec<QuestionSiblingBrief>> {
        let mut stmt = conn.prepare(
            "SELECT id, sub_label, question_label, group_order, question_type, status, content, options_json
             FROM questions
             WHERE group_id = ?1 AND deleted_at IS NULL
             ORDER BY group_order ASC, id ASC",
        )?;
        let rows = stmt.query_map(params![group_id], |row| {
            let type_str: Option<String> = row.get(4)?;
            let status_str: Option<String> = row.get(5)?;
            let options_json: Option<String> = row.get(7)?;
            Ok(QuestionSiblingBrief {
                id: row.get(0)?,
                sub_label: row.get(1)?,
                question_label: row.get(2)?,
                group_order: row.get(3)?,
                question_type: type_str
                    .map(|s| QuestionType::from_str(&s))
                    .unwrap_or_default(),
                status: status_str
                    .map(|s| QuestionStatus::from_str(&s))
                    .unwrap_or_default(),
                content: row.get(6)?,
                options: options_json.and_then(|raw| serde_json::from_str(&raw).ok()),
            })
        })?;
        Ok(rows.filter_map(Self::log_and_skip).collect())
    }

    fn log_and_skip<T>(result: rusqlite::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                warn!(
                    "[VFS::QuestionGroupRepo] row parse error (skipped): {}",
                    error
                );
                None
            }
        }
    }

    // ------------------------------------------------------------------
    // 读取：组详情 / 单题上下文
    // ------------------------------------------------------------------

    pub fn get_bundle_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Option<QuestionGroupBundle>> {
        let Some(group) = Self::get_visible_group_with_conn(conn, group_id)? else {
            return Ok(None);
        };
        let materials = Self::get_materials_with_conn(conn, group_id)?;
        let siblings = Self::get_siblings_with_conn(conn, group_id)?;
        Ok(Some(QuestionGroupBundle {
            group,
            materials,
            siblings,
        }))
    }

    pub fn get_bundle(db: &VfsDatabase, group_id: &str) -> VfsResult<Option<QuestionGroupBundle>> {
        let conn = db.get_conn_safe()?;
        Self::get_bundle_with_conn(&conn, group_id)
    }

    /// 计算单题的组上下文；组不可见时返回 None（绝不泄露半组数据）
    pub fn get_context_for_question_with_conn(
        conn: &Connection,
        question_id: &str,
    ) -> VfsResult<Option<QuestionGroupContext>> {
        let group_id: Option<String> = conn
            .query_row(
                "SELECT group_id FROM questions WHERE id = ?1 AND deleted_at IS NULL",
                params![question_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let Some(group_id) = group_id else {
            return Ok(None);
        };
        Self::build_context_with_conn(conn, &group_id)
    }

    fn build_context_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Option<QuestionGroupContext>> {
        let Some(group) = Self::get_visible_group_with_conn(conn, group_id)? else {
            return Ok(None);
        };
        let materials = Self::get_materials_with_conn(conn, group_id)?;
        let siblings = Self::get_siblings_with_conn(conn, group_id)?;
        let sibling_count = siblings.len() as i64;
        let progress = Self::group_progress_with_conn(conn, group_id)?;
        Ok(Some(QuestionGroupContext {
            group,
            materials,
            siblings,
            sibling_count,
            group_progress: progress,
        }))
    }

    pub fn group_progress_with_conn(conn: &Connection, group_id: &str) -> VfsResult<GroupProgress> {
        let mut progress = GroupProgress::default();
        let mut stmt = conn.prepare(
            "SELECT status, COUNT(*) FROM questions
             WHERE group_id = ?1 AND deleted_at IS NULL GROUP BY status",
        )?;
        let rows = stmt.query_map(params![group_id], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (status, count) = row?;
            progress.total += count;
            match status.as_deref().unwrap_or("new") {
                "mastered" => progress.mastered += count,
                "in_progress" => progress.in_progress += count,
                "review" => progress.review += count,
                _ => progress.new += count,
            }
        }
        Ok(progress)
    }

    /// **通用注入点**：为一批单题结果补 `group_context`。
    ///
    /// 所有单题序列化路径（列表/详情/搜索/错题/随机/AI 工具）都应调用本方法，
    /// 避免“页面恰好带了材料，其他入口拿不到”的缺陷。
    pub fn attach_group_contexts_with_conn(
        conn: &Connection,
        questions: &mut [Question],
    ) -> VfsResult<()> {
        let group_ids: Vec<String> = {
            let mut ids: Vec<String> = questions
                .iter()
                .filter_map(|q| q.group_id.clone())
                .collect();
            ids.sort();
            ids.dedup();
            ids
        };
        if group_ids.is_empty() {
            return Ok(());
        }

        let mut contexts: HashMap<String, QuestionGroupContext> = HashMap::new();
        for group_id in &group_ids {
            if let Some(context) = Self::build_context_with_conn(conn, group_id)? {
                contexts.insert(group_id.clone(), context);
            }
        }
        for question in questions.iter_mut() {
            if let Some(group_id) = &question.group_id {
                question.group_context = contexts.get(group_id).cloned();
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // 写入：组
    // ------------------------------------------------------------------

    pub fn insert_group_with_conn(
        conn: &Connection,
        params: &NewQuestionGroup,
    ) -> VfsResult<QuestionGroup> {
        let id = format!("qg_{}", nanoid::nanoid!(10));
        let now = Self::next_timestamp(None);
        let source_type = params
            .source_type
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("ocr");
        // 直接 insert 的组默认 staging：没有子题的组不应出现在 UI。
        // 需要可见的正式组必须走 `publish_group_bundle(_with_conn)`。
        let publish_state = params
            .publish_state
            .clone()
            .unwrap_or(GroupPublishState::Staging);
        let baseline = params
            .import_baseline
            .as_ref()
            .filter(|v| !v.is_null())
            .map(|v| v.to_string());

        conn.execute(
            r#"INSERT INTO question_groups(
                   id, exam_id, group_type, title, source_type, source_ref,
                   import_baseline_json, version, publish_state,
                   expected_child_ids_json, expected_material_ids_json,
                   created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, '[]', '[]', ?9, ?9)"#,
            params![
                id,
                params.exam_id,
                params.group_type.as_str(),
                params.title,
                source_type,
                params.source_ref,
                baseline,
                publish_state.as_str(),
                now,
            ],
        )
        .map_err(|e| {
            if e.to_string().contains("QBANK_GROUP_EXAM_MISSING") {
                VfsError::NotFound {
                    resource_type: "exam_sheet".to_string(),
                    id: params.exam_id.clone(),
                }
            } else {
                VfsError::from(e)
            }
        })?;

        info!("[VFS::QuestionGroupRepo] Created group id={}", id);
        Self::get_group_with_conn(conn, &id)?.ok_or_else(|| VfsError::NotFound {
            resource_type: "question_group".to_string(),
            id,
        })
    }

    /// 更新组元数据（CAS：`expected_version` 必填）
    pub fn update_group_with_conn(
        conn: &Connection,
        group_id: &str,
        update: &GroupUpdate,
        expected_version: i64,
    ) -> VfsResult<QuestionGroup> {
        let current =
            Self::get_group_with_conn(conn, group_id)?.ok_or_else(|| VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            })?;
        if current.deleted_at.is_some() {
            return Err(VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            });
        }
        if current.version != expected_version {
            return Err(Self::conflict(expected_version, current.version));
        }

        let mut set_clauses = vec![
            "updated_at = ?1".to_string(),
            "version = version + 1".to_string(),
        ];
        let now = Self::next_timestamp(Some(&current.updated_at));
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut idx = 2;
        if let Some(title) = &update.title {
            set_clauses.push(format!("title = ?{}", idx));
            values.push(Box::new(title.clone()));
            idx += 1;
        }
        if let Some(group_type) = &update.group_type {
            set_clauses.push(format!("group_type = ?{}", idx));
            values.push(Box::new(group_type.as_str().to_string()));
            idx += 1;
        }
        if let Some(source_ref) = &update.source_ref {
            set_clauses.push(format!("source_ref = ?{}", idx));
            values.push(Box::new(source_ref.clone()));
            idx += 1;
        }
        if let Some(baseline) = &update.import_baseline {
            set_clauses.push(format!("import_baseline_json = ?{}", idx));
            values.push(Box::new(if baseline.is_null() {
                None
            } else {
                Some(baseline.to_string())
            }));
            idx += 1;
        }

        let version_param = idx;
        values.push(Box::new(expected_version));
        idx += 1;
        let id_param = idx;
        values.push(Box::new(group_id.to_string()));

        // CAS 条件写在 UPDATE 本身，避免 SELECT 与 UPDATE 之间的竞争窗口。
        let sql = format!(
            "UPDATE question_groups SET {} WHERE id = ?{} AND deleted_at IS NULL AND version = ?{}",
            set_clauses.join(", "),
            id_param,
            version_param
        );
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let affected = conn.execute(&sql, refs.as_slice())?;
        if affected == 0 {
            let actual = Self::get_group_with_conn(conn, group_id)?
                .map(|g| g.version)
                .ok_or_else(|| VfsError::NotFound {
                    resource_type: "question_group".to_string(),
                    id: group_id.to_string(),
                })?;
            return Err(Self::conflict(expected_version, actual));
        }

        let new_version: i64 = conn.query_row(
            "SELECT version FROM question_groups WHERE id = ?1",
            params![group_id],
            |row| row.get(0),
        )?;
        Self::stamp_members_with_conn(conn, group_id, new_version)?;

        debug!(
            "[VFS::QuestionGroupRepo] Updated group id={} version {}",
            group_id, new_version
        );
        Self::get_group_with_conn(conn, group_id)?.ok_or_else(|| VfsError::NotFound {
            resource_type: "question_group".to_string(),
            id: group_id.to_string(),
        })
    }

    /// 将组版本印章同步到所有存活成员（可见性要求成员版本 == 组版本）
    fn stamp_members_with_conn(conn: &Connection, group_id: &str, version: i64) -> VfsResult<()> {
        conn.execute(
            "UPDATE questions SET group_version = ?2
             WHERE group_id = ?1 AND deleted_at IS NULL AND group_version <> ?2",
            params![group_id, version],
        )?;
        conn.execute(
            "UPDATE question_group_materials SET group_version = ?2
             WHERE group_id = ?1 AND deleted_at IS NULL AND group_version <> ?2",
            params![group_id, version],
        )?;
        Ok(())
    }

    /// 重新计算并写入 manifest（保存后用于自愈）
    fn refresh_manifest_with_conn(conn: &Connection, group_id: &str) -> VfsResult<()> {
        let children: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM questions WHERE group_id = ?1 AND deleted_at IS NULL ORDER BY group_order, id",
            )?;
            let rows = stmt.query_map(params![group_id], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let materials: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM question_group_materials WHERE group_id = ?1 AND deleted_at IS NULL ORDER BY order_index, id",
            )?;
            let rows = stmt.query_map(params![group_id], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        conn.execute(
            "UPDATE question_groups SET expected_child_ids_json = ?2, expected_material_ids_json = ?3 WHERE id = ?1",
            params![
                group_id,
                serde_json::to_string(&children).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&materials).unwrap_or_else(|_| "[]".to_string())
            ],
        )?;
        Ok(())
    }

    // ------------------------------------------------------------------
    // 写入：材料
    // ------------------------------------------------------------------

    pub fn insert_material_with_conn(
        conn: &Connection,
        params: &NewMaterial,
    ) -> VfsResult<QuestionGroupMaterial> {
        Self::insert_material_internal(conn, params, true)
    }

    /// 内部插入：`bump=false` 仅用于原子发布（版本在全组成员写完后再一次确定）。
    fn insert_material_internal(
        conn: &Connection,
        params: &NewMaterial,
        bump_group_version: bool,
    ) -> VfsResult<QuestionGroupMaterial> {
        let group = Self::get_group_with_conn(conn, &params.group_id)?.ok_or_else(|| {
            VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: params.group_id.clone(),
            }
        })?;
        if group.deleted_at.is_some() {
            return Err(VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: params.group_id.clone(),
            });
        }
        if params.order_index <= 0 {
            return Err(Self::invalid("order_index 必须从 1 开始"));
        }

        let id = format!("qm_{}", nanoid::nanoid!(10));
        let now = Self::next_timestamp(Some(&group.updated_at));
        let images_json =
            serde_json::to_string(&params.images).unwrap_or_else(|_| "[]".to_string());
        let bbox_json = params
            .bbox
            .map(|bbox| serde_json::to_string(&bbox).unwrap_or_else(|_| "null".to_string()));

        conn.execute(
            r#"INSERT INTO question_group_materials(
                   id, group_id, exam_id, order_index, content_type, title, text,
                   images_json, source_ref, page_index, bbox_json, group_version,
                   created_at, updated_at)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?13)"#,
            params![
                id,
                group.id,
                group.exam_id,
                params.order_index,
                params.content_type.as_str(),
                params.title,
                params.text,
                images_json,
                params.source_ref,
                params.page_index,
                bbox_json,
                group.version,
                now,
            ],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(_, Some(message))
                if message.contains("QBANK_GROUP_PARENT_MISSING") =>
            {
                VfsError::NotFound {
                    resource_type: "question_group".to_string(),
                    id: params.group_id.clone(),
                }
            }
            other => VfsError::from(other),
        })?;

        if bump_group_version {
            // 关系变化：组版本前移并重新盖章 + 刷新 manifest + 同事务重索引入队
            Self::bump_group_relations_with_conn(conn, &group.id)?;
        }

        Self::get_material_with_conn(conn, &id)?.ok_or_else(|| VfsError::NotFound {
            resource_type: "question_group_material".to_string(),
            id,
        })
    }

    /// 更新材料（CAS：`expected_version` 为**组版本**，材料无独立版本）
    pub fn update_material_with_conn(
        conn: &Connection,
        material_id: &str,
        update: &MaterialUpdate,
        expected_version: i64,
    ) -> VfsResult<QuestionGroupMaterial> {
        let material =
            Self::get_material_with_conn(conn, material_id)?.ok_or_else(|| VfsError::NotFound {
                resource_type: "question_group_material".to_string(),
                id: material_id.to_string(),
            })?;
        if material.deleted_at.is_some() {
            return Err(VfsError::NotFound {
                resource_type: "question_group_material".to_string(),
                id: material_id.to_string(),
            });
        }
        let group = Self::get_group_with_conn(conn, &material.group_id)?.ok_or_else(|| {
            VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: material.group_id.clone(),
            }
        })?;
        if group.version != expected_version {
            return Err(Self::conflict(expected_version, group.version));
        }
        if let Some(order_index) = update.order_index {
            if order_index <= 0 {
                return Err(Self::invalid("order_index 必须从 1 开始"));
            }
        }

        let mut set_clauses = vec!["updated_at = ?1".to_string()];
        let now = Self::next_timestamp(Some(&material.updated_at));
        let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now)];
        let mut idx = 2;
        if let Some(order_index) = update.order_index {
            set_clauses.push(format!("order_index = ?{}", idx));
            values.push(Box::new(order_index));
            idx += 1;
        }
        if let Some(content_type) = &update.content_type {
            set_clauses.push(format!("content_type = ?{}", idx));
            values.push(Box::new(content_type.as_str().to_string()));
            idx += 1;
        }
        if let Some(title) = &update.title {
            set_clauses.push(format!("title = ?{}", idx));
            values.push(Box::new(title.clone()));
            idx += 1;
        }
        if let Some(text) = &update.text {
            set_clauses.push(format!("text = ?{}", idx));
            values.push(Box::new(text.clone()));
            idx += 1;
        }
        if let Some(images) = &update.images {
            set_clauses.push(format!("images_json = ?{}", idx));
            values.push(Box::new(
                serde_json::to_string(images).unwrap_or_else(|_| "[]".to_string()),
            ));
            idx += 1;
        }
        if let Some(source_ref) = &update.source_ref {
            set_clauses.push(format!("source_ref = ?{}", idx));
            values.push(Box::new(source_ref.clone()));
            idx += 1;
        }
        if let Some(page_index) = update.page_index {
            set_clauses.push(format!("page_index = ?{}", idx));
            values.push(Box::new(page_index));
            idx += 1;
        }
        if let Some(bbox) = &update.bbox {
            set_clauses.push(format!("bbox_json = ?{}", idx));
            values.push(Box::new(serde_json::to_string(bbox).ok()));
            idx += 1;
        }

        let id_param = idx;
        values.push(Box::new(material_id.to_string()));

        // CAS 条件写在 UPDATE 本身（组版本），避免 check-then-act 竞争窗口。
        let version_param = id_param + 1;
        values.push(Box::new(group.version));
        let sql = format!(
            "UPDATE question_group_materials SET {} WHERE id = ?{} AND deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM question_groups g
                           WHERE g.id = question_group_materials.group_id
                             AND g.deleted_at IS NULL AND g.version = ?{})",
            set_clauses.join(", "),
            id_param,
            version_param
        );
        let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|v| v.as_ref()).collect();
        let affected = conn.execute(&sql, refs.as_slice())?;
        if affected == 0 {
            let actual = Self::get_group_with_conn(conn, &material.group_id)?
                .map(|g| g.version)
                .unwrap_or(expected_version);
            return Err(Self::conflict(expected_version, actual));
        }

        Self::bump_group_relations_with_conn(conn, &group.id)?;

        Self::get_material_with_conn(conn, material_id)?.ok_or_else(|| VfsError::NotFound {
            resource_type: "question_group_material".to_string(),
            id: material_id.to_string(),
        })
    }

    /// 删除单个材料：组内仍有子题时拒绝（避免阅读组被删成无材料）
    ///
    /// 用户应先替换材料或显式删除整组（`delete_group_with_conn`）。
    pub fn delete_material_with_conn(
        conn: &Connection,
        material_id: &str,
        expected_version: i64,
    ) -> VfsResult<()> {
        let material =
            Self::get_material_with_conn(conn, material_id)?.ok_or_else(|| VfsError::NotFound {
                resource_type: "question_group_material".to_string(),
                id: material_id.to_string(),
            })?;
        let live_children: i64 = conn.query_row(
            "SELECT COUNT(*) FROM questions WHERE group_id = ?1 AND deleted_at IS NULL",
            params![material.group_id],
            |row| row.get(0),
        )?;
        let live_materials: i64 = conn.query_row(
            "SELECT COUNT(*) FROM question_group_materials WHERE group_id = ?1 AND deleted_at IS NULL",
            params![material.group_id],
            |row| row.get(0),
        )?;
        if live_children > 0 && live_materials <= 1 {
            return Err(VfsError::Other(format!(
                "QBANK_GROUP_MATERIAL_IN_USE: group {} still has {} live child question(s)",
                material.group_id, live_children
            )));
        }
        let group = Self::get_group_with_conn(conn, &material.group_id)?.ok_or_else(|| {
            VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: material.group_id.clone(),
            }
        })?;
        if group.version != expected_version {
            return Err(Self::conflict(expected_version, group.version));
        }

        let now = Self::next_timestamp(Some(&material.updated_at));
        conn.execute(
            "UPDATE question_group_materials SET deleted_at = ?2, updated_at = ?2 WHERE id = ?1",
            params![material_id, now],
        )?;
        Self::bump_group_relations_with_conn(conn, &material.group_id)?;
        Ok(())
    }

    /// 子题被单删/软删后同步组关系（manifest、版本、剩余成员印章、重索引）。
    ///
    /// 调用方必须与子题删除处于**同一事务**。若删除后组内已无存活子题，组保持
    /// complete 但 manifest 为空 → 可见性谓词会将其隐藏（避免空壳组出现在 UI）。
    pub fn remove_child_from_group_with_conn(
        conn: &Connection,
        group_id: &str,
        question_id: &str,
    ) -> VfsResult<()> {
        let Some(group) = Self::get_group_with_conn(conn, group_id)? else {
            return Ok(());
        };
        if group.deleted_at.is_some() {
            return Ok(());
        }
        let remaining: Vec<String> = {
            let mut stmt = conn.prepare(
                "SELECT id FROM questions WHERE group_id = ?1 AND deleted_at IS NULL ORDER BY group_order, id",
            )?;
            let rows = stmt.query_map(params![group_id], |row| row.get::<_, String>(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let new_version = group.version + 1;
        let now = Self::next_timestamp(Some(&group.updated_at));
        let affected = conn.execute(
            "UPDATE question_groups SET version = ?2, updated_at = ?3,
                    expected_child_ids_json = ?4
             WHERE id = ?1 AND deleted_at IS NULL AND version = ?5",
            params![
                group_id,
                new_version,
                now,
                serde_json::to_string(&remaining).unwrap_or_else(|_| "[]".to_string()),
                group.version
            ],
        )?;
        if affected == 0 {
            let actual = Self::get_group_with_conn(conn, group_id)?
                .map(|g| g.version)
                .unwrap_or(group.version);
            return Err(Self::conflict(group.version, actual));
        }
        Self::stamp_members_with_conn(conn, group_id, new_version)?;
        Self::enqueue_group_reindex_with_conn(conn, group_id)?;
        debug!(
            "[VFS::QuestionGroupRepo] child {} removed from group {}; {} remaining, version {}",
            question_id,
            group_id,
            remaining.len(),
            new_version
        );
        Ok(())
    }

    // ------------------------------------------------------------------
    // Tombstones（JSON 导出/同步回放使用）
    // ------------------------------------------------------------------

    /// 列出已软删的组（tombstones）
    pub fn list_group_tombstones_with_conn(
        conn: &Connection,
        exam_id: &str,
    ) -> VfsResult<Vec<QuestionGroup>> {
        let sql = format!(
            "SELECT {} FROM question_groups WHERE exam_id = ?1 AND deleted_at IS NOT NULL ORDER BY deleted_at DESC",
            Self::group_columns()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![exam_id], Self::row_to_group)?;
        Ok(rows.filter_map(Self::log_and_skip).collect())
    }

    /// 列出已软删的材料（tombstones）
    pub fn list_material_tombstones_with_conn(
        conn: &Connection,
        exam_id: &str,
    ) -> VfsResult<Vec<QuestionGroupMaterial>> {
        let sql = format!(
            "SELECT {} FROM question_group_materials WHERE exam_id = ?1 AND deleted_at IS NOT NULL ORDER BY deleted_at DESC",
            Self::material_columns()
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![exam_id], Self::row_to_material)?;
        Ok(rows.filter_map(Self::log_and_skip).collect())
    }

    /// 列出已软删的子题 ID（tombstones；与独立题共用 questions 表）
    pub fn list_child_tombstone_ids_with_conn(
        conn: &Connection,
        exam_id: &str,
    ) -> VfsResult<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT id FROM questions WHERE exam_id = ?1 AND group_id IS NOT NULL AND deleted_at IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![exam_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 组关系变更：版本 +1、重盖章成员、刷新 manifest
    fn bump_group_relations_with_conn(conn: &Connection, group_id: &str) -> VfsResult<i64> {
        let previous: Option<(String, i64)> = conn
            .query_row(
                "SELECT updated_at, version FROM question_groups WHERE id = ?1",
                params![group_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((updated_at, version)) = previous else {
            return Err(VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            });
        };
        let new_version = version + 1;
        let now = Self::next_timestamp(Some(&updated_at));
        let affected = conn.execute(
            "UPDATE question_groups SET version = ?2, updated_at = ?3
             WHERE id = ?1 AND deleted_at IS NULL AND version = ?4",
            params![group_id, new_version, now, version],
        )?;
        if affected == 0 {
            return Err(Self::conflict(version, version + 1));
        }
        Self::stamp_members_with_conn(conn, group_id, new_version)?;
        Self::refresh_manifest_with_conn(conn, group_id)?;
        Self::enqueue_group_reindex_with_conn(conn, group_id)?;
        Ok(new_version)
    }

    // ------------------------------------------------------------------
    // 删除：整组
    // ------------------------------------------------------------------

    pub fn preview_delete_group_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<GroupDeletePreview> {
        let group = Self::get_visible_group_with_conn(conn, group_id)?.ok_or_else(|| {
            VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            }
        })?;
        let materials = Self::get_materials_with_conn(conn, group_id)?;
        let siblings = Self::get_siblings_with_conn(conn, group_id)?;
        Ok(GroupDeletePreview {
            group_id: group.id,
            version: group.version,
            material_ids: materials.into_iter().map(|m| m.id).collect(),
            total_children: siblings.len() as i64,
            child_questions: siblings,
        })
    }

    /// 整组软删：组 + 材料 + 子题同一事务，CAS 校验组版本
    pub fn delete_group_with_conn(
        conn: &Connection,
        group_id: &str,
        expected_version: i64,
    ) -> VfsResult<SoftDeleteSummary> {
        let group =
            Self::get_group_with_conn(conn, group_id)?.ok_or_else(|| VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            })?;
        if group.deleted_at.is_some() {
            return Err(VfsError::NotFound {
                resource_type: "question_group".to_string(),
                id: group_id.to_string(),
            });
        }
        if group.version != expected_version {
            return Err(Self::conflict(expected_version, group.version));
        }

        let now = Self::next_timestamp(Some(&group.updated_at));
        // 先 CAS 命中再删成员：避免"成员已软删、组 CAS 失败"的半删状态。
        let affected = conn.execute(
            "UPDATE question_groups SET deleted_at = ?2, updated_at = ?2, version = version + 1
             WHERE id = ?1 AND deleted_at IS NULL AND version = ?3",
            params![group_id, now, expected_version],
        )?;
        if affected == 0 {
            let actual = Self::get_group_with_conn(conn, group_id)?
                .map(|g| g.version)
                .unwrap_or(expected_version);
            return Err(Self::conflict(expected_version, actual));
        }
        let children_deleted = conn.execute(
            "UPDATE questions SET deleted_at = ?2, updated_at = ?2
             WHERE group_id = ?1 AND deleted_at IS NULL",
            params![group_id, now],
        )?;
        let materials_deleted = conn.execute(
            "UPDATE question_group_materials SET deleted_at = ?2, updated_at = ?2
             WHERE group_id = ?1 AND deleted_at IS NULL",
            params![group_id, now],
        )?;

        info!(
            "[VFS::QuestionGroupRepo] Deleted group {} (children={}, materials={})",
            group_id, children_deleted, materials_deleted
        );
        Ok(SoftDeleteSummary {
            group_id: group_id.to_string(),
            materials_deleted,
            children_deleted,
            new_version: expected_version + 1,
        })
    }

    // ------------------------------------------------------------------
    // 原子发布
    // ------------------------------------------------------------------

    /// 原子发布整组（自开事务；唯一推荐的生产/导入入口）
    pub fn publish_group_bundle(
        db: &VfsDatabase,
        bundle: &ProposedGroupBundle,
    ) -> VfsResult<QuestionGroupBundle> {
        let conn = db.get_conn_safe()?;
        conn.execute("BEGIN IMMEDIATE", [])?;
        let result = Self::publish_group_bundle_with_conn(&conn, bundle);
        match result {
            Ok(value) => {
                conn.execute("COMMIT", [])?;
                Ok(value)
            }
            Err(error) => {
                let _ = conn.execute("ROLLBACK", []);
                warn!(
                    "[VFS::QuestionGroupRepo] publish_group_bundle rolled back: {}",
                    error
                );
                Err(error)
            }
        }
    }

    /// 原子发布（调用方持事务）。组/材料/子题一次写入，任一校验失败整组回滚。
    pub fn publish_group_bundle_with_conn(
        conn: &Connection,
        bundle: &ProposedGroupBundle,
    ) -> VfsResult<QuestionGroupBundle> {
        // ---- 1. 纯校验，先于任何写入 ----
        let material_count = bundle.materials.len();
        let child_count = bundle.children.len();
        if child_count == 0 {
            return Err(Self::invalid("题组至少需要一个可答子题（组头不计题）"));
        }
        let requires_material = matches!(
            bundle.group.group_type,
            QuestionGroupType::Reading | QuestionGroupType::SharedCondition
        );
        if requires_material && material_count == 0 {
            return Err(Self::invalid("阅读/共享条件题组至少需要一份材料"));
        }

        let mut material_orders: Vec<i32> =
            bundle.materials.iter().map(|m| m.order_index).collect();
        material_orders.sort_unstable();
        for (position, order) in material_orders.iter().enumerate() {
            if *order != (position as i32 + 1) {
                return Err(Self::invalid("材料 order_index 必须从 1 开始连续且唯一"));
            }
        }

        let mut child_orders: Vec<i32> = bundle.children.iter().map(|c| c.group_order).collect();
        child_orders.sort_unstable();
        for (position, order) in child_orders.iter().enumerate() {
            if *order != (position as i32 + 1) {
                return Err(Self::invalid("子题 group_order 必须从 1 开始连续且唯一"));
            }
        }

        let mut labels: Vec<&str> = bundle
            .children
            .iter()
            .filter_map(|c| c.sub_label.as_deref())
            .filter(|label| !label.trim().is_empty())
            .collect();
        let label_count_before = labels.len();
        labels.sort_unstable();
        labels.dedup();
        if labels.len() != label_count_before {
            return Err(Self::invalid("组内 sub_label 必须唯一"));
        }

        // ---- 2. 组（staging，先不可见）----
        let mut group_params = bundle.group.clone();
        group_params.publish_state = Some(GroupPublishState::Staging);
        let group = Self::insert_group_with_conn(conn, &group_params)?;

        // ---- 3. 材料 ----
        let mut material_ids: Vec<String> = Vec::with_capacity(material_count);
        for material in &bundle.materials {
            if material.group_id != group.id {
                // 允许调用方只给占位 group_id
                if !material.group_id.is_empty() && material.group_id != group.id {
                    return Err(Self::invalid(
                        "材料 group_id 与待发布组不一致（禁止跨组挂载）",
                    ));
                }
            }
            let mut item = material.clone();
            item.group_id = group.id.clone();
            let inserted = Self::insert_material_internal(conn, &item, false)?;
            material_ids.push(inserted.id);
        }

        // ---- 4. 子题（关系随 INSERT 一次写入，绝不回写）----
        // 材料插入会推进组版本，子题必须使用**当前**版本，否则可见性恒不成立。
        let current_version = Self::get_group_with_conn(conn, &group.id)?
            .map(|g| g.version)
            .unwrap_or(group.version);
        let mut child_ids: Vec<String> = Vec::with_capacity(child_count);
        for child in &bundle.children {
            if child.question.exam_id != group.exam_id {
                return Err(Self::invalid(
                    "子题 exam_id 与题组不一致（禁止跨题目集挂载）",
                ));
            }
            if child.group_order <= 0 {
                return Err(Self::invalid("子题 group_order 必须从 1 开始"));
            }
            if child
                .question
                .parent_id
                .as_deref()
                .map(|value| !value.is_empty())
                .unwrap_or(false)
            {
                return Err(Self::invalid(
                    "子题不得使用 parent_id 表达题组关系（parent_id 仅表示变式题）",
                ));
            }
            let id = Self::insert_child_question_with_conn(
                conn,
                &child.question,
                &group.id,
                child.sub_label.as_deref(),
                child.group_order,
                current_version,
            )?;
            child_ids.push(id);
        }

        // ---- 5. 校验完整性后置 complete ----
        let live_children: i64 = conn.query_row(
            "SELECT COUNT(*) FROM questions WHERE group_id = ?1 AND deleted_at IS NULL",
            params![group.id],
            |row| row.get(0),
        )?;
        let live_materials: i64 = conn.query_row(
            "SELECT COUNT(*) FROM question_group_materials WHERE group_id = ?1 AND deleted_at IS NULL",
            params![group.id],
            |row| row.get(0),
        )?;
        if live_children != child_ids.len() as i64 || live_materials != material_ids.len() as i64 {
            return Err(VfsError::Other(
                "QBANK_GROUP_PUBLISH_INCOMPLETE: relation write did not match proposal".to_string(),
            ));
        }

        conn.execute(
            "UPDATE question_groups SET expected_child_ids_json = ?2, expected_material_ids_json = ?3,
                    publish_state = 'complete', updated_at = ?4
             WHERE id = ?1",
            params![
                group.id,
                serde_json::to_string(&child_ids).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&material_ids).unwrap_or_else(|_| "[]".to_string()),
                Self::next_timestamp(Some(&group.updated_at)),
            ],
        )?;

        info!(
            "[VFS::QuestionGroupRepo] Published group {} ({} materials, {} children)",
            group.id,
            material_ids.len(),
            child_ids.len()
        );
        Self::enqueue_group_reindex_with_conn(conn, &group.id)?;

        Self::get_bundle_with_conn(conn, &group.id)?.ok_or_else(|| {
            VfsError::Other(
                "QBANK_GROUP_PUBLISH_INVISIBLE: published group failed visibility predicate"
                    .to_string(),
            )
        })
    }

    /// 直接插入带组关系的子题（关系列在 INSERT 中一次写入）
    fn insert_child_question_with_conn(
        conn: &Connection,
        params: &CreateQuestionParams,
        group_id: &str,
        sub_label: Option<&str>,
        group_order: i32,
        group_version: i64,
    ) -> VfsResult<String> {
        let id = format!("q_{}", nanoid::nanoid!(10));
        let now = Self::next_timestamp(None);
        let options_json = params
            .options
            .as_ref()
            .map(|opts| serde_json::to_string(opts).unwrap_or_default());
        let tags_json = serde_json::to_string(&params.tags.clone().unwrap_or_default())
            .unwrap_or_else(|_| "[]".to_string());
        let question_type = params
            .question_type
            .as_ref()
            .map(|t| t.as_str())
            .unwrap_or("other");
        let difficulty = params.difficulty.as_ref().map(|d| d.as_str());
        let source_type = params
            .source_type
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or("ocr");
        let images_json = params
            .images
            .as_ref()
            .map(|imgs| serde_json::to_string(imgs).unwrap_or_else(|_| "[]".to_string()))
            .unwrap_or_else(|| "[]".to_string());
        let structured_data_json = params
            .structured_data
            .as_ref()
            .filter(|v| !v.is_null())
            .map(|v| v.to_string());

        conn.execute(
            r#"INSERT INTO questions(
                   id, exam_id, card_id, question_label, content, options_json,
                   answer, explanation, question_type, difficulty, tags,
                   status, source_type, source_ref, images_json, parent_id,
                   created_at, updated_at, structured_data,
                   group_id, sub_label, group_order, group_version)
               VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'new', ?12, ?13, ?14, NULL,
                       ?15, ?15, ?16, ?17, ?18, ?19, ?20)"#,
            params![
                id,
                params.exam_id,
                params.card_id,
                params.question_label,
                params.content,
                options_json,
                params.answer,
                params.explanation,
                question_type,
                difficulty,
                tags_json,
                source_type,
                params.source_ref,
                images_json,
                now,
                structured_data_json,
                group_id,
                sub_label,
                group_order,
                group_version,
            ],
        )
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(_, Some(message))
                if message.contains("QBANK_GROUP_PARENT_MISSING") =>
            {
                VfsError::NotFound {
                    resource_type: "question_group".to_string(),
                    id: group_id.to_string(),
                }
            }
            other => VfsError::from(other),
        })?;
        Ok(id)
    }

    // ------------------------------------------------------------------
    // 搜索（材料命中 → 子题）
    // ------------------------------------------------------------------

    /// 材料全文命中对应的可见子题（返回 (question_id, material_id)）
    pub fn search_material_question_ids_with_conn(
        conn: &Connection,
        escaped_fts_query: &str,
        exam_id: Option<&str>,
        limit: usize,
    ) -> VfsResult<Vec<(String, String)>> {
        if !Self::fts_exists(conn) {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 500) as i64;
        let mut sql = format!(
            "SELECT q.id, m.id
             FROM question_group_materials_fts fts
             JOIN question_group_materials m ON m.rowid = fts.rowid
             JOIN questions q ON q.group_id = m.group_id AND q.deleted_at IS NULL
             JOIN question_groups g ON g.id = m.group_id
             WHERE question_group_materials_fts MATCH ?1
               AND m.deleted_at IS NULL
               AND {}",
            visible_group_sql("g")
        );
        if exam_id.is_some() {
            sql.push_str(" AND g.exam_id = ?3");
        }
        sql.push_str(
            " ORDER BY bm25(question_group_materials_fts) ASC, q.group_order ASC LIMIT ?2",
        );

        let mut stmt = conn.prepare(&sql)?;
        let rows = if let Some(exam_id) = exam_id {
            stmt.query_map(params![escaped_fts_query, limit, exam_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(Self::log_and_skip)
            .collect()
        } else {
            stmt.query_map(params![escaped_fts_query, limit], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .filter_map(Self::log_and_skip)
            .collect()
        };
        Ok(rows)
    }

    /// 材料搜索（派生）：返回材料命中片段 + 对应可见子题定位。
    ///
    /// 查题界面可直接展示；权威材料正文仍只在 `question_group_materials`。
    pub fn search_material_hits_with_conn(
        conn: &Connection,
        escaped_fts_query: &str,
        exam_id: Option<&str>,
        limit: usize,
    ) -> VfsResult<Vec<GroupMaterialHit>> {
        if !Self::fts_exists(conn) {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 200) as i64;
        let mut sql = format!(
            "SELECT q.id, m.id, m.group_id, q.sub_label, q.question_label, q.group_order,
                    snippet(question_group_materials_fts, 0, '<mark>', '</mark>', '…', 16)
             FROM question_group_materials_fts fts
             JOIN question_group_materials m ON m.rowid = fts.rowid
             JOIN questions q ON q.group_id = m.group_id AND q.deleted_at IS NULL
             JOIN question_groups g ON g.id = m.group_id
             WHERE question_group_materials_fts MATCH ?1
               AND m.deleted_at IS NULL
               AND {}",
            visible_group_sql("g")
        );
        if exam_id.is_some() {
            sql.push_str(" AND g.exam_id = ?3");
        }
        sql.push_str(
            " ORDER BY bm25(question_group_materials_fts) ASC, q.group_order ASC LIMIT ?2",
        );

        let mut stmt = conn.prepare(&sql)?;
        let map_row = |row: &Row| -> rusqlite::Result<GroupMaterialHit> {
            Ok(GroupMaterialHit {
                question_id: row.get(0)?,
                material_id: row.get(1)?,
                group_id: row.get(2)?,
                sub_label: row.get(3)?,
                question_label: row.get(4)?,
                group_order: row.get(5)?,
                snippet: row.get(6)?,
            })
        };
        let rows: Vec<GroupMaterialHit> = if let Some(exam_id) = exam_id {
            stmt.query_map(params![escaped_fts_query, limit, exam_id], map_row)?
                .filter_map(Self::log_and_skip)
                .collect()
        } else {
            stmt.query_map(params![escaped_fts_query, limit], map_row)?
                .filter_map(Self::log_and_skip)
                .collect()
        };
        Ok(rows)
    }

    fn fts_exists(conn: &Connection) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='question_group_materials_fts'",
            [],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// 在**同一事务内**把组内子题重新入索引待办（复用 vfs_index_units ledger，
    /// 不新建第二套索引机制）。材料正文参与子题索引文本，所以材料/顺序/关系
    /// 任一变更都要重索引。
    fn enqueue_group_reindex_with_conn(conn: &Connection, group_id: &str) -> VfsResult<()> {
        let has_units: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='vfs_index_units')",
            [],
            |row| row.get(0),
        )?;
        if !has_units {
            debug!(
                "[VFS::QuestionGroupRepo] vfs_index_units 不存在，跳过重索引入队 group={}",
                group_id
            );
            return Ok(());
        }
        let question_ids = Self::material_change_reindex_question_ids_with_conn(conn, group_id)?;
        if question_ids.is_empty() {
            return Ok(());
        }
        let now_ms = chrono::Utc::now().timestamp_millis();
        crate::vfs::question_index_ledger::QuestionIndexLedger::enqueue_question_units_with_conn(
            conn,
            &question_ids,
            now_ms,
        )?;
        debug!(
            "[VFS::QuestionGroupRepo] enqueued reindex for {} child question(s) of group {}",
            question_ids.len(),
            group_id
        );
        Ok(())
    }

    /// 材料变更后需要重索引的子题 id（供索引 ledger 在同一事务内入队）
    pub fn material_change_reindex_question_ids_with_conn(
        conn: &Connection,
        group_id: &str,
    ) -> VfsResult<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT id FROM questions WHERE group_id = ?1 AND deleted_at IS NULL ORDER BY group_order, id",
        )?;
        let rows = stmt.query_map(params![group_id], |row| row.get::<_, String>(0))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// 子题可见性谓词（供 question_repo 读取路径拼接）
    pub fn visible_question_predicate(alias: &str) -> String {
        visible_child_question_sql(alias)
    }

    // ------------------------------------------------------------------
    // 行映射
    // ------------------------------------------------------------------

    fn group_columns() -> &'static str {
        "id, exam_id, group_type, title, source_type, source_ref, import_baseline_json,
         version, publish_state, expected_child_ids_json, expected_material_ids_json,
         created_at, updated_at, deleted_at"
    }

    fn group_columns_with_alias(alias: &str) -> String {
        [
            "id",
            "exam_id",
            "group_type",
            "title",
            "source_type",
            "source_ref",
            "import_baseline_json",
            "version",
            "publish_state",
            "expected_child_ids_json",
            "expected_material_ids_json",
            "created_at",
            "updated_at",
            "deleted_at",
        ]
        .iter()
        .map(|column| format!("{}.{}", alias, column))
        .collect::<Vec<_>>()
        .join(", ")
    }

    fn material_columns() -> &'static str {
        "id, group_id, exam_id, order_index, content_type, title, text, images_json,
         source_ref, page_index, bbox_json, group_version, created_at, updated_at, deleted_at"
    }

    fn row_to_group(row: &Row) -> rusqlite::Result<QuestionGroup> {
        let group_type_str: Option<String> = row.get(2)?;
        let source_type_str: Option<String> = row.get(4)?;
        let publish_state_str: Option<String> = row.get(8)?;
        let baseline: Option<String> = row.get(6)?;
        let expected_children: Option<String> = row.get(9)?;
        let expected_materials: Option<String> = row.get(10)?;
        Ok(QuestionGroup {
            id: row.get(0)?,
            exam_id: row.get(1)?,
            group_type: group_type_str
                .map(|s| QuestionGroupType::from_str(&s))
                .unwrap_or_default(),
            title: row.get(3)?,
            source_type: source_type_str
                .map(|s| SourceType::from_str(&s))
                .unwrap_or_default(),
            source_ref: row.get(5)?,
            import_baseline: baseline.and_then(|raw| serde_json::from_str(&raw).ok()),
            version: row.get(7)?,
            publish_state: publish_state_str
                .map(|s| GroupPublishState::from_str(&s))
                .unwrap_or_default(),
            expected_child_ids: Self::json_array_or_empty(expected_children.as_ref()),
            expected_material_ids: Self::json_array_or_empty(expected_materials.as_ref()),
            created_at: row.get(11)?,
            updated_at: row.get(12)?,
            deleted_at: row.get(13)?,
        })
    }

    fn row_to_material(row: &Row) -> rusqlite::Result<QuestionGroupMaterial> {
        let content_type_str: Option<String> = row.get(4)?;
        let images_json: Option<String> = row.get(7)?;
        let images: Vec<QuestionImage> = images_json
            .as_ref()
            .and_then(|raw| match serde_json::from_str(raw) {
                Ok(value) => Some(value),
                Err(error) => {
                    warn!(
                        "[VFS::QuestionGroupRepo] failed to parse material images_json: {}",
                        error
                    );
                    None
                }
            })
            .unwrap_or_default();
        let bbox_json: Option<String> = row.get(10)?;
        let bbox: Option<[f64; 4]> = bbox_json.and_then(|raw| serde_json::from_str(&raw).ok());
        Ok(QuestionGroupMaterial {
            id: row.get(0)?,
            group_id: row.get(1)?,
            exam_id: row.get(2)?,
            order_index: row.get(3)?,
            content_type: content_type_str
                .map(|s| MaterialContentType::from_str(&s))
                .unwrap_or_default(),
            title: row.get(5)?,
            text: row.get(6)?,
            images,
            source_ref: row.get(8)?,
            page_index: row.get(9)?,
            bbox,
            group_version: row.get(11)?,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
            deleted_at: row.get(14)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::repos::VfsQuestionRepo;

    fn child(
        exam_id: &str,
        content: &str,
        order: i32,
        sub_label: Option<&str>,
    ) -> ProposedChildQuestion {
        ProposedChildQuestion {
            client_ref: Some(format!("c{order}")),
            question: CreateQuestionParams {
                exam_id: exam_id.to_string(),
                card_id: None,
                question_label: None,
                content: content.to_string(),
                options: None,
                answer: Some(format!("answer-{order}")),
                explanation: Some("hidden explanation".to_string()),
                structured_data: None,
                question_type: None,
                difficulty: None,
                tags: None,
                source_type: None,
                source_ref: None,
                images: None,
                parent_id: None,
            },
            sub_label: sub_label.map(|s| s.to_string()),
            group_order: order,
        }
    }

    fn material(group_id: &str, order: i32, text: &str) -> NewMaterial {
        NewMaterial {
            group_id: group_id.to_string(),
            order_index: order,
            content_type: MaterialContentType::Text,
            title: None,
            text: text.to_string(),
            images: Vec::new(),
            source_ref: None,
            page_index: None,
            bbox: None,
        }
    }

    fn new_group(exam_id: &str, group_type: QuestionGroupType) -> NewQuestionGroup {
        NewQuestionGroup {
            exam_id: exam_id.to_string(),
            group_type,
            title: Some("Reading 1".to_string()),
            source_type: None,
            source_ref: None,
            import_baseline: None,
            publish_state: None,
        }
    }

    fn insert_exam(db: &VfsDatabase, exam_id: &str) {
        let conn = db.get_conn_safe().unwrap();
        conn.execute(
            "INSERT INTO exam_sheets (id, exam_name, status, temp_id, metadata_json, preview_json, created_at, updated_at)
             VALUES (?1, 'exam', 'completed', ?2, '{}', '{}', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            params![exam_id, format!("temp-{exam_id}")],
        )
        .unwrap();
    }

    #[test]
    fn publish_group_bundle_is_atomic_and_visible() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");

        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::Reading),
            materials: vec![material("", 1, "passage text")],
            children: vec![
                child("exam-1", "q1 stem", 1, Some("(1)")),
                child("exam-1", "q2 stem", 2, Some("(2)")),
            ],
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).expect("publish");
        assert_eq!(published.materials.len(), 1);
        assert_eq!(published.siblings.len(), 2);
        assert_eq!(published.group.expected_child_ids.len(), 2);
        assert_eq!(published.group.version, 1);

        let conn = db.get_conn_safe().unwrap();
        // 组头不是 question：题目数 == 子题数
        let question_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE exam_id='exam-1' AND deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(question_count, 2);

        // 单题上下文注入：含材料与兄弟题干，但绝不含答案/解析
        let mut question =
            VfsQuestionRepo::get_question_with_conn(&conn, &published.siblings[0].id)
                .unwrap()
                .expect("child exists");
        assert!(question.group_context.is_some());
        let context = question.group_context.take().unwrap();
        assert_eq!(context.materials.len(), 1);
        assert_eq!(context.siblings.len(), 2);
        assert_eq!(context.siblings[1].content, "q2 stem");
        let serialized = serde_json::to_string(&context).unwrap();
        assert!(!serialized.contains("answer-"));
        assert!(!serialized.contains("hidden explanation"));
        assert_eq!(context.group_progress.total, 2);
    }

    #[test]
    fn publish_rolls_back_when_any_child_invalid() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        insert_exam(&db, "exam-2");

        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::SharedCondition),
            materials: vec![material("", 1, "shared condition")],
            children: vec![
                child("exam-1", "ok", 1, Some("(1)")),
                child("exam-2", "wrong exam", 2, Some("(2)")),
            ],
        };
        assert!(VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).is_err());

        let conn = db.get_conn_safe().unwrap();
        let groups: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_groups", [], |row| row.get(0))
            .unwrap();
        let questions: i64 = conn
            .query_row("SELECT COUNT(*) FROM questions", [], |row| row.get(0))
            .unwrap();
        let materials: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_group_materials", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((groups, questions, materials), (0, 0, 0), "整组必须回滚");
    }

    #[test]
    fn reading_group_requires_material_and_contiguous_orders() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");

        let no_material = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::Reading),
            materials: vec![],
            children: vec![child("exam-1", "q", 1, None)],
        };
        assert!(VfsQuestionGroupRepo::publish_group_bundle(&db, &no_material).is_err());

        let gap_order = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::Reading),
            materials: vec![material("", 1, "m")],
            children: vec![child("exam-1", "a", 1, None), child("exam-1", "b", 3, None)],
        };
        assert!(VfsQuestionGroupRepo::publish_group_bundle(&db, &gap_order).is_err());
    }

    #[test]
    fn group_cas_rejects_stale_version_and_material_edit_bumps_version() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::Reading),
            materials: vec![material("", 1, "v1")],
            children: vec![child("exam-1", "q", 1, None)],
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).unwrap();
        let conn = db.get_conn_safe().unwrap();

        let material_id = published.materials[0].id.clone();
        let update = MaterialUpdate {
            text: Some("v2".to_string()),
            ..Default::default()
        };
        let updated =
            VfsQuestionGroupRepo::update_material_with_conn(&conn, &material_id, &update, 1)
                .expect("material update with expected group version 1");
        assert_eq!(updated.group_version, 2);

        // 旧版本 CAS 必须失败且不写入
        let err = VfsQuestionGroupRepo::update_material_with_conn(&conn, &material_id, &update, 1)
            .unwrap_err();
        assert!(err.to_string().contains("QBANK_CONFLICT"), "got {err}");

        // 组元数据 CAS 同样生效
        let err = VfsQuestionGroupRepo::update_group_with_conn(
            &conn,
            &published.group.id,
            &GroupUpdate {
                title: Some("new title".to_string()),
                ..Default::default()
            },
            1,
        )
        .unwrap_err();
        assert!(err.to_string().contains("QBANK_CONFLICT"));
    }

    #[test]
    fn staging_group_is_invisible_and_delete_cascades() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::SharedCondition),
            materials: vec![material("", 1, "shared")],
            children: vec![child("exam-1", "q", 1, Some("(1)"))],
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).unwrap();
        let conn = db.get_conn_safe().unwrap();

        conn.execute(
            "UPDATE question_groups SET publish_state='staging' WHERE id=?1",
            params![published.group.id],
        )
        .unwrap();
        assert!(
            VfsQuestionGroupRepo::get_bundle_with_conn(&conn, &published.group.id)
                .unwrap()
                .is_none()
        );
        let listed = VfsQuestionGroupRepo::list_groups_with_conn(&conn, "exam-1", 1, 10).unwrap();
        assert_eq!(listed.total, 0);
        // 子题也不能通过单题读取泄露
        assert!(
            VfsQuestionRepo::get_question_with_conn(&conn, &published.siblings[0].id)
                .unwrap()
                .is_none()
        );

        conn.execute(
            "UPDATE question_groups SET publish_state='complete' WHERE id=?1",
            params![published.group.id],
        )
        .unwrap();

        // 仅剩一份材料时拒绝单独删除
        let err = VfsQuestionGroupRepo::delete_material_with_conn(
            &conn,
            &published.materials[0].id,
            published.group.version,
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("QBANK_GROUP_MATERIAL_IN_USE"),
            "got {err}"
        );

        let summary = VfsQuestionGroupRepo::delete_group_with_conn(
            &conn,
            &published.group.id,
            published.group.version,
        )
        .unwrap();
        assert_eq!(summary.children_deleted, 1);
        assert_eq!(summary.materials_deleted, 1);
        assert!(
            VfsQuestionGroupRepo::get_bundle_with_conn(&conn, &published.group.id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn deleting_one_child_repairs_manifest_and_keeps_group_visible() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::SharedCondition),
            materials: vec![material("", 1, "shared")],
            children: vec![
                child("exam-1", "a", 1, Some("(1)")),
                child("exam-1", "b", 2, Some("(2)")),
            ],
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).unwrap();
        let conn = db.get_conn_safe().unwrap();

        VfsQuestionRepo::delete_question_with_conn(&conn, &published.siblings[0].id).unwrap();

        let bundle = VfsQuestionGroupRepo::get_bundle_with_conn(&conn, &published.group.id)
            .unwrap()
            .expect("组必须保持可见（manifest 已修复）");
        assert_eq!(bundle.siblings.len(), 1);
        assert_eq!(bundle.group.expected_child_ids.len(), 1);
        assert_eq!(bundle.group.expected_child_ids[0], published.siblings[1].id);
        assert_eq!(bundle.group.version, published.group.version + 1);
        assert_eq!(bundle.materials[0].group_version, bundle.group.version);

        // 删掉最后一个子题：组变为空壳，必须不可见
        VfsQuestionRepo::delete_question_with_conn(&conn, &published.siblings[1].id).unwrap();
        assert!(
            VfsQuestionGroupRepo::get_bundle_with_conn(&conn, &published.group.id)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn stale_delete_versions_do_not_partially_delete() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::SharedCondition),
            materials: vec![material("", 1, "cond-a"), material("", 2, "cond-b")],
            children: vec![child("exam-1", "q", 1, Some("(1)"))],
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).unwrap();
        let conn = db.get_conn_safe().unwrap();

        // 先做一次材料编辑把组版本推到 2
        let update = MaterialUpdate {
            text: Some("cond-a-v2".to_string()),
            ..Default::default()
        };
        VfsQuestionGroupRepo::update_material_with_conn(
            &conn,
            &published.materials[0].id,
            &update,
            1,
        )
        .unwrap();

        // 用过期版本删除材料：必须拒绝且不产生软删
        let error =
            VfsQuestionGroupRepo::delete_material_with_conn(&conn, &published.materials[1].id, 1)
                .unwrap_err();
        assert!(error.to_string().contains("QBANK_CONFLICT"), "got {error}");
        let live_materials: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM question_group_materials WHERE group_id=?1 AND deleted_at IS NULL",
                params![published.group.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(live_materials, 2, "过期 CAS 不得软删材料");

        // 用过期版本删整组：CAS 先失败，成员绝不能半删
        let error = VfsQuestionGroupRepo::delete_group_with_conn(&conn, &published.group.id, 1)
            .unwrap_err();
        assert!(error.to_string().contains("QBANK_CONFLICT"), "got {error}");
        let live_children: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE group_id=?1 AND deleted_at IS NULL",
                params![published.group.id],
                |row| row.get(0),
            )
            .unwrap();
        let group_deleted: Option<String> = conn
            .query_row(
                "SELECT deleted_at FROM question_groups WHERE id=?1",
                params![published.group.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(live_children, 1);
        assert!(group_deleted.is_none());
    }

    #[test]
    fn material_fts_hits_child_questions() {
        let (_tmp, db) = crate::vfs::database::setup_migrated_test_db();
        insert_exam(&db, "exam-1");
        let bundle = ProposedGroupBundle {
            group: new_group("exam-1", QuestionGroupType::Reading),
            materials: vec![material("", 1, "photosynthesis passage")],
            children: vec![child("exam-1", "what is it", 1, Some("(1)"))],
        };
        VfsQuestionGroupRepo::publish_group_bundle(&db, &bundle).unwrap();
        let conn = db.get_conn_safe().unwrap();
        let hits = VfsQuestionGroupRepo::search_material_question_ids_with_conn(
            &conn,
            "\"photosynthesis\"",
            Some("exam-1"),
            10,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
    }
}
