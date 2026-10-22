//! Issue #491：typed 材料/子题组装与题组原子发布。
//!
//! 本模块把「材料块 + 子题块 + 显式关系提案」组装成 `GroupPlan`，并提供题组
//! 关系写入的唯一实现（[`RepoGroupPublisher`]，由 `#490` 的发布事务调用）。
//! 职责边界刻意收窄：
//!
//! * **纯组装**（[`assemble_groups`] / [`proposals_from_hints`]）：不读写数据库，
//!   只消费 typed 块与显式证据提案，输出可发布的 [`GroupPlan`]、阻断冲突
//!   （[`GroupConflict`]）和未消费材料（[`UnconsumedMaterial`]）。
//! * **同事务发布**（[`publish_group_plan`] + [`RepoGroupPublisher`]）：调用方把
//!   自己已经打开的 `rusqlite::Transaction` 交给本模块；[`RepoGroupPublisher`] 在
//!   该事务内把组/材料/子题转成 `ProposedGroupBundle` 并调用
//!   `VfsQuestionGroupRepo::publish_group_bundle_with_conn` 的 staging → complete
//!   流程。本模块绝不另开连接、绝不事后回写关系。`#490` 仍须在发布事务里调用它。
//!
//! 硬性不变量（全部由代码判定，不交给模型自证）：
//!
//! 1. 材料块、组头块永远不会变成 `question`；组头也不进子题列表。
//! 2. 子题只能引用真实存在的 `ImportBlockKind::Question` 块，不凭空创造子题。
//! 3. 不按「最近的上一篇文章」猜材料；材料必须被提案显式引用，否则进入
//!    未消费材料报告。两篇文章候选、跨章节、同一身份不同载荷一律整组复核。
//! 4. 一组内任一应有子题缺失/角色不符时，整组不完整，不得发布半组。
//! 5. 同一来源身份（题目集 + 文档 + 命名空间 + source id）幂等；软删除/用户排除
//!    身份不因重导而复活（由发布层 tombstone 查询保证，本模块只提供稳定 key）。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::figure_extractor::QuestionWithFigures;
use crate::llm_structurer::StructuredQuestion;
use crate::vfs::repos::{
    CreateQuestionParams, MaterialContentType, NewMaterial, NewQuestionGroup,
    ProposedChildQuestion, ProposedGroupBundle, QuestionGroupType, QuestionImage, SourceType,
    VfsQuestionGroupRepo,
};
use crate::vlm_grounding_service::ImportBlockKind;

/// 组装与发布报告的结构版本。旧 checkpoint 没有组报告时按无组处理，
/// 本版本号只用于让重导/前端区分字段语义，不代表已建表。
pub const GROUP_REPORT_SCHEMA_VERSION: u64 = 1;

/// 与 `VfsQuestionGroupRepo` 的组类型契约一致（reading / shared_condition / other）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupKind {
    /// 阅读材料 + 多个小题（英语一篇五题）。
    Reading,
    /// 共享条件 + 多个子题（数学 (1)(2)(3)）。
    SharedCondition,
    /// 其他显式分组（例如同一材料下的填空组）。
    Other,
}

impl GroupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reading => "reading",
            Self::SharedCondition => "shared_condition",
            Self::Other => "other",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "reading" => Some(Self::Reading),
            "shared_condition" => Some(Self::SharedCondition),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// 阅读/共享条件必须真的带材料；`other` 允许只有子题。
    fn requires_material(self) -> bool {
        matches!(self, Self::Reading | Self::SharedCondition)
    }
}

/// 材料内容类型，与 `VfsQuestionGroupRepo`/迁移的 text/formula/figure/mixed 一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    Text,
    Formula,
    Figure,
    Mixed,
}

impl MaterialKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Formula => "formula",
            Self::Figure => "figure",
            Self::Mixed => "mixed",
        }
    }
}

/// 导入命名空间：同一目标题目集上的显式重导复用同一命名空间，
/// 新 session 默认不自动并库（首版范围）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupImportKey {
    /// 目标题目集（`exam_sheets.id`）。
    pub exam_id: String,
    /// 来源文档身份（导入时生成的 `document_id`）。
    pub document_id: String,
    /// 显式命名空间。`document:<document_id>` 表示「同目标显式重新导入」；
    /// `session:<session_id>` 表示新会话独立导入，不跨 session 合并。
    pub namespace: String,
}

impl GroupImportKey {
    /// 新会话导入：身份只在本 session 内稳定，不跨 session 自动合并。
    pub fn new_session(exam_id: &str, document_id: &str, session_id: &str) -> Self {
        Self {
            exam_id: exam_id.to_string(),
            document_id: document_id.to_string(),
            namespace: format!("session:{session_id}"),
        }
    }

    /// 同一目标题目集上的显式重新导入：复用原组/材料身份。
    pub fn explicit_reimport(exam_id: &str, document_id: &str) -> Self {
        Self {
            exam_id: exam_id.to_string(),
            document_id: document_id.to_string(),
            namespace: format!("document:{document_id}"),
        }
    }

    fn is_well_formed(&self) -> bool {
        !self.exam_id.trim().is_empty()
            && !self.document_id.trim().is_empty()
            && !self.namespace.trim().is_empty()
    }
}

/// 管线 typed 块。由 [`TypedBlock::from_structured`] 从阶段 5 结果无损提取，
/// 也可由测试/其他入口直接构造。
#[derive(Debug, Clone, PartialEq)]
pub struct TypedBlock {
    pub source_id: String,
    pub kind: ImportBlockKind,
    pub section: String,
    pub label: String,
    /// 原文（材料内容以它为准，不信任模型摘要）。
    pub raw_text: String,
    /// 结构化题干的 content（仅 Question 块有意义）。
    pub content: Option<String>,
    pub page_indices: Vec<usize>,
    pub bbox: [f64; 4],
    pub images: Vec<QuestionImage>,
    pub continues_from_previous: bool,
    pub continues_to_next: bool,
    /// VLM 的 `is_group` 提示；只作提案来源，不直接建关系。
    pub is_group_hint: bool,
    /// VLM 的 `sub_questions` 提示；必须能唯一匹配同 section 的真实题块。
    pub sub_question_labels: Vec<String>,
    /// 模型在 JSON 里再次声明的 block_kind（与来源 kind 矛盾时进入复核）。
    pub declared_kind: Option<ImportBlockKind>,
}

impl TypedBlock {
    /// 从**答案合并之前**的 merged 块构造。材料块只存在于这一层：
    /// `answer_merge` 之后非题块可能已从候选集中移除，不能作为唯一来源。
    pub fn from_source(source: &QuestionWithFigures) -> Self {
        let block = &source.merged.question;
        Self {
            source_id: block.source_id.clone(),
            kind: block.block_kind,
            section: block.section.trim().to_string(),
            label: block.label.trim().to_string(),
            raw_text: block.raw_text.clone(),
            content: None,
            page_indices: source.merged.page_indices.clone(),
            bbox: block.bbox,
            images: source.images.clone(),
            continues_from_previous: block.continues_from_previous,
            continues_to_next: block.continues_to_next,
            is_group_hint: block.is_group,
            sub_question_labels: block.sub_questions.clone(),
            declared_kind: None,
        }
    }

    /// 从结构化结果补齐 Question 块的题干与模型声明角色；材料仍保留原文。
    pub fn from_structured(sq: &StructuredQuestion) -> Self {
        let mut typed = Self::from_source(&sq.source);
        typed.content = sq
            .json
            .get("content")
            .and_then(Value::as_str)
            .map(str::to_string);
        typed.declared_kind = sq
            .json
            .get("block_kind")
            .and_then(Value::as_str)
            .and_then(|raw| serde_json::from_value::<ImportBlockKind>(json!(raw)).ok());
        typed
    }

    /// 从答案合并的输入项构造（`answer_merge::MergeItem` 的公开字段）。
    /// 该路径只在调用方拿不到 merged 块时使用；缺少的页/框信息按空处理。
    pub fn from_merge_item(item: &crate::answer_merge::MergeItem) -> Self {
        Self::from_json_block(
            item.label.clone(),
            item.kind,
            item.section.clone(),
            item.source_id.clone(),
            item.raw_text.clone(),
            item.json.clone(),
        )
    }

    /// 从 `answer_merge::collect_non_question_candidates` 的快照构造。这是材料在
    /// 非题过滤后的兜底来源；有 merged 块时应优先用 [`blocks_before_answer_merge`]。
    pub fn from_non_question_candidate(
        candidate: &crate::answer_merge::NonQuestionCandidate,
    ) -> Self {
        Self::from_json_block(
            candidate.label.clone(),
            candidate.block_kind,
            candidate.section.clone(),
            candidate.source_id.clone(),
            candidate.raw_text.clone(),
            candidate.json.clone(),
        )
    }

    fn from_json_block(
        label: String,
        kind: ImportBlockKind,
        section: String,
        source_id: String,
        raw_text: String,
        json: Value,
    ) -> Self {
        let page_indices = json
            .get("page_indices")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_u64)
                    .map(|value| value as usize)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let bbox = json
            .get("bbox")
            .and_then(Value::as_array)
            .and_then(|values| {
                let numbers = values.iter().filter_map(Value::as_f64).collect::<Vec<_>>();
                (numbers.len() == 4).then(|| [numbers[0], numbers[1], numbers[2], numbers[3]])
            })
            .unwrap_or([0.0, 0.0, 0.0, 0.0]);
        let sub_question_labels = json
            .get("sub_questions")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Self {
            source_id,
            kind,
            section: section.trim().to_string(),
            label: label.trim().to_string(),
            content: json
                .get("content")
                .and_then(Value::as_str)
                .map(str::to_string),
            raw_text,
            page_indices,
            bbox,
            images: Vec::new(),
            continues_from_previous: json
                .get("continues_from_previous")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            continues_to_next: json
                .get("continues_to_next")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            is_group_hint: json
                .get("is_group")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            sub_question_labels,
            declared_kind: json
                .get("block_kind")
                .and_then(Value::as_str)
                .and_then(|raw| serde_json::from_value::<ImportBlockKind>(json!(raw)).ok()),
        }
    }

    fn is_material_role(&self) -> bool {
        matches!(
            self.kind,
            ImportBlockKind::Material | ImportBlockKind::Heading | ImportBlockKind::Figure
        )
    }

    fn is_question_role(&self) -> bool {
        self.kind == ImportBlockKind::Question
    }

    fn stem_text(&self) -> &str {
        self.content
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.raw_text.trim())
    }

    fn content_hash(&self) -> String {
        material_content_hash(&self.raw_text, &self.images)
    }

    fn has_body(&self) -> bool {
        !self.raw_text.trim().is_empty() || !self.images.is_empty()
    }
}

/// 提案中的子题引用：来源身份 + 可选显式子题号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildRef {
    pub source_id: String,
    pub sub_label: Option<String>,
}

/// 显式关系提案。只允许引用已存在的来源身份；模型不能凭想象增加材料或子题。
#[derive(Debug, Clone, PartialEq)]
pub struct GroupProposal {
    /// 组头/材料块的来源身份。组头本身不是一道题。
    pub group_source_id: String,
    pub kind: GroupKind,
    pub title: Option<String>,
    /// 有序材料来源 ID；必须与组同角色（Material/Heading/Figure）。
    pub material_source_ids: Vec<String>,
    /// 有序子题引用；顺序即 `group_order`。
    pub children: Vec<ChildRef>,
}

/// 在答案合并/非题过滤**之前**收集 typed 块：merged 块仍保留 Material/Heading/
/// Figure 角色与原文，这是材料的唯一无损来源。
pub fn blocks_before_answer_merge(sources: &[QuestionWithFigures]) -> Vec<TypedBlock> {
    sources.iter().map(TypedBlock::from_source).collect()
}

/// 直接从答案合并的输入项收集块（调用方无需手工重组）。`merge_answer_sections`
/// 只标记/升级，不删除 `MergeItem`，因此合并前后调用都保留材料原文。
pub fn blocks_from_merge_items(items: &[crate::answer_merge::MergeItem]) -> Vec<TypedBlock> {
    items.iter().map(TypedBlock::from_merge_item).collect()
}

/// 一站式组装：答案合并输入项 + 结构化结果 + 显式提案 → `GroupAssembly`。
/// 调用方只需在发布事务里把它交给 [`publish_assembled_groups`]。
pub fn assemble_from_merge_items(
    key: &GroupImportKey,
    items: &[crate::answer_merge::MergeItem],
    structured: &[StructuredQuestion],
    proposals: &[GroupProposal],
) -> GroupAssembly {
    let mut blocks = blocks_from_merge_items(items);
    enrich_with_structured(&mut blocks, structured);
    assemble_groups(key, &blocks, proposals)
}

/// 把结构化结果中的题干/声明角色补回对应来源块（按 `source_id` 精确匹配，
/// 不做位置配对）。找不到对应块的 structured 结果不会凭空生成新块。
pub fn enrich_with_structured(blocks: &mut [TypedBlock], structured: &[StructuredQuestion]) {
    let mut by_source: HashMap<&str, &StructuredQuestion> = HashMap::new();
    for sq in structured {
        by_source
            .entry(sq.source.merged.question.source_id.as_str())
            .or_insert(sq);
    }
    for block in blocks.iter_mut() {
        if let Some(sq) = by_source.get(block.source_id.as_str()) {
            let enriched = TypedBlock::from_structured(sq);
            // 原文、页范围与图片以 merged 块为准；只补结构化字段。
            block.content = enriched.content;
            block.declared_kind = enriched.declared_kind;
        }
    }
}

