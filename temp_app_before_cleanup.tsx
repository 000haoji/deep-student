import { useState, useEffect, useCallback } from 'react';
import { MistakeLibrary } from './components/MistakeLibrary';
import { Settings } from './components/Settings';
import { BatchAnalysis } from './components/BatchAnalysis';
import { ReviewAnalysis } from './components/ReviewAnalysis';
import { Dashboard } from './components/Dashboard';
import ReviewAnalysisDashboard from './components/ReviewAnalysisDashboard';
import CreateReviewAnalysisView from './components/CreateReviewAnalysisView';
import ReviewAnalysisSessionView from './components/ReviewAnalysisSessionView';
import { ReviewAnalysisLibrary } from './components/ReviewAnalysisLibrary';
import { MistakeDetail } from './components/MistakeDetail';
import { DataImportExport } from './components/DataImportExport';
import BackendTest from './components/BackendTest';
import { FormulaTest } from './components/FormulaTest';
import { MessageWithThinking } from './components/MessageWithThinking';
import { AnalysisWithAISDK } from './components/AnalysisWithAISDK';
import AnkiCardGeneration from './components/AnkiCardGeneration';
import { KnowledgeBaseManagement } from './components/KnowledgeBaseManagement';
import { RagQueryView } from './components/RagQueryView';
import { WindowControls } from './components/WindowControls';
import { useWindowDrag } from './hooks/useWindowDrag';
import { ImageViewer } from './components/ImageViewer';
// 移除不再使用的streamHandler import
import { TauriAPI, MistakeItem } from './utils/tauriApi';
import './App.css';
import './DeepStudent.css';
import './components/AnkiCardGeneration.css';

interface ChatMessage {
  role: string;
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



type CurrentView = 'analysis' | 'library' | 'settings' | 'mistake-detail' | 'batch' | 'review' | 'dashboard' | 'data-management' | 'backend-test' | 'formula-test' | 'ai-sdk-analysis' | 'unified-review' | 'create-review' | 'review-session' | 'review-library' | 'anki-generation' | 'knowledge-base';

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
  
  // 监听器管理
  const [activeListeners, setActiveListeners] = useState<Array<() => void>>([]);
  
  // RAG相关状态
  const [enableRag, setEnableRag] = useState(false);
  const [ragTopK, setRagTopK] = useState(5);

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

