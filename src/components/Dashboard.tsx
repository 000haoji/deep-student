import React, { useState, useEffect } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { BarChart3, Settings, AlertTriangle, FileText, Search, BookOpen, Tag, PieChart } from 'lucide-react';

interface DashboardProps {
  onBack: () => void;
}

interface Statistics {
  totalMistakes: number;
  totalReviews: number;
  subjectStats: Record<string, number>;
  typeStats: Record<string, number>;
  tagStats: Record<string, number>;
  recentMistakes: any[];
}

export const Dashboard: React.FC<DashboardProps> = ({ onBack }) => {
  const [stats, setStats] = useState<Statistics | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [debugMode, setDebugMode] = useState(false);

  useEffect(() => {
    const loadStatistics = async () => {
      setLoading(true);
      setError(null);
      try {
        console.log('开始加载统计数据...');
        
        // 先尝试加载真实数据
        const statistics = await TauriAPI.getStatistics();
        console.log('统计数据加载成功:', statistics);
        
        // 转换后端数据格式到前端格式
        const formattedStats: Statistics = {
          totalMistakes: statistics.total_mistakes || 0,
          totalReviews: statistics.total_reviews || 0,
          subjectStats: statistics.subject_stats || {},
          typeStats: statistics.type_stats || {},
          tagStats: statistics.tag_stats || {},
          recentMistakes: statistics.recent_mistakes || []
        };
        
        setStats(formattedStats);
      } catch (error) {
        console.error('加载统计数据失败:', error);
        setError(`加载统计数据失败: ${error}`);
      } finally {
        setLoading(false);
      }
    };

    loadStatistics();
  }, []);

  // 简化的渲染逻辑，确保总是有内容显示
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
              <path d="M3 3v16a2 2 0 0 0 2 2h16" />
              <path d="M18 17V9" />
              <path d="M13 17V5" />
              <path d="M8 17v-3" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>数据统计</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            全面了解学习数据和进度分析，洞察知识掌握情况
          </p>
          {debugMode && (
            <div style={{ marginTop: '24px' }}>
              <button
                onClick={() => setDebugMode(!debugMode)}
                style={{
                  background: '#ef4444',
                  border: '1px solid #ef4444',
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
                  e.currentTarget.style.background = '#dc2626';
                  e.currentTarget.style.transform = 'translateY(-2px)';
                  e.currentTarget.style.boxShadow = '0 8px 25px rgba(239, 68, 68, 0.3)';
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.background = '#ef4444';
                  e.currentTarget.style.transform = 'translateY(0)';
                  e.currentTarget.style.boxShadow = 'none';
                }}
              >
                <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 100 4m0-4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 100 4m0-4v2m0-6V4" />
                </svg>
                关闭调试
              </button>
            </div>
          )}
          {!debugMode && (
            <div style={{ marginTop: '24px' }}>
              <button
                onClick={() => setDebugMode(!debugMode)}
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
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6V4m0 2a2 2 0 100 4m0-4a2 2 0 110 4m-6 8a2 2 0 100-4m0 4a2 2 0 100 4m0-4v2m0-6V4m6 6v10m6-2a2 2 0 100-4m0 4a2 2 0 100 4m0-4v2m0-6V4" />
                </svg>
                开启调试
              </button>
            </div>
          )}
        </div>
      </div>

      <div className="dashboard-content" style={{ padding: '24px', background: 'transparent' }}>
        {debugMode && (
          <div style={{ 
            backgroundColor: 'rgba(255, 255, 255, 0.7)', 
            padding: '1rem', 
            borderRadius: '12px', 
            marginBottom: '2rem',
            border: '1px solid rgba(255, 255, 255, 0.2)',
            backdropFilter: 'blur(10px)'
          }}>
            <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Settings size={20} />
              调试信息
            </h3>
            <p><strong>加载状态:</strong> {loading ? '加载中' : '已完成'}</p>
            <p><strong>错误状态:</strong> {error || '无错误'}</p>
            <p><strong>数据状态:</strong> {stats ? '有数据' : '无数据'}</p>
            <p><strong>组件渲染:</strong> 正常</p>
            <div style={{ marginTop: '1rem' }}>
              <button 
                onClick={async () => {
                  try {
                    console.log('手动测试API调用...');
                    const result = await TauriAPI.getStatistics();
                    console.log('手动测试结果:', result);
                    alert('API调用成功，请查看控制台日志');
                  } catch (err) {
                    console.error('手动测试失败:', err);
                    alert(`API调用失败: ${err}`);
                  }
                }}
                style={{
                  padding: '8px 16px',
                  backgroundColor: '#007bff',
                  color: 'white',
                  border: 'none',
                  borderRadius: '4px',
                  cursor: 'pointer',
                  marginRight: '8px'
                }}
              >
                测试API
              </button>
              <button 
                onClick={() => {
                  setError(null);
                  setLoading(true);
                  window.location.reload();
                }}
                style={{
                  padding: '8px 16px',
                  backgroundColor: '#28a745',
                  color: 'white',
                  border: 'none',
                  borderRadius: '4px',
                  cursor: 'pointer'
                }}
              >
                重新加载
              </button>
            </div>
          </div>
        )}

        {loading ? (
          <div style={{ 
            textAlign: 'center', 
            padding: '4rem',
            fontSize: '18px',
            color: '#666'
          }}>
            <div style={{ fontSize: '48px', marginBottom: '16px', display: 'flex', justifyContent: 'center' }}>
              <BarChart3 size={48} color="#667eea" />
            </div>
            <div>加载统计数据中...</div>
          </div>
        ) : error && !stats ? (
          <div style={{ 
            textAlign: 'center', 
            padding: '4rem',
            color: '#dc3545',
            backgroundColor: '#f8d7da',
            border: '1px solid #f5c6cb',
            borderRadius: '8px'
          }}>
            <div style={{ fontSize: '48px', marginBottom: '16px', display: 'flex', justifyContent: 'center' }}>
              <AlertTriangle size={48} color="#dc3545" />
            </div>
            <div style={{ fontSize: '18px', marginBottom: '8px' }}>加载统计数据失败</div>
            <div style={{ fontSize: '14px', color: '#721c24' }}>{error}</div>
          </div>
        ) : (
          <div>
            {/* 总览卡片 */}
            <div style={{ 
              display: 'grid', 
              gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))', 
              gap: '1rem', 
              marginBottom: '2rem' 
            }}>
              <div style={{ 
                backgroundColor: 'white', 
                padding: '1.5rem', 
                borderRadius: '8px', 
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
                display: 'flex',
                alignItems: 'center',
                gap: '1rem'
              }}>
                <div style={{ fontSize: '2rem', display: 'flex', alignItems: 'center' }}>
                  <FileText size={32} color="#667eea" />
                </div>
                <div>
                  <div style={{ fontSize: '2rem', fontWeight: 'bold', color: '#333' }}>
                    {stats?.totalMistakes || 0}
                  </div>
                  <div style={{ color: '#666' }}>总错题数</div>
                </div>
              </div>

              <div style={{ 
                backgroundColor: 'white', 
                padding: '1.5rem', 
                borderRadius: '8px', 
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
                display: 'flex',
                alignItems: 'center',
                gap: '1rem'
              }}>
                <div style={{ fontSize: '2rem', display: 'flex', alignItems: 'center' }}>
                  <Search size={32} color="#28a745" />
                </div>
                <div>
                  <div style={{ fontSize: '2rem', fontWeight: 'bold', color: '#333' }}>
                    {stats?.totalReviews || 0}
                  </div>
                  <div style={{ color: '#666' }}>回顾分析次数</div>
                </div>
              </div>

              <div style={{ 
                backgroundColor: 'white', 
                padding: '1.5rem', 
                borderRadius: '8px', 
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
                display: 'flex',
                alignItems: 'center',
                gap: '1rem'
              }}>
                <div style={{ fontSize: '2rem', display: 'flex', alignItems: 'center' }}>
                  <BookOpen size={32} color="#ffc107" />
                </div>
                <div>
                  <div style={{ fontSize: '2rem', fontWeight: 'bold', color: '#333' }}>
                    {Object.keys(stats?.subjectStats || {}).length}
                  </div>
                  <div style={{ color: '#666' }}>涉及科目</div>
                </div>
              </div>

              <div style={{ 
                backgroundColor: 'white', 
                padding: '1.5rem', 
                borderRadius: '8px', 
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
                display: 'flex',
                alignItems: 'center',
                gap: '1rem'
              }}>
                <div style={{ fontSize: '2rem', display: 'flex', alignItems: 'center' }}>
                  <Tag size={32} color="#6f42c1" />
                </div>
                <div>
                  <div style={{ fontSize: '2rem', fontWeight: 'bold', color: '#333' }}>
                    {Object.keys(stats?.tagStats || {}).length}
                  </div>
                  <div style={{ color: '#666' }}>知识点标签</div>
                </div>
              </div>
            </div>

            {/* 详细统计 */}
            {stats && (stats.totalMistakes > 0 || Object.keys(stats.subjectStats).length > 0) ? (
              <div style={{ display: 'grid', gap: '2rem' }}>
                {/* 科目分布 */}
                {Object.keys(stats.subjectStats).length > 0 && (
                  <div style={{ backgroundColor: 'white', padding: '1.5rem', borderRadius: '8px', boxShadow: '0 2px 4px rgba(0,0,0,0.1)' }}>
                    <h3 style={{ marginBottom: '1rem', display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <BookOpen size={20} />
                      科目分布
                    </h3>
                    <div>
                      {Object.entries(stats.subjectStats).map(([subject, count]) => (
                        <div key={subject} style={{ display: 'flex', alignItems: 'center', marginBottom: '0.5rem' }}>
                          <div style={{ minWidth: '80px' }}>{subject}</div>
                          <div style={{ flex: 1, backgroundColor: '#f0f0f0', height: '20px', borderRadius: '10px', margin: '0 1rem' }}>
                            <div style={{ 
                              width: `${stats.totalMistakes > 0 ? (count / stats.totalMistakes) * 100 : 0}%`,
                              height: '100%',
                              backgroundColor: '#007bff',
                              borderRadius: '10px'
                            }} />
                          </div>
                          <div style={{ minWidth: '30px', textAlign: 'right' }}>{count}</div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}

                {/* 题目类型分布 */}
                {Object.keys(stats.typeStats).length > 0 && (
                  <div style={{ backgroundColor: 'white', padding: '1.5rem', borderRadius: '8px', boxShadow: '0 2px 4px rgba(0,0,0,0.1)' }}>
                    <h3 style={{ marginBottom: '1rem', display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <PieChart size={20} />
                      题目类型分布
                    </h3>
                    <div>
                      {Object.entries(stats.typeStats).map(([type, count]) => (
                        <div key={type} style={{ display: 'flex', alignItems: 'center', marginBottom: '0.5rem' }}>
                          <div style={{ minWidth: '120px' }}>{type}</div>
                          <div style={{ flex: 1, backgroundColor: '#f0f0f0', height: '20px', borderRadius: '10px', margin: '0 1rem' }}>
                            <div style={{ 
                              width: `${stats.totalMistakes > 0 ? (count / stats.totalMistakes) * 100 : 0}%`,
                              height: '100%',
                              backgroundColor: '#28a745',
                              borderRadius: '10px'
                            }} />
                          </div>
                          <div style={{ minWidth: '30px', textAlign: 'right' }}>{count}</div>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <div style={{ 
                textAlign: 'center', 
                padding: '4rem',
                backgroundColor: 'white',
                borderRadius: '8px',
                boxShadow: '0 2px 4px rgba(0,0,0,0.1)'
              }}>
                <div style={{ fontSize: '48px', marginBottom: '16px', display: 'flex', justifyContent: 'center' }}>
                  <BarChart3 size={48} color="#667eea" />
                </div>
                <h3 style={{ marginBottom: '8px', color: '#333' }}>暂无数据</h3>
                <p style={{ color: '#666' }}>开始分析错题后，这里将显示详细的统计信息</p>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

 