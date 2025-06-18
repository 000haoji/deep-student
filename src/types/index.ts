/**
 * Unified Type Definitions
 * 
 * Consolidates all TypeScript interfaces and types used throughout the application
 * to improve maintainability and consistency.
 */

// ============================================================================
// Core Data Models (matching backend Rust structs)
// ============================================================================

export interface RagSourceInfo {
  document_id: string;
  file_name: string;
  chunk_text: string;
  score: number;
  chunk_index: number;
}

// 多模态内容块类型
export type ChatMessageContentPart = 
  | { type: 'text'; text: string }
  | { type: 'image_url'; image_url: { url: string; detail?: 'low' | 'high' | 'auto' } };

// 追问时上传的图片文件基本信息
export interface UploadedImageInfo {
  id: string; // 临时客户端ID
  name: string;
  type: string; // MIME type
  base64_data: string; // 图片的Base64编码
  file?: File; // 原始File对象，可选
}

// 用于编辑错题时的表单数据结构
export interface MistakeEditForm {
  user_question: string;
  tags: string; // 逗号分隔的字符串
  mistake_type: string;
}

export interface ChatMessage {
  id?: string; // 消息的唯一ID，前端生成或后端返回
  role: 'user' | 'assistant' | 'system';
  content: string | ChatMessageContentPart[]; // 支持旧的字符串格式和新的多模态数组格式
  timestamp: string;
  thinking_content?: string;
  rag_sources?: RagSourceInfo[];
  // 🎯 修复BUG-05：新增图片字段支持多模态对话
  image_paths?: string[]; // 用户消息中包含的图片路径
  image_base64?: string[]; // 备用：base64编码的图片数据
}

export interface MistakeItem {
  id: string;
  subject: string;
  created_at: string;
  question_images: string[]; // 后端返回的本地文件路径
  analysis_images: string[]; // 后端返回的本地文件路径  
  user_question: string;
  ocr_text: string;
  tags: string[];
  mistake_type: string;
  status: string; // "analyzing", "completed", "error", "summary_required"
  updated_at: string;
  chat_history: ChatMessage[];
  // 前端生成的用于显示的图片URLs（从question_images转换而来）
  question_image_urls?: string[];
  
  // 新增字段：错题总结相关
  mistake_summary?: string | null;        // 错题简要解析：题目要点、正确解法、关键知识点
  user_error_analysis?: string | null;    // 用户错误分析：错误原因、思维误区、薄弱点总结
}

export interface SubjectConfig {
  id: string;
  subject_name: string;
  display_name: string;
  description: string;
  is_enabled: boolean;
  prompts: SubjectPrompts;
  mistake_types: string[];
  default_tags: string[];
  created_at: string;
  updated_at: string;
}

export interface SubjectPrompts {
  analysis_prompt: string;
  review_prompt: string;
  chat_prompt: string;
  ocr_prompt: string;
  classification_prompt: string;
  consolidated_review_prompt: string;
  anki_generation_prompt: string;
}

// ============================================================================
// API Request/Response Types
// ============================================================================

export interface AnalysisRequest {
  subject: string;
  question_image_files: string[]; // Base64编码的图片字符串
  analysis_image_files: string[]; // Base64编码的图片字符串
  user_question: string;
  enable_chain_of_thought?: boolean;
}

export interface AnalysisResponse {
  temp_id: string;
  initial_data: InitialAnalysisData;
}

export interface InitialAnalysisData {
  ocr_text: string;
  tags: string[];
  mistake_type: string;
  first_answer: string;
}

export interface ContinueChatRequest {
  temp_id: string;
  chat_history: ChatMessage[];
  enable_chain_of_thought?: boolean;
}

export interface ContinueChatResponse {
  new_assistant_message: string;
}

export interface SaveMistakeRequest {
  temp_id: string;
  final_chat_history: ChatMessage[];
}

export interface SaveMistakeResponse {
  success: boolean;
  final_mistake_item?: MistakeItem;
}

// ============================================================================
// API Configuration Types
// ============================================================================