/// 由 VLM 显式提示生成提案。只用来源自身声明的 `is_group`、`sub_questions`
/// 与同 section 的唯一题号匹配；**不做**「取最近文章」或按数量猜配。
///
/// 只有材料/标题/图块能做组头；题块上的 `is_group` 提示不建组（组头不是题目，
/// 且首版不虚构材料）。提示里的隐藏语义分类不在这里猜，显式 `GroupKind::Reading`
/// 由调用方提案给出。
pub fn proposals_from_hints(blocks: &[TypedBlock]) -> Vec<GroupProposal> {
    let mut proposals = Vec::new();
    for header in blocks {
        if !header.is_group_hint {
            continue;
        }
        if !header.is_material_role() {
            continue;
        }
        let section = header.section.trim();
        // 题号必须唯一匹配，且必须与组头同 section（空 section 只匹配空 section）。
        let mut children = Vec::new();
        for label in &header.sub_question_labels {
            let label = label.trim();
            if label.is_empty() {
                continue;
            }
            let matches: Vec<&TypedBlock> = blocks
                .iter()
                .filter(|block| {
                    block.is_question_role()
                        && block.label.trim() == label
                        && block.section.trim() == section
                })
                .collect();
            if matches.len() != 1 {
                // 0 或 >1 都是证据不足，交给 assemble 阶段按未闭合报告，
                // 这里不猜一个「最像的」子题。
                continue;
            }
            let child = matches[0];
            if children
                .iter()
                .any(|existing: &ChildRef| existing.source_id == child.source_id)
            {
                continue;
            }
            children.push(ChildRef {
                source_id: child.source_id.clone(),
                sub_label: Some(label.to_string()),
            });
        }
        if children.is_empty() {
            continue;
        }
        // 材料只包含组头自身：这是唯一无需猜测的关联；其他材料必须由显式
        // 提案引用，否则进入未消费材料报告。
        proposals.push(GroupProposal {
            group_source_id: header.source_id.clone(),
            kind: GroupKind::SharedCondition,
            title: None,
            material_source_ids: vec![header.source_id.clone()],
            children,
        });
    }
    proposals
}

/// 冲突类别。`blocking = true` 的组不得发布。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GroupConflictKind {
    MissingGroupHeader,
    GroupHeaderIsQuestion,
    GroupHeaderRoleInvalid,
    ConflictingGroupIdentity,
    MissingMaterial,
    MaterialRoleInvalid,
    MaterialIsChild,
    DuplicateMaterialReference,
    MaterialClaimedByMultipleGroups,
    MaterialSectionMismatch,
    MissingChild,
    ChildRoleInvalid,
    ChildHeaderIsGroup,
    ChildClaimedByMultipleGroups,
    DuplicateChildOrder,
    DuplicateSubLabel,
    CrossSectionChild,
    AmbiguousSection,
    EmptyChildStem,
    EmptyGroup,
    NestedGroupUnsupported,
    ContradictoryBlockKind,
    DuplicateSourceIdentity,
    InvalidGroupKind,
    MissingImportIdentity,
    MissingTitle,
}

impl GroupConflictKind {
    /// 报告用的稳定字符串（与 serde 名称一致），便于前端按类别路由。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MissingGroupHeader => "missing_group_header",
            Self::GroupHeaderIsQuestion => "group_header_is_question",
            Self::GroupHeaderRoleInvalid => "group_header_role_invalid",
            Self::ConflictingGroupIdentity => "conflicting_group_identity",
            Self::MissingMaterial => "missing_material",
            Self::MaterialRoleInvalid => "material_role_invalid",
            Self::MaterialIsChild => "material_is_child",
            Self::DuplicateMaterialReference => "duplicate_material_reference",
            Self::MaterialClaimedByMultipleGroups => "material_claimed_by_multiple_groups",
            Self::MaterialSectionMismatch => "material_section_mismatch",
            Self::MissingChild => "missing_child",
            Self::ChildRoleInvalid => "child_role_invalid",
            Self::ChildHeaderIsGroup => "child_header_is_group",
            Self::ChildClaimedByMultipleGroups => "child_claimed_by_multiple_groups",
            Self::DuplicateChildOrder => "duplicate_child_order",
            Self::DuplicateSubLabel => "duplicate_sub_label",
            Self::CrossSectionChild => "cross_section_child",
            Self::AmbiguousSection => "ambiguous_section",
            Self::EmptyChildStem => "empty_child_stem",
            Self::EmptyGroup => "empty_group",
            Self::NestedGroupUnsupported => "nested_group_unsupported",
            Self::ContradictoryBlockKind => "contradictory_block_kind",
            Self::DuplicateSourceIdentity => "duplicate_source_identity",
            Self::InvalidGroupKind => "invalid_group_kind",
            Self::MissingImportIdentity => "missing_import_identity",
            Self::MissingTitle => "missing_title",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupConflict {
    pub kind: GroupConflictKind,
    pub group_source_id: Option<String>,
    pub source_ids: Vec<String>,
    pub detail: String,
    /// 是否阻断该组发布。报告里保留非阻断警告，便于前端区分。
    pub blocking: bool,
}

impl GroupConflict {
    fn blocking(
        kind: GroupConflictKind,
        group_source_id: Option<&str>,
        source_ids: Vec<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            group_source_id: group_source_id.map(str::to_string),
            source_ids,
            detail: detail.into(),
            blocking: true,
        }
    }

    fn warning(
        kind: GroupConflictKind,
        group_source_id: Option<&str>,
        source_ids: Vec<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            group_source_id: group_source_id.map(str::to_string),
            source_ids,
            detail: detail.into(),
            blocking: false,
        }
    }
}

/// 未被任何已接受题组消费的材料块。必须进报告，不能静默丢弃。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnconsumedMaterial {
    pub source_id: String,
    pub section: String,
    pub label: String,
    pub page_indices: Vec<usize>,
    /// `unreferenced` = 没有任何提案引用；`group_rejected` = 只被被拒组引用。
    pub reason: String,
}

/// 计划中的一份材料（整组只存一份，按 `order` 排序）。
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedMaterial {
    pub source_id: String,
    pub kind: MaterialKind,
    /// 原文权威内容；不允许用模型摘要替换。
    pub text: String,
    pub section: String,
    pub label: String,
    pub page_indices: Vec<usize>,
    pub bbox: [f64; 4],
    pub images: Vec<QuestionImage>,
    /// 1-based，组内唯一。
    pub order: i32,
    /// 本材料最后一次导入基线（用户编辑保护用）。
    pub content_hash: String,
}

/// 计划中的子题引用（真正的 question 由发布层创建，本模块不造题）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedChild {
    pub source_id: String,
    pub sub_label: Option<String>,
    /// 1-based，组内唯一。
    pub order: i32,
}

/// 一个可发布题组。组头不计题数、不产生作答记录。
#[derive(Debug, Clone, PartialEq)]
pub struct GroupPlan {
    /// 稳定组 ID（见 [`group_id_for`]）。
    pub group_id: String,
    pub group_source_id: String,
    pub key: GroupImportKey,
    pub kind: GroupKind,
    pub title: Option<String>,
    /// 组语义作用域；为空表示未声明。
    pub section: String,
    pub materials: Vec<PlannedMaterial>,
    pub children: Vec<PlannedChild>,
    /// 组头内容哈希（重导比较基线）。
    pub content_hash: String,
}

impl GroupPlan {
    /// 供发布层设置子题参数：`group_id` / `sub_label` / `group_order`。
    pub fn child_binding(&self, child: &PlannedChild) -> ChildBinding {
        ChildBinding {
            group_id: self.group_id.clone(),
            sub_label: child.sub_label.clone(),
            group_order: child.order,
        }
    }

    /// 发布前的最后一道自检：材料/组头身份绝不能出现在子题里。
    pub fn validate(&self) -> Result<(), String> {
        let materials: HashSet<&str> = self
            .materials
            .iter()
            .map(|material| material.source_id.as_str())
            .collect();
        if self.group_source_id.trim().is_empty() || self.group_id.trim().is_empty() {
            return Err("group_identity_missing".to_string());
        }
        if self
            .children
            .iter()
            .any(|child| child.source_id == self.group_source_id)
        {
            return Err("group_header_in_children".to_string());
        }
        if self
            .children
            .iter()
            .any(|child| materials.contains(child.source_id.as_str()))
        {
            return Err("material_in_children".to_string());
        }
        if self.children.is_empty() {
            return Err("group_without_children".to_string());
        }
        if self.kind.requires_material() && self.materials.is_empty() {
            return Err("group_without_material".to_string());
        }
        let mut orders = HashSet::new();
        for child in &self.children {
            if !orders.insert(child.order) {
                return Err("duplicate_child_order".to_string());
            }
        }
        let mut material_orders = HashSet::new();
        for material in &self.materials {
            if !material_orders.insert(material.order) {
                return Err("duplicate_material_order".to_string());
            }
        }
        Ok(())
    }

    /// 序列化为报告条目（不含原文全文，只保留定位与散列）。
    pub fn report_entry(&self) -> Value {
        json!({
            "group_id": self.group_id,
            "group_source_id": self.group_source_id,
            "kind": self.kind.as_str(),
            "title": self.title,
            "section": self.section,
            "material_count": self.materials.len(),
            "child_count": self.children.len(),
            "materials": self.materials.iter().map(|material| json!({
                "source_id": material.source_id,
                "kind": material.kind.as_str(),
                "order": material.order,
                "page_indices": material.page_indices,
                "content_hash": material.content_hash,
                "chars": material.text.chars().count(),
                "image_count": material.images.len(),
            })).collect::<Vec<_>>(),
            "children": self.children.iter().map(|child| json!({
                "source_id": child.source_id,
                "sub_label": child.sub_label,
                "order": child.order,
            })).collect::<Vec<_>>(),
            "content_hash": self.content_hash,
        })
    }
}

/// 组装汇总，供发布层与报告使用。
#[derive(Debug, Clone, Default)]
pub struct GroupAssembly {
    pub key: Option<GroupImportKey>,
    pub plans: Vec<GroupPlan>,
    pub conflicts: Vec<GroupConflict>,
    pub unconsumed: Vec<UnconsumedMaterial>,
    /// 被阻断组声明的子题；不得作为独立题发布，也不得作为组子题发布。
    pub blocked_child_sources: HashSet<String>,
    /// 已进入计划组的子题；只能经组发布，不得走 standalone 路径。
    pub planned_child_sources: HashSet<String>,
    /// 被阻断组声明的材料；不进入任何持久化写入。
    pub blocked_material_sources: HashSet<String>,
}

impl GroupAssembly {
    pub fn has_plans(&self) -> bool {
        !self.plans.is_empty()
    }

    /// 任何阻断冲突都要求复核；非阻断警告不阻断发布。
    pub fn is_blocking(&self) -> bool {
        self.conflicts.iter().any(|conflict| conflict.blocking)
    }

    /// 组装/发布报告。字段只含定位、计数与散列，不含材料全文或答案。
    pub fn report(&self) -> Value {
        let blocking: Vec<&GroupConflict> = self.conflicts.iter().filter(|c| c.blocking).collect();
        let warnings: Vec<&GroupConflict> = self.conflicts.iter().filter(|c| !c.blocking).collect();
        json!({
            "schema_version": GROUP_REPORT_SCHEMA_VERSION,
            "namespace": self.key.as_ref().map(|key| key.namespace.clone()),
            "group_count": self.plans.len(),
            "group_material_count": self.plans.iter().map(|plan| plan.materials.len()).sum::<usize>(),
            "group_child_count": self.plans.iter().map(|plan| plan.children.len()).sum::<usize>(),
            "unconsumed_material_count": self.unconsumed.len(),
            "conflict_count": self.conflicts.len(),
            "blocking_conflict_count": blocking.len(),
            "warning_count": warnings.len(),
            "blocked_group_count": self
                .conflicts
                .iter()
                .filter(|conflict| conflict.blocking && conflict.group_source_id.is_some())
                .map(|conflict| conflict.group_source_id.clone().unwrap_or_default())
                .collect::<BTreeSet<_>>()
                .len(),
            "blocked_child_count": self.blocked_child_sources.len(),
            "groups": self.plans.iter().map(GroupPlan::report_entry).collect::<Vec<_>>(),
            "group_conflicts": self.conflicts,
            "unconsumed_materials": self.unconsumed,
        })
    }
}

/// 子题参数绑定：发布层把它写进 `CreateQuestionParams` 的组字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildBinding {
    pub group_id: String,
    pub sub_label: Option<String>,
    pub group_order: i32,
}

/// 稳定组 ID：题目集 + 文档 + 命名空间 + 来源身份。
pub fn group_id_for(key: &GroupImportKey, group_source_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"question_group:v1\0");
    hash.update(key.exam_id.as_bytes());
    hash.update(b"\0");
    hash.update(key.document_id.as_bytes());
    hash.update(b"\0");
    hash.update(key.namespace.as_bytes());
    hash.update(b"\0");
    hash.update(group_source_id.trim().as_bytes());
    format!("grp_{:x}", hash.finalize())
}

