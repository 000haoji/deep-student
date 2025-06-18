/**
 * Chat Core Utils - 统一工具函数导出
 */

// AI SDK 适配器
export * from './aiSdkAdapter';

// 聊天消息转换器  
export * from './chatMessageConverter';

// 流式处理相关
export { streamHandler } from './streamHandler';
export type { StreamChunk } from './streamHandler';
export { unifiedStreamHandler } from './unifiedStreamHandler';
export { unifiedStreamManager, useUnifiedStream } from './unifiedStreamManager';

// API适配器
export * from './simplifiedApiAdapter';
export * from './tauriStreamAdapter';
export * from './tauriApiClient';

// 重导出常用的接口和函数以便使用
export type {
  ChatMessage,
  AnalysisRequest
} from './simplifiedApiAdapter';

export type {
  StreamOptions,
  StreamRequest,
  StreamState,
  StreamMessage
} from './unifiedStreamManager';

export {
  api,
  simplifiedApi,
  directApi
} from './simplifiedApiAdapter';