export interface ApiConfig {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isMultimodal: boolean;
  isReasoning: boolean;
  enabled: boolean;
  modelAdapter: string;
}

export interface ModelAssignments {
  model1_config_id: string | null;
  model2_config_id: string | null;
  review_analysis_model_config_id: string | null;
  anki_card_model_config_id: string | null;
  embedding_model_config_id: string | null;
  reranker_model_config_id: string | null;
}

export type ModelAdapter = 'openai' | 'anthropic' | 'google' | 'custom';

// ============================================================================
// System Settings Types
// ============================================================================

export interface SystemSettings {
  autoSave: boolean;
  theme: 'light' | 'dark' | 'auto';
  language: string;
  enableNotifications: boolean;
  maxChatHistory: number;
  debugMode: boolean;
  enableAnkiConnect: boolean;
}

// ============================================================================
// UI Component Types
// ============================================================================

export interface NotificationMessage {
  id: string;
  type: 'success' | 'error' | 'warning' | 'info';
  title?: string;
  text: string;
  duration?: number;
  persistent?: boolean;
}

export interface StreamChunk {
  content: string;
  is_complete: boolean;
  chunk_id: string;
}

export interface StreamMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking_content?: string;
  timestamp: string;
}

export interface StreamOptions {
  enableChainOfThought?: boolean;
  onChunk?: (chunk: string) => void;
  onThinking?: (thinking: string) => void;
  onComplete?: (fullResponse: string, thinkingContent?: string) => void;
  onError?: (error: string) => void;
  onProgress?: (progress: { content: string; thinking?: string }) => void;
}

export interface StreamRequest {
  type: 'analysis' | 'chat' | 'continue_chat';
  tempId?: string;
  mistakeId?: number;
  imageData?: string;
  userInput?: string;
  chatHistory?: StreamMessage[];
  enableChainOfThought?: boolean;
}

// ============================================================================
// Batch Analysis Types
// ============================================================================

export interface BatchTask {
  id: string; // 唯一ID (前端生成, e.g., UUID)
  subject: string; // 科目
  questionImages: File[]; // 题目图片文件列表
  questionImageUrls?: string[]; // 题目图片预览URL (前端生成)
  analysisImages: File[]; // 解析辅助图片文件列表 (可选)
  analysisImageUrls?: string[]; // 解析辅助图片预览URL (前端生成, 可选)
  userQuestion: string; // 用户问题描述
  
  // 用于持久化存储的Base64数据 (仅在序列化时使用)
  questionImagesBase64?: Array<{
    base64: string;
    name: string;
    type: string;
    size: number;
  }>;
  analysisImagesBase64?: Array<{
    base64: string;
    name: string;
    type: string;
    size: number;
  }>;
  
  status: 'pending' | // 待处理
          'ocr_processing' | // OCR识别与初步分析中
          'awaiting_stream_start' | // 等待AI解答流开始
          'streaming_answer' | // AI解答流式处理中
          'completed' | // 处理完成
          'error_ocr' | // OCR或初步分析阶段错误
          'error_stream' | // AI解答流阶段错误
          'saving'; // 保存到错题库中 (可选状态)
  
  temp_id: string | null; // 后端分析会话ID (来自 analyzeStepByStep)
  ocr_result: { // OCR及初步分类结果 (来自 analyzeStepByStep)
    ocr_text: string;
    tags: string[];
    mistake_type: string;
  } | null;
  chatHistory: ChatMessage[]; // 该任务的完整聊天记录 (包括AI的回答和用户的追问)
  thinkingContent: Map<number, string>; // 思维链内容 (key为chatHistory中的消息索引)
  
  // 用于流式处理的临时状态 (在BatchTask对象内部管理，而非全局)
  currentFullContentForStream: string; 
  currentThinkingContentForStream: string;

  errorDetails?: string; // 详细错误信息
  progress?: number; // 0-100, 用于流式处理时的进度显示 (可选)
}

// ============================================================================
// Batch Operations Types
// ============================================================================

export interface BatchOperationResult {
  success: boolean;
  processed_count: number;
  message: string;
}

export interface BatchDeleteRequest {
  mistake_ids: string[];
}

