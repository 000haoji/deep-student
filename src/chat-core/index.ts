/**
 * Chat Core Module - 聊天核心模块
 * 
 * 统一导出所有聊天相关的组件、工具函数、类型定义和样式
 * 便于独立开发和迭代
 */

// 导出所有组件
export * from './components';

// 导出工具函数（避免重复导出）
export {
  // 统一API
  api,
  simplifiedApi,
  directApi,
  
  // 流式处理
  streamHandler,
  unifiedStreamManager,
  useUnifiedStream,
  
  // 消息转换
  convertToBackendFormat,
  convertChatHistory,
  
  // AI SDK
  tauriApiAdapter,
  createAISdkFetch,
  convertFromTauriFormat,
  
  // 流式适配器
  createAnalysisStream,
  createChatStream,
  
  // API客户端
  tauriApiClient,
  TauriApiClient
} from './utils';

// 导出类型
export type { StreamChunk } from './utils';

// 导出所有类型定义
export * from './types';

// 导出钩子函数
export * from './hooks';

// 重导出核心样式 - 只需要导入这一个文件即可获得所有样式
export const CHAT_CORE_STYLES = './chat-core/styles/index.css';

// 版本信息
export const CHAT_CORE_VERSION = '1.0.0';

// 核心功能快捷导出
export {
  // 核心聊天组件
  MessageWithThinking,
  MarkdownRenderer,
  StreamingMarkdownRenderer,
  SimplifiedChatInterface,
  StreamingChatInterface,
  AIChatInterface
} from './components';

export type {
  // 核心类型
  ChatMessage,
  ChatMessageContentPart,
  RagSourceInfo
} from './types';

export type {
  StreamMessage,
  StreamOptions,
  StreamRequest,
  StreamState
} from './utils';

export type {
  AnalysisRequest
} from './utils';