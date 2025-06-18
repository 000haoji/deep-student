/**
 * Custom hook for managing API configurations
 * Extracted from Settings component to reduce complexity
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';

// API配置接口
export interface ApiConfig {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isMultimodal: boolean;
  isReasoning: boolean;
  enabled: boolean;
  modelAdapter: string;
}

// 模型分配接口
export interface ModelAssignments {
  model1_config_id: string | null;
  model2_config_id: string | null;
  review_analysis_model_config_id: string | null;
  anki_card_model_config_id: string | null;
  embedding_model_config_id: string | null;
  reranker_model_config_id: string | null;
  summary_model_config_id: string | null; 
}

// 检查是否在Tauri环境中
const isTauri = typeof window !== 'undefined' && window.__TAURI_INTERNALS__;
const invoke = isTauri ? tauriInvoke : null;

export const useApiConfig = () => {
  const [apiConfigs, setApiConfigs] = useState<ApiConfig[]>([]);
  const [modelAssignments, setModelAssignments] = useState<ModelAssignments>({
    model1_config_id: null,
    model2_config_id: null,
    review_analysis_model_config_id: null,
    anki_card_model_config_id: null,
    embedding_model_config_id: null,
    reranker_model_config_id: null,
    summary_model_config_id: null,
  });
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testingApi, setTestingApi] = useState<string | null>(null);

  // 加载API配置
  const loadApiConfigs = useCallback(async () => {
    setLoading(true);
    try {
      if (invoke) {
        const [configs, assignments] = await Promise.all([
          invoke('get_api_configurations').catch(() => []) as Promise<ApiConfig[]>,
          invoke('get_model_assignments').catch(() => ({ 
            model1_config_id: null, 
            model2_config_id: null,
            review_analysis_model_config_id: null,
            anki_card_model_config_id: null,
            embedding_model_config_id: null,
            reranker_model_config_id: null,
            summary_model_config_id: null,
          })) as Promise<ModelAssignments>
        ]);

        setApiConfigs(configs || []);
        setModelAssignments(assignments || { 
            model1_config_id: null, 
            model2_config_id: null, 
            review_analysis_model_config_id: null,
            anki_card_model_config_id: null,
            embedding_model_config_id: null,
            reranker_model_config_id: null,
            summary_model_config_id: null,
        });
      }
    } catch (error) {
      console.error('加载API配置失败:', error);
    } finally {
      setLoading(false);
    }
  }, []);

  // 保存API配置
  const saveApiConfigs = useCallback(async (configs: ApiConfig[]) => {
    setSaving(true);
    try {
      if (invoke) {
        await invoke('save_api_configurations', { configs });
        setApiConfigs(configs);
        return true;
      }
      return false;
    } catch (error) {
      console.error('保存API配置失败:', error);
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  // 保存模型分配
  const saveModelAssignments = useCallback(async (assignments: ModelAssignments) => {
    setSaving(true);
    try {
      if (invoke) {
        await invoke('save_model_assignments', { assignments });
        setModelAssignments(assignments);
        return true;
      }
      return false;
    } catch (error) {
      console.error('保存模型分配失败:', error);
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  // 测试API连接
  const testApiConnection = useCallback(async (config: ApiConfig) => {
    setTestingApi(config.id);
    try {
      if (invoke) {
        const result = await invoke('test_api_connection', {
          apiKey: config.apiKey,
          baseUrl: config.baseUrl
        }) as boolean;
        return result;
      }
      return false;
    } catch (error) {
      console.error('API连接测试失败:', error);
      return false;
    } finally {
      setTestingApi(null);
    }
  }, []);

  // 添加API配置
  const addApiConfig = useCallback((config: Omit<ApiConfig, 'id'>) => {
    const newConfig: ApiConfig = {
      ...config,
      id: Date.now().toString()
    };
    const updatedConfigs = [...apiConfigs, newConfig];
    setApiConfigs(updatedConfigs);
    return newConfig;
  }, [apiConfigs]);

  // 更新API配置
  const updateApiConfig = useCallback((id: string, updates: Partial<ApiConfig>) => {
    const updatedConfigs = apiConfigs.map(config => 
      config.id === id ? { ...config, ...updates } : config
    );
    setApiConfigs(updatedConfigs);
    return updatedConfigs.find(c => c.id === id);
  }, [apiConfigs]);

  // 删除API配置
  const deleteApiConfig = useCallback((id: string) => {
    const updatedConfigs = apiConfigs.filter(config => config.id !== id);
    setApiConfigs(updatedConfigs);
    
    // 如果删除的配置被分配给模型，清除分配
    const updatedAssignments = { ...modelAssignments };
    if (modelAssignments.model1_config_id === id) {
      updatedAssignments.model1_config_id = null;
    }
    if (modelAssignments.model2_config_id === id) {
      updatedAssignments.model2_config_id = null;
    }
    
    if (updatedAssignments.model1_config_id !== modelAssignments.model1_config_id ||
        updatedAssignments.model2_config_id !== modelAssignments.model2_config_id) {
      setModelAssignments(updatedAssignments);
    }
    
    return updatedConfigs;
  }, [apiConfigs, modelAssignments]);

  // 获取多模态配置（用于模型1）
  const getMultimodalConfigs = useCallback(() => {
    return apiConfigs.filter(config => config.isMultimodal && config.enabled);
  }, [apiConfigs]);

  // 获取所有启用的配置（用于模型2）
  const getEnabledConfigs = useCallback(() => {
    return apiConfigs.filter(config => config.enabled);
  }, [apiConfigs]);

  // 获取配置按ID
  const getConfigById = useCallback((id: string | null) => {
    if (!id) return null;
    return apiConfigs.find(config => config.id === id) || null;
  }, [apiConfigs]);

  // 验证模型分配
  const validateModelAssignments = useCallback(() => {
    const errors: string[] = [];
    
    if (modelAssignments.model1_config_id) {
      const model1Config = getConfigById(modelAssignments.model1_config_id);
      if (!model1Config) {
        errors.push('模型1配置不存在');
      } else if (!model1Config.isMultimodal) {
        errors.push('模型1必须是多模态配置');
      } else if (!model1Config.enabled) {
        errors.push('模型1配置未启用');
      }
    }
    
    if (modelAssignments.model2_config_id) {
      const model2Config = getConfigById(modelAssignments.model2_config_id);
      if (!model2Config) {
        errors.push('模型2配置不存在');
      } else if (!model2Config.enabled) {
        errors.push('模型2配置未启用');
      }
    }
    
    return errors;
  }, [modelAssignments, getConfigById]);

  // 初始化时加载配置
  useEffect(() => {
    loadApiConfigs();
  }, [loadApiConfigs]);

  return {
    // 状态
    apiConfigs,
    modelAssignments,
    loading,
    saving,
    testingApi,
    
    // 方法
    loadApiConfigs,
    saveApiConfigs,
    saveModelAssignments,
    testApiConnection,
    addApiConfig,
    updateApiConfig,
    deleteApiConfig,
    
    // 便利方法
    getMultimodalConfigs,
    getEnabledConfigs,
    getConfigById,
    validateModelAssignments
  };
};