  // 加载RAG设置
  useEffect(() => {
    const loadRagSettings = async () => {
      try {
        const [ragEnabled, ragTopKSetting] = await Promise.all([
          TauriAPI.getSetting('rag_enabled').catch(() => 'false'),
          TauriAPI.getSetting('rag_top_k').catch(() => '5'),
        ]);
        setEnableRag(ragEnabled === 'true');
        setRagTopK(parseInt(ragTopKSetting) || 5);
      } catch (error) {
        console.error('加载RAG设置失败:', error);
      }
    };

    loadRagSettings();
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

  // 键盘快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // 只在没有输入框聚焦时处理快捷键
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) {
        return;
      }

      // Ctrl/Cmd + 数字键切换视图
      if ((e.ctrlKey || e.metaKey) && !e.shiftKey && !e.altKey) {
        switch (e.key) {
          case '1':
            e.preventDefault();
            setCurrentView('analysis');
            break;
          case '2':
            e.preventDefault();
            setCurrentView('batch');
            break;
          case '3':
            e.preventDefault();
            setCurrentView('library');
            break;
          case '4':
            e.preventDefault();
            setCurrentView('review');
            break;
          case '5':
            e.preventDefault();
            setCurrentView('dashboard');
            break;
          case '6':
            e.preventDefault();
            setCurrentView('settings');
            break;
          case 's':
            e.preventDefault();
            setCurrentView('settings');
            break;
          case 'e':
            e.preventDefault();
            setShowDataManagement(true);
            break;
          case 'f':
            e.preventDefault();
            setCurrentView('formula-test');
            break;
          case 'a':
            e.preventDefault();
            setCurrentView('ai-sdk-analysis');
            break;
        }
      }

      // ESC键返回主页
      if (e.key === 'Escape') {
        if (isChatFullscreen) {
          setIsChatFullscreen(false);
        } else if (showDataManagement) {
          setShowDataManagement(false);
        } else if (currentView === 'mistake-detail') {
          setCurrentView('library');
        } else if (currentView !== 'analysis') {
          setCurrentView('analysis');
        }
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [currentView, showDataManagement, isChatFullscreen]);

  // 组件卸载时清理所有活跃的监听器
  useEffect(() => {
    return () => {
      console.log('🧹 清理组件，注销所有活跃监听器:', activeListeners.length);
      activeListeners.forEach(unlisten => {
        try {
          unlisten();
        } catch (error) {
          console.warn('注销监听器时出错:', error);
        }
      });
    };
  }, [activeListeners]);

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
        
        // 清理之前的监听器
        activeListeners.forEach(unlisten => unlisten());
        setActiveListeners([]);
        
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
            // 清理所有监听器，防止内存泄漏
            activeListeners.forEach(unlisten => {
              try {
                unlisten();
              } catch (error) {
                console.warn('注销监听器时出错（在 finalize 中）:', error);
              }
            });
            setActiveListeners([]); // 清空活跃监听器数组
          } else {
            console.log(`⏳ 流状态: 主内容=${contentListenerActive ? '进行中' : '已完成'}, 思维链=${thinkingListenerActive ? '进行中' : '已完成'}`);
          }
        };

        // 监听主内容流
        await listen(streamEvent, (event: any) => {
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
              // unlistenContent(); // DEBUG: Temporarily commented out
              
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
        // setActiveListeners(prev => [...prev, unlistenContent]); // DEBUG: Temporarily commented out

        // 如果启用了思维链，监听思维链事件
        if (enableChainOfThought) {
          const reasoningEvent = `${streamEvent}_reasoning`;
          console.log(`🧠 监听思维链事件: ${reasoningEvent}`);
          
          await listen(reasoningEvent, (event: any) => {
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
                // unlistenThinking(); // DEBUG: Temporarily commented out
                checkAndFinalizeStreams(); // 检查是否所有流都完成了
              } else if (event.payload.content) {
                fullThinkingContent += event.payload.content;
                console.log(`🧠 累积思维链，当前长度: ${fullThinkingContent.length}`);
                setThinkingContent(prev => new Map(prev).set(0, fullThinkingContent));
              }
            }
          });
          // setActiveListeners(prev => [...prev, unlistenThinking]); // DEBUG: Temporarily commented out
          
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
          
          await listen(ragSourcesEvent, (event: any) => {
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
            // 清理所有监听器
            activeListeners.forEach(unlisten => {
              try {
                unlisten();
              } catch (error) {
                console.warn('注销监听器时出错（在 finalize chat 中）:', error);
              }
            });
            setActiveListeners([]);
          } else {
            console.log(`⏳ 对话流状态: 主内容=${contentListenerActive ? '进行中' : '已完成'}, 思维链=${thinkingListenerActive ? '进行中' : '已完成'}`);
          }
        };

        // 监听主内容流
        await listen(streamEvent, (event: any) => {
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
              // unlistenContent(); // DEBUG: Temporarily commented out
              
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
        // setActiveListeners(prev => [...prev, unlistenContent]); // DEBUG: Temporarily commented out

        // 如果启用了思维链，监听思维链事件
        if (enableChainOfThought) {
          const reasoningEvent = `${streamEvent}_reasoning`;
          console.log(`🧠 监听对话思维链事件: ${reasoningEvent}`);
          const lastMessageIndex = streamingHistory.length - 1;

          await listen(reasoningEvent, (event: any) => {
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
                // unlistenThinking(); // DEBUG: Temporarily commented out
                checkAndFinalizeChatStreams(); // 检查是否所有流都完成了
              } else if (event.payload.content) {
                fullThinkingContent += event.payload.content;
                setThinkingContent(prev => new Map(prev).set(lastMessageIndex, fullThinkingContent));
              }
            }
          });
          // setActiveListeners(prev => [...prev, unlistenThinking]); // DEBUG: Temporarily commented out
        } else {
          thinkingListenerActive = false;
          checkAndFinalizeChatStreams(); // 如果没有思维链，立即检查
        }

        // 如果启用了RAG，监听对话的RAG来源信息
        if (enableRag) {
          const ragSourcesEvent = `${streamEvent}_rag_sources`;
          console.log(`📚 监听对话RAG来源事件: ${ragSourcesEvent}`);
          
          await listen(ragSourcesEvent, (event: any) => {
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
  const handleSelectMistake = (mistake: MistakeItem) => {
    setSelectedMistake(mistake);
    setCurrentView('mistake-detail');
  };

  // 更新错题
  const handleUpdateMistake = (updatedMistake: MistakeItem) => {
    setSelectedMistake(updatedMistake);
  };

  // 删除错题
  const handleDeleteMistake = (mistakeId: string) => {
    console.log('删除错题:', mistakeId);
    setSelectedMistake(null);
    setCurrentView('library');
  };

  // 渲染侧边栏导航 - 现代化风格
  const renderSidebar = () => (
    <div className={`app-sidebar ${sidebarCollapsed ? 'collapsed' : ''}`}>
      <div className="sidebar-header" onMouseDown={startDragging}>
        <div className="app-logo">
          <img src="/logo.svg" alt="Deep Student" className="logo-icon" />
          {!sidebarCollapsed && <span className="logo-text">Deep Student</span>}
        </div>
      </div>
      
      <nav className="sidebar-nav">
        <div className="nav-section">
          <div className="nav-label">{!sidebarCollapsed && '分析工具'}</div>
          <button 
            className={`nav-item tooltip-test ${currentView === 'analysis' ? 'active' : ''}`}
            onClick={() => setCurrentView('analysis')}
            data-tooltip={sidebarCollapsed ? '分析' : ''}
          >
            <span className="nav-icon">📝</span>
            {!sidebarCollapsed && <span className="nav-text">分析</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'batch' ? 'active' : ''}`}
            onClick={() => setCurrentView('batch')}
            data-tooltip={sidebarCollapsed ? '批量分析' : ''}
          >
            <span className="nav-icon">📋</span>
            {!sidebarCollapsed && <span className="nav-text">批量分析</span>}
          </button>
        </div>
        
        <div className="nav-section">
          <div className="nav-label">{!sidebarCollapsed && '数据管理'}</div>
          <button 
            className={`nav-item tooltip-test ${currentView === 'library' ? 'active' : ''}`}
            onClick={() => setCurrentView('library')}
            data-tooltip={sidebarCollapsed ? '错题库' : ''}
          >
            <span className="nav-icon">📚</span>
            {!sidebarCollapsed && <span className="nav-text">错题库</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'review' ? 'active' : ''}`}
            onClick={() => setCurrentView('review')}
            data-tooltip={sidebarCollapsed ? '单次回顾' : ''}
          >
            <span className="nav-icon">🔍</span>
            {!sidebarCollapsed && <span className="nav-text">单次回顾</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'unified-review' ? 'active' : ''}`}
            onClick={() => setCurrentView('unified-review')}
            data-tooltip={sidebarCollapsed ? '统一回顾' : ''}
          >
            <span className="nav-icon">🎯</span>
            {!sidebarCollapsed && <span className="nav-text">统一回顾</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'review-library' ? 'active' : ''}`}
            onClick={() => setCurrentView('review-library')}
            data-tooltip={sidebarCollapsed ? '分析库' : ''}
          >
            <span className="nav-icon">📚</span>
            {!sidebarCollapsed && <span className="nav-text">分析库</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'anki-generation' ? 'active' : ''}`}
            onClick={() => setCurrentView('anki-generation')}
            data-tooltip={sidebarCollapsed ? 'ANKI制卡' : ''}
          >
            <span className="nav-icon">🎯</span>
            {!sidebarCollapsed && <span className="nav-text">ANKI制卡</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'knowledge-base' ? 'active' : ''}`}
            onClick={() => setCurrentView('knowledge-base')}
            data-tooltip={sidebarCollapsed ? '知识库' : ''}
          >
            <span className="nav-icon">🧠</span>
            {!sidebarCollapsed && <span className="nav-text">知识库</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'rag-query' ? 'active' : ''}`}
            onClick={() => setCurrentView('rag-query')}
            data-tooltip={sidebarCollapsed ? 'RAG查询' : ''}
          >
            <span className="nav-icon">🔍</span>
            {!sidebarCollapsed && <span className="nav-text">RAG查询</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'dashboard' ? 'active' : ''}`}
            onClick={() => setCurrentView('dashboard')}
            data-tooltip={sidebarCollapsed ? '统计' : ''}
          >
            <span className="nav-icon">📊</span>
            {!sidebarCollapsed && <span className="nav-text">统计</span>}
          </button>
          <button 
            className={`nav-item tooltip-test`}
            onClick={() => setShowDataManagement(true)}
            data-tooltip={sidebarCollapsed ? '数据管理' : ''}
          >
            <span className="nav-icon">📦</span>
            {!sidebarCollapsed && <span className="nav-text">数据</span>}
          </button>
        </div>
        
        <div className="nav-section">
          <div className="nav-label">{!sidebarCollapsed && '开发工具'}</div>
          <button 
            className={`nav-item tooltip-test ${currentView === 'formula-test' ? 'active' : ''}`}
            onClick={() => setCurrentView('formula-test')}
            data-tooltip={sidebarCollapsed ? '公式测试' : ''}
          >
            <span className="nav-icon">🧠</span>
            {!sidebarCollapsed && <span className="nav-text">公式测试</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'ai-sdk-analysis' ? 'active' : ''}`}
            onClick={() => setCurrentView('ai-sdk-analysis')}
            data-tooltip={sidebarCollapsed ? 'AI SDK分析' : ''}
          >
            <span className="nav-icon">🚀</span>
            {!sidebarCollapsed && <span className="nav-text">AI SDK</span>}
          </button>
          <button 
            className={`nav-item tooltip-test ${currentView === 'backend-test' ? 'active' : ''}`}
            onClick={() => setCurrentView('backend-test')}
            data-tooltip={sidebarCollapsed ? '后端测试' : ''}
          >
            <span className="nav-icon">🧪</span>
            {!sidebarCollapsed && <span className="nav-text">后端测试</span>}
          </button>
        </div>
      </nav>
      
      <div className="sidebar-footer">
        <button 
          className={`nav-item tooltip-test ${currentView === 'settings' ? 'active' : ''}`}
          onClick={() => setCurrentView('settings')}
          data-tooltip={sidebarCollapsed ? '设置' : ''}
        >
          <span className="nav-icon">⚙️</span>
          {!sidebarCollapsed && <span className="nav-text">设置</span>}
        </button>
      </div>
    </div>
  );

  // 渲染分析界面 - 左右分栏布局
  const renderAnalysisView = () => (
    <div className="analysis-layout">
      {/* 左侧上传栏 */}
      <div className="left-panel">
        <div className="upload-section">
          <h3>📝 题目上传</h3>
          
          <div className="form-group">
            <label>科目:</label>
            <select 
              value={subject} 
              onChange={(e) => setSubject(e.target.value)}
              disabled={isLoadingSubjects}
            >
              {isLoadingSubjects ? (
                <option value="">加载中...</option>
              ) : (
                availableSubjects.map(subjectOption => (
                  <option key={subjectOption} value={subjectOption}>{subjectOption}</option>
                ))
              )}
            </select>
          </div>

          <div className="form-group">
            <label>题目或解析图片:</label>
            <div 
              className="file-upload-area"
              onClick={handleFileUploadClick}
              onDrop={(e) => {
                e.preventDefault();
                e.currentTarget.classList.remove('drag-over');
                const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                const remainingSlots = 9 - questionImages.length;
                const filesToAdd = files.slice(0, remainingSlots);
                if (filesToAdd.length > 0) {
                  setQuestionImages(prev => [...prev, ...filesToAdd]);
                }
              }}
              onDragOver={(e) => {
                e.preventDefault();
                e.currentTarget.classList.add('drag-over');
              }}
              onDragEnter={(e) => {
                e.preventDefault();
                e.currentTarget.classList.add('drag-over');
              }}
              onDragLeave={(e) => {
                e.preventDefault();
                if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                  e.currentTarget.classList.remove('drag-over');
                }
              }}
            >
              <div className="upload-content">
                <div className="upload-icon">📁</div>
                <div className="upload-text">拖拽图片到此处或点击上传</div>
                <input
                  type="file"
                  multiple
                  accept="image/*"
                  onChange={handleImageUpload}
                  className="file-input"
                  style={{ display: 'none' }}
                />
              </div>
            </div>
            {questionImages.length > 0 && (
              <div 
                className="image-grid-container"
                onDrop={(e) => {
                  e.preventDefault();
                  e.currentTarget.classList.remove('drag-over');
                  const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                  const remainingSlots = 9 - questionImages.length;
                  const filesToAdd = files.slice(0, remainingSlots);
                  if (filesToAdd.length > 0) {
                    setQuestionImages(prev => [...prev, ...filesToAdd]);
                  }
                }}
                onDragOver={(e) => {
                  e.preventDefault();
                  e.currentTarget.classList.add('drag-over');
                }}
                onDragEnter={(e) => {
                  e.preventDefault();
                  e.currentTarget.classList.add('drag-over');
                }}
                onDragLeave={(e) => {
                  e.preventDefault();
                  if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                    e.currentTarget.classList.remove('drag-over');
                  }
                }}
              >
                <div className="image-grid-scroll">
                  {questionImageUrls.map((url, index) => (
                    <div key={index} className="image-thumbnail-container">
                      <img
                        src={url}
                        alt={`题目图片 ${index + 1}`}
                        className="image-thumbnail"
                        onClick={() => openImageViewer(index)}
                      />
                      <button 
                        className="remove-image-btn tooltip-test"
                        onClick={() => removeImage(index)}
                        data-tooltip="删除图片"
                      >
                        ✕
                      </button>
                    </div>
                  ))}
                  {questionImages.length < 9 && (
                    <div className="add-image-placeholder">
                      <input
                        type="file"
                        multiple
                        accept="image/*"
                        onChange={handleImageUpload}
                        className="add-image-input"
                        id="add-more-images"
                      />
                      <label htmlFor="add-more-images" className="add-image-label">
                        <div className="add-image-icon">➕</div>
                        <div className="add-image-text">添加图片</div>
                      </label>
                    </div>
                  )}
                </div>
              </div>
            )}
          </div>


          <div className="form-group">
            <label>你的问题:</label>
            <textarea
              value={userQuestion}
              onChange={(e) => setUserQuestion(e.target.value)}
              placeholder="请描述你对这道题的疑问..."
              rows={3}
            />
          </div>

          <div className="form-group">
            <div style={{ padding: '0.5rem', backgroundColor: '#f0f8ff', borderRadius: '4px', fontSize: '0.9rem', color: '#666' }}>
              ℹ️ 流式输出和思维链已默认启用，为您提供最佳体验
            </div>
          </div>

          {/* RAG设置 */}
          <div className="form-group">
            <div style={{ 
              padding: '1rem', 
              backgroundColor: '#f8f9fa', 
              borderRadius: '8px', 
              border: '1px solid #e9ecef' 
            }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.5rem' }}>
                <span style={{ fontSize: '1.1rem' }}>🧠</span>
                <label style={{ fontSize: '0.95rem', fontWeight: '500', margin: 0 }}>
                  RAG知识库增强
                </label>
              </div>
              
              <label style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', cursor: 'pointer' }}>
                <input 
                  type="checkbox"
                  checked={enableRag}
                  onChange={(e) => setEnableRag(e.target.checked)}
                  style={{ transform: 'scale(1.1)' }}
                />
                <span style={{ fontSize: '0.9rem', color: '#495057' }}>
                  使用知识库增强AI分析（需要先上传文档到知识库）
                </span>
              </label>
              
              {enableRag && (
                <div style={{ 
                  marginTop: '0.75rem', 
                  paddingTop: '0.75rem', 
                  borderTop: '1px solid #dee2e6' 
                }}>
                  <label style={{ 
                    display: 'flex', 
                    alignItems: 'center', 
                    gap: '0.5rem', 
                    fontSize: '0.85rem',
                    color: '#6c757d'
                  }}>
                    检索文档数量:
                    <input 
                      type="number"
                      min="1"
                      max="10"
                      value={ragTopK}
                      onChange={(e) => setRagTopK(parseInt(e.target.value) || 5)}
                      style={{ 
                        width: '60px', 
                        padding: '0.25rem', 
                        borderRadius: '4px', 
                        border: '1px solid #ced4da',
                        fontSize: '0.85rem'
                      }}
                    />
                  </label>
                </div>
              )}
              
              {enableRag && (
                <div style={{ 
                  marginTop: '0.5rem', 
                  fontSize: '0.8rem', 
                  color: '#6c757d',
                  fontStyle: 'italic'
                }}>
                  💡 启用后，AI将从您的知识库中检索相关信息来增强分析准确性
                </div>
              )}
            </div>
          </div>

          <button
            onClick={handleAnalyze}
            disabled={isAnalyzing}
            className="analyze-button"
          >
            {isAnalyzing ? '分析中...' : '开始分析'}
          </button>

          {analysisResult && (
            <div className="action-buttons">
              <button onClick={handleSaveToLibrary} className="save-button">
                💾 保存到错题库
              </button>
              <button onClick={handleReset} className="reset-button">
                🔄 重新分析
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 右侧结果栏 */}
      <div className="right-panel">
        {!ocrResult && !analysisResult ? (
          <div className="empty-result">
            <div className="empty-icon">
              <img src="/dslogo2.png" alt="Deep Student" className="empty-logo" />
            </div>
            <h3>等待分析</h3>
            <p>请在左侧上传题目图片并点击"开始分析"</p>
          </div>
        ) : (
          <div className="analysis-result">
            <div className="result-header">
              <h3>📊 分析结果</h3>
              {isAnalyzing && (
                <div className="analyzing-indicator">
                  <span className="spinner">⏳</span>
                  <span>分析中...</span>
                </div>
              )}
            </div>

            {/* OCR结果区域 - 立即显示 */}
            {ocrResult && (
              <div className="result-info">
                <div className="info-item">
                  <strong>题目类型:</strong> 
                  <span className="info-value">{ocrResult.mistake_type}</span>
                </div>
                <div className="info-item">
                  <strong>标签:</strong> 
                  <div className="tags">
                    {ocrResult.tags.map((tag, index) => (
                      <span key={index} className="tag">{tag}</span>
                    ))}
                  </div>
                </div>
                <div className="info-item">
                  <strong>OCR文本:</strong>
                  <div className="ocr-text">{ocrResult.ocr_text}</div>
                </div>
              </div>
            )}

            {/* AI解答区域 */}
            {isOcrComplete && (
              <div className={`chat-container ${isChatFullscreen ? 'chat-fullscreen' : ''}`}>
                <div className="chat-header">
                  <h4>💬 AI解答</h4>
                  <div className="chat-header-actions">
                    {enableChainOfThought && (
                      <span className="chain-indicator">🧠 思维链模式</span>
                    )}
                    <button 
                      className="chat-fullscreen-toggle"
                      onClick={handleChatFullscreenToggle}
                    >
                      {isChatFullscreen ? '🔲' : '📱'}
                    </button>
                  </div>
                </div>
                
                <div className="chat-history">
                  {chatHistory.map((message, index) => {
                    const isStreaming = streamingMessageIndex === index;
                    const thinking = thinkingContent.get(index);
                    return (
                      <MessageWithThinking
                        key={index}
                        content={message.content}
                        thinkingContent={thinking}
                        isStreaming={isStreaming}
                        role={message.role as 'user' | 'assistant'}
                        timestamp={message.timestamp}
                        ragSources={message.rag_sources}
                      />
                    );
                  })}
                  {isAnalyzing && chatHistory.length === 0 && (
                    <div className="message assistant">
                      <div className="message-content typing">
                        <span className="typing-indicator">
                          <span></span>
                          <span></span>
                          <span></span>
                        </span>
                        AI正在思考中...
                      </div>
                    </div>
                  )}
                  {isChatting && streamingMessageIndex === null && (
                    <div className="message assistant">
                      <div className="message-content typing">
                        <span className="typing-indicator">
                          <span></span>
                          <span></span>
                          <span></span>
                        </span>
                        正在思考中...
                      </div>
                    </div>
                  )}
                </div>

                {/* 只有在分析完成后才显示输入框 */}
                {!isAnalyzing && isInputAllowed && analysisResult && (
                  <div className="chat-input">
                    <input
                      type="text"
                      value={currentMessage}
                      onChange={(e) => setCurrentMessage(e.target.value)}
                      placeholder="继续提问..."
                      onKeyDown={(e) => e.key === 'Enter' && handleSendMessage()}
                      disabled={isChatting}
                    />
                    <button
                      onClick={handleSendMessage}
                      disabled={isChatting || !currentMessage.trim()}
                      className="send-button"
                    >
                      {isChatting ? '⏳' : '📤'}
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );

  return (
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
            >
              {sidebarCollapsed ? '→' : '←'}
            </button>
            <h1 className="content-title">
              {currentView === 'analysis' && '错题分析'}
              {currentView === 'batch' && '批量分析'}
              {currentView === 'library' && '错题库'}
              {currentView === 'review' && '回顾分析'}
              {currentView === 'unified-review' && '统一回顾分析'}
              {currentView === 'create-review' && '创建回顾分析'}
              {currentView === 'review-session' && '回顾分析会话'}
              {currentView === 'review-library' && '回顾分析库'}
              {currentView === 'dashboard' && '统计数据'}
              {currentView === 'settings' && '系统设置'}
              {currentView === 'formula-test' && '公式测试'}
              {currentView === 'ai-sdk-analysis' && 'AI SDK分析'}
              {currentView === 'backend-test' && '后端测试'}
              {currentView === 'mistake-detail' && '错题详情'}
              {currentView === 'anki-generation' && 'ANKI制卡助手'}
              {currentView === 'knowledge-base' && 'RAG知识库管理'}
              {currentView === 'rag-query' && 'RAG智能查询'}
            </h1>
          </div>
        </div>
        <div className="content-body">
          {currentView === 'analysis' && renderAnalysisView()}
          {/* 批量分析组件始终挂载，只是控制显示/隐藏 */}
          <div style={{ display: currentView === 'batch' ? 'block' : 'none' }}>
            <BatchAnalysis onBack={() => setCurrentView('analysis')} />
          </div>
          {currentView === 'library' && (
            <MistakeLibrary 
              onSelectMistake={handleSelectMistake}
              onBack={() => setCurrentView('analysis')}
            />
          )}
          {currentView === 'review' && (
            <ReviewAnalysis onBack={() => setCurrentView('analysis')} />
          )}
          {currentView === 'unified-review' && (
            <ReviewAnalysisDashboard 
              onCreateNew={() => setCurrentView('create-review')}
              onViewSession={(sessionId: string) => {
                setCurrentReviewSessionId(sessionId);
                setCurrentView('review-session');
              }}
            />
          )}
          {currentView === 'create-review' && (
            <CreateReviewAnalysisView 
              onCancel={() => setCurrentView('unified-review')}
              onCreateSuccess={(sessionId: string) => {
                setCurrentReviewSessionId(sessionId);
                setCurrentView('review-session');
              }}
            />
          )}
          {currentView === 'review-session' && currentReviewSessionId && (
            <ReviewAnalysisSessionView 
              sessionId={currentReviewSessionId}
              onBack={() => setCurrentView('unified-review')}
            />
          )}
          {currentView === 'review-library' && (
            <ReviewAnalysisLibrary 
              onSelectAnalysis={(analysis) => {
                setCurrentReviewSessionId(analysis.id);
                setCurrentView('review-session');
              }}
              onBack={() => setCurrentView('unified-review')}
            />
          )}
          {currentView === 'dashboard' && (
            <Dashboard onBack={() => setCurrentView('analysis')} />
          )}
          {currentView === 'settings' && (
            <Settings onBack={() => setCurrentView('analysis')} />
          )}
          {currentView === 'mistake-detail' && selectedMistake && (
            <MistakeDetail 
              mistake={selectedMistake}
              onBack={() => setCurrentView('library')}
              onUpdate={handleUpdateMistake}
              onDelete={handleDeleteMistake}
            />
          )}
          {currentView === 'formula-test' && (
            <FormulaTest onBack={() => setCurrentView('analysis')} />
          )}
          {currentView === 'ai-sdk-analysis' && (
            <AnalysisWithAISDK 
              enableChainOfThought={enableChainOfThought}
              onAnalysisComplete={(result) => {
                console.log('AI SDK Analysis completed:', result);
                // 可以在这里添加额外的处理逻辑
              }}
            />
          )}
          {currentView === 'backend-test' && (
            <BackendTest />
          )}
          {/* ANKI制卡组件始终挂载，只是控制显示/隐藏 */}
          <div style={{ display: currentView === 'anki-generation' ? 'block' : 'none' }}>
            <AnkiCardGeneration 
              subjectConfigs={subjectConfigs}
            />
          </div>
          {currentView === 'knowledge-base' && (
            <KnowledgeBaseManagement />
          )}
          {currentView === 'rag-query' && (
            <RagQueryView />
          )}
        </div>
      </main>

      {/* 数据管理模态框 */}
      {showDataManagement && (
        <div className="modal-overlay">
          <div className="modal-container">
            <DataImportExport onClose={() => setShowDataManagement(false)} />
          </div>
        </div>
      )}

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
  );
}

export default App;
