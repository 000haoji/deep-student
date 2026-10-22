//! 题库包（native JSON bundle v2）导入导出服务
//!
//! 权威格式：`deep-student.qbank.bundle` / `format_version = 2`。
//! 契约与字段说明：`docs/dev/qbank-bundle-v2.md`。
//!
//! ## 安全与副作用边界
//! - 运行时读写的是用户自己的 VFS 数据库（`vfs.db`）与 `vfs_blobs/`；导出只读，
//!   导入写入新行并在同一事务内提交。不访问网络、不写包外路径、不改用户既有
//!   题目的教学内容。
//! - 组/材料/子题写入只经 `VfsQuestionGroupRepo::publish_group_bundle_with_conn`，
//!   独立题写入只经 `VfsQuestionRepo::create_question_with_conn`；本模块只为
//!   回读补充只读 SQL（软删行）与新建行的统计回填。
//! - 资产闭包：真实 blob 字节 → base64，校验 sha256/size，路径封闭在
//!   `vfs_blobs/` 内；缺失资产是硬错误（`include_assets=false` 时只能作为
//!   显式 lossy 投影，不得冒充可往返）。
//!
//! ## 往返语义
//! - 导入永远重映射主键（`client_ref` → 新 ID），因此题干完全相同的两道题仍是
//!   两道题；不按题干去重。
//! - `tombstones` 在导出时覆盖软删的组/材料/题，导入时命中即跳过，不复活。
//! - 合并到既有题目集：同 `source_ref` 的题不覆盖（保留用户编辑），组按
//!   `source_ref` 幂等；组内冲突整组跳过，不生成半组。
//! - CSV/Markdown/Anki 只是**导出投影**：展开材料并标注 `lossy` /
//!   `assets_not_included`，权威库仍只保存一份材料。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Component, Path, PathBuf};

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::models::AppError;
use crate::vfs::database::VfsDatabase;
use crate::vfs::repos::{
    CreateQuestionParams, Difficulty, Question, QuestionImage, QuestionOption, QuestionStatus,
    QuestionType, SourceType, VfsBlobRepo, VfsFileRepo, VfsQuestionGroupRepo, VfsQuestionRepo,
};
use crate::vfs::types::VfsCreateExamSheetParams;

/// 权威 schema 标识（唯一字段，不再引入其他头部字段）
pub const BUNDLE_SCHEMA: &str = "deep-student.qbank.bundle";
/// 当前支持的格式版本
pub const BUNDLE_FORMAT_VERSION: u32 = 2;

// ============================================================================
// 权威 bundle DTO
// ============================================================================

