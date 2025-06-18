import React, { useState, useEffect, useCallback } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { useNotification } from '../hooks/useNotification';
import type { 
  RagDocument, 
  KnowledgeBaseStatusPayload, 
  RagProcessingEvent, 
  RagDocumentStatusEvent 
} from '../types';
import { listen } from '@tauri-apps/api/event';
import './KnowledgeBaseManagement.css';
import { 
  BarChart3, FolderOpen, Target, RefreshCcw, Trash2, Upload, 
  FileText, CheckCircle, BookOpen, Sparkles, Rocket, 
  Calendar, Scale, Puzzle, Settings, File, Clock
} from 'lucide-react';

interface KnowledgeBaseManagementProps {
  className?: string;
}

export const KnowledgeBaseManagement: React.FC<KnowledgeBaseManagementProps> = ({ 
  className = '' 
}) => {
  const [documents, setDocuments] = useState<RagDocument[]>([]);
  const [status, setStatus] = useState<KnowledgeBaseStatusPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [processingDocuments, setProcessingDocuments] = useState<Map<string, RagDocumentStatusEvent>>(new Map());
  const [selectedFiles, setSelectedFiles] = useState<File[]>([]);

  const { showSuccess, showError, showWarning } = useNotification();
  
  // 拖拽状态
  const [isDragOver, setIsDragOver] = useState(false);

  // 加载知识库状态
  const loadKnowledgeBaseStatus = useCallback(async () => {
    try {
      setLoading(true);
      const [statusData, documentsData] = await Promise.all([
        TauriAPI.ragGetKnowledgeBaseStatus(),
        TauriAPI.ragGetAllDocuments()
      ]);
      
      setStatus(statusData);
      setDocuments(documentsData);
    } catch (error) {
      console.error('加载知识库状态失败:', error);
      showError(`加载知识库状态失败: ${error}`);
    } finally {
      setLoading(false);
    }
  }, [showError]);

  // 初始加载
  useEffect(() => {
    loadKnowledgeBaseStatus();
  }, [loadKnowledgeBaseStatus]);

  // 监听RAG处理事件
  useEffect(() => {
    const setupEventListeners = async () => {
      try {
        // 监听整体处理状态
        const unlistenProcessing = await listen<RagProcessingEvent>('rag_processing_status', (event) => {
          const data = event.payload;
          if (data.status === 'completed') {
            showSuccess(data.message);
            loadKnowledgeBaseStatus(); // 重新加载状态
          } else if (data.status === 'error') {
            showError(`处理失败: ${data.message}`);
            // 清空处理状态，让用户能够重试
            setProcessingDocuments(new Map());
          } else if (data.status === 'progress') {
            // 可以显示整体进度信息
            console.log(`整体处理进度: ${Math.round(data.progress * 100)}%`);
          }
        });

        // 监听文档处理状态
        const unlistenDocument = await listen<RagDocumentStatusEvent>('rag_document_status', (event) => {
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
            
            // 提供一些常见错误的解决建议
            if (errorMsg.includes('Unsupported file format')) {
              showWarning('请确保文件格式为 .txt, .md, .pdf 或 .docx');
            } else if (errorMsg.includes('File not found')) {
              showWarning('文件可能已被移动或删除，请重新选择文件');
            } else if (errorMsg.includes('Permission denied')) {
              showWarning('没有文件访问权限，请检查文件权限设置');
            }
            
            // 保持失败状态显示一段时间，然后清除
            setTimeout(() => {
              setProcessingDocuments(prev => {
                const newMap = new Map(prev);
                newMap.delete(data.document_id);
                return newMap;
              });
            }, 8000);
          }
        });

        return () => {
        };
      } catch (error) {
        console.error('设置事件监听器失败:', error);
      }
    };

    setupEventListeners();
  }, [showSuccess, showError, loadKnowledgeBaseStatus]);
  
  // 监听文件拖拽事件
  useEffect(() => {
    const setupFileDragListeners = async () => {
      try {
        // 尝试使用 Tauri v2 API
        const { listen } = await import('@tauri-apps/api/event');
        
        // 监听文件拖拽进入
        const unlistenDragEnter = await listen('tauri://file-drop-hover', () => {
          setIsDragOver(true);
          console.log('文件拖拽进入');
        });

        // 监听文件拖拽离开
        const unlistenDragLeave = await listen('tauri://file-drop-cancelled', () => {
          setIsDragOver(false);
          console.log('文件拖拽离开');
        });

        // 监听文件拖拽放下
        const unlistenDrop = await listen<string[]>('tauri://file-drop', async (event) => {
          setIsDragOver(false);
          
          const filePaths = event.payload;
          console.log('拖拽文件路径:', filePaths);
          
          // 过滤支持的文件格式
          const supportedExtensions = ['.txt', '.md', '.markdown', '.pdf', '.docx'];
          const validFiles = filePaths.filter(path => {
            const extension = path.toLowerCase().substring(path.lastIndexOf('.'));
            return supportedExtensions.includes(extension);
          });

          if (validFiles.length === 0) {
            showWarning('没有支持的文件格式。支持的格式：.txt, .md, .pdf, .docx');
            return;
          }

          if (validFiles.length !== filePaths.length) {
            showWarning(`已过滤不支持的文件，将上传 ${validFiles.length} 个支持的文件`);
          }

          try {
            setUploading(true);
            showSuccess(`开始处理 ${validFiles.length} 个文件...`);
            
            await TauriAPI.ragAddDocuments(validFiles);
            showSuccess(`成功拖拽上传 ${validFiles.length} 个文件到知识库！`);
            
            // 重新加载知识库状态
            await loadKnowledgeBaseStatus();
          } catch (error) {
            console.error('拖拽上传失败:', error);
            let errorMessage = '拖拽上传失败';
            
            if (typeof error === 'string') {
              errorMessage = `拖拽上传失败: ${error}`;
            } else if (error instanceof Error) {
              errorMessage = `拖拽上传失败: ${error.message}`;
            }
            
            // 提供更有用的错误信息和建议
            if (errorMessage.includes('系统找不到指定的文件') || errorMessage.includes('文件不存在')) {
              errorMessage += '\n\n建议：\n• 确保文件未被移动或删除\n• 尝试使用"选择文件"按钮代替拖拽\n• 检查文件名是否包含特殊字符';
            }
            
            showError(errorMessage);
          } finally {
            setUploading(false);
          }
        });

        console.log('文件拖拽监听器设置完成');
        
        return () => {
          // 清理拖拽监听器
        };
      } catch (error) {
        console.error('设置文件拖拽监听器失败:', error);
        showWarning('文件拖拽功能暂时不可用，请等待应用完全加载');
      }
    };

    setupFileDragListeners();
  }, [showSuccess, showError, showWarning, loadKnowledgeBaseStatus]);

  // 处理文件选择
  const handleFileSelect = (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files || []);
    const supportedTypes = ['.txt', '.md', '.markdown', '.pdf', '.docx'];
    
    const validFiles = files.filter(file => {
      const extension = file.name.toLowerCase().substring(file.name.lastIndexOf('.'));
      return supportedTypes.includes(extension);
    });

    if (validFiles.length !== files.length) {
      showWarning(`只支持 ${supportedTypes.join(', ')} 格式的文件`);
    }

    setSelectedFiles(validFiles);
  };

  // 上传文档
  const handleUploadDocuments = async () => {
    if (selectedFiles.length === 0) {
      showWarning('请先选择要上传的文件');
      return;
    }

    try {
      setUploading(true);
      
      const documents: Array<{ file_name: string; base64_content: string }> = [];
      
      // 读取文件内容，像Anki组件一样处理
      for (const file of selectedFiles) {
        try {
          const content = await new Promise<string>((resolve, reject) => {
            const reader = new FileReader();
            reader.onload = (e) => {
              const result = e.target?.result as string;
              resolve(result);
            };
            reader.onerror = (e) => reject(e);
            
            // 根据文件类型选择读取方式
            if (file.type === 'text/plain' || file.name.endsWith('.txt') || file.name.endsWith('.md')) {
              reader.readAsText(file);
            } else {
              // 对于PDF、DOCX等二进制文件，读取为ArrayBuffer然后转换为base64
              reader.onload = (e) => {
                const arrayBuffer = e.target?.result as ArrayBuffer;
                const base64 = btoa(String.fromCharCode(...new Uint8Array(arrayBuffer)));
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
        }
      }

      if (documents.length === 0) {
        showError('没有文件被成功处理');
        return;
      }

      showSuccess(`开始处理 ${documents.length} 个文件...`);
      
      await TauriAPI.ragAddDocumentsFromContent(documents);
      showSuccess(`成功上传 ${documents.length} 个文件到知识库！`);
      
      // 清空选择的文件
      setSelectedFiles([]);
      
      // 重新加载知识库状态
      await loadKnowledgeBaseStatus();
      
    } catch (error) {
      console.error('上传文件失败:', error);
      let errorMessage = '上传文件失败';
      
      if (typeof error === 'string') {
        errorMessage = `上传失败: ${error}`;
      } else if (error instanceof Error) {
        errorMessage = `上传失败: ${error.message}`;
      } else if (error && typeof error === 'object' && 'message' in error) {
        errorMessage = `上传失败: ${(error as any).message}`;
      }
      
      showError(errorMessage);
      
      // 提供重试建议
      showWarning('请检查文件格式是否正确');
    } finally {
      setUploading(false);
    }
  };

  // 删除文档
  const handleDeleteDocument = async (documentId: string, documentName: string) => {
    if (!confirm(`确定要删除文档 "${documentName}" 吗？此操作不可撤销。`)) {
      return;
    }

    try {
      console.log('开始删除文档:', documentId, documentName);
      await TauriAPI.ragDeleteDocument(documentId);
      console.log('文档删除成功，开始刷新数据');
      showSuccess(`文档 "${documentName}" 已删除`);
      
      // 重新加载知识库状态和文档列表
      await loadKnowledgeBaseStatus();
    } catch (error) {
      console.error('删除文档失败:', error);
      showError(`删除文档失败: ${error}`);
    }
  };

  // 清空知识库
  const handleClearKnowledgeBase = async () => {
    if (!confirm('确定要清空整个知识库吗？此操作将删除所有文档和向量数据，不可撤销。')) {
      return;
    }

    try {
      await TauriAPI.ragClearKnowledgeBase();
      showSuccess('知识库已清空');
      loadKnowledgeBaseStatus();
    } catch (error) {
      console.error('清空知识库失败:', error);
      showError(`清空知识库失败: ${error}`);
    }
  };

  // 格式化文件大小
  const formatFileSize = (bytes?: number) => {
    if (!bytes) return '未知';
    if (bytes < 1024) return `${bytes} B`;
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  };

  // 格式化日期
  const formatDate = (dateString: string) => {
    return new Date(dateString).toLocaleString('zh-CN');
  };

  // 获取处理状态显示
  const getProcessingStatusDisplay = (docId: string) => {
    const processing = processingDocuments.get(docId);
    if (!processing) return null;

    const getStatusText = (status: string) => {
      const statusMap: Record<string, string> = {
        'Reading': '读取文件',
        'Preprocessing': '预处理',
        'Chunking': '文本分块',
        'Embedding': '生成向量',
        'Storing': '存储数据',
        'Completed': '处理完成',
        'Failed': '处理失败'
      };
      return statusMap[status] || status;
    };

    const getStatusClass = (status: string) => {
      const classMap: Record<string, string> = {
        'Pending': 'pending',
        'Reading': 'reading',
        'Preprocessing': 'reading',
        'Chunking': 'reading',
        'Embedding': 'reading',
        'Storing': 'reading',
        'Completed': 'completed',
        'Failed': 'failed'
      };
      return classMap[status] || 'reading';
    };

    return (
      <div className={`kb-processing-status ${getStatusClass(processing.status)}`}>
        <div className="kb-processing-header">
          <span className="kb-processing-title">
            {getStatusText(processing.status)}
          </span>
          <span className="kb-processing-percentage">
            {Math.round(processing.progress * 100)}%
          </span>
        </div>
        {processing.status !== 'Completed' && processing.status !== 'Failed' && (
          <div className="kb-processing-bar">
            <div 
              className="kb-processing-progress"
              style={{ width: `${processing.progress * 100}%` }}
            />
          </div>
        )}
        {processing.chunks_processed > 0 && (
          <div className="kb-processing-chunks">
            <div className="kb-chunks-info" style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
              <BarChart3 size={16} />
              已处理 {processing.chunks_processed} / {processing.total_chunks} 个文本块
            </div>
            {processing.total_chunks > 0 && (
              <div className="kb-chunks-progress">
                <div 
                  className="kb-chunks-progress-bar"
                  style={{ 
                    width: `${(processing.chunks_processed / processing.total_chunks) * 100}%` 
                  }}
                />
              </div>
            )}
          </div>
        )}
      </div>
    );
  };

  return (
    <div className={`knowledge-base-container ${className} ${isDragOver ? 'drag-over' : ''}`}>
      {isDragOver && (
        <div className="kb-drag-overlay">
          <div className="kb-drag-overlay-content">
            <div className="kb-drag-icon" style={{ display: 'flex', justifyContent: 'center', marginBottom: '16px' }}>
              <FolderOpen size={48} color="#667eea" />
            </div>
            <h2 className="kb-drag-title">松开鼠标上传文档</h2>
            <p className="kb-drag-subtitle">
              支持 .txt, .md, .pdf, .docx 格式的文档
            </p>
          </div>
        </div>
      )}
      <div className="knowledge-base-content">
        {/* 页面标题 */}
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
              <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>RAG 知识库管理</h1>
            </div>
            <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
              智能文档管理系统，为AI提供丰富的知识背景，让分析更加精准和深入
            </p>
          </div>
        </div>

        {/* 知识库状态 */}
        <div className="kb-stats-grid">
          <div className="kb-stat-card">
            <div className="kb-stat-header">
              <div className="kb-stat-icon blue">
                <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.746 0 3.332.477 4.5 1.253v13C19.832 18.477 18.246 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                </svg>
              </div>
              <div>
                <div className="kb-stat-label">Total Documents</div>
                <div className="kb-stat-value">
                  {loading ? '...' : (status?.total_documents || 0)}
                </div>
              </div>
            </div>
            <div className="kb-stat-label">文档总数</div>
            <div className="kb-stat-progress">
              <div 
                className="kb-stat-progress-bar" 
                style={{width: loading ? '0%' : `${Math.min((status?.total_documents || 0) * 10, 100)}%`}}
              />
            </div>
          </div>
          
          <div className="kb-stat-card">
            <div className="kb-stat-header">
              <div className="kb-stat-icon green">
                <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8 7v8a2 2 0 002 2h6M8 7V5a2 2 0 012-2h4.586a1 1 0 01.707.293l4.414 4.414a1 1 0 01.293.707V15a2 2 0 01-2 2v0a2 2 0 01-2-2v-5a2 2 0 00-2-2H8z" />
                </svg>
              </div>
              <div>
                <div className="kb-stat-label">Text Chunks</div>
                <div className="kb-stat-value">
                  {loading ? '...' : (status?.total_chunks || 0)}
                </div>
              </div>
            </div>
            <div className="kb-stat-label">文本块总数</div>
            <div className="kb-stat-progress">
              <div 
                className="kb-stat-progress-bar" 
                style={{width: loading ? '0%' : `${Math.min((status?.total_chunks || 0) * 2, 100)}%`}}
              />
            </div>
          </div>
          
          <div className="kb-stat-card">
            <div className="kb-stat-header">
              <div className="kb-stat-icon purple">
                <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4m0 5c0 2.21-3.582 4-8 4s-8-1.79-8-4" />
                </svg>
              </div>
              <div>
                <div className="kb-stat-label">Vector Store</div>
                <div className="kb-stat-value" style={{fontSize: '1.5rem'}}>
                  {loading ? '...' : (status?.vector_store_type || 'SQLite')}
                </div>
              </div>
            </div>
            <div className="kb-stat-label">向量存储</div>
            <div className="kb-stat-status">
              <div className="kb-status-dot green"></div>
              <span>运行中</span>
            </div>
          </div>
          
          <div className="kb-stat-card">
            <div className="kb-stat-header">
              <div className="kb-stat-icon orange">
                <svg style={{ width: '24px', height: '24px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                </svg>
              </div>
              <div>
                <div className="kb-stat-label">AI Model</div>
                <div className="kb-stat-value" style={{fontSize: '1rem'}}>
                  {loading ? '...' : (status?.embedding_model_name || '未配置')}
                </div>
              </div>
            </div>
            <div className="kb-stat-label">嵌入模型</div>
            <div className="kb-stat-status">
              <div className={`kb-status-dot ${status?.embedding_model_name ? 'green' : 'red'}`}></div>
              <span>{status?.embedding_model_name ? '已配置' : '待配置'}</span>
            </div>
          </div>
        </div>

        {/* 文件上传区域 */}
        <div className="kb-upload-section">
          <div className="kb-upload-header">
            <div className="kb-upload-icon">
              <svg style={{ width: '24px', height: '24px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M9 19l3 3m0 0l3-3m-3 3V10" />
              </svg>
            </div>
            <div>
              <div className="kb-upload-title">上传文档</div>
              <div className="kb-upload-subtitle">支持多种格式，智能解析文档内容</div>
            </div>
          </div>
        
          <div>
            <div>
              <label htmlFor="file-upload" style={{fontSize: '1.1rem', fontWeight: '600', color: '#2d3748', marginBottom: '1rem', display: 'flex', alignItems: 'center', gap: '8px'}}>
                <svg style={{ width: '20px', height: '20px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                </svg>
                选择文档文件
              </label>
              <div style={{marginBottom: '1.5rem'}}>
                <div style={{fontSize: '0.9rem', color: '#666', marginBottom: '0.75rem', display: 'flex', alignItems: 'center', gap: '8px'}}>
                  <Sparkles size={16} />
                  支持格式:
                </div>
                <div className="kb-format-tags">
                  <span className="kb-format-tag txt">
                    <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                    </svg>
                    .txt
                  </span>
                  <span className="kb-format-tag md">
                    <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                    </svg>
                    .md
                  </span>
                  <span className="kb-format-tag pdf">
                    <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.746 0 3.332.477 4.5 1.253v13C19.832 18.477 18.246 18 16.5 18c-1.746 0-3.332.477-4.5 1.253" />
                    </svg>
                    .pdf
                  </span>
                  <span className="kb-format-tag docx">
                    <svg style={{ width: '16px', height: '16px', marginRight: '4px' }} fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
                    </svg>
                    .docx
                  </span>
                </div>
              </div>
              <div style={{position: 'relative'}}>
                <input
                  id="file-upload"
                  type="file"
                  multiple
                  accept=".txt,.md,.markdown,.pdf,.docx"
                  onChange={handleFileSelect}
                  style={{position: 'absolute', inset: '0', width: '100%', height: '100%', opacity: 0, cursor: 'pointer', zIndex: 10}}
                />
                <div className="kb-upload-zone">
                  <div className="kb-upload-zone-icon" style={{ display: 'flex', justifyContent: 'center', marginBottom: '16px' }}>
                    <FolderOpen size={32} color="#667eea" />
                  </div>
                  <h3 className="kb-upload-zone-title">拖拽文件到窗口任意位置</h3>
                  <p className="kb-upload-zone-subtitle">支持 .txt、.md、.pdf、.docx 格式文档</p>
                  <div className="kb-upload-zone-button" style={{ display: 'flex', alignItems: 'center', gap: '8px', justifyContent: 'center' }}>
                    <Target size={16} />
                    推荐使用拖拽上传
                  </div>
                </div>
              </div>
            </div>

            {selectedFiles.length > 0 && (
              <div className="kb-selected-files">
                <div className="kb-selected-header">
                  <div className="kb-selected-icon" style={{ display: 'flex', alignItems: 'center' }}>
                    <CheckCircle size={20} color="green" />
                  </div>
                  <h3 className="kb-selected-title">
                    已选择 {selectedFiles.length} 个文件
                  </h3>
                  <div className="kb-selected-badge">
                    准备就绪
                  </div>
                </div>
                <div className="kb-file-list">
                  {selectedFiles.map((file, index) => (
                    <div key={index} className="kb-file-item">
                      <div className="kb-file-icon" style={{ display: 'flex', alignItems: 'center' }}>
                        <FileText size={20} color="#667eea" />
                      </div>
                      <div className="kb-file-info">
                        <div className="kb-file-name">{file.name}</div>
                        <div className="kb-file-meta">
                          <span className="kb-file-size">{formatFileSize(file.size)}</span>
                          <span className="kb-file-status">等待上传</span>
                        </div>
                      </div>
                      <div className="kb-file-check" style={{ display: 'flex', alignItems: 'center' }}>
                        <CheckCircle size={16} color="green" />
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            )}

            <div className="kb-action-buttons">
              <button
                onClick={handleUploadDocuments}
                disabled={uploading || selectedFiles.length === 0}
                className="kb-button primary"
              >
                {uploading ? (
                  <>
                    <div style={{
                      width: '20px',
                      height: '20px',
                      border: '2px solid transparent',
                      borderTop: '2px solid white',
                      borderRadius: '50%',
                      animation: 'spin 1s linear infinite'
                    }} />
                    <span>处理中...</span>
                  </>
                ) : (
                  <>
                    <Upload size={16} />
                    <span>处理文件 ({selectedFiles.length})</span>
                  </>
                )}
              </button>

              <button
                onClick={loadKnowledgeBaseStatus}
                disabled={loading}
                className="kb-button secondary"
              >
                <RefreshCcw size={16} />
                <span>刷新状态</span>
              </button>
            </div>
          </div>
        </div>

        {/* 文档列表 */}
        <div className="kb-documents-section">
          <div className="kb-documents-header">
            <div className="kb-documents-title-group">
              <div className="kb-documents-icon" style={{ display: 'flex', alignItems: 'center' }}>
                <BookOpen size={24} color="#667eea" />
              </div>
              <div>
                <div className="kb-documents-title">文档库</div>
                <div className="kb-documents-subtitle">管理您的知识库文档</div>
              </div>
            </div>
            <button
              onClick={handleClearKnowledgeBase}
              disabled={!documents.length || loading}
              className="kb-button danger"
              style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
            >
              <Trash2 size={16} />
              <span>清空知识库</span>
            </button>
          </div>

          <div style={{
            overflowX: 'auto',
            overflowY: 'visible',
            maxHeight: '70vh',
            position: 'relative'
          }}>
            {loading ? (
              <div className="kb-loading">
                <div className="kb-loading-icon">
                  <div className="kb-loading-spinner"></div>
                  <div className="kb-loading-inner" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                    <BookOpen size={24} color="#667eea" />
                  </div>
                </div>
                <h3 className="kb-loading-title">正在加载文档...</h3>
                <p className="kb-loading-subtitle">请稍候，正在获取您的知识库内容</p>
                <div className="kb-loading-dots">
                  <div className="kb-loading-dot"></div>
                  <div className="kb-loading-dot"></div>
                  <div className="kb-loading-dot"></div>
                </div>
              </div>
            ) : documents.length === 0 ? (
              <div className="kb-empty">
                <div className="kb-empty-icon">
                  <div className="kb-empty-circle" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                    <BookOpen size={48} color="#667eea" />
                  </div>
                </div>
                <h3 className="kb-empty-title">
                  知识库等待您的第一份文档
                </h3>
                <p className="kb-empty-subtitle" style={{ display: 'flex', alignItems: 'center', gap: '8px', justifyContent: 'center' }}>
                  <Rocket size={16} />
                  上传一些文档开始使用RAG功能，让AI拥有丰富的知识背景，为您提供更加精准的分析结果
                </p>
                <div className="kb-empty-formats">
                  <div className="kb-format-tag pdf" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <File size={16} />
                    支持 PDF
                  </div>
                  <div className="kb-format-tag docx" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <File size={16} />
                    支持 Word
                  </div>
                  <div className="kb-format-tag md" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <File size={16} />
                    支持 Markdown
                  </div>
                  <div className="kb-format-tag txt" style={{ display: 'flex', alignItems: 'center', gap: '4px' }}>
                    <File size={16} />
                    支持 TXT
                  </div>
                </div>
                <div className="kb-empty-cta" style={{ display: 'flex', alignItems: 'center', gap: '8px', justifyContent: 'center' }}>
                  <Upload size={16} />
                  立即上传文档
                </div>
              </div>
            ) : (
              <div className="kb-table-container">
                <table className="kb-table">
                  <thead>
                    <tr>
                      <th>
                        <div style={{display: 'flex', alignItems: 'center', gap: '0.5rem'}}>
                          <FileText size={18} />
                          文档信息
                        </div>
                      </th>
                      <th>
                        <div style={{display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '0.5rem'}}>
                          <Scale size={18} />
                          大小
                        </div>
                      </th>
                      <th>
                        <div style={{display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '0.5rem'}}>
                          <Puzzle size={18} />
                          文本块
                        </div>
                      </th>
                      <th>
                        <div style={{display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '0.5rem'}}>
                          <Clock size={18} />
                          上传时间
                        </div>
                      </th>
                      <th>
                        <div style={{display: 'flex', alignItems: 'center', justifyContent: 'center', gap: '0.5rem'}}>
                          <Settings size={18} />
                          操作
                        </div>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {documents.map((doc, index) => (
                      <tr key={doc.id}>
                        <td>
                          <div className="kb-doc-cell">
                            <div className="kb-doc-icon-wrapper">
                              <div className="kb-doc-icon" style={{ display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
                                <FileText size={20} color="#667eea" />
                              </div>
                              <div className="kb-doc-number">{index + 1}</div>
                            </div>
                            <div className="kb-doc-info">
                              <div className="kb-doc-name">
                                {doc.file_name}
                              </div>
                              <div className="kb-doc-meta">
                                <span className="kb-doc-id">
                                  ID: {doc.id.substring(0, 8)}...
                                </span>
                                <span className="kb-doc-status">
                                  已处理
                                </span>
                              </div>
                              {processingDocuments.has(doc.id) && (
                                <div>
                                  {getProcessingStatusDisplay(doc.id)}
                                </div>
                              )}
                            </div>
                          </div>
                        </td>
                        <td className="kb-cell-center">
                          <div className="kb-file-size-display">
                            {formatFileSize(doc.file_size)}
                          </div>
                          <div className="kb-file-size-label">
                            文件大小
                          </div>
                        </td>
                        <td className="kb-cell-center">
                          <div className="kb-chunks-badge">
                            <div className="kb-chunks-dot"></div>
                            {doc.total_chunks} 块
                          </div>
                        </td>
                        <td className="kb-cell-center">
                          <div className="kb-date-display">
                            {formatDate(doc.created_at)}
                          </div>
                          <div className="kb-date-label">
                            上传时间
                          </div>
                        </td>
                        <td className="kb-cell-center">
                          <button
                            onClick={() => handleDeleteDocument(doc.id, doc.file_name)}
                            className="kb-delete-button"
                            style={{ display: 'flex', alignItems: 'center', gap: '8px' }}
                          >
                            <div className="kb-delete-content">
                              <Trash2 size={16} />
                              <span>删除</span>
                            </div>
                          </button>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
};

export default KnowledgeBaseManagement;