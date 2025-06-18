import React, { useState, useEffect, useCallback } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { useNotification } from '../hooks/useNotification';
import { MarkdownRenderer } from './MarkdownRenderer';
import type { 
  RagQueryResponse,
  RetrievedChunk
} from '../types';
import './EnhancedRagQueryView.css';
import { 
  Search, 
  FileText, 
  BookOpen, 
  Target, 
  BarChart3, 
  Settings,
  Lightbulb
} from 'lucide-react';

// 定义分库接口类型
interface SubLibrary {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  document_count: number;
  chunk_count: number;
}

interface RagQueryOptionsWithLibraries {
  top_k: number;
  enable_reranking?: boolean;
  target_sub_library_ids?: string[];
}

interface EnhancedRagQueryViewProps {
  className?: string;
}

export const EnhancedRagQueryView: React.FC<EnhancedRagQueryViewProps> = ({ 
  className = '' 
}) => {
  // 查询相关状态
  const [query, setQuery] = useState('');
  const [queryResult, setQueryResult] = useState<RagQueryResponse | null>(null);
  const [isQuerying, setIsQuerying] = useState(false);
  
  // 分库相关状态
  const [subLibraries, setSubLibraries] = useState<SubLibrary[]>([]);
  const [selectedLibraries, setSelectedLibraries] = useState<string[]>([]);
  const [showLibrarySelector, setShowLibrarySelector] = useState(false);
  
  // 查询选项
  const [topK, setTopK] = useState(5);
  const [enableReranking, setEnableReranking] = useState(false);
  const [showAdvancedOptions, setShowAdvancedOptions] = useState(false);

  const { showSuccess, showError, showWarning } = useNotification();

  // 加载分库列表
  const loadSubLibraries = useCallback(async () => {
    try {
      const libraries = await TauriAPI.invoke('get_rag_sub_libraries') as SubLibrary[];
      setSubLibraries(libraries);
      
      // 默认选择所有分库
      if (selectedLibraries.length === 0) {
        setSelectedLibraries(libraries.map((lib: SubLibrary) => lib.id));
      }
    } catch (error) {
      console.error('加载分库列表失败:', error);
      showError(`加载分库列表失败: ${error}`);
    }
  }, [selectedLibraries.length, showError]);

  // 执行RAG查询
  const performQuery = async () => {
    if (!query.trim()) {
      showWarning('请输入查询内容');
      return;
    }

    if (selectedLibraries.length === 0) {
      showWarning('请至少选择一个分库');
      return;
    }

    setIsQuerying(true);
    
    try {
      const options: RagQueryOptionsWithLibraries = {
        top_k: topK,
        enable_reranking: enableReranking,
        target_sub_library_ids: selectedLibraries.length === subLibraries.length 
          ? undefined // 如果选择了所有分库，传undefined表示查询所有
          : selectedLibraries
      };

      const startTime = Date.now();
      const result = await TauriAPI.invoke('rag_query_knowledge_base_in_libraries', {
        query,
        options
      }) as RagQueryResponse;
      const endTime = Date.now();

      setQueryResult(result);
      
      const selectedLibraryNames = selectedLibraries.map(id => 
        subLibraries.find(lib => lib.id === id)?.name || id
      ).join(', ');
      
      showSuccess(
        `查询完成！在分库 [${selectedLibraryNames}] 中找到 ${result.retrieved_chunks.length} 个相关结果 (${endTime - startTime}ms)`
      );
    } catch (error) {
      console.error('RAG查询失败:', error);
      showError(`RAG查询失败: ${error}`);
    } finally {
      setIsQuerying(false);
    }
  };

  // 切换分库选择
  const toggleLibrarySelection = (libraryId: string) => {
    setSelectedLibraries(prev => 
      prev.includes(libraryId)
        ? prev.filter(id => id !== libraryId)
        : [...prev, libraryId]
    );
  };

  // 全选/取消全选分库
  const toggleAllLibraries = () => {
    setSelectedLibraries(
      selectedLibraries.length === subLibraries.length 
        ? [] 
        : subLibraries.map(lib => lib.id)
    );
  };

  // 快速选择预设
  const selectDefaultLibrary = () => {
    setSelectedLibraries(['default']);
  };

  const selectNonDefaultLibraries = () => {
    setSelectedLibraries(subLibraries.filter(lib => lib.id !== 'default').map(lib => lib.id));
  };

  // 初始化加载
  useEffect(() => {
    loadSubLibraries();
  }, [loadSubLibraries]);

  // 处理回车键查询
  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      performQuery();
    }
  };

  // 格式化时间显示
  const formatTime = (ms: number) => {
    return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${ms}ms`;
  };

  // 计算相似度颜色
  const getScoreColor = (score: number) => {
    if (score >= 0.8) return '#22c55e'; // 绿色
    if (score >= 0.6) return '#f59e0b'; // 橙色
    if (score >= 0.4) return '#ef4444'; // 红色
    return '#6b7280'; // 灰色
  };

  return (
    <div style={{
      width: '100%',
      height: '100%',
      overflow: 'auto',
      background: '#f8fafc'
    }}>
      {/* 头部区域 - 统一白色样式 */}
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
            <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>增强RAG智能查询</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            智能检索知识库内容，获取精准相关信息和文档片段
          </p>
          <div style={{ marginTop: '24px', display: 'flex', gap: '12px' }}>
            <button
              onClick={() => setShowLibrarySelector(!showLibrarySelector)}
              style={{
                background: showLibrarySelector ? '#667eea' : 'white',
                border: '1px solid #667eea',
                color: showLibrarySelector ? 'white' : '#667eea',
                padding: '12px 24px',
                borderRadius: '12px',
                fontSize: '16px',
                fontWeight: '600',
                cursor: 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                gap: '8px',
                transition: 'all 0.3s ease'
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.background = '#667eea';
                e.currentTarget.style.color = 'white';
                e.currentTarget.style.transform = 'translateY(-2px)';
                e.currentTarget.style.boxShadow = '0 8px 25px rgba(102, 126, 234, 0.3)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = showLibrarySelector ? '#667eea' : 'white';
                e.currentTarget.style.color = showLibrarySelector ? 'white' : '#667eea';
                e.currentTarget.style.transform = 'translateY(0)';
                e.currentTarget.style.boxShadow = 'none';
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10" />
              </svg>
              分库选择 ({selectedLibraries.length}/{subLibraries.length})
            </button>
            <button
              onClick={() => setShowAdvancedOptions(!showAdvancedOptions)}
              style={{
                background: showAdvancedOptions ? '#667eea' : 'white',
                border: '1px solid #d1d5db',
                color: showAdvancedOptions ? 'white' : '#374151',
                padding: '12px 24px',
                borderRadius: '12px',
                fontSize: '16px',
                fontWeight: '600',
                cursor: 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                gap: '8px',
                transition: 'all 0.3s ease'
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.background = '#f9fafb';
                e.currentTarget.style.transform = 'translateY(-2px)';
                e.currentTarget.style.boxShadow = '0 8px 25px rgba(0, 0, 0, 0.1)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = showAdvancedOptions ? '#667eea' : 'white';
                e.currentTarget.style.transform = 'translateY(0)';
                e.currentTarget.style.boxShadow = 'none';
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
              </svg>
              高级选项
            </button>
          </div>
        </div>
      </div>

      <div className={`enhanced-rag-query-view ${className}`} style={{ padding: '24px', background: 'transparent' }}>

      {/* 分库选择器 */}
      {showLibrarySelector && (
        <div className="library-selector">
          <div className="selector-header">
            <h3>选择查询分库</h3>
            <div className="selector-actions">
              <button 
                onClick={toggleAllLibraries}
                className="btn btn-sm btn-secondary"
              >
                {selectedLibraries.length === subLibraries.length ? '取消全选' : '全选'}
              </button>
              <button 
                onClick={selectDefaultLibrary}
                className="btn btn-sm btn-secondary"
              >
                仅默认库
              </button>
              <button 
                onClick={selectNonDefaultLibraries}
                className="btn btn-sm btn-secondary"
              >
                仅自定义库
              </button>
            </div>
          </div>
          
          <div className="library-grid">
            {subLibraries.map(library => (
              <div 
                key={library.id}
                className={`library-card ${selectedLibraries.includes(library.id) ? 'selected' : ''}`}
                onClick={() => toggleLibrarySelection(library.id)}
              >
                <div className="library-checkbox">
                  <input 
                    type="checkbox" 
                    checked={selectedLibraries.includes(library.id)}
                    onChange={() => toggleLibrarySelection(library.id)}
                  />
                </div>
                <div className="library-info">
                  <div className="library-name">{library.name}</div>
                  <div className="library-stats" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <FileText size={14} />
                    {library.document_count} | 
                    <BookOpen size={14} />
                    {library.chunk_count}
                  </div>
                  {library.description && (
                    <div className="library-description">{library.description}</div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* 高级选项 */}
      {showAdvancedOptions && (
        <div className="advanced-options">
          <div className="options-grid">
            <div className="option-group">
              <label>返回结果数量 (Top-K)</label>
              <input
                type="number"
                min="1"
                max="20"
                value={topK}
                onChange={(e) => setTopK(parseInt(e.target.value) || 5)}
                className="form-input"
              />
              <div className="option-hint">建议值: 3-10</div>
            </div>
            
            <div className="option-group">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={enableReranking}
                  onChange={(e) => setEnableReranking(e.target.checked)}
                />
                启用重排序
              </label>
              <div className="option-hint">使用重排序模型提高结果相关性</div>
            </div>
          </div>
        </div>
      )}

      {/* 查询输入区域 */}
      <div className="query-input-section">
        <div className="input-group">
          <textarea
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyPress={handleKeyPress}
            placeholder="输入您的问题，AI将在选定的知识库分库中搜索相关信息..."
            className="query-input"
            rows={3}
            disabled={isQuerying}
          />
          <button 
            onClick={performQuery}
            disabled={isQuerying || !query.trim() || selectedLibraries.length === 0}
            className="btn btn-primary query-btn"
            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <Search size={16} />
            {isQuerying ? '查询中...' : '智能查询'}
          </button>
        </div>
        
        {selectedLibraries.length > 0 && (
          <div className="selected-libraries-info">
            <span className="info-label">查询范围:</span>
            <div className="library-tags">
              {selectedLibraries.map(id => {
                const library = subLibraries.find(lib => lib.id === id);
                return library ? (
                  <span key={id} className="library-tag">
                    {library.name}
                  </span>
                ) : null;
              })}
            </div>
          </div>
        )}
      </div>

      {/* 查询结果 */}
      {queryResult && (
        <div className="query-results">
          <div className="results-header">
            <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <BarChart3 size={20} />
              查询结果 ({queryResult.retrieved_chunks.length} 个结果)
            </h3>
            <div className="results-meta">
              <div className="meta-item">
                <span className="label">查询时间:</span>
                <span className="value">{formatTime(queryResult.query_vector_time_ms)}</span>
              </div>
              <div className="meta-item">
                <span className="label">搜索时间:</span>
                <span className="value">{formatTime(queryResult.search_time_ms)}</span>
              </div>
              {queryResult.reranking_time_ms && (
                <div className="meta-item">
                  <span className="label">重排序时间:</span>
                  <span className="value">{formatTime(queryResult.reranking_time_ms)}</span>
                </div>
              )}
              <div className="meta-item">
                <span className="label">总时间:</span>
                <span className="value">{formatTime(queryResult.total_time_ms)}</span>
              </div>
            </div>
          </div>

          {queryResult.retrieved_chunks.length === 0 ? (
            <div className="no-results">
              <div className="no-results-icon">
                <Search size={48} />
              </div>
              <div className="no-results-text">未找到相关内容</div>
              <div className="no-results-hint">
                尝试使用不同的关键词或选择更多分库
              </div>
            </div>
          ) : (
            <div className="results-list">
              {queryResult.retrieved_chunks.map((chunk: RetrievedChunk, index) => (
                <div key={`${chunk.chunk.id}-${index}`} className="result-item">
                  <div className="result-header">
                    <div className="result-meta">
                      <span className="result-index">#{index + 1}</span>
                      <span className="result-source" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                        <FileText size={14} />
                        {chunk.chunk.metadata.filename || `文档-${chunk.chunk.document_id.slice(0, 8)}`}
                      </span>
                      <span className="result-chunk" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                        <BookOpen size={14} />
                        块 {chunk.chunk.chunk_index + 1}
                      </span>
                    </div>
                    <div 
                      className="similarity-score"
                      style={{ color: getScoreColor(chunk.score), display: 'flex', alignItems: 'center', gap: '4px' }}
                    >
                      <Target size={14} />
                      {(chunk.score * 100).toFixed(1)}%
                    </div>
                  </div>
                  
                  <div className="result-content">
                    <MarkdownRenderer content={chunk.chunk.text} />
                  </div>
                  
                  {chunk.chunk.metadata && Object.keys(chunk.chunk.metadata).length > 0 && (
                    <div className="result-metadata">
                      <details>
                        <summary style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                          <BookOpen size={16} />
                          元数据
                        </summary>
                        <div className="metadata-content">
                          {Object.entries(chunk.chunk.metadata).map(([key, value]) => (
                            <div key={key} className="metadata-item">
                              <span className="metadata-key">{key}:</span>
                              <span className="metadata-value">{value}</span>
                            </div>
                          ))}
                        </div>
                      </details>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* 使用说明 */}
      {!queryResult && (
        <div className="usage-guide">
          <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Target size={20} />
            使用指南
          </h3>
          <div className="guide-content">
            <div className="guide-section">
              <h4 style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <BookOpen size={16} />
                分库选择
              </h4>
              <ul>
                <li>选择一个或多个分库进行查询</li>
                <li>默认库包含通过普通上传添加的文档</li>
                <li>自定义库包含分类管理的专项文档</li>
              </ul>
            </div>
            
            <div className="guide-section">
              <h4 style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <Search size={16} />
                查询技巧
              </h4>
              <ul>
                <li>使用具体的关键词获得更准确的结果</li>
                <li>可以提出问题，AI会寻找相关答案</li>
                <li>支持中英文混合查询</li>
                <li>按 Enter 键快速查询，Shift+Enter 换行</li>
              </ul>
            </div>
            
            <div className="guide-section">
              <h4 style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <Settings size={16} />
                高级设置
              </h4>
              <ul>
                <li><strong>Top-K:</strong> 控制返回结果的数量</li>
                <li><strong>重排序:</strong> 使用AI模型重新排序结果以提高相关性</li>
                <li><strong>相似度分数:</strong> 绿色(80%+)表示高度相关，橙色(60-80%)表示中等相关</li>
              </ul>
            </div>
          </div>
        </div>
      )}
      </div>
    </div>
  );
};

export default EnhancedRagQueryView;