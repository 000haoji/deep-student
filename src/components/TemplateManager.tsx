import React, { useState, useEffect, useRef } from 'react';
import { CustomAnkiTemplate, CreateTemplateRequest, FieldExtractionRule, AnkiCardTemplate } from '../types';
import { templateManager } from '../data/ankiTemplates';
import './TemplateManager.css';
import { IframePreview, renderCardPreview } from './SharedPreview';
import { 
  Palette, 
  BookOpen, 
  Plus, 
  Edit, 
  AlertTriangle, 
  Search, 
  FileText, 
  User, 
  Copy, 
  Trash2, 
  CheckCircle, 
  X,
  Settings,
  Paintbrush
} from 'lucide-react';

interface TemplateManagerProps {
  onClose: () => void;
  onSelectTemplate?: (template: CustomAnkiTemplate) => void;
}

const TemplateManager: React.FC<TemplateManagerProps> = ({ onClose, onSelectTemplate }) => {
  const [templates, setTemplates] = useState<CustomAnkiTemplate[]>([]);
  const [activeTab, setActiveTab] = useState<'browse' | 'edit' | 'create'>('browse');
  const [selectedTemplate, setSelectedTemplate] = useState<CustomAnkiTemplate | null>(null);
  const [editingTemplate, setEditingTemplate] = useState<CustomAnkiTemplate | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchTerm, setSearchTerm] = useState('');

  // 加载模板
  useEffect(() => {
    loadTemplates();
    
    // 订阅模板变化
    const unsubscribe = templateManager.subscribe(setTemplates);
    return unsubscribe;
  }, []);

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
    if (onSelectTemplate) {
      onSelectTemplate(template);
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
    <div className="template-manager-modal">
      <div className="template-manager-backdrop" onClick={onClose} />
      <div className="template-manager-container">
        {/* 头部 */}
        <div className="template-manager-header">
          <h2 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Palette size={20} />
            模板管理器
          </h2>
          <button className="close-btn" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        {/* 标签页导航 */}
        <div className="template-manager-tabs">
          <button 
            className={`tab-btn ${activeTab === 'browse' ? 'active' : ''}`}
            onClick={() => setActiveTab('browse')}
            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <BookOpen size={16} />
            浏览模板
          </button>
          <button 
            className={`tab-btn ${activeTab === 'create' ? 'active' : ''}`}
            onClick={() => setActiveTab('create')}
            style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
          >
            <Plus size={16} />
            创建模板
          </button>
          {editingTemplate && (
            <button 
              className={`tab-btn ${activeTab === 'edit' ? 'active' : ''}`}
              onClick={() => setActiveTab('edit')}
              style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
            >
              <Edit size={16} />
              编辑模板
            </button>
          )}
        </div>

        {/* 错误提示 */}
        {error && (
          <div className="error-banner">
            <span style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <AlertTriangle size={16} />
              {error}
            </span>
            <button onClick={() => setError(null)}>
              <X size={14} />
            </button>
          </div>
        )}

        {/* 内容区域 */}
        <div className="template-manager-content">
          {activeTab === 'browse' && (
            <TemplateBrowser
              templates={filteredTemplates}
              selectedTemplate={selectedTemplate}
              searchTerm={searchTerm}
              onSearchChange={setSearchTerm}
              onSelectTemplate={handleSelectTemplate}
              onEditTemplate={handleEditTemplate}
              onDuplicateTemplate={handleDuplicateTemplate}
              onDeleteTemplate={handleDeleteTemplate}
              isLoading={isLoading}
            />
          )}

          {activeTab === 'create' && (
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

          {activeTab === 'edit' && editingTemplate && (
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
      </div>
    </div>
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
  isLoading: boolean;
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
  isLoading
}) => {
  return (
    <div className="template-browser">
      {/* 搜索和工具栏 */}
      <div className="browser-toolbar">
        <div className="search-box">
          <input
            type="text"
            placeholder="搜索模板..."
            value={searchTerm}
            onChange={(e) => onSearchChange(e.target.value)}
            className="search-input"
          />
          <span className="search-icon">
            <Search size={16} />
          </span>
        </div>
        <div className="toolbar-info">
          共 {templates.length} 个模板
        </div>
      </div>

      {/* 模板网格 */}
      {isLoading ? (
        <div className="loading-state">
          <div className="loading-spinner"></div>
          <span>加载模板中...</span>
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
            />
          ))}
        </div>
      )}

      {templates.length === 0 && !isLoading && (
        <div className="empty-state">
          <div className="empty-icon">
            <FileText size={48} />
          </div>
          <h3>没有找到模板</h3>
          <p>试试调整搜索条件，或创建一个新模板。</p>
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
}

const TemplateCard: React.FC<TemplateCardProps> = ({
  template,
  isSelected,
  onSelect,
  onEdit,
  onDuplicate,
  onDelete
}) => {
  const cardTemplate = templateManager.toAnkiCardTemplate(template);

  return (
    <div className={`template-card ${isSelected ? 'selected' : ''} ${!template.is_active ? 'inactive' : ''}`}>
      {/* 卡片头部 */}
      <div className="card-header">
        <h4 className="template-name">{template.name}</h4>
        <div className="template-badges">
          {template.is_built_in && <span className="badge built-in">内置</span>}
          {!template.is_active && <span className="badge inactive">停用</span>}
          <span className="badge version">v{template.version}</span>
        </div>
      </div>

      {/* 预览区域 */}
      <div className="card-preview">
        <div className="preview-front">
          <div className="preview-label">正面</div>
          <div className="preview-content">
            <IframePreview
              htmlContent={renderCardPreview(cardTemplate.front_template, cardTemplate)}
              cssContent={cardTemplate.css_style}
            />
          </div>
        </div>
        <div className="preview-back">
          <div className="preview-label">背面</div>
          <div className="preview-content">
            <IframePreview
              htmlContent={renderCardPreview(cardTemplate.back_template, cardTemplate)}
              cssContent={cardTemplate.css_style}
            />
          </div>
        </div>
      </div>

      {/* 卡片信息 */}
      <div className="card-info">
        <p className="template-description">{template.description}</p>
        <div className="template-meta">
          <span className="author" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <User size={14} />
            {template.author || '未知'}
          </span>
          <span className="fields" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
            <FileText size={14} />
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

      {/* 操作按钮 */}
      <div className="card-actions">
        <button className="btn-select" onClick={onSelect} style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
          {isSelected ? (
            <>
              <CheckCircle size={16} />
              已选择
            </>
          ) : (
            '选择'
          )}
        </button>
        <div className="action-menu">
          <button className="btn-action" onClick={onEdit}>
            <Edit size={16} />
          </button>
          <button className="btn-action" onClick={onDuplicate}>
            <Copy size={16} />
          </button>
          {!template.is_built_in && (
            <button className="btn-action danger" onClick={onDelete}>
              <Trash2 size={16} />
            </button>
          )}
        </div>
      </div>
    </div>
  );
};

// 模板编辑器组件（简化版，完整版需要更多功能）
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
    front_template: template?.front_template || '<div class="card">{{Front}}</div>',
    back_template: template?.back_template || '<div class="card">{{Front}}<hr>{{Back}}</div>',
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
        <h3>{mode === 'create' ? '创建新模板' : '编辑模板'}</h3>
      </div>

      {/* 编辑器标签页 */}
      <div className="editor-tabs">
        <button 
          className={`editor-tab ${activeEditorTab === 'basic' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('basic')}
          style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
        >
          <FileText size={16} />
          基本信息
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'templates' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('templates')}
          style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
        >
          <Palette size={16} />
          模板代码
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'styles' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('styles')}
          style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
        >
          <Paintbrush size={16} />
          样式设计
        </button>
        <button 
          className={`editor-tab ${activeEditorTab === 'advanced' ? 'active' : ''}`}
          onClick={() => setActiveEditorTab('advanced')}
          style={{ display: 'flex', alignItems: 'center', gap: '6px' }}
        >
          <Settings size={16} />
          高级设置
        </button>
      </div>

      <form onSubmit={handleSubmit} className="editor-form">
        {/* 基本信息标签页 */}
        {activeEditorTab === 'basic' && (
          <div className="editor-section">
            <div className="form-grid">
              <div className="form-group">
                <label>模板名称 *</label>
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
                <label>作者</label>
                <input
                  type="text"
                  value={formData.author}
                  onChange={(e) => setFormData({...formData, author: e.target.value})}
                  className="form-input"
                  placeholder="可选"
                />
              </div>

              <div className="form-group full-width">
                <label>描述</label>
                <textarea
                  value={formData.description}
                  onChange={(e) => setFormData({...formData, description: e.target.value})}
                  className="form-textarea"
                  rows={3}
                  placeholder="请描述这个模板的用途和特点..."
                />
              </div>

              <div className="form-group">
                <label>笔记类型</label>
                <input
                  type="text"
                  value={formData.note_type}
                  onChange={(e) => setFormData({...formData, note_type: e.target.value})}
                  className="form-input"
                  placeholder="Basic"
                />
              </div>

              <div className="form-group">
                <label>字段列表 *</label>
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
                <label>预览正面 *</label>
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
                <label>预览背面 *</label>
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
                <label>正面模板 *</label>
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
                <label>背面模板 *</label>
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
              <label>CSS 样式</label>
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
              <label>AI生成提示词 *</label>
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

export default TemplateManager;