/// 稳定材料 ID：组 ID + 材料来源身份（组内顺序调整不改变身份）。
pub fn material_id_for(group_id: &str, material_source_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"question_group_material:v1\0");
    hash.update(group_id.as_bytes());
    hash.update(b"\0");
    hash.update(material_source_id.trim().as_bytes());
    format!("gm_{:x}", hash.finalize())
}

fn material_kind(block: &TypedBlock) -> MaterialKind {
    let has_text = !block.raw_text.trim().is_empty();
    let has_images = !block.images.is_empty();
    let looks_formula = block.raw_text.contains('$')
        || block.raw_text.contains("\\frac")
        || block.raw_text.contains("^{")
        || block.raw_text.contains("_{");
    match (has_text, has_images, looks_formula) {
        (true, true, _) => MaterialKind::Mixed,
        (false, true, _) => MaterialKind::Figure,
        (true, false, true) => MaterialKind::Formula,
        _ => MaterialKind::Text,
    }
}

fn inferred_title(block: &TypedBlock) -> Option<String> {
    let text = block.raw_text.trim();
    if text.is_empty() {
        return None;
    }
    let first_line = text.lines().next().unwrap_or(text).trim();
    if first_line.is_empty() {
        return None;
    }
    Some(first_line.chars().take(200).collect())
}

fn sections_compatible(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    a.is_empty() || b.is_empty() || a == b
}

/// 核心组装。输入是不可变的 typed 块与显式提案，输出可发布计划、冲突与
/// 未消费材料。任何歧义都进入 `conflicts`，不靠排序位置「挑一个最像的」。
pub fn assemble_groups(
    key: &GroupImportKey,
    blocks: &[TypedBlock],
    proposals: &[GroupProposal],
) -> GroupAssembly {
    let mut assembly = GroupAssembly {
        key: Some(key.clone()),
        ..Default::default()
    };
    if !key.is_well_formed() {
        assembly.conflicts.push(GroupConflict::blocking(
            GroupConflictKind::MissingImportIdentity,
            None,
            vec![],
            "题目集/文档/命名空间不完整，不能建立稳定组身份",
        ));
        // 没有可用身份时仍要报告未消费材料，避免材料静默丢失。
        assembly.unconsumed = blocks
            .iter()
            .filter(|block| block.is_material_role())
            .map(|block| UnconsumedMaterial {
                source_id: block.source_id.clone(),
                section: block.section.clone(),
                label: block.label.clone(),
                page_indices: block.page_indices.clone(),
                reason: "unreferenced".to_string(),
            })
            .collect();
        return assembly;
    }

    // 1. 来源索引：同 id 同载荷 = 重放；同 id 不同载荷 = 身份冲突。
    let mut by_source: HashMap<&str, &TypedBlock> = HashMap::new();
    let mut ambiguous: HashSet<&str> = HashSet::new();
    for block in blocks {
        if block.source_id.trim().is_empty() {
            continue;
        }
        if let Some(previous) = by_source.insert(block.source_id.as_str(), block) {
            if previous != block {
                ambiguous.insert(block.source_id.as_str());
            }
        }
    }

    // 2. 提案去重：同一组身份重复提案必须完全一致；不一致即整组复核。
    let mut proposal_by_group: BTreeMap<&str, &GroupProposal> = BTreeMap::new();
    let mut identity_conflicts: HashSet<String> = HashSet::new();
    for proposal in proposals {
        let group_source = proposal.group_source_id.trim();
        if group_source.is_empty() {
            assembly.conflicts.push(GroupConflict::blocking(
                GroupConflictKind::MissingGroupHeader,
                None,
                vec![],
                "提案没有组来源身份",
            ));
            continue;
        }
        if let Some(previous) = proposal_by_group.insert(group_source, proposal) {
            if previous != proposal {
                identity_conflicts.insert(group_source.to_string());
                assembly.conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::ConflictingGroupIdentity,
                    Some(group_source),
                    vec![group_source.to_string()],
                    "同一组来源身份出现不同载荷的提案",
                ));
            }
        }
    }

    // 3. 预扫描跨组争用：子题与材料都只能属于一个组。
    let mut child_claims: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut material_claims: HashMap<&str, Vec<&str>> = HashMap::new();
    for (group_source, proposal) in &proposal_by_group {
        for child in &proposal.children {
            child_claims
                .entry(child.source_id.as_str())
                .or_default()
                .push(group_source);
        }
        for material in &proposal.material_source_ids {
            material_claims
                .entry(material.as_str())
                .or_default()
                .push(group_source);
        }
    }
    let group_sources: HashSet<&str> = proposal_by_group.keys().copied().collect();
    // 某组的组头被任何组（含自己）当作子题引用 ⇒ 嵌套/自环不成立，该组不得发布。
    // 注意：被阻断的是「组头被引用」的那个组（child_source），不是引用者。
    let mut header_claimed: HashSet<&str> = HashSet::new();
    for child_source in child_claims.keys() {
        if group_sources.contains(*child_source) {
            header_claimed.insert(*child_source);
        }
    }

    let mut accepted: Vec<GroupPlan> = Vec::new();
    let mut rejected_groups: HashSet<String> = HashSet::new();
    let mut claimed_materials: HashSet<String> = HashSet::new();
    let mut claimed_children: HashSet<String> = HashSet::new();
    let mut seen_material_rows: HashSet<(String, String)> = HashSet::new();

    for (group_source, proposal) in &proposal_by_group {
        let group_source = group_source.to_string();
        // 本组的阻断冲突先收集，任一条存在就不产出计划。
        let mut group_conflicts: Vec<GroupConflict> = Vec::new();
        if identity_conflicts.contains(&group_source) {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::ConflictingGroupIdentity,
                Some(group_source.as_str()),
                vec![group_source.clone()],
                "同一组来源身份存在不同载荷，整组不发布",
            ));
        }
        if header_claimed.contains(group_source.as_str()) {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::NestedGroupUnsupported,
                Some(group_source.as_str()),
                vec![group_source.clone()],
                "本组组头被其他组当作子题引用，嵌套关系不成立，整组复核",
            ));
        }
        let block_for = |source: &str| by_source.get(source).copied();

        let header = match block_for(group_source.as_str()) {
            Some(header) => Some(header),
            None => {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MissingGroupHeader,
                    Some(group_source.as_str()),
                    vec![group_source.clone()],
                    "组来源身份在 typed 块中不存在",
                ));
                None
            }
        };
        if let Some(header) = header {
            if ambiguous.contains(header.source_id.as_str()) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::DuplicateSourceIdentity,
                    Some(group_source.as_str()),
                    vec![group_source.clone()],
                    "组来源身份出现不同载荷",
                ));
            }
            if header.is_question_role() {
                // 组头不是一道题；把题块当组头会制造「隐藏题目」。
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::GroupHeaderIsQuestion,
                    Some(group_source.as_str()),
                    vec![group_source.clone()],
                    "组来源身份是 Question 块，组头不能当作题目以外的东西",
                ));
            } else if !header.is_material_role() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::GroupHeaderRoleInvalid,
                    Some(group_source.as_str()),
                    vec![group_source.clone()],
                    format!("组来源块角色为 {:?}，不能作为材料组头", header.kind),
                ));
            }
            if !header.has_body() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MissingMaterial,
                    Some(group_source.as_str()),
                    vec![group_source.clone()],
                    "组头块没有正文或图片，不能作为材料",
                ));
            }
        }

        // 材料解析：显式引用 + 有序 + 去重。
        let mut materials: Vec<PlannedMaterial> = Vec::new();
        let mut material_sources: HashSet<String> = HashSet::new();
        let mut order = 0i32;
        for material_source in &proposal.material_source_ids {
            let material_source = material_source.trim();
            if material_source.is_empty() {
                continue;
            }
            if !material_sources.insert(material_source.to_string()) {
                group_conflicts.push(GroupConflict::warning(
                    GroupConflictKind::DuplicateMaterialReference,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "同一材料被重复引用，已按一份处理",
                ));
                continue;
            }
            if material_claims
                .get(material_source)
                .is_some_and(|owners| owners.len() > 1)
            {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MaterialClaimedByMultipleGroups,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "材料同时被多个组引用，无法判定归属",
                ));
            }
            let Some(block) = block_for(material_source) else {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MissingMaterial,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "引用的材料来源不存在",
                ));
                continue;
            };
            if block.is_question_role() {
                // 题目不能伪装成材料，否则会用「材料」名义绕过题目校验。
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MaterialRoleInvalid,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "引用的材料来源是 Question 块，不能作为材料",
                ));
                continue;
            }
            if !block.is_material_role() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MaterialRoleInvalid,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    format!("引用的材料角色为 {:?}，不是材料/标题/图块", block.kind),
                ));
                continue;
            }
            if !block.has_body() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MissingMaterial,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "材料没有正文或图片",
                ));
                continue;
            }
            if ambiguous.contains(block.source_id.as_str()) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::DuplicateSourceIdentity,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "材料来源身份出现不同载荷",
                ));
                continue;
            }
            if !sections_compatible(&header_section(header), &block.section) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MaterialSectionMismatch,
                    Some(group_source.as_str()),
                    vec![material_source.to_string()],
                    "材料章节与组章节不一致，跨章节关系不成立",
                ));
                continue;
            }
            if !seen_material_rows.insert((group_source.clone(), material_source.to_string())) {
                continue;
            }
            order += 1;
            materials.push(PlannedMaterial {
                source_id: block.source_id.clone(),
                kind: material_kind(block),
                text: block.raw_text.clone(),
                section: block.section.clone(),
                label: block.label.clone(),
                page_indices: block.page_indices.clone(),
                bbox: block.bbox,
                images: block.images.clone(),
                order,
                content_hash: block.content_hash(),
            });
        }

        // 组头没有声明章节时，多份材料跨章节同样是歧义，不能按顺序认一个。
        if header_section(header).trim().is_empty() {
            let sections: BTreeSet<&str> = materials
                .iter()
                .map(|material| material.section.trim())
                .filter(|section| !section.is_empty())
                .collect();
            if sections.len() > 1 {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::AmbiguousSection,
                    Some(group_source.as_str()),
                    sections.iter().map(|section| section.to_string()).collect(),
                    "组未声明章节且材料跨多个章节，不能确定组归属",
                ));
            }
        }

        // 子题解析：真实题块 + 有序 + 唯一子题号。
        let mut children: Vec<PlannedChild> = Vec::new();
        let mut child_sources: HashSet<String> = HashSet::new();
        let mut sub_labels: HashSet<String> = HashSet::new();
        let mut child_order = 0i32;
        for child_ref in &proposal.children {
            let child_source = child_ref.source_id.trim();
            if child_source.is_empty() {
                continue;
            }
            if !child_sources.insert(child_source.to_string()) {
                group_conflicts.push(GroupConflict::warning(
                    GroupConflictKind::DuplicateChildOrder,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "子题被重复引用，已按一次处理",
                ));
                continue;
            }
            if child_claims
                .get(child_source)
                .is_some_and(|owners| owners.len() > 1)
            {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::ChildClaimedByMultipleGroups,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "同一子题被多个组引用",
                ));
            }
            if group_sources.contains(child_source) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::NestedGroupUnsupported,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "首版不支持嵌套题组",
                ));
                continue;
            }
            if child_source == group_source {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::ChildHeaderIsGroup,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "组头不能同时是子题",
                ));
                continue;
            }
            let Some(block) = block_for(child_source) else {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::MissingChild,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "引用的子题来源不存在",
                ));
                continue;
            };
            if !block.is_question_role() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::ChildRoleInvalid,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    format!("引用的子题角色为 {:?}，不是 Question", block.kind),
                ));
                continue;
            }
            if ambiguous.contains(block.source_id.as_str()) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::DuplicateSourceIdentity,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "子题来源身份出现不同载荷",
                ));
                continue;
            }
            if block
                .declared_kind
                .is_some_and(|kind| kind != ImportBlockKind::Question)
            {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::ContradictoryBlockKind,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "来源角色是 Question，但模型声明了其他 block_kind",
                ));
                continue;
            }
            let group_section = header_section(header);
            if !sections_compatible(&group_section, &block.section) {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::CrossSectionChild,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "子题章节与组章节不一致",
                ));
                continue;
            }
            if block.stem_text().is_empty() {
                group_conflicts.push(GroupConflict::blocking(
                    GroupConflictKind::EmptyChildStem,
                    Some(group_source.as_str()),
                    vec![child_source.to_string()],
                    "子题没有可用题干",
                ));
                continue;
            }
            if let Some(sub_label) = child_ref.sub_label.as_deref() {
                let sub_label = sub_label.trim();
                if !sub_label.is_empty() && !sub_labels.insert(sub_label.to_string()) {
                    group_conflicts.push(GroupConflict::blocking(
                        GroupConflictKind::DuplicateSubLabel,
                        Some(group_source.as_str()),
                        vec![child_source.to_string()],
                        format!("组内子题号 {sub_label} 重复"),
                    ));
                    continue;
                }
            }
            child_order += 1;
            children.push(PlannedChild {
                source_id: block.source_id.clone(),
                sub_label: child_ref
                    .sub_label
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                order: child_order,
            });
        }

        // 组级校验：有材料才有意义、有子题才可发布。
        if children.is_empty() {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::EmptyGroup,
                Some(group_source.as_str()),
                vec![],
                "组内没有可发布子题",
            ));
        }
        if proposal.kind.requires_material() && materials.is_empty() {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::MissingMaterial,
                Some(group_source.as_str()),
                vec![],
                format!("{} 组必须至少包含一份材料", proposal.kind.as_str()),
            ));
        }
        // 材料在子题列表中出现（显式或经重复引用）一律阻断。
        if let Some(hit) = children
            .iter()
            .find(|child| material_sources.contains(&child.source_id))
        {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::MaterialIsChild,
                Some(group_source.as_str()),
                vec![hit.source_id.clone()],
                "材料身份同时出现在子题列表",
            ));
        }

        // 章节歧义：组头/提案没有语义章节，而子题跨多个章节。
        let mut child_sections: BTreeSet<&str> = BTreeSet::new();
        for child in &children {
            if let Some(block) = block_for(child.source_id.as_str()) {
                let section = block.section.trim();
                if !section.is_empty() {
                    child_sections.insert(section);
                }
            }
        }
        let group_section = header_section(header);
        if group_section.trim().is_empty() && child_sections.len() > 1 {
            group_conflicts.push(GroupConflict::blocking(
                GroupConflictKind::AmbiguousSection,
                Some(group_source.as_str()),
                child_sections.iter().map(|s| s.to_string()).collect(),
                "组没有声明章节，子题跨多个章节，不能确定归属",
            ));
        }

        if group_conflicts.iter().any(|conflict| conflict.blocking) {
            rejected_groups.insert(group_source.clone());
            for material in &proposal.material_source_ids {
                if !claimed_materials.contains(material) {
                    assembly.blocked_material_sources.insert(material.clone());
                }
            }
            for child in &proposal.children {
                assembly
                    .blocked_child_sources
                    .insert(child.source_id.clone());
            }
            assembly.conflicts.extend(group_conflicts);
            continue;
        }

        // 只有非阻断警告，可以发布。
        let group_section = if !group_section.trim().is_empty() {
            group_section
        } else {
            child_sections
                .iter()
                .next()
                .copied()
                .unwrap_or("")
                .to_string()
        };
        let title = proposal
            .title
            .clone()
            .filter(|title| !title.trim().is_empty())
            .or_else(|| header.and_then(inferred_title));
        if title.is_none() {
            assembly.conflicts.push(GroupConflict::warning(
                GroupConflictKind::MissingTitle,
                Some(group_source.as_str()),
                vec![],
                "组没有标题，使用空标题发布",
            ));
        }
        let group_id = group_id_for(key, &group_source);
        let content_hash = header
            .map(TypedBlock::content_hash)
            .unwrap_or_else(|| String::new());
        let plan = GroupPlan {
            group_id,
            group_source_id: group_source.clone(),
            key: key.clone(),
            kind: proposal.kind,
            title,
            section: group_section,
            materials,
            children,
            content_hash,
        };
        if let Err(reason) = plan.validate() {
            assembly.conflicts.push(GroupConflict::blocking(
                GroupConflictKind::InvalidGroupKind,
                Some(group_source.as_str()),
                vec![],
                format!("计划自检失败: {reason}"),
            ));
            rejected_groups.insert(group_source.clone());
            continue;
        }
        for material in &plan.materials {
            claimed_materials.insert(material.source_id.clone());
        }
        for child in &plan.children {
            assembly
                .planned_child_sources
                .insert(child.source_id.clone());
            claimed_children.insert(child.source_id.clone());
        }
        assembly.conflicts.extend(group_conflicts);
        accepted.push(plan);
    }

    // 4. 未消费材料：所有材料角色中未被任何接受组使用的块。
    for block in blocks {
        if !block.is_material_role() {
            continue;
        }
        if claimed_materials.contains(&block.source_id) {
            continue;
        }
        let reason = if rejected_groups.is_empty() {
            "unreferenced"
        } else {
            "group_rejected"
        };
        assembly.unconsumed.push(UnconsumedMaterial {
            source_id: block.source_id.clone(),
            section: block.section.clone(),
            label: block.label.clone(),
            page_indices: block.page_indices.clone(),
            reason: reason.to_string(),
        });
    }
    // 被接受组的子题不能同时作为未消费材料的替身（材料角色已按 kind 过滤）。

    assembly.plans = accepted;
    assembly
}

