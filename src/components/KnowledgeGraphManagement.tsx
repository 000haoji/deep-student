import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { 
  ProblemCard, 
  SearchRequest, 
  SearchResult, 
  CreateCardRequest, 
  GraphConfig,
  Recommendation,
  RecommendationRequest,
  Tag 
} from '../types/cogni-graph';
import GraphVisualization from './GraphVisualization';
import TagManagement from './TagManagement';
import './KnowledgeGraphManagement.css';

const KnowledgeGraphManagement: React.FC = () => {
  const [isInitialized, setIsInitialized] = useState(false);
  const [config, setConfig] = useState<GraphConfig>({
    neo4j: {
      uri: 'neo4j://127.0.0.1:7687',
      username: 'neo4j',
      password: '',
      database: undefined
    },
    vector_dimensions: 1536,
    similarity_threshold: 0.7,
    max_search_results: 100,
    recommendation_limit: 10
  });
  
  const [searchQuery, setSearchQuery] = useState('');
  const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
  const [selectedCard, setSelectedCard] = useState<ProblemCard | null>(null);
  const [recommendations, setRecommendations] = useState<Recommendation[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [activeTab, setActiveTab] = useState<'cards' | 'visualization' | 'tags'>('cards');
  
  const [newCard, setNewCard] = useState<CreateCardRequest>({
    content_problem: '',
    content_insight: '',
    tags: [],
    source_excalidraw_path: undefined
  });
  
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize knowledge graph
  const initializeGraph = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const result = await invoke<string>('initialize_knowledge_graph', { config });
      console.log('Graph initialized:', result);
      setIsInitialized(true);
      
      // Load initial data
      await loadAllTags();
    } catch (err) {
      setError(`Failed to initialize: ${err}`);
      console.error('Initialization error:', err);
    } finally {
      setLoading(false);
    }
  };

  // Test Neo4j connection
  const testConnection = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const result = await invoke<string>('test_neo4j_connection', { config });
      alert(`连接测试成功: ${result}`);
    } catch (err) {
      const errorMsg = `连接失败: ${err}`;
      setError(errorMsg);
      
      // Provide helpful error guidance
      if (String(err).includes('authentication failure')) {
        setError(errorMsg + '\n\n请检查:\n1. Neo4j是否正在运行\n2. 用户名和密码是否正确\n3. 默认密码可能需要重置');
      } else if (String(err).includes('Connection refused')) {
        setError(errorMsg + '\n\n请检查:\n1. Neo4j服务是否启动\n2. 端口7687是否开放\n3. 连接地址是否正确');
      }
    } finally {
      setLoading(false);
    }
  };

  // Create new problem card
  const createCard = async () => {
    if (!newCard.content_problem.trim() || !newCard.content_insight.trim()) {
      setError('Please fill in both problem and insight fields');
      return;
    }

    try {
      setLoading(true);
      setError(null);
      
      const cardId = await invoke<string>('create_problem_card', { request: newCard });
      console.log('Card created:', cardId);
      
      // Reset form
      setNewCard({
        content_problem: '',
        content_insight: '',
        tags: [],
        source_excalidraw_path: undefined
      });
      
      // Refresh tags
      await loadAllTags();
      
      alert(`Card created successfully with ID: ${cardId}`);
    } catch (err) {
      setError(`Failed to create card: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  // Search knowledge graph
  const searchGraph = async () => {
    if (!searchQuery.trim()) return;

    try {
      setLoading(true);
      setError(null);
      
      const request: SearchRequest = {
        query: searchQuery,
        limit: 20
      };
      
      const results = await invoke<SearchResult[]>('search_knowledge_graph', { request });
      setSearchResults(results);
    } catch (err) {
      setError(`Search failed: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  // Get card details and recommendations
  const selectCard = async (card: ProblemCard) => {
    try {
      setSelectedCard(card);
      setLoading(true);
      setError(null);
      
      // Get AI recommendations
      const request: RecommendationRequest = {
        card_id: card.id,
        limit: 5
      };
      
      const recs = await invoke<Recommendation[]>('get_ai_recommendations', { request });
      setRecommendations(recs);
    } catch (err) {
      setError(`Failed to get recommendations: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  // Load all tags
  const loadAllTags = async () => {
    try {
      const tags = await invoke<Tag[]>('get_all_tags');
      setAllTags(tags);
    } catch (err) {
      console.error('Failed to load tags:', err);
    }
  };

  return (
    <div className="knowledge-graph-management">
      <h2>CogniGraph 知识图谱管理</h2>
      
      {error && (
        <div className="error-message">
          错误: {error}
        </div>
      )}
      
      {!isInitialized ? (
        <div className="initialization-section">
          <h3>Neo4j 配置</h3>
          
          <div className="setup-help">
            <h4>快速设置 Neo4j</h4>
            <div className="help-option">
              <strong>Neo4j Desktop (当前推荐):</strong>
              <div>1. URI: <code>neo4j://127.0.0.1:7687</code> 或 <code>bolt://127.0.0.1:7687</code></div>
              <div>2. 用户名: <code>neo4j</code></div>
              <div>3. 密码: 你设置的数据库密码</div>
            </div>
            <div className="help-option">
              <strong>Docker方式:</strong>
              <code>
                docker run --name neo4j-cogni --publish=7474:7474 --publish=7687:7687 --env=NEO4J_AUTH=neo4j/password neo4j:latest
              </code>
            </div>
            <div className="help-option">
              <strong>下载 Neo4j Desktop:</strong> 
              <a href="https://neo4j.com/download/" target="_blank" rel="noopener noreferrer">
                https://neo4j.com/download/
              </a>
            </div>
          </div>
          
          <div className="config-form">
            <div className="form-group">
              <label>数据库地址:</label>
              <input
                type="text"
                value={config.neo4j.uri}
                onChange={(e) => setConfig({
                  ...config,
                  neo4j: { ...config.neo4j, uri: e.target.value }
                })}
                placeholder="neo4j://127.0.0.1:7687"
              />
            </div>
            
            <div className="form-group">
              <label>用户名:</label>
              <input
                type="text"
                value={config.neo4j.username}
                onChange={(e) => setConfig({
                  ...config,
                  neo4j: { ...config.neo4j, username: e.target.value }
                })}
              />
            </div>
            
            <div className="form-group">
              <label>密码:</label>
              <input
                type="password"
                value={config.neo4j.password}
                onChange={(e) => setConfig({
                  ...config,
                  neo4j: { ...config.neo4j, password: e.target.value }
                })}
                placeholder="输入你在Neo4j Desktop中设置的密码"
              />
            </div>
            
            <div className="button-group">
              <button onClick={testConnection} disabled={loading}>
                测试连接
              </button>
              <button onClick={initializeGraph} disabled={loading}>
                初始化图谱
              </button>
            </div>
          </div>
        </div>
      ) : (
        <div className="main-interface">
          {/* Tab navigation */}
          <div className="tab-navigation">
            <button 
              className={`tab-button ${activeTab === 'cards' ? 'active' : ''}`}
              onClick={() => setActiveTab('cards')}
            >
              📋 问题卡片管理
            </button>
            <button 
              className={`tab-button ${activeTab === 'visualization' ? 'active' : ''}`}
              onClick={() => setActiveTab('visualization')}
            >
              🔍 图谱可视化
            </button>
            <button 
              className={`tab-button ${activeTab === 'tags' ? 'active' : ''}`}
              onClick={() => setActiveTab('tags')}
            >
              🏷️ 标签管理
            </button>
          </div>

          {/* Tab content */}
          {activeTab === 'cards' && (
            <div className="tab-content">
              {/* Create new card section */}
              <div className="create-card-section">
            <h3>创建新题目卡片</h3>
            <div className="form-group">
              <label>题目描述:</label>
              <textarea
                value={newCard.content_problem}
                onChange={(e) => setNewCard({
                  ...newCard,
                  content_problem: e.target.value
                })}
                placeholder="请输入题目内容..."
                rows={3}
              />
            </div>
            
            <div className="form-group">
              <label>解题灵感/方法:</label>
              <textarea
                value={newCard.content_insight}
                onChange={(e) => setNewCard({
                  ...newCard,
                  content_insight: e.target.value
                })}
                placeholder="请输入解题思路和关键洞察..."
                rows={3}
              />
            </div>
            
            <div className="form-group">
              <label>标签 (用逗号分隔):</label>
              <input
                type="text"
                value={newCard.tags.join(', ')}
                onChange={(e) => setNewCard({
                  ...newCard,
                  tags: e.target.value.split(',').map(tag => tag.trim()).filter(tag => tag)
                })}
                placeholder="例如: 微积分, 求导, 高等数学"
              />
            </div>
            
            <button onClick={createCard} disabled={loading}>
              创建卡片
            </button>
          </div>

          {/* Search section */}
          <div className="search-section">
            <h3>搜索知识图谱</h3>
            <div className="search-form">
              <input
                type="text"
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
                placeholder="输入搜索关键词..."
                onKeyPress={(e) => e.key === 'Enter' && searchGraph()}
              />
              <button onClick={searchGraph} disabled={loading || !searchQuery.trim()}>
                搜索
              </button>
            </div>
            
            {searchResults.length > 0 && (
              <div className="search-results">
                <h4>搜索结果 ({searchResults.length})</h4>
                {searchResults.map((result, index) => (
                  <div 
                    key={result.card.id} 
                    className="search-result-item"
                    onClick={() => selectCard(result.card)}
                  >
                    <div className="result-score">
                      相似度: {(result.score * 100).toFixed(1)}%
                    </div>
                    <div className="result-content">
                      <strong>题目:</strong> {result.card.content_problem.substring(0, 100)}...
                    </div>
                    <div className="result-insight">
                      <strong>解法:</strong> {result.card.content_insight.substring(0, 100)}...
                    </div>
                    <div className="result-meta">
                      匹配方式: {result.matched_by.join(', ')} | 
                      访问次数: {result.card.access_count} | 
                      状态: {result.card.status}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Selected card and recommendations */}
          {selectedCard && (
            <div className="selected-card-section">
              <h3>卡片详情</h3>
              <div className="card-details">
                <div className="card-problem">
                  <strong>题目:</strong>
                  <p>{selectedCard.content_problem}</p>
                </div>
                <div className="card-insight">
                  <strong>解题洞察:</strong>
                  <p>{selectedCard.content_insight}</p>
                </div>
                <div className="card-meta">
                  <span>状态: {selectedCard.status}</span>
                  <span>访问次数: {selectedCard.access_count}</span>
                  <span>创建时间: {new Date(selectedCard.created_at).toLocaleString()}</span>
                </div>
              </div>
              
              {recommendations.length > 0 && (
                <div className="recommendations">
                  <h4>AI 推荐的相关题目</h4>
                  {recommendations.map((rec, index) => (
                    <div key={rec.card.id} className="recommendation-item">
                      <div className="rec-header">
                        <span className="rec-type">{rec.relationship_type}</span>
                        <span className="rec-confidence">
                          置信度: {(rec.confidence * 100).toFixed(1)}%
                        </span>
                      </div>
                      <div className="rec-content">
                        <strong>题目:</strong> {rec.card.content_problem.substring(0, 150)}...
                      </div>
                      <div className="rec-reasoning">
                        <strong>推荐理由:</strong> {rec.reasoning}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}

              {/* Tags overview */}
              {allTags.length > 0 && (
                <div className="tags-section">
                  <h3>标签概览 ({allTags.length})</h3>
                  <div className="tags-list">
                    {allTags.map((tag, index) => (
                      <span key={index} className={`tag tag-${tag.tag_type}`}>
                        {tag.name}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>
          )}

          {/* Graph Visualization Tab */}
          {activeTab === 'visualization' && (
            <div className="tab-content">
              {!isInitialized ? (
                <div className="visualization-setup-hint">
                  <h3>📊 图谱可视化</h3>
                  <div className="setup-message">
                    <p>🔧 要使用图谱可视化功能，请先完成以下步骤：</p>
                    <ol>
                      <li>返回 <strong>"问题卡片管理"</strong> 标签页</li>
                      <li>完成 Neo4j 数据库配置</li>
                      <li>点击 <strong>"测试连接"</strong> 确保连接成功</li>
                      <li>点击 <strong>"初始化图谱"</strong> 完成设置</li>
                    </ol>
                    <p>💡 <strong>Neo4j 快速设置：</strong></p>
                    <ul>
                      <li>下载 Neo4j Desktop: <a href="https://neo4j.com/download/" target="_blank" rel="noopener noreferrer">https://neo4j.com/download/</a></li>
                      <li>创建数据库，设置密码</li>
                      <li>使用 URI: <code>neo4j://127.0.0.1:7687</code></li>
                    </ul>
                  </div>
                  <div className="nav-hint">
                    <button 
                      onClick={() => setActiveTab('cards')}
                      className="nav-button"
                    >
                      ⬅️ 前往配置 Neo4j
                    </button>
                  </div>
                </div>
              ) : (
                <GraphVisualization 
                  config={config} 
                  isInitialized={isInitialized} 
                />
              )}
            </div>
          )}

          {/* Tag Management Tab */}
          {activeTab === 'tags' && (
            <div className="tab-content">
              <TagManagement 
                isInitialized={isInitialized} 
              />
            </div>
          )}
        </div>
      )}
      
      {loading && (
        <div className="loading-overlay">
          <div className="loading-spinner">处理中...</div>
        </div>
      )}
    </div>
  );
};

export default KnowledgeGraphManagement;