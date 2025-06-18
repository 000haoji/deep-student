import React, { useState, useEffect, useCallback } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { useNotification } from '../hooks/useNotification';
import './EnhancedKnowledgeBaseManagement.css';
import type { 
  RagDocument, 
  KnowledgeBaseStatusPayload, 
  RagProcessingEvent, 
  RagDocumentStatusEvent 
} from '../types';
import { listen } from '@tauri-apps/api/event';
import './KnowledgeBaseManagement.css';
import { 
  FileText, Edit, Trash2, BookOpen, Lightbulb, X, Upload, FolderOpen
} from 'lucide-react';

// 添加CSS动画样式
const animationKeyframes = `
  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
  
  @keyframes shimmer {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(100%); }
  }
  
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }
`;

// 将样式注入到页面中
if (typeof document !== 'undefined') {
  const styleElement = document.createElement('style');
  styleElement.textContent = animationKeyframes;
  document.head.appendChild(styleElement);
}

// 进度条组件
interface ProgressBarProps {
  progress: number;
  status: string;
  fileName: string;
  stage?: string;
  totalChunks?: number;
  currentChunk?: number;
  chunksProcessed?: number;
}

const ProgressBar: React.FC<ProgressBarProps> = ({ 
  progress, 
  status, 
  fileName, 
  stage, 
  totalChunks, 
  currentChunk,
  chunksProcessed
}) => {
  const getStatusColor = (status: string) => {
    switch (status) {
      case 'Reading': return '#3b82f6'; // 蓝色
      case 'Preprocessing': return '#8b5cf6'; // 紫色
      case 'Chunking': return '#f59e0b'; // 橙色
      case 'Embedding': return '#10b981'; // 绿色
      case 'Storing': return '#06b6d4'; // 青色
      case 'Completed': return '#22c55e'; // 成功绿色
      case 'Failed': return '#ef4444'; // 错误红色
      default: return '#6b7280'; // 灰色
    }
  };

  const getStageText = (status: string) => {
    switch (status) {
      case 'Reading': return '📖 读取文件';
      case 'Preprocessing': return '🔧 预处理';
      case 'Chunking': return '✂️ 文本分块';
      case 'Embedding': return '🧠 生成向量';
      case 'Storing': return '💾 存储数据';
      case 'Completed': return '✅ 处理完成';
      case 'Failed': return '❌ 处理失败';
      default: return '⏳ 处理中';
    }
  };

  return (
    <div style={{
      background: 'white',
      border: '1px solid #e5e7eb',
      borderRadius: '12px',
      padding: '16px',
      marginBottom: '12px',
      boxShadow: '0 2px 4px rgba(0, 0, 0, 0.05)'
    }}>
      <div style={{
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center',
        marginBottom: '8px'
      }}>
        <div style={{
          fontSize: '14px',
          fontWeight: '600',
          color: '#374151',
          display: 'flex',
          alignItems: 'center',
          gap: '8px'
        }}>
          <FileText size={16} />
          {fileName}
        </div>
        <div style={{
          fontSize: '12px',
          color: '#6b7280',
          fontWeight: '500'
        }}>
          {Math.round(progress * 100)}%
        </div>
      </div>
      
      <div style={{
        fontSize: '12px',
        color: getStatusColor(status),
        marginBottom: '8px',
        fontWeight: '500',
        display: 'flex',
        justifyContent: 'space-between',
        alignItems: 'center'
      }}>
        <span>{getStageText(status)}</span>
        {status === 'Chunking' && totalChunks && (
          <span style={{ color: '#6b7280', fontSize: '11px' }}>
            预计生成 {totalChunks} 个文本块
          </span>
        )}
        {status === 'Embedding' && chunksProcessed !== undefined && totalChunks && (
          <span style={{ color: '#6b7280', fontSize: '11px' }}>
            🧠 生成向量: {chunksProcessed} / {totalChunks}
          </span>
        )}
        {status === 'Embedding' && !chunksProcessed && totalChunks && (
          <span style={{ color: '#6b7280', fontSize: '11px' }}>
            预计生成 {totalChunks} 个向量
          </span>
        )}
        {status === 'Completed' && totalChunks && (
          <span style={{ color: '#22c55e', fontSize: '11px' }}>
            ✅ 已生成 {totalChunks} 个文本块
          </span>
        )}
      </div>
      
      <div style={{
        width: '100%',
        height: '8px',
        backgroundColor: '#f3f4f6',
        borderRadius: '4px',
        overflow: 'hidden'
      }}>
        <div style={{
          width: `${progress * 100}%`,
          height: '100%',
          backgroundColor: getStatusColor(status),
          borderRadius: '4px',
          transition: 'width 0.5s ease, background-color 0.3s ease',
          background: status === 'Embedding' 
            ? `linear-gradient(90deg, ${getStatusColor(status)}, ${getStatusColor(status)}aa, ${getStatusColor(status)})`
            : status === 'Reading' || status === 'Preprocessing' || status === 'Chunking' || status === 'Storing'
            ? `linear-gradient(90deg, ${getStatusColor(status)}, ${getStatusColor(status)}cc)`
            : getStatusColor(status),
          position: 'relative',
          overflow: 'hidden'
        }}>
          {(status === 'Embedding' || status === 'Storing') && progress < 1 && (
            <div style={{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              background: `linear-gradient(90deg, transparent, rgba(255,255,255,0.3), transparent)`,
              animation: 'shimmer 2s infinite',
              transform: 'translateX(-100%)'
            }} />
          )}
        </div>
      </div>
      
      {status === 'Failed' && (
        <div style={{
          marginTop: '8px',
          fontSize: '12px',
          color: '#ef4444',
          background: '#fef2f2',
          padding: '8px',
          borderRadius: '6px',
          border: '1px solid #fecaca'
        }}>
          处理失败，请检查文件格式或重试
        </div>
      )}
    </div>
  );
};

