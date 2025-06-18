/**
 * DEPRECATED COMPONENT - 已废弃的组件
 * 
 * 废弃日期: 2024年6月5日
 * 废弃原因: 单次回顾功能已被统一回顾模块替代，为简化用户体验而停用
 * 替代方案: 请使用 ReviewAnalysisDashboard 和相关的统一回顾功能
 * 
 * 注意: 此组件已从主界面隐藏，但保留代码以供将来可能的恢复需要
 *       如需重新启用，请修改 App.tsx 中的相关注释
 */

import { useState, useEffect } from 'react';
import { TauriAPI, MistakeItem } from '../utils/tauriApi';
import { StreamingMarkdownRenderer } from '../chat-core';

interface ReviewSession {
  id: string;
  subject: string;
  selectedMistakes: MistakeItem[];
  analysis: string;
  created_at: string;
  chatHistory: ChatMessage[];
}

interface ChatMessage {
  role: string;
  content: string;
  timestamp: string;
  thinking_content?: string;
}

interface ReviewAnalysisProps {
  onBack: () => void;
}

/**
 * @deprecated 此组件已废弃 - 请使用统一回顾功能替代
 */
export const ReviewAnalysis: React.FC<ReviewAnalysisProps> = ({ onBack }) => {
  const [selectedSubject, setSelectedSubject] = useState('');
  const [selectedMistakes, setSelectedMistakes] = useState<MistakeItem[]>([]);
  const [currentReview, setCurrentReview] = useState<ReviewSession | null>(null);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [chatHistory, setChatHistory] = useState<ChatMessage[]>([]);
  const [currentMessage, setCurrentMessage] = useState('');
  const [isChatting, setIsChatting] = useState(false);
  const [subjectMistakes, setSubjectMistakes] = useState<MistakeItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [availableSubjects, setAvailableSubjects] = useState<string[]>([]);
  const [isLoadingSubjects, setIsLoadingSubjects] = useState(true);
  
  // 流式处理相关状态
  const [streamingMessageIndex, setStreamingMessageIndex] = useState<number | null>(null);
  const [thinkingContent, setThinkingContent] = useState<Map<number, string>>(new Map());

  // 获取当前科目的错题
  const getSubjectMistakes = () => {
    return subjectMistakes;
  };

  // 加载科目错题
  const loadSubjectMistakes = async (subject: string) => {
    setLoading(true);
    try {
      const mistakes = await TauriAPI.getMistakes({ subject });
      setSubjectMistakes(mistakes);
    } catch (error) {
      console.error('加载错题失败:', error);
      alert('加载错题失败: ' + error);
    } finally {
      setLoading(false);
    }
  };

  // 加载支持的科目
  useEffect(() => {
    const loadSubjects = async () => {
      try {
        const subjects = await TauriAPI.getSupportedSubjects();
        setAvailableSubjects(subjects);
        // 设置默认科目为第一个
        if (subjects.length > 0) {
          setSelectedSubject(subjects[0]);
        }
      } catch (error) {
        console.error('加载科目失败:', error);
        // 如果API失败，使用备用科目列表
        const fallbackSubjects = ['数学', '物理', '化学', '英语'];
        setAvailableSubjects(fallbackSubjects);
        setSelectedSubject(fallbackSubjects[0]);
      } finally {
        setIsLoadingSubjects(false);
      }
    };
    loadSubjects();
  }, []);

  // 组件加载时和科目变化时加载错题
  useEffect(() => {
    if (selectedSubject) {
      loadSubjectMistakes(selectedSubject);
    }
  }, [selectedSubject]);

  // 切换错题选择
  const toggleMistakeSelection = (mistake: MistakeItem) => {
    setSelectedMistakes(prev => {
      const isSelected = prev.some(m => m.id === mistake.id);
      if (isSelected) {
        return prev.filter(m => m.id !== mistake.id);
      } else {
        return [...prev, mistake];
      }
    });
  };

  // 全选/取消全选
  const toggleSelectAll = () => {
    const subjectMistakes = getSubjectMistakes();
    if (selectedMistakes.length === subjectMistakes.length) {
      setSelectedMistakes([]);
    } else {
      setSelectedMistakes(subjectMistakes);
    }
  };

  // 开始回顾分析 - 流式版本
  const startReviewAnalysis = async () => {
    if (selectedMistakes.length === 0) {
      alert('请至少选择一道错题进行分析');
      return;
    }

    setIsAnalyzing(true);
    setStreamingMessageIndex(null);
    setChatHistory([]);
    setThinkingContent(new Map());
    
    try {
      
      const mistakeIds = selectedMistakes.map(m => m.id);
      console.log('🚀 开始流式回顾分析:', mistakeIds);
      
      // 创建空的助手消息等待流式填充
      const initialMessage: ChatMessage = {
        role: 'assistant',
        content: '',
        timestamp: new Date().toISOString(),
      };
      setChatHistory([initialMessage]);
      setStreamingMessageIndex(0);
      
      // 设置流式事件监听
      const streamEvent = 'review_analysis_stream';
      let fullContent = '';
      let fullThinkingContent = '';
      let contentListenerActive = true;
      let thinkingListenerActive = true;
      
      // 使用Tauri的listen API
      const { listen } = await import('@tauri-apps/api/event');
      
      // 监听主内容流
      const unlistenContent = await listen(streamEvent, (event: any) => {
        if (!contentListenerActive) return;
        
        console.log(`💬 收到回顾分析流式内容:`, event.payload);
        
        if (event.payload) {
          // 先检查是否完成
          if (event.payload.is_complete) {
            console.log('🎉 流式回顾分析完成，总长度:', fullContent.length);
            contentListenerActive = false;
            
            // 如果思维链监听器也不活跃了，则设置整体完成状态
            if (!thinkingListenerActive) {
              console.log('🎉 所有流式内容完成');
              setStreamingMessageIndex(null);
            }
            return;
          }
          
          // 如果有内容，则累积
          if (event.payload.content) {
            fullContent += event.payload.content;
            console.log(`📝 回顾分析累积内容长度: ${fullContent.length} 字符`);
            
            // 更新聊天历史
            setChatHistory(prev => {
              const newHistory = [...prev];
              if (newHistory[0]) {
                newHistory[0] = {
                  ...newHistory[0],
                  content: fullContent
                };
              }
              return newHistory;
            });
          }
        }
      });
      
            
      
      // 监听思维链事件（回顾分析通常需要深度思考）
      const reasoningEvent = `${streamEvent}_reasoning`;
      console.log(`🧠 监听回顾分析思维链事件: ${reasoningEvent}`);
      
      const unlistenThinking = await listen(reasoningEvent, (event: any) => {
        if (!thinkingListenerActive) return;
        
        console.log(`🧠 回顾分析思维链内容:`, event.payload);
        
        if (event.payload) {
          // 先检查是否完成
          if (event.payload.is_complete) {
            console.log('🎉 回顾分析思维链完成，总长度:', fullThinkingContent.length);
            thinkingListenerActive = false;
            
            // 如果主内容监听器也不活跃了，则设置整体完成状态
            if (!contentListenerActive) {
              console.log('🎉 所有流式内容完成');
              setStreamingMessageIndex(null);
            }
            return;
          }
          
          // 如果有内容，则累积
          if (event.payload.content) {
            fullThinkingContent += event.payload.content;
            
            // 更新思维链内容
            setThinkingContent(prev => {
              const newMap = new Map(prev);
              newMap.set(0, fullThinkingContent);
              return newMap;
            });
            
            console.log(`🧠 回顾分析思维链累积长度: ${fullThinkingContent.length} 字符`);
          }
        }
      });
      
            
      
      // 调用后端流式回顾分析API
      const response = await TauriAPI.analyzeReviewSessionStream(selectedSubject, mistakeIds);
      
      // 创建回顾会话对象
      const reviewSession: ReviewSession = {
        id: response.review_id,
        subject: selectedSubject,
        selectedMistakes: [...selectedMistakes],
        analysis: '', // 将通过流式更新
        created_at: new Date().toISOString(),
        chatHistory: []
      };

      setCurrentReview(reviewSession);
      
    } catch (error) {
      console.error('❌ 回顾分析失败:', error);
      alert('回顾分析失败: ' + error);
      setStreamingMessageIndex(null);
      setChatHistory([]);
    } finally {
      setIsAnalyzing(false);
    }
  };

  // 发送聊天消息 - 流式版本
  const handleSendMessage = async () => {
    if (!currentMessage.trim() || !currentReview) return;

    const newUserMessage: ChatMessage = {
      role: 'user',
      content: currentMessage,
      timestamp: new Date().toISOString(),
    };

    const updatedHistory = [...chatHistory, newUserMessage];
    setChatHistory(updatedHistory);
    setCurrentMessage('');
    setIsChatting(true);

    try {
      console.log('💬 开始流式回顾分析追问...');
      
      // 创建空的助手消息等待流式填充
      const assistantMessage: ChatMessage = {
        role: 'assistant',
        content: '',
        timestamp: new Date().toISOString(),
      };
      
      const streamingHistory = [...updatedHistory, assistantMessage];
      setChatHistory(streamingHistory);
      setStreamingMessageIndex(streamingHistory.length - 1);
      
      // 设置流式事件监听
      const streamEvent = `review_chat_stream_${currentReview.id}`;
      let fullContent = '';
      let fullThinkingContent = '';
      let contentListenerActive = true;
      let thinkingListenerActive = true;
      
      // 使用Tauri的listen API
      const { listen } = await import('@tauri-apps/api/event');
      
      // 监听主内容流
      const unlistenContent = await listen(streamEvent, (event: any) => {
        if (!contentListenerActive) return;
        
        console.log(`💬 收到回顾追问流式内容:`, event.payload);
        
        if (event.payload) {
          // 先检查是否完成
          if (event.payload.is_complete) {
            console.log('🎉 流式回顾追问完成，总长度:', fullContent.length);
            contentListenerActive = false;
            
            // 如果思维链监听器也不活跃了，则设置整体完成状态
            if (!thinkingListenerActive) {
              console.log('🎉 所有追问流式内容完成');
              setStreamingMessageIndex(null);
            }
            return;
          }
          
          // 如果有内容，则累积
          if (event.payload.content) {
            fullContent += event.payload.content;
            console.log(`📝 回顾追问累积内容长度: ${fullContent.length} 字符`);
            
            // 更新聊天历史中的最后一条助手消息
            setChatHistory(prev => {
              const newHistory = [...prev];
              const lastIndex = newHistory.length - 1;
              if (newHistory[lastIndex] && newHistory[lastIndex].role === 'assistant') {
                newHistory[lastIndex] = {
                  ...newHistory[lastIndex],
                  content: fullContent
                };
              }
              return newHistory;
            });
          }
        }
      });
      
            
      
      // 监听思维链事件
      const reasoningEvent = `${streamEvent}_reasoning`;
      console.log(`🧠 监听回顾追问思维链事件: ${reasoningEvent}`);
      
      const unlistenThinking = await listen(reasoningEvent, (event: any) => {
        if (!thinkingListenerActive) return;
        
        console.log(`🧠 回顾追问思维链内容:`, event.payload);
        
        if (event.payload) {
          // 先检查是否完成
          if (event.payload.is_complete) {
            console.log('🎉 回顾追问思维链完成，总长度:', fullThinkingContent.length);
            thinkingListenerActive = false;
            
            // 如果主内容监听器也不活跃了，则设置整体完成状态
            if (!contentListenerActive) {
              console.log('🎉 所有追问流式内容完成');
              setStreamingMessageIndex(null);
            }
            return;
          }
          
          // 如果有内容，则累积
          if (event.payload.content) {
            fullThinkingContent += event.payload.content;
            
            // 更新思维链内容（使用最后一条消息的索引）
            const lastMessageIndex = streamingHistory.length - 1;
            setThinkingContent(prev => {
              const newMap = new Map(prev);
              newMap.set(lastMessageIndex, fullThinkingContent);
              return newMap;
            });
            
            console.log(`🧠 回顾追问思维链累积长度: ${fullThinkingContent.length} 字符`);
          }
        }
      });
      
            
      
      // 调用后端流式追问API
      await TauriAPI.continueReviewChatStream(currentReview.id, updatedHistory);
      
    } catch (error) {
      console.error('❌ 回顾追问失败:', error);
      alert('聊天失败: ' + error);
      setStreamingMessageIndex(null);
    } finally {
      setIsChatting(false);
    }
  };

  // 重新开始分析
  const resetAnalysis = () => {
    
    
    setCurrentReview(null);
    setChatHistory([]);
    setSelectedMistakes([]);
    setStreamingMessageIndex(null);
    setThinkingContent(new Map());
  };

  return (
    <div className="review-analysis">
      <div className="review-header">
        <button onClick={onBack} className="back-button">
          ← 返回
        </button>
        <h2>回顾分析</h2>
        {currentReview && (
          <button onClick={resetAnalysis} className="reset-button">
            🔄 重新分析
          </button>
        )}
      </div>

      {!currentReview ? (
        // 选择错题界面
        <div className="mistake-selection">
          <div className="selection-controls">
            <div className="subject-selector">
              <label>选择科目:</label>
              <select 
                value={selectedSubject} 
                onChange={(e) => {
                  setSelectedSubject(e.target.value);
                  setSelectedMistakes([]);
                }}
                disabled={isLoadingSubjects}
              >
                {isLoadingSubjects ? (
                  <option value="">加载中...</option>
                ) : (
                  availableSubjects.map(subject => (
                    <option key={subject} value={subject}>{subject}</option>
                  ))
                )}
              </select>
            </div>

            <div className="selection-info">
              <span>已选择 {selectedMistakes.length} / {getSubjectMistakes().length} 道题目</span>
              <button onClick={toggleSelectAll} className="select-all-button">
                {selectedMistakes.length === getSubjectMistakes().length ? '取消全选' : '全选'}
              </button>
            </div>
          </div>

          {loading ? (
            <div className="loading">加载错题中...</div>
          ) : (
            <div className="mistakes-grid">
              {getSubjectMistakes().map((mistake) => (
              <div 
                key={mistake.id}
                className={`mistake-card ${selectedMistakes.some(m => m.id === mistake.id) ? 'selected' : ''}`}
                onClick={() => toggleMistakeSelection(mistake)}
              >
                <div className="mistake-header">
                  <input 
                    type="checkbox" 
                    checked={selectedMistakes.some(m => m.id === mistake.id)}
                    onChange={() => toggleMistakeSelection(mistake)}
                    onClick={(e) => e.stopPropagation()}
                  />
                  <span className="mistake-type">{mistake.mistake_type}</span>
                  <span className="mistake-date">
                    {new Date(mistake.created_at).toLocaleDateString('zh-CN')}
                  </span>
                </div>
                
                <div className="mistake-content">
                  <h4>{mistake.user_question}</h4>
                  <p className="ocr-preview">{mistake.ocr_text}</p>
                  <div className="tags">
                    {mistake.tags.map((tag, index) => (
                      <span key={index} className="tag">{tag}</span>
                    ))}
                  </div>
                </div>
              </div>
            ))}
            </div>
          )}

          {!loading && getSubjectMistakes().length === 0 && (
            <div className="empty-mistakes">
              <p>当前科目还没有错题记录</p>
              <p>请先在分析页面添加一些错题</p>
            </div>
          )}

          <div className="analysis-controls">
            <div style={{ padding: '0.5rem', backgroundColor: '#f0f8ff', borderRadius: '4px', fontSize: '0.9rem', color: '#666', marginBottom: '1rem' }}>
              ℹ️ 回顾分析将使用流式输出和思维链，为您提供深度分析
            </div>
            <button 
              onClick={startReviewAnalysis}
              disabled={selectedMistakes.length === 0 || isAnalyzing}
              className="start-analysis-button"
            >
              {isAnalyzing ? '分析中...' : `开始分析 (${selectedMistakes.length}道题目)`}
            </button>
          </div>
        </div>
      ) : (
        // 分析结果界面
        <div className="analysis-result">
          <div className="result-header">
            <h3>分析结果 - {currentReview.subject}</h3>
            <div className="result-info">
              <span>分析题目: {currentReview.selectedMistakes.length} 道</span>
              <span>分析时间: {new Date(currentReview.created_at).toLocaleString('zh-CN')}</span>
              <span className="stream-indicator">🌊 流式输出 🧠 思维链</span>
            </div>
          </div>

          <div className="chat-container">
            <div className="chat-history">
              {chatHistory.map((message, index) => {
                const isStreaming = streamingMessageIndex === index;
                const thinking = thinkingContent.get(index);
                return (
                  <div key={index} className={`message ${message.role}`}>
                    <div className="message-content">
                      <StreamingMarkdownRenderer
                        content={message.content}
                        isStreaming={isStreaming}
                        chainOfThought={{
                          enabled: !!thinking,
                          details: thinking || ''
                        }}
                      />
                    </div>
                    <div className="message-time">
                      {new Date(message.timestamp).toLocaleTimeString()}
                    </div>
                  </div>
                );
              })}
              {(isAnalyzing || isChatting) && streamingMessageIndex === null && (
                <div className="message assistant">
                  <div className="message-content typing">
                    <span className="typing-indicator">
                      <span></span>
                      <span></span>
                      <span></span>
                    </span>
                    {isAnalyzing ? 'AI正在深度分析多道错题...' : '正在思考中...'}
                  </div>
                </div>
              )}
            </div>

            {/* 只有在分析完成后才显示输入框 */}
            {!isAnalyzing && currentReview && (
              <div className="chat-input">
                <input
                  type="text"
                  value={currentMessage}
                  onChange={(e) => setCurrentMessage(e.target.value)}
                  placeholder="对分析结果有疑问？继续提问..."
                  onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
                  disabled={isChatting}
                />
                <button
                  onClick={handleSendMessage}
                  disabled={isChatting || !currentMessage.trim()}
                  className="send-button"
                >
                  {isChatting ? '⏳' : '📤'}
                </button>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
};