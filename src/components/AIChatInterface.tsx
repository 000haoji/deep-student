/**
 * AI Chat Interface using Vercel AI SDK
 * 
 * This component replaces the custom streaming implementation with
 * Vercel AI SDK's useChat hook for better performance and reliability.
 */

import React, { useState, useMemo } from 'react';
import { useChat } from '@ai-sdk/react';
import { createAISdkFetch, convertFromTauriFormat, MarkdownRenderer, MessageWithThinking } from '../chat-core';

interface AIChatInterfaceProps {
  tempId?: string;
  mistakeId?: number;
  initialMessages?: any[];
  enableChainOfThought?: boolean;
  onAnalysisStart?: () => void;
  onAnalysisComplete?: (response: any) => void;
  className?: string;
}

export const AIChatInterface = React.forwardRef<any, AIChatInterfaceProps>(({
  tempId,
  mistakeId,
  initialMessages = [],
  enableChainOfThought = true,
  onAnalysisStart,
  onAnalysisComplete,
  className = ''
}, ref) => {
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  
  // Convert initial messages to AI SDK format
  const convertedInitialMessages = useMemo(() => 
    convertFromTauriFormat(initialMessages), 
    [initialMessages]
  );
  
  // Determine API endpoint based on props
  const apiEndpoint = useMemo(() => {
    if (tempId) return '/api/continue-chat';
    if (mistakeId) return '/api/mistake-chat';
    return '/api/chat';
  }, [tempId, mistakeId]);
  
  // Initialize AI SDK useChat hook
  const {
    messages,
    input,
    handleInputChange,
    handleSubmit,
    isLoading,
    error,
    reload,
    stop,
    append,
    setMessages
  } = useChat({
    api: apiEndpoint,
    initialMessages: convertedInitialMessages,
    fetch: createAISdkFetch(),
    body: {
      tempId,
      mistakeId,
      enableChainOfThought
    },
    onResponse: (response) => {
      console.log('AI SDK Response received:', response.status);
      if (onAnalysisStart) {
        onAnalysisStart();
      }
      setIsAnalyzing(true);
    },
    onFinish: (message) => {
      console.log('AI SDK Response finished:', message);
      setIsAnalyzing(false);
      if (onAnalysisComplete) {
        onAnalysisComplete(message);
      }
    },
    onError: (error) => {
      console.error('AI SDK Error:', error);
      setIsAnalyzing(false);
    }
  });
  
  // Handle form submission with custom logic
  const onSubmit = async (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    
    if (!input.trim() || isLoading) return;
    
    setIsAnalyzing(true);
    
    try {
      await handleSubmit(e);
    } catch (error) {
      console.error('Chat submission error:', error);
      setIsAnalyzing(false);
    }
  };
  
  // Handle sending messages programmatically
  const sendMessage = async (content: string, role: 'user' | 'assistant' = 'user') => {
    if (!content.trim() || isLoading) return;
    
    setIsAnalyzing(true);
    
    try {
      await append({
        role,
        content,
        id: `msg-${Date.now()}`
      });
    } catch (error) {
      console.error('Send message error:', error);
      setIsAnalyzing(false);
    }
  };
  
  // Expose sendMessage for external use
  React.useImperativeHandle(ref, () => ({
    sendMessage,
    clearChat
  }));
  
  // Clear chat history
  const clearChat = () => {
    setMessages([]);
  };
  
  // Handle keyboard shortcuts
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      onSubmit(e as any);
    }
  };
  
  return (
    <div className={`ai-chat-interface ${className}`}>
      {/* Chat Messages */}
      <div className="chat-messages">
        {messages.map((message, index) => (
          <div key={message.id || index} className={`message ${message.role}`}>
            <div className="message-header">
              <span className="role">{message.role === 'user' ? '用户' : 'AI助手'}</span>
              <span className="timestamp">
                {new Date().toLocaleTimeString()}
              </span>
            </div>
            
            <div className="message-content">
              {message.role === 'assistant' ? (
                <MessageWithThinking
                  content={message.content}
                  thinkingContent={(message as any).thinking_content}
                  isStreaming={isLoading && index === messages.length - 1}
                  role="assistant"
                  timestamp={new Date().toISOString()}
                  ragSources={(message as any).rag_sources}
                />
              ) : (
                <div className="user-message">
                  <MarkdownRenderer content={message.content} />
                </div>
              )}
            </div>
          </div>
        ))}
        
        {/* Loading indicator */}
        {isLoading && (
          <div className="message assistant">
            <div className="message-header">
              <span className="role">AI助手</span>
              <span className="timestamp">正在回复...</span>
            </div>
            <div className="message-content">
              <div className="loading-indicator">
                <div className="typing-dots">
                  <span></span>
                  <span></span>
                  <span></span>
                </div>
                <span>正在分析中...</span>
              </div>
            </div>
          </div>
        )}
        
        {/* Error display */}
        {error && (
          <div className="error-message">
            <div className="error-content">
              <span className="error-icon">⚠️</span>
              <span className="error-text">
                出错了: {error.message}
              </span>
              <button onClick={() => reload()} className="retry-button">
                重试
              </button>
            </div>
          </div>
        )}
      </div>
      
      {/* Chat Input */}
      <form onSubmit={onSubmit} className="chat-input-form">
        <div className="input-container">
          <textarea
            value={input}
            onChange={handleInputChange}
            onKeyDown={handleKeyDown}
            placeholder="输入您的问题..."
            disabled={isLoading}
            rows={3}
            className="chat-input"
          />
          
          <div className="input-actions">
            <button
              type="submit"
              disabled={!input.trim() || isLoading}
              className="send-button"
            >
              {isLoading ? '发送中...' : '发送'}
            </button>
            
            {isLoading && (
              <button
                type="button"
                onClick={stop}
                className="stop-button"
              >
                停止
              </button>
            )}
            
            <button
              type="button"
              onClick={clearChat}
              disabled={isLoading}
              className="clear-button"
            >
              清空
            </button>
          </div>
        </div>
      </form>
      
      {/* Chat Controls */}
      <div className="chat-controls">
        <label className="chain-of-thought-toggle">
          <input
            type="checkbox"
            checked={enableChainOfThought}
            onChange={() => {
              // This would need to be passed up to parent component
              // for now just show the current state
            }}
            disabled={isLoading}
          />
          <span>显示思维过程</span>
        </label>
        
        <div className="chat-stats">
          <span>消息数: {messages.length}</span>
          {isAnalyzing && <span className="analyzing">分析中...</span>}
        </div>
      </div>
    </div>
  );
});

// Export utility functions for external use
export const useChatHelpers = () => {
  return {
    sendMessage: (chatRef: React.RefObject<any>, content: string) => {
      if (chatRef.current?.sendMessage) {
        chatRef.current.sendMessage(content);
      }
    },
    clearChat: (chatRef: React.RefObject<any>) => {
      if (chatRef.current?.clearChat) {
        chatRef.current.clearChat();
      }
    }
  };
};