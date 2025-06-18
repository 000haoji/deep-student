import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { 
  Tag, 
  TagType, 
  CreateTagRequest, 
  TagHierarchy 
} from '../types/cogni-graph';
import './TagManagement.css';

interface TagManagementProps {
  isInitialized: boolean;
}

const TagManagement: React.FC<TagManagementProps> = ({ isInitialized }) => {
  const [tags, setTags] = useState<Tag[]>([]);
  const [tagHierarchy, setTagHierarchy] = useState<TagHierarchy[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [selectedTagType, setSelectedTagType] = useState<TagType>('KnowledgeArea');
  
  const [newTag, setNewTag] = useState<CreateTagRequest>({
    name: '',
    tag_type: 'Concept',
    parent_id: undefined,
    description: ''
  });

  useEffect(() => {
    if (isInitialized) {
      loadTags();
      loadTagHierarchy();
    }
  }, [isInitialized]);

  const loadTags = async () => {
    try {
      setLoading(true);
      const allTags = await invoke<Tag[]>('get_tags_by_type', { 
        tagType: selectedTagType 
      });
      setTags(allTags);
    } catch (err) {
      setError(`加载标签失败: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const loadTagHierarchy = async () => {
    try {
      const hierarchy = await invoke<TagHierarchy[]>('get_tag_hierarchy', { 
        rootTagId: null 
      });
      setTagHierarchy(hierarchy);
    } catch (err) {
      console.error('加载标签层次失败:', err);
    }
  };

  const createTag = async () => {
    if (!newTag.name.trim()) {
      setError('标签名称不能为空');
      return;
    }

    try {
      setLoading(true);
      setError(null);
      
      const tagId = await invoke<string>('create_tag', { 
        request: {
          ...newTag,
          description: newTag.description || undefined
        }
      });
      
      // Reset form
      setNewTag({
        name: '',
        tag_type: 'Concept',
        parent_id: undefined,
        description: ''
      });
      setShowCreateForm(false);
      
      // Reload tags
      await loadTags();
      await loadTagHierarchy();
      
      console.log('标签创建成功:', tagId);
    } catch (err) {
      setError(`创建标签失败: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const initializeDefaultHierarchy = async () => {
    try {
      setLoading(true);
      setError(null);
      
      const result = await invoke<string>('initialize_default_tag_hierarchy');
      
      // Reload data
      await loadTags();
      await loadTagHierarchy();
      
      alert(`默认标签层次初始化成功: ${result}`);
    } catch (err) {
      setError(`初始化默认层次失败: ${err}`);
    } finally {
      setLoading(false);
    }
  };

  const renderTagHierarchy = (hierarchies: TagHierarchy[], level: number = 0): JSX.Element[] => {
    return hierarchies.map((hierarchy) => (
      <div key={hierarchy.tag.id} className={`tag-node level-${level}`}>
        <div className="tag-info">
          <span className={`tag-type ${hierarchy.tag.tag_type.toLowerCase()}`}>
            {hierarchy.tag.tag_type}
          </span>
          <span className="tag-name">{hierarchy.tag.name}</span>
          <span className="tag-level">Level {hierarchy.tag.level}</span>
        </div>
        
        {hierarchy.tag.description && (
          <div className="tag-description">{hierarchy.tag.description}</div>
        )}
        
        {hierarchy.children.length > 0 && (
          <div className="tag-children">
            {renderTagHierarchy(hierarchy.children, level + 1)}
          </div>
        )}
      </div>
    ));
  };

  const getAvailableParents = (): Tag[] => {
    return tags.filter(tag => {
      // Can only be parent if it's a higher level type
      const parentTypes: TagType[] = [];
      switch (newTag.tag_type) {
        case 'Topic':
          parentTypes.push('KnowledgeArea');
          break;
        case 'Concept':
          parentTypes.push('KnowledgeArea', 'Topic');
          break;
        case 'Method':
          parentTypes.push('Topic', 'Concept');
          break;
        case 'Difficulty':
          return []; // Difficulty tags usually standalone
        default:
          return [];
      }
      return parentTypes.includes(tag.tag_type);
    });
  };

  if (!isInitialized) {
    return (
      <div className="tag-management-container">
        <div className="not-initialized">
          <p>图谱未初始化，请先完成Neo4j连接配置</p>
        </div>
      </div>
    );
  }

  return (
    <div className="tag-management-container">
      <div className="tag-management-header">
        <h3>标签管理</h3>
        <div className="header-actions">
          <button 
            onClick={() => setShowCreateForm(!showCreateForm)}
            className="create-tag-btn"
            disabled={loading}
          >
            ➕ 创建标签
          </button>
          <button 
            onClick={initializeDefaultHierarchy}
            className="init-hierarchy-btn"
            disabled={loading}
          >
            🏗️ 初始化默认层次
          </button>
        </div>
      </div>

      {error && (
        <div className="error-message">
          {error}
          <button onClick={() => setError(null)}>✕</button>
        </div>
      )}

      {showCreateForm && (
        <div className="create-tag-form">
          <h4>创建新标签</h4>
          
          <div className="form-group">
            <label>标签名称:</label>
            <input
              type="text"
              value={newTag.name}
              onChange={(e) => setNewTag({ ...newTag, name: e.target.value })}
              placeholder="输入标签名称"
            />
          </div>

          <div className="form-group">
            <label>标签类型:</label>
            <select
              value={newTag.tag_type}
              onChange={(e) => setNewTag({ ...newTag, tag_type: e.target.value as TagType })}
            >
              <option value="KnowledgeArea">知识领域</option>
              <option value="Topic">主题</option>
              <option value="Concept">概念</option>
              <option value="Method">方法</option>
              <option value="Difficulty">难度</option>
            </select>
          </div>

          <div className="form-group">
            <label>父标签 (可选):</label>
            <select
              value={newTag.parent_id || ''}
              onChange={(e) => setNewTag({ 
                ...newTag, 
                parent_id: e.target.value || undefined 
              })}
            >
              <option value="">无父标签</option>
              {getAvailableParents().map(tag => (
                <option key={tag.id} value={tag.id}>
                  {tag.name} ({tag.tag_type})
                </option>
              ))}
            </select>
          </div>

          <div className="form-group">
            <label>描述 (可选):</label>
            <textarea
              value={newTag.description}
              onChange={(e) => setNewTag({ ...newTag, description: e.target.value })}
              placeholder="输入标签描述"
              rows={3}
            />
          </div>

          <div className="form-actions">
            <button onClick={createTag} disabled={loading}>
              ✅ 创建
            </button>
            <button onClick={() => setShowCreateForm(false)} disabled={loading}>
              ❌ 取消
            </button>
          </div>
        </div>
      )}

      <div className="tag-content">
        <div className="tag-list-section">
          <div className="section-header">
            <h4>标签列表</h4>
            <div className="filter-controls">
              <label>筛选类型:</label>
              <select
                value={selectedTagType}
                onChange={(e) => {
                  setSelectedTagType(e.target.value as TagType);
                  loadTags();
                }}
              >
                <option value="KnowledgeArea">知识领域</option>
                <option value="Topic">主题</option>
                <option value="Concept">概念</option>
                <option value="Method">方法</option>
                <option value="Difficulty">难度</option>
              </select>
            </div>
          </div>

          {loading ? (
            <div className="loading">加载中...</div>
          ) : (
            <div className="tag-list">
              {tags.map(tag => (
                <div key={tag.id} className="tag-item">
                  <div className="tag-header">
                    <span className={`tag-type ${tag.tag_type.toLowerCase()}`}>
                      {tag.tag_type}
                    </span>
                    <span className="tag-name">{tag.name}</span>
                    <span className="tag-level">L{tag.level}</span>
                  </div>
                  {tag.description && (
                    <div className="tag-description">{tag.description}</div>
                  )}
                  <div className="tag-meta">
                    <span>创建时间: {new Date(tag.created_at).toLocaleDateString()}</span>
                  </div>
                </div>
              ))}
              
              {tags.length === 0 && (
                <div className="no-tags">
                  暂无 {selectedTagType} 类型的标签
                </div>
              )}
            </div>
          )}
        </div>

        <div className="tag-hierarchy-section">
          <h4>标签层次结构</h4>
          
          {tagHierarchy.length > 0 ? (
            <div className="hierarchy-tree">
              {renderTagHierarchy(tagHierarchy)}
            </div>
          ) : (
            <div className="no-hierarchy">
              暂无标签层次结构，请先创建标签或初始化默认层次
            </div>
          )}
        </div>
      </div>
    </div>
  );
};

export default TagManagement;