export interface BatchUpdateStatusRequest {
  updates: Record<string, string>; // mistake_id -> new_status
}

export interface BatchUpdateTagsRequest {
  updates: Record<string, string[]>; // mistake_id -> new_tags
}

export interface BatchCleanupRequest {
  archive_days?: number;
}

export interface BatchCleanupResult {
  orphaned_messages_cleaned: number;
  mistakes_archived: number;
  message: string;
}

export interface BatchExportRequest {
  mistake_ids: string[];
}

// ============================================================================
// Database Query Types
// ============================================================================

export interface OptimizedGetMistakesRequest {
  subject_filter?: string;
  type_filter?: string;
  tags_filter?: string[];
  limit?: number;
  offset?: number;
}

export interface FullTextSearchRequest {
  search_term: string;
  subject_filter?: string;
  limit?: number;
}

export interface DateRangeRequest {
  start_date: string; // RFC3339 format
  end_date: string;   // RFC3339 format
  subject_filter?: string;
}

// ============================================================================
// Statistics Types
// ============================================================================

export interface Statistics {
  total_mistakes: number;
  total_reviews: number;
  subject_stats: Record<string, number>;
  type_stats: Record<string, number>;
  tag_stats: Record<string, number>;
  recent_mistakes: any[];
}

// ============================================================================
// File Management Types
// ============================================================================

export interface ImageFile {
  file: File;
  preview: string;
  id: string;
}

export interface FileUploadResult {
  success: boolean;
  path?: string;
  error?: string;
}

// ============================================================================
// Component Props Types
// ============================================================================

export interface BaseComponentProps {
  className?: string;
  children?: React.ReactNode;
}

export interface ModalProps extends BaseComponentProps {
  isOpen: boolean;
  onClose: () => void;
  title?: string;
}

export interface FormProps extends BaseComponentProps {
  onSubmit: (data: any) => void | Promise<void>;
  loading?: boolean;
  disabled?: boolean;
}

export interface ListProps<T> extends BaseComponentProps {
  items: T[];
  renderItem: (item: T, index: number) => React.ReactNode;
  loading?: boolean;
  empty?: React.ReactNode;
}

export interface PaginationProps {
  currentPage: number;
  totalPages: number;
  onPageChange: (page: number) => void;
  pageSize?: number;
  total?: number;
}

// ============================================================================
// Error Handling Types
// ============================================================================

export interface AppError {
  type: 'validation' | 'network' | 'server' | 'unknown';
  message: string;
  details?: any;
  code?: string;
}

export type AsyncResult<T> = {
  data?: T;
  error?: AppError;
  loading: boolean;
};

// ============================================================================
// Hook Return Types
// ============================================================================

export interface UseApiConfigReturn {
  apiConfigs: ApiConfig[];
  modelAssignments: ModelAssignments;
  loading: boolean;
  saving: boolean;
  testingApi: string | null;
  loadApiConfigs: () => Promise<void>;
  saveApiConfigs: (configs: ApiConfig[]) => Promise<boolean>;
  saveModelAssignments: (assignments: ModelAssignments) => Promise<boolean>;
  testApiConnection: (config: ApiConfig) => Promise<boolean>;
  addApiConfig: (config: Omit<ApiConfig, 'id'>) => ApiConfig;
  updateApiConfig: (id: string, updates: Partial<ApiConfig>) => ApiConfig | undefined;
  deleteApiConfig: (id: string) => ApiConfig[];
  getMultimodalConfigs: () => ApiConfig[];
  getEnabledConfigs: () => ApiConfig[];
  getConfigById: (id: string | null) => ApiConfig | null;
  validateModelAssignments: () => string[];
}

