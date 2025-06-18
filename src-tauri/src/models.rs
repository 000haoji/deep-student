use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String, // "user" 或 "assistant"
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rag_sources: Option<Vec<RagSourceInfo>>,
    // 🎯 修复BUG-05：新增图片字段支持多模态对话
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_paths: Option<Vec<String>>, // 用户消息中包含的图片路径
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_base64: Option<Vec<String>>, // 备用：base64编码的图片数据
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagSourceInfo {
    pub document_id: String,
    pub file_name: String,
    pub chunk_text: String,
    pub score: f32,
    pub chunk_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MistakeItem {
    pub id: String,
    pub subject: String,
    pub created_at: DateTime<Utc>,
    pub question_images: Vec<String>, // 本地存储路径
    pub analysis_images: Vec<String>, // 本地存储路径
    pub user_question: String,
    pub ocr_text: String,
    pub tags: Vec<String>,
    pub mistake_type: String,
    pub status: String, // "analyzing", "completed", "error", "summary_required"
    pub updated_at: DateTime<Utc>,
    pub chat_history: Vec<ChatMessage>,
    
    // 新增字段：用于回顾分析的结构化总结
    pub mistake_summary: Option<String>,        // 错题简要解析：题目要点、正确解法、关键知识点
    pub user_error_analysis: Option<String>,    // 用户错误分析：错误原因、思维误区、薄弱点总结
}

#[derive(Debug, Deserialize)]
pub struct AnalysisRequest {
    pub subject: String,
    pub question_image_files: Vec<String>, // base64编码的图片
    pub analysis_image_files: Vec<String>, // base64编码的图片
    pub user_question: String,
    #[serde(default)]
    pub enable_chain_of_thought: bool, // 是否启用思维链
}

// 回顾分析数据库表 - 复用错题分析结构但保留特殊性
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewAnalysisItem {
    pub id: String,                     // 回顾分析ID
    pub name: String,                   // 分析会话名称
    pub subject: String,                // 科目（继承自关联错题）
    pub created_at: DateTime<Utc>,      // 创建时间
    pub updated_at: DateTime<Utc>,      // 更新时间
    pub mistake_ids: Vec<String>,       // 关联的错题ID列表（特殊性）
    pub consolidated_input: String,     // 合并后的输入内容（特殊性）
    pub user_question: String,          // 用户问题描述
    pub status: String,                 // "analyzing", "completed", "error"
    pub chat_history: Vec<ChatMessage>, // 聊天历史（复用通用结构）
    
    // 扩展字段
    pub tags: Vec<String>,              // 分析标签
    pub analysis_type: String,          // 分析类型："consolidated_review"
}

#[derive(Debug, Serialize)]
pub struct AnalysisResponse {
    pub temp_id: String,
    pub initial_data: InitialAnalysisData,
}

#[derive(Debug, Serialize)]
pub struct InitialAnalysisData {
    pub ocr_text: String,
    pub tags: Vec<String>,
    pub mistake_type: String,
    pub first_answer: String,
}

#[derive(Debug, Deserialize)]
pub struct ContinueChatRequest {
    pub temp_id: String,
    pub chat_history: Vec<ChatMessage>,
    pub enable_chain_of_thought: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ContinueChatResponse {
    pub new_assistant_message: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveMistakeRequest {
    pub temp_id: String,
    pub final_chat_history: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
pub struct SaveMistakeResponse {
    pub success: bool,
    pub final_mistake_item: Option<MistakeItem>,
}

// 回顾分析相关结构
#[derive(Debug, Serialize)]
pub struct ReviewSessionResponse {
    pub review_id: String,
    pub analysis_summary: String,
    pub chat_history: Option<Vec<ChatMessage>>,
}

#[derive(Debug, Deserialize)]
pub struct ReviewChatRequest {
    pub review_id: String,
    pub new_message: ChatMessage,
    pub chat_history: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
pub struct ReviewChatResponse {
    pub new_assistant_message: String,
    pub chain_of_thought_details: Option<serde_json::Value>,
}

// 回顾分析会话数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSession {
    pub id: String,
    pub subject: String,
    pub mistake_ids: Vec<String>,
    pub analysis_summary: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub chat_history: Vec<ReviewChatMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewChatMessage {
    pub id: String,
    pub session_id: String,
    pub role: String, // "user" 或 "assistant"
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

// 结构化错误处理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppErrorType {
    Validation,
    Database,
    LLM,
    FileSystem,
    NotFound,
    Configuration,
    Network,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppError {
    pub error_type: AppErrorType,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl AppError {
    pub fn new(error_type: AppErrorType, message: impl Into<String>) -> Self {
        Self {
            error_type,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(error_type: AppErrorType, message: impl Into<String>, details: serde_json::Value) -> Self {
        Self {
            error_type,
            message: message.into(),
            details: Some(details),
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::Validation, message)
    }

    pub fn database(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::Database, message)
    }

    pub fn llm(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::LLM, message)
    }

    pub fn file_system(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::FileSystem, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::NotFound, message)
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::Configuration, message)
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::Network, message)
    }

    pub fn unknown(message: impl Into<String>) -> Self {
        Self::new(AppErrorType::Unknown, message)
    }
}

// 为AppError实现From trait以支持自动转换
impl From<String> for AppError {
    fn from(message: String) -> Self {
        AppError::validation(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        AppError::validation(message.to_string())
    }
}

// 实现Display trait
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

// 实现Error trait
impl std::error::Error for AppError {}

// 实现从其他错误类型的转换
impl From<zip::result::ZipError> for AppError {
    fn from(err: zip::result::ZipError) -> Self {
        AppError::file_system(format!("ZIP操作错误: {}", err))
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::unknown(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        AppError::validation(format!("JSON序列化错误: {}", err))
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::file_system(format!("文件系统错误: {}", err))
    }
}

// 新增：错题总结生成相关结构
#[derive(Debug, Deserialize)]
pub struct GenerateMistakeSummaryRequest {
    pub mistake_id: String,
    pub force_regenerate: Option<bool>, // 是否强制重新生成总结
}

#[derive(Debug, Serialize)]
pub struct GenerateMistakeSummaryResponse {
    pub success: bool,
    pub mistake_summary: Option<String>,
    pub user_error_analysis: Option<String>,
    pub error_message: Option<String>,
}

// anyhow 会自动为实现了 std::error::Error 的类型提供转换

// 统一AI接口的输出结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardModel1Output {
    pub ocr_text: String,
    pub tags: Vec<String>,
    pub mistake_type: String,
    pub raw_response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardModel2Output {
    pub assistant_message: String,
    pub raw_response: Option<String>,
    pub chain_of_thought_details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamChunk {
    pub content: String,
    pub is_complete: bool,
    pub chunk_id: String,
}

// 模型分配结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAssignments {
    pub model1_config_id: Option<String>,
    pub model2_config_id: Option<String>,
    pub review_analysis_model_config_id: Option<String>, // 回顾分析模型配置ID
    pub anki_card_model_config_id: Option<String>, // 新增: ANKI制卡模型配置ID
    pub embedding_model_config_id: Option<String>, // 新增: 第五模型（嵌入模型）配置ID
    pub reranker_model_config_id: Option<String>,  // 新增: 第六模型（重排序模型）配置ID
    pub summary_model_config_id: Option<String>, // 新增: 总结模型配置ID
}

#[derive(Debug, Deserialize)]
pub struct ReviewAnalysisRequest {
    pub subject: String,
    pub mistake_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Statistics {
    pub total_mistakes: i32,
    pub total_reviews: i32,
    pub subject_stats: std::collections::HashMap<String, i32>,
    pub type_stats: std::collections::HashMap<String, i32>,
    pub tag_stats: std::collections::HashMap<String, i32>,
    pub recent_mistakes: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct StartStreamingAnswerRequest {
    pub temp_id: String,
    pub enable_chain_of_thought: bool,
}

// 科目配置系统相关结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectConfig {
    pub id: String,
    pub subject_name: String,
    pub display_name: String,
    pub description: String,
    pub is_enabled: bool,
    pub prompts: SubjectPrompts,
    pub mistake_types: Vec<String>,
    pub default_tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubjectPrompts {
    pub analysis_prompt: String,           // 错题分析提示词
    pub review_prompt: String,             // 回顾分析提示词
    pub chat_prompt: String,               // 对话追问提示词
    pub ocr_prompt: String,                // OCR识别提示词
    pub classification_prompt: String,     // 分类标记提示词
    pub consolidated_review_prompt: String, // 统一回顾分析提示词
    pub anki_generation_prompt: String,    // 新增: ANKI制卡提示词
}

// 默认的科目配置模板
impl Default for SubjectPrompts {
    fn default() -> Self {
        Self {
            analysis_prompt: "请仔细分析这道{subject}错题，提供详细的解题思路和知识点讲解。".to_string(),
            review_prompt: "请分析这些{subject}错题的共同问题和改进建议。".to_string(),
            chat_prompt: "基于这道{subject}题目，请回答学生的问题。".to_string(),
            ocr_prompt: "请识别这张{subject}题目图片中的文字内容。".to_string(),
            classification_prompt: "请分析这道{subject}题目的类型和相关知识点标签。".to_string(),
            consolidated_review_prompt: "你是一个资深{subject}老师，请仔细阅读以下学生提交的多道错题的详细信息（包括题目原文、原始提问和历史交流）。请基于所有这些信息，对学生提出的总体回顾问题进行全面、深入的分析和解答。请注意识别错题间的关联，总结共性问题，并给出针对性的学习建议。".to_string(),
            anki_generation_prompt: "请根据以下{subject}科目的学习内容，生成适合制作Anki卡片的问题和答案对。请创建多样化的卡片类型，包括概念定义、要点列举、关系分析等。每张卡片应包含：\n- front（正面）：简洁明确的问题或概念名\n- back（背面）：详细准确的答案或解释\n- tags（标签）：相关的知识点标签\n\n请以JSON数组格式返回结果，每个对象包含 front、back、tags 三个字段。示例格式：[{\"front\": \"什么是...？\", \"back\": \"...的定义是...\", \"tags\": [\"概念\", \"定义\"]}]".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateSubjectConfigRequest {
    pub subject_name: String,
    pub display_name: String,
    pub description: Option<String>,
    pub prompts: Option<SubjectPrompts>,
    pub mistake_types: Option<Vec<String>>,
    pub default_tags: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSubjectConfigRequest {
    pub id: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub is_enabled: Option<bool>,
    pub prompts: Option<SubjectPrompts>,
    pub mistake_types: Option<Vec<String>>,
    pub default_tags: Option<Vec<String>>,
}

// 回顾分析相关的新结构
#[derive(Debug, Deserialize)]
pub struct StartConsolidatedReviewAnalysisRequest {
    pub subject: String,
    pub consolidated_input: String,
    pub overall_prompt: String,
    pub enable_chain_of_thought: bool,
    pub mistake_ids: Vec<String>, // 参与回顾分析的错题ID列表
}

#[derive(Debug, Serialize)]
pub struct StartConsolidatedReviewAnalysisResponse {
    pub review_session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct TriggerConsolidatedReviewStreamRequest {
    pub review_session_id: String,
    pub enable_chain_of_thought: bool,
}

#[derive(Debug, Deserialize)]
pub struct ContinueConsolidatedReviewStreamRequest {
    pub review_session_id: String,
    pub chat_history: Vec<ChatMessage>,
    pub enable_chain_of_thought: bool,
}

// 临时会话数据结构，用于存储回顾分析过程中的状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidatedReviewSession {
    pub review_session_id: String,
    pub subject: String,
    pub consolidated_input: String,
    pub overall_prompt: String,
    pub enable_chain_of_thought: bool,
    pub created_at: DateTime<Utc>,
    pub chat_history: Vec<ChatMessage>,
    pub mistake_ids: Vec<String>, // 🎯 新增：关联的错题ID列表，用于获取图片信息
}

// ANKI相关结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiGenerationOptions {
    pub deck_name: String,
    pub note_type: String,
    pub enable_images: bool,
    pub max_cards_per_mistake: i32,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f32>,
    // 新增：AI行为参数覆盖值
    #[serde(default)]
    pub max_output_tokens_override: Option<u32>,
    #[serde(default)]
    pub temperature_override: Option<f32>,
    // 新增：模板系统参数
    #[serde(default)]
    pub template_id: Option<String>,
    #[serde(default)]
    pub custom_anki_prompt: Option<String>,
    #[serde(default)]
    pub template_fields: Option<Vec<String>>,
    // 新增：字段提取规则用于动态解析
    #[serde(default)]
    pub field_extraction_rules: Option<std::collections::HashMap<String, FieldExtractionRule>>,
    // 新增：用户自定义制卡要求
    #[serde(default)]
    pub custom_requirements: Option<String>,
    // 新增：任务间重叠区域大小控制
    #[serde(default = "default_overlap_size")]
    pub segment_overlap_size: u32,
    // 新增：用户自定义系统 prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCardGenerationResponse {
    pub success: bool,
    pub cards: Vec<AnkiCard>,
    pub error_message: Option<String>,
}

// 增强的AnkiCard结构体，支持数据库存储和任务关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiCard {
    pub front: String,
    pub back: String,
    #[serde(default)]
    pub text: Option<String>, // 新增：用于Cloze填空题模板
    pub tags: Vec<String>,
    pub images: Vec<String>,
    // 新增字段用于数据库存储和内部管理
    #[serde(default = "default_uuid_id")]
    pub id: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub is_error_card: bool,
    #[serde(default)]
    pub error_content: Option<String>,
    #[serde(default = "default_timestamp")]
    pub created_at: String,
    #[serde(default = "default_timestamp")]
    pub updated_at: String,
    // 新增：扩展字段支持，用于自定义模板
    #[serde(default)]
    pub extra_fields: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub template_id: Option<String>,
}

// 自定义模板系统相关结构体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAnkiTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub version: String,
    pub preview_front: String,
    pub preview_back: String,
    pub note_type: String,
    pub fields: Vec<String>,
    pub generation_prompt: String,
    pub front_template: String,
    pub back_template: String,
    pub css_style: String,
    // 字段解析规则：指定如何从AI输出中提取和验证字段
    pub field_extraction_rules: std::collections::HashMap<String, FieldExtractionRule>,
    // 模板元数据
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub is_active: bool,
    pub is_built_in: bool,
}

// 字段解析规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldExtractionRule {
    pub field_type: FieldType,
    pub is_required: bool,
    pub default_value: Option<String>,
    pub validation_pattern: Option<String>, // 正则表达式
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Array,
    Number,
    Boolean,
}

// 模板创建/更新请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub description: String,
    pub author: Option<String>,
    pub preview_front: String,
    pub preview_back: String,
    pub note_type: String,
    pub fields: Vec<String>,
    pub generation_prompt: String,
    pub front_template: String,
    pub back_template: String,
    pub css_style: String,
    pub field_extraction_rules: std::collections::HashMap<String, FieldExtractionRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub author: Option<String>,
    pub preview_front: Option<String>,
    pub preview_back: Option<String>,
    pub note_type: Option<String>,
    pub fields: Option<Vec<String>>,
    pub generation_prompt: Option<String>,
    pub front_template: Option<String>,
    pub back_template: Option<String>,
    pub css_style: Option<String>,
    pub field_extraction_rules: Option<std::collections::HashMap<String, FieldExtractionRule>>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateImportRequest {
    pub template_data: String, // JSON格式的模板数据
    pub overwrite_existing: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateExportResponse {
    pub template_data: String, // JSON格式的模板数据
    pub filename: String,
}

// DocumentTask 结构体 - 支持文档分段任务管理
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentTask {
    pub id: String, // UUID
    pub document_id: String, // 关联的原始文档ID
    pub original_document_name: String, // 原始文档名，用于UI显示
    pub segment_index: u32, // 在原始文档中的分段序号 (从0开始)
    pub content_segment: String, // 该任务对应的文档内容片段
    pub status: TaskStatus, // 任务状态
    pub created_at: String, // ISO8601 格式时间戳
    pub updated_at: String, // ISO8601 格式时间戳
    pub error_message: Option<String>, // 存储任务级别的错误信息
    pub subject_name: String, // 处理该任务时使用的科目
    pub anki_generation_options_json: String, // 存储处理该任务时使用的选项
}

// 任务状态枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TaskStatus {
    Pending,        // 待处理
    Processing,     // 处理中 (AI正在生成卡片)
    Streaming,      // 正在流式返回卡片 (细化Processing状态)
    Completed,      // 处理完成，所有卡片已生成
    Failed,         // 任务处理失败 (例如，AI调用失败，无法分段等)
    Truncated,      // AI输出因达到最大长度等原因被截断
    Cancelled,      // 用户取消
}

impl TaskStatus {
    pub fn to_db_string(&self) -> String {
        match self {
            TaskStatus::Pending => "Pending".to_string(),
            TaskStatus::Processing => "Processing".to_string(),
            TaskStatus::Streaming => "Streaming".to_string(),
            TaskStatus::Completed => "Completed".to_string(),
            TaskStatus::Failed => "Failed".to_string(),
            TaskStatus::Truncated => "Truncated".to_string(),
            TaskStatus::Cancelled => "Cancelled".to_string(),
        }
    }
}

// 流式卡片数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum StreamedCardPayload {
    NewCard(AnkiCard),          // 一个新生成的、完整的卡片
    NewErrorCard(AnkiCard),     // 一个新生成的、标识错误的卡片
    TaskStatusUpdate { 
        task_id: String, 
        status: TaskStatus, 
        message: Option<String>,
        segment_index: Option<u32>, // 新增: 用于前端关联临时任务
    }, // 任务状态更新
    TaskProcessingError { 
        task_id: String, 
        error_message: String 
    }, // 任务处理过程中的严重错误
    TaskCompleted { 
        task_id: String, 
        final_status: TaskStatus, 
        total_cards_generated: u32 
    }, // 单个任务完成信号
    DocumentProcessingStarted { 
        document_id: String, 
        total_segments: u32 
    }, //整个文档开始处理，告知总任务数
    DocumentProcessingCompleted { 
        document_id: String 
    }, // 整个文档所有任务处理完毕
    RateLimitWarning { 
        message: String, 
        retry_after_seconds: Option<u32> 
    }, // API频率限制警告
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub payload: StreamedCardPayload,
}

// 默认值辅助函数
fn default_uuid_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_timestamp() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn default_overlap_size() -> u32 {
    200 // 默认重叠200个字符
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiExportResponse {
    pub success: bool,
    pub file_path: Option<String>,
    pub card_count: i32,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnkiConnectResult {
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// 新增：ANKI文档制卡请求结构
#[derive(Debug, Deserialize)]
pub struct AnkiDocumentGenerationRequest {
    pub document_content: String,
    pub subject_name: String,
    pub options: Option<AnkiGenerationOptions>,
}

// 新增：ANKI文档制卡响应结构
#[derive(Debug, Serialize)]
pub struct AnkiDocumentGenerationResponse {
    pub success: bool,
    pub cards: Vec<AnkiCard>,
    pub error_message: Option<String>,
}

// ==================== 图片遮罩卡相关数据结构 ====================

// 图片文字识别区域
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRegion {
    pub text: String,                    // 识别到的文字内容
    pub bbox: [f32; 4],                  // 边界框坐标 [x1, y1, x2, y2]
    pub confidence: f32,                 // 识别置信度
    pub region_id: String,               // 区域唯一标识
}

// 图片OCR识别请求
#[derive(Debug, Deserialize)]
pub struct ImageOcrRequest {
    pub image_base64: String,            // base64编码的图片数据
    pub extract_coordinates: bool,       // 是否提取坐标信息
    pub target_text: Option<String>,     // 可选：指定要识别的目标文字
    #[serde(default)]
    pub vl_high_resolution_images: bool, // 是否启用高分辨率模式（Qwen2.5-VL模型）
}

// 图片OCR识别响应
#[derive(Debug, Serialize)]
pub struct ImageOcrResponse {
    pub success: bool,
    pub text_regions: Vec<TextRegion>,   // 识别到的文字区域列表
    pub full_text: String,               // 完整的OCR文字
    pub image_width: u32,                // 图片宽度
    pub image_height: u32,               // 图片高度
    pub error_message: Option<String>,
}

// 遮罩区域定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcclusionMask {
    pub mask_id: String,                 // 遮罩唯一标识
    pub bbox: [f32; 4],                  // 遮罩区域坐标
    pub original_text: String,           // 被遮罩的原始文字
    pub hint: Option<String>,            // 可选提示信息
    pub mask_style: MaskStyle,           // 遮罩样式
}

// 遮罩样式枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MaskStyle {
    SolidColor { color: String },       // 纯色遮罩
    BlurEffect { intensity: u8 },       // 模糊效果
    Rectangle { color: String, opacity: f32 }, // 半透明矩形
}

// 图片遮罩卡片数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageOcclusionCard {
    pub id: String,                      // 卡片ID
    pub task_id: String,                 // 关联任务ID
    pub image_path: String,              // 原始图片路径
    pub image_base64: Option<String>,    // 图片base64数据（用于导出）
    pub image_width: u32,                // 图片尺寸
    pub image_height: u32,
    pub masks: Vec<OcclusionMask>,       // 遮罩区域列表
    pub title: String,                   // 卡片标题
    pub description: Option<String>,     // 卡片描述
    pub tags: Vec<String>,               // 标签
    pub created_at: String,              // 创建时间
    pub updated_at: String,              // 更新时间
    pub subject: String,                 // 学科
}

// 创建图片遮罩卡请求
#[derive(Debug, Deserialize)]
pub struct CreateImageOcclusionRequest {
    pub image_base64: String,            // 图片数据
    pub title: String,                   // 卡片标题
    pub description: Option<String>,     // 卡片描述
    pub subject: String,                 // 学科
    pub tags: Vec<String>,               // 标签
    pub selected_regions: Vec<String>,   // 用户选择的要遮罩的区域ID
    pub mask_style: MaskStyle,           // 遮罩样式
    #[serde(default)]
    pub use_high_resolution: bool,       // 是否使用高分辨率模式
}

// 图片遮罩卡生成响应
#[derive(Debug, Serialize)]
pub struct ImageOcclusionResponse {
    pub success: bool,
    pub card: Option<ImageOcclusionCard>,
    pub error_message: Option<String>,
}

// ==================== RAG相关数据结构 ====================

use std::collections::HashMap;

// 文档块结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DocumentChunk {
    pub id: String, // UUID for the chunk
    pub document_id: String, // ID of the source document
    pub chunk_index: usize, // Order of the chunk within the document
    pub text: String,
    pub metadata: HashMap<String, String>, // e.g., filename, page_number
}

// 带向量的文档块结构
#[derive(Debug, Clone)]
pub struct DocumentChunkWithEmbedding {
    pub chunk: DocumentChunk,
    pub embedding: Vec<f32>,
}

// 检索到的文档块结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RetrievedChunk {
    pub chunk: DocumentChunk,
    pub score: f32, // Similarity score
}

// RAG查询选项
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RagQueryOptions {
    pub top_k: usize,
    pub enable_reranking: Option<bool>,
    // pub filters: Option<HashMap<String, String>>, // Future: metadata-based filtering
}

// 知识库状态结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct KnowledgeBaseStatusPayload {
    pub total_documents: usize,
    pub total_chunks: usize,
    pub embedding_model_name: Option<String>, // Name of the currently used embedding model
    pub vector_store_type: String,
}

// RAG设置结构
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RagSettings {
    pub knowledge_base_path: String, // Path to the vector store / knowledge base files
    pub default_embedding_model_id: Option<String>, // ID of ApiConfig to use for embeddings
    pub default_reranker_model_id: Option<String>, // ID of ApiConfig to use for reranking
    pub default_top_k: usize,
    pub enable_rag_by_default: bool,
}

// RAG增强的分析请求
#[derive(Debug, Deserialize)]
pub struct RagEnhancedAnalysisRequest {
    pub temp_id: String,
    pub enable_chain_of_thought: bool,
    pub enable_rag: Option<bool>,
    pub rag_options: Option<RagQueryOptions>,
}

// RAG增强的对话请求
#[derive(Debug, Deserialize)]
pub struct RagEnhancedChatRequest {
    pub temp_id: String,
    pub chat_history: Vec<ChatMessage>,
    pub enable_chain_of_thought: Option<bool>,
    pub enable_rag: Option<bool>,
    pub rag_options: Option<RagQueryOptions>,
}

// 向量存储统计信息
#[derive(Debug, Clone)]
pub struct VectorStoreStats {
    pub total_documents: usize,
    pub total_chunks: usize,
    pub storage_size_bytes: u64,
}

// 文档上传和处理相关结构
#[derive(Debug, Deserialize)]
pub struct DocumentUploadRequest {
    pub file_paths: Vec<String>,
    pub chunk_size: Option<usize>,
    pub chunk_overlap: Option<usize>,
    pub enable_preprocessing: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct DocumentProcessingStatus {
    pub document_id: String,
    pub file_name: String,
    pub status: DocumentProcessingStage,
    pub progress: f32, // 0.0 to 1.0
    pub error_message: Option<String>,
    pub chunks_processed: usize,
    pub total_chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DocumentProcessingStage {
    Pending,
    Reading,
    Preprocessing,
    Chunking,
    Embedding,
    Storing,
    Completed,
    Failed,
}

// RAG查询响应结构
#[derive(Debug, Serialize)]
pub struct RagQueryResponse {
    pub retrieved_chunks: Vec<RetrievedChunk>,
    pub query_vector_time_ms: u64,
    pub search_time_ms: u64,
    pub reranking_time_ms: Option<u64>,
    pub total_time_ms: u64,
}

// RAG配置结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagConfiguration {
    pub id: String,
    pub chunk_size: i32,
    pub chunk_overlap: i32,
    pub chunking_strategy: String, // "fixed_size" or "semantic"
    pub min_chunk_size: i32,
    pub default_top_k: i32,
    pub default_rerank_enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// RAG配置请求
#[derive(Debug, Deserialize)]
pub struct RagConfigRequest {
    pub chunk_size: i32,
    pub chunk_overlap: i32,
    pub chunking_strategy: String,
    pub min_chunk_size: i32,
    pub default_top_k: i32,
    pub default_rerank_enabled: bool,
}

// RAG配置响应
#[derive(Debug, Serialize)]
pub struct RagConfigResponse {
    pub chunk_size: i32,
    pub chunk_overlap: i32,
    pub chunking_strategy: String,
    pub min_chunk_size: i32,
    pub default_top_k: i32,
    pub default_rerank_enabled: bool,
}

// ==================== RAG多分库相关数据结构 ====================

/// RAG分库/子库实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubLibrary {
    pub id: String, // UUID 主键
    pub name: String, // 分库名称，用户定义
    pub description: Option<String>, // 可选描述
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub document_count: usize, // 文档数量（查询时计算）
    pub chunk_count: usize, // 文本块数量（查询时计算）
}

/// 创建分库请求
#[derive(Debug, Deserialize)]
pub struct CreateSubLibraryRequest {
    pub name: String,
    pub description: Option<String>,
}

/// 更新分库请求
#[derive(Debug, Deserialize)]
pub struct UpdateSubLibraryRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// 删除分库选项
#[derive(Debug, Deserialize)]
pub struct DeleteSubLibraryOptions {
    /// 是否删除包含的文档，默认false（移到默认分库）
    pub delete_contained_documents: Option<bool>,
}

/// 带分库信息的文档上传请求
#[derive(Debug, Deserialize)]
pub struct RagAddDocumentsRequest {
    pub file_paths: Vec<String>,
    pub sub_library_id: Option<String>, // 目标分库ID，None为默认分库
}

/// 带分库信息的Base64文档上传请求
#[derive(Debug, Deserialize)]
pub struct RagAddDocumentsFromContentRequest {
    pub documents: Vec<RagDocumentContent>,
    pub sub_library_id: Option<String>, // 目标分库ID，None为默认分库
}

/// RAG文档内容
#[derive(Debug, Deserialize)]
pub struct RagDocumentContent {
    pub file_name: String,
    pub base64_content: String,
}

/// 带分库过滤的RAG查询选项
#[derive(Debug, Deserialize)]
pub struct RagQueryOptionsWithLibraries {
    pub top_k: usize,
    pub enable_reranking: Option<bool>,
    pub target_sub_library_ids: Option<Vec<String>>, // 目标分库ID列表，None表示查询所有分库
}

/// 获取文档列表请求
#[derive(Debug, Deserialize)]
pub struct GetDocumentsRequest {
    pub sub_library_id: Option<String>, // 分库ID过滤，None表示获取所有文档
    pub page: Option<usize>, // 分页页码
    pub page_size: Option<usize>, // 每页大小
}

/// RAG增强的分析请求（带分库支持）
#[derive(Debug, Deserialize)]
pub struct RagEnhancedAnalysisRequestWithLibraries {
    pub temp_id: String,
    pub enable_chain_of_thought: bool,
    pub enable_rag: Option<bool>,
    pub rag_options: Option<RagQueryOptionsWithLibraries>,
}

/// RAG增强的对话请求（带分库支持）
#[derive(Debug, Deserialize)]
pub struct RagEnhancedChatRequestWithLibraries {
    pub temp_id: String,
    pub chat_history: Vec<ChatMessage>,
    pub enable_chain_of_thought: Option<bool>,
    pub enable_rag: Option<bool>,
    pub rag_options: Option<RagQueryOptionsWithLibraries>,
}