/// 题目包根对象（native JSON v2）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionBankBundle {
    pub schema: String,
    pub format_version: u32,
    pub exported_at: String,
    pub exam: BundleExam,
    pub sources: Vec<BundleSource>,
    pub groups: Vec<BundleGroup>,
    pub materials: Vec<BundleMaterial>,
    pub children: Vec<BundleChild>,
    pub standalone_questions: Vec<BundleStandaloneQuestion>,
    /// 全部 client_ref（子题 + 独立题）的展示顺序
    pub order: Vec<String>,
    pub tombstones: Vec<BundleTombstone>,
    /// 资产闭包：唯一字节载体（base64）
    pub assets: Vec<BundleAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleExam {
    pub name: String,
    #[serde(default)]
    pub source_exam_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleSource {
    pub source_id: String,
    pub source_type: SourceType,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub page_index: Option<u32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleGroupType {
    Reading,
    SharedCondition,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleMaterialContentType {
    Text,
    Formula,
    Figure,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleGroup {
    pub group_ref: String,
    pub group_type: BundleGroupType,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleMaterial {
    pub material_ref: String,
    pub group_ref: String,
    /// 组内顺序，1..M 连续（与 `question_group_materials.order_index` 一致）
    pub order_index: u32,
    pub content_type: BundleMaterialContentType,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub images: Vec<BundleImageRef>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub page_index: Option<u32>,
    #[serde(default)]
    pub bbox: Option<serde_json::Value>,
}

/// 图片/资产引用（字节在根级 `assets` 里，图片这里只放引用）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleImageRef {
    pub asset_id: String,
    pub name: String,
    pub mime: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleChild {
    pub client_ref: String,
    pub group_ref: String,
    /// 组内子题顺序，1..N 连续（与 `questions.group_order` 一致）
    pub group_order: u32,
    #[serde(default)]
    pub sub_label: Option<String>,
    pub question: BundleQuestion,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleStandaloneQuestion {
    pub client_ref: String,
    pub question: BundleQuestion,
}

/// 题目载荷（与 `CreateQuestionParams` 一一对应，外加可选 study 快照）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleQuestion {
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub options: Option<Vec<QuestionOption>>,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(default)]
    pub structured_data: Option<serde_json::Value>,
    #[serde(default)]
    pub question_type: Option<QuestionType>,
    #[serde(default)]
    pub difficulty: Option<Difficulty>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub source_type: Option<SourceType>,
    #[serde(default)]
    pub source_ref: Option<String>,
    #[serde(default)]
    pub question_label: Option<String>,
    #[serde(default)]
    pub images: Vec<BundleImageRef>,
    /// 学习状态快照；仅在导入新建题目且 `include_study_state=true` 时应用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub study: Option<BundleStudyState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleStudyState {
    #[serde(default)]
    pub status: QuestionStatus,
    #[serde(default)]
    pub user_answer: Option<String>,
    #[serde(default)]
    pub is_correct: Option<bool>,
    #[serde(default)]
    pub user_note: Option<String>,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub is_bookmarked: bool,
    #[serde(default)]
    pub attempt_count: i32,
    #[serde(default)]
    pub correct_count: i32,
    #[serde(default)]
    pub last_attempt_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum BundleTombstoneKind {
    Group,
    Material,
    Question,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleTombstone {
    pub kind: BundleTombstoneKind,
    /// 与 bundle 内 ref 同名（group_ref / material_ref / client_ref）
    pub ref_id: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleAssetEncoding {
    Base64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BundleAsset {
    pub asset_id: String,
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    pub name: String,
    pub encoding: BundleAssetEncoding,
    pub data: String,
}

// ============================================================================
// 请求 / 结果
// ============================================================================

/// 导出请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleExportRequest {
    pub exam_id: String,
    /// 写入文件路径；None 时 JSON 通过 `json` 字段返回
    #[serde(default)]
    pub file_path: Option<String>,
    /// 是否内嵌真实资产字节；false 是显式 lossy 投影
    #[serde(default = "default_true")]
    pub include_assets: bool,
    /// 是否导出学习状态快照
    #[serde(default = "default_true")]
    pub include_study_state: bool,
}

fn default_true() -> bool {
    true
}

/// 导出的非致命提示
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleWarning {
    pub code: String,
    pub message: String,
    #[serde(default)]
    pub ref_id: Option<String>,
}

/// 导出结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleExportResult {
    pub schema: String,
    pub format_version: u32,
    #[serde(default)]
    pub file_path: Option<String>,
    pub file_size: u64,
    pub sha256: String,
    /// 是否具备完整往返能力（false 时看 warnings）
    pub round_trip_safe: bool,
    pub groups: u32,
    pub materials: u32,
    pub questions: u32,
    pub standalone_questions: u32,
    pub assets: u32,
    pub tombstones: u32,
    #[serde(default)]
    pub warnings: Vec<BundleWarning>,
    /// 仅当未指定 file_path 时返回完整 JSON 文本
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub json: Option<String>,
}

/// 导入目标
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BundleImportTarget {
    /// 新建题目集
    NewExam { name: Option<String> },
    /// 合并进既有题目集（不覆盖已存在的同 source_ref 题目）
    Merge { exam_id: String },
}

/// 导入请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleImportRequest {
    /// bundle 根对象；也接受 v1 的独立题数组（legacy 兼容）
    pub bundle: serde_json::Value,
    pub target: BundleImportTarget,
    /// 默认 dry_run（安全默认），只校验不写
    #[serde(default = "default_true")]
    pub dry_run: bool,
    #[serde(default = "default_true")]
    pub include_study_state: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleIdMapEntry {
    pub client_ref: String,
    /// group | material | question
    pub kind: String,
    pub new_id: String,
}

/// 导入结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleImportResult {
    pub schema: String,
    pub format_version: u32,
    pub dry_run: bool,
    pub committed: bool,
    #[serde(default)]
    pub exam_id: Option<String>,
    pub legacy_v1: bool,
    pub groups_created: u32,
    pub materials_created: u32,
    pub questions_created: u32,
    pub questions_skipped: u32,
    pub assets_created: u32,
    pub assets_deduped: u32,
    pub tombstones_applied: u32,
    pub id_map: Vec<BundleIdMapEntry>,
    pub warnings: Vec<BundleWarning>,
}

// ============================================================================
// 解析与校验（纯函数）
// ============================================================================

/// 解析入口结果
pub enum ParsedBundle {
    V2(QuestionBankBundle),
    /// v1：裸独立题数组（无组/材料/资产）
    LegacyV1(Vec<BundleQuestion>),
}

/// 解析并做版本门禁。
///
/// 规则：对象形式必须是 `schema == BUNDLE_SCHEMA` 且 `format_version == 2`
/// （高/低版本都拒绝，不静默降级）；数组形式视为 v1 独立题列表。
pub fn parse_bundle(value: serde_json::Value) -> Result<ParsedBundle, AppError> {
    match value {
        serde_json::Value::Array(items) => {
            let mut questions = Vec::with_capacity(items.len());
            for (idx, item) in items.into_iter().enumerate() {
                let q: BundleQuestion = serde_json::from_value(item).map_err(|e| {
                    AppError::validation(format!("v1 独立题数组第 {} 项解析失败: {}", idx + 1, e))
                })?;
                questions.push(q);
            }
            Ok(ParsedBundle::LegacyV1(questions))
        }
        serde_json::Value::Object(map) => {
            let schema = map
                .get("schema")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::validation("题目包缺少 schema 头字段"))?;
            if schema != BUNDLE_SCHEMA {
                return Err(AppError::validation(format!(
                    "不支持的题目包 schema: {}（期望 {}）",
                    schema, BUNDLE_SCHEMA
                )));
            }
            let version = map
                .get("format_version")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| AppError::validation("题目包缺少 format_version"))?;
            if version != BUNDLE_FORMAT_VERSION as u64 {
                return Err(AppError::validation(format!(
                    "不支持的题目包格式版本: {}（当前支持 {}）",
                    version, BUNDLE_FORMAT_VERSION
                )));
            }
            let bundle: QuestionBankBundle = serde_json::from_value(serde_json::Value::Object(map))
                .map_err(|e| AppError::validation(format!("题目包结构解析失败: {}", e)))?;
            Ok(ParsedBundle::V2(bundle))
        }
        other => Err(AppError::validation(format!(
            "题目包根节点必须是对象或 v1 独立题数组，实际是 {}",
            json_kind(&other)
        ))),
    }
}

fn json_kind(v: &serde_json::Value) -> &'static str {
    match v {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "bool",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// 校验通过后的结构摘要
#[derive(Debug, Clone, PartialEq)]
pub struct BundleValidation {
    pub group_count: usize,
    pub material_count: usize,
    pub child_count: usize,
    pub standalone_count: usize,
    pub asset_count: usize,
    pub tombstone_count: usize,
}

/// 语义校验（不碰数据库、不碰磁盘）
pub fn validate_bundle(bundle: &QuestionBankBundle) -> Result<BundleValidation, AppError> {
    validate_bundle_inner(bundle, true)
}

/// 语义校验；`require_assets=false` 只用于显式 lossy 导出（图片引用保留但字节不带）
pub fn validate_bundle_inner(
    bundle: &QuestionBankBundle,
    require_assets: bool,
) -> Result<BundleValidation, AppError> {
    if bundle.schema != BUNDLE_SCHEMA {
        return Err(AppError::validation(format!(
            "schema 不是 {}",
            BUNDLE_SCHEMA
        )));
    }
    if bundle.format_version != BUNDLE_FORMAT_VERSION {
        return Err(AppError::validation(format!(
            "format_version 必须是 {}",
            BUNDLE_FORMAT_VERSION
        )));
    }
    if bundle.exam.name.trim().is_empty() {
        return Err(AppError::validation("exam.name 不能为空"));
    }

    let mut source_ids = HashSet::new();
    for s in &bundle.sources {
        require_non_empty("sources.source_id", &s.source_id)?;
        if !source_ids.insert(s.source_id.as_str()) {
            return Err(AppError::validation(format!(
                "sources.source_id 重复: {}",
                s.source_id
            )));
        }
    }

    let mut group_refs: HashSet<&str> = HashSet::new();
    for g in &bundle.groups {
        require_non_empty("groups.group_ref", &g.group_ref)?;
        if !group_refs.insert(g.group_ref.as_str()) {
            return Err(AppError::validation(format!(
                "group_ref 重复: {}",
                g.group_ref
            )));
        }
    }

    let mut material_refs: HashSet<&str> = HashSet::new();
    let mut material_orders: HashMap<&str, BTreeSet<u32>> = HashMap::new();
    for m in &bundle.materials {
        require_non_empty("materials.material_ref", &m.material_ref)?;
        if !material_refs.insert(m.material_ref.as_str()) {
            return Err(AppError::validation(format!(
                "material_ref 重复: {}",
                m.material_ref
            )));
        }
        if !group_refs.contains(m.group_ref.as_str()) {
            return Err(AppError::validation(format!(
                "材料 {} 引用了不存在的 group_ref {}",
                m.material_ref, m.group_ref
            )));
        }
        if m.order_index == 0 {
            return Err(AppError::validation(format!(
                "材料 {} 的 order_index 必须从 1 开始",
                m.material_ref
            )));
        }
        if !material_orders
            .entry(m.group_ref.as_str())
            .or_default()
            .insert(m.order_index)
        {
            return Err(AppError::validation(format!(
                "组 {} 内 order_index 重复: {}",
                m.group_ref, m.order_index
            )));
        }
    }
    for (group_ref, orders) in &material_orders {
        require_contiguous_1_based(group_ref, "order_index", orders)?;
    }

    let mut client_refs: HashSet<&str> = HashSet::new();
    let mut child_orders: HashMap<&str, BTreeSet<u32>> = HashMap::new();
    for c in &bundle.children {
        require_non_empty("children.client_ref", &c.client_ref)?;
        if !client_refs.insert(c.client_ref.as_str()) {
            return Err(AppError::validation(format!(
                "client_ref 重复: {}",
                c.client_ref
            )));
        }
        if !group_refs.contains(c.group_ref.as_str()) {
            return Err(AppError::validation(format!(
                "子题 {} 引用了不存在的 group_ref {}",
                c.client_ref, c.group_ref
            )));
        }
        if c.group_order == 0 {
            return Err(AppError::validation(format!(
                "子题 {} 的 group_order 必须从 1 开始",
                c.client_ref
            )));
        }
        if !child_orders
            .entry(c.group_ref.as_str())
            .or_default()
            .insert(c.group_order)
        {
            return Err(AppError::validation(format!(
                "组 {} 内 group_order 重复: {}",
                c.group_ref, c.group_order
            )));
        }
        validate_question(&c.question, &c.client_ref)?;
    }
    for (group_ref, orders) in &child_orders {
        require_contiguous_1_based(group_ref, "group_order", orders)?;
    }

    for s in &bundle.standalone_questions {
        require_non_empty("standalone_questions.client_ref", &s.client_ref)?;
        if !client_refs.insert(s.client_ref.as_str()) {
            return Err(AppError::validation(format!(
                "client_ref 重复: {}",
                s.client_ref
            )));
        }
        validate_question(&s.question, &s.client_ref)?;
    }

    // order 必须恰好覆盖全部 client_ref（子题 + 独立题），不重复不遗漏
    let mut order_seen = HashSet::new();
    for r in &bundle.order {
        if !client_refs.contains(r.as_str()) {
            return Err(AppError::validation(format!(
                "order 引用了不存在的 client_ref: {}",
                r
            )));
        }
        if !order_seen.insert(r.as_str()) {
            return Err(AppError::validation(format!("order 重复: {}", r)));
        }
    }
    if order_seen.len() != client_refs.len() {
        let missing: Vec<&str> = client_refs
            .iter()
            .filter(|r| !order_seen.contains(*r))
            .copied()
            .collect();
        return Err(AppError::validation(format!(
            "order 未覆盖全部题目，缺少: {:?}",
            missing
        )));
    }

    let mut tombstone_keys = HashSet::new();
    for t in &bundle.tombstones {
        require_non_empty("tombstones.ref_id", &t.ref_id)?;
        let key = (t.kind, t.ref_id.as_str());
        if !tombstone_keys.insert(key) {
            return Err(AppError::validation(format!(
                "tombstone 重复: {:?} {}",
                t.kind, t.ref_id
            )));
        }
    }

    let mut asset_ids = HashSet::new();
    for a in &bundle.assets {
        require_non_empty("assets.asset_id", &a.asset_id)?;
        if !asset_ids.insert(a.asset_id.as_str()) {
            return Err(AppError::validation(format!(
                "asset_id 重复: {}",
                a.asset_id
            )));
        }
        if a.sha256.len() != 64 || !a.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(AppError::validation(format!(
                "资产 {} 的 sha256 非法",
                a.asset_id
            )));
        }
        validate_asset_name(&a.asset_id, &a.name)?;
        if a.mime.trim().is_empty() {
            return Err(AppError::validation(format!(
                "资产 {} 缺少 mime",
                a.asset_id
            )));
        }
        if a.data.is_empty() {
            return Err(AppError::validation(format!(
                "资产 {} 缺少 data",
                a.asset_id
            )));
        }
    }

    // 所有图片引用必须能落到 assets（导入侧始终强制；导出侧仅在 include_assets 时强制）
    for m in &bundle.materials {
        for img in &m.images {
            if require_assets && !asset_ids.contains(img.asset_id.as_str()) {
                return Err(AppError::validation(format!(
                    "材料 {} 引用的 asset_id {} 不在 assets 中",
                    m.material_ref, img.asset_id
                )));
            }
        }
    }
    if require_assets {
        for c in &bundle.children {
            check_question_assets(&c.question, &asset_ids, &c.client_ref)?;
        }
        for s in &bundle.standalone_questions {
            check_question_assets(&s.question, &asset_ids, &s.client_ref)?;
        }
    }

    Ok(BundleValidation {
        group_count: bundle.groups.len(),
        material_count: bundle.materials.len(),
        child_count: bundle.children.len(),
        standalone_count: bundle.standalone_questions.len(),
        asset_count: bundle.assets.len(),
        tombstone_count: bundle.tombstones.len(),
    })
}

fn check_question_assets(
    q: &BundleQuestion,
    asset_ids: &HashSet<&str>,
    owner: &str,
) -> Result<(), AppError> {
    for img in &q.images {
        if !asset_ids.contains(img.asset_id.as_str()) {
            return Err(AppError::validation(format!(
                "题目 {} 引用的 asset_id {} 不在 assets 中",
                owner, img.asset_id
            )));
        }
    }
    Ok(())
}

fn validate_question(q: &BundleQuestion, owner: &str) -> Result<(), AppError> {
    if q.content.trim().is_empty() && q.images.is_empty() {
        return Err(AppError::validation(format!(
            "题目 {} 既没有题干也没有图片",
            owner
        )));
    }
    if let Some(options) = &q.options {
        let mut keys = HashSet::new();
        for o in options {
            if o.key.trim().is_empty() {
                return Err(AppError::validation(format!(
                    "题目 {} 存在空选项 key",
                    owner
                )));
            }
            if !keys.insert(o.key.as_str()) {
                return Err(AppError::validation(format!(
                    "题目 {} 选项 key 重复: {}",
                    owner, o.key
                )));
            }
        }
    }
    Ok(())
}

fn require_non_empty(field: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::validation(format!("{} 不能为空", field)));
    }
    Ok(())
}

fn require_contiguous_1_based(
    owner: &str,
    field: &str,
    values: &BTreeSet<u32>,
) -> Result<(), AppError> {
    let expected: Vec<u32> = (1..=values.len() as u32).collect();
    let actual: Vec<u32> = values.iter().copied().collect();
    if expected != actual {
        return Err(AppError::validation(format!(
            "{} 的 {} 必须是 1..{} 连续且唯一，实际为 {:?}",
            owner,
            field,
            values.len(),
            actual
        )));
    }
    Ok(())
}

/// 资产文件名只能是普通文件名（无路径分隔、无 `..`），防路径逃逸
fn validate_asset_name(asset_id: &str, name: &str) -> Result<(), AppError> {
    if name.trim().is_empty() {
        return Err(AppError::validation(format!("资产 {} 缺少 name", asset_id)));
    }
    let path = Path::new(name);
    if path.is_absolute()
        || name.contains('/')
        || name.contains('\\')
        || path.components().any(|c| {
            matches!(
                c,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
    {
        return Err(AppError::validation(format!(
            "资产 {} 的 name 含路径成分，拒绝: {}",
            asset_id, name
        )));
    }
    Ok(())
}

// ============================================================================
// 资产闭包
// ============================================================================

/// 资产读取结果
#[derive(Debug, Clone)]
pub struct StoredAsset {
    pub asset_id: String,
    pub sha256: String,
    pub size: u64,
    pub mime: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 从 `vfs_blobs/` 读取并校验真实字节。
///
/// 校验点：hash 与 blob 记录一致；路径必须落在 canonical `blobs_dir` 内；
/// 文件大小与内容 sha256 必须与期望一致。任何不一致都返回明确错误，
/// 不静默跳过（避免导出“看起来成功”的残缺包）。
pub fn read_blob_for_export(
    blobs_dir: &Path,
    asset_id: &str,
    expected_hash: &str,
    name: &str,
    mime: &str,
) -> Result<StoredAsset, AppError> {
    validate_asset_name(asset_id, name)?;
    let canonical_root = blobs_dir
        .canonicalize()
        .map_err(|e| AppError::file_system(format!("blob 目录不可用: {}", e)))?;
    let hash = expected_hash.trim();
    if hash.len() != 64 || !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::validation(format!(
            "资产 {} 的 blob hash 非法: {}",
            asset_id, expected_hash
        )));
    }
    // 直接按约定布局拼路径（prefix/hash.ext），不信任任何外部路径字符串
    let prefix = &hash[..2];
    let mut found: Option<PathBuf> = None;
    let prefix_dir = canonical_root.join(prefix);
    let entries = std::fs::read_dir(&prefix_dir).map_err(|e| {
        AppError::not_found(format!(
            "asset_missing: 资产 {} 的 blob 目录不存在 ({}): {}",
            asset_id,
            prefix_dir.display(),
            e
        ))
    })?;
    for entry in entries {
        let entry =
            entry.map_err(|e| AppError::file_system(format!("读取 blob 目录失败: {}", e)))?;
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if stem == hash {
            found = Some(path);
            break;
        }
    }
    let path = found.ok_or_else(|| {
        AppError::not_found(format!(
            "asset_missing: 资产 {} 的 blob 文件不存在 (hash={})",
            asset_id, hash
        ))
    })?;
    // 路径封闭：canonicalize 后必须仍在 blobs_dir 下
    let canonical = path
        .canonicalize()
        .map_err(|e| AppError::file_system(format!("资产 {} 路径不可解析: {}", asset_id, e)))?;
    if !canonical.starts_with(&canonical_root) {
        return Err(AppError::validation(format!(
            "资产 {} 路径逃逸出 blob 目录，拒绝导出",
            asset_id
        )));
    }
    let bytes = std::fs::read(&canonical)
        .map_err(|e| AppError::file_system(format!("资产 {} 读取失败: {}", asset_id, e)))?;
    let actual = VfsBlobRepo::compute_hash(&bytes);
    if actual != hash {
        return Err(AppError::validation(format!(
            "资产 {} 内容哈希与记录不一致（期望 {}，实际 {}）",
            asset_id, hash, actual
        )));
    }
    Ok(StoredAsset {
        asset_id: asset_id.to_string(),
        sha256: actual,
        size: bytes.len() as u64,
        mime: mime.to_string(),
        name: name.to_string(),
        bytes,
    })
}

/// 把已校验的字节编码成 bundle 资产（base64）
pub fn encode_asset(asset: StoredAsset) -> BundleAsset {
    BundleAsset {
        asset_id: asset.asset_id,
        sha256: asset.sha256,
        size: asset.size,
        mime: asset.mime,
        name: asset.name,
        encoding: BundleAssetEncoding::Base64,
        data: BASE64.encode(&asset.bytes),
    }
}

/// 解码并严格校验 bundle 资产：base64 / size / sha256 / 文件名
pub fn decode_asset(asset: &BundleAsset) -> Result<StoredAsset, AppError> {
    validate_asset_name(&asset.asset_id, &asset.name)?;
    if asset.encoding != BundleAssetEncoding::Base64 {
        return Err(AppError::validation(format!(
            "资产 {} 使用不支持的编码",
            asset.asset_id
        )));
    }
    let bytes = BASE64.decode(asset.data.as_bytes()).map_err(|e| {
        AppError::validation(format!("资产 {} base64 解码失败: {}", asset.asset_id, e))
    })?;
    if bytes.len() as u64 != asset.size {
        return Err(AppError::validation(format!(
            "资产 {} 大小不匹配（声明 {}，实际 {}）",
            asset.asset_id,
            asset.size,
            bytes.len()
        )));
    }
    let actual = VfsBlobRepo::compute_hash(&bytes);
    if actual != asset.sha256 {
        return Err(AppError::validation(format!(
            "资产 {} sha256 不匹配（声明 {}，实际 {}）",
            asset.asset_id, asset.sha256, actual
        )));
    }
    Ok(StoredAsset {
        asset_id: asset.asset_id.clone(),
        sha256: actual,
        size: bytes.len() as u64,
        mime: asset.mime.clone(),
        name: asset.name.clone(),
        bytes,
    })
}

fn mime_extension(mime: &str, name: &str) -> String {
    if let Some(ext) = Path::new(name).extension().and_then(|e| e.to_str()) {
        if !ext.is_empty() && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return ext.to_ascii_lowercase();
        }
    }
    match mime {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        _ => "bin",
    }
    .to_string()
}

// ============================================================================
// 扁平化投影（CSV / Markdown / Anki）
// ============================================================================

/// 扁平化投影的一行（每行对应一个可答子题/独立题，材料展开进 `group_context`）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlatQuestionRow {
    pub group_ref: Option<String>,
    pub group_title: Option<String>,
    pub sub_label: Option<String>,
    pub group_order: Option<u32>,
    /// 展开后的共享材料正文（Markdown）；无组时为空
    pub group_context: String,
    pub content: String,
    pub question_type: String,
    pub options: String,
    pub answer: String,
    pub explanation: String,
    pub tags: String,
    pub question_label: String,
    /// 该行是否因展开投影而无法无损回导
    pub lossy: bool,
}

/// 扁平投影结果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FlatProjection {
    pub rows: Vec<FlatQuestionRow>,
    pub warnings: Vec<BundleWarning>,
}

