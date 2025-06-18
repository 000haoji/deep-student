import React, { useState, useEffect } from 'react';
import { CustomAnkiTemplate, CreateTemplateRequest, FieldExtractionRule } from '../types';
import { templateManager } from '../data/ankiTemplates';
import { IframePreview, renderCardPreview } from './SharedPreview';
import './TemplateManagementPage.css';

interface TemplateManagementPageProps {
  isSelectingMode?: boolean;
  onTemplateSelected?: (template: CustomAnkiTemplate) => void;
  onCancel?: () => void;
}

const TemplateManagementPage: React.FC<TemplateManagementPageProps> = ({ 
  isSelectingMode = false, 
  onTemplateSelected, 
  onCancel 
}) => {
  const [templates, setTemplates] = useState<CustomAnkiTemplate[]>([]);
  const [activeTab, setActiveTab] = useState<'browse' | 'edit' | 'create'>('browse');
  const [selectedTemplate, setSelectedTemplate] = useState<CustomAnkiTemplate | null>(null);
  const [editingTemplate, setEditingTemplate] = useState<CustomAnkiTemplate | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');
  const [defaultTemplateId, setDefaultTemplateId] = useState<string | null>(null);

  // 加载模板
  useEffect(() => {
    loadTemplates();
    loadDefaultTemplateId();
    
    // 订阅模板变化
    const unsubscribe = templateManager.subscribe(setTemplates);
    return unsubscribe;
  }, []);

  const loadDefaultTemplateId = async () => {
    try {
      await templateManager.loadUserDefaultTemplate();
      setDefaultTemplateId(templateManager.getDefaultTemplateId());
    } catch (err) {
      console.warn('Failed to load default template ID:', err);
    }
  };

  const loadTemplates = async () => {
    setIsLoading(true);
    try {
      await templateManager.refresh();
      setTemplates(templateManager.getAllTemplates());
    } catch (err) {
      setError(`加载模板失败: ${err}`);
    } finally {
      setIsLoading(false);
    }
  };

  // 过滤模板
  const filteredTemplates = templates.filter(template =>
    template.name.toLowerCase().includes(searchTerm.toLowerCase()) ||
    template.description.toLowerCase().includes(searchTerm.toLowerCase())
  );

  // 选择模板
  const handleSelectTemplate = (template: CustomAnkiTemplate) => {
    setSelectedTemplate(template);
  };

  // 设置默认模板
  const handleSetDefaultTemplate = async (template: CustomAnkiTemplate) => {
    try {
      await templateManager.setDefaultTemplate(template.id);
      setDefaultTemplateId(template.id); // 立即更新本地状态
      setError(null);
      console.log(`✅ 已将"${template.name}"设置为默认模板`);
    } catch (err) {
      setError(`设置默认模板失败: ${err}`);
    }
  };

  // 编辑模板
  const handleEditTemplate = (template: CustomAnkiTemplate) => {
    setEditingTemplate({ ...template });
    setActiveTab('edit');
  };

  // 复制模板
  const handleDuplicateTemplate = (template: CustomAnkiTemplate) => {
    const duplicated: CustomAnkiTemplate = {
      ...template,
      id: `${template.id}-copy-${Date.now()}`,
      name: `${template.name} - 副本`,
      author: '用户创建',
      is_built_in: false,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString()
    };
    setEditingTemplate(duplicated);
    setActiveTab('create');
  };

  // 使用统一的预览渲染函数
  const renderTemplatePreview = (template: string, templateData: CustomAnkiTemplate) => {
    return renderCardPreview(template, templateData);
  };

  // 删除模板
  const handleDeleteTemplate = async (template: CustomAnkiTemplate) => {
    if (template.is_built_in) {
      setError('不能删除内置模板');
      return;
    }

    if (!confirm(`确定要删除模板 "${template.name}" 吗？此操作不可撤销。`)) {
      return;
    }

    try {
      await templateManager.deleteTemplate(template.id);
      setError(null);
    } catch (err) {
      setError(`删除模板失败: ${err}`);
    }
  };

  return (
    <>
      {/* 页面头部 */}
      <div className={`page-header ${isSelectingMode ? 'selecting-mode' : 'management-mode'}`}>
        <div className="header-content">
          <div className="header-main">
            {isSelectingMode && onCancel && (
              <button className="back-button" onClick={onCancel}>
                <span className="back-icon">←</span>
                返回
              </button>
            )}
            <div className="title-section">
              <h1 className="page-title">
                <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
                  <path d="M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z" />
                  <circle cx="13.5" cy="6.5" r=".5" fill="currentColor" />
                  <circle cx="17.5" cy="10.5" r=".5" fill="currentColor" />
                  <circle cx="6.5" cy="12.5" r=".5" fill="currentColor" />
                  <circle cx="8.5" cy="7.5" r=".5" fill="currentColor" />
                </svg>
                {isSelectingMode ? '选择模板' : '模板管理'}
              </h1>
              <div className={`mode-indicator ${isSelectingMode ? 'selecting' : 'management'}`}>
                <span className="mode-icon">{isSelectingMode ? '🎯' : '⚙️'}</span>
                <span className="mode-text">
                  {isSelectingMode ? '选择模式' : '管理模式'}
                </span>
              </div>
            </div>
          </div>
          <p className="page-description">
            {isSelectingMode 
              ? '请选择一个模板用于生成ANKI卡片，点击"使用此模板"按钮完成选择' 
              : '管理和编辑ANKI卡片模板，自定义卡片样式和字段，创建符合需求的模板'
            }
          </p>
        </div>
      </div>

      {/* 标签页导航 */}
      {!isSelectingMode && (
        <div className="page-tabs">
          <button 
            className={`tab-button ${activeTab === 'browse' ? 'active' : ''}`}
            onClick={() => setActiveTab('browse')}
          >
            <span className="tab-icon">📋</span>
            浏览模板
          </button>
          <button 
            className={`tab-button ${activeTab === 'create' ? 'active' : ''}`}
            onClick={() => setActiveTab('create')}
          >
            <span className="tab-icon">➕</span>
            创建模板
          </button>
          {editingTemplate && (
            <button 
              className={`tab-button ${activeTab === 'edit' ? 'active' : ''}`}
              onClick={() => setActiveTab('edit')}
            >
              <span className="tab-icon">✏️</span>
              编辑模板
            </button>
          )}
        </div>
      )}

      {/* 错误提示 */}
      {error && (
        <div className="error-alert">
          <div className="alert-content">
            <span className="alert-icon">⚠️</span>
            <span className="alert-message">{error}</span>
            <button className="alert-close" onClick={() => setError(null)}>✕</button>
          </div>
        </div>
      )}

      {/* 内容区域 */}
      <div className="page-content">
        {(isSelectingMode || activeTab === 'browse') && (
          <TemplateBrowser
            templates={filteredTemplates}
            selectedTemplate={selectedTemplate}
            searchTerm={searchTerm}
            onSearchChange={setSearchTerm}
            onSelectTemplate={handleSelectTemplate}
            onEditTemplate={handleEditTemplate}
            onDuplicateTemplate={handleDuplicateTemplate}
            onDeleteTemplate={handleDeleteTemplate}
            onSetDefaultTemplate={handleSetDefaultTemplate}
            defaultTemplateId={defaultTemplateId}
            isLoading={isLoading}
            isSelectingMode={isSelectingMode}
            onTemplateSelected={onTemplateSelected}
            renderPreview={renderTemplatePreview}
          />
        )}

        {!isSelectingMode && activeTab === 'create' && (
          <TemplateEditor
            template={editingTemplate}
            mode="create"
            onSave={async (templateData) => {
              try {
                await templateManager.createTemplate(templateData);
                setActiveTab('browse');
                setEditingTemplate(null);
                setError(null);
              } catch (err) {
                setError(`创建模板失败: ${err}`);
              }
            }}
            onCancel={() => {
              setActiveTab('browse');
              setEditingTemplate(null);
            }}
          />
        )}

        {!isSelectingMode && activeTab === 'edit' && editingTemplate && (
          <TemplateEditor
            template={editingTemplate}
            mode="edit"
            onSave={async (_templateData) => {
              try {
                // TODO: 实现模板更新
                setActiveTab('browse');
                setEditingTemplate(null);
                setError(null);
              } catch (err) {
                setError(`更新模板失败: ${err}`);
              }
            }}
            onCancel={() => {
              setActiveTab('browse');
              setEditingTemplate(null);
            }}
          />
        )}
      </div>
    </>
  );
};

