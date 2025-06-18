import React, { useState, useEffect } from 'react';
import { ReviewSessionTask } from '../types/index';
import { useNotification } from '../hooks/useNotification';
import { TauriAPI, ReviewAnalysisItem } from '../utils/tauriApi';
import { useSubject } from '../contexts/SubjectContext';
import { UnifiedSubjectSelector } from './shared/UnifiedSubjectSelector';
import './ReviewAnalysis.css';

interface ReviewAnalysisDashboardProps {
  onCreateNew: () => void;
  onViewSession: (sessionId: string) => void;
}

const ReviewAnalysisDashboard: React.FC<ReviewAnalysisDashboardProps> = ({
  onCreateNew,
  onViewSession,
}) => {
  const [reviewSessions, setReviewSessions] = useState<ReviewAnalysisItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState<{
    status: string;
    searchTerm: string;
  }>({
    status: '',
    searchTerm: '',
  });
  
  const [statusDropdownOpen, setStatusDropdownOpen] = useState(false);
  
  const statusOptions = [
    { value: '', label: '全部状态' },
    { value: 'pending', label: '待处理' },
    { value: 'processing_setup', label: '设置中' },
    { value: 'streaming_answer', label: '分析中' },
    { value: 'completed', label: '已完成' },
    { value: 'error_setup', label: '设置错误' },
    { value: 'error_stream', label: '分析错误' }
  ];


  const { showNotification } = useNotification();
  const { currentSubject } = useSubject();
  
  // 从现有数据中提取可用的科目选项
  const availableSubjects = Array.from(new Set(reviewSessions.map(session => session.subject).filter(Boolean)));

  useEffect(() => {
    loadReviewSessions();
  }, []);

  // 处理点击外部关闭下拉框
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (statusDropdownOpen) {
        const target = event.target as Node;
        const dropdown = document.querySelector('.status-dropdown-container');
        if (dropdown && !dropdown.contains(target)) {
          setStatusDropdownOpen(false);
        }
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [statusDropdownOpen]);



  const loadReviewSessions = async () => {
    try {
      setLoading(true);
      // 从数据库加载回顾分析会话列表（复用错题分析的模式）
      const sessions = await TauriAPI.getReviewAnalyses();
      setReviewSessions(sessions);
      console.log('✅ 从数据库加载回顾分析列表成功:', sessions.length);
      // 验证错题数量是否正确加载
      sessions.forEach(s => {
        console.log(`📊 回顾分析 "${s.name}" 包含 ${s.mistake_ids?.length || 0} 个错题`);
      });
    } catch (error) {
      console.error('加载回顾分析会话失败:', error);
      showNotification('error', '加载回顾分析会话失败');
      setReviewSessions([]);
    } finally {
      setLoading(false);
    }
  };

  const handleDeleteSession = async (sessionId: string) => {
    if (!confirm('确定要删除这个回顾分析会话吗？')) {
      return;
    }

    try {
      console.log('🗑️ 开始删除回顾分析:', sessionId);
      const deleted = await TauriAPI.deleteReviewAnalysis(sessionId);
      
      if (deleted) {
        showNotification('success', '回顾分析删除成功');
        // 重新加载列表以反映删除操作
        await loadReviewSessions();
      } else {
        showNotification('warning', '回顾分析不存在或已被删除');
        // 即使删除失败，也刷新列表以确保数据一致性
        await loadReviewSessions();
      }
    } catch (error) {
      console.error('❌ 删除回顾分析失败:', error);
      showNotification('error', `删除回顾分析失败: ${error.message || error}`);
    }
  };

  const filteredSessions = reviewSessions.filter(session => {
    const matchesSubject = !currentSubject || currentSubject === '全部' || session.subject === currentSubject;
    const matchesStatus = !filter.status || session.status === filter.status;
    const matchesSearch = !filter.searchTerm || 
      session.name.toLowerCase().includes(filter.searchTerm.toLowerCase()) ||
      session.user_question.toLowerCase().includes(filter.searchTerm.toLowerCase()) ||
      session.consolidated_input.toLowerCase().includes(filter.searchTerm.toLowerCase());
    
    return matchesSubject && matchesStatus && matchesSearch;
  });

  const getStatusDisplay = (status: string) => {
    const statusMap: Record<string, { text: string; style: any }> = {
      pending: { text: '待处理', style: { backgroundColor: '#fef3c7', color: '#92400e', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      processing_setup: { text: '设置中', style: { backgroundColor: '#dbeafe', color: '#1e40af', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      awaiting_stream_start: { text: '等待分析', style: { backgroundColor: '#dbeafe', color: '#1e40af', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      streaming_answer: { text: '分析中', style: { backgroundColor: '#dbeafe', color: '#1e40af', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      completed: { text: '已完成', style: { backgroundColor: '#dcfce7', color: '#166534', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      error_setup: { text: '设置错误', style: { backgroundColor: '#fee2e2', color: '#991b1b', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
      error_stream: { text: '分析错误', style: { backgroundColor: '#fee2e2', color: '#991b1b', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } },
    };
    
    return statusMap[status] || { text: status, style: { backgroundColor: '#f3f4f6', color: '#374151', padding: '4px 8px', borderRadius: '12px', fontSize: '12px', fontWeight: '500' } };
  };

  if (loading) {
    return (
      <div className="review-loading">
        <div className="review-loading-spinner"></div>
        <span className="review-loading-text">加载中...</span>
      </div>
    );
  }

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
            <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
              <circle cx="12" cy="12" r="10" />
              <circle cx="12" cy="12" r="6" />
              <circle cx="12" cy="12" r="2" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>统一回顾分析</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            对多个错题进行统一深度分析，发现学习模式，制定改进计划
          </p>
          <div style={{ marginTop: '24px' }}>
            <button
              onClick={onCreateNew}
              style={{
                background: '#667eea',
                border: '1px solid #667eea',
                color: 'white',
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
                e.currentTarget.style.background = '#5a67d8';
                e.currentTarget.style.transform = 'translateY(-2px)';
                e.currentTarget.style.boxShadow = '0 8px 25px rgba(102, 126, 234, 0.3)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = '#667eea';
                e.currentTarget.style.transform = 'translateY(0)';
                e.currentTarget.style.boxShadow = 'none';
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              创建新的回顾分析
            </button>
          </div>
        </div>
      </div>

      {/* 筛选器 - 重新设计 */}
      <div style={{ 
        background: 'white', 
        margin: '0 24px 24px 24px',
        borderRadius: '16px',
        padding: '24px',
        boxShadow: '0 4px 20px rgba(0, 0, 0, 0.05)',
        border: '1px solid #f1f5f9'
      }}>
        <div style={{ 
          display: 'grid', 
          gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
          gap: '20px',
          alignItems: 'end'
        }}>
          {/* 科目筛选现在由全局状态控制 */}
          
          <div>
            <label style={{ 
              display: 'block',
              fontSize: '14px',
              fontWeight: '600',
              color: '#374151',
              marginBottom: '8px'
            }}>状态筛选</label>
            {/* 自定义状态下拉框 - 保持原生样式外观 + 自定义下拉列表 */}
            <div className="status-dropdown-container" style={{ position: 'relative' }}>
              <div 
                style={{
                  width: '100%',
                  padding: '12px 16px',
                  border: '2px solid #e2e8f0',
                  borderRadius: '12px',
                  fontSize: '14px',
                  background: 'white',
                  cursor: 'pointer',
                  transition: 'all 0.2s ease',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  minHeight: '20px'
                }}
                onClick={() => setStatusDropdownOpen(!statusDropdownOpen)}
                onMouseOver={(e) => {
                  if (!statusDropdownOpen) {
                    e.currentTarget.style.borderColor = '#667eea';
                  }
                }}
                onMouseOut={(e) => {
                  if (!statusDropdownOpen) {
                    e.currentTarget.style.borderColor = '#e2e8f0';
                  }
                }}
              >
                <span style={{ color: '#374151' }}>
                  {filter.status ? statusOptions.find(opt => opt.value === filter.status)?.label : '全部状态'}
                </span>
                <span style={{
                  transform: statusDropdownOpen ? 'rotate(180deg)' : 'rotate(0deg)',
                  transition: 'transform 0.2s ease',
                  color: '#6b7280',
                  fontSize: '12px'
                }}>▼</span>
              </div>
              {statusDropdownOpen && (
                <div style={{
                  position: 'absolute',
                  top: '100%',
                  left: 0,
                  right: 0,
                  backgroundColor: '#fff',
                  borderRadius: '12px',
                  border: '1px solid #e0e0e0',
                  marginTop: '8px',
                  boxShadow: '0 6px 16px rgba(0,0,0,0.12)',
                  zIndex: 9999,
                  overflow: 'hidden',
                  maxHeight: '300px',
                  overflowY: 'auto'
                }}>
                  {statusOptions.map((option, index) => (
                    <div
                      key={option.value}
                      style={{
                        padding: '12px 16px',
                        cursor: 'pointer',
                        color: '#333',
                        fontSize: '14px',
                        borderBottom: index < statusOptions.length - 1 ? '1px solid #f0f0f0' : 'none',
                        backgroundColor: filter.status === option.value ? '#f0f7ff' : 'transparent',
                        transition: 'all 0.2s ease',
                        minHeight: '44px',
                        display: 'flex',
                        alignItems: 'center'
                      }}
                      onClick={() => {
                        setFilter(prev => ({ ...prev, status: option.value }));
                        setStatusDropdownOpen(false);
                      }}
                      onMouseOver={(e) => {
                        if (filter.status !== option.value) {
                          e.currentTarget.style.backgroundColor = '#f7f7f7';
                        }
                      }}
                      onMouseOut={(e) => {
                        if (filter.status !== option.value) {
                          e.currentTarget.style.backgroundColor = 'transparent';
                        }
                      }}
                    >
                      {option.label}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
          
          <div style={{ gridColumn: 'span 2' }}>
            <label style={{ 
              display: 'block',
              fontSize: '14px',
              fontWeight: '600',
              color: '#374151',
              marginBottom: '8px'
            }}>搜索</label>
            <div style={{ position: 'relative' }}>
              <svg style={{
                position: 'absolute',
                left: '16px',
                top: '50%',
                transform: 'translateY(-50%)',
                width: '18px',
                height: '18px',
                color: '#9ca3af'
              }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
              </svg>
              <input
                type="text"
                placeholder="搜索回顾分析名称或问题..."
                value={filter.searchTerm}
                onChange={(e) => setFilter(prev => ({ ...prev, searchTerm: e.target.value }))}
                style={{
                  width: '100%',
                  padding: '12px 16px 12px 48px',
                  border: '2px solid #e2e8f0',
                  borderRadius: '12px',
                  fontSize: '14px',
                  transition: 'all 0.2s ease'
                }}
                onFocus={(e) => e.target.style.borderColor = '#667eea'}
                onBlur={(e) => e.target.style.borderColor = '#e2e8f0'}
              />
            </div>
          </div>
        </div>
      </div>

      {/* 会话列表 */}
      {filteredSessions.length === 0 ? (
        <div style={{
          textAlign: 'center',
          padding: '80px 24px',
          background: 'white',
          margin: '0 24px',
          borderRadius: '16px',
          boxShadow: '0 4px 20px rgba(0, 0, 0, 0.05)',
          border: '1px solid #f1f5f9'
        }}>
          <div style={{
            width: '80px',
            height: '80px',
            background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
            borderRadius: '50%',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            margin: '0 auto 24px',
            opacity: 0.8
          }}>
            <svg style={{ width: '40px', height: '40px', color: 'white' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </div>
          <h3 style={{ 
            fontSize: '20px', 
            fontWeight: '600', 
            color: '#1F2937', 
            marginBottom: '12px',
            margin: 0 
          }}>
            {reviewSessions.length === 0 ? '开始您的第一个回顾分析' : '未找到匹配的分析'}
          </h3>
          <p style={{ 
            fontSize: '16px', 
            color: '#6B7280', 
            lineHeight: '1.6',
            marginBottom: '32px',
            maxWidth: '400px',
            margin: '0 auto 32px'
          }}>
            {reviewSessions.length === 0 
              ? '创建回顾分析来深度分析多个错题，发现学习模式，制定改进计划' 
              : '尝试调整筛选条件或搜索关键词'}
          </p>
          {reviewSessions.length === 0 && (
            <button
              onClick={onCreateNew}
              style={{
                background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
                color: 'white',
                border: 'none',
                padding: '16px 32px',
                borderRadius: '12px',
                fontSize: '16px',
                fontWeight: '600',
                cursor: 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                gap: '8px',
                transition: 'all 0.3s ease',
                boxShadow: '0 4px 15px rgba(102, 126, 234, 0.3)'
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.transform = 'translateY(-2px)';
                e.currentTarget.style.boxShadow = '0 8px 25px rgba(102, 126, 234, 0.4)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.transform = 'translateY(0)';
                e.currentTarget.style.boxShadow = '0 4px 15px rgba(102, 126, 234, 0.3)';
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              立即创建回顾分析
            </button>
          )}
        </div>
      ) : (
        <div style={{ 
          display: 'grid', 
          gap: '20px', 
          margin: '0 24px 24px 24px'
        }}>
          {filteredSessions.map((session) => {
            const statusInfo = getStatusDisplay(session.status);
            return (
              <div 
                key={session.id} 
                style={{ 
                  background: 'white', 
                  borderRadius: '16px', 
                  boxShadow: '0 4px 20px rgba(0, 0, 0, 0.05)', 
                  border: '1px solid #f1f5f9',
                  transition: 'all 0.3s ease',
                  cursor: 'pointer',
                  position: 'relative',
                  overflow: 'hidden'
                }}
                onMouseOver={(e) => {
                  e.currentTarget.style.transform = 'translateY(-2px)';
                  e.currentTarget.style.boxShadow = '0 8px 30px rgba(0, 0, 0, 0.12)';
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.transform = 'translateY(0)';
                  e.currentTarget.style.boxShadow = '0 4px 20px rgba(0, 0, 0, 0.05)';
                }}
              >
                {/* 顶部装饰条 */}
                <div style={{
                  height: '4px',
                  background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
                  width: '100%'
                }}></div>
                
                <div style={{ padding: '28px' }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
                    <div style={{ flex: 1 }}>
                      {/* 标题和状态 */}
                      <div style={{ display: 'flex', alignItems: 'center', marginBottom: '16px' }}>
                        <div style={{
                          width: '12px',
                          height: '12px',
                          background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
                          borderRadius: '50%',
                          marginRight: '12px'
                        }}></div>
                        <h3 style={{ 
                          fontSize: '20px', 
                          fontWeight: '700', 
                          color: '#1F2937', 
                          margin: 0, 
                          marginRight: '16px',
                          lineHeight: '1.2'
                        }}>
                          {session.name}
                        </h3>
                        <span style={statusInfo.style}>
                          {statusInfo.text}
                        </span>
                      </div>
                      
                      {/* 元信息 */}
                      <div style={{ 
                        display: 'grid', 
                        gridTemplateColumns: 'repeat(auto-fit, minmax(140px, 1fr))', 
                        gap: '20px', 
                        marginBottom: '20px',
                        padding: '16px',
                        background: '#f8fafc',
                        borderRadius: '12px',
                        border: '1px solid #f1f5f9'
                      }}>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <svg style={{ width: '16px', height: '16px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.746 0 3.332.477 4.5 1.253v13C19.832 18.477 18.246 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                          </svg>
                          <div>
                            <div style={{ fontSize: '12px', color: '#6B7280', fontWeight: '500' }}>科目</div>
                            <div style={{ fontSize: '14px', color: '#1F2937', fontWeight: '600' }}>{session.subject}</div>
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <svg style={{ width: '16px', height: '16px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                          </svg>
                          <div>
                            <div style={{ fontSize: '12px', color: '#6B7280', fontWeight: '500' }}>错题数量</div>
                            <div style={{ fontSize: '14px', color: '#1F2937', fontWeight: '600' }}>{session.mistake_ids.length} 个</div>
                          </div>
                        </div>
                        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <svg style={{ width: '16px', height: '16px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
                          </svg>
                          <div>
                            <div style={{ fontSize: '12px', color: '#6B7280', fontWeight: '500' }}>创建时间</div>
                            <div style={{ fontSize: '14px', color: '#1F2937', fontWeight: '600' }}>{new Date(session.created_at).toLocaleDateString()}</div>
                          </div>
                        </div>
                      </div>
                      
                      {/* 描述 */}
                      <div style={{
                        padding: '16px',
                        background: 'rgba(102, 126, 234, 0.05)',
                        borderRadius: '12px',
                        borderLeft: '4px solid #667eea'
                      }}>
                        <div style={{ fontSize: '12px', color: '#667eea', fontWeight: '600', marginBottom: '4px' }}>分析指引</div>
                        <p style={{ 
                          color: '#374151', 
                          lineHeight: '1.6',
                          fontSize: '14px',
                          margin: 0,
                          overflow: 'hidden',
                          display: '-webkit-box',
                          WebkitLineClamp: 2,
                          WebkitBoxOrient: 'vertical'
                        }}>
                          {session.user_question}
                        </p>
                      </div>
                    </div>
                    
                    {/* 操作按钮 */}
                    <div style={{ 
                      display: 'flex', 
                      flexDirection: 'column',
                      gap: '12px', 
                      marginLeft: '24px',
                      minWidth: '120px'
                    }}>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          onViewSession(session.id);
                        }}
                        style={{
                          background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
                          color: 'white',
                          border: 'none',
                          padding: '12px 20px',
                          borderRadius: '10px',
                          fontSize: '14px',
                          fontWeight: '600',
                          cursor: 'pointer',
                          transition: 'all 0.2s ease',
                          boxShadow: '0 2px 8px rgba(102, 126, 234, 0.2)'
                        }}
                        onMouseOver={(e) => {
                          e.currentTarget.style.transform = 'translateY(-1px)';
                          e.currentTarget.style.boxShadow = '0 4px 15px rgba(102, 126, 234, 0.3)';
                        }}
                        onMouseOut={(e) => {
                          e.currentTarget.style.transform = 'translateY(0)';
                          e.currentTarget.style.boxShadow = '0 2px 8px rgba(102, 126, 234, 0.2)';
                        }}
                      >
                        <svg style={{ width: '14px', height: '14px', marginRight: '6px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M2.458 12C3.732 7.943 7.523 5 12 5c4.478 0 8.268 2.943 9.542 7-1.274 4.057-5.064 7-9.542 7-4.477 0-8.268-2.943-9.542-7z" />
                        </svg>
                        查看详情
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          handleDeleteSession(session.id);
                        }}
                        style={{ 
                          background: '#ffffff',
                          color: '#ef4444',
                          border: '2px solid #fee2e2',
                          padding: '10px 20px',
                          borderRadius: '10px',
                          fontSize: '14px',
                          fontWeight: '600',
                          cursor: 'pointer',
                          transition: 'all 0.2s ease'
                        }}
                        onMouseOver={(e) => {
                          e.currentTarget.style.background = '#fef2f2';
                          e.currentTarget.style.borderColor = '#fecaca';
                          e.currentTarget.style.transform = 'translateY(-1px)';
                        }}
                        onMouseOut={(e) => {
                          e.currentTarget.style.background = '#ffffff';
                          e.currentTarget.style.borderColor = '#fee2e2';
                          e.currentTarget.style.transform = 'translateY(0)';
                        }}
                      >
                        <svg style={{ width: '14px', height: '14px', marginRight: '6px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                        </svg>
                        删除
                      </button>
                    </div>
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
};

export default ReviewAnalysisDashboard;