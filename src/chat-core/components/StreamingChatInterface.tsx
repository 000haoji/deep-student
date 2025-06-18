/**
 * Streaming Chat Interface for AI SDK Analysis
 * 
 * A simplified chat interface optimized for handling Tauri streaming events
 * with proper thinking chain support and real-time rendering.
 */

import React, { useState, useImperativeHandle, useEffect, useRef } from 'react';
import { MessageWithThinking } from './MessageWithThinking';
import { MarkdownRenderer } from './MarkdownRenderer';

interface StreamingChatMessage {
  id: string;
  role: 'user' | 'assistant';
  content: string;
  thinking_content?: string;
  timestamp: string;
  rag_sources?: Array<{
    document_id: string;
    file_name: string;
    chunk_text: string;
    score: number;
    chunk_index: number;
  }>;
}

interface StreamingChatInterfaceProps {
  messages: StreamingChatMessage[];
  isStreaming: boolean;
  enableChainOfThought?: boolean;
  onSendMessage?: (message: string) => void;
  className?: string;
  streamingMode?: 'typewriter' | 'instant'; // 流式输出模式
}

export interface StreamingChatInterfaceRef {
  sendMessage: (content: string) => void;
  clearInput: () => void;
  clearChat: () => void;
  setMessages: (messages: StreamingChatMessage[]) => void;
}