// 模板浏览器组件
interface TemplateBrowserProps {
  templates: CustomAnkiTemplate[];
  selectedTemplate: CustomAnkiTemplate | null;
  searchTerm: string;
  onSearchChange: (term: string) => void;
  onSelectTemplate: (template: CustomAnkiTemplate) => void;
  onEditTemplate: (template: CustomAnkiTemplate) => void;
  onDuplicateTemplate: (template: CustomAnkiTemplate) => void;
  onDeleteTemplate: (template: CustomAnkiTemplate) => void;
  onSetDefaultTemplate: (template: CustomAnkiTemplate) => void;
  defaultTemplateId: string | null;
  isLoading: boolean;
  isSelectingMode?: boolean;
  onTemplateSelected?: (template: CustomAnkiTemplate) => void;
  renderPreview: (template: string, templateData: CustomAnkiTemplate) => string;
}

const TemplateBrowser: React.FC<TemplateBrowserProps> = ({
  templates,
  selectedTemplate,
  searchTerm,
  onSearchChange,
  onSelectTemplate,
  onEditTemplate,
  onDuplicateTemplate,
  onDeleteTemplate,
  onSetDefaultTemplate,
  defaultTemplateId,
  isLoading,
  isSelectingMode = false,
  onTemplateSelected,
  renderPreview
}) => {
  return (
    <div className="template-browser">
      {/* 搜索和工具栏 */}
      <div className="browser-toolbar">
        <div className="search-container">
          <div className="search-input-wrapper">
            <span className="search-icon">🔍</span>
            <input
              type="text"
              placeholder="搜索模板名称或描述..."
              value={searchTerm}
              onChange={(e) => onSearchChange(e.target.value)}
              className="search-input"
            />
          </div>
        </div>
        <div className="toolbar-stats">
          <span className="stats-text">共 {templates.length} 个模板</span>
          {isSelectingMode && (
            <div className="mode-hint">
              <span className="hint-icon">💡</span>
              <span className="hint-text">点击"使用此模板"选择模板</span>
            </div>
          )}
        </div>
      </div>

      {/* 模板网格 */}
      {isLoading ? (
        <div className="loading-container">
          <div className="loading-spinner"></div>
          <span className="loading-text">加载模板中...</span>
        </div>
      ) : (
        <div className="templates-grid">
          {templates.map(template => (
            <TemplateCard
              key={template.id}
              template={template}
              isSelected={selectedTemplate?.id === template.id}
              onSelect={() => onSelectTemplate(template)}
              onEdit={() => onEditTemplate(template)}
              onDuplicate={() => onDuplicateTemplate(template)}
              onDelete={() => onDeleteTemplate(template)}
              onSetDefaultTemplate={() => onSetDefaultTemplate(template)}
              defaultTemplateId={defaultTemplateId}
              isSelectingMode={isSelectingMode}
              onTemplateSelected={onTemplateSelected}
              renderPreview={renderPreview}
            />
          ))}
        </div>
      )}

      {templates.length === 0 && !isLoading && (
        <div className="empty-state">
          <div className="empty-icon">📝</div>
          <h3 className="empty-title">没有找到模板</h3>
          <p className="empty-description">试试调整搜索条件，或创建一个新模板。</p>
        </div>
      )}
    </div>
  );
};

