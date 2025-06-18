/**
 * Custom hook for managing system settings
 * Extracted from Settings component to reduce complexity
 */

import { useState, useEffect, useCallback } from 'react';
import { invoke as tauriInvoke } from '@tauri-apps/api/core';

// 系统设置接口
export interface SystemSettings {
  autoSave: boolean;
  theme: string;
  language: string;
  enableNotifications: boolean;
  maxChatHistory: number;
  debugMode: boolean;
  enableAnkiConnect: boolean;
}

// 检查是否在Tauri环境中
const isTauri = typeof window !== 'undefined' && window.__TAURI_INTERNALS__;
const invoke = isTauri ? tauriInvoke : null;

// 默认设置
const DEFAULT_SETTINGS: SystemSettings = {
  autoSave: true,
  theme: 'light',
  language: 'zh-CN',
  enableNotifications: true,
  maxChatHistory: 100,
  debugMode: false,
  enableAnkiConnect: true
};

export const useSystemSettings = () => {
  const [settings, setSettings] = useState<SystemSettings>(DEFAULT_SETTINGS);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  // 加载系统设置
  const loadSettings = useCallback(async () => {
    setLoading(true);
    try {
      if (invoke) {
        const settingsPromises = Object.keys(DEFAULT_SETTINGS).map(async (key) => {
          try {
            const value = await invoke('get_setting', { key }) as string;
            return { key, value };
          } catch {
            return { key, value: String(DEFAULT_SETTINGS[key as keyof SystemSettings]) };
          }
        });

        const settingsResults = await Promise.all(settingsPromises);
        const loadedSettings: SystemSettings = { ...DEFAULT_SETTINGS };

        for (const { key, value } of settingsResults) {
          const settingKey = key as keyof SystemSettings;
          
          // 类型转换
          switch (settingKey) {
            case 'autoSave':
            case 'enableNotifications':
            case 'debugMode':
              loadedSettings[settingKey] = value === 'true';
              break;
            case 'maxChatHistory':
              loadedSettings[settingKey] = parseInt(value, 10) || DEFAULT_SETTINGS[settingKey];
              break;
            default:
              (loadedSettings as any)[settingKey] = value;
          }
        }

        setSettings(loadedSettings);
      }
    } catch (error) {
      console.error('加载系统设置失败:', error);
      setSettings(DEFAULT_SETTINGS);
    } finally {
      setLoading(false);
    }
  }, []);

  // 保存单个设置
  const saveSetting = useCallback(async (key: keyof SystemSettings, value: any) => {
    setSaving(true);
    try {
      if (invoke) {
        await invoke('save_setting', { key, value: String(value) });
        setSettings(prev => ({ ...prev, [key]: value }));
        return true;
      }
      return false;
    } catch (error) {
      console.error(`保存设置 ${key} 失败:`, error);
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  // 保存所有设置
  const saveAllSettings = useCallback(async (newSettings: SystemSettings) => {
    setSaving(true);
    try {
      if (invoke) {
        const savePromises = Object.entries(newSettings).map(([key, value]) =>
          invoke('save_setting', { key, value: String(value) })
        );
        
        await Promise.all(savePromises);
        setSettings(newSettings);
        return true;
      }
      return false;
    } catch (error) {
      console.error('保存系统设置失败:', error);
      return false;
    } finally {
      setSaving(false);
    }
  }, []);

  // 重置设置为默认值
  const resetSettings = useCallback(async () => {
    return await saveAllSettings(DEFAULT_SETTINGS);
  }, [saveAllSettings]);

  // 更新设置（本地状态，不保存）
  const updateSetting = useCallback(<K extends keyof SystemSettings>(
    key: K, 
    value: SystemSettings[K]
  ) => {
    setSettings(prev => ({ ...prev, [key]: value }));
  }, []);

  // 批量更新设置（本地状态，不保存）
  const updateSettings = useCallback((updates: Partial<SystemSettings>) => {
    setSettings(prev => ({ ...prev, ...updates }));
  }, []);

  // 应用主题
  const applyTheme = useCallback(async (theme: string) => {
    const success = await saveSetting('theme', theme);
    if (success) {
      // 应用主题到DOM
      document.documentElement.setAttribute('data-theme', theme);
      document.body.className = theme === 'dark' ? 'dark-theme' : 'light-theme';
    }
    return success;
  }, [saveSetting]);

  // 验证设置
  const validateSettings = useCallback((settingsToValidate: Partial<SystemSettings>) => {
    const errors: string[] = [];
    
    if (settingsToValidate.maxChatHistory !== undefined) {
      if (settingsToValidate.maxChatHistory < 10 || settingsToValidate.maxChatHistory > 1000) {
        errors.push('最大聊天历史记录数量必须在10-1000之间');
      }
    }
    
    if (settingsToValidate.theme !== undefined) {
      if (!['light', 'dark', 'auto'].includes(settingsToValidate.theme)) {
        errors.push('主题必须是light、dark或auto');
      }
    }
    
    if (settingsToValidate.language !== undefined) {
      if (!['zh-CN', 'en-US'].includes(settingsToValidate.language)) {
        errors.push('语言必须是zh-CN或en-US');
      }
    }
    
    return errors;
  }, []);

  // 获取设置摘要（用于显示）
  const getSettingsSummary = useCallback(() => {
    return {
      configuredItems: Object.keys(settings).length,
      autoSaveEnabled: settings.autoSave,
      currentTheme: settings.theme,
      debugModeEnabled: settings.debugMode,
      maxHistorySize: settings.maxChatHistory
    };
  }, [settings]);

  // 初始化时加载设置
  useEffect(() => {
    loadSettings();
  }, [loadSettings]);

  // 应用主题到DOM
  useEffect(() => {
    if (!loading) {
      document.documentElement.setAttribute('data-theme', settings.theme);
      document.body.className = settings.theme === 'dark' ? 'dark-theme' : 'light-theme';
    }
  }, [settings.theme, loading]);

  return {
    // 状态
    settings,
    loading,
    saving,
    
    // 方法
    loadSettings,
    saveSetting,
    saveAllSettings,
    resetSettings,
    updateSetting,
    updateSettings,
    applyTheme,
    validateSettings,
    getSettingsSummary,
    
    // 便利属性
    isAutoSaveEnabled: settings.autoSave,
    isDarkTheme: settings.theme === 'dark',
    isDebugMode: settings.debugMode
  };
};