export interface UseSystemSettingsReturn {
  settings: SystemSettings;
  loading: boolean;
  saving: boolean;
  loadSettings: () => Promise<void>;
  saveSetting: <K extends keyof SystemSettings>(key: K, value: SystemSettings[K]) => Promise<boolean>;
  saveAllSettings: (newSettings: SystemSettings) => Promise<boolean>;
  resetSettings: () => Promise<boolean>;
  updateSetting: <K extends keyof SystemSettings>(key: K, value: SystemSettings[K]) => void;
  updateSettings: (updates: Partial<SystemSettings>) => void;
  applyTheme: (theme: string) => Promise<boolean>;
  validateSettings: (settingsToValidate: Partial<SystemSettings>) => string[];
  getSettingsSummary: () => any;
  isAutoSaveEnabled: boolean;
  isDarkTheme: boolean;
  isDebugMode: boolean;
}

export interface UseNotificationReturn {
  notifications: NotificationMessage[];
  hasNotifications: boolean;
  showNotification: (type: NotificationMessage['type'], text: string, options?: any) => string;
  removeNotification: (id: string) => void;
  clearAllNotifications: () => void;
  updateNotification: (id: string, updates: Partial<Omit<NotificationMessage, 'id'>>) => void;
  showSuccess: (text: string, options?: any) => string;
  showError: (text: string, options?: any) => string;
  showWarning: (text: string, options?: any) => string;
  showInfo: (text: string, options?: any) => string;
  showLoading: (text: string, options?: any) => string;
  hasNotificationType: (type: NotificationMessage['type']) => boolean;
  getNotificationCount: (type?: NotificationMessage['type']) => number;
  showBatchResult: (results: { success: number; failed: number; total: number }, operation: string) => void;
  showOperationProgress: (operation: string, current: number, total: number) => string;
}

// ============================================================================
// Utility Types
// ============================================================================

export type DeepPartial<T> = {
  [P in keyof T]?: T[P] extends object ? DeepPartial<T[P]> : T[P];
};

export type RequiredKeys<T, K extends keyof T> = T & Required<Pick<T, K>>;

export type OptionalKeys<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;

export type KeyOf<T> = keyof T;

export type ValueOf<T> = T[keyof T];

// ============================================================================
// Event Types
// ============================================================================

export interface CustomEvent<T = any> {
  type: string;
  payload: T;
  timestamp: number;
}

export type EventHandler<T = any> = (event: CustomEvent<T>) => void;

// ============================================================================
// Theme Types
// ============================================================================

export interface ThemeColors {
  primary: string;
  secondary: string;
  accent: string;
  background: string;
  surface: string;
  text: string;
  textSecondary: string;
  border: string;
  error: string;
  warning: string;
  success: string;
  info: string;
}

export interface Theme {
  name: string;
  colors: ThemeColors;
  spacing: {
    xs: string;
    sm: string;
    md: string;
    lg: string;
    xl: string;
  };
  typography: {
    fontFamily: string;
    fontSize: {
      xs: string;
      sm: string;
      md: string;
      lg: string;
      xl: string;
    };
  };
  borderRadius: string;
  shadows: {
    sm: string;
    md: string;
    lg: string;
  };
}

// ============================================================================
// Review Analysis Types (new feature)
// ============================================================================

export interface ReviewSessionTask {
  id: string; // 回顾分析会话的唯一ID (前端生成或后端返回后设置)
  name: string; // 用户为该回顾任务设定的名称 (e.g., "期中数学复习 - 函数部分")
  creationDate: string; // 创建日期 (ISO 8601)
  subject: string; // 所属科目
  mistakeIds: string[]; // 参与此次回顾分析的原始错题ID列表
  
  userConsolidatedInput: string; // 前端整合的包含多个错题信息和用户引导问题的长文本
  userOverallPrompt: string; // 用户针对此次回顾的总体性问题或分析指引

  status: 'pending' |         // 待处理（刚创建，未开始分析）
          'processing_setup' | // 后端正在设置会话
          'awaiting_stream_start' | // 等待AI解答流开始
          'streaming_answer' |    // AI解答流式处理中
          'completed' |           // 处理完成
          'error_setup' |         // 后端设置阶段错误
          'error_stream';         // AI解答流阶段错误
          
  review_session_id: string | null; // 后端分析会话ID (用于流式通信)
  
