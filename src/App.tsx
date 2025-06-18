import { useState, useEffect, useCallback, useMemo } from 'react';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { MistakeLibrary } from './components/MistakeLibrary';
import { Settings } from './components/Settings';
import { BatchAnalysis } from './components/BatchAnalysis';
import UniversalAppChatHost, { HostedChatApiProvider, UniversalAppChatHostProps } from './components/UniversalAppChatHost';
// DEPRECATED: 单次回顾模块已废弃 - 2024年6月5日，功能被统一回顾替代
// import { ReviewAnalysis } from './components/ReviewAnalysis';
import { Dashboard } from './components/Dashboard';
import ReviewAnalysisDashboard from './components/ReviewAnalysisDashboard';
import CreateReviewAnalysisView from './components/CreateReviewAnalysisView';
import ReviewAnalysisSessionView from './components/ReviewAnalysisSessionView';
// DEPRECATED: import { ReviewAnalysisLibrary } from './components/ReviewAnalysisLibrary';
import { DataImportExport } from './components/DataImportExport';
import { MessageWithThinking } from './chat-core';
import AnkiCardGeneration from './components/AnkiCardGeneration';
import { EnhancedKnowledgeBaseManagement } from './components/EnhancedKnowledgeBaseManagement';
import { EnhancedRagQueryView } from './components/EnhancedRagQueryView';
import ImageOcclusion from './components/ImageOcclusion';
import TemplateManagementPage from './components/TemplateManagementPage';
import KnowledgeGraphManagement from './components/KnowledgeGraphManagement';
import { WindowControls } from './components/WindowControls';
import { useWindowDrag } from './hooks/useWindowDrag';
import { ImageViewer } from './components/ImageViewer';
import { GeminiAdapterTest } from './components/GeminiAdapterTest';
import { ModernSidebar } from './components/ModernSidebar';
// 移除不再使用的streamHandler import
import { TauriAPI, MistakeItem } from './utils/tauriApi';
import { SubjectProvider } from './contexts/SubjectContext';
import { UnifiedSubjectSelector } from './components/shared/UnifiedSubjectSelector';
import './App.css';
import './DeepStudent.css';
import './components/AnkiCardGeneration.css';
import './chat-core/styles/index.css';
import './styles/modern-sidebar.css';

interface ChatMessage {
  role: 'user' | 'assistant' | 'system';
  content: string;
  timestamp: string;
  thinking_content?: string;
  rag_sources?: Array<{
    document_id: string;
    file_name: string;
    chunk_text: string;
    score: number;
    chunk_index: number;
  }>;
}

interface AnalysisResponse {
  temp_id: string;
  initial_data: {
    ocr_text: string;
    tags: string[];
    mistake_type: string;
    first_answer: string;
  };
}

interface ContinueChatResponse {
  new_assistant_message: string;
}

type CurrentView = 'analysis' | 'library' | 'settings' | 'mistake-detail' | 'batch' | 'review' | 'dashboard' | 'data-management' | 'unified-review' | 'create-review' | 'review-session' | /* 'review-library' - DEPRECATED */ 'anki-generation' | 'knowledge-base' | 'rag-query' | 'image-occlusion' | 'template-management' | 'gemini-adapter-test' | 'cogni-graph';

// 真实API调用
const analyzeNewMistake = TauriAPI.analyzeNewMistake;

const continueChat = async (request: any): Promise<ContinueChatResponse> => {
  return TauriAPI.continueChat(request);
};

