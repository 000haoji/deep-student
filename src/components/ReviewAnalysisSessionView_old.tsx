import React, { useState, useEffect, useRef, useCallback } from 'react';
import { ReviewSessionTask, ChatMessage } from '../types/index';
import { useNotification } from '../hooks/useNotification';
import { TauriAPI } from '../utils/tauriApi';
import { listen } from '@tauri-apps/api/event';
import { MessageWithThinking } from '../chat-core';

interface ReviewAnalysisSessionViewProps {
  sessionId: string;
  onBack: () => void;
}

const ReviewAnalysisSessionView: React.FC<ReviewAnalysisSessionViewProps> = ({
  sessionId,
  onBack,
}) => {
  const [session, setSession] = useState<ReviewSessionTask | null>(null);
  const [loading, setLoading] = useState(true);
  const [userInput, setUserInput] = useState('');
  const [isStreaming, setIsStreaming] = useState(false);
  const [showThinking, setShowThinking] = useState(true);
  const [sessionError, setSessionError] = useState<string | null>(null);
  
  const chatContainerRef = useRef<HTMLDivElement>(null);
  const { showNotification } = useNotification();


  useEffect(() => {
    loadSession();
  }, [sessionId]);

  useEffect(() => {
    scrollToBottom();
  }, [session?.chatHistory]);

  const loadSession = async () => {
    try {
      setLoading(true);
      
      // 优先尝试从后端API加载会话数据
      try {
        const backendSession = await TauriAPI.getConsolidatedReviewSession(sessionId);
        if (backendSession) {
          // 如果后端有数据，将其转换为前端格式
          const sessionData: ReviewSessionTask = {
            id: sessionId,
            name: backendSession.name || '统一回顾分析',
            creationDate: backendSession.created_at || new Date().toISOString(),
            subject: backendSession.subject,
            mistakeIds: backendSession.mistake_ids || [],
            userConsolidatedInput: backendSession.consolidated_input || '',
            userOverallPrompt: backendSession.overall_prompt || '',
            status: 'pending', // 从后端加载的会话默认为pending状态
            review_session_id: backendSession.review_session_id,
            chatHistory: [],
            thinkingContent: new Map(),
            currentFullContentForStream: '',
            currentThinkingContentForStream: '',
          };
          
          setSession(sessionData);
          
          // 如果会话状态为pending，自动开始分析
          if (sessionData.status === 'pending') {
            startAnalysis(sessionData);
          }
          
          console.log('✅ 从后端API成功加载会话数据');
          return;
        }
      } catch (apiError) {
        console.warn('从后端API加载会话失败，尝试从localStorage加载:', apiError);
      }
      
      // 如果API调用失败或数据不存在，回退到localStorage
      const existingSessions = JSON.parse(localStorage.getItem('reviewSessions') || '[]');
      const foundSession = existingSessions.find((s: ReviewSessionTask) => s.id === sessionId);
      
      if (foundSession) {
        // 转换Map对象
        foundSession.thinkingContent = new Map(Object.entries(foundSession.thinkingContent || {}));
        setSession(foundSession);
        
        // 如果会话状态为pending，自动开始分析
        if (foundSession.status === 'pending') {
          startAnalysis(foundSession);
        }
        
        console.log('✅ 从localStorage成功加载会话数据（回退）');
      } else {
        showNotification('error', '找不到指定的回顾分析会话');
      }
    } catch (error) {
      console.error('加载回顾分析会话失败:', error);
      showNotification('error', '加载回顾分析会话失败');
    } finally {
      setLoading(false);
    }
  };

  const startAnalysis = async (sessionData: ReviewSessionTask) => {
    try {
      setSession(prev => prev ? { ...prev, status: 'processing_setup' } : null);
      
      // 如果会话已经有review_session_id，直接开始流式分析
      if (sessionData.review_session_id) {
        setSession(prev => prev ? {
          ...prev,
          status: 'awaiting_stream_start'
        } : null);
        
        // 监听流式事件
        await setupStreamListeners(sessionData.review_session_id);
        
        // 开始流式分析
        await TauriAPI.triggerConsolidatedReviewStream({
          reviewSessionId: sessionData.review_session_id,
          enableChainOfThought: true
        });
      } else {
        // 应该不会发生，因为创建时就有了review_session_id
        throw new Error('缺少review_session_id');
      }
      
    } catch (error) {
      console.error('开始分析失败:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      setSession(prev => prev ? { ...prev, status: 'error_setup', errorDetails: errorMessage } : null);
      setSessionError('分析启动失败: ' + errorMessage);
      showNotification('error', '分析启动失败: ' + errorMessage);
    }
  };

  const setupStreamListeners = async (reviewSessionId: string) => {
    const streamEvent = `review_analysis_stream_${reviewSessionId}`;
    const thinkingEvent = `review_analysis_stream_${reviewSessionId}_reasoning`;
    
    let fullContent = '';
    let fullThinkingContent = '';
    
    // 监听主内容流
    await listen(streamEvent, (event: any) => {
      console.log('回顾分析流内容:', event.payload);
      
      if (event.payload) {
        if (event.payload.is_complete) {
          console.log('回顾分析流完成');
          setSession(prev => {
            if (!prev) return null;
            
            const assistantMessage: ChatMessage = {
              role: 'assistant',
              content: fullContent,
              timestamp: new Date().toISOString(),
              thinking_content: fullThinkingContent || undefined,
            };
            
            return {
              ...prev,
              chatHistory: [...prev.chatHistory, assistantMessage],
              status: 'completed',
            };
          });
          setIsStreaming(false);
        } else if (event.payload.content) {
          fullContent += event.payload.content;
          // 实时更新显示
          setSession(prev => {
            if (!prev) return null;
            return {
              ...prev,
              currentFullContentForStream: fullContent,
              status: 'streaming_answer',
            };
          });
        }
      }
    });
    
    // 监听思维链流
    if (showThinking) {
      await listen(thinkingEvent, (event: any) => {
        console.log('回顾分析思维链:', event.payload);
        
        if (event.payload) {
          if (event.payload.is_complete) {
            console.log('回顾分析思维链完成');
          } else if (event.payload.content) {
            fullThinkingContent += event.payload.content;
            setSession(prev => {
              if (!prev) return null;
              return {
                ...prev,
                currentThinkingContentForStream: fullThinkingContent,
              };
            });
          }
        }
      });
    }
  };

  const handleSendMessage = async () => {
    if (!userInput.trim() || !session || isStreaming) return;

    try {
      setIsStreaming(true);

      const newUserMessage: ChatMessage = {
        role: 'user',
        content: userInput.trim(),
        timestamp: new Date().toISOString(),
      };

      // 更新聊天历史
      setSession(prev => prev ? {
        ...prev,
        chatHistory: [...prev.chatHistory, newUserMessage]
      } : null);

      setUserInput('');

      // 调用后端API继续对话
      if (session.review_session_id) {
        try {
          // 设置流式监听器
          await setupStreamListeners(session.review_session_id);
          
          // 发起对话请求
          await TauriAPI.continueConsolidatedReviewStream({
            reviewSessionId: session.review_session_id,
            chatHistory: [...session.chatHistory, newUserMessage],
            enableChainOfThought: true,
          });
        } catch (apiError) {
          console.error('继续对话API调用失败:', apiError);
          const errorMessage = apiError instanceof Error ? apiError.message : String(apiError);
          setSessionError('对话失败: ' + errorMessage);
          setIsStreaming(false);
          showNotification('error', '对话失败: ' + errorMessage);
        }
      } else {
        showNotification('error', '缺少回顾分析会话ID');
      }

    } catch (error) {
      console.error('发送消息失败:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      setSessionError('发送消息失败: ' + errorMessage);
      showNotification('error', '发送消息失败: ' + errorMessage);
      setIsStreaming(false);
    }
  };

  const scrollToBottom = () => {
    if (chatContainerRef.current) {
      chatContainerRef.current.scrollTop = chatContainerRef.current.scrollHeight;
    }
  };

  if (loading) {
    return (
      <div className="flex justify-center items-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
        <span className="ml-2 text-gray-600">加载中...</span>
      </div>
    );
  }

  if (!session) {
    return (
      <div className="text-center py-12">
        <h3 className="text-lg font-medium text-gray-900 mb-2">未找到回顾分析会话</h3>
        <button
          onClick={onBack}
          className="bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-lg transition-colors"
        >
          返回列表
        </button>
      </div>
    );
  }

  return (
    <div className="review-analysis-session-view">
      {/* 头部信息 - 简化并移除思维链控制（移动到聊天区域） */}
      <div style={{
        backgroundColor: 'white',
        borderBottom: '1px solid #e2e8f0',
        padding: '16px 24px',
        flexShrink: 0
      }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', marginBottom: '8px' }}>
              <button
                onClick={onBack}
                style={{
                  marginRight: '12px',
                  padding: '4px',
                  border: 'none',
                  background: 'transparent',
                  cursor: 'pointer',
                  borderRadius: '4px',
                  display: 'flex',
                  alignItems: 'center'
                }}
                onMouseOver={(e) => e.currentTarget.style.backgroundColor = '#f3f4f6'}
                onMouseOut={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
              >
                <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                </svg>
              </button>
              <h2 style={{ fontSize: '20px', fontWeight: '600', color: '#1f2937', margin: 0 }}>
                {session.name}
              </h2>
              <span style={{
                marginLeft: '12px',
                padding: '4px 8px',
                fontSize: '12px',
                fontWeight: '500',
                borderRadius: '12px',
                backgroundColor: session.status === 'completed' ? '#dcfce7' : 
                               session.status === 'streaming_answer' ? '#dbeafe' :
                               session.status.includes('error') ? '#fecaca' : '#fef3c7',
                color: session.status === 'completed' ? '#166534' :
                       session.status === 'streaming_answer' ? '#1d4ed8' :
                       session.status.includes('error') ? '#dc2626' : '#d97706'
              }}>
                {session.status === 'completed' ? '已完成' :
                 session.status === 'streaming_answer' ? '分析中' :
                 session.status.includes('error') ? '错误' : '处理中'}
              </span>
            </div>
            <div style={{ fontSize: '14px', color: '#6b7280' }}>
              <span>科目: {session.subject}</span>
              <span style={{ margin: '0 8px' }}>•</span>
              <span>错题数量: {session.mistakeIds.length}</span>
              <span style={{ margin: '0 8px' }}>•</span>
              <span>创建时间: {new Date(session.creationDate).toLocaleString()}</span>
            </div>
          </div>
        </div>
      </div>

      {/* 统一回顾分析上下文信息区域 - 参考BatchTaskDetailView结构 */}
      <div style={{
        backgroundColor: 'white',
        borderBottom: '1px solid #e2e8f0',
        padding: '16px 24px',
        flexShrink: 0
      }}>
        <div className="task-info-section">
          <h4 style={{ fontSize: '16px', fontWeight: '600', color: '#374151', marginBottom: '12px' }}>
            📋 回顾分析信息
          </h4>
          
          <div className="info-row" style={{ display: 'flex', marginBottom: '8px' }}>
            <span className="info-label" style={{ minWidth: '80px', color: '#6b7280', fontWeight: '500' }}>
              分析目标:
            </span>
            <span className="info-value" style={{ color: '#1f2937', flex: 1 }}>
              {session.userOverallPrompt || '统一分析多个错题的共同问题和改进建议'}
            </span>
          </div>
          
          <div className="info-row" style={{ display: 'flex', marginBottom: '8px' }}>
            <span className="info-label" style={{ minWidth: '80px', color: '#6b7280', fontWeight: '500' }}>
              错题数量:
            </span>
            <span className="info-value" style={{ color: '#1f2937' }}>
              {session.mistakeIds.length} 个错题
            </span>
          </div>
          
          {session.mistakeIds.length > 0 && (
            <div className="info-row" style={{ display: 'flex', marginBottom: '8px' }}>
              <span className="info-label" style={{ minWidth: '80px', color: '#6b7280', fontWeight: '500' }}>
                错题ID:
              </span>
              <span className="info-value" style={{ color: '#1f2937' }}>
                {session.mistakeIds.slice(0, 3).join(', ')}
                {session.mistakeIds.length > 3 && ` 等${session.mistakeIds.length}个`}
              </span>
            </div>
          )}
          
          {session.userConsolidatedInput && (
            <div className="consolidated-input-section" style={{ marginTop: '16px' }}>
              <h5 style={{ fontSize: '14px', fontWeight: '600', color: '#374151', marginBottom: '8px' }}>
                📝 整合内容预览
              </h5>
              <div className="consolidated-preview" style={{
                backgroundColor: '#f8fafc',
                border: '1px solid #e2e8f0',
                borderRadius: '8px',
                padding: '12px',
                fontSize: '13px',
                color: '#4b5563',
                maxHeight: '120px',
                overflow: 'auto',
                lineHeight: '1.4',
                whiteSpace: 'pre-wrap'
              }}>
                {session.userConsolidatedInput.length > 200 
                  ? session.userConsolidatedInput.substring(0, 200) + '...' 
                  : session.userConsolidatedInput}
              </div>
            </div>
          )}
          
          {/* 错误信息 - 借鉴BatchTaskDetailView */}
          {(session.errorDetails || sessionError) && (
            <div className="error-section" style={{ marginTop: '16px' }}>
              <h4 style={{ fontSize: '14px', fontWeight: '600', color: '#dc2626', marginBottom: '8px' }}>
                ❌ 错误信息
              </h4>
              <div className="error-text" style={{
                backgroundColor: '#fef2f2',
                border: '1px solid #fecaca',
                borderRadius: '8px',
                padding: '12px',
                fontSize: '13px',
                color: '#991b1b',
                lineHeight: '1.4'
              }}>
                {sessionError || session.errorDetails}
              </div>
            </div>
          )}
          
          {/* 处理状态显示 - 借鉴BatchTaskDetailView的状态指示 */}
          {session.status !== 'completed' && session.status !== 'pending' && !session.status.includes('error') && (
            <div className="processing-status" style={{ marginTop: '16px' }}>
              <div style={{
                backgroundColor: '#eff6ff',
                border: '1px solid #bfdbfe',
                borderRadius: '8px',
                padding: '12px',
                display: 'flex',
                alignItems: 'center',
                gap: '8px'
              }}>
                <div className="spinner" style={{
                  width: '16px',
                  height: '16px',
                  border: '2px solid #dbeafe',
                  borderTop: '2px solid #3b82f6',
                  borderRadius: '50%',
                  animation: 'spin 1s linear infinite'
                }}></div>
                <span style={{ fontSize: '14px', color: '#1d4ed8', fontWeight: '500' }}>
                  {session.status === 'processing_setup' ? '正在设置分析环境...' :
                   session.status === 'awaiting_stream_start' ? '等待AI开始分析...' :
                   session.status === 'streaming_answer' ? 'AI正在分析您的错题...' :
                   '处理中...'}
                </span>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* AI解答区域 - 复用BatchTaskDetailView的聊天界面结构 */}
      <div className="chat-container">
        <div className="chat-header">
          <h4>💬 AI回顾分析</h4>
          <div className="chat-header-actions">
            <span className="chain-indicator">🧠 思维链模式</span>
            <button
              onClick={() => setShowThinking(!showThinking)}
              className="chat-fullscreen-toggle"
              style={{ marginLeft: '8px' }}
            >
              {showThinking ? '隐藏思维链' : '显示思维链'}
            </button>
          </div>
        </div>
        
        <div className="chat-history" ref={chatContainerRef} style={{
          flex: 1,
          overflowY: 'auto',
          padding: '1rem',
          minHeight: 0
        }}>
          {session.chatHistory.map((message, index) => {
            const thinking = session.thinkingContent.get(index);
            return (
              <MessageWithThinking
                key={index}
                content={message.content}
                thinkingContent={thinking}
                isStreaming={isStreaming && index === session.chatHistory.length - 1}
                role={message.role as 'user' | 'assistant'}
                timestamp={message.timestamp}
              />
            );
          })}
          
          {isStreaming && session.chatHistory.length === 0 && (
            <div className="message assistant">
              <div className="message-content typing">
                <span className="typing-indicator">
                  <span></span>
                  <span></span>
                  <span></span>
                </span>
                AI正在分析您的错题...
              </div>
            </div>
          )}
          
          {isStreaming && (
            <div className="message assistant">
              <div className="message-content typing">
                <span className="typing-indicator">
                  <span></span>
                  <span></span>
                  <span></span>
                </span>
                正在思考中...
              </div>
            </div>
          )}
        </div>

        {/* 输入框 - 复用BatchTaskDetailView的输入区域结构 */}
        {session.status === 'completed' && (
          <div className="chat-input" style={{
            flexShrink: 0,
            padding: '1rem',
            borderTop: '1px solid #e1e5e9',
            backgroundColor: 'white',
            display: 'flex',
            gap: '0.5rem',
            alignItems: 'center'
          }}>
            <input
              type="text"
              value={userInput}
              onChange={(e) => setUserInput(e.target.value)}
              placeholder="继续提问..."
              onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
              disabled={isStreaming}
              style={{
                flex: 1,
                padding: '0.75rem',
                border: '1px solid #d1d5db',
                borderRadius: '0.375rem',
                fontSize: '0.875rem',
                outline: 'none'
              }}
            />
            <button
              onClick={handleSendMessage}
              disabled={isStreaming || !userInput.trim()}
              className="send-button"
              style={{
                padding: '0.75rem 1rem',
                backgroundColor: '#3b82f6',
                color: 'white',
                border: 'none',
                borderRadius: '0.375rem',
                cursor: 'pointer',
                fontSize: '0.875rem',
                opacity: (isStreaming || !userInput.trim()) ? 0.5 : 1
              }}
            >
              {isStreaming ? '⏳' : '📤'}
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export default ReviewAnalysisSessionView;