/// 把 bundle 投影成扁平行：材料只展开到投影，不改权威库。
///
/// 无法表达关系/资产的目标格式（CSV/Markdown/Anki）必须带上
/// `lossy` 与 `assets_not_included` 提示。
pub fn project_flat_rows(bundle: &QuestionBankBundle) -> FlatProjection {
    let mut warnings = vec![BundleWarning {
        code: "lossy".to_string(),
        message: "扁平投影不保留题组/材料关系，无法无损回导".to_string(),
        ref_id: None,
    }];
    if bundle.assets.is_empty() {
        warnings.push(BundleWarning {
            code: "assets_not_included".to_string(),
            message: "该投影不含图片字节，需要图片请使用 native JSON v2 导出".to_string(),
            ref_id: None,
        });
    }

    let groups: HashMap<&str, &BundleGroup> = bundle
        .groups
        .iter()
        .map(|g| (g.group_ref.as_str(), g))
        .collect();
    let mut materials_by_group: HashMap<&str, Vec<&BundleMaterial>> = HashMap::new();
    for m in &bundle.materials {
        materials_by_group
            .entry(m.group_ref.as_str())
            .or_default()
            .push(m);
    }
    for list in materials_by_group.values_mut() {
        list.sort_by_key(|m| m.order_index);
    }

    let questions: BTreeMap<&str, (&BundleQuestion, Option<&BundleChild>)> = bundle
        .children
        .iter()
        .map(|c| (c.client_ref.as_str(), (&c.question, Some(c))))
        .chain(
            bundle
                .standalone_questions
                .iter()
                .map(|s| (s.client_ref.as_str(), (&s.question, None))),
        )
        .collect();

    let mut rows = Vec::new();
    for client_ref in &bundle.order {
        let Some((q, child)) = questions.get(client_ref.as_str()) else {
            continue;
        };
        let (group_ref, group_title, sub_label, group_order, context, lossy) = match child {
            Some(c) => {
                let group = groups.get(c.group_ref.as_str());
                let material_text = materials_by_group
                    .get(c.group_ref.as_str())
                    .map(|list| {
                        list.iter()
                            .map(|m| {
                                let body = if !m.text.trim().is_empty() {
                                    m.text.trim().to_string()
                                } else if !m.images.is_empty() {
                                    let names: Vec<&str> =
                                        m.images.iter().map(|i| i.name.as_str()).collect();
                                    format!("[图片: {}]", names.join(", "))
                                } else {
                                    String::new()
                                };
                                match &m.title {
                                    Some(t) if !t.trim().is_empty() => {
                                        format!("## {}\n{}", t.trim(), body)
                                    }
                                    _ => body,
                                }
                            })
                            .filter(|s| !s.trim().is_empty())
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    })
                    .unwrap_or_default();
                (
                    Some(c.group_ref.clone()),
                    group.and_then(|g| g.title.clone()),
                    c.sub_label.clone(),
                    Some(c.group_order),
                    material_text,
                    true,
                )
            }
            None => (None, None, None, None, String::new(), false),
        };
        rows.push(FlatQuestionRow {
            group_ref,
            group_title,
            sub_label,
            group_order,
            group_context: context,
            content: q.content.clone(),
            question_type: q
                .question_type
                .as_ref()
                .map(|t| t.as_str().to_string())
                .unwrap_or_default(),
            options: q
                .options
                .as_ref()
                .map(|opts| {
                    opts.iter()
                        .map(|o| format!("{}. {}", o.key, o.content))
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default(),
            answer: q.answer.clone().unwrap_or_default(),
            explanation: q.explanation.clone().unwrap_or_default(),
            tags: q.tags.join(","),
            question_label: q.question_label.clone().unwrap_or_default(),
            lossy,
        });
    }

    FlatProjection { rows, warnings }
}

/// 投影为 Markdown（Anki/CSV 共用的扁平表示）
pub fn project_markdown(bundle: &QuestionBankBundle) -> String {
    let projection = project_flat_rows(bundle);
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", bundle.exam.name));
    for row in &projection.rows {
        if let Some(title) = &row.group_title {
            out.push_str(&format!("## {}\n\n", title));
        }
        if !row.group_context.is_empty() {
            out.push_str(&row.group_context);
            out.push_str("\n\n");
        }
        if let Some(label) = Some(row.question_label.as_str()).filter(|s| !s.is_empty()) {
            out.push_str(&format!("**{}** ", label));
        }
        if let Some(sub) = row.sub_label.as_deref().filter(|s| !s.is_empty()) {
            out.push_str(&format!("**{}** ", sub));
        }
        out.push_str(&row.content);
        out.push_str("\n\n");
        if !row.options.is_empty() {
            out.push_str(&row.options);
            out.push_str("\n\n");
        }
        if !row.answer.is_empty() {
            out.push_str(&format!("答案：{}\n\n", row.answer));
        }
    }
    out
}

/// CSV 兼容投影的列契约（与 `question_export_service` 的扁平字段互补：
/// 这里多出题组关系列，让 CSV 用户能看到共享材料；仍然是投影，不可无损回导）
pub const FLAT_CSV_HEADERS: &[&str] = &[
    "group_ref",
    "group_title",
    "sub_label",
    "group_order",
    "group_context",
    "content",
    "question_type",
    "options",
    "answer",
    "explanation",
    "tags",
    "question_label",
];

/// 把 bundle 投影为 CSV 文本（含表头）。
///
/// 该输出显式 lossy（见 [`project_flat_rows`] 的 warnings），且不包含资产字节；
/// 需要无损往返请使用 native JSON v2。公式注入按 OWASP 规则中和。
pub fn project_csv_text(bundle: &QuestionBankBundle) -> String {
    let projection = project_flat_rows(bundle);
    let mut out = String::new();
    out.push_str(
        &FLAT_CSV_HEADERS
            .iter()
            .map(|h| escape_csv_cell(h))
            .collect::<Vec<_>>()
            .join(","),
    );
    out.push('\n');
    for row in &projection.rows {
        let cells = [
            row.group_ref.clone().unwrap_or_default(),
            row.group_title.clone().unwrap_or_default(),
            row.sub_label.clone().unwrap_or_default(),
            row.group_order.map(|v| v.to_string()).unwrap_or_default(),
            row.group_context.clone(),
            row.content.clone(),
            row.question_type.clone(),
            row.options.clone(),
            row.answer.clone(),
            row.explanation.clone(),
            row.tags.clone(),
            row.question_label.clone(),
        ];
        out.push_str(
            &cells
                .iter()
                .map(|c| escape_csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

fn escape_csv_cell(cell: &str) -> String {
    let mut value = cell.to_string();
    let first = value.chars().next();
    let is_formula_prefix = matches!(
        first,
        Some(
            '=' | '+'
                | '-'
                | '@'
                | '\t'
                | '\r'
                | '\n'
                | '\u{FF1D}'
                | '\u{FF0B}'
                | '\u{FF0D}'
                | '\u{FF20}'
        )
    );
    if is_formula_prefix {
        value = format!("\t{}", value);
    }
    if value.contains(',')
        || value.contains('"')
        || value.contains('\n')
        || value.contains('\r')
        || is_formula_prefix
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value
    }
}

// ============================================================================
// 组/材料端口（由 VfsQuestionGroupRepo 适配）
// ============================================================================

/// 组快照（导出用）
#[derive(Debug, Clone)]
pub struct GroupSnapshot {
    pub group_id: String,
    pub group_type: BundleGroupType,
    pub title: Option<String>,
    pub source_type: SourceType,
    pub source_ref: Option<String>,
    pub version: u32,
    pub deleted_at: Option<String>,
    pub materials: Vec<MaterialSnapshot>,
    pub children: Vec<ChildLink>,
}

#[derive(Debug, Clone)]
pub struct MaterialSnapshot {
    pub material_id: String,
    pub order_index: u32,
    pub content_type: BundleMaterialContentType,
    pub title: Option<String>,
    pub text: String,
    pub images: Vec<QuestionImage>,
    pub source_ref: Option<String>,
    pub page_index: Option<u32>,
    pub bbox: Option<serde_json::Value>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ChildLink {
    pub question_id: String,
    pub group_order: u32,
    pub sub_label: Option<String>,
    pub deleted_at: Option<String>,
}

/// 组发布提案（导入用；与 repo `ProposedGroupBundle` 结构一一对应）
#[derive(Debug, Clone)]
pub struct ProposedGroup {
    pub group_ref: String,
    pub group_type: BundleGroupType,
    pub title: Option<String>,
    pub source_type: SourceType,
    pub source_ref: Option<String>,
    pub materials: Vec<ProposedMaterial>,
    pub children: Vec<ProposedChild>,
}

#[derive(Debug, Clone)]
pub struct ProposedMaterial {
    pub material_ref: String,
    pub order_index: u32,
    pub content_type: BundleMaterialContentType,
    pub title: Option<String>,
    pub text: String,
    pub images: Vec<QuestionImage>,
    pub source_ref: Option<String>,
    pub page_index: Option<u32>,
    pub bbox: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct ProposedChild {
    pub client_ref: String,
    pub group_order: u32,
    pub sub_label: Option<String>,
    pub question: CreateQuestionParams,
}

/// 组发布结果（新 ID 映射）
#[derive(Debug, Clone, Default)]
pub struct PublishedGroup {
    pub group_id: String,
    pub material_ids: Vec<(String, String)>,
    pub child_question_ids: Vec<(String, String)>,
}

// ============================================================================
// 服务实现
// ============================================================================

/// 题目包服务入口
pub struct QuestionBundleService;

impl QuestionBundleService {
    pub fn project_csv_for_exam(
        vfs_db: &VfsDatabase,
        exam_id: &str,
        include_study_state: bool,
    ) -> Result<String, AppError> {
        let exported = Self::export_json_v2(
            vfs_db,
            &BundleExportRequest {
                exam_id: exam_id.to_string(),
                file_path: None,
                include_assets: false,
                include_study_state,
            },
        )?;
        let json = exported
            .json
            .ok_or_else(|| AppError::internal("CSV 投影缺少题目包内容"))?;
        let bundle: QuestionBankBundle = serde_json::from_str(&json)
            .map_err(|_| AppError::internal("CSV 投影无法解析题目包内容"))?;
        Ok(project_csv_text(&bundle))
    }

    /// 导出 native JSON v2（组/材料经 `VfsQuestionGroupRepo` 读取）
    pub fn export_json_v2(
        vfs_db: &VfsDatabase,
        request: &BundleExportRequest,
    ) -> Result<BundleExportResult, AppError> {
        if let Some(path) = &request.file_path {
            validate_output_path(path)?;
        }
        let conn = vfs_db
            .get_conn_safe()
            .map_err(|e| AppError::database(format!("获取 VFS 连接失败: {}", e)))?;

        let exam =
            crate::vfs::repos::VfsExamRepo::get_exam_sheet_with_conn(&conn, &request.exam_id)
                .map_err(|e| AppError::database(format!("读取题目集失败: {}", e)))?
                .ok_or_else(|| AppError::not_found(format!("题目集不存在: {}", request.exam_id)))?;

        let mut warnings = Vec::new();
        let mut bundle = QuestionBankBundle {
            schema: BUNDLE_SCHEMA.to_string(),
            format_version: BUNDLE_FORMAT_VERSION,
            exported_at: chrono::Utc::now().to_rfc3339(),
            exam: BundleExam {
                name: exam
                    .exam_name
                    .clone()
                    .unwrap_or_else(|| "未命名题目集".into()),
                source_exam_id: Some(request.exam_id.clone()),
            },
            sources: Vec::new(),
            groups: Vec::new(),
            materials: Vec::new(),
            children: Vec::new(),
            standalone_questions: Vec::new(),
            order: Vec::new(),
            tombstones: Vec::new(),
            assets: Vec::new(),
        };

        // 1) 组与材料（经 VfsQuestionGroupRepo 读取；任何错误都不静默降级）
        let groups = read_groups_from_repo(&conn, &request.exam_id)?;
        let mut child_owner: HashMap<String, (String, u32, Option<String>, bool)> = HashMap::new();
        let mut group_source_refs = Vec::new();
        for g in &groups {
            let deleted = g.deleted_at.is_some();
            if deleted {
                bundle.tombstones.push(BundleTombstone {
                    kind: BundleTombstoneKind::Group,
                    ref_id: g.group_id.clone(),
                    deleted_at: g.deleted_at.clone(),
                });
            }
            if !deleted {
                bundle.groups.push(BundleGroup {
                    group_ref: g.group_id.clone(),
                    group_type: g.group_type,
                    title: g.title.clone(),
                    source_ref: g.source_ref.clone(),
                    version: g.version,
                });
                group_source_refs.push((g.source_ref.clone(), g.source_type.clone(), None));
            }
            for m in &g.materials {
                if m.deleted_at.is_some() {
                    bundle.tombstones.push(BundleTombstone {
                        kind: BundleTombstoneKind::Material,
                        ref_id: m.material_id.clone(),
                        deleted_at: m.deleted_at.clone(),
                    });
                    continue;
                }
                if deleted {
                    continue;
                }
                let mut images = Vec::new();
                for img in &m.images {
                    images.push(image_ref_for_asset(
                        &mut bundle.assets,
                        &mut warnings,
                        vfs_db.blobs_dir(),
                        img,
                        request.include_assets,
                    )?);
                }
                bundle.materials.push(BundleMaterial {
                    material_ref: m.material_id.clone(),
                    group_ref: g.group_id.clone(),
                    order_index: m.order_index,
                    content_type: m.content_type,
                    title: m.title.clone(),
                    text: m.text.clone(),
                    images,
                    source_id: None,
                    source_ref: m.source_ref.clone(),
                    page_index: m.page_index,
                    bbox: m.bbox.clone(),
                });
            }
            for c in &g.children {
                if c.deleted_at.is_some() {
                    bundle.tombstones.push(BundleTombstone {
                        kind: BundleTombstoneKind::Question,
                        ref_id: c.question_id.clone(),
                        deleted_at: c.deleted_at.clone(),
                    });
                    continue;
                }
                child_owner.insert(
                    c.question_id.clone(),
                    (
                        g.group_id.clone(),
                        c.group_order,
                        c.sub_label.clone(),
                        false,
                    ),
                );
            }
        }
        // 去重 sources
        {
            let mut seen: BTreeSet<String> = BTreeSet::new();
            for (source_ref, source_type, page_index) in group_source_refs {
                if let Some(sr) = source_ref {
                    if seen.insert(sr.clone()) {
                        bundle.sources.push(BundleSource {
                            source_id: sr.clone(),
                            source_type,
                            source_ref: Some(sr),
                            page_index,
                        });
                    }
                }
            }
        }

        // 2) 题目（一次分页拉全量）
        let questions = list_all_questions(&conn, &request.exam_id)?;
        let mut order: Vec<String> = Vec::new();
        for q in &questions {
            if let Some((group_ref, group_order, sub_label, _)) = child_owner.get(&q.id) {
                let bundle_q = build_bundle_question(
                    vfs_db,
                    &mut bundle.assets,
                    &mut warnings,
                    q,
                    request.include_assets,
                    request.include_study_state,
                )?;
                bundle.children.push(BundleChild {
                    client_ref: q.id.clone(),
                    group_ref: group_ref.clone(),
                    group_order: *group_order,
                    sub_label: sub_label.clone(),
                    question: bundle_q,
                });
                order.push(q.id.clone());
            } else {
                let bundle_q = build_bundle_question(
                    vfs_db,
                    &mut bundle.assets,
                    &mut warnings,
                    q,
                    request.include_assets,
                    request.include_study_state,
                )?;
                bundle.standalone_questions.push(BundleStandaloneQuestion {
                    client_ref: q.id.clone(),
                    question: bundle_q,
                });
                order.push(q.id.clone());
            }
        }
        // 子题按 group_order 排序后再拼独立题，保证稳定顺序
        let order_index: HashMap<String, usize> = order
            .iter()
            .enumerate()
            .map(|(i, r)| (r.clone(), i))
            .collect();
        bundle.children.sort_by(|a, b| {
            (
                a.group_ref.clone(),
                a.group_order,
                order_index
                    .get(&a.client_ref)
                    .copied()
                    .unwrap_or(usize::MAX),
            )
                .cmp(&(
                    b.group_ref.clone(),
                    b.group_order,
                    order_index
                        .get(&b.client_ref)
                        .copied()
                        .unwrap_or(usize::MAX),
                ))
        });
        let mut final_order: Vec<String> = Vec::new();
        let mut seen = HashSet::new();
        for g in &bundle.groups {
            for c in bundle
                .children
                .iter()
                .filter(|c| c.group_ref == g.group_ref)
            {
                if seen.insert(c.client_ref.clone()) {
                    final_order.push(c.client_ref.clone());
                }
            }
        }
        for s in &bundle.standalone_questions {
            if seen.insert(s.client_ref.clone()) {
                final_order.push(s.client_ref.clone());
            }
        }
        for r in order {
            if seen.insert(r.clone()) {
                final_order.push(r);
            }
        }
        bundle.order = final_order;

        // 2.5) 软删记录（tombstones）：同 exam 只读回读，避免回灌复活被删数据
        for t in read_tombstones_conn(&conn, &request.exam_id)? {
            if !bundle
                .tombstones
                .iter()
                .any(|existing| existing.kind == t.kind && existing.ref_id == t.ref_id)
            {
                bundle.tombstones.push(t);
            }
        }

        if !request.include_assets {
            warnings.push(BundleWarning {
                code: "assets_not_included".to_string(),
                message: "已按请求排除资产字节；该包不能无损往返图片".to_string(),
                ref_id: None,
            });
        }
        let round_trip_safe = request.include_assets;

        validate_bundle_inner(&bundle, request.include_assets)?;
        let json = serde_json::to_string_pretty(&bundle)
            .map_err(|e| AppError::internal(format!("序列化题目包失败: {}", e)))?;
        let sha256 = VfsBlobRepo::compute_hash(json.as_bytes());

        let (file_path, file_size) = match &request.file_path {
            Some(path) => {
                std::fs::write(path, json.as_bytes())
                    .map_err(|e| AppError::file_system(format!("写入题目包失败: {}", e)))?;
                (Some(path.clone()), json.len() as u64)
            }
            None => (None, json.len() as u64),
        };

        log::info!(
            "[QBankBundle] 导出 exam={} groups={} materials={} children={} standalone={} assets={} size={}",
            request.exam_id,
            bundle.groups.len(),
            bundle.materials.len(),
            bundle.children.len(),
            bundle.standalone_questions.len(),
            bundle.assets.len(),
            file_size
        );

        Ok(BundleExportResult {
            schema: BUNDLE_SCHEMA.to_string(),
            format_version: BUNDLE_FORMAT_VERSION,
            file_path,
            file_size,
            sha256,
            round_trip_safe,
            groups: bundle.groups.len() as u32,
            materials: bundle.materials.len() as u32,
            questions: bundle.children.len() as u32,
            standalone_questions: bundle.standalone_questions.len() as u32,
            assets: bundle.assets.len() as u32,
            tombstones: bundle.tombstones.len() as u32,
            warnings,
            json: if request.file_path.is_none() {
                Some(json)
            } else {
                None
            },
        })
    }

    /// 导入 native JSON v2（或 v1 独立题数组）
    pub fn import_json_v2(
        vfs_db: &VfsDatabase,
        request: &BundleImportRequest,
    ) -> Result<BundleImportResult, AppError> {
        let parsed = parse_bundle(request.bundle.clone())?;
        let mut warnings = Vec::new();
        let legacy_v1 = matches!(parsed, ParsedBundle::LegacyV1(_));

        let (bundle, questions_only) = match parsed {
            ParsedBundle::V2(b) => {
                validate_bundle(&b)?;
                (Some(b), Vec::new())
            }
            ParsedBundle::LegacyV1(items) => {
                if items.is_empty() {
                    return Err(AppError::validation("v1 独立题数组为空"));
                }
                (None, items)
            }
        };

        if !request.include_study_state && (!questions_only.is_empty() || bundle.is_some()) {
            warnings.push(BundleWarning {
                code: "study_state_skipped".to_string(),
                message: "按请求忽略学习状态快照".to_string(),
                ref_id: None,
            });
        }

        // 目标解析：merge 需要既有题目集；new_exam 需要名字
        let (target_exam_name, merge_exam_id) = match &request.target {
            BundleImportTarget::NewExam { name } => {
                let some_name = name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| bundle.as_ref().map(|b| b.exam.name.clone()))
                    .ok_or_else(|| AppError::validation("new_exam 目标缺少题目集名称"))?;
                (some_name, None)
            }
            BundleImportTarget::Merge { exam_id } => {
                require_non_empty("target.exam_id", exam_id)?;
                (String::new(), Some(exam_id.clone()))
            }
        };

        let conn = vfs_db
            .get_conn_safe()
            .map_err(|e| AppError::database(format!("获取 VFS 连接失败: {}", e)))?;

        // merge 目标存在性检查（dry_run 也要检查，避免“看起来可导入”）
        if let Some(exam_id) = &merge_exam_id {
            let exists = crate::vfs::repos::VfsExamRepo::get_exam_sheet_with_conn(&conn, exam_id)
                .map_err(|e| AppError::database(format!("读取题目集失败: {}", e)))?
                .is_some();
            if !exists {
                return Err(AppError::not_found(format!(
                    "合并目标题目集不存在: {}",
                    exam_id
                )));
            }
        }

        // 资产预解码（严格校验；无论 dry_run 都做，让校验真实）
        let mut decoded_assets: HashMap<String, StoredAsset> = HashMap::new();
        if let Some(b) = &bundle {
            for asset in &b.assets {
                let stored = decode_asset(asset)?;
                decoded_assets.insert(asset.asset_id.clone(), stored);
            }
        }

        // 半组阻断：引用关系在写入前全部成立；tombstone 命中的组/材料/题直接跳过
        let (skip_groups, skip_materials, skip_questions) = match &bundle {
            Some(b) => tombstone_sets(b),
            None => (HashSet::new(), HashSet::new(), HashSet::new()),
        };

        // merge 去重基线：既有题目 source_ref 与既有题组 source_ref
        // （保留用户编辑，不覆盖；组按来源身份幂等）
        let (existing_source_refs, existing_group_source_refs): (HashSet<String>, HashSet<String>) =
            if let Some(exam_id) = &merge_exam_id {
                let questions: HashSet<String> = list_all_questions(&conn, exam_id)?
                    .into_iter()
                    .filter_map(|q| q.source_ref)
                    .collect();
                let groups: HashSet<String> = read_groups_from_repo(&conn, exam_id)?
                    .into_iter()
                    .filter_map(|g| g.source_ref)
                    .collect();
                (questions, groups)
            } else {
                (HashSet::new(), HashSet::new())
            };

        let mut result = BundleImportResult {
            schema: BUNDLE_SCHEMA.to_string(),
            format_version: BUNDLE_FORMAT_VERSION,
            dry_run: request.dry_run,
            committed: false,
            exam_id: merge_exam_id.clone(),
            legacy_v1,
            groups_created: 0,
            materials_created: 0,
            questions_created: 0,
            questions_skipped: 0,
            assets_created: 0,
            assets_deduped: 0,
            tombstones_applied: (skip_groups.len() + skip_materials.len() + skip_questions.len())
                as u32,
            id_map: Vec::new(),
            warnings,
        };

        if request.dry_run {
            // 只统计将发生的写入，不建题目集、不落资产。
            // 与真实写入路径使用同一套跳过规则（墓碑 / 同 source_ref / 整组跳过）。
            if let Some(b) = &bundle {
                let mut planned_groups = 0u32;
                let mut planned_materials = 0u32;
                let mut planned_questions = 0u32;
                let mut skipped_questions = 0u32;
                for group in &b.groups {
                    if skip_groups.contains(&group.group_ref) {
                        skipped_questions += b
                            .children
                            .iter()
                            .filter(|c| c.group_ref == group.group_ref)
                            .count() as u32;
                        continue;
                    }
                    if group
                        .source_ref
                        .as_ref()
                        .map(|r| existing_group_source_refs.contains(r))
                        .unwrap_or(false)
                    {
                        skipped_questions += b
                            .children
                            .iter()
                            .filter(|c| c.group_ref == group.group_ref)
                            .count() as u32;
                        continue;
                    }
                    let children: Vec<&BundleChild> = b
                        .children
                        .iter()
                        .filter(|c| c.group_ref == group.group_ref)
                        .filter(|c| !skip_questions.contains(&c.client_ref))
                        .collect();
                    let tombstoned = b
                        .children
                        .iter()
                        .filter(|c| c.group_ref == group.group_ref)
                        .count()
                        - children.len();
                    skipped_questions += tombstoned as u32;
                    if children.is_empty() {
                        continue;
                    }
                    if children
                        .iter()
                        .any(|c| source_ref_duplicated(&c.question, &existing_source_refs))
                    {
                        skipped_questions += children.len() as u32;
                        continue;
                    }
                    planned_groups += 1;
                    planned_materials += b
                        .materials
                        .iter()
                        .filter(|m| {
                            m.group_ref == group.group_ref
                                && !skip_materials.contains(&m.material_ref)
                        })
                        .count() as u32;
                    planned_questions += children.len() as u32;
                }
                for s in &b.standalone_questions {
                    if skip_questions.contains(&s.client_ref)
                        || source_ref_duplicated(&s.question, &existing_source_refs)
                    {
                        skipped_questions += 1;
                    } else {
                        planned_questions += 1;
                    }
                }
                result.groups_created = planned_groups;
                result.materials_created = planned_materials;
                result.questions_created = planned_questions;
                result.questions_skipped = skipped_questions;
            } else {
                let mut planned = 0u32;
                let mut skipped = 0u32;
                for q in &questions_only {
                    if source_ref_duplicated(q, &existing_source_refs) {
                        skipped += 1;
                    } else {
                        planned += 1;
                    }
                }
                result.questions_created = planned;
                result.questions_skipped = skipped;
            }
            result.assets_created = decoded_assets.len() as u32;
            return Ok(result);
        }

        // ===== 真正写入：单事务 =====
        conn.execute("BEGIN IMMEDIATE", [])
            .map_err(|e| AppError::database(format!("开启导入事务失败: {}", e)))?;

        let write_result = (|| -> Result<(), AppError> {
            let exam_id = match &merge_exam_id {
                Some(id) => id.clone(),
                None => create_target_exam(&conn, &target_exam_name)?,
            };
            result.exam_id = Some(exam_id.clone());

            // 1) 资产：先按 hash 去重落库，供图片引用使用
            let mut asset_files: HashMap<String, QuestionImage> = HashMap::new();
            // 稳定顺序，便于日志/复现
            let mut asset_ids: Vec<&String> = decoded_assets.keys().collect();
            asset_ids.sort();
            for asset_id in asset_ids {
                let stored = &decoded_assets[asset_id];
                let existed =
                    VfsBlobRepo::blob_exists_with_conn(&conn, &stored.sha256).unwrap_or(false);
                let extension = mime_extension(&stored.mime, &stored.name);
                VfsBlobRepo::store_blob_with_conn(
                    &conn,
                    vfs_db.blobs_dir(),
                    &stored.bytes,
                    Some(stored.mime.as_str()),
                    Some(extension.as_str()),
                )
                .map_err(|e| AppError::database(format!("资产 {} 落库失败: {}", asset_id, e)))?;
                let file = VfsFileRepo::create_file_with_conn(
                    &conn,
                    &stored.sha256,
                    &stored.name,
                    stored.size as i64,
                    "image",
                    Some(&stored.mime),
                    Some(&stored.sha256),
                    None,
                )
                .map_err(|e| {
                    AppError::database(format!("资产 {} 建立 VFS 文件失败: {}", asset_id, e))
                })?;
                if existed {
                    result.assets_deduped += 1;
                } else {
                    result.assets_created += 1;
                }
                asset_files.insert(
                    asset_id.clone(),
                    QuestionImage {
                        id: file.id,
                        name: stored.name.clone(),
                        mime: stored.mime.clone(),
                        hash: stored.sha256.clone(),
                    },
                );
            }

            // 2) 组 + 材料 + 子题（每组一个原子发布，同 conn）
            if let Some(b) = &bundle {
                let mut materials_by_group: HashMap<&str, Vec<&BundleMaterial>> = HashMap::new();
                for m in &b.materials {
                    materials_by_group
                        .entry(m.group_ref.as_str())
                        .or_default()
                        .push(m);
                }
                for group in &b.groups {
                    if skip_groups.contains(&group.group_ref) {
                        continue;
                    }
                    if group
                        .source_ref
                        .as_ref()
                        .map(|r| existing_group_source_refs.contains(r))
                        .unwrap_or(false)
                    {
                        result.warnings.push(BundleWarning {
                            code: "group_skipped_existing_source_ref".to_string(),
                            message: "目标题目集已有同来源身份的题组，跳过以保持幂等".to_string(),
                            ref_id: Some(group.group_ref.clone()),
                        });
                        continue;
                    }

                    // merge 冲突：组内任一子题在目标里已有同 source_ref（用户可能已编辑）
                    // → 整组跳过，既不覆盖用户编辑，也不生成“少子题”的半组。
                    let mut group_children: Vec<&BundleChild> = b
                        .children
                        .iter()
                        .filter(|c| c.group_ref == group.group_ref)
                        .filter(|c| !skip_questions.contains(&c.client_ref))
                        .collect();
                    let tombstoned_children = b
                        .children
                        .iter()
                        .filter(|c| c.group_ref == group.group_ref)
                        .count()
                        - group_children.len();
                    result.questions_skipped += tombstoned_children as u32;
                    if group_children
                        .iter()
                        .any(|c| source_ref_duplicated(&c.question, &existing_source_refs))
                    {
                        result.questions_skipped += group_children.len() as u32;
                        result.warnings.push(BundleWarning {
                            code: "group_skipped_source_ref_conflict".to_string(),
                            message: "组内已有同来源身份的子题，整组跳过以保留用户编辑并避免半组"
                                .to_string(),
                            ref_id: Some(group.group_ref.clone()),
                        });
                        continue;
                    }
                    if group_children.is_empty() {
                        result.warnings.push(BundleWarning {
                            code: "group_skipped_no_children".to_string(),
                            message: "组内没有可发布的子题（全部被墓碑/排除），跳过整组"
                                .to_string(),
                            ref_id: Some(group.group_ref.clone()),
                        });
                        continue;
                    }

                    let mut eligible_materials: Vec<&BundleMaterial> = materials_by_group
                        .get(group.group_ref.as_str())
                        .cloned()
                        .unwrap_or_default();
                    eligible_materials.retain(|m| !skip_materials.contains(&m.material_ref));
                    eligible_materials.sort_by_key(|m| m.order_index);

                    // 顺序归一化：跳过墓碑成员后按相对顺序重排为 1..M / 1..N，
                    // 既满足 repo 的连续性约束，也不改变相对次序。
                    let mut proposed_materials = Vec::new();
                    for (idx, m) in eligible_materials.iter().enumerate() {
                        let images = m
                            .images
                            .iter()
                            .map(|r| {
                                asset_files.get(&r.asset_id).cloned().ok_or_else(|| {
                                    AppError::validation(format!(
                                        "材料 {} 的图片资产 {} 未解码",
                                        m.material_ref, r.asset_id
                                    ))
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        proposed_materials.push(ProposedMaterial {
                            material_ref: m.material_ref.clone(),
                            order_index: idx as u32 + 1,
                            content_type: m.content_type,
                            title: m.title.clone(),
                            text: m.text.clone(),
                            images,
                            source_ref: m.source_ref.clone(),
                            page_index: m.page_index,
                            bbox: m.bbox.clone(),
                        });
                    }

                    group_children.sort_by_key(|c| c.group_order);
                    let mut proposed_children = Vec::new();
                    for (idx, c) in group_children.iter().enumerate() {
                        let params = to_create_params(
                            &exam_id,
                            &c.question,
                            request.include_study_state,
                            &asset_files,
                        )?;
                        proposed_children.push(ProposedChild {
                            client_ref: c.client_ref.clone(),
                            group_order: idx as u32 + 1,
                            sub_label: c.sub_label.clone(),
                            question: params,
                        });
                    }

                    let published = publish_group_via_repo(
                        &conn,
                        &exam_id,
                        &ProposedGroup {
                            group_ref: group.group_ref.clone(),
                            group_type: group.group_type,
                            title: group.title.clone(),
                            source_type: infer_source_type(b, group),
                            source_ref: group.source_ref.clone(),
                            materials: proposed_materials,
                            children: proposed_children,
                        },
                    )?;
                    result.groups_created += 1;
                    result.id_map.push(BundleIdMapEntry {
                        client_ref: group.group_ref.clone(),
                        kind: "group".to_string(),
                        new_id: published.group_id.clone(),
                    });
                    result.materials_created += published.material_ids.len() as u32;
                    for (material_ref, new_id) in published.material_ids {
                        result.id_map.push(BundleIdMapEntry {
                            client_ref: material_ref,
                            kind: "material".to_string(),
                            new_id,
                        });
                    }
                    for (client_ref, new_id) in published.child_question_ids {
                        result.questions_created += 1;
                        if request.include_study_state {
                            if let Some(child) =
                                b.children.iter().find(|c| c.client_ref == client_ref)
                            {
                                apply_study_state(&conn, &new_id, child.question.study.as_ref())?;
                            }
                        }
                        result.id_map.push(BundleIdMapEntry {
                            client_ref,
                            kind: "question".to_string(),
                            new_id,
                        });
                    }
                }

                // 3) 独立题
                for s in &b.standalone_questions {
                    if skip_questions.contains(&s.client_ref) {
                        result.questions_skipped += 1;
                        continue;
                    }
                    if source_ref_duplicated(&s.question, &existing_source_refs) {
                        result.questions_skipped += 1;
                        result.warnings.push(BundleWarning {
                            code: "existing_source_ref_skipped".to_string(),
                            message: "目标题目集已有相同来源身份的题目，保留既有内容（含用户编辑）"
                                .to_string(),
                            ref_id: Some(s.client_ref.clone()),
                        });
                        continue;
                    }
                    let params = to_create_params(
                        &exam_id,
                        &s.question,
                        request.include_study_state,
                        &asset_files,
                    )?;
                    let created = VfsQuestionRepo::create_question_with_conn(&conn, &params)
                        .map_err(|e| AppError::database(format!("写入独立题失败: {}", e)))?;
                    result.questions_created += 1;
                    if request.include_study_state {
                        apply_study_state(&conn, &created.id, s.question.study.as_ref())?;
                    }
                    result.id_map.push(BundleIdMapEntry {
                        client_ref: s.client_ref.clone(),
                        kind: "question".to_string(),
                        new_id: created.id,
                    });
                }
            } else {
                // v1 独立题数组
                for (idx, q) in questions_only.iter().enumerate() {
                    if source_ref_duplicated(q, &existing_source_refs) {
                        result.questions_skipped += 1;
                        continue;
                    }
                    let params =
                        to_create_params(&exam_id, q, request.include_study_state, &asset_files)?;
                    let created = VfsQuestionRepo::create_question_with_conn(&conn, &params)
                        .map_err(|e| AppError::database(format!("写入 v1 题目失败: {}", e)))?;
                    result.questions_created += 1;
                    if request.include_study_state {
                        apply_study_state(&conn, &created.id, q.study.as_ref())?;
                    }
                    result.id_map.push(BundleIdMapEntry {
                        client_ref: format!("v1:{}", idx + 1),
                        kind: "question".to_string(),
                        new_id: created.id,
                    });
                }
            }

            // 统计表与导入结果保持一致（同事务内重算，不依赖后台任务）
            VfsQuestionRepo::refresh_stats_with_conn(&conn, &exam_id)
                .map_err(|e| AppError::database(format!("刷新题目集统计失败: {}", e)))?;

            conn.execute("COMMIT", [])
                .map_err(|e| AppError::database(format!("提交导入事务失败: {}", e)))?;
            result.committed = true;
            Ok(())
        })();

        match write_result {
            Ok(()) => {
                log::info!(
                    "[QBankBundle] 导入完成 exam={:?} groups={} materials={} questions={} skipped={} assets={}",
                    result.exam_id,
                    result.groups_created,
                    result.materials_created,
                    result.questions_created,
                    result.questions_skipped,
                    result.assets_created
                );
                Ok(result)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                log::warn!("[QBankBundle] 导入回滚: {}", e.message);
                Err(e)
            }
        }
    }
}

fn infer_source_type(bundle: &QuestionBankBundle, group: &BundleGroup) -> SourceType {
    bundle
        .sources
        .iter()
        .find(|s| {
            group
                .source_ref
                .as_deref()
                .map(|r| r == s.source_id.as_str() || Some(r) == s.source_ref.as_deref())
                .unwrap_or(false)
        })
        .map(|s| s.source_type.clone())
        .unwrap_or(SourceType::Imported)
}

fn tombstone_sets(
    bundle: &QuestionBankBundle,
) -> (HashSet<String>, HashSet<String>, HashSet<String>) {
    let mut groups = HashSet::new();
    let mut materials = HashSet::new();
    let mut questions = HashSet::new();
    for t in &bundle.tombstones {
        match t.kind {
            BundleTombstoneKind::Group => {
                groups.insert(t.ref_id.clone());
                // 组删除时其成员也视为已删除，避免复活半组
                for g in bundle.groups.iter().filter(|g| g.group_ref == t.ref_id) {
                    for m in bundle
                        .materials
                        .iter()
                        .filter(|m| m.group_ref == g.group_ref)
                    {
                        materials.insert(m.material_ref.clone());
                    }
                    for c in bundle
                        .children
                        .iter()
                        .filter(|c| c.group_ref == g.group_ref)
                    {
                        questions.insert(c.client_ref.clone());
                    }
                }
            }
            BundleTombstoneKind::Material => {
                materials.insert(t.ref_id.clone());
            }
            BundleTombstoneKind::Question => {
                questions.insert(t.ref_id.clone());
            }
        }
    }
    (groups, materials, questions)
}

fn source_ref_duplicated(q: &BundleQuestion, existing: &HashSet<String>) -> bool {
    q.source_ref
        .as_ref()
        .map(|r| existing.contains(r))
        .unwrap_or(false)
}

fn validate_output_path(path: &str) -> Result<(), AppError> {
    if path.trim().is_empty() {
        return Err(AppError::validation("导出路径不能为空"));
    }
    if Path::new(path)
        .components()
        .any(|c| c == Component::ParentDir)
    {
        return Err(AppError::validation("导出路径不允许包含 '..'"));
    }
    Ok(())
}

fn list_all_questions(conn: &Connection, exam_id: &str) -> Result<Vec<Question>, AppError> {
    let mut out = Vec::new();
    let mut page = 1u32;
    let page_size = 500u32;
    loop {
        let result = VfsQuestionRepo::list_questions_with_conn(
            conn,
            exam_id,
            &Default::default(),
            page,
            page_size,
        )
        .map_err(|e| AppError::database(format!("读取题目失败: {}", e)))?;
        out.extend(result.questions);
        if !result.has_more {
            break;
        }
        page += 1;
        if page > 10_000 {
            return Err(AppError::database("题目分页异常：超过上限"));
        }
    }
    Ok(out)
}

fn image_ref_for_asset(
    assets: &mut Vec<BundleAsset>,
    warnings: &mut Vec<BundleWarning>,
    blobs_dir: &Path,
    img: &QuestionImage,
    include_assets: bool,
) -> Result<BundleImageRef, AppError> {
    let asset_id = img.hash.clone();
    if include_assets {
        if !assets.iter().any(|a| a.asset_id == asset_id) {
            let stored =
                read_blob_for_export(blobs_dir, &asset_id, &img.hash, &img.name, &img.mime)?;
            assets.push(encode_asset(stored));
        }
    } else {
        warnings.push(BundleWarning {
            code: "asset_omitted".to_string(),
            message: format!("未内嵌资产 {}", asset_id),
            ref_id: Some(asset_id.clone()),
        });
    }
    Ok(BundleImageRef {
        asset_id,
        name: img.name.clone(),
        mime: img.mime.clone(),
    })
}

fn build_bundle_question(
    vfs_db: &VfsDatabase,
    assets: &mut Vec<BundleAsset>,
    warnings: &mut Vec<BundleWarning>,
    q: &Question,
    include_assets: bool,
    include_study_state: bool,
) -> Result<BundleQuestion, AppError> {
    let mut images = Vec::new();
    for img in &q.images {
        images.push(image_ref_for_asset(
            assets,
            warnings,
            vfs_db.blobs_dir(),
            img,
            include_assets,
        )?);
    }
    Ok(BundleQuestion {
        content: q.content.clone(),
        options: q.options.clone(),
        answer: q.answer.clone(),
        explanation: q.explanation.clone(),
        structured_data: q.structured_data.clone(),
        question_type: Some(q.question_type.clone()),
        difficulty: q.difficulty.clone(),
        tags: q.tags.clone(),
        source_type: Some(q.source_type.clone()),
        source_ref: q.source_ref.clone(),
        question_label: q.question_label.clone(),
        images,
        study: if include_study_state {
            Some(BundleStudyState {
                status: q.status.clone(),
                user_answer: q.user_answer.clone(),
                is_correct: q.is_correct,
                user_note: q.user_note.clone(),
                is_favorite: q.is_favorite,
                is_bookmarked: q.is_bookmarked,
                attempt_count: q.attempt_count,
                correct_count: q.correct_count,
                last_attempt_at: q.last_attempt_at.clone(),
            })
        } else {
            None
        },
    })
}

fn to_create_params(
    exam_id: &str,
    q: &BundleQuestion,
    include_study_state: bool,
    asset_files: &HashMap<String, QuestionImage>,
) -> Result<CreateQuestionParams, AppError> {
    let mut images = Vec::new();
    for r in &q.images {
        let img = asset_files.get(&r.asset_id).cloned().ok_or_else(|| {
            AppError::validation(format!(
                "题目引用的图片资产 {} 未解码（不能创建缺图题目）",
                r.asset_id
            ))
        })?;
        images.push(img);
    }
    // 学习状态目前只能通过 create 之后的 update 应用；这里只保留教学内容，
    // study 快照在下面单独返回给调用方（见 apply 路径）。
    let _ = include_study_state;
    Ok(CreateQuestionParams {
        exam_id: exam_id.to_string(),
        card_id: None,
        question_label: q.question_label.clone(),
        content: q.content.clone(),
        options: q.options.clone(),
        answer: q.answer.clone(),
        explanation: q.explanation.clone(),
        structured_data: q.structured_data.clone(),
        question_type: q.question_type.clone(),
        difficulty: q.difficulty.clone(),
        tags: if q.tags.is_empty() {
            None
        } else {
            Some(q.tags.clone())
        },
        source_type: q.source_type.clone().or(Some(SourceType::Imported)),
        source_ref: q.source_ref.clone(),
        images: if images.is_empty() {
            None
        } else {
            Some(images)
        },
        parent_id: None,
    })
}

/// 导出软删记录（tombstones）：只读、限定同 `exam_id`，不跨题目集。
///
/// repo 没有暴露软删行读取，这里用最小的只读 SELECT 补齐回读；
/// 只用到 `questions.exam_id` / `question_groups.exam_id` /
/// `question_group_materials.exam_id` 三个作用域列，不碰其他表或其他考试。
fn read_tombstones_conn(
    conn: &Connection,
    exam_id: &str,
) -> Result<Vec<BundleTombstone>, AppError> {
    let mut out = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut push = |kind: BundleTombstoneKind, id: String, deleted_at: Option<String>| {
        let key = (format!("{:?}", kind), id.clone());
        if seen.insert(key) {
            out.push(BundleTombstone {
                kind,
                ref_id: id,
                deleted_at,
            });
        }
    };

    let mut stmt = conn
        .prepare(
            "SELECT id, deleted_at FROM question_groups
             WHERE exam_id = ?1 AND deleted_at IS NOT NULL",
        )
        .map_err(sql_err("读取已删除题组失败"))?;
    let rows = stmt
        .query_map(rusqlite::params![exam_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sql_err("读取已删除题组失败"))?;
    for row in rows {
        let (id, deleted_at) = row.map_err(sql_err("读取已删除题组失败"))?;
        push(BundleTombstoneKind::Group, id, deleted_at);
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, deleted_at FROM question_group_materials
             WHERE exam_id = ?1 AND deleted_at IS NOT NULL",
        )
        .map_err(sql_err("读取已删除材料失败"))?;
    let rows = stmt
        .query_map(rusqlite::params![exam_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sql_err("读取已删除材料失败"))?;
    for row in rows {
        let (id, deleted_at) = row.map_err(sql_err("读取已删除材料失败"))?;
        push(BundleTombstoneKind::Material, id, deleted_at);
    }

    let mut stmt = conn
        .prepare(
            "SELECT id, deleted_at FROM questions
             WHERE exam_id = ?1 AND deleted_at IS NOT NULL",
        )
        .map_err(sql_err("读取已删除题目失败"))?;
    let rows = stmt
        .query_map(rusqlite::params![exam_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(sql_err("读取已删除题目失败"))?;
    for row in rows {
        let (id, deleted_at) = row.map_err(sql_err("读取已删除题目失败"))?;
        push(BundleTombstoneKind::Question, id, deleted_at);
    }

    Ok(out)
}

/// 只对本次事务中新建的题目恢复学习快照，不覆盖合并目标的已有题目。
fn apply_study_state(
    conn: &Connection,
    question_id: &str,
    study: Option<&BundleStudyState>,
) -> Result<(), AppError> {
    let Some(study) = study else {
        return Ok(());
    };
    if study.attempt_count < 0
        || study.correct_count < 0
        || study.correct_count > study.attempt_count
    {
        return Err(AppError::validation("学习快照的答题次数或正确次数无效"));
    }
    let state = crate::vfs::repos::question_repo::QuestionLearningState {
        status: study.status.clone(),
        user_answer: study.user_answer.clone(),
        is_correct: study.is_correct,
        attempt_count: study.attempt_count,
        correct_count: study.correct_count,
        last_attempt_at: study.last_attempt_at.clone(),
        user_note: study.user_note.clone(),
        is_favorite: study.is_favorite,
        is_bookmarked: study.is_bookmarked,
    };
    VfsQuestionRepo::restore_learning_state_with_conn(conn, question_id, &state, None)
        .map(|_| ())
        .map_err(|e| AppError::database(format!("恢复学习状态失败: {e}")))
}

fn create_target_exam(conn: &Connection, name: &str) -> Result<String, AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    let sheet = crate::vfs::repos::VfsExamRepo::create_exam_sheet_with_conn(
        conn,
        VfsCreateExamSheetParams {
            exam_name: Some(name.to_string()),
            temp_id: format!("bundle_import_{}", nanoid::nanoid!(10)),
            metadata_json: serde_json::json!({
                "source": "qbank_bundle_v2",
                "imported_at": now,
            }),
            preview_json: serde_json::json!({"questions": [], "total": 0}),
            status: "ready".to_string(),
            folder_id: None,
        },
    )
    .map_err(|e| AppError::database(format!("创建导入题目集失败: {}", e)))?;
    Ok(sheet.id)
}

/// 读取题目集的全部可见组（组/材料/子题链接），直接调用 `VfsQuestionGroupRepo`。
fn read_groups_from_repo(conn: &Connection, exam_id: &str) -> Result<Vec<GroupSnapshot>, AppError> {
    use crate::vfs::repos::question_group_repo::{MaterialContentType, QuestionGroupType};
    let mut out = Vec::new();
    let mut page = 1u32;
    loop {
        let page_result = VfsQuestionGroupRepo::list_groups_with_conn(conn, exam_id, page, 100)
            .map_err(db_err("读取题组失败"))?;
        for g in &page_result.groups {
            let materials = VfsQuestionGroupRepo::get_materials_with_conn(conn, &g.id)
                .map_err(db_err("读取题组材料失败"))?;
            let siblings = VfsQuestionGroupRepo::get_siblings_with_conn(conn, &g.id)
                .map_err(db_err("读取组内子题失败"))?;
            out.push(GroupSnapshot {
                group_id: g.id.clone(),
                group_type: bundle_group_type(
                    g.group_type == QuestionGroupType::Reading,
                    g.group_type == QuestionGroupType::SharedCondition,
                ),
                title: g.title.clone(),
                source_type: g.source_type.clone(),
                source_ref: g.source_ref.clone(),
                version: g.version.max(0) as u32,
                deleted_at: g.deleted_at.clone(),
                materials: materials
                    .into_iter()
                    .map(|m| MaterialSnapshot {
                        material_id: m.id,
                        order_index: m.order_index.max(0) as u32,
                        content_type: match m.content_type {
                            MaterialContentType::Text => BundleMaterialContentType::Text,
                            MaterialContentType::Formula => BundleMaterialContentType::Formula,
                            MaterialContentType::Figure => BundleMaterialContentType::Figure,
                            MaterialContentType::Mixed => BundleMaterialContentType::Mixed,
                        },
                        title: m.title,
                        text: m.text,
                        images: m.images,
                        source_ref: m.source_ref,
                        page_index: m.page_index.map(|v| v.max(0) as u32),
                        bbox: m.bbox.map(|b| serde_json::json!(b)),
                        deleted_at: m.deleted_at,
                    })
                    .collect(),
                children: siblings
                    .into_iter()
                    .map(|s| ChildLink {
                        question_id: s.id,
                        group_order: s.group_order.unwrap_or(0).max(0) as u32,
                        sub_label: s.sub_label,
                        deleted_at: None,
                    })
                    .collect(),
            });
        }
        if !page_result.has_more {
            break;
        }
        page += 1;
        if page > 10_000 {
            return Err(AppError::database("题组分页异常：超过上限"));
        }
    }
    Ok(out)
}

/// 原子发布一个组，直接调用 `VfsQuestionGroupRepo::publish_group_bundle_with_conn`。
fn publish_group_via_repo(
    conn: &Connection,
    exam_id: &str,
    group: &ProposedGroup,
) -> Result<PublishedGroup, AppError> {
    use crate::vfs::repos::question_group_repo::{
        MaterialContentType, NewMaterial, NewQuestionGroup, ProposedChildQuestion,
        ProposedGroupBundle, QuestionGroupType,
    };

    let materials = group
        .materials
        .iter()
        .map(|m| {
            Ok::<NewMaterial, AppError>(NewMaterial {
                // 发布器会回填真实 group_id；空串是允许的占位
                group_id: String::new(),
                order_index: m.order_index as i32,
                content_type: match m.content_type {
                    BundleMaterialContentType::Text => MaterialContentType::Text,
                    BundleMaterialContentType::Formula => MaterialContentType::Formula,
                    BundleMaterialContentType::Figure => MaterialContentType::Figure,
                    BundleMaterialContentType::Mixed => MaterialContentType::Mixed,
                },
                title: m.title.clone(),
                text: m.text.clone(),
                images: m.images.clone(),
                source_ref: m.source_ref.clone(),
                page_index: m.page_index.map(|v| v as i32),
                bbox: m.bbox.as_ref().map(parse_bbox).transpose()?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let children = group
        .children
        .iter()
        .map(|c| ProposedChildQuestion {
            client_ref: Some(c.client_ref.clone()),
            question: c.question.clone(),
            sub_label: c.sub_label.clone(),
            group_order: c.group_order as i32,
        })
        .collect();

    let proposal = ProposedGroupBundle {
        group: NewQuestionGroup {
            exam_id: exam_id.to_string(),
            group_type: match group.group_type {
                BundleGroupType::Reading => QuestionGroupType::Reading,
                BundleGroupType::SharedCondition => QuestionGroupType::SharedCondition,
                BundleGroupType::Other => QuestionGroupType::Other,
            },
            title: group.title.clone(),
            source_type: Some(group.source_type.clone()),
            source_ref: group.source_ref.clone(),
            import_baseline: None,
            publish_state: None,
        },
        materials,
        children,
    };

    let published = VfsQuestionGroupRepo::publish_group_bundle_with_conn(conn, &proposal)
        .map_err(|e| AppError::database(format!("发布题组失败: {}", e)))?;

    // 回执按 group_order / order_index 映射回 client_ref（repo 不把 client_ref 落库）
    let mut child_ids = Vec::new();
    for sibling in &published.siblings {
        let Some(order) = sibling.group_order else {
            continue;
        };
        if let Some(child) = group
            .children
            .iter()
            .find(|c| c.group_order as i32 == order)
        {
            child_ids.push((child.client_ref.clone(), sibling.id.clone()));
        }
    }
    let mut material_ids = Vec::new();
    for material in &published.materials {
        if let Some(m) = group
            .materials
            .iter()
            .find(|m| m.order_index as i32 == material.order_index)
        {
            material_ids.push((m.material_ref.clone(), material.id.clone()));
        }
    }

    Ok(PublishedGroup {
        group_id: published.group.id,
        material_ids,
        child_question_ids: child_ids,
    })
}

fn db_err(context: &'static str) -> impl Fn(crate::vfs::error::VfsError) -> AppError {
    move |e| AppError::database(format!("{}: {}", context, e))
}

fn sql_err(context: &'static str) -> impl Fn(rusqlite::Error) -> AppError {
    move |e| AppError::database(format!("{}: {}", context, e))
}

fn bundle_group_type(reading: bool, shared: bool) -> BundleGroupType {
    if reading {
        BundleGroupType::Reading
    } else if shared {
        BundleGroupType::SharedCondition
    } else {
        BundleGroupType::Other
    }
}

fn parse_bbox(value: &serde_json::Value) -> Result<[f64; 4], AppError> {
    let arr = value
        .as_array()
        .ok_or_else(|| AppError::validation("bbox 必须是 [x1,y1,x2,y2] 数组"))?;
    if arr.len() != 4 {
        return Err(AppError::validation("bbox 必须包含 4 个数值"));
    }
    let mut out = [0f64; 4];
    for (i, v) in arr.iter().enumerate() {
        out[i] = v
            .as_f64()
            .ok_or_else(|| AppError::validation("bbox 元素必须是数值"))?;
    }
    Ok(out)
}
