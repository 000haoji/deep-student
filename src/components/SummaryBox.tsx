import React, { useState, useEffect } from 'react';
import { ChatMessage } from '../types';

interface SummaryBoxProps {
  chatHistory: ChatMessage[];
  isVisible: boolean;
  onClose?: () => void;
  subject?: string;
  mistakeId?: string; // 用于错题库详情
  reviewSessionId?: string; // 用于批量分析详情
  // 新增：与AI调用一致的接口
  onGenerateSummary?: (summaryPrompt: string) => void;
  currentStreamId?: string;
  isGenerating?: boolean;
  // 新增：从父组件传递的流式内容
  summaryStreamContent?: string;
  summaryStreamComplete?: boolean;
}

export const SummaryBox: React.FC<SummaryBoxProps> = ({
  chatHistory,
  isVisible,
  onClose,
  subject = '数学',
  mistakeId: _mistakeId,
  reviewSessionId: _reviewSessionId,
  onGenerateSummary,
  isGenerating = false,
  summaryStreamContent = '',
  summaryStreamComplete = false
}) => {
  const [isExpanded, setIsExpanded] = useState(true);
  const [summaryRequested, setSummaryRequested] = useState(false);
  const [summaryContent, setSummaryContent] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);

  // 监听流式内容更新
  useEffect(() => {
    if (summaryStreamContent !== undefined) {
      setSummaryContent(summaryStreamContent);
      setIsStreaming(!summaryStreamComplete);
      if (summaryStreamContent && !summaryRequested) {
        setSummaryRequested(true);
      }
    }
  }, [summaryStreamContent, summaryStreamComplete, summaryRequested]);

  // 🎯 新增：当父组件传入的总结内容发生变化时，重置内部状态 - 配合保活机制
  useEffect(() => {
    // 当切换到不同错题时，重置总结请求状态
    // 这确保了在保活模式下，SummaryBox能正确响应新错题的总结状态
    if (summaryStreamContent === '' && summaryRequested) {
      console.log('🔄 [SummaryBox] 检测到新错题无总结内容，重置请求状态');
      setSummaryRequested(false);
    }
  }, [summaryStreamContent, summaryStreamComplete]);

  // 监听生成状态
  useEffect(() => {
    if (isGenerating) {
      setIsStreaming(true);
    } else if (summaryStreamComplete) {
      setIsStreaming(false);
    }
  }, [isGenerating, summaryStreamComplete]);

  // 如果不可见就不渲染
  if (!isVisible) return null;

  const generateSummary = () => {
    if (chatHistory.length === 0) {
      return;
    }

    if (!onGenerateSummary) {
      console.warn('⚠️ onGenerateSummary回调未提供');
      return;
    }

    // 构建总结提示词
    const chatHistoryText = chatHistory.map(msg => {
      const roleDisplay = msg.role === 'user' ? '用户' : '助手';
      const contentStr = typeof msg.content === 'string' ? msg.content : JSON.stringify(msg.content);
      return `${roleDisplay}: ${contentStr}`;
    }).join('\\n\\n');

    const summaryPrompt = `请基于以下对话记录，生成简洁的学习总结：

对话记录：
${chatHistoryText}

请生成：
1. 题目核心知识点：题目所涉及的知识点具体分类与定位
2. 错误分析：根据聊天上下文，学生存在的主要问题以及误区
3. 学生分析：根据聊天上下文，学生可能的薄弱点位置，需要重点注意的细节
4. 解题方法: 根据聊天上下文，学生应重点掌握的具体解题方法，

要求：
- 严禁使用markdown和latex语法，尤其是**
- 严禁使用markdown和latex语法，尤其是**
- 严禁使用markdown和latex语法，尤其是**
- 内容简洁明了，不长篇大论，不啰嗦，一针见血
- 重点突出学习要点`;

    console.log('📝 准备通过回调生成总结，提示词长度:', summaryPrompt.length);
    setSummaryRequested(true);
    onGenerateSummary(summaryPrompt);
  };

  return (
    <div className="summary-box" style={{
      border: '1px solid #e0e0e0',
      borderRadius: '8px',
      margin: '8px 0',
      backgroundColor: '#f9f9f9',
      fontSize: '14px'
    }}>
      {/* 头部 */}
      <div 
        className="summary-header"
        style={{
          padding: '8px 12px',
          backgroundColor: '#f0f0f0',
          borderRadius: '8px 8px 0 0',
          cursor: 'pointer',
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          borderBottom: isExpanded ? '1px solid #e0e0e0' : 'none'
        }}
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <span style={{ 
            fontSize: '12px', 
            color: '#666',
            transform: isExpanded ? 'rotate(90deg)' : 'rotate(0deg)',
            transition: 'transform 0.2s',
            width: '12px',
            textAlign: 'center'
          }}>
            ▶
          </span>
          <span style={{ fontWeight: '500', color: '#333' }}>
            学习总结
          </span>
          {summaryContent && (
            <span style={{ 
              fontSize: '11px', 
              color: '#999',
              backgroundColor: '#e8f4fd',
              padding: '2px 6px',
              borderRadius: '10px'
            }}>
              已生成
            </span>
          )}
        </div>
        
        <div style={{ display: 'flex', gap: '4px' }}>
          {!isGenerating && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                generateSummary();
              }}
              style={{
                padding: '4px 8px',
                fontSize: '11px',
                backgroundColor: '#4CAF50',
                color: 'white',
                border: 'none',
                borderRadius: '4px',
                cursor: 'pointer'
              }}
              title="生成总结"
            >
              {summaryRequested ? '重新生成总结' : '生成学习总结'}
            </button>
          )}
          
          {onClose && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onClose();
              }}
              style={{
                padding: '4px 6px',
                fontSize: '11px',
                backgroundColor: '#f44336',
                color: 'white',
                border: 'none',
                borderRadius: '4px',
                cursor: 'pointer'
              }}
              title="关闭总结框"
            >
              ✕
            </button>
          )}
        </div>
      </div>

      {/* 总结内容显示区域 */}
      {isExpanded && (
        <div 
          className="summary-content"
          style={{
            padding: '12px',
            minHeight: '40px',
            maxHeight: '300px',
            overflowY: 'auto',
            backgroundColor: '#f8f9fa'
          }}
        >
          {isGenerating && (
            <div style={{ 
              color: '#666', 
              fontStyle: 'italic',
              display: 'flex',
              alignItems: 'center',
              gap: '8px',
              marginBottom: summaryContent ? '8px' : '0'
            }}>
              <div 
                style={{
                  width: '16px',
                  height: '16px',
                  border: '2px solid #e0e0e0',
                  borderTop: '2px solid #4CAF50',
                  borderRadius: '50%',
                  animation: 'spin 1s linear infinite'
                }}
              />
              正在生成学习总结...
            </div>
          )}
          
          {summaryContent ? (
            <div style={{ 
              lineHeight: '1.5',
              color: '#333',
              whiteSpace: 'pre-wrap',
              fontSize: '14px'
            }}>
              {summaryContent}
              {isStreaming && (
                <span 
                  style={{
                    opacity: 0.7,
                    animation: 'blink 1s infinite'
                  }}
                >
                  |
                </span>
              )}
            </div>
          ) : !isGenerating && (
            <div style={{ 
              color: '#999', 
              fontStyle: 'italic',
              textAlign: 'center',
              padding: '20px',
              fontSize: '12px'
            }}>
              💡 点击"生成学习总结"按钮，AI将基于当前对话生成学习要点总结
            </div>
          )}
        </div>
      )}
      
      {/* CSS动画 */}
      <style>{`
        @keyframes spin {
          0% { transform: rotate(0deg); }
          100% { transform: rotate(360deg); }
        }
        
        @keyframes blink {
          0%, 50% { opacity: 1; }
          51%, 100% { opacity: 0; }
        }
        
        .summary-box:hover {
          box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        }
        
        .summary-header:hover {
          background-color: #e8e8e8;
        }
      `}</style>
    </div>
  );
};