export const StreamingChatInterface = React.forwardRef<StreamingChatInterfaceRef, StreamingChatInterfaceProps>(({
  messages,
  isStreaming,
  enableChainOfThought = true,
  onSendMessage,
  className = '',
  streamingMode = 'typewriter' // 默认使用打字机模式
}, ref) => {
  const [input, setInput] = useState('');
  const [displayedContent, setDisplayedContent] = useState<{[key: string]: string}>({});
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [isAnimating, setIsAnimating] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const typewriterTimeouts = useRef<{[key: string]: number}>({});
  const containerRef = useRef<HTMLDivElement>(null);

  // Handle form submission
  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || isStreaming) return;
    
    if (onSendMessage) {
      onSendMessage(input);
    }
    setInput('');
  };

  // Handle keyboard shortcuts
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSubmit(e as any);
    }
    // ESC键退出全屏
    if (e.key === 'Escape' && isFullscreen) {
      handleToggleFullscreen();
    }
  };

  // 全屏切换功能
  const handleToggleFullscreen = () => {
    if (isAnimating) return;
    
    setIsAnimating(true);
    
    if (isFullscreen) {
      // 退出全屏
      if (containerRef.current) {
        containerRef.current.classList.add('collapsing');
        containerRef.current.classList.remove('fullscreen');
      }
      
      setTimeout(() => {
        setIsFullscreen(false);
        setIsAnimating(false);
        if (containerRef.current) {
          containerRef.current.classList.remove('collapsing');
        }
      }, 500);
    } else {
      // 进入全屏
      setIsFullscreen(true);
      
      if (containerRef.current) {
        containerRef.current.classList.add('expanding');
      }
      
      setTimeout(() => {
        if (containerRef.current) {
          containerRef.current.classList.remove('expanding');
          containerRef.current.classList.add('fullscreen');
        }
        setIsAnimating(false);
      }, 500);
    }
  };

  // 处理全屏快捷键
  useEffect(() => {
    const handleKeyPress = (e: KeyboardEvent) => {
      // F11或Ctrl+Enter切换全屏
      if (e.key === 'F11' || (e.ctrlKey && e.key === 'Enter')) {
        e.preventDefault();
        handleToggleFullscreen();
      }
    };

    if (isFullscreen) {
      document.addEventListener('keydown', handleKeyPress);
      return () => document.removeEventListener('keydown', handleKeyPress);
    }
  }, [isFullscreen]);

  // 打字机效果的流式内容显示
  useEffect(() => {
    if (streamingMode === 'instant') return;

    messages.forEach((message) => {
      if (message.role === 'assistant' && message.content) {
        const messageId = message.id;
        const fullContent = message.content;
        const currentDisplayed = displayedContent[messageId] || '';
        
        // Only start typewriter if content has changed and is longer
        if (fullContent !== currentDisplayed && fullContent.length > currentDisplayed.length) {
          // Clear existing timeout for this message
          if (typewriterTimeouts.current[messageId]) {
            clearTimeout(typewriterTimeouts.current[messageId]);
          }
          
          // Start typewriter from current position
          let currentIndex = currentDisplayed.length;
          
          const typeNextChar = () => {
            if (currentIndex < fullContent.length) {
              const nextContent = fullContent.substring(0, currentIndex + 1);
              setDisplayedContent(prev => ({
                ...prev,
                [messageId]: nextContent
              }));
              currentIndex++;
              
              // 根据字符类型调整显示速度
              const char = fullContent[currentIndex - 1];
              let delay = 25; // Base typing speed
              if (char === ' ') delay = 8;
              else if (char === '.' || char === '!' || char === '?') delay = 150;
              else if (char === ',' || char === ';' || char === ':') delay = 80;
              else if (char === '\n') delay = 100;
              
              typewriterTimeouts.current[messageId] = setTimeout(typeNextChar, delay);
            }
          };
          
          // Start typing animation
          if (currentIndex < fullContent.length) {
            typeNextChar();
          }
        }
      }
    });

    return () => {
      // Cleanup timeouts
      Object.values(typewriterTimeouts.current).forEach(timeout => clearTimeout(timeout));
    };
  }, [messages, streamingMode, displayedContent]);

  // 自动滚动到底部
  useEffect(() => {
    if (messagesEndRef.current) {
      messagesEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [messages, displayedContent]);

  // Get content for display (original or typewriter)
  const getDisplayContent = (message: StreamingChatMessage) => {
    if (message.role === 'user' || streamingMode === 'instant') {
      return message.content;
    }
    return displayedContent[message.id] || '';
  };

  // Expose methods to parent component
  useImperativeHandle(ref, () => ({
    sendMessage: (content: string) => {
      if (onSendMessage) {
        onSendMessage(content);
      }
    },
    clearInput: () => {
      setInput('');
    },
    clearChat: () => {
      setDisplayedContent({});
      Object.values(typewriterTimeouts.current).forEach(timeout => clearTimeout(timeout));
      typewriterTimeouts.current = {};
    },
    setMessages: (_newMessages: StreamingChatMessage[]) => {
      // Reset displayed content for new messages
      setDisplayedContent({});
    }
  }));

  return (
    <div 
      ref={containerRef}
      className={`streaming-chat-interface ${className} ${isFullscreen ? 'fullscreen' : ''}`}
    >
      {/* 全屏工具栏 */}
      <div className="chat-toolbar">
        <div className="toolbar-left">
          <span className="chat-title">💬 AI智能对话</span>
        </div>
        <div className="toolbar-right">
          <button
            onClick={handleToggleFullscreen}
            disabled={isAnimating}
            className="fullscreen-toggle-btn"
            title={isFullscreen ? "退出全屏 (ESC)" : "进入全屏 (F11)"}
          >
            {isFullscreen ? '⤋' : '⤢'}
          </button>
        </div>
      </div>

      {/* Chat Messages */}
      <div className="chat-messages">
        {messages.map((message, index) => (
          <div key={message.id} className={`message-wrapper ${message.role}`}>
            <div className="message-header">
              <span className="role">{message.role === 'user' ? '用户' : 'AI助手'}</span>
              <span className="timestamp">
                {new Date(message.timestamp).toLocaleTimeString()}
              </span>
            </div>

            {message.role === 'assistant' ? (
              <MessageWithThinking
                content={getDisplayContent(message)}
                thinkingContent={message.thinking_content}
                isStreaming={isStreaming && index === messages.length - 1}
                role="assistant"
                timestamp={message.timestamp}
                ragSources={message.rag_sources}
              />
            ) : (
              <div className="user-message-content">
                <MarkdownRenderer content={message.content} />
              </div>
            )}
          </div>
        ))}

        {/* Streaming indicator */}
        {isStreaming && (
          <div className="streaming-indicator">
            <div className="typing-dots">
              <span></span>
              <span></span>
              <span></span>
            </div>
            <span className="streaming-text">AI正在思考和回答中...</span>
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
            placeholder="继续提问..."
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
              {isStreaming ? '回答中...' : '发送'}
            </button>
          </div>
        </div>
      </form>

      {/* Chat Info */}
      <div className="chat-info">
        <div className="chat-stats">
          <span>消息数: {messages.length}</span>
          {isStreaming && <span className="streaming-status">🔄 流式回答中</span>}
          {enableChainOfThought && <span className="thinking-status">🧠 显示思维过程</span>}
        </div>
      </div>
    </div>
  );
});

StreamingChatInterface.displayName = 'StreamingChatInterface';