  // 存储针对整合后输入的单一AI分析结果
  chatHistory: ChatMessage[]; // 本次回顾分析的完整聊天记录 (用户总体问题 + AI统一回答 + 后续追问)
  thinkingContent: Map<number, string>; // 本次回顾分析的思维链内容 (key为chatHistory中的消息索引)
  
  // 用于流式处理的临时状态
  currentFullContentForStream: string; 
  currentThinkingContentForStream: string;

  errorDetails?: string; // 详细错误信息
}

// 回顾分析相关的API请求/响应类型
export interface StartConsolidatedReviewAnalysisRequest {
  subject: string;
  consolidatedInput: string;
  overallPrompt: string;
  enableChainOfThought: boolean;
  mistakeIds: string[]; // 参与回顾分析的错题ID列表
}

export interface StartConsolidatedReviewAnalysisResponse {
  review_session_id: string;
}

export interface TriggerConsolidatedReviewStreamRequest {
  reviewSessionId: string;
  enableChainOfThought: boolean;
}

export interface ContinueConsolidatedReviewStreamRequest {
  reviewSessionId: string;
  chatHistory: ChatMessage[];
  enableChainOfThought: boolean;
}

// 数据整合引擎使用的类型
export interface MistakeConsolidationData {
  mistakeId: string;
  ocr_text: string;
  user_question: string;
  chat_history: ChatMessage[];
}

export interface ConsolidatedMistakeData {
  selectedMistakes: MistakeConsolidationData[];
  consolidatedText: string;
  userOverallPrompt: string;
}

// ============================================================================
// Mistake Summary Generation Types
// ============================================================================

export interface GenerateMistakeSummaryRequest {
  mistake_id: string;
  force_regenerate?: boolean; // 是否强制重新生成总结
}

export interface GenerateMistakeSummaryResponse {
  success: boolean;
  mistake_summary?: string | null;
  user_error_analysis?: string | null;
  error_message?: string | null;
}

// ============================================================================
// ANKI Card Generation Types
// ============================================================================

export interface AnkiCard {
  front: string;
  back: string;
  text?: string; // 填空题使用的字段
  tags: string[];
  images: string[];
}

// ANKI卡片模板定义
export interface AnkiCardTemplate {
  id: string;
  name: string;
  description: string;
  preview_front: string;
  preview_back: string;
  front_template: string;
  back_template: string;
  css_style: string;
  note_type: string; // 对应的Anki笔记类型
  generation_prompt: string; // 每个模板专门的生成prompt
  fields: string[]; // 模板包含的字段列表
}

// 自定义ANKI模板系统类型定义
export type FieldType = 'Text' | 'Array' | 'Number' | 'Boolean';

export interface FieldExtractionRule {
  field_type: FieldType;
  is_required: boolean;
  default_value?: string;
  validation_pattern?: string; // 正则表达式
  description: string;
}

export interface CustomAnkiTemplate {
  id: string;
  name: string;
  description: string;
  author?: string;
  version: string;
  preview_front: string;
  preview_back: string;
  note_type: string;
  fields: string[];
  generation_prompt: string;
  front_template: string;
  back_template: string;
  css_style: string;
  field_extraction_rules: Record<string, FieldExtractionRule>;
  created_at: string;
  updated_at: string;
  is_active: boolean;
  is_built_in: boolean;
}

export interface CreateTemplateRequest {
  name: string;
  description: string;
  author?: string;
  preview_front: string;
  preview_back: string;
  note_type: string;
  fields: string[];
  generation_prompt: string;
  front_template: string;
  back_template: string;
  css_style: string;
  field_extraction_rules: Record<string, FieldExtractionRule>;
}

export interface UpdateTemplateRequest {
  name?: string;
  description?: string;
  author?: string;
  preview_front?: string;
  preview_back?: string;
  note_type?: string;
  fields?: string[];
  generation_prompt?: string;
  front_template?: string;
  back_template?: string;
  css_style?: string;
  field_extraction_rules?: Record<string, FieldExtractionRule>;
  is_active?: boolean;
}

export interface TemplateImportRequest {
  template_data: string; // JSON格式的模板数据
  overwrite_existing: boolean;
}

