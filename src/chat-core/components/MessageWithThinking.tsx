import React, { useState } from 'react';
import { MarkdownRenderer } from './MarkdownRenderer';
import { StreamingMarkdownRenderer } from './StreamingMarkdownRenderer';
import { SummaryBox } from '../../components/SummaryBox';
import { ChatMessageContentPart } from '../types/chat';
import { ChatMessage } from '../../types';

interface MessageWithThinkingProps {
  content: string | ChatMessageContentPart[];
  thinkingContent?: string;
  isStreaming: boolean;
  role: 'user' | 'assistant';
  timestamp: string;
  ragSources?: Array<{
    document_id: string;
    file_name: string;
    chunk_text: string;
    score: number;
    chunk_index: number;
  }>;
  // 新增：总结框相关props
  showSummaryBox?: boolean;
  chatHistory?: ChatMessage[];
  subject?: string;
  mistakeId?: string;
  reviewSessionId?: string;
  onGenerateSummary?: (summaryPrompt: string) => void;
  currentStreamId?: string;
  isGenerating?: boolean;
  summaryStreamContent?: string;
  summaryStreamComplete?: boolean;
}

export const MessageWithThinking: React.FC<MessageWithThinkingProps> = ({
  content,
  thinkingContent,
  isStreaming,
  role,
  timestamp,
  ragSources,
  showSummaryBox = false,
  chatHistory = [],
  subject,
  mistakeId,
  reviewSessionId,
  onGenerateSummary,
  currentStreamId,
  isGenerating,
  summaryStreamContent,
  summaryStreamComplete
}) => {
  const [isThinkingExpanded, setIsThinkingExpanded] = useState(true);
  const [isRagSourcesExpanded, setIsRagSourcesExpanded] = useState(false);

  // 渲染多模态内容的函数
  const renderContent = (content: string | ChatMessageContentPart[], isStreaming: boolean) => {
    // 处理新的多模态数组格式
    if (Array.isArray(content)) {
      return content.map((part, index) => {
        if (part.type === 'text') {
          return (
            <div key={`text-${index}`} className="content-text-part">
              {isStreaming ? (
                <StreamingMarkdownRenderer 
                  content={part.text} 
                  isStreaming={true}
                />
              ) : (
                <MarkdownRenderer content={part.text} />
              )}
            </div>
          );
        }
        if (part.type === 'image_url' && part.image_url && part.image_url.url) {
          return (
            <div key={`image-${index}`} className="content-image-part">
              <img 
                src={part.image_url.url} 
                alt="上传的内容" 
                style={{ 
                  maxWidth: '100%', 
                  maxHeight: '300px', 
                  display: 'block', 
                  marginTop: '8px', 
                  marginBottom: '8px',
                  borderRadius: '8px',
                  border: '1px solid #e5e7eb'
                }} 
                onError={(e) => {
                  e.currentTarget.style.display = 'none';
                  console.error('图片显示错误:', part.image_url.url.substring(0, 50));
                }}
              />
            </div>
          );
        }
        return null;
      });
    } 
    // 兼容旧的字符串格式
    else if (typeof content === 'string') {
      return isStreaming ? (
        <StreamingMarkdownRenderer 
          content={content} 
          isStreaming={true}
        />
      ) : (
        <MarkdownRenderer content={content} />
      );
    }
    
    // 未知内容格式的占位符
    return <div className="content-unknown">未知内容格式</div>;
  };

  return (
    <div className={`message ${role} ${isStreaming ? 'streaming' : ''}`}>
      {thinkingContent && role === 'assistant' && (
        <div className="thinking-container">
          <div 
            className="thinking-header"
            onClick={() => setIsThinkingExpanded(!isThinkingExpanded)}
          >
            <span className="thinking-icon">🧠</span>
            <span className="thinking-title">AI 思考过程</span>
            <span className="thinking-toggle">
              {isThinkingExpanded ? '▼' : '▶'}
            </span>
          </div>
          {isThinkingExpanded && (
            <div className="thinking-content-box">
              {isStreaming ? (
                <StreamingMarkdownRenderer 
                  content={thinkingContent || ''} 
                  isStreaming={true}
                />
              ) : (
                <MarkdownRenderer content={thinkingContent || ''} />
              )}
            </div>
          )}
        </div>
      )}
      
      <div className="message-content">
        {renderContent(content, isStreaming)}
      </div>

      {ragSources && ragSources.length > 0 && role === 'assistant' && (
        <div className="rag-sources-container" style={{
          marginTop: '12px',
          border: '1px solid #e0e7ff',
          borderRadius: '8px',
          backgroundColor: '#f8faff'
        }}>
          <div 
            className="rag-sources-header"
            onClick={() => setIsRagSourcesExpanded(!isRagSourcesExpanded)}
            style={{
              padding: '8px 12px',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: '6px',
              borderBottom: isRagSourcesExpanded ? '1px solid #e0e7ff' : 'none',
              fontSize: '13px',
              fontWeight: '500',
              color: '#4338ca'
            }}
          >
            <span>📚</span>
            <span>知识库来源 ({ragSources.length})</span>
            <span style={{ marginLeft: 'auto' }}>
              {isRagSourcesExpanded ? '▼' : '▶'}
            </span>
          </div>
          {isRagSourcesExpanded && (
            <div className="rag-sources-content" style={{ padding: '8px 12px' }}>
              {ragSources.map((source, index) => (
                <div 
                  key={`${source.document_id}-${source.chunk_index}`}
                  style={{
                    marginBottom: index < ragSources.length - 1 ? '12px' : '0',
                    padding: '10px',
                    backgroundColor: 'white',
                    borderRadius: '6px',
                    border: '1px solid #e5e7eb'
                  }}
                >
                  <div style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    marginBottom: '6px'
                  }}>
                    <div style={{
                      fontSize: '12px',
                      fontWeight: '600',
                      color: '#374151',
                      display: 'flex',
                      alignItems: 'center',
                      gap: '4px'
                    }}>
                      <span>📄</span>
                      <span>{source.file_name}</span>
                      <span style={{ color: '#6b7280', fontWeight: 'normal' }}>
                        (块 #{source.chunk_index + 1})
                      </span>
                    </div>
                    <div style={{
                      fontSize: '11px',
                      color: '#6b7280',
                      backgroundColor: '#f3f4f6',
                      padding: '2px 6px',
                      borderRadius: '4px'
                    }}>
                      相关度: {Math.round(source.score * 100)}%
                    </div>
                  </div>
                  <div style={{
                    fontSize: '12px',
                    color: '#4b5563',
                    lineHeight: '1.4',
                    fontStyle: 'italic',
                    backgroundColor: '#f9fafb',
                    padding: '8px',
                    borderRadius: '4px',
                    border: '1px solid #f3f4f6'
                  }}>
                    {source.chunk_text.length > 200 
                      ? `${source.chunk_text.substring(0, 200)}...` 
                      : source.chunk_text
                    }
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* 总结框 - 仅在assistant消息且启用时显示 */}
      {showSummaryBox && role === 'assistant' && (
        <SummaryBox
          chatHistory={chatHistory}
          isVisible={true}
          subject={subject}
          mistakeId={mistakeId}
          reviewSessionId={reviewSessionId}
          onGenerateSummary={onGenerateSummary}
          currentStreamId={currentStreamId}
          isGenerating={isGenerating}
          summaryStreamContent={summaryStreamContent}
          summaryStreamComplete={summaryStreamComplete}
        />
      )}
      
      <div className="message-time">
        {new Date(timestamp).toLocaleTimeString()}
      </div>
    </div>
  );
};