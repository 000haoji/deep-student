/**
 * Chat Core Type Definitions
 * 
 * 聊天核心模块的类型定义
 */

// RAG知识库来源信息
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
  type?: string; // MIME type
  size?: number; // 文件大小
  base64_data: string; // 图片的Base64编码
  file?: File; // 原始File对象，可选
}

// 基础聊天消息类型
export interface ChatMessage {
  id?: string; // 消息的唯一ID，前端生成或后端返回
  role: 'user' | 'assistant' | 'system';
  content: string | ChatMessageContentPart[]; // 支持旧的字符串格式和新的多模态数组格式
  timestamp: string;
  thinking_content?: string;
  rag_sources?: RagSourceInfo[];
}

// 流式消息类型
export interface StreamMessage extends ChatMessage {
  isStreaming?: boolean;
  streamId?: string;
}

// 流式数据块类型
export interface StreamChunk {
  content: string;
  is_complete: boolean;
  chunk_id?: string;
  timestamp?: string;
}

// 流式选项类型
export interface StreamOptions {
  enableChainOfThought?: boolean;
  enableRag?: boolean;
  ragOptions?: {
    top_k: number;
    enable_reranking?: boolean;
  };
  autoScroll?: boolean;
  showTypingIndicator?: boolean;
}

// 聊天界面配置
export interface ChatInterfaceConfig {
  enableFullscreen?: boolean;
  enableImageUpload?: boolean;
  maxImages?: number;
  showThinking?: boolean;
  showRagSources?: boolean;
  showTimestamp?: boolean;
  enableKeyboardShortcuts?: boolean;
}

// 思维链相关类型
export interface ThinkingChainOptions {
  enabled: boolean;
  expandedByDefault?: boolean;
  showIcon?: boolean;
  customIcon?: string;
}

// 消息渲染选项
export interface MessageRenderOptions {
  showCursor?: boolean;
  enableLatex?: boolean;
  enableCodeHighlight?: boolean;
  enableTableRender?: boolean;
}