import React, { useState, useEffect, useCallback } from 'react';
import { SubjectConfig } from './SubjectConfig';
import { 
  Bot, 
  FlaskConical, 
  Target, 
  Settings as SettingsIcon, 
  Plus, 
  TestTube, 
  Edit, 
  Trash2, 
  X, 
  Check, 
  AlertTriangle, 
  Save, 
  Undo2, 
  Zap, 
  CheckCircle, 
  XCircle, 
  Book, 
  Box, 
  Cpu, 
  RefreshCcw, 
  HardDrive, 
  Atom, 
  FileText, 
  ScrollText, 
  File, 
  FileWarning, 
  BookOpen, 
  BookText, 
  StickyNote, 
  Library, 
  SquareStack,
  Image,
  Brain,
  Palette,
  Database,
  PartyPopper,
  Construction
} from 'lucide-react';

// Tauri 2.x API导入
import { invoke as tauriInvoke } from '@tauri-apps/api/core';

// Tauri类型声明
declare global {
  interface Window {
    __TAURI_INTERNALS__?: any;
  }
}

// 检查是否在Tauri环境中
const isTauri = typeof window !== 'undefined' && window.__TAURI_INTERNALS__;
const invoke = isTauri ? tauriInvoke : null;

// API配置接口
interface ApiConfig {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isMultimodal: boolean;
  isReasoning: boolean;  // 新增：是否为推理模型
  enabled: boolean;
  modelAdapter: string;  // 新增：模型适配器类型
}

// 系统配置接口
interface SystemConfig {
  apiConfigs: ApiConfig[];
  model1ConfigId: string;  // 第一模型（必须是多模态）
  model2ConfigId: string;  // 第二模型（可以是任意类型）
  reviewAnalysisModelConfigId: string;  // 回顾分析模型（可以是任意类型）
  ankiCardModelConfigId: string;  // ANKI制卡模型（可以是任意类型）
  embeddingModelConfigId: string;  // 嵌入模型（RAG用）
  rerankerModelConfigId: string;   // 重排序模型（RAG用）
  autoSave: boolean;
  theme: string;
  // RAG设置
  ragEnabled: boolean;
  ragTopK: number;
  // 开发功能设置
  batchAnalysisEnabled: boolean;
  ankiConnectEnabled: boolean;
  geminiAdapterTestEnabled: boolean; // 新增：Gemini适配器测试功能开关
  imageOcclusionEnabled: boolean; // 新增：图片遮罩卡功能开关
  summary_model_config_id: string; // 新增：总结模型配置ID
}

interface SettingsProps {
  onBack: () => void;
}