/// 组语义章节只认组头块自己声明的 section。组头没有章节时，不允许从
/// 「版面位置更近的材料/子题」推断——那正是方案禁止的最近文章猜测。
fn header_section(header: Option<&TypedBlock>) -> String {
    header
        .map(|header| header.section.trim().to_string())
        .unwrap_or_default()
}

// ============================================================================
// 发布：与 VfsQuestionGroupRepo 的同事务 seam
// ============================================================================

/// 一个子题的完整发布参数。`CreateQuestionParams` 由调用方（#490 persistence）
/// 用其既有 `validated_params` 生成，避免两套校验。组字段通过 [`ChildBinding`]
/// 单独携带，直到模型 repo 的 `CreateQuestionParams` 补齐 `group_id/sub_label/
/// group_order` 后由适配器写入。
#[derive(Debug, Clone)]
pub struct PlannedChildQuestion {
    pub binding: ChildBinding,
    pub source_id: String,
    pub card_id: String,
    pub question: CreateQuestionParams,
}

/// 一次题组原子发布请求。`children` 必须与 `plan.children` 一一对应，
/// 不多、不少、不重排。
#[derive(Debug, Clone)]
pub struct GroupPublishRequest {
    pub plan: GroupPlan,
    /// 稳定材料 ID，顺序与 `plan.materials` 一致。
    pub material_ids: Vec<String>,
    pub children: Vec<PlannedChildQuestion>,
}

impl GroupPublishRequest {
    /// 用计划 + 已构建的子题参数组装请求，并做一一对应校验。
    pub fn new(plan: GroupPlan, children: Vec<PlannedChildQuestion>) -> Result<Self, String> {
        plan.validate()?;
        if plan.children.len() != children.len() {
            return Err("child_count_mismatch".to_string());
        }
        for (expected, actual) in plan.children.iter().zip(children.iter()) {
            if expected.source_id != actual.source_id {
                return Err("child_identity_mismatch".to_string());
            }
            if expected.order != actual.binding.group_order
                || actual.binding.group_id != plan.group_id
            {
                return Err("child_binding_mismatch".to_string());
            }
        }
        let material_ids = plan
            .materials
            .iter()
            .map(|material| material_id_for(&plan.group_id, &material.source_id))
            .collect();
        Ok(Self {
            plan,
            material_ids,
            children,
        })
    }

    pub fn child_count(&self) -> usize {
        self.children.len()
    }
}

/// 组发布失败。`IncompleteChildSet` 表示组内应有子题未通过校验/未创建，
/// 整组不得发布。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupPublishError {
    IncompleteChildSet {
        group_id: String,
        missing: Vec<String>,
    },
    Rejected(String),
    Repository(String),
}

impl std::fmt::Display for GroupPublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IncompleteChildSet { group_id, missing } => write!(
                f,
                "组 {group_id} 缺少应有子题 {}，整组不发布",
                missing.join(",")
            ),
            Self::Rejected(reason) => write!(f, "题组发布被拒绝: {reason}"),
            Self::Repository(reason) => write!(f, "题组仓储发布失败: {reason}"),
        }
    }
}

impl std::error::Error for GroupPublishError {}

/// 发布回执（与仓储 bundle 的版本/状态字段对齐）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPublishReceipt {
    pub group_id: String,
    pub group_version: i64,
    pub publish_state: String,
    pub material_ids: Vec<String>,
    pub child_question_ids: Vec<String>,
    pub material_count: usize,
    pub child_count: usize,
    /// 本次是否复用已有组身份（幂等重放）。
    pub reused: bool,
    /// 重放时按三方基线安全更新的材料数。
    pub updated_material_count: usize,
    /// 重放时因用户已编辑而保留原内容的材料来源。
    pub preserved_material_sources: Vec<String>,
    /// 非阻断警告（供报告合并）。
    pub warnings: Vec<String>,
}

/// 仓储 seam。`RepoGroupPublisher` 是唯一实现；`#490` 在发布事务里调用
/// [`publish_group_plan`] 或 [`publish_assembled_groups`]。
pub trait GroupPublisher {
    fn publish_group_bundle(
        &mut self,
        conn: &Connection,
        request: &GroupPublishRequest,
    ) -> Result<GroupPublishReceipt, GroupPublishError>;
}

/// 在调用方事务内发布一个题组。组/材料/子题由仓储在同一 `rusqlite::Transaction`
/// 内 staging → complete；调用方继续在该事务里刷新预览与统计。任何错误都原样
/// 上抛，由调用方回滚整个事务。
pub fn publish_group_plan<P: GroupPublisher + ?Sized>(
    tx: &rusqlite::Transaction<'_>,
    request: &GroupPublishRequest,
    publisher: &mut P,
) -> Result<GroupPublishReceipt, GroupPublishError> {
    request
        .plan
        .validate()
        .map_err(GroupPublishError::Rejected)?;
    if request.children.len() != request.plan.children.len() {
        return Err(GroupPublishError::IncompleteChildSet {
            group_id: request.plan.group_id.clone(),
            missing: request
                .plan
                .children
                .iter()
                .map(|child| child.source_id.clone())
                .collect(),
        });
    }
    // Deref coercion：Transaction -> Connection，与仓储签名一致。
    publisher.publish_group_bundle(tx, request)
}

/// 组装 + 发布的汇总结果，调用方直接并入导入报告。
#[derive(Debug, Clone, Default)]
pub struct GroupPublishOutcome {
    pub receipts: Vec<GroupPublishReceipt>,
    /// 未能发布的组：(group_source_id, reason)。
    pub rejected: Vec<(String, String)>,
    pub published_child_sources: HashSet<String>,
    /// 本次不得以 standalone 题发布的子题来源（组阻断或发布失败）。
    pub skipped_child_sources: HashSet<String>,
    pub updated_material_count: usize,
    pub preserved_material_sources: Vec<String>,
}

impl GroupPublishOutcome {
    pub fn is_partial(&self) -> bool {
        !self.rejected.is_empty() || !self.skipped_child_sources.is_empty()
    }

    pub fn report(&self) -> Value {
        json!({
            "schema_version": GROUP_REPORT_SCHEMA_VERSION,
            "published_group_count": self.receipts.len(),
            "rejected_group_count": self.rejected.len(),
            "published_child_count": self.published_child_sources.len(),
            "skipped_child_count": self.skipped_child_sources.len(),
            "updated_material_count": self.updated_material_count,
            "preserved_user_material_count": self.preserved_material_sources.len(),
            "rejected_groups": self.rejected.iter().map(|(group, reason)| json!({
                "group_source_id": group,
                "reason": reason,
            })).collect::<Vec<_>>(),
            "preserved_user_materials": self.preserved_material_sources,
        })
    }
}

/// 一步发布整个组装结果：调用方只需按 `(plan, child)` 提供子题参数。
///
/// 语义：
/// * 组阻断（assembly 已判）或子题参数缺失 → 整组跳过，子题进入
///   `skipped_child_sources`，绝不半组发布；
/// * 仓储拒绝（如用户已删除该组身份）→ 记入 `rejected`，不中断事务；
/// * 仓储错误 → 原样上抛，由调用方回滚。
pub fn publish_assembled_groups<F>(
    tx: &rusqlite::Transaction<'_>,
    assembly: &GroupAssembly,
    mut child_params: F,
) -> Result<GroupPublishOutcome, GroupPublishError>
where
    F: FnMut(&GroupPlan, &PlannedChild) -> Option<(String, CreateQuestionParams)>,
{
    let mut publisher = RepoGroupPublisher::new();
    publish_assembled_groups_with(tx, assembly, &mut publisher, &mut child_params)
}

/// 最小接入：组装 + 发布一次完成（使用内置 hint 提案）。
/// `#490` 只需在发布事务里提供子题参数回调。
pub fn assemble_and_publish_from_hints<F>(
    tx: &rusqlite::Transaction<'_>,
    key: &GroupImportKey,
    items: &[crate::answer_merge::MergeItem],
    structured: &[StructuredQuestion],
    child_params: F,
) -> Result<(GroupAssembly, GroupPublishOutcome), GroupPublishError>
where
    F: FnMut(&GroupPlan, &PlannedChild) -> Option<(String, CreateQuestionParams)>,
{
    let mut blocks = blocks_from_merge_items(items);
    enrich_with_structured(&mut blocks, structured);
    let proposals = proposals_from_hints(&blocks);
    assemble_and_publish(tx, key, blocks, proposals, child_params)
}

