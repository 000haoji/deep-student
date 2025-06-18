/**
 * Chat Message Converter
 * 
 * Utilities for converting between different chat message formats.
 * Single responsibility: Message format conversion.
 */

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking_content?: string;
  timestamp?: string;
}

export interface TauriChatMessage {
  role: string;
  content: string;
  thinking_content?: string;
  timestamp?: string;
}

/**
 * Convert Tauri chat history to standard ChatMessage format
 */
export function convertFromTauriFormat(tauriHistory: TauriChatMessage[]): ChatMessage[] {
  return tauriHistory.map((msg, index) => ({
    id: `msg-${Date.now()}-${index}`,
    role: msg.role as 'user' | 'assistant' | 'system',
    content: msg.content,
    thinking_content: msg.thinking_content,
    timestamp: msg.timestamp || new Date().toISOString()
  }));
}

/**
 * Convert ChatMessage format to Tauri backend format
 */
export function convertToTauriFormat(messages: ChatMessage[]): TauriChatMessage[] {
  return messages.map(msg => ({
    role: msg.role,
    content: msg.content,
    thinking_content: msg.thinking_content,
    timestamp: msg.timestamp || new Date().toISOString()
  }));
}

/**
 * Create a new chat message
 */
export function createChatMessage(
  role: 'user' | 'assistant' | 'system',
  content: string,
  thinking_content?: string
): ChatMessage {
  return {
    id: `msg-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`,
    role,
    content,
    thinking_content,
    timestamp: new Date().toISOString()
  };
}

/**
 * Merge thinking content into a chat message
 */
export function addThinkingToMessage(message: ChatMessage, thinking: string): ChatMessage {
  return {
    ...message,
    thinking_content: (message.thinking_content || '') + thinking
  };
}

/**
 * Update message content
 */
export function updateMessageContent(message: ChatMessage, content: string): ChatMessage {
  return {
    ...message,
    content
  };
}