export const Settings: React.FC<SettingsProps> = ({ onBack }) => {
  const [config, setConfig] = useState<SystemConfig>({
    apiConfigs: [],
    model1ConfigId: '',
    model2ConfigId: '',
    reviewAnalysisModelConfigId: '',
    ankiCardModelConfigId: '',
    embeddingModelConfigId: '',
    rerankerModelConfigId: '',
    autoSave: true,
    theme: 'light',
    ragEnabled: false,
    ragTopK: 5,
    batchAnalysisEnabled: false,
    ankiConnectEnabled: true,
    geminiAdapterTestEnabled: false,
    imageOcclusionEnabled: false, // 默认关闭
    summary_model_config_id: '' 
  });
  const [saving, setSaving] = useState(false);
  const [loading, setLoading] = useState(true);
  const [testingApi, setTestingApi] = useState<string | null>(null);
  const [message, setMessage] = useState<{ type: 'success' | 'error', text: string } | null>(null);
  const [activeTab, setActiveTab] = useState('apis');
  const [editingApi, setEditingApi] = useState<ApiConfig | null>(null);

  // 处理返回按钮，确保在返回前保存配置
  const handleBack = async () => {
    if (!loading) {
      await handleSave(true); // 静默保存
    }
    onBack();
  };

  const showMessage = (type: 'success' | 'error', text: string) => {
    setMessage({ type, text });
    setTimeout(() => setMessage(null), 3000);
  };

  const loadConfig = async () => {
    setLoading(true);
    try {
      if (invoke) {
        // 使用新的专用API配置管理命令
        const [apiConfigs, modelAssignments, autoSave, theme, ragEnabled, ragTopK, batchAnalysisEnabled, ankiConnectEnabled, geminiAdapterTestEnabled, imageOcclusionEnabled] = await Promise.all([
          invoke('get_api_configurations').catch(() => []) as Promise<ApiConfig[]>,
          invoke('get_model_assignments').catch(() => ({ 
            model1_config_id: null, 
            model2_config_id: null, 
            review_analysis_model_config_id: null, 
            anki_card_model_config_id: null,
            embedding_model_config_id: null,
            reranker_model_config_id: null,
            summary_model_config_id: null // 新增
          })) as Promise<{ 
            model1_config_id: string | null, 
            model2_config_id: string | null, 
            review_analysis_model_config_id: string | null, 
            anki_card_model_config_id: string | null,
            embedding_model_config_id: string | null,
            reranker_model_config_id: string | null,
            summary_model_config_id: string | null // 新增
          }>,
          invoke('get_setting', { key: 'auto_save' }).catch(() => 'true') as Promise<string>,
          invoke('get_setting', { key: 'theme' }).catch(() => 'light') as Promise<string>,
          invoke('get_setting', { key: 'rag_enabled' }).catch(() => 'false') as Promise<string>,
          invoke('get_setting', { key: 'rag_top_k' }).catch(() => '5') as Promise<string>,
          invoke('get_setting', { key: 'batch_analysis_enabled' }).catch(() => 'false') as Promise<string>,
          invoke('get_setting', { key: 'anki_connect_enabled' }).catch(() => 'true') as Promise<string>,
          invoke('get_setting', { key: 'gemini_adapter_test_enabled' }).catch(() => 'false') as Promise<string>,
          invoke('get_setting', { key: 'image_occlusion_enabled' }).catch(() => 'false') as Promise<string>,
        ]);

        const newConfig = {
          apiConfigs: apiConfigs || [],
          model1ConfigId: modelAssignments?.model1_config_id || '',
          model2ConfigId: modelAssignments?.model2_config_id || '',
          reviewAnalysisModelConfigId: modelAssignments?.review_analysis_model_config_id || '',
          ankiCardModelConfigId: modelAssignments?.anki_card_model_config_id || '',
          embeddingModelConfigId: modelAssignments?.embedding_model_config_id || '',
          rerankerModelConfigId: modelAssignments?.reranker_model_config_id || '',
          summary_model_config_id: modelAssignments?.summary_model_config_id || '', // 新增
          autoSave: (autoSave || 'true') === 'true',
          theme: theme || 'light',
          ragEnabled: (ragEnabled || 'false') === 'true',
          ragTopK: parseInt(ragTopK || '5', 10),
          batchAnalysisEnabled: (batchAnalysisEnabled || 'false') === 'true',
          ankiConnectEnabled: (ankiConnectEnabled || 'true') === 'true',
          geminiAdapterTestEnabled: (geminiAdapterTestEnabled || 'false') === 'true',
          imageOcclusionEnabled: (imageOcclusionEnabled || 'false') === 'true'
        };
        
        console.log('加载的配置:', {
          apiConfigs: newConfig.apiConfigs.length,
          model1ConfigId: newConfig.model1ConfigId,
          model2ConfigId: newConfig.model2ConfigId,
          reviewAnalysisModelConfigId: newConfig.reviewAnalysisModelConfigId,
          modelAssignments
        });
        
        setConfig(newConfig);
      } else {
        // 浏览器环境
        const savedConfig = localStorage.getItem('ai-mistake-manager-config');
        if (savedConfig) {
          setConfig(JSON.parse(savedConfig));
        }
      }
    } catch (error) {
      console.error('加载配置失败:', error);
      showMessage('error', '加载配置失败: ' + error);
    } finally {
      setLoading(false);
    }
  };

  const handleSave = useCallback(async (silent = false) => {
    setSaving(true);
    try {
      if (invoke) {
        await Promise.all([
          invoke('save_api_configurations', { configs: config.apiConfigs }),
          invoke('save_model_assignments', { 
            assignments: { 
              model1_config_id: config.model1ConfigId || null, 
              model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null,
                            reranker_model_config_id: config.rerankerModelConfigId || null,
                            summary_model_config_id: config.summary_model_config_id || null // 新增
            } 
          }),
          invoke('save_setting', { key: 'auto_save', value: config.autoSave.toString() }),
          invoke('save_setting', { key: 'theme', value: config.theme }),
          invoke('save_setting', { key: 'rag_enabled', value: config.ragEnabled.toString() }),
          invoke('save_setting', { key: 'rag_top_k', value: config.ragTopK.toString() }),
          invoke('save_setting', { key: 'batch_analysis_enabled', value: config.batchAnalysisEnabled.toString() }),
          invoke('save_setting', { key: 'anki_connect_enabled', value: config.ankiConnectEnabled.toString() }),
          invoke('save_setting', { key: 'gemini_adapter_test_enabled', value: config.geminiAdapterTestEnabled.toString() }),
          invoke('save_setting', { key: 'image_occlusion_enabled', value: config.imageOcclusionEnabled.toString() }),
        ]);
        if (!silent) {
          showMessage('success', '配置保存成功！');
        }
        
        // 触发设置变更事件，通知其他组件
        window.dispatchEvent(new CustomEvent('systemSettingsChanged', { 
          detail: { ankiConnectEnabled: config.ankiConnectEnabled } 
        }));
      } else {
        localStorage.setItem('ai-mistake-manager-config', JSON.stringify(config));
        if (!silent) {
          showMessage('success', '配置保存成功！（浏览器模式）');
        }
      }
    } catch (error) {
      console.error('保存配置失败:', error);
      if (!silent) {
        showMessage('error', '配置保存失败: ' + error);
      }
    } finally {
      setSaving(false);
    }
  }, [config, invoke]);

  // 标签页切换时自动保存配置
  const handleTabChange = async (newTab: string) => {
    if (!loading) {
      // 在切换标签页前先保存当前配置
      await handleSave(true);
    }
    setActiveTab(newTab);
  };

  useEffect(() => {
    loadConfig();
  }, []);

  // 自动保存配置（当配置发生变化时）
  // 注意：模型分配已经在onChange中立即保存，这里主要处理其他配置项
  useEffect(() => {
    if (!loading && config.autoSave) {
      const timeoutId = setTimeout(() => {
        // 只保存API配置和通用设置，模型分配已经立即保存了
        handleSave(true); // 静默保存
      }, 1000); // 1秒后自动保存

      return () => clearTimeout(timeoutId);
    }
  }, [config.apiConfigs, config.autoSave, config.theme, loading, handleSave]);

  const testApiConnection = async (apiId: string) => {
    const api = config.apiConfigs.find(a => a.id === apiId);
    if (!api || !api.apiKey.trim()) {
      showMessage('error', '请先输入API密钥');
      return;
    }

    if (!api.model.trim()) {
      showMessage('error', '请先设置模型名称');
      return;
    }

    setTestingApi(apiId);

    try {
      if (invoke) {
        // 使用用户指定的模型名称进行测试
        const result = await invoke('test_api_connection', {
          apiKey: api.apiKey,
          apiBase: api.baseUrl,
          model: api.model // 传递用户指定的模型名称
        });
        
        if (result) {
          showMessage('success', `${api.name} (${api.model}) API连接测试成功！`);
        } else {
          showMessage('error', `${api.name} (${api.model}) API连接测试失败`);
        }
      } else {
        // 浏览器环境模拟
        await new Promise(resolve => setTimeout(resolve, 2000));
        showMessage('success', `${api.name} API连接测试成功！（模拟）`);
      }
    } catch (error) {
      console.error('连接测试失败:', error);
      showMessage('error', `${api.name} 连接测试失败: ` + error);
    } finally {
      setTestingApi(null);
    }
  };

  const addOrUpdateApi = async (api: ApiConfig) => {
    setConfig(prev => {
      const existingIndex = prev.apiConfigs.findIndex(a => a.id === api.id);
      if (existingIndex >= 0) {
        // 更新现有配置
        const newConfigs = [...prev.apiConfigs];
        newConfigs[existingIndex] = api;
        return { ...prev, apiConfigs: newConfigs };
      } else {
        // 添加新配置
        return { ...prev, apiConfigs: [...prev.apiConfigs, api] };
      }
    });
    setEditingApi(null);
    
    // 立即保存
    if (!config.autoSave) {
      await handleSave(true);
    }
  };

  const deleteApi = async (apiId: string) => {
    // 检查是否被使用
    if (config.model1ConfigId === apiId || 
        config.model2ConfigId === apiId || 
        config.reviewAnalysisModelConfigId === apiId || 
        config.ankiCardModelConfigId === apiId || 
        config.embeddingModelConfigId === apiId || 
        config.rerankerModelConfigId === apiId ||
        config.summary_model_config_id === apiId // 新增
    ) {
      showMessage('error', '该API配置正在被使用，无法删除');
      return;
    }

    if (confirm('确定要删除这个API配置吗？')) {
      setConfig(prev => {
        const newConfig = {
          ...prev,
          apiConfigs: prev.apiConfigs.filter(a => a.id !== apiId)
        };
        return newConfig;
      });
      
      // 立即保存删除操作
      setTimeout(async () => {
        await handleSave(true);
        showMessage('success', 'API配置已删除');
      }, 100);
    }
  };

  const getMultimodalApis = () => {
    return config.apiConfigs.filter(api => api.isMultimodal && api.enabled);
  };

  const getAllEnabledApis = () => {
    return config.apiConfigs.filter(api => api.enabled);
  };

  // 渲染API类型图标的辅助函数
  const renderApiTypeIcons = (api: ApiConfig) => {
    return (
      <span style={{ display: 'inline-flex', alignItems: 'center', gap: '4px', marginLeft: '8px' }}>
        {api.isMultimodal ? <Image size={14} color="#4a90e2" /> : <FileText size={14} color="#6b7280" />}
        {api.isReasoning && <Brain size={14} color="#8b5cf6" />}
      </span>
    );
  };

  if (loading) {
    return (
      <div className="settings" style={{ display: 'flex', flexDirection: 'column', height: 'calc(100vh - 80px)' /* 调整以适应可能的外部边距/标题栏 */ }}>
        <div className="settings-header">
          <button onClick={handleBack} className="back-button">← 返回</button>
          <h2>设置</h2>
        </div>
        <div className="settings-content" style={{ textAlign: 'center', padding: '2rem' }}>
          <div>加载配置中...</div>
        </div>
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
      {/* 头部区域 - 白底统一样式 */}
      <div style={{
        padding: '1.5rem 2rem',
        borderBottom: '1px solid #e2e8f0',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        background: '#ffffff',
        boxShadow: '0 1px 3px rgba(0, 0, 0, 0.05)',
        minHeight: '72px',
        boxSizing: 'border-box'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          <svg style={{ width: '20px', height: '20px', color: '#4a5568' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
          </svg>
          <h1 style={{ fontSize: '1.5rem', fontWeight: '600', margin: 0, color: '#2d3748' }}>系统设置</h1>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
          {config.autoSave && (
            <div style={{
              background: 'rgba(40, 167, 69, 0.1)',
              border: '1px solid rgba(40, 167, 69, 0.3)',
              color: '#22c55e',
              padding: '6px 12px',
              borderRadius: '6px',
              fontSize: '12px',
              fontWeight: '500',
              display: 'inline-flex',
              alignItems: 'center',
              gap: '4px'
            }}>
              <CheckCircle size={14} style={{ marginRight: '4px' }} />
              自动保存
            </div>
          )}
          {saving && (
            <div style={{
              background: 'rgba(59, 130, 246, 0.1)',
              border: '1px solid rgba(59, 130, 246, 0.3)',
              color: '#3b82f6',
              padding: '6px 12px',
              borderRadius: '6px',
              fontSize: '12px',
              fontWeight: '500',
              display: 'inline-flex',
              alignItems: 'center',
              gap: '6px'
            }}>
              <Save size={14} style={{ marginRight: '4px' }} />
              保存中...
            </div>
          )}
          {!isTauri && (
            <div style={{
              background: 'rgba(156, 163, 175, 0.1)',
              border: '1px solid rgba(156, 163, 175, 0.3)',
              color: '#6b7280',
              padding: '6px 12px',
              borderRadius: '6px',
              fontSize: '12px',
              fontWeight: '500'
            }}>
              浏览器模式
            </div>
          )}
        </div>
      </div>

      {message && (
        <div className={`message ${message.type}`} style={{
          padding: '0.75rem 1.5rem',
          margin: '0 1.5rem',
          borderRadius: '4px',
          backgroundColor: message.type === 'success' ? '#d4edda' : '#f8d7da',
          color: message.type === 'success' ? '#155724' : '#721c24',
          border: `1px solid ${message.type === 'success' ? '#c3e6cb' : '#f5c6cb'}`
        }}>
          {message.text}
        </div>
      )}

      <div className="settings-content" style={{ padding: '24px', background: 'transparent' }}>
        <div style={{
          backgroundColor: 'white',
          padding: '1.5rem',
          borderRadius: '8px',
          boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
          marginBottom: '1.5rem'
        }}>
          <div style={{ 
            display: 'flex', 
            gap: '8px', 
            marginBottom: '0', 
            flexShrink: 0,
            borderBottom: '1px solid #e5e7eb',
            paddingBottom: '0'
          }}>
            <button 
              onClick={() => handleTabChange('apis')}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '12px 20px',
                border: 'none',
                background: activeTab === 'apis' ? '#f0f9ff' : 'transparent',
                color: activeTab === 'apis' ? '#0369a1' : '#6b7280',
                borderRadius: '8px',
                fontWeight: activeTab === 'apis' ? '600' : '500',
                fontSize: '14px',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                position: 'relative',
                borderBottom: activeTab === 'apis' ? '3px solid #0369a1' : '3px solid transparent',
                marginBottom: '-1px'
              }}
              onMouseEnter={(e) => {
                if (activeTab !== 'apis') {
                  e.currentTarget.style.background = '#f3f4f6';
                  e.currentTarget.style.color = '#374151';
                }
              }}
              onMouseLeave={(e) => {
                if (activeTab !== 'apis') {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = '#6b7280';
                }
              }}
            >
              <Bot style={{ width: '16px', height: '16px', marginRight: '4px' }} />
              <span>API配置</span>
            </button>
            <button 
              onClick={() => handleTabChange('models')}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '12px 20px',
                border: 'none',
                background: activeTab === 'models' ? '#f0f9ff' : 'transparent',
                color: activeTab === 'models' ? '#0369a1' : '#6b7280',
                borderRadius: '8px',
                fontWeight: activeTab === 'models' ? '600' : '500',
                fontSize: '14px',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                position: 'relative',
                borderBottom: activeTab === 'models' ? '3px solid #0369a1' : '3px solid transparent',
                marginBottom: '-1px'
              }}
              onMouseEnter={(e) => {
                if (activeTab !== 'models') {
                  e.currentTarget.style.background = '#f3f4f6';
                  e.currentTarget.style.color = '#374151';
                }
              }}
              onMouseLeave={(e) => {
                if (activeTab !== 'models') {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = '#6b7280';
                }
              }}
            >
              <FlaskConical style={{ width: '16px', height: '16px', marginRight: '4px' }} />
              <span>模型分配</span>
            </button>
            <button 
              onClick={() => handleTabChange('subjects')}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '12px 20px',
                border: 'none',
                background: activeTab === 'subjects' ? '#f0f9ff' : 'transparent',
                color: activeTab === 'subjects' ? '#0369a1' : '#6b7280',
                borderRadius: '8px',
                fontWeight: activeTab === 'subjects' ? '600' : '500',
                fontSize: '14px',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                position: 'relative',
                borderBottom: activeTab === 'subjects' ? '3px solid #0369a1' : '3px solid transparent',
                marginBottom: '-1px'
              }}
              onMouseEnter={(e) => {
                if (activeTab !== 'subjects') {
                  e.currentTarget.style.background = '#f3f4f6';
                  e.currentTarget.style.color = '#374151';
                }
              }}
              onMouseLeave={(e) => {
                if (activeTab !== 'subjects') {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = '#6b7280';
                }
              }}
            >
              <Target style={{ width: '16px', height: '16px', marginRight: '4px' }} />
              <span>科目配置</span>
            </button>
            <button 
              onClick={() => handleTabChange('general')}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '8px',
                padding: '12px 20px',
                border: 'none',
                background: activeTab === 'general' ? '#f0f9ff' : 'transparent',
                color: activeTab === 'general' ? '#0369a1' : '#6b7280',
                borderRadius: '8px',
                fontWeight: activeTab === 'general' ? '600' : '500',
                fontSize: '14px',
                cursor: 'pointer',
                transition: 'all 0.2s ease',
                position: 'relative',
                borderBottom: activeTab === 'general' ? '3px solid #0369a1' : '3px solid transparent',
                marginBottom: '-1px'
              }}
              onMouseEnter={(e) => {
                if (activeTab !== 'general') {
                  e.currentTarget.style.background = '#f3f4f6';
                  e.currentTarget.style.color = '#374151';
                }
              }}
              onMouseLeave={(e) => {
                if (activeTab !== 'general') {
                  e.currentTarget.style.background = 'transparent';
                  e.currentTarget.style.color = '#6b7280';
                }
              }}
            >
              <SettingsIcon style={{ width: '16px', height: '16px', marginRight: '4px' }} />
              <span>通用设置</span>
            </button>
          </div>
        </div>

        {/* API配置管理 */}
        {activeTab === 'apis' && (
          <div style={{
            backgroundColor: 'white',
            padding: '1.5rem',
            borderRadius: '8px',
            boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
            marginBottom: '1.5rem'
          }}>
            <div className="apis-section">
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '20px' }}>
              <h3>API配置管理</h3>
              <button
                className="btn btn-success add-api-button"
                onClick={() => setEditingApi({
                  id: `api_${Date.now()}`,
                  name: '新API配置',
                  apiKey: '',
                  baseUrl: 'https://api.openai.com/v1',
                  model: '',
                  isMultimodal: false,
                  isReasoning: false,
                  enabled: true,
                  modelAdapter: 'general'
                })}
              >
                <Plus style={{ width: '16px', height: '16px', marginRight: '4px' }} /> 添加API配置
              </button>
            </div>

            {config.apiConfigs.length === 0 ? (
              <div style={{ 
                textAlign: 'center', 
                padding: '40px', 
                color: '#666',
                border: '2px dashed #ddd',
                borderRadius: '8px'
              }}>
                <Bot style={{ width: '48px', height: '48px', marginBottom: '16px', color: '#ccc' }} />
                <div style={{ fontSize: '18px', marginBottom: '8px' }}>还没有API配置</div>
                <div style={{ fontSize: '14px' }}>点击上方"添加API配置"按钮开始设置</div>
              </div>
            ) : (
              config.apiConfigs.map(api => (
                <div key={api.id} className="api-card" style={{
                  border: '1px solid #ddd',
                  borderRadius: '8px',
                  padding: '20px',
                  marginBottom: '15px',
                  backgroundColor: api.enabled ? '#fff' : '#f8f9fa'
                }}>
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                    <div>
                      <h4 style={{ margin: '0 0 10px 0' }}>{api.name}</h4>
                      <div style={{ fontSize: '14px', color: '#666' }}>
                        <div><strong>模型:</strong> {api.model || '未设置'}</div>
                        <div><strong>地址:</strong> {api.baseUrl}</div>
                        <div><strong>类型:</strong> {api.isMultimodal ? <FileText style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} /> : <Book style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} />} {api.isMultimodal ? '多模态' : '纯文本'}</div>
                        <div><strong>推理:</strong> {api.isReasoning ? <Cpu style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} /> : <RefreshCcw style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} />} {api.isReasoning ? '推理模型' : '标准模型'}</div>
                        <div><strong>适配器:</strong> {api.modelAdapter === 'deepseek-r1' ? <Atom style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} /> : <HardDrive style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px' }} />} {api.modelAdapter === 'deepseek-r1' ? 'DeepSeek-R1' : '通用模型'}</div>
                        <div><strong>状态:</strong> {api.enabled ? <CheckCircle style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px', color: '#28a745' }} /> : <XCircle style={{ display: 'inline', width: '16px', height: '16px', verticalAlign: 'middle', marginRight: '4px', color: '#dc3545' }} />} {api.enabled ? '启用' : '禁用'}</div>
                      </div>
                    </div>
                    <div style={{ display: 'flex', gap: '10px' }}>
                      <button 
                        className="btn btn-primary"
                        onClick={() => testApiConnection(api.id)}
                        disabled={testingApi === api.id || !api.model.trim()}
                        title={!api.model.trim() ? '请先设置模型名称' : ''}
                      >
                        {testingApi === api.id ? '测试中...' : <TestTube style={{ width: '16px', height: '16px', marginRight: '4px' }} />} {testingApi === api.id ? '测试中...' : '测试连接'}
                      </button>
                      <button 
                        className="btn btn-secondary"
                        onClick={() => setEditingApi(api)}
                      >
                        <Edit style={{ width: '16px', height: '16px', marginRight: '4px' }} /> 编辑
                      </button>
                      <button 
                        className="btn btn-danger"
                        onClick={() => deleteApi(api.id)}
                      >
                        <Trash2 style={{ width: '16px', height: '16px', marginRight: '4px' }} /> 删除
                      </button>
                    </div>
                  </div>
                </div>
              ))
            )}
            </div>
          </div>
        )}

        {/* 模型分配 */}
        {activeTab === 'models' && (
          <div style={{
            backgroundColor: 'white',
            padding: '1.5rem',
            borderRadius: '8px',
            boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
            marginBottom: '1.5rem'
          }}>
            <div className="models-section">
            <h3>模型分配</h3>
            <p style={{ color: '#666', marginBottom: '20px' }}>
              为系统的各项AI功能分配合适的模型。不同的功能（如OCR识别、题目解答、回顾分析、ANKI制卡、知识库嵌入、总结生成等）可能需要不同类型或能力的模型以达到最佳效果。请根据您的API配置和需求进行选择。
            </p>

            <div style={{ display: 'grid', gap: '20px' }}>
              {/* 第一模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                    <Image size={20} color="#4a90e2" />
                    <h4 style={{ margin: 0 }}>第一模型（OCR + 分类）</h4>
                  </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于图片识别和题目分类，必须选择多模态模型
                </p>
                <select 
                  value={config.model1ConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    // 立即保存模型选择，使用最新的值
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: newValue || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null, // 保持其他字段
                            reranker_model_config_id: config.rerankerModelConfigId || null,  // 保持其他字段
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        // 保存成功后再更新前端状态
                        setConfig(prev => ({ ...prev, model1ConfigId: newValue }));
                        showMessage('success', '第一模型配置已保存');
                        console.log('第一模型配置保存成功:', newValue);
                        
                        // 验证保存结果
                        setTimeout(async () => {
                          try {
                            const verification = await invoke('get_model_assignments');
                            console.log('验证第一模型保存结果:', verification);
                          } catch (err) {
                            console.error('验证保存结果失败:', err);
                          }
                        }, 500);
                      }
                    } catch (error) {
                      console.error('保存第一模型配置失败:', error);
                      showMessage('error', '保存第一模型配置失败: ' + error);
                      // 保存失败时不更新前端状态
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择多模态模型...</option>
                  {getMultimodalApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model})
                    </option>
                  ))}
                </select>
                {getMultimodalApis().length === 0 && (
                  <div style={{ color: '#dc3545', fontSize: '14px', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <AlertTriangle size={16} />
                    没有可用的多模态API配置，请先添加并启用多模态API
                  </div>
                )}
              </div>

              {/* 第二模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                    <Brain size={20} color="#8b5cf6" />
                    <h4 style={{ margin: 0 }}>第二模型（解答 + 对话）</h4>
                  </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于题目解答和对话交互，可以选择任意类型的模型
                </p>
                <select 
                  value={config.model2ConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    // 立即保存模型选择，使用最新的值
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: newValue || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null, // 保持其他字段
                            reranker_model_config_id: config.rerankerModelConfigId || null,  // 保持其他字段
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        // 保存成功后再更新前端状态
                        setConfig(prev => ({ ...prev, model2ConfigId: newValue }));
                        showMessage('success', '第二模型配置已保存');
                        console.log('第二模型配置保存成功:', newValue);
                        
                        // 验证保存结果
                        setTimeout(async () => {
                          try {
                            const verification = await invoke('get_model_assignments');
                            console.log('验证第二模型保存结果:', verification);
                          } catch (err) {
                            console.error('验证保存结果失败:', err);
                          }
                        }, 500);
                      }
                    } catch (error) {
                      console.error('保存第二模型配置失败:', error);
                      showMessage('error', '保存第二模型配置失败: ' + error);
                      // 保存失败时不更新前端状态
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择模型...</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
                {getAllEnabledApis().length === 0 && (
                  <div style={{ color: '#dc3545', fontSize: '14px', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <AlertTriangle size={16} />
                    没有可用的API配置，请先添加并启用API
                  </div>
                )}
              </div>

              {/* 回顾分析模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Target size={20} color="#ef4444" />
                  <h4 style={{ margin: 0 }}>第三模型（回顾分析）</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于回顾分析功能，对多个错题进行统一分析，可以选择任意类型的模型
                </p>
                <select 
                  value={config.reviewAnalysisModelConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    // 立即保存模型选择，使用最新的值
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: newValue || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null, // 保持其他字段
                            reranker_model_config_id: config.rerankerModelConfigId || null,  // 保持其他字段
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        // 保存成功后再更新前端状态
                        setConfig(prev => ({ ...prev, reviewAnalysisModelConfigId: newValue }));
                        showMessage('success', '回顾分析模型配置已保存');
                        console.log('回顾分析模型配置保存成功:', newValue);
                        
                        // 验证保存结果
                        setTimeout(async () => {
                          try {
                            const verification = await invoke('get_model_assignments');
                            console.log('验证回顾分析模型保存结果:', verification);
                          } catch (err) {
                            console.error('验证保存结果失败:', err);
                          }
                        }, 500);
                      }
                    } catch (error) {
                      console.error('保存回顾分析模型配置失败:', error);
                      showMessage('error', '保存回顾分析模型配置失败: ' + error);
                      // 保存失败时不更新前端状态
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择模型...</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
                {getAllEnabledApis().length === 0 && (
                  <div style={{ color: '#dc3545', fontSize: '14px', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <AlertTriangle size={16} />
                    没有可用的API配置，请先添加并启用API
                  </div>
                )}
              </div>

              {/* ANKI制卡模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Target size={20} color="#10b981" />
                  <h4 style={{ margin: 0 }}>ANKI制卡模型（卡片生成）</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于ANKI卡片生成功能，根据学习内容智能生成问答卡片
                </p>
                <select 
                  value={config.ankiCardModelConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    // 立即保存模型选择，使用最新的值
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: newValue || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null, // 保持其他字段
                            reranker_model_config_id: config.rerankerModelConfigId || null,  // 保持其他字段
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        // 保存成功后再更新前端状态
                        setConfig(prev => ({ ...prev, ankiCardModelConfigId: newValue }));
                        showMessage('success', 'ANKI制卡模型配置已保存');
                        console.log('ANKI制卡模型配置保存成功:', newValue);
                        
                        // 验证保存结果
                        setTimeout(async () => {
                          try {
                            const verification = await invoke('get_model_assignments');
                            console.log('验证ANKI制卡模型保存结果:', verification);
                          } catch (err) {
                            console.error('验证保存结果失败:', err);
                          }
                        }, 500);
                      }
                    } catch (error) {
                      console.error('保存ANKI制卡模型配置失败:', error);
                      showMessage('error', '保存ANKI制卡模型配置失败: ' + error);
                      // 保存失败时不更新前端状态
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择模型...</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
                {getAllEnabledApis().length === 0 && (
                  <div style={{ color: '#dc3545', fontSize: '14px', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <AlertTriangle size={16} />
                    没有可用的API配置，请先添加并启用API
                  </div>
                )}
              </div>

              {/* RAG嵌入模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                    <Database size={20} color="#10b981" />
                    <h4 style={{ margin: 0 }}>嵌入模型（RAG知识库）</h4>
                  </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于将文档转换为向量嵌入，支持RAG知识库检索功能
                </p>
                <select 
                  value={config.embeddingModelConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: newValue || null,
                            reranker_model_config_id: config.rerankerModelConfigId || null,
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        setConfig(prev => ({ ...prev, embeddingModelConfigId: newValue }));
                        showMessage('success', '嵌入模型配置已保存');
                      }
                    } catch (error) {
                      console.error('保存嵌入模型配置失败:', error);
                      showMessage('error', '保存嵌入模型配置失败: ' + error);
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择嵌入模型...</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
              </div>

              {/* RAG重排序模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <RefreshCcw size={20} color="#6366f1" />
                  <h4 style={{ margin: 0 }}>重排序模型（RAG优化，可选）</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  用于对RAG检索结果进行重排序，提高相关性（可选配置）
                </p>
                <select 
                  value={config.rerankerModelConfigId}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null,
                            reranker_model_config_id: newValue || null,
                            summary_model_config_id: config.summary_model_config_id || null // 保持其他字段
                          } 
                        });
                        setConfig(prev => ({ ...prev, rerankerModelConfigId: newValue }));
                        showMessage('success', '重排序模型配置已保存');
                      }
                    } catch (error) {
                      console.error('保存重排序模型配置失败:', error);
                      showMessage('error', '保存重排序模型配置失败: ' + error);
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">不使用重排序（可选）</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
              </div>

              {/* 总结生成模型配置 */}
              <div className="model-config" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                                  <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                    <ScrollText size={20} color="#f59e0b" />
                    <h4 style={{ margin: 0 }}>总结生成模型（错题总结）</h4>
                  </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  专门用于生成错题总结，建议使用理解和概括能力强的模型。如果未选择，将使用"第二模型（解答 + 对话）"。
                </p>
                <select 
                  value={config.summary_model_config_id}
                  onChange={async (e) => {
                    const newValue = e.target.value;
                    try {
                      if (invoke) {
                        await invoke('save_model_assignments', { 
                          assignments: { 
                            model1_config_id: config.model1ConfigId || null, 
                            model2_config_id: config.model2ConfigId || null,
                            review_analysis_model_config_id: config.reviewAnalysisModelConfigId || null,
                            anki_card_model_config_id: config.ankiCardModelConfigId || null,
                            embedding_model_config_id: config.embeddingModelConfigId || null,
                            reranker_model_config_id: config.rerankerModelConfigId || null,
                            summary_model_config_id: newValue || null
                          } 
                        });
                        setConfig(prev => ({ ...prev, summary_model_config_id: newValue }));
                        showMessage('success', '总结生成模型配置已保存');
                      }
                    } catch (error) {
                      console.error('保存总结生成模型配置失败:', error);
                      showMessage('error', '保存总结生成模型配置失败: ' + error);
                    }
                  }}
                  style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="">请选择模型（默认使用第二模型）...</option>
                  {getAllEnabledApis().map(api => (
                    <option key={api.id} value={api.id}>
                      {api.name} ({api.model}) {api.isMultimodal ? '[图像]' : '[文本]'} {api.isReasoning ? '[推理]' : ''}
                    </option>
                  ))}
                </select>
              </div>
            </div>

            {/* 配置状态检查 */}
            <div style={{ marginTop: '20px', padding: '15px', backgroundColor: '#f8f9fa', borderRadius: '8px' }}>
              <h5>配置状态检查</h5>
              <div style={{ fontSize: '14px' }}>
                <div style={{ color: config.model1ConfigId ? '#28a745' : '#dc3545', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.model1ConfigId ? <CheckCircle size={16} /> : <XCircle size={16} />}
                  第一模型: {config.model1ConfigId ? '已配置' : '未配置'}
                </div>
                <div style={{ color: config.model2ConfigId ? '#28a745' : '#dc3545', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.model2ConfigId ? <CheckCircle size={16} /> : <XCircle size={16} />}
                  第二模型: {config.model2ConfigId ? '已配置' : '未配置'}
                </div>
                <div style={{ color: config.reviewAnalysisModelConfigId ? '#28a745' : '#dc3545', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.reviewAnalysisModelConfigId ? <CheckCircle size={16} /> : <XCircle size={16} />}
                  回顾分析模型: {config.reviewAnalysisModelConfigId ? '已配置' : '未配置'}
                </div>
                <div style={{ color: config.ankiCardModelConfigId ? '#28a745' : '#dc3545', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.ankiCardModelConfigId ? <CheckCircle size={16} /> : <XCircle size={16} />}
                  ANKI制卡模型: {config.ankiCardModelConfigId ? '已配置' : '未配置'}
                </div>
                <div style={{ color: config.embeddingModelConfigId ? '#28a745' : '#dc3545', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.embeddingModelConfigId ? <CheckCircle size={16} /> : <XCircle size={16} />}
                  RAG嵌入模型: {config.embeddingModelConfigId ? '已配置' : '未配置'}
                </div>
                <div style={{ color: config.rerankerModelConfigId ? '#28a745' : '#ffc107', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.rerankerModelConfigId ? <CheckCircle size={16} /> : <AlertTriangle size={16} />}
                  RAG重排序模型: {config.rerankerModelConfigId ? '已配置' : '未配置（可选）'}
                </div>
                <div style={{ color: config.summary_model_config_id ? '#28a745' : '#ffc107', display: 'flex', alignItems: 'center', gap: '8px' }}>
                  {config.summary_model_config_id ? <CheckCircle size={16} /> : <AlertTriangle size={16} />}
                  总结生成模型: {config.summary_model_config_id ? '已配置' : '未配置（将使用第二模型）'}
                </div>
                {config.model1ConfigId && config.model2ConfigId && (
                  <div style={{ color: '#28a745', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <PartyPopper size={16} />
                    基础功能配置完成，可以开始使用错题分析功能！
                  </div>
                )}
                {config.model1ConfigId && config.model2ConfigId && config.reviewAnalysisModelConfigId && (
                  <div style={{ color: '#28a745', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <PartyPopper size={16} />
                    高级功能配置完成，可以使用错题分析和回顾分析功能！
                  </div>
                )}
                {config.model1ConfigId && config.model2ConfigId && config.reviewAnalysisModelConfigId && config.ankiCardModelConfigId && (
                  <div style={{ color: '#28a745', marginTop: '8px', display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <PartyPopper size={16} />
                    所有功能配置完成，可以使用错题分析、回顾分析和ANKI制卡功能！
                  </div>
                )}
              </div>
            </div>
            </div>
          </div>
        )}

        {/* 科目配置 */}
        {activeTab === 'subjects' && (
          <div style={{
            backgroundColor: 'white',
            padding: '1.5rem',
            borderRadius: '8px',
            boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
            marginBottom: '1.5rem'
          }}>
            <div className="subjects-section">
              <SubjectConfig />
            </div>
          </div>
        )}

        {/* 通用设置 */}
        {activeTab === 'general' && (
          <div style={{
            backgroundColor: 'white',
            padding: '1.5rem',
            borderRadius: '8px',
            boxShadow: '0 2px 4px rgba(0,0,0,0.1)',
            marginBottom: '1.5rem'
          }}>
            <div className="general-section">
            <h3>通用设置</h3>
            
            <div style={{ display: 'grid', gap: '20px' }}>
              <div className="setting-item" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Save size={20} color="#10b981" />
                  <h4 style={{ margin: 0 }}>自动保存</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  启用后，配置更改将自动保存，无需手动点击保存按钮
                </p>
                <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <input 
                    type="checkbox"
                    checked={config.autoSave}
                    onChange={(e) => setConfig(prev => ({ ...prev, autoSave: e.target.checked }))}
                  />
                  启用自动保存
                </label>
              </div>

              <div className="setting-item" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Palette size={20} color="#8b5cf6" />
                  <h4 style={{ margin: 0 }}>主题设置</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  选择应用的外观主题
                </p>
                <select 
                  value={config.theme}
                  onChange={(e) => setConfig(prev => ({ ...prev, theme: e.target.value }))}
                  style={{ width: '200px', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                >
                  <option value="light">浅色主题</option>
                  <option value="dark">深色主题</option>
                  <option value="auto">跟随系统</option>
                </select>
              </div>

              {/* RAG设置 */}
              <div className="setting-item" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Database size={20} color="#3b82f6" />
                  <h4 style={{ margin: 0 }}>RAG知识库设置</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  配置检索增强生成(RAG)功能的全局设置
                </p>
                
                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input 
                      type="checkbox"
                      checked={config.ragEnabled}
                      onChange={(e) => setConfig(prev => ({ ...prev, ragEnabled: e.target.checked }))}
                    />
                    启用RAG知识库功能
                  </label>
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px', marginLeft: '20px' }}>
                    启用后，AI分析将能够利用您上传的知识库文档提供更准确的解答
                  </p>
                </div>

                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'block', marginBottom: '5px', fontSize: '14px', fontWeight: '500' }}>
                    检索文档数量 (Top-K):
                  </label>
                  <input 
                    type="number"
                    min="1"
                    max="20"
                    value={config.ragTopK}
                    onChange={(e) => setConfig(prev => ({ ...prev, ragTopK: parseInt(e.target.value) || 5 }))}
                    style={{ width: '100px', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
                  />
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px' }}>
                    每次查询时从知识库检索的相关文档数量，建议设置为3-10
                  </p>
                </div>

                {config.ragEnabled && !config.embeddingModelConfigId && (
                  <div style={{ 
                    backgroundColor: '#fff3cd', 
                    border: '1px solid #ffeaa7', 
                    borderRadius: '4px', 
                    padding: '10px', 
                    marginTop: '10px' 
                  }}>
                    <div style={{ color: '#856404', fontSize: '14px', margin: 0, display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <AlertTriangle size={16} />
                      RAG功能已启用，但未配置嵌入模型。请在"模型分配"标签页中配置嵌入模型。
                    </div>
                  </div>
                )}
              </div>

              {/* 开发功能设置 */}
              <div className="setting-item" style={{
                border: '1px solid #ddd',
                borderRadius: '8px',
                padding: '20px'
              }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '8px', marginBottom: '15px' }}>
                  <Construction size={20} color="#f59e0b" />
                  <h4 style={{ margin: 0 }}>开发功能设置</h4>
                </div>
                <p style={{ color: '#666', fontSize: '14px', marginBottom: '15px' }}>
                  控制实验性和开发中的功能
                </p>
                
                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input 
                      type="checkbox"
                      checked={config.batchAnalysisEnabled}
                      onChange={(e) => setConfig(prev => ({ ...prev, batchAnalysisEnabled: e.target.checked }))}
                    />
                    启用批量分析功能
                  </label>
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px', marginLeft: '20px' }}>
                    启用后，将在侧边栏显示"批量分析"选项。此功能仍在开发中，可能存在一些问题。
                  </p>
                </div>

                {/* AnkiConnect功能开关 */}
                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input 
                      type="checkbox"
                      checked={config.ankiConnectEnabled}
                      onChange={(e) => setConfig(prev => ({ ...prev, ankiConnectEnabled: e.target.checked }))}
                    />
                    启用AnkiConnect集成
                  </label>
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px', marginLeft: '20px' }}>
                    启用后，ANKI制卡页面将显示AnkiConnect相关功能，可以直接导入卡片到Anki应用。
                  </p>
                </div>

                {/* Gemini适配器测试功能开关 */}
                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input 
                      type="checkbox"
                      checked={config.geminiAdapterTestEnabled}
                      onChange={(e) => setConfig(prev => ({ ...prev, geminiAdapterTestEnabled: e.target.checked }))}
                    />
                    启用Gemini适配器测试模块
                  </label>
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px', marginLeft: '20px' }}>
                    启用后，将在侧边栏显示"Gemini适配器测试"选项。此功能用于测试和调试Gemini API适配器。
                  </p>
                </div>

                {/* 图片遮罩卡功能开关 */}
                <div style={{ marginBottom: '15px' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                    <input 
                      type="checkbox"
                      checked={config.imageOcclusionEnabled}
                      onChange={(e) => setConfig(prev => ({ ...prev, imageOcclusionEnabled: e.target.checked }))}
                    />
                    启用图片遮罩卡功能
                  </label>
                  <p style={{ color: '#666', fontSize: '12px', marginTop: '5px', marginLeft: '20px' }}>
                    启用后，将在侧边栏显示"图片遮罩卡"选项。此功能用于创建ANKI图片遮罩记忆卡片。
                  </p>
                </div>

                {config.batchAnalysisEnabled && (
                  <div style={{ 
                    backgroundColor: '#fff3cd', 
                    border: '1px solid #ffeaa7', 
                    borderRadius: '4px', 
                    padding: '10px', 
                    marginTop: '10px' 
                  }}>
                    <div style={{ color: '#856404', fontSize: '14px', margin: 0, display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <AlertTriangle size={16} />
                      批量分析功能仍在开发中，可能会遇到一些问题。建议优先使用单题分析功能。
                    </div>
                  </div>
                )}
              </div>

              {!config.autoSave && (
                <div style={{ textAlign: 'center', marginTop: '20px' }}>
                  <button
                    className="btn btn-primary save-all-settings-button"
                    onClick={() => handleSave()}
                    disabled={saving}
                    style={{ padding: '12px 24px', fontSize: '16px' }}
                  >
                    {saving ? '保存中...' : (
                      <span style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        <Save size={16} />
                        保存所有设置
                      </span>
                    )}
                  </button>
                </div>
              )}
            </div>
            </div>
          </div>
        )}
      </div>

      {/* API编辑模态框 */}
      {editingApi && (
        <ApiEditModal 
          api={editingApi}
          onSave={addOrUpdateApi}
          onCancel={() => setEditingApi(null)}
        />
      )}

    </div>
  );
};

// API编辑模态框组件
interface ApiEditModalProps {
  api: ApiConfig;
  onSave: (api: ApiConfig) => void;
  onCancel: () => void;
}

const ApiEditModal: React.FC<ApiEditModalProps> = ({ api, onSave, onCancel }) => {
  const [formData, setFormData] = useState({
    ...api,
    isReasoning: api.isReasoning || false,  // 确保有默认值
    modelAdapter: api.modelAdapter || 'general'  // 确保有默认值
  });
  const [modelAdapterOptions, setModelAdapterOptions] = useState<any[]>([]);

  // 加载模型适配器选项
  React.useEffect(() => {
    const loadModelAdapterOptions = async () => {
      try {
        if (invoke) {
          const options = await invoke('get_model_adapter_options') as any[];
          setModelAdapterOptions(options);
        } else {
          // 浏览器环境的默认选项
          setModelAdapterOptions([
            { value: 'general', label: '通用模型', description: '适用于大多数标准AI模型' },
            { value: 'deepseek-r1', label: 'DeepSeek-R1', description: '专为DeepSeek-R1推理模型优化' },
            { value: 'google', label: 'Google Gemini', description: 'Google Gemini系列模型，支持多模态和高质量文本生成' },
            { value: 'o1-series', label: 'OpenAI o1系列', description: 'OpenAI o1-preview和o1-mini等推理模型' },
            { value: 'claude-3-5-sonnet', label: 'Claude 3.5 Sonnet', description: 'Anthropic Claude 3.5 Sonnet高性能模型' }
          ]);
        }
      } catch (error) {
        console.error('加载模型适配器选项失败:', error);
        // 使用默认选项
        setModelAdapterOptions([
          { value: 'general', label: '通用模型', description: '适用于大多数标准AI模型' },
          { value: 'deepseek-r1', label: 'DeepSeek-R1', description: '专为DeepSeek-R1推理模型优化' },
          { value: 'google', label: 'Google Gemini', description: 'Google Gemini系列模型，支持多模态和高质量文本生成' },
          { value: 'o1-series', label: 'OpenAI o1系列', description: 'OpenAI o1-preview和o1-mini等推理模型' },
          { value: 'claude-3-5-sonnet', label: 'Claude 3.5 Sonnet', description: 'Anthropic Claude 3.5 Sonnet高性能模型' }
        ]);
      }
    };

    loadModelAdapterOptions();
  }, []);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    
    // 验证必填字段
    if (!formData.name.trim()) {
      alert('请输入配置名称');
      return;
    }
    if (!formData.baseUrl.trim()) {
      alert('请输入API地址');
      return;
    }
    if (!formData.model.trim()) {
      alert('请输入模型名称');
      return;
    }
    
    onSave(formData);
  };

  return (
    <div style={{
      position: 'fixed',
      top: 0,
      left: 0,
      right: 0,
      bottom: 0,
      backgroundColor: 'rgba(0,0,0,0.5)',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      zIndex: 1000
    }}>
      <div style={{
        backgroundColor: 'white',
        borderRadius: '8px',
        padding: '24px',
        width: '90%',
        maxWidth: '500px',
        maxHeight: '90vh',
        overflow: 'auto'
      }}>
        <h3 style={{ marginTop: 0 }}>
          {api.id.startsWith('api_') ? '添加API配置' : '编辑API配置'}
        </h3>
        
        <form onSubmit={handleSubmit}>
          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontWeight: 'bold' }}>
              配置名称 *
            </label>
            <input 
              type="text"
              value={formData.name}
              onChange={(e) => setFormData(prev => ({ ...prev, name: e.target.value }))}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
              placeholder="例如：OpenAI GPT-4"
              required
            />
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontWeight: 'bold' }}>
              API地址 *
            </label>
            <input 
              type="url"
              value={formData.baseUrl}
              onChange={(e) => setFormData(prev => ({ ...prev, baseUrl: e.target.value }))}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
              placeholder="https://api.openai.com/v1"
              required
            />
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontWeight: 'bold' }}>
              模型名称 *
            </label>
            <input 
              type="text"
              value={formData.model}
              onChange={(e) => setFormData(prev => ({ ...prev, model: e.target.value }))}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
              placeholder="例如：gpt-4-vision-preview"
              required
            />
            <div style={{ fontSize: '12px', color: '#666', marginTop: '4px' }}>
              请输入准确的模型名称，这将用于API调用
            </div>
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontWeight: 'bold' }}>
              API密钥
            </label>
            <input 
              type="password"
              value={formData.apiKey}
              onChange={(e) => setFormData(prev => ({ ...prev, apiKey: e.target.value }))}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
              placeholder="sk-..."
            />
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <input 
                type="checkbox"
                checked={formData.isMultimodal}
                onChange={(e) => setFormData(prev => ({ ...prev, isMultimodal: e.target.checked }))}
              />
              <span style={{ fontWeight: 'bold' }}>多模态模型</span>
            </label>
            <div style={{ fontSize: '12px', color: '#666', marginTop: '4px' }}>
              勾选此项表示该模型支持图片输入（如GPT-4V、Claude-3等）
            </div>
          </div>

          <div style={{ marginBottom: '16px' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <input 
                type="checkbox"
                checked={formData.isReasoning}
                onChange={(e) => setFormData(prev => ({ ...prev, isReasoning: e.target.checked }))}
              />
              <span style={{ fontWeight: 'bold' }}>推理模型</span>
            </label>
            <div style={{ fontSize: '12px', color: '#666', marginTop: '4px' }}>
              勾选此项表示该模型支持推理功能
            </div>
          </div>

          <div style={{ marginBottom: '24px' }}>
            <label style={{ display: 'block', marginBottom: '4px', fontWeight: 'bold' }}>
              模型适配器 *
            </label>
            <select 
              value={formData.modelAdapter}
              onChange={(e) => setFormData(prev => ({ ...prev, modelAdapter: e.target.value }))}
              style={{ width: '100%', padding: '8px', borderRadius: '4px', border: '1px solid #ddd' }}
              required
            >
              {modelAdapterOptions.map(option => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
            {formData.modelAdapter && modelAdapterOptions.find(opt => opt.value === formData.modelAdapter) && (
              <div style={{ fontSize: '12px', color: '#666', marginTop: '4px' }}>
                {modelAdapterOptions.find(opt => opt.value === formData.modelAdapter)?.description}
              </div>
            )}
          </div>

          <div style={{ marginBottom: '24px' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <input 
                type="checkbox"
                checked={formData.enabled}
                onChange={(e) => setFormData(prev => ({ ...prev, enabled: e.target.checked }))}
              />
              <span style={{ fontWeight: 'bold' }}>启用此配置</span>
            </label>
          </div>

          <div style={{ display: 'flex', gap: '12px', justifyContent: 'flex-end' }}>
            <button
              type="button"
              className="cancel-button"
              onClick={onCancel}
            >
              取消
            </button>
            <button
              type="submit"
              className="save-button"
            >
              保存
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