/// 最小接入：显式提案版本。材料在装配前已按来源身份引用，不存在猜测。
pub fn assemble_and_publish<F>(
    tx: &rusqlite::Transaction<'_>,
    key: &GroupImportKey,
    blocks: Vec<TypedBlock>,
    proposals: Vec<GroupProposal>,
    child_params: F,
) -> Result<(GroupAssembly, GroupPublishOutcome), GroupPublishError>
where
    F: FnMut(&GroupPlan, &PlannedChild) -> Option<(String, CreateQuestionParams)>,
{
    let assembly = assemble_groups(key, &blocks, &proposals);
    let outcome = publish_assembled_groups(tx, &assembly, child_params)?;
    Ok((assembly, outcome))
}

/// 可注入 publisher 的版本（测试/未来替换用）。
pub fn publish_assembled_groups_with<P, F>(
    tx: &rusqlite::Transaction<'_>,
    assembly: &GroupAssembly,
    publisher: &mut P,
    child_params: &mut F,
) -> Result<GroupPublishOutcome, GroupPublishError>
where
    P: GroupPublisher + ?Sized,
    F: FnMut(&GroupPlan, &PlannedChild) -> Option<(String, CreateQuestionParams)>,
{
    let mut outcome = GroupPublishOutcome {
        skipped_child_sources: assembly.blocked_child_sources.clone(),
        ..Default::default()
    };
    for plan in &assembly.plans {
        let mut children = Vec::with_capacity(plan.children.len());
        let mut missing = false;
        for child in &plan.children {
            match child_params(plan, child) {
                Some((card_id, question)) => children.push(PlannedChildQuestion {
                    binding: plan.child_binding(child),
                    source_id: child.source_id.clone(),
                    card_id,
                    question,
                }),
                None => {
                    missing = true;
                    break;
                }
            }
        }
        let skip_all = |outcome: &mut GroupPublishOutcome, reason: &str| {
            outcome
                .rejected
                .push((plan.group_source_id.clone(), reason.to_string()));
            for child in &plan.children {
                outcome
                    .skipped_child_sources
                    .insert(child.source_id.clone());
            }
        };
        if missing {
            skip_all(&mut outcome, "child_params_missing");
            continue;
        }
        let request = match GroupPublishRequest::new(plan.clone(), children) {
            Ok(request) => request,
            Err(reason) => {
                skip_all(&mut outcome, &format!("invalid_request:{reason}"));
                continue;
            }
        };
        match publish_group_plan(tx, &request, publisher) {
            Ok(receipt) => {
                outcome.updated_material_count += receipt.updated_material_count;
                outcome
                    .preserved_material_sources
                    .extend(receipt.preserved_material_sources.iter().cloned());
                outcome
                    .published_child_sources
                    .extend(plan.children.iter().map(|child| child.source_id.clone()));
                outcome.receipts.push(receipt);
            }
            Err(GroupPublishError::Rejected(reason)) => skip_all(&mut outcome, &reason),
            Err(error) => return Err(error),
        }
    }
    Ok(outcome)
}

/// 写入 `question_groups.source_ref` 的稳定来源身份串，也是同身份重放的查找键。
pub fn group_source_ref(plan: &GroupPlan) -> String {
    json!({
        "schema_version": GROUP_REPORT_SCHEMA_VERSION,
        "document_id": plan.key.document_id,
        "namespace": plan.key.namespace,
        "group_source_id": plan.group_source_id,
    })
    .to_string()
}

/// 材料内容散列：与 [`TypedBlock::content_hash`] 同一算法，用于三方基线比较。
pub fn material_content_hash(text: &str, images: &[QuestionImage]) -> String {
    let mut hash = Sha256::new();
    hash.update(text.trim().as_bytes());
    for image in images {
        hash.update(image.hash.as_bytes());
    }
    format!("{:x}", hash.finalize())
}

fn material_source_ref(material: &PlannedMaterial) -> String {
    json!({
        "source_id": material.source_id,
        "content_hash": material.content_hash,
    })
    .to_string()
}

fn parse_material_ref(source_ref: Option<&str>) -> Option<(String, String)> {
    let value: Value = serde_json::from_str(source_ref?).ok()?;
    let source_id = value.get("source_id")?.as_str()?.to_string();
    let content_hash = value
        .get("content_hash")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    Some((source_id, content_hash))
}

/// 真实仓储适配器：把 [`GroupPublishRequest`] 转成 `ProposedGroupBundle`，在同一
/// `&Connection`（调用方事务）里调用 `VfsQuestionGroupRepo::publish_group_bundle_with_conn`。
///
/// 重放同一来源身份时按**三方基线**处理材料：
/// * 当前库内容 == 上次导入基线 ⇒ 用户未编辑，可安全更新为本次内容；
/// * 当前库内容 != 上次导入基线 ⇒ 用户已编辑，保留用户内容并报告；
/// * 组/材料/子题集合或顺序变化 ⇒ 交复核，不静默改关系。
#[derive(Debug, Default, Clone, Copy)]
pub struct RepoGroupPublisher;

impl RepoGroupPublisher {
    pub fn new() -> Self {
        Self
    }
}

fn group_type(kind: GroupKind) -> QuestionGroupType {
    match kind {
        GroupKind::Reading => QuestionGroupType::Reading,
        GroupKind::SharedCondition => QuestionGroupType::SharedCondition,
        GroupKind::Other => QuestionGroupType::Other,
    }
}

fn content_type(kind: MaterialKind) -> MaterialContentType {
    match kind {
        MaterialKind::Text => MaterialContentType::Text,
        MaterialKind::Formula => MaterialContentType::Formula,
        MaterialKind::Figure => MaterialContentType::Figure,
        MaterialKind::Mixed => MaterialContentType::Mixed,
    }
}

fn repository(error: impl std::fmt::Display) -> GroupPublishError {
    GroupPublishError::Repository(error.to_string())
}

impl GroupPublisher for RepoGroupPublisher {
    fn publish_group_bundle(
        &mut self,
        conn: &Connection,
        request: &GroupPublishRequest,
    ) -> Result<GroupPublishReceipt, GroupPublishError> {
        let plan = &request.plan;
        let source_ref = group_source_ref(plan);
        let existing: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT id, deleted_at FROM question_groups \
                 WHERE exam_id = ?1 AND source_ref = ?2 ORDER BY created_at LIMIT 1",
                rusqlite::params![plan.key.exam_id, source_ref],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| repository(error))?;
        if let Some((group_id, deleted_at)) = existing {
            if deleted_at.is_some() {
                // 用户排除/删除的组身份不因重导复活。
                return Err(GroupPublishError::Rejected(
                    "group_previously_soft_deleted".to_string(),
                ));
            }
            return replay_existing_group(conn, plan, &group_id);
        }

        let materials = plan
            .materials
            .iter()
            .map(|material| NewMaterial {
                // 仓储会重写为真实组 ID；空串表示「随本次发布」。
                group_id: String::new(),
                order_index: material.order,
                content_type: content_type(material.kind),
                title: (!material.label.trim().is_empty()).then(|| material.label.clone()),
                text: material.text.clone(),
                images: material.images.clone(),
                source_ref: Some(material_source_ref(material)),
                page_index: material.page_indices.first().map(|page| *page as i32),
                bbox: Some(material.bbox),
            })
            .collect::<Vec<_>>();
        let children = request
            .children
            .iter()
            .map(|child| ProposedChildQuestion {
                client_ref: Some(child.card_id.clone()),
                question: child.question.clone(),
                sub_label: child.binding.sub_label.clone(),
                group_order: child.binding.group_order,
            })
            .collect::<Vec<_>>();
        let bundle = ProposedGroupBundle {
            group: NewQuestionGroup {
                exam_id: plan.key.exam_id.clone(),
                group_type: group_type(plan.kind),
                title: plan.title.clone(),
                source_type: Some(SourceType::Imported),
                source_ref: Some(source_ref),
                import_baseline: Some(group_import_baseline(plan)),
                publish_state: None,
            },
            materials,
            children,
        };
        let published = VfsQuestionGroupRepo::publish_group_bundle_with_conn(conn, &bundle)
            .map_err(|error| repository(error))?;
        let receipt = GroupPublishReceipt {
            group_id: published.group.id.clone(),
            group_version: published.group.version,
            publish_state: published.group.publish_state.as_str().to_string(),
            material_count: published.materials.len(),
            child_count: published.siblings.len(),
            material_ids: published
                .materials
                .iter()
                .map(|row| row.id.clone())
                .collect(),
            child_question_ids: published
                .siblings
                .iter()
                .map(|row| row.id.clone())
                .collect(),
            reused: false,
            updated_material_count: 0,
            preserved_material_sources: Vec::new(),
            warnings: Vec::new(),
        };
        if receipt.child_count != plan.children.len()
            || receipt.material_count != plan.materials.len()
        {
            return Err(repository("QBANK_GROUP_PUBLISH_INCOMPLETE"));
        }
        Ok(receipt)
    }
}

fn group_import_baseline(plan: &GroupPlan) -> Value {
    json!({
        "schema_version": GROUP_REPORT_SCHEMA_VERSION,
        "document_id": plan.key.document_id,
        "namespace": plan.key.namespace,
        "group_source_id": plan.group_source_id,
        "content_hash": plan.content_hash,
        "materials": plan.materials.iter().map(|material| json!({
            "source_id": material.source_id,
            "content_hash": material.content_hash,
        })).collect::<Vec<_>>(),
    })
}

/// 同身份重放：先校验关系集合未变，再按三方基线更新材料。
fn replay_existing_group(
    conn: &Connection,
    plan: &GroupPlan,
    group_id: &str,
) -> Result<GroupPublishReceipt, GroupPublishError> {
    let existing_materials =
        VfsQuestionGroupRepo::get_materials_with_conn(conn, group_id).map_err(repository)?;
    let existing_children =
        VfsQuestionGroupRepo::get_siblings_with_conn(conn, group_id).map_err(repository)?;

    if existing_children.len() != plan.children.len() {
        return Err(GroupPublishError::Rejected(
            "group_child_set_changed_requires_review".to_string(),
        ));
    }
    for (expected, actual) in plan.children.iter().zip(existing_children.iter()) {
        if expected.sub_label != actual.sub_label || actual.group_order != Some(expected.order) {
            return Err(GroupPublishError::Rejected(
                "group_child_set_changed_requires_review".to_string(),
            ));
        }
    }

    let mut by_source: HashMap<String, &crate::vfs::repos::QuestionGroupMaterial> = HashMap::new();
    for material in &existing_materials {
        if let Some((source_id, _)) = parse_material_ref(material.source_ref.as_deref()) {
            by_source.insert(source_id, material);
        }
    }
    let plan_sources: HashSet<&str> = plan
        .materials
        .iter()
        .map(|material| material.source_id.as_str())
        .collect();
    if by_source.len() != plan_sources.len()
        || !plan_sources
            .iter()
            .all(|source| by_source.contains_key(*source))
    {
        return Err(GroupPublishError::Rejected(
            "group_material_set_changed_requires_review".to_string(),
        ));
    }

    let mut updated = 0usize;
    let mut preserved = Vec::new();
    for material in &plan.materials {
        let row = by_source
            .get(material.source_id.as_str())
            .copied()
            .ok_or_else(|| {
                GroupPublishError::Rejected(
                    "group_material_set_changed_requires_review".to_string(),
                )
            })?;
        if row.order_index != material.order {
            return Err(GroupPublishError::Rejected(
                "material_order_changed_requires_review".to_string(),
            ));
        }
        let (_, baseline_hash) = parse_material_ref(row.source_ref.as_deref())
            .unwrap_or_else(|| (material.source_id.clone(), String::new()));
        let current_hash = material_content_hash(&row.text, &row.images);
        if current_hash != baseline_hash {
            // 用户已编辑：保留用户内容，只报告，不覆盖。
            preserved.push(material.source_id.clone());
            continue;
        }
        if current_hash == material.content_hash {
            // 内容未变，无需写入。
            continue;
        }
        let version = VfsQuestionGroupRepo::get_group_with_conn(conn, group_id)
            .map_err(repository)?
            .ok_or_else(|| GroupPublishError::Rejected("group_missing".to_string()))?
            .version;
        VfsQuestionGroupRepo::update_material_with_conn(
            conn,
            &row.id,
            &crate::vfs::repos::MaterialUpdate {
                content_type: Some(content_type(material.kind)),
                text: Some(material.text.clone()),
                images: Some(material.images.clone()),
                source_ref: Some(material_source_ref(material)),
                page_index: material.page_indices.first().map(|page| *page as i32),
                bbox: Some(material.bbox),
                ..Default::default()
            },
            version,
        )
        .map_err(repository)?;
        updated += 1;
    }

    if updated > 0 {
        let version = VfsQuestionGroupRepo::get_group_with_conn(conn, group_id)
            .map_err(repository)?
            .ok_or_else(|| GroupPublishError::Rejected("group_missing".to_string()))?
            .version;
        VfsQuestionGroupRepo::update_group_with_conn(
            conn,
            group_id,
            &crate::vfs::repos::GroupUpdate {
                import_baseline: Some(group_import_baseline(plan)),
                ..Default::default()
            },
            version,
        )
        .map_err(repository)?;
    }

    let group = VfsQuestionGroupRepo::get_group_with_conn(conn, group_id)
        .map_err(repository)?
        .ok_or_else(|| GroupPublishError::Rejected("group_missing".to_string()))?;
    let materials =
        VfsQuestionGroupRepo::get_materials_with_conn(conn, group_id).map_err(repository)?;
    let children =
        VfsQuestionGroupRepo::get_siblings_with_conn(conn, group_id).map_err(repository)?;
    let mut warnings = Vec::new();
    if !preserved.is_empty() {
        warnings.push("user_modified_materials_preserved".to_string());
    }
    Ok(GroupPublishReceipt {
        group_id: group.id,
        group_version: group.version,
        publish_state: group.publish_state.as_str().to_string(),
        material_count: materials.len(),
        child_count: children.len(),
        material_ids: materials.iter().map(|row| row.id.clone()).collect(),
        child_question_ids: children.iter().map(|row| row.id.clone()).collect(),
        reused: true,
        updated_material_count: updated,
        preserved_material_sources: preserved,
        warnings,
    })
}