export interface TemplateExportResponse {
  template_data: string; // JSON格式的模板数据
  filename: string;
}

export interface AnkiGenerationOptions {
  deck_name: string;
  note_type: string;
  enable_images: boolean;
  max_cards_per_mistake: number;
  max_tokens?: number;
  temperature?: number;
  template_id?: string; // 选择的模板ID
  custom_anki_prompt?: string; // 模板专用的生成prompt
  template_fields?: string[]; // 模板包含的字段列表
  field_extraction_rules?: Record<string, FieldExtractionRule>; // 字段提取规则用于动态解析
  custom_requirements?: string; // 用户自定义制卡要求
  segment_overlap_size?: number; // 任务间重叠区域大小（字符数）
  system_prompt?: string; // 用户自定义系统 prompt
}

export interface AnkiDocumentGenerationRequest {
  document_content: string;
  subject_name: string;
  options?: AnkiGenerationOptions;
}

export interface AnkiDocumentGenerationResponse {
  success: boolean;
  cards: AnkiCard[];
  error_message?: string;
}

export interface AnkiCardGenerationResponse {
  success: boolean;
  cards: AnkiCard[];
  error_message?: string;
}

export interface AnkiExportResponse {
  success: boolean;
  file_path?: string;
  card_count: number;
  error_message?: string;
}

export interface AnkiConnectResult {
  success: boolean;
  result?: any;
  error?: string;
}

// ============================================================================
// RAG Knowledge Base Types
// ============================================================================

export interface DocumentChunk {
  id: string;
  document_id: string;
  chunk_index: number;
  text: string;
  metadata: Record<string, string>;
}

export interface RetrievedChunk {
  chunk: DocumentChunk;
  score: number;
}

export interface RagQueryOptions {
  top_k: number;
  enable_reranking?: boolean;
}

export interface RagQueryResponse {
  retrieved_chunks: RetrievedChunk[];
  query_vector_time_ms: number;
  search_time_ms: number;
  reranking_time_ms?: number;
  total_time_ms: number;
}

export interface KnowledgeBaseStatusPayload {
  total_documents: number;
  total_chunks: number;
  embedding_model_name?: string;
  vector_store_type: string;
}

export interface DocumentUploadRequest {
  file_paths: string[];
  chunk_size?: number;
  chunk_overlap?: number;
  enable_preprocessing?: boolean;
}

export interface DocumentProcessingStatus {
  document_id: string;
  file_name: string;
  status: DocumentProcessingStage;
  progress: number;
  error_message?: string;
  chunks_processed: number;
  total_chunks: number;
}

export type DocumentProcessingStage =
  | 'Pending'
  | 'Reading'
  | 'Preprocessing'
  | 'Chunking'
  | 'Embedding'
  | 'Storing'
  | 'Completed'
  | 'Failed';

export interface RagSettings {
  knowledge_base_path: string;
  default_embedding_model_id?: string;
  default_reranker_model_id?: string;
  default_top_k: number;
  enable_rag_by_default: boolean;
}

export interface RagEnhancedAnalysisRequest {
  temp_id: string;
  enable_chain_of_thought: boolean;
  enable_rag?: boolean;
  rag_options?: RagQueryOptions;
}

export interface RagEnhancedChatRequest {
  temp_id: string;
  chat_history: ChatMessage[];
  enable_chain_of_thought?: boolean;
  enable_rag?: boolean;
  rag_options?: RagQueryOptions;
}

// RAG文档信息
export interface RagDocument {
  id: string;
  file_name: string;
  file_path?: string;
  file_size?: number;
  total_chunks: number;
  created_at: string;
  updated_at: string;
}

// RAG处理事件
export interface RagProcessingEvent {
  id: string;
  status: string;
  progress: number;
  message: string;
  timestamp: string;
}

export interface RagDocumentStatusEvent {
  document_id: string;
  file_name: string;
  status: DocumentProcessingStage;
  progress: number;
  error_message?: string;
  chunks_processed: number;
  total_chunks: number;
}

// ============================================================================
// Export all types for easy importing
// ============================================================================

// 所有类型都已经在上面定义并导出，无需重复导出