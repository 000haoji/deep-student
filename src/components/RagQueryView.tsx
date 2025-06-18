import React, { useState, useCallback } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { useNotification } from '../hooks/useNotification';
import type { RetrievedChunk, RagQueryOptions } from '../types';
import './RagQueryView.css';
import { Search, Lightbulb, Sparkles } from 'lucide-react';

interface RagQueryViewProps {
  className?: string;
}

export const RagQueryView: React.FC<RagQueryViewProps> = ({ 
  className = '' 
}) => {
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [retrievedChunks, setRetrievedChunks] = useState<RetrievedChunk[]>([]);
  const [llmAnswer, setLlmAnswer] = useState('');
  const [queryOptions, setQueryOptions] = useState<RagQueryOptions>({
    top_k: 5,
    enable_reranking: false,
  });

  const { showSuccess, showError, showWarning } = useNotification();

  // 执行RAG查询
  const handleRagQuery = useCallback(async () => {
    if (!query.trim()) {
      showWarning('请输入查询内容');
      return;
    }

    try {
      setLoading(true);
      setRetrievedChunks([]);
      setLlmAnswer('');

      console.log('执行RAG查询:', query, queryOptions);

      // 1. 执行RAG检索
      const retrievalResult = await TauriAPI.ragQueryKnowledgeBase(query, queryOptions);
      setRetrievedChunks(retrievalResult.retrieved_chunks || []);

      if (retrievalResult.retrieved_chunks.length === 0) {
        showWarning('在知识库中未找到相关内容');
        return;
      }

      showSuccess(`检索到 ${retrievalResult.retrieved_chunks.length} 个相关文档片段`);

      // 2. 使用LLM生成回答
      console.log('调用LLM生成回答...');
      const answer = await TauriAPI.llmGenerateAnswerWithContext(query, JSON.stringify(retrievalResult.retrieved_chunks));
      setLlmAnswer(answer);

      showSuccess('RAG查询完成');

    } catch (error) {
      console.error('RAG查询失败:', error);
      let errorMessage = '查询失败';
      
      if (typeof error === 'string') {
        errorMessage = `查询失败: ${error}`;
      } else if (error instanceof Error) {
        errorMessage = `查询失败: ${error.message}`;
      } else if (error && typeof error === 'object' && 'message' in error) {
        errorMessage = `查询失败: ${(error as any).message}`;
      }
      
      showError(errorMessage);
    } finally {
      setLoading(false);
    }
  }, [query, queryOptions, showSuccess, showError, showWarning]);

  // 处理Enter键提交
  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleRagQuery();
    }
  };

  // 格式化相似度分数
  const formatScore = (score: number) => {
    return (score * 100).toFixed(1) + '%';
  };

  // 截断文本用于显示
  const truncateText = (text: string, maxLength: number = 200) => {
    if (text.length <= maxLength) return text;
    return text.substring(0, maxLength) + '...';
  };

  return (
    <div className={`rag-query-view ${className}`} style={{ width: '100%', height: '100%', overflow: 'auto', background: '#f8fafc' }}>
      {/* 统一头部样式 */}
      <div style={{
        background: 'white',
        borderBottom: '1px solid #e5e7eb',
        padding: '24px 32px',
        position: 'relative'
      }}>
        <div style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: '4px',
          background: 'linear-gradient(90deg, #667eea, #764ba2)'
        }}></div>
        
        <div style={{ position: 'relative', zIndex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', marginBottom: '8px' }}>
            <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
              <path d="m21 21-4.34-4.34" />
              <circle cx="11" cy="11" r="8" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>RAG 知识库查询</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            测试知识库检索和增强生成功能，验证AI回答质量
          </p>
        </div>
      </div>
      
      <div style={{ padding: '24px' }}>

      {/* 查询配置区域 */}
      <div className="query-config-section">
        <h3>查询参数配置</h3>
        <div className="config-controls">
          <div className="config-item">
            <label htmlFor="top-k">检索片段数量 (top_k):</label>
            <input
              id="top-k"
              type="number"
              min="1"
              max="20"
              value={queryOptions.top_k}
              onChange={(e) => setQueryOptions(prev => ({
                ...prev,
                top_k: parseInt(e.target.value) || 5
              }))}
            />
          </div>
          <div className="config-item">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={queryOptions.enable_reranking}
                onChange={(e) => setQueryOptions(prev => ({
                  ...prev,
                  enable_reranking: e.target.checked
                }))}
              />
              启用重排序 (Reranking)
            </label>
          </div>
        </div>
      </div>

      {/* 查询输入区域 */}
      <div className="query-input-section">
        <h3>查询输入</h3>
        <div className="query-input-container">
          <textarea
            className="query-input"
            placeholder="请输入您的问题..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyPress={handleKeyPress}
            rows={3}
            disabled={loading}
          />
          <button
            className="query-button"
            onClick={handleRagQuery}
            disabled={loading || !query.trim()}
          >
            {loading ? '查询中...' : '执行查询'}
          </button>
        </div>
        <div className="input-hint" style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          <Lightbulb size={16} />
          提示：按 Enter 键快速提交查询，Shift+Enter 换行
        </div>
      </div>

      {/* 检索结果区域 */}
      {retrievedChunks.length > 0 && (
        <div className="retrieval-results-section">
          <h3>检索到的知识片段</h3>
          <div className="retrieved-chunks">
            {retrievedChunks.map((chunk, index) => (
              <div key={chunk.chunk.id} className="retrieved-chunk">
                <div className="chunk-header">
                  <span className="chunk-rank">#{index + 1}</span>
                  <span className="chunk-score">相似度: {formatScore(chunk.score)}</span>
                  <span className="chunk-source">
                    来源: {chunk.chunk.metadata.file_name || '未知'}
                  </span>
                </div>
                <div className="chunk-content">
                  <p>{truncateText(chunk.chunk.text)}</p>
                  {chunk.chunk.text.length > 200 && (
                    <button 
                      className="expand-button"
                      onClick={() => {
                        // 可以添加展开/收起功能
                        console.log('展开完整内容:', chunk.chunk.text);
                      }}
                    >
                      查看完整内容
                    </button>
                  )}
                </div>
                <div className="chunk-metadata">
                  <span>块索引: {chunk.chunk.chunk_index}</span>
                  <span>文档ID: {chunk.chunk.document_id.substring(0, 8)}...</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* LLM回答区域 */}
      {llmAnswer && (
        <div className="llm-answer-section">
          <h3>AI 回答</h3>
          <div className="llm-answer">
            <div className="answer-content">
              {llmAnswer.split('\n').map((paragraph, index) => (
                <p key={index}>{paragraph}</p>
              ))}
            </div>
            <div className="answer-footer">
              <span className="answer-source" style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <Sparkles size={16} />
                基于知识库中的 {retrievedChunks.length} 个相关片段生成
              </span>
            </div>
          </div>
        </div>
      )}

      {/* 加载状态 */}
      {loading && (
        <div className="loading-overlay">
          <div className="loading-spinner"></div>
          <p>正在查询知识库...</p>
        </div>
      )}
      </div>
    </div>
  );
};

export default RagQueryView;