// ============================================================================
// 测试（模块内纯测试 + 内存 SQLite 事务测试）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn block(
        source_id: &str,
        kind: ImportBlockKind,
        section: &str,
        label: &str,
        text: &str,
    ) -> TypedBlock {
        TypedBlock {
            source_id: source_id.to_string(),
            kind,
            section: section.to_string(),
            label: label.to_string(),
            raw_text: text.to_string(),
            content: if kind == ImportBlockKind::Question {
                Some(text.to_string())
            } else {
                None
            },
            page_indices: vec![0],
            bbox: [0.0, 0.0, 1.0, 1.0],
            images: vec![],
            continues_from_previous: false,
            continues_to_next: false,
            is_group_hint: false,
            sub_question_labels: vec![],
            declared_kind: None,
        }
    }

    fn question(source_id: &str, section: &str, label: &str) -> TypedBlock {
        block(
            source_id,
            ImportBlockKind::Question,
            section,
            label,
            &format!("题干 {label}"),
        )
    }

    fn material(source_id: &str, section: &str, label: &str) -> TypedBlock {
        block(
            source_id,
            ImportBlockKind::Material,
            section,
            label,
            "阅读材料：共享条件正文",
        )
    }

    fn child(source_id: &str, sub_label: &str) -> ChildRef {
        ChildRef {
            source_id: source_id.to_string(),
            sub_label: Some(sub_label.to_string()),
        }
    }

    fn key() -> GroupImportKey {
        GroupImportKey::explicit_reimport("exam_1", "doc_1")
    }

    fn reading_case() -> (Vec<TypedBlock>, Vec<GroupProposal>) {
        let blocks = vec![
            material("src_material", "Reading", ""),
            question("src_q1", "Reading", "1"),
            question("src_q2", "Reading", "2"),
            question("src_q3", "Reading", "3"),
            question("src_q4", "Reading", "4"),
            question("src_q5", "Reading", "5"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: Some("一篇文章".to_string()),
            material_source_ids: vec!["src_material".to_string()],
            children: (1..=5)
                .map(|index| child(&format!("src_q{index}"), &index.to_string()))
                .collect(),
        };
        (blocks, vec![proposal])
    }

    // g491_reading_one_material_five_children_assembles_single_group：
    // 英语一篇五题：1 组、1 份材料、5 个子题，材料只存一份。
    #[test]
    fn g491_reading_one_material_five_children_assembles_single_group() {
        let (blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        assert_eq!(assembly.plans.len(), 1);
        let plan = &assembly.plans[0];
        assert_eq!(plan.kind, GroupKind::Reading);
        assert_eq!(plan.materials.len(), 1);
        assert_eq!(plan.children.len(), 5);
        assert_eq!(plan.materials[0].order, 1);
        assert!(plan.materials[0].text.contains("共享条件"));
        assert_eq!(plan.children[0].order, 1);
        assert_eq!(plan.children[4].order, 5);
        assert!(assembly.unconsumed.is_empty());
    }

    // g491_math_shared_condition_three_children_keeps_order：
    // 数学共享条件三小题，顺序稳定，子题号保留。
    #[test]
    fn g491_math_shared_condition_three_children_keeps_order() {
        let blocks = vec![
            material("src_cond", "Math", ""),
            question("src_a", "Math", "(1)"),
            question("src_b", "Math", "(2)"),
            question("src_c", "Math", "(3)"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_cond".to_string(),
            kind: GroupKind::SharedCondition,
            title: None,
            material_source_ids: vec!["src_cond".to_string()],
            children: vec![
                child("src_a", "(1)"),
                child("src_b", "(2)"),
                child("src_c", "(3)"),
            ],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        let plan = &assembly.plans[0];
        assert_eq!(
            plan.children
                .iter()
                .map(|c| c.sub_label.clone().unwrap())
                .collect::<Vec<_>>(),
            vec!["(1)", "(2)", "(3)"]
        );
        assert_eq!(
            plan.children.iter().map(|c| c.order).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    fn source_block(
        source_id: &str,
        kind: ImportBlockKind,
        section: &str,
        label: &str,
        text: &str,
    ) -> crate::figure_extractor::QuestionWithFigures {
        crate::figure_extractor::QuestionWithFigures {
            merged: crate::cross_page_merger::MergedQuestion {
                question: crate::vlm_grounding_service::VlmQuestion {
                    block_kind: kind,
                    section: section.to_string(),
                    source_id: source_id.to_string(),
                    label: label.to_string(),
                    raw_text: text.to_string(),
                    ..Default::default()
                },
                page_indices: vec![0],
                figures_with_page: vec![],
                source_spans: vec![],
            },
            images: vec![],
        }
    }

    // g491_blocks_captured_before_answer_merge_keep_material：
    // 材料必须在答案合并/非题过滤前从 merged 块取得；结构化只补题干，不吞材料。
    #[test]
    fn g491_blocks_captured_before_answer_merge_keep_material() {
        let sources = vec![
            source_block(
                "src_material",
                ImportBlockKind::Material,
                "S",
                "",
                "共享材料原文",
            ),
            source_block("src_q1", ImportBlockKind::Question, "S", "1", "题干1"),
            source_block("src_q2", ImportBlockKind::Question, "S", "2", "题干2"),
        ];
        let mut blocks = blocks_before_answer_merge(&sources);
        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].kind, ImportBlockKind::Material);

        let structured = vec![StructuredQuestion {
            json: json!({"content": "结构化题干1", "block_kind": "question"}),
            source: sources[1].clone(),
            origin: crate::llm_structurer::StructureSource::Llm,
        }];
        enrich_with_structured(&mut blocks, &structured);
        assert_eq!(blocks[1].content.as_deref(), Some("结构化题干1"));
        assert_eq!(blocks[0].raw_text, "共享材料原文");
        assert_eq!(blocks[0].content, None);

        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_q1", "1"), child("src_q2", "2")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        assert_eq!(assembly.plans[0].materials.len(), 1);
        assert_eq!(assembly.plans[0].materials[0].kind, MaterialKind::Text);
        assert_eq!(assembly.plans[0].materials[0].text, "共享材料原文");
    }

    // g491_material_block_never_becomes_question：
    // 材料块不在计划子题中，也不会出现在可发布子题列表；材料被当子题引用直接拒绝。
    #[test]
    fn g491_material_block_never_becomes_question() {
        let (mut blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(!assembly.planned_child_sources.contains("src_material"));
        blocks.push(material("src_material_b", "Reading", ""));
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_material_b", "1")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.is_blocking());
        assert!(assembly.conflicts.iter().any(|conflict| {
            conflict.kind == GroupConflictKind::ChildRoleInvalid && conflict.blocking
        }));
        assert!(assembly.plans.is_empty());
    }

    // g491_group_header_never_becomes_question：
    // 组头是题块 → 直接阻断，不能变成「隐藏题目」。
    #[test]
    fn g491_group_header_never_becomes_question() {
        let blocks = vec![question("src_q1", "S", "1"), question("src_q2", "S", "2")];
        let proposal = GroupProposal {
            group_source_id: "src_q1".to_string(),
            kind: GroupKind::Other,
            title: None,
            material_source_ids: vec![],
            children: vec![child("src_q2", "(1)")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::GroupHeaderIsQuestion && c.blocking));
    }

    // g491_missing_child_reference_blocks_whole_group：
    // 组内子题引用不存在 → 整组阻断，其余子题不单独发布。
    #[test]
    fn g491_missing_child_reference_blocks_whole_group() {
        let (blocks, mut proposals) = reading_case();
        proposals[0].children.push(child("src_missing", "6"));
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::MissingChild && c.blocking));
        for index in 1..=5 {
            assert!(assembly
                .blocked_child_sources
                .contains(&format!("src_q{index}")));
        }
    }

    // g491_child_pointing_to_material_is_rejected：
    // 子题引用材料身份 → 角色冲突，整组阻断。
    #[test]
    fn g491_child_pointing_to_material_is_rejected() {
        let blocks = vec![
            material("src_material", "S", ""),
            material("src_second", "S", ""),
            question("src_q1", "S", "1"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_second", "(1)"), child("src_q1", "(2)")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::ChildRoleInvalid && c.blocking));
    }

    // g491_unconsumed_material_reported：
    // 未被引用的材料必须报告，不能静默丢弃。
    #[test]
    fn g491_unconsumed_material_reported() {
        let (mut blocks, proposals) = reading_case();
        blocks.push(material("src_orphan", "Reading", ""));
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert_eq!(assembly.unconsumed.len(), 1);
        assert_eq!(assembly.unconsumed[0].source_id, "src_orphan");
        assert_eq!(assembly.unconsumed[0].reason, "unreferenced");
        let report = assembly.report();
        assert_eq!(report["unconsumed_material_count"], 1);
    }

    // g491_ambiguous_material_association_needs_review：
    // 两个文章候选同时被引用 → 不猜，两篇都保留，整组复核。
    #[test]
    fn g491_ambiguous_material_association_needs_review() {
        let (mut blocks, mut proposals) = reading_case();
        blocks.push(material("src_material_b", "Reading", ""));
        proposals[0]
            .material_source_ids
            .push("src_material_b".to_string());
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        // 本组自身多个材料是合法的「有序多材料」；判定歧义的是跨组争用。
        assert!(!assembly.is_blocking());
        assert_eq!(assembly.plans[0].materials.len(), 2);

        let mut proposals = proposals;
        proposals.push(GroupProposal {
            group_source_id: "src_material_b".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material_b".to_string()],
            children: vec![child("src_q1", "1")],
        });
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly.conflicts.iter().any(|c| {
            c.kind == GroupConflictKind::MaterialClaimedByMultipleGroups && c.blocking
        }));
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| { c.kind == GroupConflictKind::ChildClaimedByMultipleGroups && c.blocking }));
    }

    // g491_ordered_multiple_materials_preserved：
    // 一组多份材料按提案顺序 1..n 保存，内容不合并、不复制。
    #[test]
    fn g491_ordered_multiple_materials_preserved() {
        let blocks = vec![
            material("src_m1", "S", ""),
            material("src_m2", "S", ""),
            question("src_q1", "S", "1"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_m1".to_string(),
            kind: GroupKind::SharedCondition,
            title: None,
            material_source_ids: vec!["src_m1".to_string(), "src_m2".to_string()],
            children: vec![child("src_q1", "(1)")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        let plan = &assembly.plans[0];
        assert_eq!(
            plan.materials
                .iter()
                .map(|m| m.source_id.as_str())
                .collect::<Vec<_>>(),
            vec!["src_m1", "src_m2"]
        );
        assert_eq!(
            plan.materials.iter().map(|m| m.order).collect::<Vec<_>>(),
            vec![1, 2]
        );
    }

    // g491_cross_page_material_kept_once：
    // 跨页合并后的材料只保留一份，页范围完整保留。
    #[test]
    fn g491_cross_page_material_kept_once() {
        let mut passage = material("src_passage", "S", "");
        passage.page_indices = vec![0, 1];
        passage.continues_from_previous = true;
        let blocks = vec![passage, question("src_q1", "S", "1")];
        let proposal = GroupProposal {
            group_source_id: "src_passage".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_passage".to_string()],
            children: vec![child("src_q1", "1")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(!assembly.is_blocking());
        assert_eq!(assembly.plans[0].materials.len(), 1);
        assert_eq!(assembly.plans[0].materials[0].page_indices, vec![0, 1]);
    }

    // g491_cross_section_child_is_ambiguous：
    // 子题章节与组章节不一致 → 阻断，不按「版面最近」硬配。
    #[test]
    fn g491_cross_section_child_is_ambiguous() {
        let blocks = vec![
            material("src_material", "Reading", ""),
            question("src_q1", "Reading", "1"),
            question("src_other", "Math", "1"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_q1", "1"), child("src_other", "2")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::CrossSectionChild && c.blocking));
    }

    // g491_material_section_mismatch_rejected：
    // 材料章节与组章节不一致 → 阻断。
    #[test]
    fn g491_material_section_mismatch_rejected() {
        let blocks = vec![
            material("src_material", "Reading", ""),
            material("src_math", "Math", ""),
            question("src_q1", "Reading", "1"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string(), "src_math".to_string()],
            children: vec![child("src_q1", "1")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::MaterialSectionMismatch));
    }

    // g491_ambiguous_section_without_declared_scope：
    // 组没有章节且子题跨章节 → AmbiguousSection 阻断。
    #[test]
    fn g491_ambiguous_section_without_declared_scope() {
        let blocks = vec![
            material("src_material", "", ""),
            question("src_q1", "A", "1"),
            question("src_q2", "B", "2"),
        ];
        let proposal = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_q1", "1"), child("src_q2", "2")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[proposal]);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::AmbiguousSection));
    }

    // g491_nested_group_rejected：
    // 子题身份同时是另一个组的组头 → 首版不支持嵌套。
    #[test]
    fn g491_nested_group_rejected() {
        let blocks = vec![
            material("src_outer", "S", ""),
            material("src_inner", "S", ""),
            question("src_q1", "S", "1"),
        ];
        let proposals = vec![
            GroupProposal {
                group_source_id: "src_outer".to_string(),
                kind: GroupKind::Reading,
                title: None,
                material_source_ids: vec!["src_outer".to_string()],
                children: vec![child("src_inner", "(1)")],
            },
            GroupProposal {
                group_source_id: "src_inner".to_string(),
                kind: GroupKind::Other,
                title: None,
                material_source_ids: vec![],
                children: vec![child("src_q1", "(1)")],
            },
        ];
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::NestedGroupUnsupported));
    }

    // g491_conflicting_group_identity_rejected：
    // 同一组身份两份不同提案 → 复核，不选一个。
    #[test]
    fn g491_conflicting_group_identity_rejected() {
        let (blocks, mut proposals) = reading_case();
        let mut second = proposals[0].clone();
        second.title = Some("另一个标题".to_string());
        proposals.push(second);
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::ConflictingGroupIdentity));
    }

    // g491_duplicate_child_and_sub_label_rejected：
    // 组内重复子题号/重复顺序 → 不静默去重成「少一题」的组。
    #[test]
    fn g491_duplicate_child_and_sub_label_rejected() {
        let (blocks, mut proposals) = reading_case();
        proposals[0].children[1].sub_label = Some("1".to_string());
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::DuplicateSubLabel && c.blocking));
    }

    // g491_replay_same_identity_is_idempotent：
    // 同身份重放产生同一 group_id / material_id；不同命名空间才产生新身份。
    #[test]
    fn g491_replay_same_identity_is_idempotent() {
        let (blocks, proposals) = reading_case();
        let first = assemble_groups(&key(), &blocks, &proposals);
        let second = assemble_groups(&key(), &blocks, &proposals);
        assert_eq!(first.plans[0].group_id, second.plans[0].group_id);
        assert_eq!(
            material_id_for(&first.plans[0].group_id, "src_material"),
            material_id_for(&second.plans[0].group_id, "src_material")
        );

        let other_session = GroupImportKey::new_session("exam_1", "doc_1", "sess_other");
        let third = assemble_groups(&other_session, &blocks, &proposals);
        assert_ne!(first.plans[0].group_id, third.plans[0].group_id);

        let reimport = GroupImportKey::explicit_reimport("exam_1", "doc_1");
        assert_eq!(key(), reimport);
    }

    // g491_proposals_from_hints_do_not_guess_materials：
    // 旧提示只生成显式子题组；材料只认组头自身，绝不取最近文章。
    #[test]
    fn g491_proposals_from_hints_do_not_guess_materials() {
        let mut header = material("src_header", "S", "");
        header.is_group_hint = true;
        header.sub_question_labels = vec!["1".to_string(), "2".to_string()];
        let mut stray = material("src_stray", "S", "");
        stray.raw_text = "无关的另一篇文章".to_string();
        let blocks = vec![
            stray,
            header,
            question("src_q1", "S", "1"),
            question("src_q2", "S", "2"),
        ];
        let proposals = proposals_from_hints(&blocks);
        assert_eq!(proposals.len(), 1);
        assert_eq!(
            proposals[0].material_source_ids,
            vec!["src_header".to_string()]
        );
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        assert_eq!(assembly.plans[0].children.len(), 2);
        assert_eq!(assembly.unconsumed.len(), 1);
        assert_eq!(assembly.unconsumed[0].source_id, "src_stray");
    }

    // g491_hint_with_ambiguous_label_match_is_not_guessed：
    // 同 section 内同题号两块 → 不猜，组不成立。
    #[test]
    fn g491_hint_with_ambiguous_label_match_is_not_guessed() {
        let mut header = material("src_header", "S", "");
        header.is_group_hint = true;
        header.sub_question_labels = vec!["1".to_string()];
        let blocks = vec![
            header,
            question("src_q1a", "S", "1"),
            question("src_q1b", "S", "1"),
        ];
        let proposals = proposals_from_hints(&blocks);
        assert!(proposals.is_empty());
    }

    // g491_empty_group_and_missing_material_blocked：
    // 没有子题、或 Reading 组没有材料 → 阻断。
    #[test]
    fn g491_empty_group_and_missing_material_blocked() {
        let blocks = vec![
            material("src_material", "S", ""),
            question("src_q1", "S", "1"),
        ];
        let empty = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec!["src_material".to_string()],
            children: vec![],
        };
        let assembly = assemble_groups(&key(), &blocks, &[empty]);
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::EmptyGroup));

        let no_material = GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: None,
            material_source_ids: vec![],
            children: vec![child("src_q1", "(1)")],
        };
        let assembly = assemble_groups(&key(), &blocks, &[no_material]);
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::MissingMaterial && c.blocking));
    }

    // g491_missing_import_identity_reports_materials：
    // 身份不完整时不建组，但材料仍进报告。
    #[test]
    fn g491_missing_import_identity_reports_materials() {
        let (blocks, proposals) = reading_case();
        let bad = GroupImportKey {
            exam_id: "".to_string(),
            document_id: "doc_1".to_string(),
            namespace: "ns".to_string(),
        };
        let assembly = assemble_groups(&bad, &blocks, &proposals);
        assert!(assembly.plans.is_empty());
        assert!(assembly
            .conflicts
            .iter()
            .any(|c| c.kind == GroupConflictKind::MissingImportIdentity));
        assert_eq!(assembly.unconsumed.len(), 1);
    }

    // g491_report_has_no_material_full_text：
    // 报告只保留定位/计数/散列，不回显材料全文。
    #[test]
    fn g491_report_has_no_material_full_text() {
        let (blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        let rendered = assembly.report().to_string();
        assert!(!rendered.contains("共享条件正文"));
        assert!(rendered.contains("content_hash"));
    }

    // g491_publish_request_requires_exact_child_set：
    // 请求与计划必须一一对应，缺一个子题整组失败。
    #[test]
    fn g491_publish_request_requires_exact_child_set() {
        let (blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        let plan = assembly.plans[0].clone();
        let mut children: Vec<PlannedChildQuestion> = plan
            .children
            .iter()
            .map(|child| PlannedChildQuestion {
                binding: plan.child_binding(child),
                source_id: child.source_id.clone(),
                card_id: format!("card_{}", child.source_id),
                question: sample_params(&plan, &child.source_id),
            })
            .collect();
        assert!(GroupPublishRequest::new(plan.clone(), children.clone()).is_ok());
        let missing = children.pop().unwrap();
        let error = GroupPublishRequest::new(plan, children).unwrap_err();
        assert_eq!(error, "child_count_mismatch");
        assert!(!missing.source_id.is_empty());
    }

    // g491_group_publish_uses_same_transaction_and_rolls_back：
    // 组发布在调用方事务内；子题写入失败时组/材料行一起回滚。
    #[test]
    fn g491_group_publish_uses_same_transaction_and_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE questions (id TEXT PRIMARY KEY, exam_id TEXT, group_id TEXT);
            CREATE TABLE question_groups (id TEXT PRIMARY KEY, exam_id TEXT, group_source_id TEXT);
            CREATE TABLE question_group_materials (id TEXT PRIMARY KEY, group_id TEXT, material_order INTEGER);
            "#,
        )
        .unwrap();
        let (blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key(), &blocks, &proposals);
        let plan = assembly.plans[0].clone();
        let children = plan
            .children
            .iter()
            .map(|child| PlannedChildQuestion {
                binding: plan.child_binding(child),
                source_id: child.source_id.clone(),
                card_id: format!("card_{}", child.source_id),
                question: sample_params(&plan, &child.source_id),
            })
            .collect::<Vec<_>>();
        let request = GroupPublishRequest::new(plan, children).unwrap();

        let mut publisher = FakePublisher {
            calls: RefCell::new(Vec::new()),
            fail_on_child: None,
        };
        {
            let tx = conn.transaction().unwrap();
            let receipt = publish_group_plan(&tx, &request, &mut publisher).unwrap();
            assert_eq!(receipt.group_id, request.plan.group_id);
            assert_eq!(receipt.child_count, 5);
            tx.commit().unwrap();
        }
        let groups: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_groups", [], |row| row.get(0))
            .unwrap();
        assert_eq!(groups, 1);
        assert_eq!(publisher.calls.borrow().len(), 1);

        // 第二次：子题写入失败 → 调用方回滚，组与材料都不留存。
        let mut failing = FakePublisher {
            calls: RefCell::new(Vec::new()),
            fail_on_child: Some("src_q3".to_string()),
        };
        {
            let tx = conn.transaction().unwrap();
            let error = publish_group_plan(&tx, &request, &mut failing).unwrap_err();
            assert!(matches!(error, GroupPublishError::Repository(_)));
            drop(tx);
        }
        let groups: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_groups", [], |row| row.get(0))
            .unwrap();
        assert_eq!(groups, 1, "回滚后只保留第一次成功的组");
        let materials: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_group_materials", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(materials, 1);
    }

    // g491_group_source_ref_is_stable_and_namespaced：
    // 同目标题目集显式重引导出同一来源身份串；新 session 命名空间不同。
    #[test]
    fn g491_group_source_ref_is_stable_and_namespaced() {
        let (blocks, proposals) = reading_case();
        let plan = assemble_groups(&key(), &blocks, &proposals).plans[0].clone();
        let again = assemble_groups(&key(), &blocks, &proposals).plans[0].clone();
        assert_eq!(group_source_ref(&plan), group_source_ref(&again));
        assert!(group_source_ref(&plan).contains("doc_1"));

        let other = assemble_groups(
            &GroupImportKey::new_session("exam_1", "doc_1", "sess_x"),
            &blocks,
            &proposals,
        )
        .plans[0]
            .clone();
        assert_ne!(group_source_ref(&plan), group_source_ref(&other));
    }

    // g491_repo_publisher_publishes_in_caller_transaction_and_replays：
    // 真实 VfsQuestionGroupRepo：组/材料/子题一次发布，同身份重放不重复，
    // 软删除身份不复活。使用迁移后的临时数据库，不触碰用户库。
    #[test]
    fn g491_repo_publisher_publishes_in_caller_transaction_and_replays() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        conn.execute(
            "INSERT INTO exam_sheets (id, exam_name, status, temp_id, metadata_json, preview_json, created_at, updated_at) \
             VALUES ('exam_grp', '合成题组', 'pending', 'exam_grp', '{}', '{}', '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z')",
            [],
        )
        .expect("exam row");

        // 发布身份必须与 fixture 中的 exam 一致，否则子题触发
        // `QBANK_GROUP_EXAM_MISSING`（测试曾误用 exam_1）。
        let db_key = GroupImportKey::explicit_reimport("exam_grp", "doc_1");
        let (blocks, proposals) = reading_case();
        let plan = assemble_groups(&db_key, &blocks, &proposals).plans[0].clone();
        let children = plan
            .children
            .iter()
            .map(|child| PlannedChildQuestion {
                binding: plan.child_binding(child),
                source_id: child.source_id.clone(),
                card_id: format!("card_{}", child.source_id),
                question: sample_params(&plan, &child.source_id),
            })
            .collect::<Vec<_>>();
        let request = GroupPublishRequest::new(plan.clone(), children).expect("request");

        let mut publisher = RepoGroupPublisher::new();
        let receipt = {
            let tx = conn.transaction().expect("tx");
            let receipt = publish_group_plan(&tx, &request, &mut publisher).expect("publish");
            tx.commit().expect("commit");
            receipt
        };
        assert!(!receipt.reused);
        assert_eq!(receipt.publish_state, "complete");
        assert_eq!(receipt.material_count, 1);
        assert_eq!(receipt.child_count, 5);
        assert_eq!(receipt.child_question_ids.len(), 5);
        let group_id = receipt.group_id.clone();
        let children_with_group: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE group_id = ?1 AND deleted_at IS NULL",
                rusqlite::params![group_id],
                |row| row.get(0),
            )
            .expect("children count");
        assert_eq!(children_with_group, 5);

        // 同身份重放：不新增组/材料/子题。
        let replay = {
            let tx = conn.transaction().expect("tx");
            let receipt = publish_group_plan(&tx, &request, &mut publisher).expect("replay");
            tx.commit().expect("commit");
            receipt
        };
        assert!(replay.reused);
        assert_eq!(replay.group_id, group_id);
        assert_eq!(replay.child_count, 5);
        let group_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_groups", [], |row| row.get(0))
            .expect("group count");
        assert_eq!(group_rows, 1);

        // 用户删除/排除的组身份不因重导复活。
        conn.execute(
            "UPDATE question_groups SET deleted_at = '2026-10-21T01:00:00Z' WHERE id = ?1",
            rusqlite::params![group_id],
        )
        .expect("soft delete");
        let error = {
            let tx = conn.transaction().expect("tx");
            publish_group_plan(&tx, &request, &mut publisher).unwrap_err()
        };
        assert_eq!(
            error,
            GroupPublishError::Rejected("group_previously_soft_deleted".to_string())
        );
    }

    fn merge_item(
        source_id: &str,
        kind: ImportBlockKind,
        section: &str,
        label: &str,
        text: &str,
    ) -> crate::answer_merge::MergeItem {
        crate::answer_merge::MergeItem::new(label.to_string(), json!({"content": text}))
            .with_source(kind, section, source_id)
            .with_raw_text(text)
    }

    // g491_assemble_from_merge_items_keeps_material：
    // 一站式入口直接吃答案合并输入项，材料不因非题过滤而丢失。
    #[test]
    fn g491_assemble_from_merge_items_keeps_material() {
        let items = vec![
            merge_item(
                "src_material",
                ImportBlockKind::Material,
                "S",
                "",
                "阅读材料原文",
            ),
            merge_item("src_q1", ImportBlockKind::Question, "S", "1", "题干1"),
            merge_item("src_q2", ImportBlockKind::Question, "S", "2", "题干2"),
        ];
        let proposals = vec![GroupProposal {
            group_source_id: "src_material".to_string(),
            kind: GroupKind::Reading,
            title: Some("一篇".to_string()),
            material_source_ids: vec!["src_material".to_string()],
            children: vec![child("src_q1", "1"), child("src_q2", "2")],
        }];
        let assembly = assemble_from_merge_items(&key(), &items, &[], &proposals);
        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        assert_eq!(assembly.plans.len(), 1);
        assert_eq!(assembly.plans[0].materials[0].text, "阅读材料原文");
    }

    // g491_publish_assembled_groups_publishes_in_one_call：
    // 调用方只给子题参数，组/材料/子题一次写入同一事务。
    #[test]
    fn g491_publish_assembled_groups_publishes_in_one_call() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let key = GroupImportKey::explicit_reimport("exam_grp", "doc_1");
        let (blocks, proposals) = reading_case();
        let assembly = assemble_groups(&key, &blocks, &proposals);
        let tx = conn.transaction().expect("tx");
        let outcome = publish_assembled_groups(&tx, &assembly, |plan, child| {
            Some((
                format!("card_{}", child.source_id),
                sample_params(plan, &child.source_id),
            ))
        })
        .expect("publish");
        tx.commit().expect("commit");

        assert_eq!(outcome.receipts.len(), 1);
        assert!(!outcome.is_partial());
        assert_eq!(outcome.published_child_sources.len(), 5);
        let children: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE group_id IS NOT NULL AND deleted_at IS NULL",
                [],
                |row| row.get(0),
            )
            .expect("children");
        assert_eq!(children, 5);
        assert_eq!(outcome.report()["published_group_count"], 1);
    }

    // g491_publish_assembled_groups_skips_blocked_group：
    // 阻断组整组不发布，子题进入 skipped，不允许独立发布。
    #[test]
    fn g491_publish_assembled_groups_skips_blocked_group() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let key = GroupImportKey::explicit_reimport("exam_grp", "doc_1");
        let (blocks, mut proposals) = reading_case();
        proposals[0].children.push(child("src_missing", "6"));
        let assembly = assemble_groups(&key, &blocks, &proposals);
        assert!(assembly.plans.is_empty());

        let tx = conn.transaction().expect("tx");
        let outcome = publish_assembled_groups(&tx, &assembly, |plan, child| {
            Some((
                format!("card_{}", child.source_id),
                sample_params(plan, &child.source_id),
            ))
        })
        .expect("publish");
        tx.commit().expect("commit");

        assert!(outcome.receipts.is_empty());
        assert_eq!(outcome.skipped_child_sources.len(), 6);
        let children: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE group_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("children");
        assert_eq!(children, 0);
    }

    // g491_assemble_and_publish_from_hints_one_call：
    // 最小接入路径：候选集合 + 结构化 → 组装 + 同事务发布，一步完成。
    #[test]
    fn g491_assemble_and_publish_from_hints_one_call() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let header = crate::answer_merge::MergeItem::new(
            String::new(),
            json!({"content": "共享材料", "is_group": true, "sub_questions": ["1", "2"]}),
        )
        .with_source(ImportBlockKind::Material, "S", "src_material")
        .with_raw_text("共享材料");
        let items = vec![
            header,
            merge_item("src_q1", ImportBlockKind::Question, "S", "1", "题干1"),
            merge_item("src_q2", ImportBlockKind::Question, "S", "2", "题干2"),
        ];
        let key = GroupImportKey::explicit_reimport("exam_grp", "doc_1");
        let tx = conn.transaction().expect("tx");
        let (assembly, outcome) =
            assemble_and_publish_from_hints(&tx, &key, &items, &[], |plan, child| {
                Some((
                    format!("card_{}", child.source_id),
                    sample_params(plan, &child.source_id),
                ))
            })
            .expect("assemble+publish");
        tx.commit().expect("commit");

        assert!(!assembly.is_blocking(), "{:?}", assembly.conflicts);
        assert_eq!(outcome.receipts.len(), 1);
        assert_eq!(outcome.published_child_sources.len(), 2);
        let children: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM questions WHERE group_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .expect("children");
        assert_eq!(children, 2);
    }

    fn reading_case_text(text: &str) -> (Vec<TypedBlock>, Vec<GroupProposal>) {
        let (mut blocks, proposals) = reading_case();
        blocks[0].raw_text = text.to_string();
        (blocks, proposals)
    }

    fn seed_exam(conn: &Connection) {
        conn.execute(
            "INSERT INTO exam_sheets (id, exam_name, status, temp_id, metadata_json, preview_json, created_at, updated_at) \
             VALUES ('exam_grp', '合成题组', 'pending', 'exam_grp', '{}', '{}', '2026-10-21T00:00:00Z', '2026-10-21T00:00:00Z')",
            [],
        )
        .expect("exam row");
    }

    fn publish_once(
        conn: &mut Connection,
        blocks: &[TypedBlock],
        proposals: &[GroupProposal],
    ) -> Result<GroupPublishReceipt, GroupPublishError> {
        let key = GroupImportKey::explicit_reimport("exam_grp", "doc_1");
        let assembly = assemble_groups(&key, blocks, proposals);
        assert!(assembly.plans.len() == 1, "{:?}", assembly.conflicts);
        let plan = assembly.plans[0].clone();
        let children = plan
            .children
            .iter()
            .map(|child| PlannedChildQuestion {
                binding: plan.child_binding(child),
                source_id: child.source_id.clone(),
                card_id: format!("card_{}", child.source_id),
                question: sample_params(&plan, &child.source_id),
            })
            .collect::<Vec<_>>();
        let request = GroupPublishRequest::new(plan, children).expect("request");
        let tx = conn.transaction().expect("tx");
        let mut publisher = RepoGroupPublisher::new();
        match publish_group_plan(&tx, &request, &mut publisher) {
            Ok(receipt) => {
                tx.commit().expect("commit");
                Ok(receipt)
            }
            Err(error) => Err(error),
        }
    }

    fn material_text(conn: &Connection, group_id: &str) -> String {
        conn.query_row(
            "SELECT text FROM question_group_materials WHERE group_id = ?1 AND deleted_at IS NULL",
            rusqlite::params![group_id],
            |row| row.get(0),
        )
        .expect("material text")
    }

    // g491_replay_updates_material_when_user_did_not_edit：
    // 三方基线：库内内容 == 上次导入基线 ⇒ 可按新来源安全更新材料。
    #[test]
    fn g491_replay_updates_material_when_user_did_not_edit() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let (blocks_v1, proposals) = reading_case_text("材料原文 V1");
        let first = publish_once(&mut conn, &blocks_v1, &proposals).expect("first publish");
        assert!(!first.reused);
        assert_eq!(material_text(&conn, &first.group_id), "材料原文 V1");

        let (blocks_v2, proposals) = reading_case_text("材料原文 V2");
        let second = publish_once(&mut conn, &blocks_v2, &proposals).expect("replay");
        assert!(second.reused);
        assert_eq!(second.group_id, first.group_id);
        assert_eq!(second.updated_material_count, 1);
        assert!(second.preserved_material_sources.is_empty());
        assert_eq!(material_text(&conn, &first.group_id), "材料原文 V2");
        assert!(second.group_version > first.group_version);
    }

    // g491_replay_preserves_user_edited_material：
    // 三方基线：库内内容 != 上次导入基线 ⇒ 用户改过，保留用户内容并报告。
    #[test]
    fn g491_replay_preserves_user_edited_material() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let (blocks_v1, proposals) = reading_case_text("材料原文 V1");
        let first = publish_once(&mut conn, &blocks_v1, &proposals).expect("first publish");
        conn.execute(
            "UPDATE question_group_materials SET text = '用户改写后的材料' \
             WHERE group_id = ?1 AND deleted_at IS NULL",
            rusqlite::params![first.group_id],
        )
        .expect("user edit");

        let (blocks_v2, proposals) = reading_case_text("材料原文 V2");
        let second = publish_once(&mut conn, &blocks_v2, &proposals).expect("replay");
        assert!(second.reused);
        assert_eq!(second.updated_material_count, 0);
        assert_eq!(
            second.preserved_material_sources,
            vec!["src_material".to_string()]
        );
        assert!(second
            .warnings
            .iter()
            .any(|warning| warning == "user_modified_materials_preserved"));
        assert_eq!(material_text(&conn, &first.group_id), "用户改写后的材料");
    }

    // g491_replay_child_set_change_requires_review：
    // 关系集合变化不静默改，交复核。
    #[test]
    fn g491_replay_child_set_change_requires_review() {
        let (_dir, db) = crate::vfs::database::setup_migrated_test_db();
        let mut conn = db.get_conn_safe().expect("vfs conn");
        seed_exam(&conn);

        let (blocks, proposals) = reading_case();
        let first = publish_once(&mut conn, &blocks, &proposals).expect("first publish");

        let (mut blocks2, mut proposals2) = reading_case();
        proposals2[0].children.pop();
        blocks2.pop();
        let error = publish_once(&mut conn, &blocks2, &proposals2).unwrap_err();
        assert_eq!(
            error,
            GroupPublishError::Rejected("group_child_set_changed_requires_review".to_string())
        );
        let group_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM question_groups", [], |row| row.get(0))
            .expect("group count");
        assert_eq!(group_rows, 1);
        assert!(!first.group_id.is_empty());
    }

    fn sample_params(plan: &GroupPlan, source_id: &str) -> CreateQuestionParams {
        CreateQuestionParams {
            exam_id: plan.key.exam_id.clone(),
            card_id: Some(format!("card_{source_id}")),
            question_label: Some(source_id.to_string()),
            content: format!("题干 {source_id}"),
            options: None,
            answer: None,
            explanation: None,
            structured_data: None,
            question_type: None,
            difficulty: None,
            tags: None,
            source_type: None,
            source_ref: None,
            images: None,
            parent_id: None,
        }
    }

    struct FakePublisher {
        calls: RefCell<Vec<String>>,
        fail_on_child: Option<String>,
    }

    impl GroupPublisher for FakePublisher {
        fn publish_group_bundle(
            &mut self,
            conn: &Connection,
            request: &GroupPublishRequest,
        ) -> Result<GroupPublishReceipt, GroupPublishError> {
            self.calls.borrow_mut().push(request.plan.group_id.clone());
            conn.execute(
                "INSERT INTO question_groups (id, exam_id, group_source_id) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    request.plan.group_id,
                    request.plan.key.exam_id,
                    request.plan.group_source_id
                ],
            )
            .map_err(|error| GroupPublishError::Repository(error.to_string()))?;
            for (index, material_id) in request.material_ids.iter().enumerate() {
                conn.execute(
                    "INSERT INTO question_group_materials (id, group_id, material_order) VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        material_id,
                        request.plan.group_id,
                        index as i64 + 1
                    ],
                )
                .map_err(|error| GroupPublishError::Repository(error.to_string()))?;
            }
            for child in &request.children {
                if self
                    .fail_on_child
                    .as_deref()
                    .is_some_and(|source| source == child.source_id)
                {
                    return Err(GroupPublishError::Repository(
                        "child_insert_failed".to_string(),
                    ));
                }
                conn.execute(
                    "INSERT INTO questions (id, exam_id, group_id) VALUES (?1, ?2, ?3)",
                    rusqlite::params![
                        child.card_id,
                        request.plan.key.exam_id,
                        request.plan.group_id
                    ],
                )
                .map_err(|error| GroupPublishError::Repository(error.to_string()))?;
            }
            Ok(GroupPublishReceipt {
                group_id: request.plan.group_id.clone(),
                group_version: 1,
                publish_state: "complete".to_string(),
                material_count: request.material_ids.len(),
                child_count: request.children.len(),
                material_ids: request.material_ids.clone(),
                child_question_ids: request
                    .children
                    .iter()
                    .map(|child| child.card_id.clone())
                    .collect(),
                reused: false,
                updated_material_count: 0,
                preserved_material_sources: Vec::new(),
                warnings: Vec::new(),
            })
        }
    }
}
