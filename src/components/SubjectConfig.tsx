import React, { useState, useEffect } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { useSubject } from '../contexts/SubjectContext';
import { Target, Plus, RefreshCcw, Edit, Trash2, CheckCircle, XCircle, Lightbulb, FileText, Tag, X } from 'lucide-react';

interface SubjectConfig {
  id: string;
  subject_name: string;
  display_name: string;
  description: string;
  is_enabled: boolean;
  prompts: SubjectPrompts;
  mistake_types: string[];
  default_tags: string[];
  created_at: string;
  updated_at: string;
}

interface SubjectPrompts {
  analysis_prompt: string;
  review_prompt: string;
  chat_prompt: string;
  ocr_prompt: string;
  classification_prompt: string;
  consolidated_review_prompt: string;
  anki_generation_prompt: string;
}

interface SubjectConfigProps {}

export const SubjectConfig: React.FC<SubjectConfigProps> = () => {
  const { refreshSubjects } = useSubject();
  const [configs, setConfigs] = useState<SubjectConfig[]>([]);
  const [selectedConfig, setSelectedConfig] = useState<SubjectConfig | null>(null);
  const [isEditing, setIsEditing] = useState(false);
  const [isCreating, setIsCreating] = useState(false);
  const [loading, setLoading] = useState(false);

  // LaTeX数学公式输出规范
  const latexRules = `\n\n【LaTeX 数学公式输出规范】
1. 所有数学公式、符号、变量等都必须使用LaTeX格式包裹。
2. 行内公式使用 \`$...$\` 包裹，例如：\`$E=mc^2$\`。
3. 独立展示的公式或方程组使用 \`$$...$$\` 包裹。
4. 对于矩阵，请务必使用 \`bmatrix\` 环境，例如：\`$$\\begin{bmatrix} a & b \\\\ c & d \\end{bmatrix}$$\`。在 \`bmatrix\` 环境中，使用 \`&\` 分隔列元素，使用 \`\\\\\` (单个反斜杠，在JSON字符串中可能需要转义为 \`\\\\\\\\\`) 换行。
5. 确保所有LaTeX环境（如 \`bmatrix\`）和括号都正确配对和闭合。
6. 避免使用不常见或自定义的LaTeX宏包或命令，尽量使用标准KaTeX支持的命令。`;

  // 新建/编辑表单状态
  const [formData, setFormData] = useState<Partial<SubjectConfig>>({
    subject_name: '',
    display_name: '',
    description: '',
    is_enabled: true,
    prompts: {
      analysis_prompt: `请仔细分析这道{subject}错题，提供详细的解题思路和知识点讲解。${latexRules}`,
      review_prompt: `请分析这些{subject}错题的共同问题和改进建议。${latexRules}`,
      chat_prompt: `基于这道{subject}题目，请回答学生的问题。${latexRules}`,
      ocr_prompt: '请识别这张{subject}题目图片中的文字内容。',
      classification_prompt: '请分析这道{subject}题目的类型和相关知识点标签。',
      consolidated_review_prompt: `请对这些{subject}错题进行统一分析，提供综合性的学习建议和知识点总结。${latexRules}`,
      anki_generation_prompt: `请根据这道{subject}错题生成Anki卡片，包含问题、答案和关键知识点。${latexRules}`,
    },
    mistake_types: ['计算错误', '概念理解', '方法应用', '知识遗忘', '审题不清'],
    default_tags: ['基础知识', '重点难点', '易错点'],
  });

  useEffect(() => {
    loadConfigs();
  }, []);

  const loadConfigs = async () => {
    setLoading(true);
    try {
      const allConfigs = await TauriAPI.getAllSubjectConfigs(false);
      setConfigs(allConfigs);
      // 同时刷新全局科目状态
      await refreshSubjects();
    } catch (error) {
      console.error('加载科目配置失败:', error);
      alert('加载科目配置失败: ' + error);
    } finally {
      setLoading(false);
    }
  };

  const handleEdit = (config: SubjectConfig) => {
    setSelectedConfig(config);
    setFormData(config);
    setIsEditing(true);
    setIsCreating(false);
  };

  const handleCreate = () => {
    setSelectedConfig(null);
    setFormData({
      subject_name: '',
      display_name: '',
      description: '',
      is_enabled: true,
      prompts: {
        analysis_prompt: `请仔细分析这道{subject}错题，提供详细的解题思路和知识点讲解。${latexRules}`,
        review_prompt: `请分析这些{subject}错题的共同问题和改进建议。${latexRules}`,
        chat_prompt: `基于这道{subject}题目，请回答学生的问题。${latexRules}`,
        ocr_prompt: '请识别这张{subject}题目图片中的文字内容。',
        classification_prompt: '请分析这道{subject}题目的类型和相关知识点标签。',
        consolidated_review_prompt: `请对这些{subject}错题进行统一分析，提供综合性的学习建议和知识点总结。${latexRules}`,
        anki_generation_prompt: `请根据这道{subject}错题生成Anki卡片，包含问题、答案和关键知识点。${latexRules}`,
      },
      mistake_types: ['计算错误', '概念理解', '方法应用', '知识遗忘', '审题不清'],
      default_tags: ['基础知识', '重点难点', '易错点'],
    });
    setIsCreating(true);
    setIsEditing(false);
  };

  const handleSave = async () => {
    if (!formData.subject_name || !formData.display_name) {
      alert('请填写科目名称和显示名称');
      return;
    }

    setLoading(true);
    try {
      if (isCreating) {
        await TauriAPI.createSubjectConfig({
          subject_name: formData.subject_name!,
          display_name: formData.display_name!,
          description: formData.description,
          prompts: formData.prompts,
          mistake_types: formData.mistake_types,
          default_tags: formData.default_tags,
        });
      } else if (selectedConfig) {
        await TauriAPI.updateSubjectConfig({
          id: selectedConfig.id,
          display_name: formData.display_name,
          description: formData.description,
          is_enabled: formData.is_enabled,
          prompts: formData.prompts,
          mistake_types: formData.mistake_types,
          default_tags: formData.default_tags,
        });
      }
      
      alert(isCreating ? '创建成功！' : '更新成功！');
      setIsEditing(false);
      setIsCreating(false);
      setSelectedConfig(null);
      await loadConfigs();
      // 刷新全局科目状态，以更新标题栏的科目下拉栏
      await refreshSubjects();
    } catch (error) {
      console.error('保存失败:', error);
      alert('保存失败: ' + error);
    } finally {
      setLoading(false);
    }
  };

  const handleDelete = async (config: SubjectConfig) => {
    if (!confirm(`确定要删除科目配置 "${config.display_name}" 吗？`)) {
      return;
    }

    setLoading(true);
    try {
      await TauriAPI.deleteSubjectConfig(config.id);
      alert('删除成功！');
      await loadConfigs();
      // 刷新全局科目状态，以更新标题栏的科目下拉栏
      await refreshSubjects();
    } catch (error) {
      console.error('删除失败:', error);
      alert('删除失败: ' + error);
    } finally {
      setLoading(false);
    }
  };

  const handleCancel = () => {
    setIsEditing(false);
    setIsCreating(false);
    setSelectedConfig(null);
  };

  const updateFormField = (field: string, value: any) => {
    setFormData(prev => ({
      ...prev,
      [field]: value
    }));
  };

  const updatePromptField = (field: string, value: string) => {
    setFormData(prev => ({
      ...prev,
      prompts: {
        ...prev.prompts!,
        [field]: value
      }
    }));
  };

  const updateArrayField = (field: 'mistake_types' | 'default_tags', value: string) => {
    const items = value.split(',').map(item => item.trim()).filter(item => item);
    setFormData(prev => ({
      ...prev,
      [field]: items
    }));
  };

  return (
    <>
      <div className="subject-config">
        <div className="settings-section">
          <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <Target size={20} />
            科目配置管理
          </h3>
          
          <div className="toolbar">
            <button onClick={handleCreate} className="create-button" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Plus size={16} />
              新建科目
            </button>
            <button onClick={loadConfigs} disabled={loading} className="refresh-button" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <RefreshCcw size={16} />
              刷新
            </button>
          </div>

          {loading ? (
            <div className="loading">加载中...</div>
          ) : (
            <div className="config-list">
              {configs.length === 0 ? (
                <div className="empty-state">
                  <p>暂无科目配置</p>
                  <button onClick={handleCreate}>创建第一个科目配置</button>
                </div>
              ) : (
                configs.map(config => (
                  <div key={config.id} className={`config-item ${!config.is_enabled ? 'disabled' : ''}`}>
                    <div className="config-header">
                      <h4>{config.display_name}</h4>
                      <div className="config-actions">
                        <button onClick={() => handleEdit(config)} className="edit-button" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <Edit size={14} />
                          编辑
                        </button>
                        <button onClick={() => handleDelete(config)} className="delete-button" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                          <Trash2 size={14} />
                          删除
                        </button>
                      </div>
                    </div>
                    <div className="config-info">
                      <p><strong>科目名称:</strong> {config.subject_name}</p>
                      <p><strong>描述:</strong> {config.description}</p>
                      <p style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        <strong>状态:</strong> 
                        {config.is_enabled ? (
                          <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <CheckCircle size={16} color="green" />
                            启用
                          </span>
                        ) : (
                          <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                            <XCircle size={16} color="red" />
                            禁用
                          </span>
                        )}
                      </p>
                      <p><strong>错题类型:</strong> {config.mistake_types.join(', ')}</p>
                      <p><strong>默认标签:</strong> {config.default_tags.join(', ')}</p>
                      <p><strong>更新时间:</strong> {new Date(config.updated_at).toLocaleString('zh-CN')}</p>
                    </div>
                  </div>
                ))
              )}
            </div>
          )}

          <div className="tips">
            <h5 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <Lightbulb size={16} />
              使用提示:
            </h5>
            <ul>
              <li>科目名称创建后不可修改，请谨慎填写</li>
              <li>提示词中可使用 {'{subject}'} 作为科目名称占位符</li>
              <li>禁用的科目在前端选择器中不会显示</li>
              <li>修改提示词会影响后续的AI分析结果</li>
            </ul>
          </div>
        </div>
      </div>
      
      {(isEditing || isCreating) && (
        <div className="modal-overlay">
          <div className="modal-dialog">
            <div className="modal-header">
              <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <Target size={20} />
                {isCreating ? '新建科目配置' : '编辑科目配置'}
              </h3>
              <button onClick={handleCancel} className="close-button">
                <X size={16} />
              </button>
            </div>

            <div className="modal-content">
              <div className="form-section">
                <div className="form-group">
                  <label>科目名称:</label>
                  <input
                    type="text"
                    value={formData.subject_name || ''}
                    onChange={(e) => updateFormField('subject_name', e.target.value)}
                    disabled={!isCreating}
                    placeholder="如：数学、物理"
                  />
                  {!isCreating && <small>科目名称创建后不可修改</small>}
                </div>

                <div className="form-group">
                  <label>显示名称:</label>
                  <input
                    type="text"
                    value={formData.display_name || ''}
                    onChange={(e) => updateFormField('display_name', e.target.value)}
                    placeholder="如：高中数学、初中物理"
                  />
                </div>

                <div className="form-group">
                  <label>描述:</label>
                  <textarea
                    value={formData.description || ''}
                    onChange={(e) => updateFormField('description', e.target.value)}
                    placeholder="科目的详细描述"
                    rows={2}
                  />
                </div>

                <div className="form-group checkbox-group">
                  <div className="checkbox-container">
                    <span>启用此科目</span>
                    <input
                      type="checkbox"
                      checked={formData.is_enabled}
                      onChange={(e) => updateFormField('is_enabled', e.target.checked)}
                    />
                  </div>
                </div>
              </div>

              <div className="form-section">
                <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <FileText size={16} />
                  提示词配置
                </h4>
                <small>使用 {'{subject}'} 作为科目名称的占位符</small>

                <div className="form-group">
                  <label>错题分析提示词:</label>
                  <textarea
                    value={formData.prompts?.analysis_prompt || ''}
                    onChange={(e) => updatePromptField('analysis_prompt', e.target.value)}
                    rows={3}
                    placeholder="用于分析错题的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>回顾分析提示词:</label>
                  <textarea
                    value={formData.prompts?.review_prompt || ''}
                    onChange={(e) => updatePromptField('review_prompt', e.target.value)}
                    rows={3}
                    placeholder="用于回顾分析的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>对话追问提示词:</label>
                  <textarea
                    value={formData.prompts?.chat_prompt || ''}
                    onChange={(e) => updatePromptField('chat_prompt', e.target.value)}
                    rows={3}
                    placeholder="用于对话追问的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>OCR识别提示词:</label>
                  <textarea
                    value={formData.prompts?.ocr_prompt || ''}
                    onChange={(e) => updatePromptField('ocr_prompt', e.target.value)}
                    rows={2}
                    placeholder="用于OCR图片识别的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>分类标记提示词:</label>
                  <textarea
                    value={formData.prompts?.classification_prompt || ''}
                    onChange={(e) => updatePromptField('classification_prompt', e.target.value)}
                    rows={2}
                    placeholder="用于分类和标记的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>统一回顾分析提示词:</label>
                  <textarea
                    value={formData.prompts?.consolidated_review_prompt || ''}
                    onChange={(e) => updatePromptField('consolidated_review_prompt', e.target.value)}
                    rows={3}
                    placeholder="用于统一回顾分析的提示词"
                  />
                </div>

                <div className="form-group">
                  <label>Anki卡片生成提示词:</label>
                  <textarea
                    value={formData.prompts?.anki_generation_prompt || ''}
                    onChange={(e) => updatePromptField('anki_generation_prompt', e.target.value)}
                    rows={3}
                    placeholder="用于生成Anki卡片的提示词"
                  />
                </div>
              </div>

              <div className="form-section">
                <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                  <Tag size={16} />
                  默认配置
                </h4>

                <div className="form-group">
                  <label>错题类型 (逗号分隔):</label>
                  <input
                    type="text"
                    value={formData.mistake_types?.join(', ') || ''}
                    onChange={(e) => updateArrayField('mistake_types', e.target.value)}
                    placeholder="计算错误, 概念理解, 方法应用"
                  />
                </div>

                <div className="form-group">
                  <label>默认标签 (逗号分隔):</label>
                  <input
                    type="text"
                    value={formData.default_tags?.join(', ') || ''}
                    onChange={(e) => updateArrayField('default_tags', e.target.value)}
                    placeholder="基础知识, 重点难点, 易错点"
                  />
                </div>
              </div>
            </div>

            <div className="modal-footer">
              <button onClick={handleCancel} className="cancel-button">取消</button>
              <button onClick={handleSave} disabled={loading} className="save-button">
                {loading ? '保存中...' : '保存'}
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};