/**
 * System Settings Section Component
 * Split from the large Settings component
 */

import React from 'react';
import { useSystemSettings } from '../../hooks/useSystemSettings';
import { useNotification } from '../../hooks/useNotification';
import { Settings as SettingsIcon, CheckCircle, XCircle } from 'lucide-react';

export const SystemSettingsSection: React.FC = () => {
  const {
    settings,
    loading,
    saving,
    saveSetting,
    saveAllSettings,
    resetSettings,
    updateSetting,
    validateSettings,
    getSettingsSummary,
    isAutoSaveEnabled,
    isDarkTheme: _isDarkTheme,
    isDebugMode
  } = useSystemSettings();

  const { showSuccess, showError, showWarning } = useNotification();

  // 处理设置变更
  const handleSettingChange = async <K extends keyof typeof settings>(
    key: K,
    value: typeof settings[K],
    autoSave: boolean = true
  ) => {
    updateSetting(key, value);
    
    if (autoSave && isAutoSaveEnabled) {
      const success = await saveSetting(key, value);
      if (!success) {
        showError(`保存 ${key} 设置失败`);
      }
    }
  };

  // 手动保存所有设置
  const handleSaveAll = async () => {
    const errors = validateSettings(settings);
    if (errors.length > 0) {
      showWarning(`设置验证失败: ${errors.join(', ')}`);
      return;
    }

    const success = await saveAllSettings(settings);
    if (success) {
      showSuccess('所有设置保存成功');
    } else {
      showError('保存设置失败');
    }
  };

  // 重置所有设置
  const handleReset = async () => {
    if (window.confirm('确定要重置所有设置为默认值吗？此操作不可恢复。')) {
      const success = await resetSettings();
      if (success) {
        showSuccess('设置已重置为默认值');
      } else {
        showError('重置设置失败');
      }
    }
  };

  if (loading) {
    return <div className="loading">加载系统设置中...</div>;
  }

  const summary = getSettingsSummary();

  return (
    <div className="system-settings-section">
      <div className="section-header">
        <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
          <SettingsIcon size={24} color="#4a5568" />
          <h3>系统设置</h3>
        </div>
        <div className="settings-summary">
          <span>主题: {settings.theme}</span>
          <span>自动保存: {isAutoSaveEnabled ? '开启' : '关闭'}</span>
          {isDebugMode && <span className="debug-indicator">调试模式</span>}
        </div>
      </div>

      <div className="settings-grid">
        {/* 界面设置 */}
        <div className="settings-group">
          <h4>界面设置</h4>
          
          <div className="setting-item">
            <label>主题:</label>
            <select
              value={settings.theme}
              onChange={e => handleSettingChange('theme', e.target.value)}
              disabled={saving}
            >
              <option value="light">浅色主题</option>
              <option value="dark">深色主题</option>
              <option value="auto">跟随系统</option>
            </select>
          </div>

          <div className="setting-item">
            <label>语言:</label>
            <select
              value={settings.language}
              onChange={e => handleSettingChange('language', e.target.value)}
              disabled={saving}
            >
              <option value="zh-CN">简体中文</option>
              <option value="en-US">English</option>
            </select>
          </div>
        </div>

        {/* 功能设置 */}
        <div className="settings-group">
          <h4>功能设置</h4>
          
          <div className="setting-item">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.autoSave}
                onChange={e => handleSettingChange('autoSave', e.target.checked)}
                disabled={saving}
              />
              自动保存配置
            </label>
            <p className="help-text">更改后自动保存，无需手动保存</p>
          </div>

          <div className="setting-item">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.enableNotifications}
                onChange={e => handleSettingChange('enableNotifications', e.target.checked)}
                disabled={saving}
              />
              启用通知
            </label>
            <p className="help-text">显示操作成功/失败的通知消息</p>
          </div>

          <div className="setting-item">
            <label>最大聊天历史记录数:</label>
            <input
              type="number"
              min="10"
              max="1000"
              value={settings.maxChatHistory}
              onChange={e => handleSettingChange('maxChatHistory', parseInt(e.target.value, 10))}
              disabled={saving}
            />
            <p className="help-text">每个对话保留的最大消息数量 (10-1000)</p>
          </div>
        </div>

        {/* 高级设置 */}
        <div className="settings-group">
          <h4>高级设置</h4>
          
          <div className="setting-item">
            <label className="checkbox-label">
              <input
                type="checkbox"
                checked={settings.debugMode}
                onChange={e => handleSettingChange('debugMode', e.target.checked)}
                disabled={saving}
              />
              调试模式
            </label>
            <p className="help-text">显示详细的调试信息和日志</p>
          </div>

          {isDebugMode && (
            <div className="debug-info">
              <h5>调试信息:</h5>
              <pre>{JSON.stringify(summary, null, 2)}</pre>
            </div>
          )}
        </div>
      </div>

      {/* 操作按钮 */}
      <div className="settings-actions">
        {!isAutoSaveEnabled && (
          <button
            onClick={handleSaveAll}
            disabled={saving}
            className="save-button primary"
          >
            {saving ? '保存中...' : '保存所有设置'}
          </button>
        )}
        
        <button
          onClick={handleReset}
          disabled={saving}
          className="reset-button secondary"
        >
          重置为默认值
        </button>
      </div>

      {/* 设置状态 */}
      <div className="settings-status">
        <div className="status-item">
          <span>配置项数量: {summary.configuredItems}</span>
        </div>
        <div className="status-item">
          <span>自动保存: {summary.autoSaveEnabled ? <CheckCircle size={16} color="#28a745" /> : <XCircle size={16} color="#dc3545" />}</span>
        </div>
        <div className="status-item">
          <span>当前主题: {summary.currentTheme}</span>
        </div>
        <div className="status-item">
          <span>最大历史: {summary.maxHistorySize}</span>
        </div>
        {summary.debugModeEnabled && (
          <div className="status-item debug">
            <span>调试模式: 已启用</span>
          </div>
        )}
      </div>
    </div>
  );
};