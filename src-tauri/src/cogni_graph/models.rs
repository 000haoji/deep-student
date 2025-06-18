use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProblemCard {
    pub id: String,
    pub content_problem: String,
    pub content_insight: String,
    pub status: String, // 'unsolved', 'solved'
    pub embedding: Option<Vec<f32>>,
    pub created_at: DateTime<Utc>,
    pub last_accessed_at: DateTime<Utc>,
    pub access_count: i32,
    pub source_excalidraw_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub tag_type: TagType,
    pub level: i32, // 0=根节点, 1=一级分类, 2=二级分类, etc.
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub enum TagType {
    #[default]
    KnowledgeArea,    // 知识领域 (如: 代数、几何)
    Topic,           // 主题 (如: 函数、三角形)
    Concept,         // 概念 (如: 二次函数、等腰三角形)
    Method,          // 方法 (如: 换元法、辅助线法)
    Difficulty,      // 难度 (如: 基础、进阶、竞赛)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Relationship {
    pub from_id: String,
    pub to_id: String,
    pub relationship_type: RelationshipType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum RelationshipType {
    // ProblemCard 关系
    HasTag,
    IsVariationOf,
    UsesGeneralMethod,
    ContrastsWith,
    
    // Tag 层次关系
    ParentOf,         // 父标签关系
    ChildOf,          // 子标签关系 (ParentOf的反向)
    RelatedTo,        // 相关标签
    PrerequisiteOf,   // 前置知识关系
}

impl ToString for RelationshipType {
    fn to_string(&self) -> String {
        match self {
            RelationshipType::HasTag => "HAS_TAG".to_string(),
            RelationshipType::IsVariationOf => "IS_VARIATION_OF".to_string(),
            RelationshipType::UsesGeneralMethod => "USES_GENERAL_METHOD".to_string(),
            RelationshipType::ContrastsWith => "CONTRASTS_WITH".to_string(),
            RelationshipType::ParentOf => "PARENT_OF".to_string(),
            RelationshipType::ChildOf => "CHILD_OF".to_string(),
            RelationshipType::RelatedTo => "RELATED_TO".to_string(),
            RelationshipType::PrerequisiteOf => "PREREQUISITE_OF".to_string(),
        }
    }
}

impl ToString for TagType {
    fn to_string(&self) -> String {
        match self {
            TagType::KnowledgeArea => "knowledge_area".to_string(),
            TagType::Topic => "topic".to_string(),
            TagType::Concept => "concept".to_string(),
            TagType::Method => "method".to_string(),
            TagType::Difficulty => "difficulty".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: Option<usize>,
    pub libraries: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub card: ProblemCard,
    pub score: f32,
    pub matched_by: Vec<String>, // "vector", "fulltext", "graph"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RecommendationRequest {
    pub card_id: String,
    pub limit: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Recommendation {
    pub card: ProblemCard,
    pub relationship_type: RelationshipType,
    pub confidence: f32,
    pub reasoning: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateCardRequest {
    pub content_problem: String,
    pub content_insight: String,
    pub tags: Vec<String>,
    pub source_excalidraw_path: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateTagRequest {
    pub name: String,
    pub tag_type: TagType,
    pub parent_id: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TagHierarchy {
    pub tag: Tag,
    pub children: Vec<TagHierarchy>,
    pub parent: Option<Tag>,
}

impl ProblemCard {
    pub fn new(
        content_problem: String,
        content_insight: String,
        source_excalidraw_path: Option<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content_problem,
            content_insight,
            status: "unsolved".to_string(),
            embedding: None,
            created_at: now,
            last_accessed_at: now,
            access_count: 0,
            source_excalidraw_path,
        }
    }

    pub fn get_combined_content(&self) -> String {
        format!("{}\n{}", self.content_problem, self.content_insight)
    }

    pub fn mark_accessed(&mut self) {
        self.last_accessed_at = Utc::now();
        self.access_count += 1;
    }
}

impl Tag {
    pub fn new(name: String, tag_type: TagType, level: i32, description: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            tag_type,
            level,
            description,
            created_at: Utc::now(),
        }
    }

    pub fn get_type_string(&self) -> String {
        self.tag_type.to_string()
    }
}