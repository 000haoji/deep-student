/**
 * API Configuration Section Component
 * Split from the large Settings component
 */

import React, { useState } from 'react';
import { useApiConfig, ApiConfig } from '../../hooks/useApiConfig';
import { useNotification } from '../../hooks/useNotification';
import { ApiConfigModal } from './ApiConfigModal';

export const ApiConfigSection: React.FC = () => {
  const {
    apiConfigs,
    modelAssignments,
    loading,
    saving,
    testingApi,
    saveApiConfigs,
    saveModelAssignments,
    testApiConnection,
    addApiConfig,
    updateApiConfig,
    deleteApiConfig,
    getMultimodalConfigs,
    getEnabledConfigs,
    validateModelAssignments
  } = useApiConfig();

  const { showSuccess, showError, showWarning } = useNotification();
  const [editingApi, setEditingApi] = useState<ApiConfig | null>(null);
  const [showModal, setShowModal] = useState(false);

  // 保存配置
  const handleSaveConfigs = async () => {
    const success = await saveApiConfigs(apiConfigs);
    if (success) {
      showSuccess('API配置保存成功');
    } else {
      showError('API配置保存失败');
    }
  };

  // 保存模型分配
  const handleSaveModelAssignments = async () => {
    const errors = validateModelAssignments();
    if (errors.length > 0) {
      showWarning(`模型分配验证失败: ${errors.join(', ')}`);
      return;
    }

    const success = await saveModelAssignments(modelAssignments);
    if (success) {
      showSuccess('模型分配保存成功');
    } else {
      showError('模型分配保存失败');
    }
  };

  // 测试API连接
  const handleTestApi = async (config: ApiConfig) => {
    const success = await testApiConnection(config);
    if (success) {
      showSuccess(`${config.name} 连接测试成功`);
    } else {
      showError(`${config.name} 连接测试失败`);
    }
  };

  // 添加新API配置
  const handleAddApi = () => {
    setEditingApi(null);
    setShowModal(true);
  };

  // 编辑API配置
  const handleEditApi = (config: ApiConfig) => {
    setEditingApi(config);
    setShowModal(true);
  };

  // 保存API配置（从模态框）
  const handleSaveApi = async (apiData: Omit<ApiConfig, 'id'>) => {
    if (editingApi) {
      updateApiConfig(editingApi.id, apiData);
      showSuccess('API配置已更新');
    } else {
      addApiConfig(apiData);
      showSuccess('API配置已添加');
    }
    setShowModal(false);
    setEditingApi(null);
  };

  // 删除API配置
  const handleDeleteApi = async (config: ApiConfig) => {
    if (window.confirm(`确定要删除 ${config.name} 配置吗？`)) {
      deleteApiConfig(config.id);
      showSuccess('API配置已删除');
    }
  };

  if (loading) {
    return <div className="loading">加载API配置中...</div>;
  }

  const multimodalConfigs = getMultimodalConfigs();
  const enabledConfigs = getEnabledConfigs();

  return (
    <div className="api-config-section">
      <div className="section-header">
        <h3>API配置管理</h3>
        <button 
          onClick={handleAddApi}
          className="add-api-button"
          disabled={saving}
        >
          + 添加API配置
        </button>
      </div>

      {/* API配置列表 */}
      <div className="api-configs-list">
        {apiConfigs.length === 0 ? (
          <div className="empty-state">
            <p>暂无API配置，请添加至少一个配置</p>
          </div>
        ) : (
          apiConfigs.map(config => (
            <div key={config.id} className={`api-config-item ${!config.enabled ? 'disabled' : ''}`}>
              <div className="config-info">
                <div className="config-header">
                  <h4>{config.name}</h4>
                  <div className="config-badges">
                    {config.isMultimodal && <span className="badge multimodal">多模态</span>}
                    {config.isReasoning && <span className="badge reasoning">推理模型</span>}
                    {!config.enabled && <span className="badge disabled">已禁用</span>}
                  </div>
                </div>
                <div className="config-details">
                  <p><strong>模型:</strong> {config.model}</p>
                  <p><strong>适配器:</strong> {config.modelAdapter}</p>
                  <p><strong>API地址:</strong> {config.baseUrl}</p>
                </div>
              </div>
              
              <div className="config-actions">
                <button 
                  onClick={() => handleTestApi(config)}
                  disabled={testingApi === config.id || !config.enabled}
                  className="test-button"
                >
                  {testingApi === config.id ? '测试中...' : '测试连接'}
                </button>
                
                <button 
                  onClick={() => handleEditApi(config)}
                  className="edit-button"
                >
                  编辑
                </button>
                
                <button 
                  onClick={() => handleDeleteApi(config)}
                  className="delete-button"
                  disabled={saving}
                >
                  删除
                </button>
              </div>
            </div>
          ))
        )}
      </div>

      {/* 模型分配 */}
      <div className="model-assignments">
        <h4>模型分配</h4>
        <div className="assignment-grid">
          <div className="assignment-item">
            <label>第一模型（OCR+分类）:</label>
            <select 
              value={modelAssignments.model1_config_id || ''}
              onChange={(e) => saveModelAssignments({
                ...modelAssignments,
                model1_config_id: e.target.value || null
              })}
              disabled={saving}
            >
              <option value="">请选择...</option>
              {multimodalConfigs.map(config => (
                <option key={config.id} value={config.id}>
                  {config.name} - {config.model}
                </option>
              ))}
            </select>
            <p className="help-text">必须是多模态模型，用于图片识别和分类</p>
          </div>

          <div className="assignment-item">
            <label>第二模型（对话分析）:</label>
            <select 
              value={modelAssignments.model2_config_id || ''}
              onChange={(e) => saveModelAssignments({
                ...modelAssignments,
                model2_config_id: e.target.value || null
              })}
              disabled={saving}
            >
              <option value="">请选择...</option>
              {enabledConfigs.map(config => (
                <option key={config.id} value={config.id}>
                  {config.name} - {config.model}
                  {config.isReasoning && ' (推理模型)'}
                </option>
              ))}
            </select>
            <p className="help-text">用于AI对话和深度分析，推理模型效果更好</p>
          </div>

          <div className="assignment-item">
            <label>回顾分析模型（多题统一分析）:</label>
            <select 
              value={modelAssignments.review_analysis_model_config_id || ''}
              onChange={(e) => saveModelAssignments({
                ...modelAssignments,
                review_analysis_model_config_id: e.target.value || null
              })}
              disabled={saving}
            >
              <option value="">请选择...</option>
              {enabledConfigs.map(config => (
                <option key={config.id} value={config.id}>
                  {config.name} - {config.model}
                  {config.isReasoning && ' (推理模型)'}
                </option>
              ))}
            </select>
            <p className="help-text">用于回顾分析功能，对多个错题进行统一深度分析，建议使用强大的推理模型</p>
          </div>

          <div className="assignment-item">
            <label>ANKI制卡模型（卡片生成）:</label>
            <select 
              value={modelAssignments.anki_card_model_config_id || ''}
              onChange={(e) => saveModelAssignments({
                ...modelAssignments,
                anki_card_model_config_id: e.target.value || null
              })}
              disabled={saving}
            >
              <option value="">请选择...</option>
              {enabledConfigs.map(config => (
                <option key={config.id} value={config.id}>
                  {config.name} - {config.model}
                  {config.isReasoning && ' (推理模型)'}
                </option>
              ))}
            </select>
            <p className="help-text">用于ANKI卡片生成功能，根据学习内容智能生成问答卡片</p>
          </div>

          <div className="assignment-item">
            <label>总结生成模型（错题总结）:</label>
            <select 
              value={modelAssignments.summary_model_config_id || ''}
              onChange={(e) => saveModelAssignments({
                ...modelAssignments,
                summary_model_config_id: e.target.value || null
              })}
              disabled={saving}
            >
              <option value="">请选择（默认使用对话分析模型）...</option>
              {enabledConfigs.map(config => (
                <option key={config.id} value={config.id}>
                  {config.name} - {config.model}
                  {config.isReasoning && ' (推理模型)'}
                </option>
              ))}
            </select>
            <p className="help-text">专门用于生成错题总结，建议使用理解和概括能力强的模型。如果未选择，将使用“第二模型（对话分析）”。</p>
          </div>
        </div>
      </div>

      {/* 保存按钮 */}
      <div className="section-actions">
        <button 
          onClick={handleSaveConfigs}
          disabled={saving}
          className="save-button primary"
        >
          {saving ? '保存中...' : '保存API配置'}
        </button>
        
        <button 
          onClick={handleSaveModelAssignments}
          disabled={saving}
          className="save-button secondary"
        >
          {saving ? '保存中...' : '保存模型分配'}
        </button>
      </div>

      {/* API配置模态框 */}
      {showModal && (
        <ApiConfigModal
          config={editingApi}
          onSave={handleSaveApi}
          onCancel={() => {
            setShowModal(false);
            setEditingApi(null);
          }}
        />
      )}
    </div>
  );
};