// 定义分库接口类型
interface SubLibrary {
  id: string;
  name: string;
  description?: string;
  created_at: string;
  updated_at: string;
  document_count: number;
  chunk_count: number;
}

interface CreateSubLibraryRequest {
  name: string;
  description?: string;
}

interface UpdateSubLibraryRequest {
  name?: string;
  description?: string;
}

interface EnhancedKnowledgeBaseManagementProps {
  className?: string;
}

export const EnhancedKnowledgeBaseManagement: React.FC<EnhancedKnowledgeBaseManagementProps> = ({ 
  className = '' 
}) => {
  // 分库相关状态
  const [subLibraries, setSubLibraries] = useState<SubLibrary[]>([]);
  const [selectedLibrary, setSelectedLibrary] = useState<string>('default');
  const [documents, setDocuments] = useState<RagDocument[]>([]);
  const [status, setStatus] = useState<KnowledgeBaseStatusPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [processingDocuments, setProcessingDocuments] = useState<Map<string, RagDocumentStatusEvent>>(new Map());
  const [selectedFiles, setSelectedFiles] = useState<File[]>([]);

  // 分库管理状态
  const [showCreateLibraryModal, setShowCreateLibraryModal] = useState(false);
  const [showEditLibraryModal, setShowEditLibraryModal] = useState(false);
  const [editingLibrary, setEditingLibrary] = useState<SubLibrary | null>(null);
  const [newLibraryName, setNewLibraryName] = useState('');
  const [newLibraryDescription, setNewLibraryDescription] = useState('');

  const { showSuccess, showError, showWarning } = useNotification();
  
  // 拖拽状态
  const [isDragOver, setIsDragOver] = useState(false);

  // 格式化文件大小
  const formatFileSize = (bytes?: number) => {
    if (!bytes) return '未知';
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return Math.round(bytes / Math.pow(1024, i) * 100) / 100 + ' ' + sizes[i];
  };

  // 格式化日期
  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleDateString('zh-CN');
  };

  // 加载分库列表
  const loadSubLibraries = useCallback(async () => {
    try {
      const libraries = await TauriAPI.invoke('get_rag_sub_libraries') as SubLibrary[];
      setSubLibraries(libraries);
    } catch (error) {
      console.error('加载分库列表失败:', error);
      showError(`加载分库列表失败: ${error}`);
    }
  }, [showError]);

  // 加载知识库状态
  const loadKnowledgeBaseStatus = useCallback(async () => {
    try {
      setLoading(true);
      const statusData = await TauriAPI.ragGetKnowledgeBaseStatus();
      setStatus(statusData);
    } catch (error) {
      console.error('加载知识库状态失败:', error);
      showError(`加载知识库状态失败: ${error}`);
    } finally {
      setLoading(false);
    }
  }, [showError]);

  // 加载指定分库的文档
  const loadLibraryDocuments = useCallback(async (libraryId: string) => {
    try {
      setLoading(true);
      const documentsData = await TauriAPI.invoke('get_rag_documents_by_library', {
        request: {
          sub_library_id: libraryId === 'default' ? null : libraryId,
          page: 1,
          page_size: 100
        }
      }) as RagDocument[];
      setDocuments(documentsData);
    } catch (error) {
      console.error('加载文档列表失败:', error);
      showError(`加载文档列表失败: ${error}`);
    } finally {
      setLoading(false);
    }
  }, [showError]);

  // 创建新分库
  const createSubLibrary = async () => {
    if (!newLibraryName.trim()) {
      showError('请输入分库名称');
      return;
    }

    try {
      const request: CreateSubLibraryRequest = {
        name: newLibraryName.trim(),
        description: newLibraryDescription.trim() || undefined
      };

      await TauriAPI.invoke('create_rag_sub_library', { request });
      showSuccess(`分库 "${newLibraryName}" 创建成功`);
      
      // 重置表单
      setNewLibraryName('');
      setNewLibraryDescription('');
      setShowCreateLibraryModal(false);
      
      // 重新加载分库列表
      await loadSubLibraries();
    } catch (error) {
      console.error('创建分库失败:', error);
      showError(`创建分库失败: ${error}`);
    }
  };

  // 更新分库
  const updateSubLibrary = async () => {
    if (!editingLibrary || !newLibraryName.trim()) {
      showError('请输入分库名称');
      return;
    }

    try {
      const request: UpdateSubLibraryRequest = {
        name: newLibraryName.trim(),
        description: newLibraryDescription.trim() || undefined
      };

      await TauriAPI.updateRagSubLibrary(editingLibrary.id, request);
      
      showSuccess(`分库更新成功`);
      
      // 重置表单
      setNewLibraryName('');
      setNewLibraryDescription('');
      setShowEditLibraryModal(false);
      setEditingLibrary(null);
      
      // 重新加载分库列表
      await loadSubLibraries();
    } catch (error) {
      console.error('更新分库失败:', error);
      showError(`更新分库失败: ${error}`);
    }
  };

  // 删除分库
  const deleteSubLibrary = async (library: SubLibrary) => {
    if (library.id === 'default') {
      showError('不能删除默认分库');
      return;
    }

    const confirmMessage = `确定要删除分库 "${library.name}" 吗？

分库信息：
- 文档数量：${library.document_count} 个
- 文本块数量：${library.chunk_count} 个

⚠️ 注意：分库删除后，其中的文档将自动移动到默认分库，文档内容不会丢失。

确定要继续吗？`;

    const confirmDelete = window.confirm(confirmMessage);
    if (!confirmDelete) return;

    try {
      console.log('开始删除分库:', library.id, library.name);
      await TauriAPI.deleteRagSubLibrary(library.id, false); // 将文档移动到默认分库而不是删除
      console.log('分库删除成功，开始刷新数据');
      
      showSuccess(`分库 "${library.name}" 已删除，文档已移动到默认分库`);
      
      // 立即从本地状态中移除该分库，提升界面响应速度
      setSubLibraries(prev => prev.filter(lib => lib.id !== library.id));

      if (selectedLibrary === library.id) {
        // 切换到默认分库并加载其文档
        setSelectedLibrary('default');
        await loadLibraryDocuments('default');
      } else {
        // 重新加载当前分库文档
        await loadLibraryDocuments(selectedLibrary);
      }

      // 同步更新知识库状态
      await loadKnowledgeBaseStatus();
    } catch (error) {
      console.error('删除分库失败:', error);
      showError(`删除分库失败: ${error}`);
    }
  };

  // 上传文档到指定分库
  const uploadDocuments = async () => {
    if (selectedFiles.length === 0) {
      showError('请选择要上传的文件');
      return;
    }

    setUploading(true);
    
    try {
      // 读取文件内容，使用内容模式上传
      const documents: Array<{ file_name: string; base64_content: string }> = [];
      
      for (const file of selectedFiles) {
        try {
                  const content = await new Promise<string>((resolve, reject) => {
          const reader = new FileReader();
          reader.onerror = (e) => reject(e);
          
          // 根据文件类型选择读取方式
          if (file.type === 'text/plain' || file.name.endsWith('.txt') || file.name.endsWith('.md')) {
            reader.onload = (e) => {
              const result = e.target?.result as string;
              resolve(result);
            };
            reader.readAsText(file);
          } else {
            // 对于PDF、DOCX等二进制文件，读取为ArrayBuffer然后转换为base64
            reader.onload = (e) => {
              const arrayBuffer = e.target?.result as ArrayBuffer;
              const uint8Array = new Uint8Array(arrayBuffer);
              
              // 分块处理大文件，避免栈溢出
              let binary = '';
              const chunkSize = 8192; // 8KB chunks
              for (let i = 0; i < uint8Array.length; i += chunkSize) {
                const chunk = uint8Array.subarray(i, i + chunkSize);
                binary += String.fromCharCode.apply(null, Array.from(chunk));
              }
              
              const base64 = btoa(binary);
              resolve(base64);
            };
            reader.readAsArrayBuffer(file);
          }
        });
          
          documents.push({
            file_name: file.name,
            base64_content: content
          });
        } catch (fileError) {
          console.error(`处理文件 ${file.name} 失败:`, fileError);
          showError(`处理文件 ${file.name} 失败`);
          return;
        }
      }

      if (documents.length === 0) {
        showError('没有文件被成功处理');
        return;
      }

      // 使用内容模式上传到分库
      await TauriAPI.invoke('rag_add_documents_from_content_to_library', {
        request: {
          documents: documents,
          sub_library_id: selectedLibrary === 'default' ? null : selectedLibrary
        }
      });
      
      showSuccess(`成功上传 ${selectedFiles.length} 个文件到分库`);
      setSelectedFiles([]);
      
      // 重新加载当前分库的文档
      await loadLibraryDocuments(selectedLibrary);
      await loadKnowledgeBaseStatus();
    } catch (error) {
      console.error('上传文档失败:', error);
      let errorMessage = '上传文档失败';
      
      if (typeof error === 'string') {
        errorMessage = `上传失败: ${error}`;
      } else if (error instanceof Error) {
        errorMessage = `上传失败: ${error.message}`;
      } else if (error && typeof error === 'object' && 'message' in error) {
        errorMessage = `上传失败: ${(error as any).message}`;
      }
      
      // 提供更有用的错误信息和建议
      if (errorMessage.includes('系统找不到指定的文件') || errorMessage.includes('文件不存在')) {
        errorMessage += '\n\n建议：\n• 文件可能已被移动或删除\n• 请重新选择文件\n• 检查文件名是否包含特殊字符';
      }
      
      showError(errorMessage);
    } finally {
      setUploading(false);
    }
  };

  // 初始化加载
  useEffect(() => {
    loadSubLibraries();
    loadKnowledgeBaseStatus();
  }, [loadSubLibraries, loadKnowledgeBaseStatus]);

  // 当选中的分库改变时，重新加载文档
  useEffect(() => {
    if (selectedLibrary) {
      loadLibraryDocuments(selectedLibrary);
    }
  }, [selectedLibrary, loadLibraryDocuments]);

  // 监听文档处理事件
  useEffect(() => {
    const setupListeners = async () => {
      // 监听处理状态更新
      await listen<RagProcessingEvent>('rag-processing-status', (event) => {
        console.log('RAG处理状态:', event.payload);
      });

      // 监听文档状态更新
      await listen<RagDocumentStatusEvent>('rag_document_status', (event) => {
        const data = event.payload;
        setProcessingDocuments(prev => {
          const newMap = new Map(prev);
          newMap.set(data.document_id, data);
          return newMap;
        });

        if (data.status === 'Completed') {
          showSuccess(`文档 "${data.file_name}" 处理完成，共生成 ${data.total_chunks} 个文本块`);
          setTimeout(() => {
            setProcessingDocuments(prev => {
              const newMap = new Map(prev);
              newMap.delete(data.document_id);
              return newMap;
            });
          }, 3000);
        } else if (data.status === 'Failed') {
          const errorMsg = data.error_message || '未知错误';
          showError(`文档 "${data.file_name}" 处理失败: ${errorMsg}`);
          
          setTimeout(() => {
            setProcessingDocuments(prev => {
              const newMap = new Map(prev);
              newMap.delete(data.document_id);
              return newMap;
            });
          }, 8000);
        }
      });
    };

    setupListeners();
  }, [selectedLibrary, loadLibraryDocuments, loadKnowledgeBaseStatus]);

  // 文件拖拽处理
  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    e.dataTransfer.dropEffect = 'copy';
    setIsDragOver(true);
  };

  const handleDragEnter = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(true);
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    if (!e.currentTarget.contains(e.relatedTarget as Node)) {
      setIsDragOver(false);
    }
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    e.stopPropagation();
    setIsDragOver(false);
    
    const files = Array.from(e.dataTransfer.files);
    const supportedFiles = files.filter(file => {
      const lowerName = file.name.toLowerCase();
      return (
        lowerName.endsWith('.pdf') ||
        lowerName.endsWith('.docx') ||
        lowerName.endsWith('.txt') ||
        lowerName.endsWith('.md')
      );
    });
    
    if (supportedFiles.length > 0) {
      setSelectedFiles(prev => [...prev, ...supportedFiles]);
    } else {
      showWarning('请选择支持的文件格式（PDF、DOCX、TXT、MD）');
    }
  };

  return (
    <div style={{
      width: '100%',
      height: '100%',
      overflow: 'auto',
      background: '#f8fafc'
    }}>
      {/* 头部区域 - 统一白色样式 */}
      <div style={{
        background: 'white',
        borderBottom: '1px solid #e5e7eb',
        padding: '24px 32px',
        position: 'relative'
      }}>
        <div style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: '4px',
          background: 'linear-gradient(90deg, #667eea, #764ba2)'
        }}></div>
        
        <div style={{ position: 'relative', zIndex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', marginBottom: '8px' }}>
            <svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#667eea" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{ marginRight: '12px' }}>
              <path d="M12 5a3 3 0 1 0-5.997.125 4 4 0 0 0-2.526 5.77 4 4 0 0 0 .556 6.588A4 4 0 1 0 12 18Z"/>
              <path d="M12 5a3 3 0 1 1 5.997.125 4 4 0 0 1 2.526 5.77 4 4 0 0 1-.556 6.588A4 4 0 1 1 12 18Z"/>
              <path d="M15 13a4.5 4.5 0 0 1-3-4 4.5 4.5 0 0 1-3 4"/>
              <path d="M17.599 6.5a3 3 0 0 0 .399-1.375"/>
              <path d="M6.003 5.125A3 3 0 0 0 6.401 6.5"/>
              <path d="M3.477 10.896a4 4 0 0 1 .585-.396"/>
              <path d="M19.938 10.5a4 4 0 0 1 .585.396"/>
              <path d="M6 18a4 4 0 0 1-1.967-.516"/>
              <path d="M19.967 17.484A4 4 0 0 1 18 18"/>
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>增强知识库管理</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            智能管理文档资源，构建强大的RAG知识检索系统
          </p>
          <div style={{ marginTop: '24px', display: 'flex', gap: '12px' }}>
            <button
              onClick={() => setShowCreateLibraryModal(true)}
              style={{
                background: '#667eea',
                border: '1px solid #667eea',
                color: 'white',
                padding: '12px 24px',
                borderRadius: '12px',
                fontSize: '16px',
                fontWeight: '600',
                cursor: 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                gap: '8px',
                transition: 'all 0.3s ease',
                backdropFilter: 'blur(10px)'
              }}
              onMouseOver={(e) => {
                e.currentTarget.style.background = '#5a67d8';
                e.currentTarget.style.transform = 'translateY(-2px)';
                e.currentTarget.style.boxShadow = '0 8px 25px rgba(102, 126, 234, 0.4)';
              }}
              onMouseOut={(e) => {
                e.currentTarget.style.background = '#667eea';
                e.currentTarget.style.transform = 'translateY(0)';
                e.currentTarget.style.boxShadow = 'none';
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              创建分库
            </button>
            <button
              onClick={() => {
                loadSubLibraries();
                loadKnowledgeBaseStatus();
                loadLibraryDocuments(selectedLibrary);
              }}
              disabled={loading}
              style={{
                background: 'white',
                border: '1px solid #d1d5db',
                color: '#374151',
                padding: '12px 24px',
                borderRadius: '12px',
                fontSize: '16px',
                fontWeight: '600',
                cursor: loading ? 'not-allowed' : 'pointer',
                display: 'inline-flex',
                alignItems: 'center',
                gap: '8px',
                transition: 'all 0.3s ease',
                opacity: loading ? 0.6 : 1
              }}
              onMouseOver={(e) => {
                if (!loading) {
                  e.currentTarget.style.background = '#f9fafb';
                  e.currentTarget.style.transform = 'translateY(-2px)';
                  e.currentTarget.style.boxShadow = '0 8px 25px rgba(0, 0, 0, 0.1)';
                }
              }}
              onMouseOut={(e) => {
                if (!loading) {
                  e.currentTarget.style.background = 'white';
                  e.currentTarget.style.transform = 'translateY(0)';
                  e.currentTarget.style.boxShadow = 'none';
                }
              }}
            >
              <svg style={{ width: '20px', height: '20px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
              刷新
            </button>
          </div>
        </div>
      </div>

      <div className={`knowledge-base-management ${className}`} style={{ padding: '24px', background: 'transparent' }}>
        {/* 知识库状态概览 */}
      {status && (
        <div className="status-overview">
          <div className="status-item">
            <span className="label">总文档数:</span>
            <span className="value">{status.total_documents}</span>
          </div>
          <div className="status-item">
            <span className="label">总文本块:</span>
            <span className="value">{status.total_chunks}</span>
          </div>
          <div className="status-item">
            <span className="label">向量存储:</span>
            <span className="value">{status.vector_store_type}</span>
          </div>
          {status.embedding_model_name && (
            <div className="status-item">
              <span className="label">嵌入模型:</span>
              <span className="value">{status.embedding_model_name}</span>
            </div>
          )}
        </div>
      )}

      <div className="main-content">
        {/* 分库管理侧栏 */}
        <div className="library-sidebar">
          <h3>分库列表</h3>
          <div className="library-list">
            {subLibraries.map(library => (
              <div 
                key={library.id}
                className={`library-item ${selectedLibrary === library.id ? 'selected' : ''}`}
                onClick={() => setSelectedLibrary(library.id)}
              >
                <div className="library-info">
                  <div className="library-name">{library.name}</div>
                  <div className="library-stats" style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                    <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                      <FileText size={14} />
                      {library.document_count}
                    </span>
                    <span>|</span>
                    <span style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                      <BookOpen size={14} />
                      {library.chunk_count}
                    </span>
                  </div>
                  {library.description && (
                    <div className="library-description">{library.description}</div>
                  )}
                </div>
                <div className="library-actions">
                  {library.id !== 'default' && (
                    <>
                      <button 
                        onClick={(e) => {
                          e.stopPropagation();
                          setEditingLibrary(library);
                          setNewLibraryName(library.name);
                          setNewLibraryDescription(library.description || '');
                          setShowEditLibraryModal(true);
                        }}
                        className="library-action-btn edit"
                        style={{ display: 'flex', alignItems: 'center', gap: '4px' }}
                      >
                        <Edit size={14} />
                      </button>
                      <button 
                        onClick={(e) => {
                          e.stopPropagation();
                          deleteSubLibrary(library);
                        }}
                        className="library-action-btn delete"
                        disabled={library.id === 'default'}
                        style={{ display: 'flex', alignItems: 'center', gap: '4px' }}
                      >
                        <Trash2 size={14} />
                      </button>
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* 文档管理主区域 */}
        <div className="documents-main">
          <div className="section-header">
            <h3 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <FileText size={20} />
              {subLibraries.find(lib => lib.id === selectedLibrary)?.name || '默认分库'} 
              - 文档管理
            </h3>
          </div>

          {/* 文件上传区域 */}
          <div 
            className={`upload-zone ${isDragOver ? 'drag-over' : ''} ${uploading ? 'uploading' : ''}`}
            onDragEnter={handleDragEnter}
            onDragOver={handleDragOver}
            onDragLeave={handleDragLeave}
            onDrop={handleDrop}
          >
            <div 
              className="upload-content"
              onDragEnter={handleDragEnter}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
            >
              <div className="upload-icon">📁</div>
              <div className="upload-text">
                {uploading ? '上传中...' : '拖拽文件到此处或点击选择文件'}
              </div>
              <div className="upload-hint">
                支持格式: PDF, DOCX, TXT, MD
              </div>
              <input
                type="file"
                multiple
                accept=".pdf,.PDF,.docx,.DOCX,.txt,.TXT,.md,.MD"
                onChange={(e) => {
                  const files = Array.from(e.target.files || []);
                  setSelectedFiles(prev => [...prev, ...files]);
                }}
                style={{ display: 'none' }}
                id="file-input"
              />
              <label htmlFor="file-input" className="btn btn-primary">
                选择文件
              </label>
            </div>
            
            {selectedFiles.length > 0 && (
              <div className="selected-files">
                <h4>待上传文件 ({selectedFiles.length})</h4>
                <div className="file-list">
                  {selectedFiles.map((file, index) => (
                    <div key={index} className="file-item">
                      <span className="file-name">{file.name}</span>
                      <span className="file-size">
                        {(file.size / 1024 / 1024).toFixed(2)} MB
                      </span>
                      <button 
                        onClick={() => setSelectedFiles(prev => 
                          prev.filter((_, i) => i !== index)
                        )}
                        className="btn-remove"
                      >
                        ✕
                      </button>
                    </div>
                  ))}
                </div>
                <div className="upload-actions">
                  <button 
                    onClick={uploadDocuments}
                    disabled={uploading}
                    className="btn btn-success"
                  >
                    {uploading ? '上传中...' : `上传到 ${subLibraries.find(lib => lib.id === selectedLibrary)?.name}`}
                  </button>
                  <button 
                    onClick={() => setSelectedFiles([])}
                    disabled={uploading}
                    className="btn btn-secondary"
                  >
                    清空
                  </button>
                </div>
              </div>
            )}
          </div>

          {/* 文档处理进度区域 */}
          {processingDocuments.size > 0 && (
            <div style={{
              background: 'white',
              border: '1px solid #e5e7eb',
              borderRadius: '12px',
              padding: '20px',
              marginBottom: '24px',
              boxShadow: '0 2px 4px rgba(0, 0, 0, 0.05)'
            }}>
              <div style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
                marginBottom: '16px'
              }}>
                <div style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px',
                  fontSize: '16px',
                  fontWeight: '600',
                  color: '#374151'
                }}>
                  <div style={{
                    width: '20px',
                    height: '20px',
                    border: '2px solid #3b82f6',
                    borderTop: '2px solid transparent',
                    borderRadius: '50%',
                    animation: 'spin 1s linear infinite'
                  }} />
                  正在处理文档 ({processingDocuments.size} 个)
                </div>
                
                {/* 总体进度 */}
                <div style={{
                  fontSize: '14px',
                  color: '#6b7280',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '8px'
                }}>
                  <span>总体进度:</span>
                  <div style={{
                    width: '100px',
                    height: '6px',
                    backgroundColor: '#f3f4f6',
                    borderRadius: '3px',
                    overflow: 'hidden'
                  }}>
                    <div style={{
                      width: `${(Array.from(processingDocuments.values()).reduce((sum, doc) => sum + doc.progress, 0) / processingDocuments.size) * 100}%`,
                      height: '100%',
                      backgroundColor: '#3b82f6',
                      borderRadius: '3px',
                      transition: 'width 0.3s ease'
                    }} />
                  </div>
                  <span style={{ fontWeight: '500', minWidth: '35px' }}>
                    {Math.round((Array.from(processingDocuments.values()).reduce((sum, doc) => sum + doc.progress, 0) / processingDocuments.size) * 100)}%
                  </span>
                </div>
              </div>
              
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {Array.from(processingDocuments.values()).map((doc) => (
                  <ProgressBar
                    key={doc.document_id}
                    progress={doc.progress}
                    status={doc.status}
                    fileName={doc.file_name}
                    totalChunks={doc.total_chunks}
                    chunksProcessed={doc.chunks_processed}
                  />
                ))}
              </div>
            </div>
          )}

          {/* 文档列表 */}
          <div className="documents-section">
            <div className="documents-header">
              <h4 style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                <FileText size={20} />
                {subLibraries.find(lib => lib.id === selectedLibrary)?.name || '默认分库'} - 文档管理
              </h4>
            </div>
            
            {loading ? (
              <div className="loading-state">加载中...</div>
            ) : documents.length === 0 ? (
              <div className="empty-state">
                <div className="empty-icon" style={{ display: 'flex', justifyContent: 'center', marginBottom: '16px' }}>
                  <FolderOpen size={48} color="#ccc" />
                </div>
                <div className="empty-text">该分库中暂无文档</div>
                <div className="empty-hint">上传一些文档开始使用吧！</div>
              </div>
            ) : (
              <div className="documents-grid">
                {documents.map((doc: any) => (
                  <div key={doc.id} className="document-card">
                    <div className="doc-header">
                      <div className="doc-name" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                        <FileText size={16} />
                        {doc.file_name}
                      </div>
                      <div className="doc-actions">
                        <button 
                          onClick={async () => {
                            const confirmDelete = window.confirm(`确定要删除文档 "${doc.file_name}" 吗？此操作不可撤销。`);
                            if (!confirmDelete) return;

                            try {
                              console.log('开始删除文档:', doc.id, doc.file_name);
                              await TauriAPI.ragDeleteDocument(doc.id);
                              console.log('文档删除成功，开始刷新数据');
                              showSuccess(`文档 "${doc.file_name}" 删除成功`);
                              
                              // 前端即时移除该文档以提升体验
                              setDocuments(prev => prev.filter(d => d.id !== doc.id));
                              // 更新知识库状态和分库统计
                              await loadKnowledgeBaseStatus();
                              await loadSubLibraries();
                            } catch (error) {
                              console.error('删除文档失败:', error);
                              showError(`删除文档失败: ${error}`);
                            }
                          }}
                          className="btn-icon"
                          title="删除文档"
                          style={{ display: 'flex', alignItems: 'center', gap: '4px' }}
                        >
                          <Trash2 size={14} />
                        </button>
                      </div>
                    </div>
                    <div className="doc-info">
                      <div className="doc-meta">
                        <span>大小: {formatFileSize(doc.file_size)}</span>
                        <span>文本块: {doc.total_chunks}</span>
                        <span>上传时间: {formatDate(doc.created_at)}</span>
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* 创建分库模态框 */}
      {showCreateLibraryModal && (
        <div className="modal-overlay">
          <div className="modal">
            <div className="modal-header">
              <h3>创建新分库</h3>
              <button 
                onClick={() => setShowCreateLibraryModal(false)}
                className="btn-close"
                style={{ display: 'flex', alignItems: 'center' }}
              >
                <X size={16} />
              </button>
            </div>
            <div className="modal-body">
              <div className="form-group">
                <label>分库名称 *</label>
                <input
                  type="text"
                  value={newLibraryName}
                  onChange={(e) => setNewLibraryName(e.target.value)}
                  placeholder="请输入分库名称"
                  className="form-input"
                />
              </div>
              <div className="form-group">
                <label>分库描述</label>
                <textarea
                  value={newLibraryDescription}
                  onChange={(e) => setNewLibraryDescription(e.target.value)}
                  placeholder="请输入分库描述（可选）"
                  className="form-textarea"
                  rows={3}
                />
              </div>
            </div>
            <div className="modal-footer">
              <button 
                onClick={() => setShowCreateLibraryModal(false)}
                className="btn btn-secondary"
              >
                取消
              </button>
              <button 
                onClick={createSubLibrary}
                className="btn btn-primary"
                disabled={!newLibraryName.trim()}
              >
                创建
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 编辑分库模态框 */}
      {showEditLibraryModal && editingLibrary && (
        <div className="modal-overlay">
          <div className="modal">
            <div className="modal-header">
              <h3>编辑分库</h3>
              <button 
                onClick={() => {
                  setShowEditLibraryModal(false);
                  setEditingLibrary(null);
                }}
                className="btn-close"
                style={{ display: 'flex', alignItems: 'center' }}
              >
                <X size={16} />
              </button>
            </div>
            <div className="modal-body">
              <div className="form-group">
                <label>分库名称 *</label>
                <input
                  type="text"
                  value={newLibraryName}
                  onChange={(e) => setNewLibraryName(e.target.value)}
                  placeholder="请输入分库名称"
                  className="form-input"
                />
              </div>
              <div className="form-group">
                <label>分库描述</label>
                <textarea
                  value={newLibraryDescription}
                  onChange={(e) => setNewLibraryDescription(e.target.value)}
                  placeholder="请输入分库描述（可选）"
                  className="form-textarea"
                  rows={3}
                />
              </div>
            </div>
            <div className="modal-footer">
              <button 
                onClick={() => {
                  setShowEditLibraryModal(false);
                  setEditingLibrary(null);
                }}
                className="btn btn-secondary"
              >
                取消
              </button>
              <button 
                onClick={updateSubLibrary}
                className="btn btn-primary"
                disabled={!newLibraryName.trim()}
              >
                保存
              </button>
            </div>
          </div>
        </div>
      )}
      </div>
    </div>
  );
};

export default EnhancedKnowledgeBaseManagement;