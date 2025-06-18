import React, { useState, useEffect } from 'react';
import { ReviewSessionTask, MistakeConsolidationData, ConsolidatedMistakeData } from '../types/index';
import type { MistakeItem } from '../utils/tauriApi';
import { useNotification } from '../hooks/useNotification';
import { TauriAPI } from '../utils/tauriApi';
import { useSubject } from '../contexts/SubjectContext';

interface CreateReviewAnalysisViewProps {
  onCancel: () => void;
  onCreateSuccess: (sessionId: string) => void;
}

const CreateReviewAnalysisView: React.FC<CreateReviewAnalysisViewProps> = ({
  onCancel,
  onCreateSuccess,
}) => {
  const [step, setStep] = useState<'setup' | 'select_mistakes' | 'configure_prompt' | 'creating'>('setup');
  const [formData, setFormData] = useState({
    name: '',
    subject: '',
    overallPrompt: '',
  });
  const [availableMistakes, setAvailableMistakes] = useState<MistakeItem[]>([]);
  const [selectedMistakeIds, setSelectedMistakeIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchTerm, setSearchTerm] = useState('');
  const [subjectFilter, setSubjectFilter] = useState('');

  const { showNotification } = useNotification();
  const { currentSubject, getEnabledSubjects } = useSubject();

  // 🎯 同步全局科目状态到本地表单（优先级更高，避免循环）
  useEffect(() => {
    if (currentSubject && currentSubject !== formData.subject) {
      console.log('🔄 [回顾分析] 同步全局科目到表单:', currentSubject);
      setFormData(prev => ({ ...prev, subject: currentSubject }));
    }
  }, [currentSubject]);

  // 移除双向同步，避免循环更新
  // 用户只能通过标题栏科目选择器更改科目，表单中的科目选择框变为只读显示

  useEffect(() => {
    if (step === 'select_mistakes') {
      loadAvailableMistakes();
    }
  }, [step, formData.subject]);

  const loadAvailableMistakes = async () => {
    try {
      setLoading(true);
      const mistakes = await TauriAPI.getMistakes();
      
      const filteredMistakes = formData.subject 
        ? mistakes.filter(mistake => mistake.subject === formData.subject)
        : mistakes;
      
      setAvailableMistakes(filteredMistakes);
    } catch (error) {
      console.error('加载错题列表失败:', error);
      showNotification('error', '加载错题列表失败');
    } finally {
      setLoading(false);
    }
  };

  const getSubjects = () => {
    const subjects = new Set(availableMistakes.map(mistake => mistake.subject));
    return Array.from(subjects);
  };

  const filteredMistakes = availableMistakes.filter(mistake => {
    const matchesSearch = !searchTerm || 
      mistake.user_question.toLowerCase().includes(searchTerm.toLowerCase()) ||
      mistake.ocr_text.toLowerCase().includes(searchTerm.toLowerCase()) ||
      mistake.tags.some(tag => tag.toLowerCase().includes(searchTerm.toLowerCase()));
    
    const matchesSubject = !subjectFilter || mistake.subject === subjectFilter;
    
    return matchesSearch && matchesSubject;
  });

  const handleNextStep = () => {
    if (step === 'setup') {
      if (!formData.name.trim() || !formData.subject.trim()) {
        showNotification('warning', '请填写回顾分析名称和科目');
        return;
      }
      setStep('select_mistakes');
    } else if (step === 'select_mistakes') {
      if (selectedMistakeIds.length === 0) {
        showNotification('warning', '请至少选择一个错题');
        return;
      }
      setStep('configure_prompt');
    } else if (step === 'configure_prompt') {
      if (!formData.overallPrompt.trim()) {
        showNotification('warning', '请输入分析指引');
        return;
      }
      handleCreateReviewAnalysis();
    }
  };

  const handlePrevStep = () => {
    if (step === 'select_mistakes') {
      setStep('setup');
    } else if (step === 'configure_prompt') {
      setStep('select_mistakes');
    }
  };

  const consolidateMistakeData = (selectedMistakes: MistakeItem[]): ConsolidatedMistakeData => {
    if (!selectedMistakes || selectedMistakes.length === 0) {
      throw new Error('没有选中的错题数据');
    }

    const consolidationData: MistakeConsolidationData[] = selectedMistakes.map(mistake => {
      if (!mistake) {
        throw new Error('错题数据为空');
      }
      
      return {
        mistakeId: mistake.id || '',
        ocr_text: mistake.ocr_text || '',
        user_question: mistake.user_question || '',
        chat_history: (mistake.chat_history || []).map(msg => ({
          role: (msg?.role as 'user' | 'assistant' | 'system') || 'user',
          content: msg?.content || '',
          timestamp: msg?.timestamp || new Date().toISOString(),
          thinking_content: msg?.thinking_content,
        })),
      };
    });

    let consolidatedText = '';
    
    consolidationData.forEach((data, index) => {
      const mistake = selectedMistakes[index];
      consolidatedText += `--- 错题 ${index + 1} (ID: ${data.mistakeId}) ---\\n`;
      consolidatedText += `题目内容:\\n${data.ocr_text}\\n\\n`;
      consolidatedText += `我的原始问题:\\n${data.user_question}\\n\\n`;
      
      // 🎯 关键改进：优先使用AI生成的结构化总结
      if (mistake.mistake_summary && mistake.user_error_analysis) {
        consolidatedText += `题目解析总结:\\n${mistake.mistake_summary}\\n\\n`;
        consolidatedText += `错误分析总结:\\n${mistake.user_error_analysis}\\n\\n`;
      } else if (data.chat_history && data.chat_history.length > 0) {
        // 降级方案：如果没有总结，使用聊天记录（但会提示需要生成总结）
        consolidatedText += `⚠️ 注意：此错题缺少AI总结，建议先生成总结以提高回顾分析质量\\n`;
        consolidatedText += `历史交流（原始记录）:\\n`;
        // 只包含关键的几条对话，避免过长
        const keyMessages = data.chat_history.slice(-3); // 只取最后3条对话
        keyMessages.forEach(message => {
          const roleDisplay = message.role === 'user' ? '用户' : '助手';
          const contentStr = typeof message.content === 'string' ? message.content : JSON.stringify(message.content);
          const truncatedContent = contentStr.length > 200 
            ? contentStr.substring(0, 200) + '...' 
            : contentStr;
          consolidatedText += `${roleDisplay}: ${truncatedContent}\\n`;
        });
        consolidatedText += '\\n';
      } else {
        consolidatedText += `⚠️ 注意：此错题缺少分析记录和总结\\n\\n`;
      }
      
      consolidatedText += '\\n';
    });

    return {
      selectedMistakes: consolidationData,
      consolidatedText,
      userOverallPrompt: formData.overallPrompt || '',
    };
  };

  const handleCreateReviewAnalysis = async () => {
    try {
      setStep('creating');
      setLoading(true);

      // 验证数据完整性
      if (!formData.name || !formData.subject || !formData.overallPrompt) {
        throw new Error('缺少必要的表单数据');
      }

      if (selectedMistakeIds.length === 0) {
        throw new Error('没有选择任何错题');
      }

      // 关键修复：根据ID获取每个错题的完整详情，以确保包含总结字段
      const selectedMistakesPromises = selectedMistakeIds.map(id =>
        TauriAPI.getMistakeDetails(id)
      );
      const selectedMistakesWithDetails = await Promise.all(selectedMistakesPromises);

      const selectedMistakes = selectedMistakesWithDetails.filter(
        (mistake): mistake is MistakeItem => mistake !== null
      );

      if (selectedMistakes.length !== selectedMistakeIds.length) {
        showNotification('warning', '部分选中的错题无法加载详情，可能已被删除。');
      }

      if (selectedMistakes.length === 0) {
        throw new Error('选中的错题数据不存在或无法加载');
      }

      const consolidatedData = consolidateMistakeData(selectedMistakes);

      // 调用后端API创建回顾分析会话（复用错题分析的数据库存储模式）
      const response = await TauriAPI.startConsolidatedReviewAnalysis({
        subject: formData.subject,
        consolidatedInput: consolidatedData.consolidatedText,
        overallPrompt: formData.overallPrompt,
        enableChainOfThought: true,
        mistakeIds: selectedMistakeIds, // 🔧 修复：传递选中的错题ID列表
      });
      
      if (!response || !response.review_session_id) {
        throw new Error('API响应无效，未获得会话ID');
      }
      
      const reviewSessionId = response.review_session_id;
      console.log('✅ 回顾分析会话创建成功 (数据库模式):', {
        reviewSessionId: reviewSessionId,
        name: formData.name,
        subject: formData.subject,
        mistakeCount: selectedMistakeIds.length
      });

      showNotification('success', '回顾分析创建成功');
      
      // 短暂显示成功状态
      setLoading(false);
      
      // 延迟一下再跳转，让用户看到成功状态
      setTimeout(() => {
        console.log('🔄 正在跳转到会话页面，reviewSessionId:', reviewSessionId);
        // 使用后端返回的review_session_id作为前端的sessionId，完全复用错题分析模式
        onCreateSuccess(reviewSessionId);
      }, 1500);
      
    } catch (error) {
      console.error('创建回顾分析失败:', error);
      const errorMessage = error instanceof Error ? error.message : '创建回顾分析失败';
      showNotification('error', errorMessage);
      setStep('configure_prompt');
      setLoading(false);
    }
  };

  const renderSetupStep = () => (
    <div>
      <div style={{
        background: 'linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%)',
        padding: '24px',
        borderRadius: '16px',
        marginBottom: '32px',
        border: '1px solid rgba(102, 126, 234, 0.1)'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: '12px' }}>
          <svg style={{ width: '24px', height: '24px', color: '#667eea', marginRight: '12px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          <h3 style={{ fontSize: '20px', fontWeight: '700', color: '#1f2937', margin: 0 }}>基本设置</h3>
        </div>
        <p style={{ fontSize: '14px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
          设置回顾分析的基本信息，包括名称和科目等
        </p>
      </div>
      
      <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>
        <div>
          <label style={{
            display: 'block',
            fontSize: '14px',
            fontWeight: '600',
            color: '#374151',
            marginBottom: '8px'
          }}>
            回顾分析名称 *
          </label>
          <input
            type="text"
            value={formData.name}
            onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
            placeholder="例如：期中数学复习 - 函数部分"
            style={{
              width: '100%',
              padding: '16px 20px',
              border: '2px solid #e2e8f0',
              borderRadius: '12px',
              fontSize: '16px',
              background: 'white',
              transition: 'all 0.2s ease',
              outline: 'none'
            }}
            onFocus={(e) => {
              e.target.style.borderColor = '#667eea';
              e.target.style.boxShadow = '0 0 0 4px rgba(102, 126, 234, 0.1)';
            }}
            onBlur={(e) => {
              e.target.style.borderColor = '#e2e8f0';
              e.target.style.boxShadow = 'none';
            }}
          />
          <p style={{ fontSize: '12px', color: '#9ca3af', marginTop: '4px' }}>
            请输入一个有意义的名称，方便后续管理
          </p>
        </div>
        
        <div>
          <label style={{
            display: 'block',
            fontSize: '14px',
            fontWeight: '600',
            color: '#374151',
            marginBottom: '8px'
          }}>
            科目 *
          </label>
          <div style={{ position: 'relative' }}>
            <div
              style={{
                width: '100%',
                padding: '16px 20px',
                border: '2px solid #e2e8f0',
                borderRadius: '12px',
                fontSize: '16px',
                background: '#f8fafc',
                color: '#374151',
                cursor: 'not-allowed',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between'
              }}
            >
              <span>{formData.subject || '请在标题栏选择科目'}</span>
              <svg style={{
                width: '20px',
                height: '20px',
                color: '#9ca3af'
              }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z" />
              </svg>
            </div>
          </div>
          <p style={{ fontSize: '12px', color: '#9ca3af', marginTop: '4px' }}>
            科目由标题栏下拉框控制，请在标题栏中选择所需科目
          </p>
        </div>
      </div>
    </div>
  );

  const renderSelectMistakesStep = () => (
    <div>
      <div style={{
        background: 'linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%)',
        padding: '24px',
        borderRadius: '16px',
        marginBottom: '32px',
        border: '1px solid rgba(102, 126, 234, 0.1)'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: '12px' }}>
          <svg style={{ width: '24px', height: '24px', color: '#667eea', marginRight: '12px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <h3 style={{ fontSize: '20px', fontWeight: '700', color: '#1f2937', margin: 0 }}>
            选择错题 
            <span style={{ color: '#667eea', fontWeight: '600', marginLeft: '8px' }}>({selectedMistakeIds.length} / {filteredMistakes.length})</span>
          </h3>
        </div>
        <p style={{ fontSize: '14px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
          从下方列表中选择需要进行统一分析的错题
        </p>
      </div>
      
      {/* 筛选器 */}
      <div style={{
        display: 'grid',
        gridTemplateColumns: '2fr 1fr',
        gap: '16px',
        marginBottom: '24px'
      }}>
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
            placeholder="搜索错题内容、问题或标签..."
            value={searchTerm}
            onChange={(e) => setSearchTerm(e.target.value)}
            style={{
              width: '100%',
              padding: '12px 16px 12px 48px',
              border: '2px solid #e2e8f0',
              borderRadius: '12px',
              fontSize: '14px',
              transition: 'all 0.2s ease',
              outline: 'none'
            }}
            onFocus={(e) => {
              e.target.style.borderColor = '#667eea';
              e.target.style.boxShadow = '0 0 0 4px rgba(102, 126, 234, 0.1)';
            }}
            onBlur={(e) => {
              e.target.style.borderColor = '#e2e8f0';
              e.target.style.boxShadow = 'none';
            }}
          />
        </div>
        <div style={{ position: 'relative' }}>
          <select
            value={subjectFilter}
            onChange={(e) => setSubjectFilter(e.target.value)}
            style={{
              width: '100%',
              padding: '12px 16px',
              border: '2px solid #e2e8f0',
              borderRadius: '12px',
              fontSize: '14px',
              background: 'white',
              cursor: 'pointer',
              transition: 'all 0.2s ease',
              outline: 'none',
              appearance: 'none'
            }}
            onFocus={(e) => {
              e.target.style.borderColor = '#667eea';
              e.target.style.boxShadow = '0 0 0 4px rgba(102, 126, 234, 0.1)';
            }}
            onBlur={(e) => {
              e.target.style.borderColor = '#e2e8f0';
              e.target.style.boxShadow = 'none';
            }}
          >
            <option value="">全部科目</option>
            {getSubjects().map(subject => (
              <option key={subject} value={subject}>{subject}</option>
            ))}
          </select>
          <svg style={{
            position: 'absolute',
            right: '16px',
            top: '50%',
            transform: 'translateY(-50%)',
            width: '16px',
            height: '16px',
            color: '#9ca3af',
            pointerEvents: 'none'
          }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </div>
      </div>

      {loading ? (
        <div style={{
          display: 'flex',
          justifyContent: 'center',
          alignItems: 'center',
          height: '200px',
          gap: '12px'
        }}>
          <div style={{
            width: '32px',
            height: '32px',
            border: '3px solid #f1f5f9',
            borderTop: '3px solid #667eea',
            borderRadius: '50%',
            animation: 'spin 1s linear infinite'
          }}></div>
          <span style={{ color: '#6b7280', fontSize: '16px', fontWeight: '500' }}>加载中...</span>
        </div>
      ) : filteredMistakes.length === 0 ? (
        <div style={{
          textAlign: 'center',
          padding: '80px 24px',
          background: '#f8fafc',
          borderRadius: '16px',
          border: '2px dashed #d1d5db'
        }}>
          <div style={{
            width: '64px',
            height: '64px',
            background: '#e5e7eb',
            borderRadius: '50%',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            margin: '0 auto 16px'
          }}>
            <svg style={{ width: '32px', height: '32px', color: '#9ca3af' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </div>
          <h4 style={{ fontSize: '18px', fontWeight: '600', color: '#374151', marginBottom: '8px' }}>
            {availableMistakes.length === 0 ? '暂无错题数据' : '没有找到符合条件的错题'}
          </h4>
          <p style={{ color: '#6b7280', fontSize: '14px' }}>
            {availableMistakes.length === 0 ? '请先在错题库中添加一些错题' : '试试调整搜索条件或科目筛选'}
          </p>
        </div>
      ) : (
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fill, minmax(350px, 1fr))',
          gap: '20px',
          maxHeight: '500px',
          overflow: 'auto',
          padding: '4px'
        }}>
          {filteredMistakes.map((mistake) => {
            const isSelected = selectedMistakeIds.includes(mistake.id);
            return (
              <div
                key={mistake.id}
                onClick={() => {
                  if (isSelected) {
                    setSelectedMistakeIds(prev => prev.filter(id => id !== mistake.id));
                  } else {
                    setSelectedMistakeIds(prev => [...prev, mistake.id]);
                  }
                }}
                style={{
                  background: isSelected ? 'linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%)' : 'white',
                  border: isSelected ? '2px solid #667eea' : '2px solid #f1f5f9',
                  borderRadius: '16px',
                  padding: '20px',
                  cursor: 'pointer',
                  transition: 'all 0.3s ease',
                  position: 'relative',
                  overflow: 'hidden'
                }}
                onMouseOver={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.borderColor = '#d1d5db';
                    e.currentTarget.style.transform = 'translateY(-2px)';
                    e.currentTarget.style.boxShadow = '0 8px 25px rgba(0, 0, 0, 0.1)';
                  }
                }}
                onMouseOut={(e) => {
                  if (!isSelected) {
                    e.currentTarget.style.borderColor = '#f1f5f9';
                    e.currentTarget.style.transform = 'translateY(0)';
                    e.currentTarget.style.boxShadow = 'none';
                  }
                }}
              >
                {/* 选中指示器 */}
                {isSelected && (
                  <div style={{
                    position: 'absolute',
                    top: '0',
                    left: '0',
                    right: '0',
                    height: '4px',
                    background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)'
                  }}></div>
                )}
                
                {/* 头部 */}
                <div style={{ display: 'flex', alignItems: 'center', marginBottom: '16px', gap: '12px' }}>
                  <div style={{
                    width: '20px',
                    height: '20px',
                    borderRadius: '4px',
                    border: isSelected ? '2px solid #667eea' : '2px solid #d1d5db',
                    background: isSelected ? '#667eea' : 'white',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    transition: 'all 0.2s ease'
                  }}>
                    {isSelected && (
                      <svg style={{ width: '12px', height: '12px', color: 'white' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
                      </svg>
                    )}
                  </div>
                  <span style={{
                    fontSize: '12px',
                    color: '#667eea',
                    fontWeight: '600',
                    background: 'rgba(102, 126, 234, 0.1)',
                    padding: '4px 8px',
                    borderRadius: '8px'
                  }}>
                    {mistake.subject}
                  </span>
                  <span style={{ fontSize: '12px', color: '#9ca3af', marginLeft: 'auto' }}>
                    {new Date(mistake.created_at).toLocaleDateString()}
                  </span>
                </div>
                
                {/* 问题内容 */}
                <p style={{
                  fontWeight: '600',
                  color: '#1f2937',
                  marginBottom: '12px',
                  lineHeight: '1.5',
                  fontSize: '14px',
                  overflow: 'hidden',
                  display: '-webkit-box',
                  WebkitLineClamp: 2,
                  WebkitBoxOrient: 'vertical'
                }}>
                  {mistake.user_question}
                </p>
                
                {/* OCR内容 */}
                <p style={{
                  color: '#6b7280',
                  fontSize: '13px',
                  lineHeight: '1.4',
                  marginBottom: '16px',
                  overflow: 'hidden',
                  display: '-webkit-box',
                  WebkitLineClamp: 3,
                  WebkitBoxOrient: 'vertical'
                }}>
                  {mistake.ocr_text}
                </p>
                
                {/* 标签 */}
                <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
                  {mistake.tags.slice(0, 3).map((tag, index) => (
                    <span key={index} style={{
                      background: '#f3f4f6',
                      color: '#374151',
                      padding: '4px 8px',
                      borderRadius: '8px',
                      fontSize: '11px',
                      fontWeight: '500'
                    }}>
                      {tag}
                    </span>
                  ))}
                  {mistake.tags.length > 3 && (
                    <span style={{ color: '#9ca3af', fontSize: '11px', padding: '4px 0' }}>+{mistake.tags.length - 3}</span>
                  )}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );

  const renderConfigurePromptStep = () => (
    <div>
      <div style={{
        background: 'linear-gradient(135deg, rgba(102, 126, 234, 0.1) 0%, rgba(118, 75, 162, 0.1) 100%)',
        padding: '24px',
        borderRadius: '16px',
        marginBottom: '32px',
        border: '1px solid rgba(102, 126, 234, 0.1)'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: '12px' }}>
          <svg style={{ width: '24px', height: '24px', color: '#667eea', marginRight: '12px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
          </svg>
          <h3 style={{ fontSize: '20px', fontWeight: '700', color: '#1f2937', margin: 0 }}>配置分析指引</h3>
        </div>
        <p style={{ fontSize: '14px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
          设置 AI 如何分析选中的错题，包括分析角度和重点关注的方面
        </p>
      </div>
      
      {/* 分析指引输入 */}
      <div style={{ marginBottom: '32px' }}>
        <label style={{
          display: 'block',
          fontSize: '14px',
          fontWeight: '600',
          color: '#374151',
          marginBottom: '8px'
        }}>
          总体分析指引 *
        </label>
        <textarea
          value={formData.overallPrompt}
          onChange={(e) => setFormData(prev => ({ ...prev, overallPrompt: e.target.value }))}
          placeholder="例如：请总结这些二次函数错题的常见错误类型，并提供针对性的学习建议。请重点分析我在概念理解和计算方法上的薄弱环节。"
          rows={6}
          style={{
            width: '100%',
            padding: '16px 20px',
            border: '2px solid #e2e8f0',
            borderRadius: '12px',
            fontSize: '14px',
            lineHeight: '1.6',
            background: 'white',
            transition: 'all 0.2s ease',
            outline: 'none',
            resize: 'vertical',
            minHeight: '120px'
          }}
          onFocus={(e) => {
            e.target.style.borderColor = '#667eea';
            e.target.style.boxShadow = '0 0 0 4px rgba(102, 126, 234, 0.1)';
          }}
          onBlur={(e) => {
            e.target.style.borderColor = '#e2e8f0';
            e.target.style.boxShadow = 'none';
          }}
        />
        <p style={{ fontSize: '12px', color: '#9ca3af', marginTop: '8px', lineHeight: '1.4' }}>
          请描述您希望AI如何分析这些错题，比如总结共同问题、分析薄弱环节、提供学习建议等。
        </p>
      </div>

      {/* 快速模板 */}
      <div style={{ marginBottom: '32px' }}>
        <label style={{
          display: 'block',
          fontSize: '14px',
          fontWeight: '600',
          color: '#374151',
          marginBottom: '12px'
        }}>
          快速模板
        </label>
        <div style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(280px, 1fr))',
          gap: '12px'
        }}>
          {[
            "请总结这些错题的共同问题和改进建议",
            "分析我在这些知识点上的薄弱环节",
            "比较这些错题的解题方法和思路差异",
            "针对这些错题制定复习计划",
          ].map((template, index) => (
            <button
              key={index}
              onClick={() => setFormData(prev => ({ ...prev, overallPrompt: template }))}
              style={{
                textAlign: 'left',
                padding: '16px',
                fontSize: '14px',
                border: '2px solid #f1f5f9',
                borderRadius: '12px',
                background: 'white',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                lineHeight: '1.4'
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.borderColor = '#667eea';
                e.currentTarget.style.background = 'rgba(102, 126, 234, 0.05)';
                e.currentTarget.style.transform = 'translateY(-1px)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.borderColor = '#f1f5f9';
                e.currentTarget.style.background = 'white';
                e.currentTarget.style.transform = 'translateY(0)';
              }}
            >
              {template}
            </button>
          ))}
        </div>
      </div>

      {/* 选中错题预览 */}
      <div style={{
        background: '#f8fafc',
        border: '2px solid #f1f5f9',
        borderRadius: '16px',
        padding: '24px'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', marginBottom: '16px' }}>
          <svg style={{ width: '20px', height: '20px', color: '#667eea', marginRight: '8px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
          </svg>
          <h4 style={{ fontSize: '16px', fontWeight: '600', color: '#374151', margin: 0 }}>
            已选中的错题 
            <span style={{ color: '#667eea', marginLeft: '8px' }}>({selectedMistakeIds.length} 个)</span>
          </h4>
        </div>
        <div style={{
          maxHeight: '200px',
          overflow: 'auto',
          background: 'white',
          borderRadius: '12px',
          padding: '16px'
        }}>
          {selectedMistakeIds.length === 0 ? (
            <p style={{ color: '#9ca3af', fontSize: '14px', textAlign: 'center', margin: 0 }}>
              尚未选择任何错题
            </p>
          ) : (
            selectedMistakeIds.map((id, index) => {
              const mistake = availableMistakes.find(m => m.id === id);
              return mistake ? (
                <div key={id} style={{
                  display: 'flex',
                  alignItems: 'flex-start',
                  padding: '12px 0',
                  borderBottom: index < selectedMistakeIds.length - 1 ? '1px solid #f1f5f9' : 'none'
                }}>
                  <span style={{
                    background: '#667eea',
                    color: 'white',
                    width: '24px',
                    height: '24px',
                    borderRadius: '50%',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    fontSize: '12px',
                    fontWeight: '600',
                    marginRight: '12px',
                    flexShrink: 0
                  }}>
                    {index + 1}
                  </span>
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: '14px', color: '#1f2937', lineHeight: '1.4', marginBottom: '4px' }}>
                      {mistake.user_question.length > 60 
                        ? mistake.user_question.substring(0, 60) + '...' 
                        : mistake.user_question}
                    </div>
                    <div style={{
                      fontSize: '12px',
                      color: '#667eea',
                      background: 'rgba(102, 126, 234, 0.1)',
                      padding: '2px 6px',
                      borderRadius: '6px',
                      display: 'inline-block'
                    }}>
                      {mistake.subject}
                    </div>
                  </div>
                </div>
              ) : null;
            })
          )}
        </div>
      </div>
    </div>
  );

  const renderCreatingStep = () => (
    <div style={{ 
      textAlign: 'center', 
      padding: '80px 24px',
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      position: 'relative'
    }}>
      {/* 加载指示器 */}
      {loading && (
        <div style={{
          width: '80px',
          height: '80px',
          background: 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
          borderRadius: '50%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          marginBottom: '24px',
          position: 'relative'
        }}>
          <div style={{
            width: '40px',
            height: '40px',
            border: '3px solid rgba(255, 255, 255, 0.3)',
            borderTop: '3px solid white',
            borderRadius: '50%',
            animation: 'spin 1s linear infinite'
          }}></div>
        </div>
      )}
      
      {/* 成功指示器 */}
      {!loading && (
        <div style={{
          width: '80px',
          height: '80px',
          background: 'linear-gradient(135deg, #10b981 0%, #059669 100%)',
          borderRadius: '50%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          marginBottom: '24px',
          animation: 'fadeIn 0.5s ease-in'
        }}>
          <svg style={{ width: '40px', height: '40px', color: 'white' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
          </svg>
        </div>
      )}
      
      <h3 style={{ 
        fontSize: '24px', 
        fontWeight: '700', 
        color: loading ? '#1f2937' : '#10b981', 
        marginBottom: '12px',
        margin: '0 0 12px 0',
        transition: 'color 0.5s ease'
      }}>
        {loading ? '正在创建回顾分析...' : '创建成功！'}
      </h3>
      <p style={{ 
        color: '#6b7280', 
        fontSize: '16px',
        lineHeight: '1.5',
        maxWidth: '400px',
        margin: 0,
        transition: 'all 0.5s ease'
      }}>
        {loading 
          ? '正在整合错题数据并设置分析环境，请稍候...' 
          : '回顾分析已成功创建，正在跳转到分析页面...'}
      </p>
      
      <style>
        {`
          @keyframes spin {
            0% { transform: rotate(0deg); }
            100% { transform: rotate(360deg); }
          }
          @keyframes fadeIn {
            0% { opacity: 0; transform: scale(0.8); }
            100% { opacity: 1; transform: scale(1); }
          }
        `}
      </style>
    </div>
  );

  return (
    <div style={{
      width: '100%',
      height: '100%',
      overflow: 'auto',
      background: '#f8fafc',
      display: 'flex',
      flexDirection: 'column'
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
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>创建回顾分析</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            配置多错题统一分析，发现学习模式，制定改进计划
          </p>
        </div>
      </div>

      {/* 步骤指示器 - 重新设计 */}
      <div style={{
        background: 'white',
        margin: '24px 24px 0 24px',
        borderRadius: '16px',
        padding: '24px',
        boxShadow: '0 4px 20px rgba(0, 0, 0, 0.05)',
        border: '1px solid #f1f5f9'
      }}>
        <div style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          position: 'relative'
        }}>
          {/* 连接线 */}
          <div style={{
            position: 'absolute',
            top: '50%',
            left: '60px',
            right: '60px',
            height: '2px',
            background: '#e2e8f0',
            zIndex: 1
          }}></div>
          
          {[
            { key: 'setup', label: '基本设置', icon: 'M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z M15 12a3 3 0 11-6 0 3 3 0 016 0z' },
            { key: 'select_mistakes', label: '选择错题', icon: 'M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z' },
            { key: 'configure_prompt', label: '配置指引', icon: 'M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z' },
            { key: 'creating', label: '创建中', icon: 'M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z' },
          ].map((stepInfo, index) => {
            const currentStepIndex = ['setup', 'select_mistakes', 'configure_prompt', 'creating'].indexOf(step);
            const isActive = step === stepInfo.key;
            const isCompleted = currentStepIndex > index;
            const isPending = currentStepIndex < index;
            
            return (
              <div key={stepInfo.key} style={{
                display: 'flex',
                flexDirection: 'column',
                alignItems: 'center',
                position: 'relative',
                zIndex: 2
              }}>
                <div style={{
                  width: '60px',
                  height: '60px',
                  borderRadius: '50%',
                  background: isActive 
                    ? 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)'
                    : isCompleted 
                      ? 'linear-gradient(135deg, #10b981 0%, #059669 100%)'
                      : '#f1f5f9',
                  color: isActive || isCompleted ? 'white' : '#9ca3af',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  marginBottom: '12px',
                  transition: 'all 0.3s ease',
                  boxShadow: isActive 
                    ? '0 8px 25px rgba(102, 126, 234, 0.3)'
                    : isCompleted 
                      ? '0 4px 15px rgba(16, 185, 129, 0.2)'
                      : 'none'
                }}>
                  {isCompleted ? (
                    <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
                    </svg>
                  ) : (
                    <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d={stepInfo.icon} />
                    </svg>
                  )}
                </div>
                <span style={{
                  fontSize: '14px',
                  fontWeight: '600',
                  color: isActive ? '#667eea' : isCompleted ? '#10b981' : '#6b7280',
                  textAlign: 'center'
                }}>
                  {stepInfo.label}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* 表单内容区域 - 重新设计 */}
      <div style={{
        background: 'white',
        margin: '24px',
        borderRadius: '16px',
        boxShadow: '0 4px 20px rgba(0, 0, 0, 0.05)',
        border: '1px solid #f1f5f9',
        flex: 1,
        display: 'flex',
        flexDirection: 'column'
      }}>
        <div style={{
          padding: '32px',
          flex: 1,
          overflow: 'auto'
        }}>
          {step === 'setup' && renderSetupStep()}
          {step === 'select_mistakes' && renderSelectMistakesStep()}
          {step === 'configure_prompt' && renderConfigurePromptStep()}
          {step === 'creating' && renderCreatingStep()}
        </div>

        {/* 底部操作区域 - 重新设计 */}
        {step !== 'creating' && (
          <div style={{
            padding: '24px 32px',
            borderTop: '1px solid #f1f5f9',
            background: '#fafbfc',
            borderRadius: '0 0 16px 16px',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center'
          }}>
            <div>
              {step !== 'setup' && (
                <button
                  onClick={handlePrevStep}
                  style={{
                    background: 'white',
                    color: '#6b7280',
                    border: '2px solid #e5e7eb',
                    padding: '12px 24px',
                    borderRadius: '12px',
                    fontSize: '14px',
                    fontWeight: '600',
                    cursor: 'pointer',
                    display: 'inline-flex',
                    alignItems: 'center',
                    gap: '8px',
                    transition: 'all 0.2s ease'
                  }}
                  onMouseOver={(e) => {
                    e.currentTarget.style.background = '#f9fafb';
                    e.currentTarget.style.borderColor = '#d1d5db';
                  }}
                  onMouseOut={(e) => {
                    e.currentTarget.style.background = 'white';
                    e.currentTarget.style.borderColor = '#e5e7eb';
                  }}
                >
                  <svg style={{ width: '16px', height: '16px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 19l-7-7 7-7" />
                  </svg>
                  上一步
                </button>
              )}
            </div>
            
            <div style={{ display: 'flex', gap: '12px' }}>
              <button
                onClick={onCancel}
                style={{
                  background: 'white',
                  color: '#6b7280',
                  border: '2px solid #e5e7eb',
                  padding: '12px 24px',
                  borderRadius: '12px',
                  fontSize: '14px',
                  fontWeight: '600',
                  cursor: 'pointer',
                  transition: 'all 0.2s ease'
                }}
                onMouseOver={(e) => {
                  e.currentTarget.style.background = '#f9fafb';
                  e.currentTarget.style.borderColor = '#d1d5db';
                }}
                onMouseOut={(e) => {
                  e.currentTarget.style.background = 'white';
                  e.currentTarget.style.borderColor = '#e5e7eb';
                }}
              >
                取消
              </button>
              <button
                onClick={handleNextStep}
                disabled={loading}
                style={{
                  background: loading 
                    ? '#d1d5db' 
                    : 'linear-gradient(135deg, #667eea 0%, #764ba2 100%)',
                  color: 'white',
                  border: 'none',
                  padding: '12px 32px',
                  borderRadius: '12px',
                  fontSize: '14px',
                  fontWeight: '600',
                  cursor: loading ? 'not-allowed' : 'pointer',
                  display: 'inline-flex',
                  alignItems: 'center',
                  gap: '8px',
                  transition: 'all 0.3s ease',
                  boxShadow: loading ? 'none' : '0 4px 15px rgba(102, 126, 234, 0.3)'
                }}
                onMouseOver={(e) => {
                  if (!loading) {
                    e.currentTarget.style.transform = 'translateY(-2px)';
                    e.currentTarget.style.boxShadow = '0 8px 25px rgba(102, 126, 234, 0.4)';
                  }
                }}
                onMouseOut={(e) => {
                  if (!loading) {
                    e.currentTarget.style.transform = 'translateY(0)';
                    e.currentTarget.style.boxShadow = '0 4px 15px rgba(102, 126, 234, 0.3)';
                  }
                }}
              >
                {step === 'configure_prompt' ? (
                  <>
                    <svg style={{ width: '16px', height: '16px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 10V3L4 14h7v7l9-11h-7z" />
                    </svg>
                    创建分析
                  </>
                ) : (
                  <>
                    下一步
                    <svg style={{ width: '16px', height: '16px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                    </svg>
                  </>
                )}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};

export default CreateReviewAnalysisView;
