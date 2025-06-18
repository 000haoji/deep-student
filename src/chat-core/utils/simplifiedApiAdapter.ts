/**
 * Simplified API Adapter
 * 
 * Replaces the complex aiSdkAdapter.ts with a simpler implementation
 * that leverages the unified stream handler.
 */

import { invoke } from '@tauri-apps/api/core';
import { unifiedStreamHandler, StreamRequest, StreamOptions, StreamMessage as _StreamMessage } from './unifiedStreamHandler';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking_content?: string;
  timestamp?: string;
}

export interface AnalysisRequest {
  imageData?: string;
  userInput?: string;
  chatHistory?: ChatMessage[];
  enableChainOfThought?: boolean;
}

/**
 * Simplified API calls that use the unified stream handler
 */
export const simplifiedApi = {
  /**
   * Analyze step by step with streaming
   */
  async analyzeStepByStep(
    imageData: string, 
    userInput: string, 
    options: StreamOptions & { enableChainOfThought?: boolean } = {}
  ): Promise<string> {
    const request: StreamRequest = {
      type: 'analysis',
      imageData,
      userInput,
      enableChainOfThought: options.enableChainOfThought !== false
    };

    return unifiedStreamHandler.startStream(request, options);
  },

  /**
   * Start chat with streaming
   */
  async startChat(
    chatHistory: ChatMessage[] = [],
    options: StreamOptions & { enableChainOfThought?: boolean } = {}
  ): Promise<string> {
    const request: StreamRequest = {
      type: 'chat',
      chatHistory: chatHistory.map(msg => ({
        id: msg.id,
        role: msg.role,
        content: msg.content,
        thinking_content: msg.thinking_content,
        timestamp: msg.timestamp || new Date().toISOString()
      })),
      enableChainOfThought: options.enableChainOfThought !== false
    };

    return unifiedStreamHandler.startStream(request, options);
  },

  /**
   * Continue chat with streaming
   */
  async continueChat(
    tempId: string,
    chatHistory: ChatMessage[] = [],
    options: StreamOptions & { enableChainOfThought?: boolean } = {}
  ): Promise<string> {
    const request: StreamRequest = {
      type: 'continue_chat',
      tempId,
      chatHistory: chatHistory.map(msg => ({
        id: msg.id,
        role: msg.role,
        content: msg.content,
        thinking_content: msg.thinking_content,
        timestamp: msg.timestamp || new Date().toISOString()
      })),
      enableChainOfThought: options.enableChainOfThought !== false
    };

    return unifiedStreamHandler.startStream(request, options);
  },

  /**
   * Stop a specific stream
   */
  stopStream(streamId: string): void {
    unifiedStreamHandler.stopStream(streamId);
  },

  /**
   * Stop all streams
   */
  stopAllStreams(): void {
    unifiedStreamHandler.stopAllStreams();
  },

  /**
   * Get current progress for a stream
   */
  getStreamProgress(streamId: string): { content: string; thinking: string } | null {
    return unifiedStreamHandler.getProgress(streamId);
  },

  /**
   * Check if a stream is active
   */
  isStreamActive(streamId: string): boolean {
    return unifiedStreamHandler.isStreamActive(streamId);
  }
};

/**
 * Non-streaming utility functions for direct backend calls
 */
export const directApi = {
  async getMistakeChatHistory(mistakeId: number): Promise<ChatMessage[]> {
    const result = await invoke('get_mistake_chat_history', { mistakeId });
    return this.convertChatHistory(result as any[]);
  },

  async getAllMistakes(): Promise<any[]> {
    return invoke('get_all_mistakes');
  },

  async saveMistake(mistake: any): Promise<number> {
    return invoke('save_mistake', { mistake });
  },

  async deleteMistake(mistakeId: number): Promise<void> {
    return invoke('delete_mistake', { mistakeId });
  },

  async updateMistake(mistakeId: number, updates: any): Promise<void> {
    return invoke('update_mistake', { mistakeId, updates });
  },

  async getApiConfigurations(): Promise<any> {
    return invoke('get_api_configurations');
  },

  async saveApiConfigurations(configs: any): Promise<void> {
    return invoke('save_api_configurations', { configs });
  },

  /**
   * Convert backend chat history to frontend format
   */
  convertChatHistory(backendHistory: any[]): ChatMessage[] {
    return backendHistory.map((msg, index) => ({
      id: msg.id || `msg-${index}`,
      role: msg.role as 'user' | 'assistant',
      content: msg.content || '',
      thinking_content: msg.thinking_content,
      timestamp: msg.timestamp || new Date().toISOString()
    }));
  },

  /**
   * Convert frontend messages to backend format
   */
  convertToBackendFormat(messages: ChatMessage[]): any[] {
    return messages.map(msg => ({
      role: msg.role,
      content: msg.content,
      thinking_content: msg.thinking_content,
      timestamp: msg.timestamp || new Date().toISOString()
    }));
  }
};

/**
 * Combined API object that includes both streaming and direct calls
 */
export const api = {
  ...simplifiedApi,
  ...directApi
};

// Export individual functions for backward compatibility
export const {
  analyzeStepByStep,
  startChat,
  continueChat,
  stopStream,
  stopAllStreams,
  getStreamProgress,
  isStreamActive
} = simplifiedApi;

export const {
  getMistakeChatHistory,
  getAllMistakes,
  saveMistake,
  deleteMistake,
  updateMistake,
  getApiConfigurations,
  saveApiConfigurations,
  convertChatHistory,
  convertToBackendFormat
} = directApi;