function App() {
  const [currentView, setCurrentView] = useState<CurrentView>('analysis');
  const [selectedMistake, setSelectedMistake] = useState<MistakeItem | null>(null);
  const [showDataManagement, setShowDataManagement] = useState(false);
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [isChatFullscreen, setIsChatFullscreen] = useState(false);
  const [currentReviewSessionId, setCurrentReviewSessionId] = useState<string | null>(null);

  // 🎯 修复：使用固定的分析会话ID，避免重新渲染时组件重新初始化
  const [analysisSessionId] = useState(() => `analysis_session_${Date.now()}`);

  // 🎯 修复：错题库刷新触发器，每次切换到错题库页面时递增
  const [libraryRefreshTrigger, setLibraryRefreshTrigger] = useState<number>(0);

  // 🎯 修复：处理页面切换，在切换到错题库时触发刷新
  const handleViewChange = (newView: CurrentView) => {
    console.log('🔄 页面切换:', currentView, '->', newView);

    // 如果切换到错题库页面，触发刷新
    if (newView === 'library' && currentView !== 'library') {
      console.log('🔄 切换到错题库，触发数据刷新');
      setLibraryRefreshTrigger(prev => prev + 1);
    }

    setCurrentView(newView);
  };

  // 开发者工具快捷键支持 (仅在生产模式下使用，开发模式依赖Tauri原生支持)
  useEffect(() => {
    // 检查是否为生产模式
    const isProduction = !window.location.hostname.includes('localhost') && 
                        !window.location.hostname.includes('127.0.0.1') &&
                        !window.location.hostname.includes('tauri.localhost');
    
    if (!isProduction) {
      // 开发模式：不拦截F12，让Tauri原生处理
      console.log('🔧 开发模式：使用Tauri原生F12支持');
      return;
    }
    
    const handleKeyDown = async (event: KeyboardEvent) => {
      // 支持多种快捷键组合 (仅生产模式)
      const isDevtoolsShortcut = 
        event.key === 'F12' || 
        (event.ctrlKey && event.shiftKey && event.key === 'I') ||
        (event.metaKey && event.altKey && event.key === 'I');
      
      if (isDevtoolsShortcut) {
        event.preventDefault();
        try {
          const webview = getCurrentWebviewWindow();
          // 使用Tauri 2.x的API
          if (await webview.isDevtoolsOpen()) {
            await webview.closeDevtools();
            console.log('🔧 开发者工具已关闭');
          } else {
            await webview.openDevtools();
            console.log('🔧 开发者工具已打开');
          }
        } catch (error) {
          console.error('❌ 切换开发者工具失败:', error);
          // 降级到基本切换方法
          try {
            const webview = getCurrentWebviewWindow();
            await webview.toggleDevtools();
            console.log('🔧 使用降级方法打开开发者工具');
          } catch (fallbackError) {
            console.error('❌ 降级方法也失败:', fallbackError);
          }
        }
      }
    };

    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, []);

  // 模板管理状态
  const [isSelectingTemplate, setIsSelectingTemplate] = useState(false);
  const [templateSelectionCallback, setTemplateSelectionCallback] = useState<((template: any) => void) | null>(null);
  const [previousView, setPreviousView] = useState<CurrentView>('anki-generation');

  // 开发功能设置状态
  const [geminiAdapterTestEnabled, setGeminiAdapterTestEnabled] = useState(false);

  // 调试App组件状态变化
  console.log('🔍 App组件渲染状态:', {
    currentView,
    currentReviewSessionId,
    timestamp: new Date().toISOString()
  });
  const { startDragging } = useWindowDrag();
  
  // 分析相关状态
  const [subject, setSubject] = useState('数学');
  const [availableSubjects, setAvailableSubjects] = useState<string[]>([]);
  const [isLoadingSubjects, setIsLoadingSubjects] = useState(true);
  const [subjectConfigs, setSubjectConfigs] = useState<any[]>([]);
  const [userQuestion, setUserQuestion] = useState('');
  const [questionImages, setQuestionImages] = useState<File[]>([]);
  const [questionImageUrls, setQuestionImageUrls] = useState<string[]>([]);
  const [imageViewerOpen, setImageViewerOpen] = useState(false);
  const [currentImageIndex, setCurrentImageIndex] = useState(0);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analysisResult, setAnalysisResult] = useState<AnalysisResponse | null>(null);
  const [chatHistory, setChatHistory] = useState<ChatMessage[]>([]);
  const [currentMessage, setCurrentMessage] = useState('');
  const [isChatting, setIsChatting] = useState(false);
  const [streamingMessageIndex, setStreamingMessageIndex] = useState<number | null>(null);
  const [isInputAllowed, setIsInputAllowed] = useState(false);
  const [useStreamMode] = useState(true); // 固定启用流式模式
  
  // 新增状态：用于立即显示OCR结果
  const [ocrResult, setOcrResult] = useState<{
    ocr_text: string;
    tags: string[];
    mistake_type: string;
  } | null>(null);
  const [isOcrComplete, setIsOcrComplete] = useState(false);
  const [enableChainOfThought] = useState(true); // 固定启用思维链
  const [thinkingContent, setThinkingContent] = useState<Map<number, string>>(new Map()); // 存储每条消息的思维链内容
  
  
  // RAG相关状态
  const [enableRag, setEnableRag] = useState(false);
  const [ragTopK, setRagTopK] = useState(5);
  const [selectedLibraries, setSelectedLibraries] = useState<string[]>([]);

  // 批量分析功能开关状态
  const [batchAnalysisEnabled, setBatchAnalysisEnabled] = useState(false);
  
  // 图片遮罩卡功能开关状态
  const [imageOcclusionEnabled, setImageOcclusionEnabled] = useState(false);

  // 加载支持的科目和科目配置
  useEffect(() => {
    const loadSubjects = async () => {
      try {
        const subjects = await TauriAPI.getSupportedSubjects();
        setAvailableSubjects(subjects);
        // 设置默认科目为第一个
        if (subjects.length > 0) {
          setSubject(subjects[0]);
        }
      } catch (error) {
        console.error('加载科目失败:', error);
        // 如果API失败，使用备用科目列表
        const fallbackSubjects = ['数学', '物理', '化学', '英语'];
        setAvailableSubjects(fallbackSubjects);
        setSubject(fallbackSubjects[0]);
      } finally {
        setIsLoadingSubjects(false);
      }
    };

    const loadSubjectConfigs = async () => {
      try {
        const configs = await TauriAPI.getAllSubjectConfigs(true); // 只获取启用的科目配置
        setSubjectConfigs(configs);
      } catch (error) {
        console.error('加载科目配置失败:', error);
        setSubjectConfigs([]);
      } finally {
        // Loading completed
      }
    };

    loadSubjects();
    loadSubjectConfigs();
  }, []);

  // 加载RAG设置、批量分析设置和开发功能设置
  const loadSettings = async () => {
    try {
      const [ragEnabled, ragTopKSetting, batchAnalysisEnabledSetting, geminiAdapterTestEnabledSetting, imageOcclusionEnabledSetting] = await Promise.all([
        TauriAPI.getSetting('rag_enabled').catch(() => 'false'),
        TauriAPI.getSetting('rag_top_k').catch(() => '5'),
        TauriAPI.getSetting('batch_analysis_enabled').catch(() => 'false'),
        TauriAPI.getSetting('gemini_adapter_test_enabled').catch(() => 'false'),
        TauriAPI.getSetting('image_occlusion_enabled').catch(() => 'false'),
      ]);
      console.log('🔄 加载系统设置:', { ragEnabled, ragTopKSetting, batchAnalysisEnabledSetting, geminiAdapterTestEnabledSetting, imageOcclusionEnabledSetting });
      setEnableRag(ragEnabled === 'true');
      setRagTopK(parseInt(ragTopKSetting || '5') || 5);
      setBatchAnalysisEnabled(batchAnalysisEnabledSetting === 'true');
      setGeminiAdapterTestEnabled(geminiAdapterTestEnabledSetting === 'true');
      setImageOcclusionEnabled(imageOcclusionEnabledSetting === 'true');
    } catch (error) {
      console.error('加载设置失败:', error);
    }
  };

  useEffect(() => {
    loadSettings();
  }, []);

  // 监听设置变化，如果禁用了功能且当前在对应页面，则跳转到分析页面
  useEffect(() => {
    if (!geminiAdapterTestEnabled && currentView === 'gemini-adapter-test') {
      console.log('🔄 Gemini适配器测试已禁用，跳转到分析页面');
      setCurrentView('analysis');
    }
    if (!imageOcclusionEnabled && currentView === 'image-occlusion') {
      console.log('🔄 图片遮罩卡已禁用，跳转到分析页面');
      setCurrentView('analysis');
    }
  }, [geminiAdapterTestEnabled, imageOcclusionEnabled, currentView]);

  // 监听窗口焦点，当用户切换回页面时重新加载设置
  useEffect(() => {
    const handleWindowFocus = () => {
      console.log('🔄 窗口获得焦点，重新加载系统设置');
      loadSettings();
    };

    window.addEventListener('focus', handleWindowFocus);
    
    return () => {
      window.removeEventListener('focus', handleWindowFocus);
    };
  }, []);

  // 处理聊天全屏切换 - 简化为直接状态切换
  const handleChatFullscreenToggle = () => {
    setIsChatFullscreen(!isChatFullscreen);
  };

  // 处理图片上传
  const handleImageUpload = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(e.target.files || []);
    const remainingSlots = 9 - questionImages.length;
    const filesToAdd = files.slice(0, remainingSlots);
    
    if (filesToAdd.length > 0) {
      setQuestionImages(prev => [...prev, ...filesToAdd]);
      // URL管理由useEffect自动处理，不需要在这里手动创建
    }
    
    // 清空input
    e.target.value = '';
  };

  // 删除图片
  const removeImage = (index: number) => {
    // 只需要更新questionImages状态，URL管理由useEffect自动处理
    setQuestionImages(prev => prev.filter((_, i) => i !== index));
  };

  // 打开图片查看器
  const openImageViewer = (index: number) => {
    setCurrentImageIndex(index);
    setImageViewerOpen(true);
  };

  // 优化的文件上传点击处理器
  const handleFileUploadClick = useCallback(() => {
    const fileInput = document.querySelector('.file-input') as HTMLInputElement;
    if (fileInput) {
      fileInput.click();
    }
  }, []);

  // 处理模板选择请求
  const handleTemplateSelectionRequest = useCallback((callback: (template: any) => void) => {
    setPreviousView(currentView);
    setTemplateSelectionCallback(() => callback);
    setIsSelectingTemplate(true);
    setCurrentView('template-management');
  }, [currentView]);

  // 处理模板选择完成
  const handleTemplateSelected = useCallback((template: any) => {
    if (templateSelectionCallback) {
      templateSelectionCallback(template);
    }
    setIsSelectingTemplate(false);
    setTemplateSelectionCallback(null);
    setCurrentView(previousView);
  }, [templateSelectionCallback, previousView]);

  // 取消模板选择
  const handleTemplateSelectionCancel = useCallback(() => {
    setIsSelectingTemplate(false);
    setTemplateSelectionCallback(null);
    setCurrentView(previousView);
  }, [previousView]);

  // 键盘快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // 只在没有输入框聚焦时处理快捷键
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      // Ctrl/Cmd + 数字键切换视图 (已移除开发工具相关快捷键)
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        switch (e.key) {
          case '1':
            e.preventDefault();
            handleViewChange('analysis');
            break;
          case '2':
            e.preventDefault();
            if (batchAnalysisEnabled) {
              handleViewChange('batch');
            }
            break;
          case '3':
            e.preventDefault();
            handleViewChange('unified-review');
            break;
          case '4':
            e.preventDefault();
            handleViewChange('library');
            break;
          case '5':
            e.preventDefault();
            handleViewChange('dashboard');
            break;
          case '6':
            e.preventDefault();
            setCurrentView('settings');
            break;
          case '7':
            e.preventDefault();
            setCurrentView('image-occlusion');
            break;
          case '8':
            e.preventDefault();
            if (geminiAdapterTestEnabled) {
              setCurrentView('gemini-adapter-test');
            }
            break;
          case 's':
            e.preventDefault();
            setCurrentView('settings');
            break;
          case 'e':
            e.preventDefault();
            setCurrentView('data-management');
            break;
          case 'r':
            e.preventDefault();
            console.log('🔄 手动刷新系统设置 (Ctrl+R)');
            loadSettings();
            // 同时刷新其他相关设置
            break;
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [currentView, showDataManagement, isChatFullscreen, batchAnalysisEnabled, geminiAdapterTestEnabled]);


  // 管理题目图片URL的生命周期
  useEffect(() => {
    // 清理旧的URLs（避免在第一次渲染时清理不存在的URLs）
    if (questionImageUrls.length > 0) {
      questionImageUrls.forEach(url => {
        try {
          URL.revokeObjectURL(url);
        } catch (error) {
          console.warn('清理URL时出错:', error);
        }
      });
    }
    
    // 创建新的URLs
    const newUrls = questionImages.map(file => {
      try {
        return URL.createObjectURL(file);
      } catch (error) {
        console.error('创建图片URL失败:', error);
        return '';
      }
    }).filter(url => url !== '');
    
    setQuestionImageUrls(newUrls);
    
    // 清理函数
    return () => {
      newUrls.forEach(url => {
        try {
          URL.revokeObjectURL(url);
        } catch (error) {
          console.warn('清理URL时出错:', error);
        }
      });
    };
  }, [questionImages]); // 仅依赖questionImages，避免questionImageUrls导致循环

  // 开始分析
  const handleAnalyze = async () => {
    if (!userQuestion.trim() || questionImages.length === 0) {
      alert('请输入问题并上传至少一张题目图片');
      return;
    }

    setIsAnalyzing(true);
    setStreamingMessageIndex(null);
    setOcrResult(null);
    setIsOcrComplete(false);
    setAnalysisResult(null);
    setChatHistory([]);
    
    try {
      const request = {
        subject,
        question_image_files: questionImages,
        analysis_image_files: [], // 不再使用解析图片
        user_question: userQuestion,
        enable_chain_of_thought: enableChainOfThought,
      };

      if (useStreamMode) {
        console.log('🚀 开始分步骤分析...');
        
        // 第一步：OCR分析，立即显示结果
        console.log('📝 第一步：OCR和分类分析...');
        const stepResult = await TauriAPI.analyzeStepByStep(request);
        
        // 立即显示OCR结果
        setOcrResult(stepResult.ocr_result);
        setIsOcrComplete(true);
        console.log('✅ OCR分析完成:', stepResult.ocr_result);
        
        // 创建临时的分析结果对象
        const tempAnalysisResult: AnalysisResponse = {
          temp_id: stepResult.temp_id,
          initial_data: {
            ocr_text: stepResult.ocr_result.ocr_text,
            tags: stepResult.ocr_result.tags,
            mistake_type: stepResult.ocr_result.mistake_type,
            first_answer: '', // 暂时为空，等待流式填充
          },
        };
        setAnalysisResult(tempAnalysisResult);
        
        // 第二步：开始流式AI解答
        console.log('🤖 第二步：开始流式AI解答...');
        
        // 创建初始的助手消息（空内容，等待流式填充）
        const initialMessage: ChatMessage = {
          role: 'assistant',
          content: '',
          timestamp: new Date().toISOString(),
        };
        setChatHistory([initialMessage]);
        setStreamingMessageIndex(0);
        
        // 改进的流式处理逻辑
        const streamEvent = `analysis_stream_${stepResult.temp_id}`;
        let fullContent = '';
        let fullThinkingContent = '';
        let contentListenerActive = true;
        let thinkingListenerActive = true;
        
        // 使用Tauri的listen API而不是streamHandler
        const { listen } = await import('@tauri-apps/api/event');
        
        
        
        // 统一检查并处理流完成状态的函数
        const checkAndFinalizeStreams = () => {
          console.log(`🔍 检查流完成状态: contentActive=${contentListenerActive}, thinkingActive=${thinkingListenerActive}`);
          
          // 核心改进：主内容流完成就是整个流程完成
          if (!contentListenerActive) {
            console.log('✅ 主内容流已完成，标记整个流程为完成状态');
            setStreamingMessageIndex(null); // 停止显示流式光标
            setIsInputAllowed(true); // 允许用户输入
            setIsAnalyzing(false); // 分析完成
          }
          
          // 当所有流都完成时，清理监听器
          if (!contentListenerActive && !thinkingListenerActive) {
            console.log('🎉 所有流式内容（分析或对话）均已完成，清理监听器');
          } else {
            console.log(`⏳ 流状态: 主内容=${contentListenerActive ? '进行中' : '已完成'}, 思维链=${thinkingListenerActive ? '进行中' : '已完成'}`);
          }
        };

        // 监听主内容流
        const unlistenContent = await listen(streamEvent, (event: any) => {
          if (!contentListenerActive) return;
          
          console.log(`💬 收到主内容流:`, event.payload);
          
          if (event.payload) {
            if (event.payload.is_complete) {
              if (event.payload.content && event.payload.content.length >= fullContent.length) {
                fullContent = event.payload.content;
              }
              console.log('🎉 主内容流完成，总长度:', fullContent.length);
              setChatHistory(prev => {
                const newHistory = [...prev];
                if (newHistory[0]) {
                  newHistory[0] = { ...newHistory[0], content: fullContent };
                }
                return newHistory;
              });
              contentListenerActive = false;
              
              // 立即更新分析结果中的first_answer
              if (analysisResult) {
                setAnalysisResult(prev => prev ? {
                  ...prev,
                  initial_data: {
                    ...prev.initial_data,
                    first_answer: fullContent
                  }
                } : null);
              }
              
              checkAndFinalizeStreams(); // 检查是否所有流都完成了
            } else if (event.payload.content) {
              fullContent += event.payload.content;
              console.log(`📝 累积内容，当前长度: ${fullContent.length}`);
              setChatHistory(prev => {
                const newHistory = [...prev];
                if (newHistory[0]) {
                  newHistory[0] = { ...newHistory[0], content: fullContent };
                }
                return newHistory;
              });
            }
          }
        });

        // 如果启用了思维链，监听思维链事件
        if (enableChainOfThought) {
          const reasoningEvent = `${streamEvent}_reasoning`;
          console.log(`🧠 监听思维链事件: ${reasoningEvent}`);
          
          const unlistenThinking = await listen(reasoningEvent, (event: any) => {
            if (!thinkingListenerActive) return;
            console.log(`🧠 思维链流内容:`, event.payload);

            if (event.payload) {
              if (event.payload.is_complete) {
                if (event.payload.content && event.payload.content.length >= fullThinkingContent.length) {
                  fullThinkingContent = event.payload.content;
                }
                console.log('🎉 思维链流完成，总长度:', fullThinkingContent.length);
                setThinkingContent(prev => new Map(prev).set(0, fullThinkingContent));
                thinkingListenerActive = false;
                checkAndFinalizeStreams(); // 检查是否所有流都完成了
              } else if (event.payload.content) {
                fullThinkingContent += event.payload.content;
                console.log(`🧠 累积思维链，当前长度: ${fullThinkingContent.length}`);
                setThinkingContent(prev => new Map(prev).set(0, fullThinkingContent));
              }
            }
          });
          
          // 添加超时机制：如果主内容完成后5秒思维链还没完成，自动标记为完成
          setTimeout(() => {
            if (!contentListenerActive && thinkingListenerActive) {
              console.warn('⚠️ 思维链流超时，自动标记为完成');
              thinkingListenerActive = false;
              checkAndFinalizeStreams();
            }
          }, 5000);
        } else {
          console.log('ℹ️ 未启用思维链，直接标记为完成');
          thinkingListenerActive = false; 
          checkAndFinalizeStreams(); // 如果没有思维链，立即检查一次，因为此时主内容可能已经完成
        }

        // 如果启用了RAG，监听RAG来源信息
        if (enableRag) {
          const ragSourcesEvent = `${streamEvent}_rag_sources`;
          console.log(`📚 监听RAG来源事件: ${ragSourcesEvent}`);
          
          const unlistenRagSources = await listen(ragSourcesEvent, (event: any) => {
            console.log(`📚 收到RAG来源信息:`, event.payload);
            
            if (event.payload && event.payload.sources) {
              // 更新聊天历史中的RAG来源信息
              setChatHistory(prev => {
                const newHistory = [...prev];
                if (newHistory[0] && newHistory[0].role === 'assistant') {
                  newHistory[0] = { 
                    ...newHistory[0], 
                    rag_sources: event.payload.sources 
                  };
                }
                return newHistory;
              });
              console.log('✅ RAG来源信息已更新');
            }
          });
        }
        
        // 启动流式解答
        console.log(`🚀 启动流式解答，temp_id: ${stepResult.temp_id}, enable_chain_of_thought: ${enableChainOfThought}, enable_rag: ${enableRag}`);
        
        if (enableRag) {
          // 使用RAG增强的流式解答
          await TauriAPI.startRagEnhancedStreamingAnswer({
            temp_id: stepResult.temp_id,
            enable_chain_of_thought: enableChainOfThought,
            enable_rag: true,
            rag_options: {
              top_k: ragTopK,
              enable_reranking: true // 如果配置了重排序模型会自动使用
            }
          });
        } else {
          // 使用普通的流式解答
          await TauriAPI.startStreamingAnswer(stepResult.temp_id, enableChainOfThought);
        }
        
      } else {
        // 使用传统非流式分析
        console.log('📊 使用传统分析模式...');
        const response = await analyzeNewMistake(request);
        setAnalysisResult(response);
        
        // 立即显示OCR结果
        setOcrResult({
          ocr_text: response.initial_data.ocr_text,
          tags: response.initial_data.tags,
          mistake_type: response.initial_data.mistake_type,
        });
        setIsOcrComplete(true);
        
        setChatHistory([{
          role: 'assistant',
          content: response.initial_data.first_answer,
          timestamp: new Date().toISOString(),
        }]);
      }
    } catch (error) {
      console.error('❌ 分析失败:', error);
      alert('分析失败: ' + error);
      setStreamingMessageIndex(null);
      setOcrResult(null);
      setIsOcrComplete(false);
    } finally {
      setIsAnalyzing(false);
    }
  };

  // 发送聊天消息 - 完全重写，修复所有流式问题
  const handleSendMessage = async () => {
    if (!currentMessage.trim() || !analysisResult) return;

    const userMessage: ChatMessage = {
      role: 'user',
      content: currentMessage,
      timestamp: new Date().toISOString(),
    };

    const newChatHistory = [...chatHistory, userMessage];
    setChatHistory(newChatHistory);
    setCurrentMessage('');
    setIsChatting(true);

    try {
      if (useStreamMode) {
        // 流式对话 - 全新改进版本
        console.log('💬 开始流式对话...');
        
        // 创建空的助手消息等待流式填充
        const assistantMessage: ChatMessage = {
          role: 'assistant',
          content: '',
          timestamp: new Date().toISOString(),
        };
        
        const streamingHistory = [...newChatHistory, assistantMessage];
        setChatHistory(streamingHistory);
        setStreamingMessageIndex(streamingHistory.length - 1);
        
        // 改进的流式处理逻辑
        const streamEvent = `continue_chat_stream_${analysisResult.temp_id}`;
        let fullContent = '';
        let fullThinkingContent = '';
        let contentListenerActive = true;
        let thinkingListenerActive = true;
        
        // 使用Tauri的listen API
        const { listen } = await import('@tauri-apps/api/event');
        
        // 统一检查并处理流完成状态的函数 (对话部分，可以考虑提取到外部)
        const checkAndFinalizeChatStreams = () => {
          console.log(`🔍 检查对话流完成状态: contentActive=${contentListenerActive}, thinkingActive=${thinkingListenerActive}`);
          
          // 核心改进：主内容流完成就是整个对话完成
          if (!contentListenerActive) {
            console.log('✅ 对话主内容流已完成，标记整个对话为完成状态');
            setStreamingMessageIndex(null); // 停止显示流式光标
            setIsInputAllowed(true); // 允许用户继续输入
            setIsChatting(false); // 对话完成
          }
          
          // 当所有流都完成时，清理监听器
          if (!contentListenerActive && !thinkingListenerActive) {
            console.log('🎉 所有对话流式内容均已完成，清理监听器');
          } else {
            console.log(`⏳ 对话流状态: 主内容=${contentListenerActive ? '进行中' : '已完成'}, 思维链=${thinkingListenerActive ? '进行中' : '已完成'}`);
          }
        };

        // 监听主内容流
        const unlistenContent = await listen(streamEvent, (event: any) => {
          if (!contentListenerActive) return;
          console.log(`💬 收到对话主内容流:`, event.payload);

          if (event.payload) {
            if (event.payload.is_complete) {
              if (event.payload.content && event.payload.content.length >= fullContent.length) {
                fullContent = event.payload.content;
              }
              console.log('🎉 对话主内容流完成，总长度:', fullContent.length);
              setChatHistory(prev => {
                const newHistory = [...prev];
                const lastIdx = newHistory.length - 1;
                if (newHistory[lastIdx] && newHistory[lastIdx].role === 'assistant') {
                  newHistory[lastIdx] = { ...newHistory[lastIdx], content: fullContent };
                }
                return newHistory;
              });
              contentListenerActive = false;
              
              // 立即更新分析结果中的first_answer
              if (analysisResult) {
                setAnalysisResult(prev => prev ? {
                  ...prev,
                  initial_data: {
                    ...prev.initial_data,
                    first_answer: fullContent
                  }
                } : null);
              }
              
              checkAndFinalizeChatStreams(); // 检查是否所有流都完成了
            } else if (event.payload.content) {
              fullContent += event.payload.content;
              setChatHistory(prev => {
                const newHistory = [...prev];
                const lastIdx = newHistory.length - 1;
                if (newHistory[lastIdx] && newHistory[lastIdx].role === 'assistant') {
                  newHistory[lastIdx] = { ...newHistory[lastIdx], content: fullContent };
                }
                return newHistory;
              });
            }
          }
        });

        // 如果启用了思维链，监听思维链事件
        if (enableChainOfThought) {
          const reasoningEvent = `${streamEvent}_reasoning`;
          console.log(`🧠 监听对话思维链事件: ${reasoningEvent}`);
          const lastMessageIndex = streamingHistory.length - 1;

          const unlistenThinking = await listen(reasoningEvent, (event: any) => {
            if (!thinkingListenerActive) return;
            console.log(`🧠 对话思维链流内容:`, event.payload);

            if (event.payload) {
              if (event.payload.is_complete) {
                if (event.payload.content && event.payload.content.length >= fullThinkingContent.length) {
                  fullThinkingContent = event.payload.content;
                }
                console.log('🎉 对话思维链流完成，总长度:', fullThinkingContent.length);
                setThinkingContent(prev => new Map(prev).set(lastMessageIndex, fullThinkingContent));
                thinkingListenerActive = false;
                checkAndFinalizeChatStreams(); // 检查是否所有流都完成了
              } else if (event.payload.content) {
                fullThinkingContent += event.payload.content;
                setThinkingContent(prev => new Map(prev).set(lastMessageIndex, fullThinkingContent));
              }
            }
          });
        } else {
          thinkingListenerActive = false;
          checkAndFinalizeChatStreams(); // 如果没有思维链，立即检查
        }

        // 如果启用了RAG，监听对话的RAG来源信息
        if (enableRag) {
          const ragSourcesEvent = `${streamEvent}_rag_sources`;
          console.log(`📚 监听对话RAG来源事件: ${ragSourcesEvent}`);
          
          const unlistenRagSources = await listen(ragSourcesEvent, (event: any) => {
            console.log(`📚 收到对话RAG来源信息:`, event.payload);
            
            if (event.payload && event.payload.sources) {
              // 更新聊天历史中最后一条助手消息的RAG来源信息
              setChatHistory(prev => {
                const newHistory = [...prev];
                const lastIdx = newHistory.length - 1;
                if (newHistory[lastIdx] && newHistory[lastIdx].role === 'assistant') {
                  newHistory[lastIdx] = { 
                    ...newHistory[lastIdx], 
                    rag_sources: event.payload.sources 
                  };
                }
                return newHistory;
              });
              console.log('✅ 对话RAG来源信息已更新');
            }
          });
        }
        
        // 调用后端API
        if (enableRag) {
          // 使用RAG增强的对话
          const ragRequest = {
            temp_id: analysisResult.temp_id,
            chat_history: newChatHistory,
            enable_chain_of_thought: enableChainOfThought,
            enable_rag: true,
            rag_options: {
              top_k: ragTopK,
              enable_reranking: true
            }
          };
          
          await TauriAPI.continueRagEnhancedChatStream(ragRequest);
        } else {
          // 使用普通对话
          const request = {
            temp_id: analysisResult.temp_id,
            chat_history: newChatHistory,
            enable_chain_of_thought: enableChainOfThought,
          };
          
          await TauriAPI.continueChatStream(request);
        }
        
      } else {
        // 传统非流式对话
        const request = {
          temp_id: analysisResult.temp_id,
          chat_history: newChatHistory,
          enable_chain_of_thought: enableChainOfThought,
        };

        const response = await continueChat(request);
        
        const assistantMessage: ChatMessage = {
          role: 'assistant',
          content: response.new_assistant_message,
          timestamp: new Date().toISOString(),
        };

        setChatHistory([...newChatHistory, assistantMessage]);
      }
    } catch (error) {
      console.error('❌ 对话失败:', error);
      alert('对话失败: ' + error);
      setStreamingMessageIndex(null);
    } finally {
      setIsChatting(false);
    }
  };

  // 保存到错题库
  const handleSaveToLibrary = async () => {
    if (!analysisResult) return;
    
    try {
      // 复制聊天历史，并将思维链内容保存到message中
      const chatHistoryWithThinking = chatHistory.map((message, index) => {
        // 如果是assistant消息且有思维链，则添加thinking_content字段
        if (message.role === 'assistant' && thinkingContent.has(index)) {
          return {
            ...message,
            thinking_content: thinkingContent.get(index)
          };
        }
        return message;
      });
      
      const result = await TauriAPI.saveMistakeFromAnalysis({
        temp_id: analysisResult.temp_id,
        final_chat_history: chatHistoryWithThinking,
      });
      if (result.success) {
        alert('题目已保存到错题库！');
        // 重置分析状态
        handleReset();
      } else {
        alert('保存失败，请重试');
      }
    } catch (error) {
      console.error('保存失败:', error);
      alert('保存失败: ' + error);
    }
  };

  // 重置分析
  const handleReset = () => {
    setAnalysisResult(null);
    setChatHistory([]);
    setCurrentMessage('');
    setStreamingMessageIndex(null);
    setOcrResult(null);
    setIsOcrComplete(false);
    setUserQuestion('');
    setQuestionImages([]);
    setThinkingContent(new Map());
    setIsInputAllowed(false);
  };

  // 选择错题
  const handleSelectMistake = async (mistake: MistakeItem) => {
    try {
      // 🎯 修复：保留MistakeLibrary中转换的图片URLs，但补充聊天记录
      console.log('🔍 正在获取错题完整数据:', mistake.id);
      console.log('🔍 MistakeLibrary传入的数据:', {
        id: mistake.id,
        hasQuestionImageUrls: !!mistake.question_image_urls,
        questionImageUrlsLength: mistake.question_image_urls?.length || 0,
        hasQuestionImages: mistake.question_images?.length || 0,
        chatHistoryLength: mistake.chat_history?.length || 0
      });
      
      const fullMistakeData = await TauriAPI.getMistakeDetails(mistake.id);
      
      if (fullMistakeData) {
        // 🎯 关键修复：合并数据，保留转换后的图片URLs，并正确处理思维链
        const mergedMistake = {
          ...fullMistakeData,
          // 如果MistakeLibrary提供了转换后的图片URLs，使用它们
          question_image_urls: mistake.question_image_urls || fullMistakeData.question_image_urls || []
        };
        
        // 🎯 修复：从聊天历史中恢复思维链数据
        const recoveredThinkingContent = new Map<number, string>();
        if (mergedMistake.chat_history) {
          mergedMistake.chat_history.forEach((message, index) => {
            if (message.role === 'assistant' && message.thinking_content) {
              console.log(`🧠 [错题加载] 恢复思维链，索引${index}:`, message.thinking_content.substring(0, 50) + '...');
              recoveredThinkingContent.set(index, message.thinking_content);
            }
          });
        }
        
        console.log('✅ 获取到完整错题数据并保留图片URLs:', {
          id: mergedMistake.id,
          chatHistoryLength: mergedMistake.chat_history?.length || 0,
          chatHistoryData: mergedMistake.chat_history,
          hasQuestionImages: mergedMistake.question_images?.length || 0,
          hasQuestionImageUrls: !!mergedMistake.question_image_urls,
          questionImageUrlsLength: mergedMistake.question_image_urls?.length || 0,
          thinkingContentSize: recoveredThinkingContent.size
        });
        
        // 🎯 修复：将恢复的思维链数据添加到错题对象中
        const finalMistake = {
          ...mergedMistake,
          thinkingContent: recoveredThinkingContent
        };
        
        setSelectedMistake(finalMistake);
      } else {
        console.warn('⚠️ 未获取到完整数据，使用原始数据');
        setSelectedMistake(mistake);
      }
      
      handleViewChange('mistake-detail');
    } catch (error) {
      console.error('❌ 获取错题详情失败:', error);
      // 如果获取失败，使用原始数据作为fallback
      setSelectedMistake(mistake);
      handleViewChange('mistake-detail');
    }
  };

  // 更新错题
  const handleUpdateMistake = (updatedMistake: MistakeItem) => {
    setSelectedMistake(updatedMistake);
  };

  // 删除错题
  const handleDeleteMistake = (mistakeId: string) => {
    console.log('删除错题:', mistakeId);
    setSelectedMistake(null);
    handleViewChange('library');
  };

  // 为NEW_MISTAKE_ANALYSIS模式创建API Provider - 使用useCallback缓存
  const createAnalysisApiProvider = useCallback((): HostedChatApiProvider => ({
    initiateAndGetStreamId: async (params) => {
      const request = {
        subject: params.subject!,
        question_image_files: params.questionImages!,
        analysis_image_files: [], // 不再使用解析图片
        user_question: params.userQuestion!,
        enable_chain_of_thought: params.enableChainOfThought,
      };
      
      const stepResult = await TauriAPI.analyzeStepByStep(request);
      
      return {
        streamIdForEvents: stepResult.temp_id,
        ocrResultData: stepResult.ocr_result,
        initialMessages: []
      };
    },

    startMainStreaming: async (params) => {
      if (params.enableRag) {
        await TauriAPI.startRagEnhancedStreamingAnswer({
          temp_id: params.streamIdForEvents,
          enable_chain_of_thought: params.enableChainOfThought,
          enable_rag: true,
          rag_options: {
            top_k: params.ragTopK,
            enable_reranking: true
          }
        });
      } else {
        await TauriAPI.startStreamingAnswer(params.streamIdForEvents, params.enableChainOfThought);
      }
    },

    continueUserChat: async (params) => {
      if (params.enableRag) {
        const ragRequest = {
          temp_id: params.streamIdForEvents,
          chat_history: params.fullChatHistory,
          enable_chain_of_thought: params.enableChainOfThought,
          enable_rag: true,
          rag_options: {
            top_k: params.ragTopK,
            enable_reranking: true
          }
        };
        await TauriAPI.continueRagEnhancedChatStream(ragRequest);
      } else {
        const request = {
          temp_id: params.streamIdForEvents,
          chat_history: params.fullChatHistory,
          enable_chain_of_thought: params.enableChainOfThought,
        };
        await TauriAPI.continueChatStream(request);
      }
    }
  }), []); // 空依赖数组，因为这个provider不依赖任何状态

  // 渲染侧边栏导航 - 现代化风格
  const renderSidebar = () => (
    <ModernSidebar
      currentView={currentView}
      onViewChange={handleViewChange}
      sidebarCollapsed={sidebarCollapsed}
      onToggleSidebar={() => setSidebarCollapsed(!sidebarCollapsed)}
      batchAnalysisEnabled={batchAnalysisEnabled}
      geminiAdapterTestEnabled={geminiAdapterTestEnabled}
      imageOcclusionEnabled={imageOcclusionEnabled}
      startDragging={startDragging}
    />
  );

  // 使用useMemo缓存hostProps，避免每次渲染都创建新对象
  const analysisHostProps = useMemo((): UniversalAppChatHostProps => ({
      mode: 'NEW_MISTAKE_ANALYSIS',
      businessSessionId: analysisSessionId,
      preloadedData: {
        subject,
        userQuestion,
        questionImages,
        // 不需要预加载其他数据，因为是新分析
      },
      serviceConfig: {
        apiProvider: createAnalysisApiProvider(),
        streamEventNames: {
          initialStream: (id) => ({ 
            data: `analysis_stream_${id}`, 
            reasoning: `analysis_stream_${id}_reasoning`, 
            ragSources: `analysis_stream_${id}_rag_sources` 
          }),
          continuationStream: (id) => ({ 
            data: `continue_chat_stream_${id}`, 
            reasoning: `continue_chat_stream_${id}_reasoning`, 
            ragSources: `continue_chat_stream_${id}_rag_sources` 
          }),
        },
        defaultEnableChainOfThought: true,
        defaultEnableRag: enableRag,
        defaultRagTopK: ragTopK,
        defaultSelectedLibraries: selectedLibraries,
      },
      onSaveRequest: async (data) => {
        // 实现原App.tsx中的保存逻辑
        try {
          console.log('🔍 App.tsx保存请求:', data);
          console.log('📊 analysisResult:', analysisResult);
          
          // 优先使用直接传递的temp_id
          let temp_id = null;
          if (data.temp_id) {
            temp_id = data.temp_id;
            console.log('✅ 从data.temp_id获取temp_id:', temp_id);
          } else if (analysisResult?.temp_id) {
            temp_id = analysisResult.temp_id;
            console.log('✅ 从analysisResult获取temp_id:', temp_id);
          } else if (data.ocrResult?.temp_id) {
            temp_id = data.ocrResult.temp_id;
            console.log('✅ 从data.ocrResult获取temp_id:', temp_id);
          } else {
            // 如果都没有，使用businessSessionId作为fallback
            temp_id = data.businessSessionId;
            console.log('⚠️ 使用businessSessionId作为fallback temp_id:', temp_id);
          }
          
          // 验证数据有效性
          if (!temp_id) {
            throw new Error('无法获取有效的temp_id，无法保存');
          }
          
          if (!data.chatHistory || data.chatHistory.length === 0) {
            throw new Error('聊天历史为空，无法保存');
          }
          
          console.log('🆔 最终使用的temp_id:', temp_id);
          console.log('📜 保存的聊天历史数量:', data.chatHistory.length);
          console.log('📝 聊天历史详情:', data.chatHistory);
          
          const result = await TauriAPI.saveMistakeFromAnalysis({
            temp_id: temp_id,
            final_chat_history: data.chatHistory,
          });
          
          if (result.success) {
            alert('题目已保存到错题库！');
            
            // 🎯 优先处理前端传递的总结内容
            if (data.summaryContent && result.final_mistake_item) {
              try {
                console.log('📝 保存前端生成的总结内容到数据库...');
                
                // 🎯 修复：改进解析逻辑，保持原始格式
                const parseSummaryContent = (content: string) => {
                  console.log('📄 [总结解析] 原始内容长度:', content.length);
                  console.log('📄 [总结解析] 原始内容预览:', content.substring(0, 200) + '...');
                  
                  // 🎯 策略1：如果内容较短或者没有明显的分段标识，保存到第一个字段
                  const lines = content.split('\n');
                  const hasNumberedSections = lines.some(line => /^\s*\d+\.\s*(核心知识点|错误分析|学习建议)/.test(line));
                  const hasMarkdownSections = lines.some(line => /^#+\s*(核心知识点|错误分析|学习建议)/.test(line));
                  
                  if (!hasNumberedSections && !hasMarkdownSections) {
                    console.log('📄 [总结解析] 无明确分段，保存到mistake_summary');
                    return {
                      mistakeSummary: content.trim(),
                      userErrorAnalysis: null,
                    };
                  }
                  
                  // 🎯 策略2：尝试分段，但保持更完整的内容
                  let mistakeSummary = '';
                  let userErrorAnalysis = '';
                  let currentSection = '';
                  let includeCurrentLine = false;

                  for (const line of lines) {
                    const trimmedLine = line.trim();
                    
                    // 检测章节标题
                    if (/^\s*\d+\.\s*核心知识点|^#+\s*核心知识点|题目解析|正确解法/.test(trimmedLine)) {
                      currentSection = 'mistake_summary';
                      includeCurrentLine = true;
                    } else if (/^\s*\d+\.\s*错误分析|^#+\s*错误分析|^\s*\d+\.\s*学习建议|^#+\s*学习建议|薄弱环节/.test(trimmedLine)) {
                      currentSection = 'user_error_analysis';
                      includeCurrentLine = true;
                    } else {
                      includeCurrentLine = true;
                    }
                    
                    if (includeCurrentLine) {
                      if (currentSection === 'mistake_summary') {
                        mistakeSummary += line + '\n';
                      } else if (currentSection === 'user_error_analysis') {
                        userErrorAnalysis += line + '\n';
                      } else if (!currentSection) {
                        // 如果还没有检测到分段，先放到第一个字段
                        mistakeSummary += line + '\n';
                      }
                    }
                  }

                  // 🎯 策略3：如果分段后某个字段为空，将所有内容保存到第一个字段
                  if (!mistakeSummary.trim() && !userErrorAnalysis.trim()) {
                    console.log('📄 [总结解析] 分段失败，保存完整内容到mistake_summary');
                    return {
                      mistakeSummary: content.trim(),
                      userErrorAnalysis: null,
                    };
                  }
                  
                  console.log('📄 [总结解析] 分段结果:', {
                    mistakeSummaryLength: mistakeSummary.trim().length,
                    userErrorAnalysisLength: userErrorAnalysis.trim().length
                  });

                  return {
                    mistakeSummary: mistakeSummary.trim() || null,
                    userErrorAnalysis: userErrorAnalysis.trim() || null,
                  };
                };
                
                const { mistakeSummary, userErrorAnalysis } = parseSummaryContent(data.summaryContent);
                
                // 更新错题记录，添加总结字段
                const updatedMistake = {
                  ...result.final_mistake_item,
                  mistake_summary: mistakeSummary,
                  user_error_analysis: userErrorAnalysis,
                  status: "completed", // 🎯 修复：设置状态为已完成
                  updated_at: new Date().toISOString(),
                };
                
                await TauriAPI.updateMistake(updatedMistake);
                console.log('✅ 前端总结内容已成功保存到数据库');
                
              } catch (error) {
                console.error('保存前端总结内容失败:', error);
                alert(`⚠️ 总结保存失败：${error}\n错题已保存，可稍后手动生成总结。`);
              }
            }
          } else {
            alert('保存失败，请重试');
          }
        } catch (error) {
          console.error('保存失败:', error);
          alert('保存失败: ' + error);
        }
      },
  }), [analysisSessionId, subject, userQuestion, questionImages, enableRag, ragTopK, selectedLibraries, analysisResult]);

  // 缓存回调函数
  const handleCoreStateUpdate = useCallback((data: any) => {
    // 仅在开发环境打印状态更新信息，避免生产环境噪音
    if (import.meta.env.DEV) {
      console.log('🔄 UniversalAppChatHost state update:', {
        sessionId: analysisSessionId,
        chatHistoryLength: data.chatHistory.length,
        thinkingContentSize: data.thinkingContent.size,
        hasOcrResult: !!data.ocrResult,
        isAnalyzing: data.isAnalyzing,
        isChatting: data.isChatting
      });
    }
  }, [analysisSessionId]);

  const handleSaveRequest = useCallback(async (data: any) => {
    // 实现原App.tsx中的保存逻辑
    try {
      console.log('🔍 App.tsx保存请求:', data);
      console.log('📊 analysisResult:', analysisResult);
      
      // 优先使用直接传递的temp_id
      let temp_id = null;
      if (data.temp_id) {
        temp_id = data.temp_id;
        console.log('✅ 从data.temp_id获取temp_id:', temp_id);
      } else if (analysisResult?.temp_id) {
        temp_id = analysisResult.temp_id;
        console.log('✅ 从analysisResult获取temp_id:', temp_id);
      } else if (data.ocrResult?.temp_id) {
        temp_id = data.ocrResult.temp_id;
        console.log('✅ 从data.ocrResult获取temp_id:', temp_id);
      } else {
        // 如果都没有，使用businessSessionId作为fallback
        temp_id = data.businessSessionId;
        console.log('⚠️ 使用businessSessionId作为fallback temp_id:', temp_id);
      }
      
      // 验证数据有效性
      if (!temp_id) {
        throw new Error('无法获取有效的temp_id，无法保存');
      }
      
      if (!data.chatHistory || data.chatHistory.length === 0) {
        throw new Error('聊天历史为空，无法保存');
      }
      
      console.log('🆔 最终使用的temp_id:', temp_id);
      console.log('📜 保存的聊天历史数量:', data.chatHistory.length);
      console.log('📝 聊天历史详情:', data.chatHistory);
      
      const result = await TauriAPI.saveMistakeFromAnalysis({
        temp_id: temp_id,
        final_chat_history: data.chatHistory,
      });
      
      if (result.success) {
        alert('题目已保存到错题库！');
        
        // 🎯 优先处理前端传递的总结内容
        if (data.summaryContent && result.final_mistake_item) {
          try {
            console.log('📝 保存前端生成的总结内容到数据库...');
            
            // 🎯 修复：改进解析逻辑，保持原始格式
            const parseSummaryContent = (content: string) => {
              console.log('📄 [总结解析] 原始内容长度:', content.length);
              console.log('📄 [总结解析] 原始内容预览:', content.substring(0, 200) + '...');
              
              // 🎯 策略1：如果内容较短或者没有明显的分段标识，保存到第一个字段
              const lines = content.split('\n');
              const hasNumberedSections = lines.some(line => /^\s*\d+\.\s*(核心知识点|错误分析|学习建议)/.test(line));
              const hasMarkdownSections = lines.some(line => /^#+\s*(核心知识点|错误分析|学习建议)/.test(line));
              
              if (!hasNumberedSections && !hasMarkdownSections) {
                console.log('📄 [总结解析] 无明确分段，保存到mistake_summary');
                return {
                  mistakeSummary: content.trim(),
                  userErrorAnalysis: null,
                };
              }
              
              // 🎯 策略2：尝试分段，但保持更完整的内容
              let mistakeSummary = '';
              let userErrorAnalysis = '';
              let currentSection = '';
              let includeCurrentLine = false;

              for (const line of lines) {
                const trimmedLine = line.trim();
                
                // 检测章节标题
                if (/^\s*\d+\.\s*核心知识点|^#+\s*核心知识点|题目解析|正确解法/.test(trimmedLine)) {
                  currentSection = 'mistake_summary';
                  includeCurrentLine = true;
                } else if (/^\s*\d+\.\s*错误分析|^#+\s*错误分析|^\s*\d+\.\s*学习建议|^#+\s*学习建议|薄弱环节/.test(trimmedLine)) {
                  currentSection = 'user_error_analysis';
                  includeCurrentLine = true;
                } else {
                  includeCurrentLine = true;
                }
                
                if (includeCurrentLine) {
                  if (currentSection === 'mistake_summary') {
                    mistakeSummary += line + '\n';
                  } else if (currentSection === 'user_error_analysis') {
                    userErrorAnalysis += line + '\n';
                  } else if (!currentSection) {
                    // 如果还没有检测到分段，先放到第一个字段
                    mistakeSummary += line + '\n';
                  }
                }
              }

              // 🎯 策略3：如果分段后某个字段为空，将所有内容保存到第一个字段
              if (!mistakeSummary.trim() && !userErrorAnalysis.trim()) {
                console.log('📄 [总结解析] 分段失败，保存完整内容到mistake_summary');
                return {
                  mistakeSummary: content.trim(),
                  userErrorAnalysis: null,
                };
              }
              
              console.log('📄 [总结解析] 分段结果:', {
                mistakeSummaryLength: mistakeSummary.trim().length,
                userErrorAnalysisLength: userErrorAnalysis.trim().length
              });

              return {
                mistakeSummary: mistakeSummary.trim() || null,
                userErrorAnalysis: userErrorAnalysis.trim() || null,
              };
            };
            
            const { mistakeSummary, userErrorAnalysis } = parseSummaryContent(data.summaryContent);
            
            // 更新错题记录，添加总结字段
            const updatedMistake = {
              ...result.final_mistake_item,
              mistake_summary: mistakeSummary,
              user_error_analysis: userErrorAnalysis,
              status: "completed", // 🎯 修复：设置状态为已完成
              updated_at: new Date().toISOString(),
            };
            
            await TauriAPI.updateMistake(updatedMistake);
            console.log('✅ 前端总结内容已成功保存到数据库');
            
          } catch (error) {
            console.error('保存前端总结内容失败:', error);
            alert(`⚠️ 总结保存失败：${error}\n错题已保存，可稍后手动生成总结。`);
          }
        }
      } else {
        alert('保存失败，请重试');
      }
    } catch (error) {
      console.error('保存失败:', error);
      alert('保存失败: ' + error);
    }
  }, [analysisResult]);

  // 渲染分析界面 - 左右分栏布局
  const renderAnalysisView = () => {
    return <UniversalAppChatHost 
      key="analysis-host" 
      {...analysisHostProps}
      onCoreStateUpdate={handleCoreStateUpdate}
      onSaveRequest={handleSaveRequest}
    />;
  };

  return (
    <SubjectProvider>
      <div 
        className="app"
        style={{
          '--sidebar-width': sidebarCollapsed ? '60px' : '240px'
        } as React.CSSProperties}
      >
      <WindowControls />
      <div className="app-body">
        {renderSidebar()}
        <main className="app-content">
        <div className="content-header" onMouseDown={startDragging}>
          <div className="content-header-left">
            <button 
              className="sidebar-toggle"
              onClick={() => setSidebarCollapsed(!sidebarCollapsed)}
              onMouseDown={(e) => e.stopPropagation()}
              title={sidebarCollapsed ? '展开侧边栏' : '收起侧边栏'}
            >
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                {sidebarCollapsed ? (
                  // 展开图标 (chevron-right)
                  <polyline points="9,18 15,12 9,6"></polyline>
                ) : (
                  // 收起图标 (chevron-left)
                  <polyline points="15,18 9,12 15,6"></polyline>
                )}
              </svg>
            </button>
            {/* 全局科目选择器 */}
            <UnifiedSubjectSelector 
              mode="enabled"
              className="header-subject-selector compact"
            />
          </div>
        </div>
        <div className="content-body">
          {/* 分析页面组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'analysis' ? 'block' : 'none' }}>
            {(() => {
              console.log('🔄 [保活检查] 分析页面容器渲染，当前视图:', currentView, '显示状态:', currentView === 'analysis' ? 'block' : 'none');
              return renderAnalysisView();
            })()}
          </div>
          {/* 批量分析组件始终挂载，只是控制显示/隐藏 */}
          <div className="page-container" style={{ display: currentView === 'batch' ? 'block' : 'none' }}>
            <BatchAnalysis onBack={() => setCurrentView('analysis')} />
          </div>
          {/* 🎯 修复：错题库组件每次切换时重新加载数据，不再保活 */}
          <div className="page-container" style={{ display: currentView === 'library' ? 'block' : 'none' }}>
            <MistakeLibrary
              onSelectMistake={handleSelectMistake}
              onBack={() => handleViewChange('analysis')}
              refreshTrigger={libraryRefreshTrigger}
            />
          </div>
          {/* 
            DEPRECATED: 单次回顾视图已废弃 - 2024年6月5日
            该功能已被统一回顾模块替代
          */}
          {/* 
          {currentView === 'review' && (
            <ReviewAnalysis onBack={() => setCurrentView('analysis')} />
          )}
          */}
          {/* 统一回顾分析组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'unified-review' ? 'block' : 'none' }}>
            <ReviewAnalysisDashboard 
              onCreateNew={() => setCurrentView('create-review')}
              onViewSession={(sessionId: string) => {
                setCurrentReviewSessionId(sessionId);
                setCurrentView('review-session');
              }}
            />
          </div>
          {/* 创建回顾分析组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'create-review' ? 'block' : 'none' }}>
            <CreateReviewAnalysisView
              onCancel={() => setCurrentView('unified-review')}
              onCreateSuccess={(sessionId: string) => {
                console.log('📍 App.tsx收到创建成功回调, sessionId:', sessionId);
                console.log('📍 App.tsx当前状态: currentView=', currentView, ', currentReviewSessionId=', currentReviewSessionId);
                setCurrentReviewSessionId(sessionId);
                setCurrentView('review-session');
                console.log('📍 App.tsx状态更新后: currentView=review-session, currentReviewSessionId=', sessionId);
                // 确保状态更新后的下一个渲染周期能看到正确的组件
                setTimeout(() => {
                  console.log('📍 验证状态更新: currentView=', currentView, ', currentReviewSessionId=', currentReviewSessionId);
                }, 100);
              }}
            />
          </div>
          {/* 回顾分析会话组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'review-session' ? 'block' : 'none' }}>
            {currentReviewSessionId && (() => {
              console.log('📍 正在渲染ReviewAnalysisSessionView, sessionId:', currentReviewSessionId);
              return (
                <ReviewAnalysisSessionView 
                  sessionId={currentReviewSessionId}
                  onBack={() => {
                  console.log('📍 回到统一回顾页面');
                  setCurrentView('unified-review');
                }}
              />
              );
            })()}
          </div>
          {/* DEPRECATED: 分析库组件已废弃 - 2024年6月8日，功能被统一回顾分析替代 */}
          {/* {currentView === 'review-library' && (
            <ReviewAnalysisLibrary 
              onSelectAnalysis={(analysis) => {
                setCurrentReviewSessionId(analysis.id);
                setCurrentView('review-session');
              }}
              onBack={() => setCurrentView('unified-review')}
            />
          )} */}
          {/* 数据统计组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'dashboard' ? 'block' : 'none' }}>
            <Dashboard onBack={() => setCurrentView('analysis')} />
          </div>
          {/* 设置组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'settings' ? 'block' : 'none' }}>
            <Settings onBack={() => setCurrentView('analysis')} />
          </div>
          {/* 错题详情组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'mistake-detail' ? 'block' : 'none' }}>
            {selectedMistake && (() => {
            // 调试日志：检查传递给UniversalAppChatHost的数据
            console.log('🔍 App.tsx 传递给 UniversalAppChatHost 的错题数据:', {
              mistakeId: selectedMistake.id,
              chatHistoryLength: selectedMistake.chat_history?.length || 0,
              chatHistoryExists: !!selectedMistake.chat_history,
              chatHistoryData: selectedMistake.chat_history,
              questionImageUrls: selectedMistake.question_image_urls || [],
              questionImageUrlsExists: !!selectedMistake.question_image_urls,
              questionImageUrlsLength: selectedMistake.question_image_urls?.length || 0,
              questionImagesOriginal: selectedMistake.question_images || [],
              questionImageUrlsPreview: selectedMistake.question_image_urls?.map((url, i) => `${i+1}: ${url.substring(0, 50)}...`) || [],
              ocrText: selectedMistake.ocr_text?.substring(0, 100) + '...',
            });
            
            console.log('🔍 App.tsx selectedMistake 完整对象检查:', {
              hasQuestionImageUrls: 'question_image_urls' in selectedMistake,
              questionImageUrlsValue: selectedMistake.question_image_urls,
              questionImageUrlsType: typeof selectedMistake.question_image_urls,
              allKeys: Object.keys(selectedMistake),
              mistakeId: selectedMistake.id
            });
            
            return (
              <UniversalAppChatHost 
                mode="EXISTING_MISTAKE_DETAIL"
                businessSessionId={selectedMistake.id}
                preloadedData={{
                  subject: selectedMistake.subject,
                  userQuestion: selectedMistake.user_question,
                  questionImageUrls: selectedMistake.question_image_urls || [],
                  ocrText: selectedMistake.ocr_text,
                  tags: selectedMistake.tags || [],
                  chatHistory: selectedMistake.chat_history || [],
                  thinkingContent: (selectedMistake as any).thinkingContent || new Map(), // 🎯 修复：传递恢复的思维链数据
                  mistake_summary: selectedMistake.mistake_summary, // 错题总结
                  user_error_analysis: selectedMistake.user_error_analysis, // 用户错误分析
                  originalMistake: selectedMistake // 完整的原始错题对象
                }}
              
              serviceConfig={{
                apiProvider: {
                  initiateAndGetStreamId: async (params) => ({
                    streamIdForEvents: params.businessId!,
                    ocrResultData: {
                      ocr_text: selectedMistake.ocr_text,
                      tags: selectedMistake.tags || [],
                      mistake_type: selectedMistake.mistake_type || '错题分析'
                    },
                    initialMessages: selectedMistake.chat_history || []
                  }),
                  startMainStreaming: async () => {}, // 错题详情不需要启动新的主流
                  continueUserChat: async (params) => {
                    await TauriAPI.continueMistakeChatStream({
                      mistakeId: params.businessId,
                      chatHistory: params.fullChatHistory,
                      enableChainOfThought: params.enableChainOfThought
                    });
                  }
                },
                streamEventNames: {
                  initialStream: (id) => ({ 
                    data: `mistake_chat_stream_${id}`, 
                    reasoning: `mistake_chat_stream_${id}_reasoning`,
                    ragSources: `mistake_chat_stream_${id}_rag_sources`
                  }),
                  continuationStream: (id) => ({ 
                    data: `mistake_chat_stream_${id}`, 
                    reasoning: `mistake_chat_stream_${id}_reasoning`,
                    ragSources: `mistake_chat_stream_${id}_rag_sources`
                  }),
                },
                defaultEnableChainOfThought: true,
                defaultEnableRag: enableRag,
                defaultRagTopK: ragTopK,
                defaultSelectedLibraries: selectedLibraries,
              }}
              onCoreStateUpdate={(data) => {
                // 减少控制台噪音
                if (import.meta.env.DEV) {
                  console.log('🔄 错题详情状态更新:', {
                    chatHistoryLength: data.chatHistory.length,
                    thinkingContentSize: data.thinkingContent.size
                  });
                }
              }}
              onSaveRequest={async (data) => {
                try {
                  // 🎯 如果有总结内容，解析并更新
                  let updatedMistake = {
                    ...selectedMistake,
                    chat_history: data.chatHistory
                  };

                  if (data.summaryContent) {
                    console.log('📝 [App] 处理总结内容更新...');
                    // 🎯 修复：改进解析逻辑，保持原始格式
                    const parseSummaryContent = (content: string) => {
                      console.log('📄 [总结解析] 原始内容长度:', content.length);
                      console.log('📄 [总结解析] 原始内容预览:', content.substring(0, 200) + '...');
                      
                      // 🎯 策略1：如果内容较短或者没有明显的分段标识，保存到第一个字段
                      const lines = content.split('\n');
                      const hasNumberedSections = lines.some(line => /^\s*\d+\.\s*(核心知识点|错误分析|学习建议)/.test(line));
                      const hasMarkdownSections = lines.some(line => /^#+\s*(核心知识点|错误分析|学习建议)/.test(line));
                      
                      if (!hasNumberedSections && !hasMarkdownSections) {
                        console.log('📄 [总结解析] 无明确分段，保存到mistake_summary');
                        return {
                          mistakeSummary: content.trim(),
                          userErrorAnalysis: null,
                        };
                      }
                      
                      // 🎯 策略2：尝试分段，但保持更完整的内容
                      let mistakeSummary = '';
                      let userErrorAnalysis = '';
                      let currentSection = '';
                      let includeCurrentLine = false;

                      for (const line of lines) {
                        const trimmedLine = line.trim();
                        
                        // 检测章节标题
                        if (/^\s*\d+\.\s*核心知识点|^#+\s*核心知识点|题目解析|正确解法/.test(trimmedLine)) {
                          currentSection = 'mistake_summary';
                          includeCurrentLine = true;
                        } else if (/^\s*\d+\.\s*错误分析|^#+\s*错误分析|^\s*\d+\.\s*学习建议|^#+\s*学习建议|薄弱环节/.test(trimmedLine)) {
                          currentSection = 'user_error_analysis';
                          includeCurrentLine = true;
                        } else {
                          includeCurrentLine = true;
                        }
                        
                        if (includeCurrentLine) {
                          if (currentSection === 'mistake_summary') {
                            mistakeSummary += line + '\n';
                          } else if (currentSection === 'user_error_analysis') {
                            userErrorAnalysis += line + '\n';
                          } else if (!currentSection) {
                            // 如果还没有检测到分段，先放到第一个字段
                            mistakeSummary += line + '\n';
                          }
                        }
                      }

                      // 🎯 策略3：如果分段后某个字段为空，将所有内容保存到第一个字段
                      if (!mistakeSummary.trim() && !userErrorAnalysis.trim()) {
                        console.log('📄 [总结解析] 分段失败，保存完整内容到mistake_summary');
                        return {
                          mistakeSummary: content.trim(),
                          userErrorAnalysis: null,
                        };
                      }
                      
                      console.log('📄 [总结解析] 分段结果:', {
                        mistakeSummaryLength: mistakeSummary.trim().length,
                        userErrorAnalysisLength: userErrorAnalysis.trim().length
                      });

                      return {
                        mistakeSummary: mistakeSummary.trim() || null,
                        userErrorAnalysis: userErrorAnalysis.trim() || null,
                      };
                    };

                    const { mistakeSummary, userErrorAnalysis } = parseSummaryContent(data.summaryContent);
                    
                    updatedMistake = {
                      ...updatedMistake,
                      mistake_summary: mistakeSummary,
                      user_error_analysis: userErrorAnalysis,
                      status: "completed", // 🎯 修复：设置状态为已完成
                      updated_at: new Date().toISOString()
                    };
                    
                    console.log('📝 [App] 总结内容已加入更新数据');
                  }

                  await TauriAPI.updateMistake(updatedMistake);
                  
                  // 🎯 更新本地状态 - 确保 selectedMistake 包含最新的总结内容
                  handleUpdateMistake(updatedMistake);
                  setSelectedMistake(updatedMistake); // 重要：直接更新 selectedMistake
                  
                  console.log('✅ [App] 错题状态已更新，包含总结内容');
                  alert('错题已更新！');
                } catch (error) {
                  console.error('更新失败:', error);
                  alert('更新失败: ' + error);
                }
              }}
                onExitRequest={() => setCurrentView('library')}
              />
            );
          })()}
          </div>
          {/* ANKI制卡组件始终挂载，只是控制显示/隐藏 */}
          <div className="page-container" style={{ display: currentView === 'anki-generation' ? 'block' : 'none' }}>
            <AnkiCardGeneration
              onTemplateSelectionRequest={handleTemplateSelectionRequest}
            />
          </div>
          {/* RAG知识库管理组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'knowledge-base' ? 'block' : 'none' }}>
            <EnhancedKnowledgeBaseManagement />
          </div>
          {/* RAG智能查询组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'rag-query' ? 'block' : 'none' }}>
            <EnhancedRagQueryView />
          </div>
          {/* 数据管理组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'data-management' ? 'block' : 'none' }}>
            <DataImportExport />
          </div>
          {/* 图片遮罩卡组件始终挂载，只是控制显示/隐藏 - 实现保活机制 */}
          <div className="page-container" style={{ display: currentView === 'image-occlusion' ? 'block' : 'none' }}>
            <ImageOcclusion />
          </div>
          {/* 模板管理页面 */}
          <div className="page-container" style={{ display: currentView === 'template-management' ? 'block' : 'none' }}>
            <TemplateManagementPage
              isSelectingMode={isSelectingTemplate}
              onTemplateSelected={handleTemplateSelected}
              onCancel={handleTemplateSelectionCancel}
            />
          </div>
          {/* Gemini适配器测试页面 */}
          <div className="page-container" style={{ display: currentView === 'gemini-adapter-test' ? 'block' : 'none' }}>
            <GeminiAdapterTest />
          </div>
          {/* CogniGraph知识图谱管理页面 */}
          <div className="page-container" style={{ display: currentView === 'cogni-graph' ? 'block' : 'none' }}>
            <KnowledgeGraphManagement />
          </div>
        </div>
      </main>

      {/* 图片查看器 */}
      <ImageViewer
        images={questionImageUrls}
        currentIndex={currentImageIndex}
        isOpen={imageViewerOpen}
        onClose={() => setImageViewerOpen(false)}
        onNext={() => setCurrentImageIndex(prev => (prev + 1) % questionImageUrls.length)}
        onPrev={() => setCurrentImageIndex(prev => (prev - 1 + questionImageUrls.length) % questionImageUrls.length)}
      />
      </div>
    </div>
    </SubjectProvider>
  );
}

export default App;
