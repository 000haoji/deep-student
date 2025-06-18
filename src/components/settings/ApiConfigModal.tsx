/**
 * API Configuration Modal Component
 * Split from the large Settings component for better maintainability
 */

import React, { useState, useEffect } from 'react';
import { ApiConfig } from '../../hooks/useApiConfig';

interface ApiConfigModalProps {
  config: ApiConfig | null; // null for new config
  onSave: (config: Omit<ApiConfig, 'id'>) => void;
  onCancel: () => void;
}

export const ApiConfigModal: React.FC<ApiConfigModalProps> = ({
  config,
  onSave,
  onCancel
}) => {
  const [formData, setFormData] = useState<Omit<ApiConfig, 'id'>>({
    name: '',
    apiKey: '',
    baseUrl: '',
    model: '',
    isMultimodal: false,
    isReasoning: false,
    enabled: true,
    modelAdapter: 'general'
  });

  const [errors, setErrors] = useState<Record<string, string>>({});

  // 预定义的模型适配器选项
  const modelAdapters = [
    { value: 'general', label: '通用模型' },
    { value: 'deepseek-r1', label: 'DeepSeek-R1' },
    { value: 'google', label: 'Google Gemini' },
    { value: 'o1-series', label: 'OpenAI o1系列' },
    { value: 'claude-3-5-sonnet', label: 'Claude 3.5 Sonnet' }
  ];

  // 常见模型预设
  const modelPresets: Record<string, Partial<Omit<ApiConfig, 'id'>>> = {
    'gpt-4o': {
      model: 'gpt-4o',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'general'
    },
    'gpt-4o-mini': {
      model: 'gpt-4o-mini',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'general'
    },
    'o1-preview': {
      model: 'o1-preview',
      isMultimodal: false,
      isReasoning: true,
      modelAdapter: 'o1-series'
    },
    'o1-mini': {
      model: 'o1-mini',
      isMultimodal: false,
      isReasoning: true,
      modelAdapter: 'o1-series'
    },
    'claude-3-5-sonnet': {
      model: 'claude-3-5-sonnet-20241022',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'claude-3-5-sonnet'
    },
    'gemini-1.5-pro': {
      model: 'gemini-1.5-pro',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'google'
    },
    'gemini-1.5-pro-latest': {
      model: 'gemini-1.5-pro-latest',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'google'
    },
    'gemini-1.5-flash': {
      model: 'gemini-1.5-flash',
      isMultimodal: true,
      isReasoning: false,
      modelAdapter: 'google'
    }
  };

  // 初始化表单数据
  useEffect(() => {
    if (config) {
      setFormData(config);
    } else {
      setFormData({
        name: '',
        apiKey: '',
        baseUrl: '',
        model: '',
        isMultimodal: false,
        isReasoning: false,
        enabled: true,
        modelAdapter: 'general'
      });
    }
    setErrors({});
  }, [config]);

  // 表单验证
  const validateForm = (): boolean => {
    const newErrors: Record<string, string> = {};

    if (!formData.name.trim()) {
      newErrors.name = '配置名称不能为空';
    }

    if (!formData.apiKey.trim()) {
      newErrors.apiKey = 'API密钥不能为空';
    }

    if (!formData.baseUrl.trim()) {
      newErrors.baseUrl = 'API地址不能为空';
    } else {
      try {
        new URL(formData.baseUrl);
      } catch {
        newErrors.baseUrl = 'API地址格式不正确';
      }
    }

    if (!formData.model.trim()) {
      newErrors.model = '模型名称不能为空';
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  // 处理输入变化
  const handleInputChange = (field: keyof typeof formData, value: any) => {
    setFormData(prev => ({ ...prev, [field]: value }));
    
    // 清除相关错误
    if (errors[field]) {
      setErrors(prev => ({ ...prev, [field]: '' }));
    }
  };

  // 应用模型预设
  const applyModelPreset = (presetKey: string) => {
    const preset = modelPresets[presetKey];
    if (preset) {
      setFormData(prev => ({
        ...prev,
        ...preset,
        name: prev.name || preset.model || presetKey
      }));
    }
  };

  // 处理保存
  const handleSave = () => {
    if (validateForm()) {
      onSave(formData);
    }
  };

  return (
    <div className="modal-overlay" onClick={onCancel}>
      <div className="modal-content api-config-modal" onClick={e => e.stopPropagation()}>
        <div className="modal-header">
          <h3>{config ? '编辑API配置' : '添加API配置'}</h3>
          <button className="close-button" onClick={onCancel}>×</button>
        </div>

        <div className="modal-body">
          {/* 模型预设 */}
          <div className="form-group">
            <label>模型预设（可选）:</label>
            <div className="preset-buttons">
              {Object.keys(modelPresets).map(presetKey => (
                <button
                  key={presetKey}
                  type="button"
                  onClick={() => applyModelPreset(presetKey)}
                  className="preset-button"
                >
                  {presetKey}
                </button>
              ))}
            </div>
          </div>

          {/* 基本信息 */}
          <div className="form-group">
            <label>配置名称 *:</label>
            <input
              type="text"
              value={formData.name}
              onChange={e => handleInputChange('name', e.target.value)}
              placeholder="例如：GPT-4o 主配置"
              className={errors.name ? 'error' : ''}
            />
            {errors.name && <span className="error-text">{errors.name}</span>}
          </div>

          <div className="form-group">
            <label>API密钥 *:</label>
            <input
              type="password"
              value={formData.apiKey}
              onChange={e => handleInputChange('apiKey', e.target.value)}
              placeholder="sk-..."
              className={errors.apiKey ? 'error' : ''}
            />
            {errors.apiKey && <span className="error-text">{errors.apiKey}</span>}
          </div>

          <div className="form-group">
            <label>API地址 *:</label>
            <input
              type="url"
              value={formData.baseUrl}
              onChange={e => handleInputChange('baseUrl', e.target.value)}
              placeholder="https://api.openai.com/v1"
              className={errors.baseUrl ? 'error' : ''}
            />
            {errors.baseUrl && <span className="error-text">{errors.baseUrl}</span>}
          </div>

          <div className="form-group">
            <label>模型名称 *:</label>
            <input
              type="text"
              value={formData.model}
              onChange={e => handleInputChange('model', e.target.value)}
              placeholder="gpt-4o"
              className={errors.model ? 'error' : ''}
            />
            {errors.model && <span className="error-text">{errors.model}</span>}
          </div>

          <div className="form-group">
            <label>模型适配器:</label>
            <select
              value={formData.modelAdapter}
              onChange={e => handleInputChange('modelAdapter', e.target.value)}
            >
              {modelAdapters.map(adapter => (
                <option key={adapter.value} value={adapter.value}>
                  {adapter.label}
                </option>
              ))}
            </select>
          </div>

          {/* 模型特性 */}
          <div className="form-group">
            <div className="checkbox-group">
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={formData.isMultimodal}
                  onChange={e => handleInputChange('isMultimodal', e.target.checked)}
                />
                多模态模型（支持图片输入）
              </label>
              
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={formData.isReasoning}
                  onChange={e => handleInputChange('isReasoning', e.target.checked)}
                />
                推理模型（具有思维链能力）
              </label>
              
              <label className="checkbox-label">
                <input
                  type="checkbox"
                  checked={formData.enabled}
                  onChange={e => handleInputChange('enabled', e.target.checked)}
                />
                启用此配置
              </label>
            </div>
          </div>

          {/* 说明文本 */}
          <div className="help-section">
            <h4>配置说明：</h4>
            <ul>
              <li><strong>多模态模型</strong>：能够处理图片输入，适合用作第一模型（OCR+分类）</li>
              <li><strong>推理模型</strong>：具有思维链能力，适合复杂分析，如 o1 系列</li>
              <li><strong>第一模型</strong>：必须是多模态，用于图片识别和错题分类</li>
              <li><strong>第二模型</strong>：用于AI对话和深度分析，推理模型效果更好</li>
            </ul>
          </div>
        </div>

        <div className="modal-footer">
          <button onClick={onCancel} className="cancel-button">
            取消
          </button>
          <button onClick={handleSave} className="save-button">
            {config ? '更新配置' : '添加配置'}
          </button>
        </div>
      </div>
    </div>
  );
};