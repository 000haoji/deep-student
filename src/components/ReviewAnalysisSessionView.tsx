import React, { useState, useEffect, useCallback, useRef } from 'react';
import { useNotification } from '../hooks/useNotification';
import { TauriAPI, ReviewAnalysisItem } from '../utils/tauriApi';
import UniversalAppChatHost from './UniversalAppChatHost';

interface ReviewAnalysisSessionViewProps {
  sessionId: string;
  onBack: () => void;
}

/**
 * 新的回顾分析详情页 - 完全复用错题分析的逻辑和模式
 */
const ReviewAnalysisSessionView: React.FC<ReviewAnalysisSessionViewProps> = ({
  sessionId,
  onBack,
}) => {
  console.log('🔍 ReviewAnalysisSessionView 渲染, sessionId:', sessionId);
  
  // 使用数据库模式，复用错题分析的状态管理
  const [reviewAnalysis, setReviewAnalysis] = useState<ReviewAnalysisItem | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const { showNotification } = useNotification();
  
  // 用于存储最新的聊天历史状态
  const [latestChatHistory, setLatestChatHistory] = useState<any[]>([]);
  
  // 定时保存的引用
  const autoSaveTimerRef = useRef<NodeJS.Timeout | null>(null);

  // 从数据库加载回顾分析（复用错题分析的加载模式）
  const loadReviewAnalysis = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      
      console.log('🔍 从数据库加载回顾分析:', sessionId);
      const analysis = await TauriAPI.getReviewAnalysisById(sessionId);
      
      if (analysis) {
        console.log('✅ 成功加载回顾分析:', analysis);
        setReviewAnalysis(analysis);
      } else {
        console.error('❌ 未找到回顾分析:', sessionId);
        setError('未找到指定的回顾分析');
      }
    } catch (error) {
      console.error('加载回顾分析失败:', error);
      setError(String(error));
      showNotification('error', '加载回顾分析失败');
    } finally {
      setLoading(false);
    }
  }, [sessionId, showNotification]);

  useEffect(() => {
    loadReviewAnalysis();
  }, [loadReviewAnalysis]);

  // 🎯 保存聊天历史的辅助函数
  const saveReviewAnalysisData = useCallback(async (chatHistory: any[], context: string) => {
    if (!reviewAnalysis || chatHistory.length === 0) return;
    
    try {
      console.log(`🔄 [${context}] 保存聊天历史，数量:`, chatHistory.length);
      
      const updatedAnalysis = {
        ...reviewAnalysis,
        chat_history: chatHistory,
        status: 'completed' as const,
        updated_at: new Date().toISOString()
      };
      
      // 🎯 首选方案：使用新的更新API
      try {
        await TauriAPI.updateReviewAnalysis(updatedAnalysis);
        console.log(`✅ [${context}] 聊天历史保存成功 (直接更新)`);
        return;
      } catch (updateError) {
        console.log(`⚠️ [${context}] 直接更新失败，尝试变通方案:`, updateError);
      }
      
      // 🎯 变通方案：使用追问API（它会自动保存聊天历史）
      await TauriAPI.continueReviewChatStream({
        reviewId: reviewAnalysis.id,
        chatHistory: chatHistory,
        enableChainOfThought: true,
        enableRag: false,
        ragTopK: 5
      });
      
      console.log(`✅ [${context}] 聊天历史保存成功 (变通方案)`);
    } catch (error) {
      console.error(`❌ [${context}] 保存聊天历史失败:`, error);
    }
  }, [reviewAnalysis]);

  // 🎯 新增：页面卸载时保存最新的聊天历史
  useEffect(() => {
    const handleBeforeUnload = () => {
      if (latestChatHistory.length > 0) {
        // beforeunload 不能使用 async，所以只能发起请求
        saveReviewAnalysisData(latestChatHistory, '页面卸载').catch(console.error);
      }
    };

    // 监听页面卸载事件
    window.addEventListener('beforeunload', handleBeforeUnload);
    
    // 组件卸载时的清理
    return () => {
      window.removeEventListener('beforeunload', handleBeforeUnload);
      
      // 清理定时器
      if (autoSaveTimerRef.current) {
        clearTimeout(autoSaveTimerRef.current);
        autoSaveTimerRef.current = null;
      }
      
      // 组件卸载时保存（非async）
      if (latestChatHistory.length > 0) {
        console.log('🔄 [组件卸载] 尝试保存最新聊天历史');
        saveReviewAnalysisData(latestChatHistory, '组件卸载').catch(console.error);
      }
    };
  }, [latestChatHistory, saveReviewAnalysisData]);

  // 保存回顾分析到数据库（复用错题分析的保存模式）
  const handleSaveReviewAnalysis = useCallback(async (updatedAnalysis: ReviewAnalysisItem) => {
    try {
      // 注意：后端API已经自动保存，这里主要是更新本地状态
      setReviewAnalysis(updatedAnalysis);
      console.log('✅ 回顾分析状态已更新');
    } catch (error) {
      console.error('更新回顾分析状态失败:', error);
      showNotification('error', '更新状态失败');
    }
  }, [showNotification]);

  // 加载中状态
  if (loading) {
    return (
      <div className="flex justify-center items-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
        <span className="ml-2 text-gray-600">加载中...</span>
      </div>
    );
  }

  // 错误状态
  if (error || !reviewAnalysis) {
    return (
      <div className="text-center py-12">
        <h3 className="text-lg font-medium text-red-700 mb-2">加载失败</h3>
        <p className="text-gray-600 mb-4">
          {error || '未找到回顾分析'}
        </p>
        <button
          onClick={onBack}
          className="bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-lg transition-colors"
        >
          返回列表
        </button>
      </div>
    );
  }

  // 主界面 - 使用UniversalAppChatHost，完全复用错题分析的逻辑
  return (
    <UniversalAppChatHost
      mode="REVIEW_SESSION_DETAIL"
      businessSessionId={reviewAnalysis.id}
      preloadedData={{
        subject: reviewAnalysis.subject,
        userQuestion: reviewAnalysis.user_question,
        ocrText: reviewAnalysis.consolidated_input,
        tags: reviewAnalysis.mistake_ids, // 使用mistake_ids作为标签
        chatHistory: reviewAnalysis.chat_history || [],
        thinkingContent: new Map(), // 从聊天历史中恢复思维链
        status: reviewAnalysis.status,
      }}
      serviceConfig={{
        apiProvider: {
          initiateAndGetStreamId: async () => ({
            streamIdForEvents: reviewAnalysis.id,
            ocrResultData: {
              ocr_text: reviewAnalysis.consolidated_input,
              tags: reviewAnalysis.mistake_ids,
              mistake_type: reviewAnalysis.analysis_type
            },
            initialMessages: reviewAnalysis.chat_history || []
          }),
          startMainStreaming: async (params) => {
            // 根据回顾分析状态决定是否启动流式处理
            const needsInitialAnalysis = !reviewAnalysis.chat_history || 
                                       reviewAnalysis.chat_history.length === 0 ||
                                       reviewAnalysis.status === 'pending' ||
                                       reviewAnalysis.status === 'analyzing';
            
            if (needsInitialAnalysis) {
              console.log('🚀 启动回顾分析流式处理');
              try {
                await TauriAPI.triggerConsolidatedReviewStream({
                  review_session_id: reviewAnalysis.id,
                  enable_chain_of_thought: params.enableChainOfThought,
                  enable_rag: params.enableRag,
                  rag_options: params.enableRag ? { top_k: params.ragTopK } : undefined
                });
                console.log('✅ 回顾分析流式处理已启动');
              } catch (error) {
                console.error('❌ 启动回顾分析流式处理失败:', error);
                throw error;
              }
            } else {
              console.log('ℹ️ 回顾分析已完成，无需启动流式处理');
            }
          },
          continueUserChat: async (params) => {
            // 使用新的API进行追问（自动保存到数据库）
            await TauriAPI.continueReviewChatStream({
              reviewId: params.businessId,
              chatHistory: params.fullChatHistory,
              enableChainOfThought: params.enableChainOfThought,
              enableRag: params.enableRag,
              ragTopK: params.ragTopK
            });
          }
        },
        streamEventNames: {
          initialStream: (id) => ({ 
            data: `review_analysis_stream_${id}`, 
            reasoning: `review_analysis_stream_${id}_reasoning`,
            ragSources: `review_analysis_stream_${id}_rag_sources`
          }),
          // 🎯 修复：追问使用不同的事件名称，参考错题分析的正确实现
          continuationStream: (id) => ({ 
            data: `review_chat_stream_${id}`, 
            reasoning: `review_chat_stream_${id}_reasoning`,
            ragSources: `review_chat_stream_${id}_rag_sources`
          }),
        },
        defaultEnableChainOfThought: true,
        defaultEnableRag: false,
        defaultRagTopK: 5,
      }}
      onCoreStateUpdate={(data) => {
        // 🎯 实现状态更新监听，主要用于调试和状态同步
        console.log('🔄 [回顾分析] 核心状态更新:', {
          sessionId,
          chatHistoryLength: data.chatHistory?.length || 0,
          thinkingContentSize: data.thinkingContent?.size || 0,
          isAnalyzing: data.isAnalyzing,
          isChatting: data.isChatting
        });
        
        // 🎯 关键修复：保存最新的聊天历史到状态中
        if (data.chatHistory) {
          setLatestChatHistory(data.chatHistory);
          
          // 🎯 新增：设置定时自动保存（防抖）
          if (autoSaveTimerRef.current) {
            clearTimeout(autoSaveTimerRef.current);
          }
          
          // 30秒后自动保存
          autoSaveTimerRef.current = setTimeout(() => {
            if (data.chatHistory.length > 0) {
              console.log('🕒 [定时保存] 触发自动保存');
              saveReviewAnalysisData(data.chatHistory, '定时保存').catch(console.error);
            }
          }, 30000); // 30秒定时保存
        }
        
        // 更新本地状态
        if (data.chatHistory && data.chatHistory.length > 0) {
          setReviewAnalysis(prev => prev ? {
            ...prev,
            chat_history: data.chatHistory,
            status: 'completed',
            updated_at: new Date().toISOString()
          } : prev);
        }
      }}
      onSaveRequest={async (data) => {
        try {
          console.log('💾 回顾分析手动保存请求，数据:', data);
          
          // 🎯 使用新的保存函数
          await saveReviewAnalysisData(data.chatHistory, '手动保存');
          
          // 更新本地状态
          setReviewAnalysis(prev => prev ? {
            ...prev,
            chat_history: data.chatHistory,
            status: 'completed',
            updated_at: new Date().toISOString()
          } : prev);
          
          showNotification('success', '回顾分析已保存');
        } catch (error) {
          console.error('❌ 手动保存回顾分析失败:', error);
          showNotification('error', '保存失败: ' + error);
        }
      }}
      onExitRequest={onBack}
    />
  );
};

export default ReviewAnalysisSessionView;