// 模板卡片组件
interface TemplateCardProps {
  template: CustomAnkiTemplate;
  isSelected: boolean;
  onSelect: () => void;
  onEdit: () => void;
  onDuplicate: () => void;
  onDelete: () => void;
  onSetDefaultTemplate: () => void;
  defaultTemplateId: string | null;
  isSelectingMode?: boolean;
  onTemplateSelected?: (template: CustomAnkiTemplate) => void;
  renderPreview: (template: string, templateData: CustomAnkiTemplate) => string;
}

const TemplateCard: React.FC<TemplateCardProps> = ({
  template,
  isSelected,
  onSelect,
  onEdit,
  onDuplicate,
  onDelete,
  onSetDefaultTemplate,
  defaultTemplateId,
  isSelectingMode = false,
  onTemplateSelected,
  renderPreview
}) => {
  // 检查是否为默认模板
  const isDefault = defaultTemplateId === template.id;

  return (
    <div className={`template-card template-card-${template.id.replace(/[^a-zA-Z0-9]/g, '_')} ${isSelected ? 'selected' : ''} ${!template.is_active ? 'inactive' : ''} ${isSelectingMode ? 'selecting-mode' : 'management-mode'} ${isDefault ? 'default-template' : ''}`}>
      {/* 卡片头部 */}
      <div className="card-header">
        <h4 className="template-name">{template.name}</h4>
        <div className="template-badges">
          {isDefault && <span className="badge default">默认</span>}
          {template.is_built_in && <span className="badge built-in">内置</span>}
          {!template.is_active && <span className="badge inactive">停用</span>}
          <span className="badge version">v{template.version}</span>
        </div>
      </div>

      {/* 预览区域 */}
      <div className="card-preview">
        <div className="preview-section">
          <div className="preview-label">正面</div>
          <div className="preview-content">
            <IframePreview 
              htmlContent={renderPreview(template.front_template || template.preview_front || '', template)} 
              cssContent={template.css_style || ''} 
            />
          </div>
        </div>
        <div className="preview-section">
          <div className="preview-label">背面</div>
          <div className="preview-content">
            <IframePreview 
              htmlContent={renderPreview(template.back_template || template.preview_back || '', template)} 
              cssContent={template.css_style || ''} 
            />
          </div>
        </div>
      </div>

      {/* 卡片信息 */}
      <div className="card-info">
        <p className="template-description">{template.description}</p>
        <div className="template-meta">
          <span className="meta-item">
            <span className="meta-icon">👤</span>
            {template.author || '未知'}
          </span>
          <span className="meta-item">
            <span className="meta-icon">📝</span>
            {template.fields.length} 个字段
          </span>
        </div>
        <div className="template-fields">
          {template.fields.slice(0, 3).map(field => (
            <span key={field} className="field-tag">{field}</span>
          ))}
          {template.fields.length > 3 && (
            <span className="field-tag more">+{template.fields.length - 3}</span>
          )}
        </div>
      </div>

      {/* 模式提示 */}
      {isSelectingMode && (
        <div className="mode-banner">
          <span className="banner-icon">🎯</span>
          <span className="banner-text">点击下方按钮选择此模板</span>
        </div>
      )}

      {/* 操作按钮 */}
      <div className="card-actions">
        {isSelectingMode ? (
          <button 
            className="btn-select primary" 
            onClick={() => {
              console.log('🎯 点击使用模板:', template.name, template);
              console.log('🔧 onTemplateSelected回调:', onTemplateSelected);
              if (onTemplateSelected) {
                onTemplateSelected(template);
              } else {
                console.error('❌ onTemplateSelected回调函数不存在');
              }
            }}
          >
            <span className="btn-icon">✨</span>
            使用此模板
          </button>
        ) : (
          <>
            <button 
              className={`btn-select ${isDefault ? 'default' : ''}`} 
              onClick={isDefault ? undefined : onSetDefaultTemplate}
              disabled={isDefault}
            >
              {isDefault ? '⭐ 默认模板' : '设为默认'}
            </button>
            <div className="action-buttons">
              <button className="action-btn" onClick={onEdit} title="编辑">
                <span>✏️</span>
              </button>
              <button className="action-btn" onClick={onDuplicate} title="复制">
                <span>📋</span>
              </button>
              {!template.is_built_in && (
                <button className="action-btn danger" onClick={onDelete} title="删除">
                  <span>🗑️</span>
                </button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
};

// 模板编辑器组件
interface TemplateEditorProps {
  template: CustomAnkiTemplate | null;
  mode: 'create' | 'edit';
  onSave: (templateData: CreateTemplateRequest) => Promise<void>;
  onCancel: () => void;
}

const TemplateEditor: React.FC<TemplateEditorProps> = ({
  template,
  mode,
  onSave,
  onCancel
}) => {
  const [formData, setFormData] = useState({
    name: template?.name || '',
    description: template?.description || '',
    author: template?.author || '',
    preview_front: template?.preview_front || '',
    preview_back: template?.preview_back || '',
    note_type: template?.note_type || 'Basic',
    fields: template?.fields.join(',') || 'Front,Back,Notes',
    generation_prompt: template?.generation_prompt || '',
    front_template: template?.front_template || '<div class="card">' + '{{Front}}' + '</div>',
    back_template: template?.back_template || '<div class="card">' + '{{Front}}' + '<hr>' + '{{Back}}' + '</div>',
    css_style: template?.css_style || '.card { padding: 20px; background: white; border-radius: 8px; }'
  });

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [activeEditorTab, setActiveEditorTab] = useState<'basic' | 'templates' | 'styles' | 'advanced'>('basic');

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setIsSubmitting(true);
    
    try {
      const fields = formData.fields.split(',').map(f => f.trim()).filter(f => f);
      const fieldExtractionRules: Record<string, FieldExtractionRule> = {};
      
      fields.forEach(field => {
        fieldExtractionRules[field] = {
          field_type: field.toLowerCase() === 'tags' ? 'Array' : 'Text',
          is_required: field.toLowerCase() === 'front' || field.toLowerCase() === 'back',
          default_value: field.toLowerCase() === 'tags' ? '[]' : '',
          description: `${field} 字段`
        };
      });

      const templateData: CreateTemplateRequest = {
        name: formData.name,
        description: formData.description,
        author: formData.author || undefined,
        preview_front: formData.preview_front,
        preview_back: formData.preview_back,
        note_type: formData.note_type,
        fields,
        generation_prompt: formData.generation_prompt,
        front_template: formData.front_template,
        back_template: formData.back_template,
        css_style: formData.css_style,
        field_extraction_rules: fieldExtractionRules
      };

      await onSave(templateData);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="template-editor">
      <div className="editor-header">
        <h3 className="editor-title">{mode === 'create' ? '创建新模板' : '编辑模板'}</h3>
      </div>

      {/* 编辑器标签页 */}
      <div className="editor-tabs">
        <button 
          className={`editor-tab ${activeEditorTab === 'basic' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('basic')}
        >
          <span className="tab-icon">📝</span>
          基本信息
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'templates' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('templates')}
        >
          <span className="tab-icon">🎨</span>
          模板代码
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'styles' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('styles')}
        >
          <span className="tab-icon">💄</span>
          样式设计
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'advanced' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('advanced')}
        >
          <span className="tab-icon">⚙️</span>
          高级设置
        </button>
      </div>

      <form onSubmit={handleSubmit} className="editor-form">
        {/* 基本信息标签页 */}
        {activeEditorTab === 'basic' && (
          <div className="editor-section">
            <div className="form-grid">
              <div className="form-group">
                <label className="form-label">模板名称 *</label>
                <input
                  type="text"
                  value={formData.name}
                  onChange={(e) => setFormData({...formData, name: e.target.value})}
                  required
                  className="form-input"
                  placeholder="请输入模板名称"
                />
              </div>

              <div className="form-group">
                <label className="form-label">作者</label>
                <input
                  type="text"
                  value={formData.author}
                  onChange={(e) => setFormData({...formData, author: e.target.value})}
                  className="form-input"
                  placeholder="可选"
                />
              </div>

              <div className="form-group full-width">
                <label className="form-label">描述</label>
                <textarea
                  value={formData.description}
                  onChange={(e) => setFormData({...formData, description: e.target.value})}
                  className="form-textarea"
                  rows={3}
                  placeholder="请描述这个模板的用途和特点..."
                />
              </div>

              <div className="form-group">
                <label className="form-label">笔记类型</label>
                <input
                  type="text"
                  value={formData.note_type}
                  onChange={(e) => setFormData({...formData, note_type: e.target.value})}
                  className="form-input"
                  placeholder="Basic"
                />
              </div>

              <div className="form-group">
                <label className="form-label">字段列表 *</label>
                <input
                  type="text"
                  value={formData.fields}
                  onChange={(e) => setFormData({...formData, fields: e.target.value})}
                  required
                  className="form-input"
                  placeholder="Front,Back,Notes"
                />
                <small className="form-help">用逗号分隔，至少需要包含 Front 和 Back 字段</small>
              </div>

              <div className="form-group">
                <label className="form-label">预览正面 *</label>
                <input
                  type="text"
                  value={formData.preview_front}
                  onChange={(e) => setFormData({...formData, preview_front: e.target.value})}
                  required
                  className="form-input"
                  placeholder="示例问题"
                />
              </div>

              <div className="form-group">
                <label className="form-label">预览背面 *</label>
                <input
                  type="text"
                  value={formData.preview_back}
                  onChange={(e) => setFormData({...formData, preview_back: e.target.value})}
                  required
                  className="form-input"
                  placeholder="示例答案"
                />
              </div>
            </div>
          </div>
        )}

        {/* 模板代码标签页 */}
        {activeEditorTab === 'templates' && (
          <div className="editor-section">
            <div className="template-code-editor">
              <div className="code-group">
                <label className="form-label">正面模板 *</label>
                <textarea
                  value={formData.front_template}
                  onChange={(e) => setFormData({...formData, front_template: e.target.value})}
                  required
                  className="code-textarea"
                  rows={8}
                  placeholder="<div class=&quot;card&quot;>&#123;&#123;Front&#125;&#125;</div>"
                />
                <small className="form-help">使用 Mustache 语法，如 {`{{Front}}`}、{`{{Back}}`} 等</small>
              </div>

              <div className="code-group">
                <label className="form-label">背面模板 *</label>
                <textarea
                  value={formData.back_template}
                  onChange={(e) => setFormData({...formData, back_template: e.target.value})}
                  required
                  className="code-textarea"
                  rows={8}
                  placeholder="<div class=&quot;card&quot;>&#123;&#123;Front&#125;&#125;<hr>&#123;&#123;Back&#125;&#125;</div>"
                />
              </div>
            </div>
          </div>
        )}

        {/* 样式设计标签页 */}
        {activeEditorTab === 'styles' && (
          <div className="editor-section">
            <div className="styles-editor">
              <label className="form-label">CSS 样式</label>
              <textarea
                value={formData.css_style}
                onChange={(e) => setFormData({...formData, css_style: e.target.value})}
                className="css-textarea"
                rows={12}
                placeholder=".card { padding: 20px; background: white; border-radius: 8px; }"
              />
              <small className="form-help">自定义CSS样式来美化卡片外观</small>
            </div>
          </div>
        )}

        {/* 高级设置标签页 */}
        {activeEditorTab === 'advanced' && (
          <div className="editor-section">
            <div className="advanced-settings">
              <label className="form-label">AI生成提示词 *</label>
              <textarea
                value={formData.generation_prompt}
                onChange={(e) => setFormData({...formData, generation_prompt: e.target.value})}
                required
                className="prompt-textarea"
                rows={8}
                placeholder="请输入AI生成卡片时使用的提示词..."
              />
              <small className="form-help">指导AI如何生成符合此模板的卡片内容</small>
            </div>
          </div>
        )}

        {/* 操作按钮 */}
        <div className="editor-actions">
          <button 
            type="submit" 
            disabled={isSubmitting}
            className="btn-primary"
          >
            {isSubmitting ? '保存中...' : mode === 'create' ? '创建模板' : '保存修改'}
          </button>
          <button 
            type="button" 
            onClick={onCancel}
            className="btn-secondary"
          >
            取消
          </button>
        </div>
      </form>
    </div>
  );
};

export default TemplateManagementPage;