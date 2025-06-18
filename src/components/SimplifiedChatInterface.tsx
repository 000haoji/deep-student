/**
 * Simplified Chat Interface
 * 
 * Replaces the complex AI SDK implementation with a simpler approach
 * that uses the unified stream handler directly.
 */

import React, { useState, useRef, useEffect } from 'react';
import { useUnifiedStream, StreamMessage, MarkdownRenderer, MessageWithThinking } from '../chat-core';

interface SimplifiedChatInterfaceProps {
  tempId?: string;
  mistakeId?: number;
  initialMessages?: StreamMessage[];
  enableChainOfThought?: boolean;
  onAnalysisStart?: () => void;
  onAnalysisComplete?: (response: any) => void;
  className?: string;
}

export const SimplifiedChatInterface = React.forwardRef<any, SimplifiedChatInterfaceProps>(({
  tempId,
  mistakeId: _mistakeId,
  initialMessages = [],
  enableChainOfThought = true,
  onAnalysisStart,
  onAnalysisComplete,
  className = ''
}, ref) => {
  const [messages, setMessages] = useState<StreamMessage[]>(initialMessages);
  const [input, setInput] = useState('');
  const [currentStreamId, setCurrentStreamId] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  
  const {
    startStream,
    stopStream,
    isStreamActive
  } = useUnifiedStream();

  // Auto-scroll to bottom when new messages appear
  useEffect(() => {
    if (messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, streamProgress]);

  // Update messages from streaming progress
  useEffect(() => {
    if (currentStreamId && streamProgress[currentStreamId]) {
      const progress = streamProgress[currentStreamId];
      setMessages(prev => {
        const updated = [...prev];
        const lastMessage = updated[updated.length - 1];
        
        if (lastMessage && lastMessage.role === 'assistant') {
          // Update the last assistant message with streaming content
          updated[updated.length - 1] = {
            ...lastMessage,
            content: progress.content,
            thinking_content: progress.thinking
          };
        }
        
        return updated;
      });
    }
  }, [currentStreamId, streamProgress]);

  // Handle form submission
  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isStreaming) return;

    const userMessage: StreamMessage = {
      id: `msg-${Date.now()}`,
      role: 'user',
      content: input.trim(),
      timestamp: new Date().toISOString()
    };

    // Add user message immediately
    setMessages(prev => [...prev, userMessage]);
    setInput('');

    // Create placeholder assistant message
    const assistantMessage: StreamMessage = {
      id: `msg-${Date.now() + 1}`,
      role: 'assistant',
      content: '',
      thinking_content: '',
      timestamp: new Date().toISOString()
    };
    
    setMessages(prev => [...prev, assistantMessage]);

    try {
      if (onAnalysisStart) {
        onAnalysisStart();
      }

      const updatedChatHistory = [...messages, userMessage];

      // Start streaming
      const streamId = await startStream(
        'chat_stream',
        JSON.stringify({
          type: tempId ? 'continue_chat' : 'chat',
          tempId,
          chatHistory: updatedChatHistory,
          enableChainOfThought
        }),
        {
          onComplete: (content, thinking) => {
            // Final update with complete content
            setMessages(prev => {
              const updated = [...prev];
              const lastMessage = updated[updated.length - 1];
              
              if (lastMessage && lastMessage.role === 'assistant') {
                updated[updated.length - 1] = {
                  ...lastMessage,
                  content,
                  thinking_content: thinking
                };
              }
              
              return updated;
            });

            setCurrentStreamId(null);
            
            if (onAnalysisComplete) {
              onAnalysisComplete({ content, thinking });
            }
          },
          onError: (error) => {
            console.error('Chat stream error:', error);
            setCurrentStreamId(null);
            
            // Update the last message to show error
            setMessages(prev => {
              const updated = [...prev];
              const lastMessage = updated[updated.length - 1];
              
              if (lastMessage && lastMessage.role === 'assistant') {
                updated[updated.length - 1] = {
                  ...lastMessage,
                  content: `错误: ${error}`
                };
              }
              
              return updated;
            });
          }
        }
      );

      setCurrentStreamId(streamId);
    } catch (error) {
      console.error('Failed to start chat stream:', error);
    }
  };

  // Handle keyboard shortcuts
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e as any);
    }
  };

  // Send message programmatically
  const sendMessage = async (content: string) => {
    setInput(content);
    // Trigger form submission
    setTimeout(() => {
      const form = document.querySelector('.chat-input-form') as HTMLFormElement;
      if (form) {
        form.requestSubmit();
      }
    }, 0);
  };

  // Clear chat
  const clearChat = () => {
    setMessages(initialMessages);
    setInput('');
    if (currentStreamId) {
      stopStream(currentStreamId);
      setCurrentStreamId(null);
    }
  };

  // Stop current stream
  const stopCurrentStream = () => {
    if (currentStreamId) {
      stopStream(currentStreamId);
      setCurrentStreamId(null);
    }
  };

  // Expose methods to parent
  React.useImperativeHandle(ref, () => ({
    sendMessage,
    clearChat,
    stopStream: stopCurrentStream
  }));

  return (
    <div className={`simplified-chat-interface ${className}`}>
      {/* Chat Messages */}
      <div className="chat-messages">
        {messages.map((message, index) => (
          <div key={message.id} className={`message ${message.role}`}>
            <div className="message-header">
              <span className="role">{message.role === 'user' ? '用户' : 'AI助手'}</span>
              <span className="timestamp">
                {new Date(message.timestamp).toLocaleTimeString()}
              </span>
            </div>
            
            <div className="message-content">
              {message.role === 'assistant' ? (
                <MessageWithThinking
                  content={message.content}
                  thinkingContent={message.thinking_content}
                  isStreaming={isStreaming && index === messages.length - 1}
                  role="assistant"
                  timestamp={message.timestamp}
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
        {isStreaming && (
          <div className="streaming-indicator">
            <div className="typing-dots">
              <span></span>
              <span></span>
              <span></span>
            </div>
            <span>AI正在思考和回答中...</span>
          </div>
        )}
        
        {/* Auto-scroll anchor */}
        <div ref={messagesEndRef} />
      </div>
      
      {/* Chat Input */}
      <form onSubmit={handleSubmit} className="chat-input-form">
        <div className="input-container">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="输入您的问题..."
            disabled={isStreaming}
            rows={3}
            className="chat-input"
          />
          
          <div className="input-actions">
            <button
              type="submit"
              disabled={!input.trim() || isStreaming}
              className="send-button"
            >
              {isStreaming ? '发送中...' : '发送'}
            </button>
            
            {isStreaming && (
              <button
                type="button"
                onClick={stopCurrentStream}
                className="stop-button"
              >
                停止
              </button>
            )}
            
            <button
              type="button"
              onClick={clearChat}
              disabled={isStreaming}
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
              // This would need to be controlled by parent component
            }}
            disabled={isStreaming}
          />
          <span>显示思维过程</span>
        </label>
        
        <div className="chat-stats">
          <span>消息数: {messages.length}</span>
          {isStreaming && <span className="analyzing">分析中...</span>}
          {currentStreamId && <span className="stream-id">Stream: {currentStreamId.slice(-8)}</span>}
        </div>
      </div>
    </div>
  );
});