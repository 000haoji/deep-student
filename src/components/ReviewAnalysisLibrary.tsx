/**
 * DEPRECATED: 此组件已废弃 - 2024年6月8日
 * 原因: 功能已被统一回顾分析(ReviewAnalysisDashboard)替代
 * 
 * 这个组件使用localStorage存储数据，与新的数据库存储系统不兼容
 * 请使用 ReviewAnalysisDashboard 代替此组件
 */

import React, { useState, useEffect } from 'react';
import { ReviewSessionTask } from '../types/index';
import { useNotification } from '../hooks/useNotification';

interface ReviewAnalysisLibraryProps {
  onSelectAnalysis: (analysis: ReviewSessionTask) => void;
  onBack: () => void;
}

export const ReviewAnalysisLibrary: React.FC<ReviewAnalysisLibraryProps> = ({ onSelectAnalysis, onBack }) => {
  const [analyses, setAnalyses] = useState<ReviewSessionTask[]>([]);
  const [filteredAnalyses, setFilteredAnalyses] = useState<ReviewSessionTask[]>([]);
  const [selectedSubject, setSelectedSubject] = useState('全部');
  const [selectedStatus, setSelectedStatus] = useState('全部');
  const [searchTerm, setSearchTerm] = useState('');
  const [loading, setLoading] = useState(false);
  const [availableSubjects, setAvailableSubjects] = useState<string[]>([]);
  
  const { showNotification } = useNotification();

  // 加载回顾分析数据
  useEffect(() => {
    const loadData = async () => {
      setLoading(true);
      try {
        // 从localStorage加载回顾分析数据 - 使用和会话相同的存储键
        const savedAnalyses = localStorage.getItem('reviewSessions');
        if (savedAnalyses) {
          const parsedAnalyses = JSON.parse(savedAnalyses).map((analysis: any) => ({
            ...analysis,
            thinkingContent: new Map(Object.entries(analysis.thinkingContent || {}))
          }));
          
          setAnalyses(parsedAnalyses);
          setFilteredAnalyses(parsedAnalyses);
          
          // 动态提取可用的科目选项
          const subjects = Array.from(new Set(parsedAnalyses.map((a: ReviewSessionTask) => a.subject))).sort();
          setAvailableSubjects(subjects);
          
          console.log('加载回顾分析库数据:', {
            总数: parsedAnalyses.length,
            科目: subjects
          });
        } else {
          setAnalyses([]);
          setFilteredAnalyses([]);
          setAvailableSubjects([]);
        }
      } catch (error) {
        console.error('加载回顾分析失败:', error);
        showNotification('error', '加载回顾分析失败: ' + error);
      } finally {
        setLoading(false);
      }
    };
    
    loadData();
  }, [showNotification]);

  // 筛选逻辑
  useEffect(() => {
    let filtered = analyses;
    
    if (selectedSubject !== '全部') {
      filtered = filtered.filter(analysis => analysis.subject === selectedSubject);
    }
    
    if (selectedStatus !== '全部') {
      filtered = filtered.filter(analysis => {
        if (selectedStatus === '已完成') return analysis.status === 'completed';
        if (selectedStatus === '进行中') return analysis.status !== 'completed' && !analysis.status.includes('error');
        if (selectedStatus === '错误') return analysis.status.includes('error');
        return true;
      });
    }
    
    if (searchTerm.trim()) {
      const term = searchTerm.toLowerCase();
      filtered = filtered.filter(analysis => 
        analysis.name.toLowerCase().includes(term) ||
        analysis.userOverallPrompt?.toLowerCase().includes(term) ||
        analysis.id.toLowerCase().includes(term)
      );
    }
    
    setFilteredAnalyses(filtered);
  }, [analyses, selectedSubject, selectedStatus, searchTerm]);

  const deleteAnalysis = async (analysisId: string) => {
    if (!confirm('确定要删除这个回顾记录吗？')) return;
    
    try {
      const updatedAnalyses = analyses.filter(a => a.id !== analysisId);
      setAnalyses(updatedAnalyses);
      
      // 保存到localStorage
      const analysesToSave = updatedAnalyses.map(analysis => ({
        ...analysis,
        thinkingContent: Object.fromEntries(analysis.thinkingContent)
      }));
      localStorage.setItem('reviewSessions', JSON.stringify(analysesToSave));
      
      showNotification('success', '回顾记录已删除');
    } catch (error) {
      console.error('删除回顾分析失败:', error);
      showNotification('error', '删除失败: ' + error);
    }
  };

  const getStatusText = (status: string) => {
    switch (status) {
      case 'completed': return '已完成';
      case 'pending': return '等待中';
      case 'processing_setup': return '准备中';
      case 'streaming_answer': return '分析中';
      default: 
        if (status.includes('error')) return '错误';
        return '未知';
    }
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'completed': return '#28a745';
      case 'pending': 
      case 'processing_setup':
      case 'streaming_answer': return '#007bff';
      default:
        if (status.includes('error')) return '#dc3545';
        return '#6c757d';
    }
  };

  return (
    <div className="review-analysis-container">
      <div className="review-analysis-header">
        <button onClick={onBack} className="review-analysis-back-btn">
          ← 返回
        </button>
        <h2 className="review-analysis-title">📚 回顾库</h2>
        <div className="library-stats">
          <span className="stats-text">共 {filteredAnalyses.length} 个回顾</span>
          {filteredAnalyses.length !== analyses.length && (
            <span className="stats-filter">（从 {analyses.length} 个中筛选）</span>
          )}
        </div>
      </div>

      <div className="review-analysis-main">
        <div className="review-filters">
          <div className="review-form-group">
            <label className="review-form-label">科目:</label>
            <select 
              className="review-form-select"
              value={selectedSubject} 
              onChange={(e) => setSelectedSubject(e.target.value)}
            >
              <option value="全部">全部科目</option>
              {availableSubjects.map(subject => (
                <option key={subject} value={subject}>{subject}</option>
              ))}
            </select>
          </div>

          <div className="review-form-group">
            <label className="review-form-label">状态:</label>
            <select 
              className="review-form-select"
              value={selectedStatus} 
              onChange={(e) => setSelectedStatus(e.target.value)}
            >
              <option value="全部">全部状态</option>
              <option value="已完成">已完成</option>
              <option value="进行中">进行中</option>
              <option value="错误">错误</option>
            </select>
          </div>

          <div className="review-form-group search-group">
            <label className="review-form-label">搜索:</label>
            <input
              className="review-form-input search-input"
              type="text"
              placeholder="搜索回顾名称、目标或ID..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
            />
          </div>
        </div>

        {loading ? (
          <div className="review-loading">
            <div className="review-loading-spinner"></div>
            <span className="review-loading-text">加载中...</span>
          </div>
        ) : filteredAnalyses.length === 0 ? (
          <div className="review-empty">
            <div className="empty-icon">📭</div>
            <h3 className="empty-title">暂无回顾记录</h3>
            <p className="empty-description">还没有保存的回顾记录</p>
            <p className="empty-hint">完成回顾分析后，点击"保存到回顾库"按钮即可保存</p>
          </div>
        ) : (
          <div className="analysis-library-grid">
            {filteredAnalyses.map((analysis) => (
              <div 
                key={analysis.id}
                className="analysis-library-card"
                onClick={() => onSelectAnalysis(analysis)}
              >
                <div className="analysis-card-header">
                  <div className="analysis-card-info">
                    <h4 className="analysis-card-title">{analysis.name}</h4>
                    <span className="analysis-card-id">ID: {analysis.id}</span>
                  </div>
                  <div className="analysis-card-actions">
                    <span 
                      className="analysis-status-badge"
                      style={{ 
                        backgroundColor: getStatusColor(analysis.status),
                        color: 'white'
                      }}
                    >
                      {getStatusText(analysis.status)}
                    </span>
                    <button
                      className="analysis-delete-btn"
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteAnalysis(analysis.id);
                      }}
                      title="删除回顾"
                    >
                      🗑️
                    </button>
                  </div>
                </div>
                
                <div className="analysis-card-content">
                  <div className="analysis-meta-row">
                    <span className="meta-item subject-tag">📖 {analysis.subject}</span>
                    <span className="meta-item mistake-count">📝 {analysis.mistakeIds.length} 个错题</span>
                    <span className="meta-item chat-count">💬 {analysis.chatHistory.length} 条对话</span>
                  </div>
                  
                  <div className="analysis-target-section">
                    <div className="target-label">回顾目标:</div>
                    <div className="target-content">{analysis.userOverallPrompt || '统一回顾多个错题'}</div>
                  </div>
                  
                  {analysis.chatHistory.length > 0 && (
                    <div className="analysis-preview-section">
                      <div className="preview-label">回顾摘要:</div>
                      <div className="preview-content">{analysis.chatHistory[0]?.content?.substring(0, 100)}...</div>
                    </div>
                  )}
                  
                  <div className="analysis-date-section">
                    <span className="date-icon">🕒</span>
                    <span className="date-text">{new Date(analysis.creationDate).toLocaleString('zh-CN')}</span>
                  </div>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};