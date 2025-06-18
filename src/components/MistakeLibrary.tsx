import React, { useState, useEffect, useMemo } from 'react';
import { TauriAPI, MistakeItem } from '../utils/tauriApi';
import { useSubject } from '../contexts/SubjectContext';

interface MistakeLibraryProps {
  onSelectMistake: (mistake: MistakeItem) => void;
  onBack: () => void;
  // 🎯 修复：添加刷新触发器，每次切换到错题库页面时会变化
  refreshTrigger?: number;
}

export const MistakeLibrary: React.FC<MistakeLibraryProps> = ({ onSelectMistake, onBack, refreshTrigger }) => {
  const [mistakes, setMistakes] = useState<MistakeItem[]>([]);
  const [filteredMistakes, setFilteredMistakes] = useState<MistakeItem[]>([]);
  const [selectedType, setSelectedType] = useState('全部');
  const [searchTerm, setSearchTerm] = useState('');
  const [loading, setLoading] = useState(false);
  const [availableTypes, setAvailableTypes] = useState<string[]>([]);
  
  // 使用全局科目状态
  const { currentSubject } = useSubject();
  
  // 从现有错题数据中提取可用科目
  const availableSubjects = useMemo(() => {
    const subjects = Array.from(new Set(mistakes.map(mistake => mistake.subject).filter(Boolean)));
    return subjects;
  }, [mistakes]);

  // 新增：分页状态
  const [currentPage, setCurrentPage] = useState(1);
  const [itemsPerPage, setItemsPerPage] = useState(12); // 每页显示12个卡片
  const [typeDropdownOpen, setTypeDropdownOpen] = useState(false);

  // 🎯 修复：将loadData提取为独立函数，支持手动刷新
  const loadData = async () => {
      setLoading(true);
      try {
        // 加载错题数据
        console.log('🔍 [MistakeLibrary] 开始加载错题数据...');
        const rawMistakes = await TauriAPI.getMistakes();
        console.log('🔍 [MistakeLibrary] 从数据库加载的原始错题数据:', {
          错题总数: rawMistakes.length,
          前3个错题信息: rawMistakes.slice(0, 3).map(m => ({
            id: m.id,
            questionImagesLength: m.question_images?.length || 0,
            questionImages: m.question_images,
            hasQuestionImages: !!m.question_images && m.question_images.length > 0
          }))
        });
        
        // 转换错题数据：为每个错题生成图片URLs
        const mistakesWithUrls = await Promise.all(rawMistakes.map(async (mistake) => {
          try {
            // 转换 question_images (file paths) 为 question_image_urls (URLs)
            console.log(`🖼️ [图片处理] 错题 ${mistake.id} 的图片路径:`, {
              questionImages: mistake.question_images,
              questionImagesLength: mistake.question_images?.length || 0,
              questionImagesType: typeof mistake.question_images
            });
            
            if (!mistake.question_images || mistake.question_images.length === 0) {
              console.log(`⚠️ [图片处理] 错题 ${mistake.id} 没有图片路径`);
              return {
                ...mistake,
                question_image_urls: []
              };
            }
            
            const questionImageUrls = await Promise.all(
              mistake.question_images.map(async (imagePath, index) => {
                try {
                  console.log(`🖼️ [图片处理] 正在处理图片 ${index + 1}/${mistake.question_images.length}: "${imagePath}"`);
                  
                  // 添加超时机制
                  const timeoutPromise = new Promise<never>((_, reject) => {
                    setTimeout(() => reject(new Error('图片加载超时')), 10000); // 10秒超时
                  });
                  
                  const base64Promise = TauriAPI.getImageAsBase64(imagePath);
                  const base64Data = await Promise.race([base64Promise, timeoutPromise]);
                  
                  // 检查返回的数据是否已经是完整的data URL
                  const dataUrl = base64Data.startsWith('data:') ? base64Data : `data:image/jpeg;base64,${base64Data}`;
                  console.log(`✅ [图片处理] 图片 ${index + 1} 处理成功`, {
                    原始数据长度: base64Data.length,
                    是否已是DataURL: base64Data.startsWith('data:'),
                    最终URL长度: dataUrl.length,
                    URL前缀: dataUrl.substring(0, 50)
                  });
                  
                  // 验证生成的data URL
                  if (dataUrl.length < 100) {
                    console.warn(`⚠️ [图片处理] 图片 ${index + 1} 的data URL似乎太短: ${dataUrl.length} 字符`);
                  }
                  
                  return dataUrl;
                } catch (error) {
                  console.error(`❌ [图片处理] 加载图片失败: "${imagePath}"`, {
                    error,
                    errorMessage: error instanceof Error ? error.message : String(error),
                    mistakeId: mistake.id,
                    imageIndex: index,
                    isTimeout: error instanceof Error && error.message === '图片加载超时'
                  });
                  return ''; // 返回空字符串作为fallback
                }
              })
            );
            
            const validUrls = questionImageUrls.filter(url => url !== '');
            console.log(`🖼️ [图片处理] 错题 ${mistake.id} 最终图片URLs:`, {
              总数量: questionImageUrls.length,
              有效数量: validUrls.length,
              失败数量: questionImageUrls.length - validUrls.length,
              validUrlsPreview: validUrls.map((url, i) => `${i+1}: ${url.substring(0, 50)}...`),
              validUrlsActual: validUrls
            });
            
            const filteredUrls = questionImageUrls.filter(url => url !== '');
            const result = {
              ...mistake,
              question_image_urls: filteredUrls // 过滤掉失败的图片
            };
            
            console.log(`🔧 [数据组装] 错题 ${mistake.id} 最终结果:`, {
              有question_image_urls字段: 'question_image_urls' in result,
              question_image_urls长度: result.question_image_urls?.length || 0,
              question_image_urls值: result.question_image_urls
            });
            
            // 调试日志：检查聊天历史数据
            console.log(`🔍 错题 ${mistake.id} 数据结构:`, {
              id: mistake.id,
              chatHistoryLength: mistake.chat_history?.length || 0,
              chatHistoryExists: !!mistake.chat_history,
              chatHistoryType: typeof mistake.chat_history,
              chatHistoryFirst: mistake.chat_history?.[0],
              questionImagesCount: mistake.question_images?.length || 0,
              questionImageUrlsCount: result.question_image_urls?.length || 0
            });
            
            return result;
          } catch (error) {
            console.warn(`处理错题图片失败: ${mistake.id}`, error);
            return {
              ...mistake,
              question_image_urls: [] // 如果所有图片都失败，返回空数组
            };
          }
        }));
        
        const allMistakes = mistakesWithUrls;
        setMistakes(allMistakes);
        setFilteredMistakes(allMistakes);
        
        // 科目选项现在由全局状态管理
        
        // 动态提取可用的错题类型选项
        const types = Array.from(new Set(allMistakes.map(m => m.mistake_type).filter(t => t && t.trim() !== ''))).sort();
        setAvailableTypes(types);
        
        console.log('加载错题库数据:', {
          总数: allMistakes.length,
          科目: Array.from(new Set(allMistakes.map(m => m.subject))).sort(),
          类型: types
        });
      } catch (error) {
        console.error('加载错题失败:', error);
        alert('加载错题失败: ' + error);
      } finally {
        setLoading(false);
      }
    };

  // 🎯 修复：页面切换时自动重新加载数据
  useEffect(() => {
    console.log('🔄 错题库页面加载/刷新，refreshTrigger:', refreshTrigger);
    loadData();
  }, [refreshTrigger]); // 依赖refreshTrigger，每次切换到错题库页面时都会重新加载

  // 应用筛选条件
  useEffect(() => {
    let filtered = mistakes;
    
    // 使用全局科目状态进行筛选
    if (currentSubject && currentSubject !== '全部') {
      filtered = filtered.filter(m => m.subject === currentSubject);
    }
    
    if (selectedType !== '全部') {
      filtered = filtered.filter(m => m.mistake_type === selectedType);
    }
    
    if (searchTerm) {
      filtered = filtered.filter(m => 
        m.user_question.toLowerCase().includes(searchTerm.toLowerCase()) ||
        m.ocr_text.toLowerCase().includes(searchTerm.toLowerCase()) ||
        m.tags.some(tag => tag.toLowerCase().includes(searchTerm.toLowerCase()))
      );
    }
    
    setFilteredMistakes(filtered);
    setCurrentPage(1); // 每次筛选后重置到第一页
    console.log('应用筛选条件:', {
      科目: currentSubject,
      类型: selectedType,
      搜索词: searchTerm,
      筛选结果: filtered.length
    });
  }, [mistakes, currentSubject, selectedType, searchTerm]);

  // 处理点击外部关闭下拉框
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (typeDropdownOpen) {
        const target = event.target as Node;
        const dropdown = document.querySelector('.type-dropdown-container');
        if (dropdown && !dropdown.contains(target)) {
          setTypeDropdownOpen(false);
        }
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [typeDropdownOpen]);

  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString('zh-CN', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit'
    });
  };

  // 分页逻辑
  const paginatedMistakes = useMemo(() => {
    const startIndex = (currentPage - 1) * itemsPerPage;
    const endIndex = startIndex + itemsPerPage;
    return filteredMistakes.slice(startIndex, endIndex);
  }, [filteredMistakes, currentPage, itemsPerPage]);

  const totalPages = Math.ceil(filteredMistakes.length / itemsPerPage);

  const handlePageChange = (newPage: number) => {
    if (newPage >= 1 && newPage <= totalPages) {
      setCurrentPage(newPage);
    }
  };

  return (
    <div style={{
      width: '100%',
      background: '#f8fafc'
    }}>
      {/* 🎯 修复：添加CSS动画支持 */}
      <style>{`
        @keyframes spin {
          from { transform: rotate(0deg); }
          to { transform: rotate(360deg); }
        }
      `}</style>

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
              <path d="M12 7v14" />
              <path d="M3 18a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h5a4 4 0 0 1 4 4 4 4 0 0 1 4-4h5a1 1 0 0 1 1 1v13a1 1 0 0 1-1 1h-6a3 3 0 0 0-3 3 3 3 0 0 0-3-3z" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>错题库</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            管理和回顾您的错题集合，追踪学习进度和薄弱环节
          </p>
        </div>
      </div>

      {/* 筛选器 - 与统一回顾分析一致的样式 */}
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
            }}>类型筛选</label>
            {/* 自定义类型下拉框 - 保持原生样式外观 + 自定义下拉列表 */}
            <div className="type-dropdown-container" style={{ position: 'relative' }}>
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
                onClick={() => setTypeDropdownOpen(!typeDropdownOpen)}
                onMouseOver={(e) => {
                  if (!typeDropdownOpen) {
                    e.currentTarget.style.borderColor = '#667eea';
                  }
                }}
                onMouseOut={(e) => {
                  if (!typeDropdownOpen) {
                    e.currentTarget.style.borderColor = '#e2e8f0';
                  }
                }}
              >
                <span style={{ color: '#374151' }}>
                  {selectedType === '全部' ? '全部类型' : selectedType}
                </span>
                <span style={{
                  transform: typeDropdownOpen ? 'rotate(180deg)' : 'rotate(0deg)',
                  transition: 'transform 0.2s ease',
                  color: '#6b7280',
                  fontSize: '12px'
                }}>▼</span>
              </div>
              {typeDropdownOpen && (
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
                  <div
                    style={{
                      padding: '12px 16px',
                      cursor: 'pointer',
                      color: '#333',
                      fontSize: '14px',
                      borderBottom: availableTypes.length > 0 ? '1px solid #f0f0f0' : 'none',
                      backgroundColor: selectedType === '全部' ? '#f0f7ff' : 'transparent',
                      transition: 'all 0.2s ease',
                      minHeight: '44px',
                      display: 'flex',
                      alignItems: 'center'
                    }}
                    onClick={() => {
                      setSelectedType('全部');
                      setTypeDropdownOpen(false);
                    }}
                    onMouseOver={(e) => {
                      if (selectedType !== '全部') {
                        e.currentTarget.style.backgroundColor = '#f7f7f7';
                      }
                    }}
                    onMouseOut={(e) => {
                      if (selectedType !== '全部') {
                        e.currentTarget.style.backgroundColor = 'transparent';
                      }
                    }}
                  >
                    全部类型
                  </div>
                  {availableTypes.map((type, index) => (
                    <div
                      key={type}
                      style={{
                        padding: '12px 16px',
                        cursor: 'pointer',
                        color: '#333',
                        fontSize: '14px',
                        borderBottom: index < availableTypes.length - 1 ? '1px solid #f0f0f0' : 'none',
                        backgroundColor: selectedType === type ? '#f0f7ff' : 'transparent',
                        transition: 'all 0.2s ease',
                        minHeight: '44px',
                        display: 'flex',
                        alignItems: 'center'
                      }}
                      onClick={() => {
                        setSelectedType(type);
                        setTypeDropdownOpen(false);
                      }}
                      onMouseOver={(e) => {
                        if (selectedType !== type) {
                          e.currentTarget.style.backgroundColor = '#f7f7f7';
                        }
                      }}
                      onMouseOut={(e) => {
                        if (selectedType !== type) {
                          e.currentTarget.style.backgroundColor = 'transparent';
                        }
                      }}
                    >
                      {type}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>

          <div>
            <label style={{ 
              display: 'block',
              fontSize: '14px',
              fontWeight: '600',
              color: '#374151',
              marginBottom: '8px'
            }}>搜索</label>
            <input
              type="text"
              placeholder="搜索题目、标签或内容..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              style={{
                width: '100%',
                padding: '12px 16px',
                border: '2px solid #e2e8f0',
                borderRadius: '12px',
                fontSize: '14px',
                background: 'white',
                transition: 'all 0.2s ease'
              }}
              onFocus={(e) => e.target.style.borderColor = '#667eea'}
              onBlur={(e) => e.target.style.borderColor = '#e2e8f0'}
            />
          </div>
        </div>
      </div>

      <div className="mistake-library" style={{ padding: '0 24px 24px 24px', background: 'transparent' }}>

      <div className="library-content">
        {loading ? (
          <div className="loading">加载中...</div>
        ) : filteredMistakes.length === 0 ? (
          <div className="empty-state">
            <p>暂无错题记录</p>
            <p>开始分析题目来建立您的错题库吧！</p>
          </div>
        ) : (
          <>
            <div className="mistakes-grid">
              {paginatedMistakes.map((mistake) => (
                <div 
                  key={mistake.id} 
                  className="mistake-card"
                  onClick={() => onSelectMistake(mistake)}
                >
                  <div className="mistake-header">
                    <span className="subject-badge">{mistake.subject}</span>
                    <span className="date">{formatDate(mistake.created_at)}</span>
                  </div>
                  
                  <div className="mistake-content">
                    <h4>{mistake.user_question}</h4>
                    <p className="ocr-preview">
                      {mistake.ocr_text.length > 100 
                        ? mistake.ocr_text.substring(0, 100) + '...'
                        : mistake.ocr_text
                      }
                    </p>
                  </div>
                  
                  <div className="mistake-tags">
                    {mistake.tags.slice(0, 3).map((tag, index) => (
                      <span key={index} className="tag">{tag}</span>
                    ))}
                    {mistake.tags.length > 3 && (
                      <span className="tag-more">+{mistake.tags.length - 3}</span>
                    )}
                  </div>
                  
                  <div className="mistake-footer">
                    <span className="type">{mistake.mistake_type}</span>
                    <span className="status">{mistake.status === 'completed' ? '已完成' : '分析中'}</span>
                  </div>
                </div>
              ))}
            </div>
            {totalPages > 1 && (
              <div className="pagination-controls" style={{ marginTop: '24px', display: 'flex', justifyContent: 'center', alignItems: 'center', gap: '8px' }}>
                <button 
                  onClick={() => handlePageChange(currentPage - 1)}
                  disabled={currentPage === 1}
                  style={{ padding: '8px 16px', borderRadius: '8px', border: '1px solid #ddd', cursor: 'pointer', background: currentPage === 1 ? '#f5f5f5' : 'white' }}
                >
                  上一页
                </button>
                <span style={{ fontSize: '14px', color: '#333' }}>
                  第 {currentPage} 页 / 共 {totalPages} 页
                </span>
                <button 
                  onClick={() => handlePageChange(currentPage + 1)}
                  disabled={currentPage === totalPages}
                  style={{ padding: '8px 16px', borderRadius: '8px', border: '1px solid #ddd', cursor: 'pointer', background: currentPage === totalPages ? '#f5f5f5' : 'white' }}
                >
                  下一页
                </button>
              </div>
            )}
          </>
        )}
        </div>
      </div>
    </div>
  );
}; 