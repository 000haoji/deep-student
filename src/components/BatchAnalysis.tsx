import React, { useState, useEffect, useCallback } from 'react';
import { TauriAPI } from '../utils/tauriApi';
import { BatchTask, ChatMessage } from '../types';
import UniversalAppChatHost, { UniversalAppChatHostProps } from './UniversalAppChatHost';

interface BatchAnalysisProps {
  onBack: () => void;
}

export const BatchAnalysis: React.FC<BatchAnalysisProps> = ({ onBack }) => {
  // 基础状态
  const [tasks, setTasks] = useState<BatchTask[]>([]);
  const [isProcessing, setIsProcessing] = useState(false);
  const [currentTaskIndex, setCurrentTaskIndex] = useState(-1);
  const [showDetailView, setShowDetailView] = useState(false);
  const [selectedTaskForDetail, setSelectedTaskForDetail] = useState<BatchTask | null>(null);
  const [availableSubjects, setAvailableSubjects] = useState<string[]>([]);
  const [isLoadingSubjects, setIsLoadingSubjects] = useState(true);

  // RAG相关状态
  const [enableRag, setEnableRag] = useState(false);
  const [ragTopK, setRagTopK] = useState(5);
  

  // 工具函数
  const base64ToFile = (base64: string, filename: string): File => {
    const arr = base64.split(',');
    const mime = arr[0].match(/:(.*?);/)?.[1] || 'image/jpeg';
    const bstr = atob(arr[1]);
    let n = bstr.length;
    const u8arr = new Uint8Array(n);
    while (n--) {
      u8arr[n] = bstr.charCodeAt(n);
    }
    return new File([u8arr], filename, { type: mime });
  };

  const generateUniqueId = () => {
    return `batch_task_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  };

  // 基础函数定义
  const updateTask = useCallback((taskId: string, updates: Partial<BatchTask>) => {
    setTasks(prev => prev.map(task => 
      task.id === taskId ? { ...task, ...updates } : task
    ));
  }, []);

  const addTask = useCallback(() => {
    const newTask: BatchTask = {
      id: generateUniqueId(),
      subject: availableSubjects[0] || '数学',
      userQuestion: '',
      questionImages: [],
      analysisImages: [],
      status: 'pending',
      chatHistory: [],
      thinkingContent: new Map(),
      temp_id: null,
      ocr_result: null,
      currentFullContentForStream: '',
      currentThinkingContentForStream: '',
    };
    setTasks(prev => [...prev, newTask]);
  }, [availableSubjects]);

  const removeTask = useCallback((taskId: string) => {
    setTasks(prev => prev.filter(task => task.id !== taskId));
  }, []);

  const saveTasksToStorage = useCallback(async (tasksToSave: BatchTask[]) => {
    try {
      const tasksToStore = tasksToSave.map(task => ({
        ...task,
        thinkingContent: Object.fromEntries(task.thinkingContent || new Map()),
        questionImagesBase64: task.questionImages?.map(file => ({
          name: file.name,
          base64: '' // 简化处理
        })) || [],
        analysisImagesBase64: task.analysisImages?.map(file => ({
          name: file.name,
          base64: '' // 简化处理
        })) || []
      }));
      localStorage.setItem('batch-analysis-tasks', JSON.stringify(tasksToStore));
    } catch (error) {
      console.error('保存任务失败:', error);
    }
  }, []);

  const handleImageUpload = useCallback((taskId: string, files: FileList | File[] | null, isAnalysis = false) => {
    if (!files) return;
    
    const fileArray = Array.from(files);
    updateTask(taskId, {
      [isAnalysis ? 'analysisImages' : 'questionImages']: fileArray
    });
  }, [updateTask]);

  const removeImage = useCallback((taskId: string, imageIndex: number, isAnalysis = false) => {
    const task = tasks.find(t => t.id === taskId);
    if (!task) return;

    const images = isAnalysis ? task.analysisImages : task.questionImages;
    if (!images) return;

    const newImages = images.filter((_, index) => index !== imageIndex);
    updateTask(taskId, {
      [isAnalysis ? 'analysisImages' : 'questionImages']: newImages
    });
  }, [tasks, updateTask]);


  // 加载支持的科目
  useEffect(() => {
    const loadSubjects = async () => {
      try {
        const subjects = await TauriAPI.getSupportedSubjects();
        setAvailableSubjects(subjects);
      } catch (error) {
        console.error('加载科目失败:', error);
        setAvailableSubjects(['数学', '物理', '化学', '英语']);
      } finally {
        setIsLoadingSubjects(false);
      }
    };
    loadSubjects();
  }, []);

  // 组件挂载时恢复正在运行的任务
  useEffect(() => {
    const resumeRunningTasks = async () => {
      const runningTasks = tasks.filter(task => 
        ['ocr_processing', 'awaiting_stream_start', 'streaming_answer'].includes(task.status)
      );
      
      if (runningTasks.length > 0) {
        console.log(`🔄 发现 ${runningTasks.length} 个运行中的任务，正在恢复...`);
        
        // 建立全局状态监听器
        await setupGlobalStatusListeners();
        
        // 恢复流式监听
        for (const task of runningTasks) {
          if (task.temp_id) {
            try {
              if (task.status === 'streaming_answer') {
                console.log(`🔄 恢复流式监听: 任务 ${task.id}`);
                await resumeTaskStreaming(task);
              } else {
                console.log(`🔄 监听状态变化: 任务 ${task.id} (${task.status})`);
                // 为OCR和等待阶段的任务建立状态监听
                await setupTaskStatusListener(task);
              }
            } catch (error) {
              console.error(`恢复任务 ${task.id} 失败:`, error);
              updateTask(task.id, { 
                status: 'error_stream',
                errorDetails: '页面切换后恢复失败: ' + error
              });
            }
          }
        }
      }
    };

    // 延迟执行，确保组件完全挂载
    const timer = setTimeout(resumeRunningTasks, 100);
    return () => clearTimeout(timer);
  }, []); // 只在组件挂载时执行一次

  const setupGlobalStatusListeners = async () => {
    const { listen } = await import('@tauri-apps/api/event');
    
    // 监听任务状态变更事件
    const unlistenStatus = await listen('task_status_update', (event: any) => {
      console.log('🔄 收到任务状态更新:', event.payload);
      const { temp_id, status, details } = event.payload;
      
      // 查找对应的任务并更新状态
      setTasks(prevTasks => {
        const taskIndex = prevTasks.findIndex(t => t.temp_id === temp_id);
        if (taskIndex === -1) return prevTasks;
        
        const newTasks = [...prevTasks];
        newTasks[taskIndex] = {
          ...newTasks[taskIndex],
          status: status,
          ...(details && { errorDetails: details })
        };
        
        console.log(`✅ 更新任务状态: ${newTasks[taskIndex].id} -> ${status}`);
        
        // 如果状态变为streaming_answer，建立流式监听
        if (status === 'streaming_answer') {
          setTimeout(() => {
            resumeTaskStreaming(newTasks[taskIndex]).catch(console.error);
          }, 100);
        }
        
        // 异步保存状态
        saveTasksToStorage(newTasks).catch(console.error);
        return newTasks;
      });
    });

    // 监听流开始事件
    const unlistenStreamStart = await listen('stream_started', (event: any) => {
      console.log('🚀 收到流开始事件:', event.payload);
      const { temp_id } = event.payload;
      
      setTasks(prevTasks => {
        const taskIndex = prevTasks.findIndex(t => t.temp_id === temp_id);
        if (taskIndex === -1) return prevTasks;
        
        const newTasks = [...prevTasks];
        newTasks[taskIndex] = {
          ...newTasks[taskIndex],
          status: 'streaming_answer'
        };
        
        console.log(`🚀 任务开始流式处理: ${newTasks[taskIndex].id}`);
        
        // 建立流式监听
        setTimeout(() => {
          resumeTaskStreaming(newTasks[taskIndex]).catch(console.error);
        }, 100);
        
        saveTasksToStorage(newTasks).catch(console.error);
        return newTasks;
      });
    });

    
  };

  // 为单个任务建立状态监听
  const setupTaskStatusListener = async (task: BatchTask) => {
    if (!task.temp_id) return;
    
    const { listen } = await import('@tauri-apps/api/event');
    
    // 监听该任务的流启动事件
    const streamEvent = `analysis_stream_${task.temp_id}`;
    const reasoningEvent = `${streamEvent}_reasoning`;
    
    console.log(`🎧 为任务 ${task.id} 建立状态监听: ${streamEvent}`);
    
    // 监听流事件，用于检测流开始
    const unlistenStream = await listen(streamEvent, (event: any) => {
      console.log(`📡 任务 ${task.id} 收到流事件，切换到streaming状态`);
      updateTask(task.id, { status: 'streaming_answer' });
      
      // 立即建立完整的流式监听
      setTimeout(() => {
        resumeTaskStreaming(task).catch(console.error);
      }, 50);
    });

    
  };

  // 恢复流式任务监听
  const resumeTaskStreaming = async (task: BatchTask) => {
    if (!task.temp_id) return;
    
    console.log(`🔄 恢复任务 ${task.id} 的流式监听`);
    
    const streamEvent = `analysis_stream_${task.temp_id}`;
    let fullContent = task.currentFullContentForStream || '';
    let fullThinkingContent = task.currentThinkingContentForStream || '';
    let contentListenerActive = true;
    let thinkingListenerActive = true;

    const { listen } = await import('@tauri-apps/api/event');

    // 监听主内容流
    const unlistenContent = await listen(streamEvent, (event: any) => {
      if (!contentListenerActive) return;
      
      console.log(`💬 任务 ${task.id} 收到主内容流:`, event.payload);
      
      if (event.payload) {
        if (event.payload.is_complete) {
          if (event.payload.content && event.payload.content.length >= fullContent.length) {
            fullContent = event.payload.content;
          }
          console.log(`🎉 任务 ${task.id} 主内容流完成，总长度:`, fullContent.length);
          
          updateTask(task.id, {
            chatHistory: [{
              role: 'assistant',
              content: fullContent,
              timestamp: new Date().toISOString(),
            }],
            status: 'completed',
            currentFullContentForStream: fullContent,
          });
          
          contentListenerActive = false;
        } else if (event.payload.content) {
          fullContent += event.payload.content;
          console.log(`📝 任务 ${task.id} 累积内容，当前长度: ${fullContent.length}`);
          
          updateTask(task.id, {
            chatHistory: [{
              role: 'assistant',
              content: fullContent,
              timestamp: new Date().toISOString(),
            }],
            currentFullContentForStream: fullContent,
          });
        }
      }
    });

    // 监听思维链流
    const reasoningEvent = `${streamEvent}_reasoning`;
    const unlistenThinking = await listen(reasoningEvent, (event: any) => {
      if (!thinkingListenerActive) return;
      console.log(`🧠 任务 ${task.id} 思维链流内容:`, event.payload);

      if (event.payload) {
        if (event.payload.is_complete) {
          if (event.payload.content && event.payload.content.length >= fullThinkingContent.length) {
            fullThinkingContent = event.payload.content;
          }
          console.log(`🎉 任务 ${task.id} 思维链流完成，总长度:`, fullThinkingContent.length);
          
          updateTask(task.id, {
            thinkingContent: new Map([[0, fullThinkingContent]]),
            currentThinkingContentForStream: fullThinkingContent,
          });
          
          thinkingListenerActive = false;
        } else if (event.payload.content) {
          fullThinkingContent += event.payload.content;
          console.log(`🧠 任务 ${task.id} 累积思维链，当前长度: ${fullThinkingContent.length}`);
          
          updateTask(task.id, {
            thinkingContent: new Map([[0, fullThinkingContent]]),
            currentThinkingContentForStream: fullThinkingContent,
          });
        }
      }
    });

    // 监听流错误
    const unlistenError = await listen('stream_error', (event: any) => {
      console.error(`❌ 任务 ${task.id} 流式处理错误:`, event.payload);
      updateTask(task.id, {
        status: 'error_stream',
        errorDetails: '流式处理出错: ' + (event.payload?.message || '未知错误'),
      });
      contentListenerActive = false;
      thinkingListenerActive = false;
    });

    // 注册监听器注销函数
    const taskListeners = [unlistenContent, unlistenThinking, unlistenError];
    
  };

  // 处理单个任务的完整分析流程
  const processSingleTask = async (task: BatchTask) => {
    console.log(`🚀 开始处理任务: ${task.id}`);
    
    try {
      // 步骤1: 更新状态为OCR处理中
      updateTask(task.id, { status: 'ocr_processing' });

      // 步骤2: 调用OCR与初步分析
      console.log(`📝 任务 ${task.id}: 开始OCR分析...`);
      const stepResult = await TauriAPI.analyzeStepByStep({
        subject: task.subject,
        question_image_files: task.questionImages,
        analysis_image_files: task.analysisImages,
        user_question: task.userQuestion,
        enable_chain_of_thought: true,
      });

      console.log(`✅ 任务 ${task.id}: OCR分析完成`, stepResult.ocr_result);

      // 步骤3: 存储OCR结果并更新状态
      updateTask(task.id, {
        temp_id: stepResult.temp_id,
        ocr_result: stepResult.ocr_result,
        status: 'awaiting_stream_start',
      });

      // 步骤4: 准备流式处理
      console.log(`🤖 任务 ${task.id}: 开始流式AI解答...`);
      updateTask(task.id, { status: 'streaming_answer' });

      // 创建初始的助手消息（空内容，等待流式填充）
      const initialMessage: ChatMessage = {
        role: 'assistant',
        content: '',
        timestamp: new Date().toISOString(),
      };
      
      updateTask(task.id, { 
        chatHistory: [initialMessage],
        currentFullContentForStream: '',
        currentThinkingContentForStream: '',
      });

      // 步骤5: 设置流式监听器
      const streamEvent = `analysis_stream_${stepResult.temp_id}`;
      let fullContent = '';
      let fullThinkingContent = '';
      let contentListenerActive = true;
      let thinkingListenerActive = true;

      const { listen } = await import('@tauri-apps/api/event');

      // 先定义一个占位函数，稍后在Promise中重新定义
      let checkAndFinalizeTaskStreams = () => {};

      // 监听主内容流
      const unlistenContent = await listen(streamEvent, (event: any) => {
        if (!contentListenerActive) return;
        
        console.log(`💬 任务 ${task.id} 收到主内容流:`, event.payload);
        
        if (event.payload) {
          if (event.payload.is_complete) {
            if (event.payload.content && event.payload.content.length >= fullContent.length) {
              fullContent = event.payload.content;
            }
            console.log(`🎉 任务 ${task.id} 主内容流完成，总长度:`, fullContent.length);
            
            // 更新聊天历史的最终内容
            updateTask(task.id, {
              chatHistory: [{
                role: 'assistant',
                content: fullContent,
                timestamp: new Date().toISOString(),
              }],
            });
            
            contentListenerActive = false;
            checkAndFinalizeTaskStreams();
          } else if (event.payload.content) {
            fullContent += event.payload.content;
            console.log(`📝 任务 ${task.id} 累积内容，当前长度: ${fullContent.length}`);
            
            // 实时更新聊天历史
            updateTask(task.id, {
              chatHistory: [{
                role: 'assistant',
                content: fullContent,
                timestamp: new Date().toISOString(),
              }],
              currentFullContentForStream: fullContent,
            });
          }
        }
      });

      // 监听思维链流
      const reasoningEvent = `${streamEvent}_reasoning`;
      console.log(`🧠 任务 ${task.id} 监听思维链事件: ${reasoningEvent}`);

      const unlistenThinking = await listen(reasoningEvent, (event: any) => {
        if (!thinkingListenerActive) return;
        console.log(`🧠 任务 ${task.id} 思维链流内容:`, event.payload);

        if (event.payload) {
          if (event.payload.is_complete) {
            if (event.payload.content && event.payload.content.length >= fullThinkingContent.length) {
              fullThinkingContent = event.payload.content;
            }
            console.log(`🎉 任务 ${task.id} 思维链流完成，总长度:`, fullThinkingContent.length);
            
            // 更新思维链内容
            updateTask(task.id, {
              thinkingContent: new Map([[0, fullThinkingContent]]),
              currentThinkingContentForStream: fullThinkingContent,
            });
            
            thinkingListenerActive = false;
            checkAndFinalizeTaskStreams();
          } else if (event.payload.content) {
            fullThinkingContent += event.payload.content;
            console.log(`🧠 任务 ${task.id} 累积思维链，当前长度: ${fullThinkingContent.length}`);
            
            // 实时更新思维链内容
            updateTask(task.id, {
              thinkingContent: new Map([[0, fullThinkingContent]]),
              currentThinkingContentForStream: fullThinkingContent,
            });
          }
        }
      });

      // 监听流错误
      const unlistenError = await listen('stream_error', (event: any) => {
        console.error(`❌ 任务 ${task.id} 流式处理错误:`, event.payload);
        updateTask(task.id, {
          status: 'error_stream',
          errorDetails: '流式处理出错: ' + (event.payload?.message || '未知错误'),
        });
        contentListenerActive = false;
        thinkingListenerActive = false;
      });

      // 监听RAG来源信息（如果启用了RAG）
      let unlistenRagSources: (() => void) | null = null;
      if (enableRag) {
        const ragSourcesEvent = `${streamEvent}_rag_sources`;
        console.log(`📚 任务 ${task.id} 监听RAG来源事件: ${ragSourcesEvent}`);
        
        unlistenRagSources = await listen(ragSourcesEvent, (event: any) => {
          console.log(`📚 任务 ${task.id} 收到RAG来源信息:`, event.payload);
          
          if (event.payload && event.payload.sources) {
            // 更新任务的聊天历史，为助手消息添加RAG来源信息
            setTasks(prevTasks => 
              prevTasks.map(currentTask => {
                if (currentTask.id === task.id) {
                  const updatedChatHistory = [...currentTask.chatHistory];
                  if (updatedChatHistory.length > 0 && updatedChatHistory[0].role === 'assistant') {
                    updatedChatHistory[0] = {
                      ...updatedChatHistory[0],
                      rag_sources: event.payload.sources
                    };
                  }
                  return { ...currentTask, chatHistory: updatedChatHistory };
                }
                return currentTask;
              })
            );
            console.log(`✅ 任务 ${task.id} RAG来源信息已更新`);
          }
        });
      }

      // 注册监听器注销函数（任务级别）
      const taskListeners = enableRag 
        ? [unlistenContent, unlistenThinking, unlistenError, unlistenRagSources!]
        : [unlistenContent, unlistenThinking, unlistenError];
      

      // 步骤6: 启动流式解答
      console.log(`🚀 任务 ${task.id} 启动流式解答，temp_id: ${stepResult.temp_id}, enable_rag: ${enableRag}`);
      
      if (enableRag) {
        // 使用RAG增强的流式解答
        await TauriAPI.startRagEnhancedStreamingAnswer({
          temp_id: stepResult.temp_id,
          enable_chain_of_thought: true,
          enable_rag: true,
          rag_options: {
            top_k: ragTopK,
            enable_reranking: true
          }
        });
      } else {
        // 使用普通的流式解答
        await TauriAPI.startStreamingAnswer(stepResult.temp_id, true);
      }

      // 等待流处理完成（通过Promise机制避免状态循环检查）
      return new Promise<void>((resolve, reject) => {
        let isResolved = false;
        
        const resolveOnce = () => {
          if (!isResolved) {
            isResolved = true;
            console.log(`✅ 任务 ${task.id} 处理完成`);
            resolve();
          }
        };

        const rejectOnce = (error: string) => {
          if (!isResolved) {
            isResolved = true;
            console.error(`❌ 任务 ${task.id} 处理失败:`, error);
            reject(new Error(error));
          }
        };

        // 在流监听器中直接调用resolve
        let checkAndFinalizeTaskStreams = () => {
          console.log(`🔍 任务 ${task.id} 检查流完成状态: contentActive=${contentListenerActive}, thinkingActive=${thinkingListenerActive}`);
          
          if (!contentListenerActive) {
            console.log(`✅ 任务 ${task.id} 主内容流已完成，标记任务为完成状态`);
            updateTask(task.id, { 
              status: 'completed',
              currentFullContentForStream: fullContent,
              currentThinkingContentForStream: fullThinkingContent,
            });
            resolveOnce();
          }
          
          if (!contentListenerActive && !thinkingListenerActive) {
            console.log(`🎉 任务 ${task.id} 所有流式内容均已完成`);
          }
        };

        // 设置超时
        setTimeout(() => {
          rejectOnce('任务处理超时');
        }, 60000); // 60秒超时
      });

    } catch (error) {
      console.error(`❌ 任务 ${task.id} 处理失败:`, error);
      updateTask(task.id, {
        status: 'error_ocr',
        errorDetails: `处理失败: ${error}`,
      });
      throw error;
    }
  };

  // 开始批量处理
  const startBatchProcessing = async () => {
    if (tasks.length === 0) {
      alert('请先添加要分析的题目');
      return;
    }

    const validTasks = tasks.filter(task => 
      task.userQuestion.trim() && task.questionImages.length > 0 && task.status === 'pending'
    );

    if (validTasks.length === 0) {
      alert('请确保每个任务都有问题描述和题目图片');
      return;
    }

    setIsProcessing(true);
    console.log(`🚀 开始批量处理 ${validTasks.length} 个任务`);

    try {
      // 严格按顺序处理队列中的任务
      for (let i = 0; i < validTasks.length; i++) {
        const task = validTasks[i];
        setCurrentTaskIndex(i);
        console.log(`📋 处理任务 ${i + 1}/${validTasks.length}: ${task.id}`);
        
        try {
          await processSingleTask(task);
          console.log(`✅ 任务 ${task.id} 处理成功`);
        } catch (error) {
          console.error(`❌ 任务 ${task.id} 处理失败:`, error);
          // 继续处理下一个任务
        }
      }

      console.log('🎉 批量分析完成！');
      alert('批量分析完成！');
    } catch (error) {
      console.error('❌ 批量处理失败:', error);
      alert('批量处理失败: ' + error);
    } finally {
      setIsProcessing(false);
      setCurrentTaskIndex(-1);
    }
  };

  // 创建优化的文件上传点击处理器
  const createFileUploadClickHandler = useCallback((taskId: string, isAnalysisImage = false) => {
    return () => {
      const inputId = isAnalysisImage ? `#analysis-input-${taskId}` : `#file-input-${taskId}`;
      const fileInput = document.querySelector(inputId) as HTMLInputElement;
      if (fileInput) {
        fileInput.click();
      }
    };
  }, []);

  // 保存单个任务到错题库
  const saveTaskToLibrary = async (taskToSave: BatchTask) => {
    if (!taskToSave.temp_id) {
      alert('任务数据不完整，无法保存');
      return;
    }

    try {
      updateTask(taskToSave.id, { status: 'saving' });

      // 复制聊天历史，并将思维链内容保存到message中
      const chatHistoryWithThinking = taskToSave.chatHistory.map((message, index) => {
        if (message.role === 'assistant' && taskToSave.thinkingContent.has(index)) {
          return {
            ...message,
            thinking_content: taskToSave.thinkingContent.get(index)
          };
        }
        return message;
      });

      const result = await TauriAPI.saveMistakeFromAnalysis({
        temp_id: taskToSave.temp_id,
        final_chat_history: chatHistoryWithThinking,
      });

      if (result.success) {
        alert(`任务 ${taskToSave.id} 已保存到错题库！`);
        // 可以选择从队列中移除已保存的任务
        // removeTask(taskToSave.id);
      } else {
        alert('保存失败，请重试');
        updateTask(taskToSave.id, { status: 'completed' });
      }
    } catch (error) {
      console.error('保存失败:', error);
      alert('保存失败: ' + error);
      updateTask(taskToSave.id, { status: 'completed' });
    }
  };

  // 为统一组件创建的适配器函数
  const saveTaskToLibraryAdapter = async (saveData: any, chatImagesToSave?: any[]) => {
    if (!selectedTaskForDetail) return;
    
    // 从当前任务数据构造完整的 BatchTask
    const currentTaskData = tasks.find(t => t.id === selectedTaskForDetail.id);
    if (!currentTaskData) return;
    
    const fullTaskData: BatchTask = {
      ...currentTaskData,
      chatHistory: saveData.chatHistory || currentTaskData.chatHistory,
      thinkingContent: saveData.thinkingContent || currentTaskData.thinkingContent,
      // 如果有图片数据，可以在这里处理
    };
    await saveTaskToLibrary(fullTaskData);
  };

  // 保存所有有内容的任务到错题库
  const saveAllToLibrary = async () => {
    const savableTasks = tasks.filter(task => task.chatHistory.length > 0 || task.status === 'completed');
    if (savableTasks.length === 0) {
      alert('没有可保存的分析结果');
      return;
    }

    if (!confirm(`确定要保存 ${savableTasks.length} 个任务到错题库吗？`)) {
      return;
    }

    let successCount = 0;
    let failCount = 0;

    for (const task of savableTasks) {
      try {
        await saveTaskToLibrary(task);
        successCount++;
      } catch (error) {
        console.error(`保存任务 ${task.id} 失败:`, error);
        failCount++;
      }
    }

    if (failCount === 0) {
      alert(`已成功将 ${successCount} 道题目保存到错题库！`);
    } else {
      alert(`保存完成：成功 ${successCount} 道，失败 ${failCount} 道`);
    }
  };

  // 清空所有任务
  const clearAllTasks = () => {
    if (confirm('确定要清空所有任务吗？')) {
      setTasks([]);
      saveTasksToStorage([]).catch(console.error);
    }
  };

  // 处理聊天历史更新回调 - 添加防抖逻辑
  const handleChatHistoryUpdated = useCallback((taskId: string, newChatHistory: ChatMessage[], newThinkingContent: Map<number, string>) => {
    // 只在聊天历史真正有变化时才更新
    setTasks(prevTasks => {
      const taskIndex = prevTasks.findIndex(t => t.id === taskId);
      if (taskIndex === -1) return prevTasks;
      
      const currentTask = prevTasks[taskIndex];
      // 检查是否真的有变化
      if (currentTask.chatHistory.length === newChatHistory.length && 
          currentTask.thinkingContent.size === newThinkingContent.size) {
        return prevTasks; // 没有变化，不更新
      }
      
      const newTasks = [...prevTasks];
      newTasks[taskIndex] = {
        ...currentTask,
        chatHistory: newChatHistory,
        thinkingContent: newThinkingContent,
      };
      return newTasks;
    });
  }, []);

  // 为UniversalAppChatHost创建稳定的核心状态更新回调
  const handleCoreStateUpdate = useCallback((taskId: string) => {
    return (data: any) => {
      // 实时更新聊天历史，确保批量分析详情页面能够实时渲染
      if (data.chatHistory.length > 0) {
        console.log('📝 BatchAnalysis: 实时状态更新', {
          taskId,
          chatHistoryLength: data.chatHistory.length,
          lastMessageLength: data.chatHistory[data.chatHistory.length - 1]?.content?.length || 0
        });
        handleChatHistoryUpdated(taskId, data.chatHistory, data.thinkingContent);
      }
    };
  }, [handleChatHistoryUpdated]);

  const getStatusText = (status: string) => {
    switch (status) {
      case 'pending': return '等待中';
      case 'ocr_processing': return 'OCR处理中';
      case 'awaiting_stream_start': return '等待AI解答';
      case 'streaming_answer': return 'AI解答中';
      case 'completed': return '已完成';
      case 'error_ocr': return 'OCR错误';
      case 'error_stream': return '流式处理错误';
      case 'saving': return '保存中';
      default: return '未知状态';
    }
  };

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'pending': return '#6c757d';
      case 'ocr_processing': 
      case 'awaiting_stream_start':
      case 'streaming_answer': return '#007bff';
      case 'completed': return '#28a745';
      case 'error_ocr':
      case 'error_stream': return '#dc3545';
      case 'saving': return '#ffc107';
      default: return '#6c757d';
    }
  };

  // 如果显示详情页面，渲染详情视图
  if (showDetailView && selectedTaskForDetail) {
    // 获取实时的任务数据
    const currentTaskData = tasks.find(t => t.id === selectedTaskForDetail.id) || selectedTaskForDetail;
    
    return (
      <div className="batch-detail-page">
        <div className="detail-header">
          <button 
            onClick={() => {
              setShowDetailView(false);
              setSelectedTaskForDetail(null);
            }} 
            className="back-button"
          >
            ← 返回批量分析
          </button>
          <h2>📋 任务详情 - #{tasks.findIndex(t => t.id === selectedTaskForDetail.id) + 1}</h2>
        </div>
        <div className="detail-content">
          <UniversalAppChatHost
            mode="EXISTING_BATCH_TASK_DETAIL"
            businessSessionId={currentTaskData.id}
            preloadedData={{
              subject: currentTaskData.subject,
              userQuestion: currentTaskData.userQuestion,
              questionImages: currentTaskData.questionImages,
              ocrText: currentTaskData.ocr_result?.ocr_text,
              tags: currentTaskData.ocr_result?.tags || [],
              chatHistory: currentTaskData.chatHistory || [],
              thinkingContent: currentTaskData.thinkingContent || new Map()
            }}
            serviceConfig={{
              apiProvider: {
                initiateAndGetStreamId: async (params) => {
                  // 🎯 修复：检查原临时会话是否还存在，如果不存在则创建新的
                  let workingTempId = currentTaskData.temp_id;
                  
                  if (workingTempId) {
                    try {
                      // 测试原临时会话是否还存在
                      await TauriAPI.continueChatStream({
                        temp_id: workingTempId,
                        chat_history: [],
                        enable_chain_of_thought: false
                      });
                    } catch (error) {
                      // 原临时会话不存在，需要重新创建
                      console.log('🔄 [批量分析] 原临时会话已失效，重新创建临时会话');
                      
                      if (currentTaskData.questionImages && currentTaskData.questionImages.length > 0) {
                        const stepResult = await TauriAPI.analyzeStepByStep({
                          subject: currentTaskData.subject,
                          question_image_files: currentTaskData.questionImages,
                          analysis_image_files: currentTaskData.analysisImages || [],
                          user_question: currentTaskData.userQuestion,
                          enable_chain_of_thought: true,
                        });
                        workingTempId = stepResult.temp_id;
                        console.log('✅ [批量分析] 重新创建临时会话:', workingTempId);
                      }
                    }
                  }
                  
                  return {
                    streamIdForEvents: workingTempId || params.businessId!,
                    ocrResultData: currentTaskData.ocr_result,
                    initialMessages: currentTaskData.chatHistory || []
                  };
                },
                startMainStreaming: async (params) => {
                  // 如果任务还在流式处理中，重新建立流监听
                  if (currentTaskData.status === 'streaming_answer' && currentTaskData.temp_id) {
                    console.log(`🔄 重新建立批量任务 ${currentTaskData.id} 的流监听`);
                    // 不需要调用API，只需要重新监听已经在进行的流
                  }
                  // 如果任务已完成，不需要做任何事情
                },
                continueUserChat: async (params) => {
                  // 🎯 修复：批量分析使用临时会话API，因为使用的是temp_id不是真实的错题ID
                  console.log('🔍 [批量分析追问] 调用参数:', {
                    streamIdForEvents: params.streamIdForEvents,
                    businessId: params.businessId,
                    currentTaskTempId: currentTaskData.temp_id,
                    chatHistoryLength: params.fullChatHistory.length
                  });
                  
                  await TauriAPI.continueChatStream({
                    temp_id: params.streamIdForEvents, // 使用temp_id
                    chat_history: params.fullChatHistory,
                    enable_chain_of_thought: params.enableChainOfThought
                  });
                }
              },
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
              defaultEnableRag: false,
              defaultRagTopK: 5,
            }}
            onCoreStateUpdate={handleCoreStateUpdate(currentTaskData.id)}
            onSaveRequest={async (data) => {
              // 保存批量任务到错题库
              await saveTaskToLibraryAdapter(data, []);
              alert('批量任务已保存到错题库！');
            }}
            onExitRequest={() => {
              setShowDetailView(false);
              setSelectedTaskForDetail(null);
            }}
          />
        </div>
      </div>
    );
  }

  return (
    <div className="batch-analysis">
      <div className="batch-header">
        <button onClick={onBack} className="back-button">
          ← 返回
        </button>
        <h2>📋 批量分析</h2>
        <div className="batch-actions">
          <button onClick={addTask} className="add-button">
            ➕ 添加题目
          </button>
          <button 
            onClick={startBatchProcessing} 
            disabled={isProcessing || tasks.length === 0}
            className="process-button"
          >
            {isProcessing ? '处理中...' : '🚀 开始批量分析'}
          </button>
        </div>
      </div>

      {/* RAG设置面板 */}
      <div className="rag-settings-panel" style={{
        margin: '1rem 0',
        padding: '1rem',
        backgroundColor: '#f8f9fa',
        borderRadius: '8px',
        border: '1px solid #e9ecef'
      }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', marginBottom: '0.5rem' }}>
          <span style={{ fontSize: '1.1rem' }}>🧠</span>
          <label style={{ fontSize: '0.95rem', fontWeight: '500', margin: 0 }}>
            RAG知识库增强（批量分析）
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
            为所有任务启用知识库增强AI分析（需要先上传文档到知识库）
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

      {isProcessing && (
        <div className="progress-info">
          <div className="progress-text">
            正在处理第 {currentTaskIndex + 1} / {tasks.length} 道题目
          </div>
          <div className="progress-bar">
            <div 
              className="progress-fill"
              style={{ width: `${((currentTaskIndex + 1) / Math.max(tasks.length, 1)) * 100}%` }}
            />
          </div>
        </div>
      )}

      <div className="batch-content">
        {tasks.length === 0 ? (
          <div className="empty-batch">
            <p>还没有添加任何题目</p>
            <button onClick={addTask} className="add-first-button">
              添加第一道题目
            </button>
          </div>
        ) : (
          <>
            <div className="batch-summary">
              <div className="summary-item">
                <span className="label">总题目数:</span>
                <span className="value">{tasks.length}</span>
              </div>
              <div className="summary-item">
                <span className="label">已完成:</span>
                <span className="value">
                  {tasks.filter(t => t.status === 'completed').length}
                </span>
              </div>
              <div className="summary-item">
                <span className="label">处理中:</span>
                <span className="value">
                  {tasks.filter(t => ['ocr_processing', 'awaiting_stream_start', 'streaming_answer'].includes(t.status)).length}
                </span>
              </div>
              <div className="summary-item">
                <span className="label">失败:</span>
                <span className="value">
                  {tasks.filter(t => t.status.startsWith('error_')).length}
                </span>
              </div>
            </div>

            <div className="tasks-list">
              {tasks.map((task, index) => (
                <div key={task.id} className={`task-card ${currentTaskIndex === index ? 'current' : ''}`}>
                  <div className="task-header">
                    <span className="task-number">#{index + 1}</span>
                    <span 
                      className="task-status"
                      style={{ color: getStatusColor(task.status) }}
                    >
                      {getStatusText(task.status)}
                    </span>
                    <div className="task-actions">
                      {(task.status !== 'pending' && task.temp_id) && (
                        <button 
                          onClick={() => {
                            setSelectedTaskForDetail(task);
                            setShowDetailView(true);
                          }}
                          className="detail-button"
                        >
                          👁️ 查看详情
                        </button>
                      )}
                      {(task.chatHistory.length > 0 || task.status === 'completed') && (
                        <button 
                          onClick={() => saveTaskToLibrary(task)}
                          className="save-button"
                        >
                          💾 保存
                        </button>
                      )}
                      <button 
                        onClick={() => removeTask(task.id)}
                        disabled={isProcessing && task.status !== 'pending'}
                        className="remove-button"
                      >
                        ✕
                      </button>
                    </div>
                  </div>

                  <div className="task-form">
                    <div className="form-row">
                      <div className="form-group">
                        <label>科目:</label>
                        <select
                          value={task.subject}
                          onChange={(e) => updateTask(task.id, { subject: e.target.value })}
                          disabled={isProcessing || task.status !== 'pending' || isLoadingSubjects}
                        >
                          {isLoadingSubjects ? (
                            <option value="">加载中...</option>
                          ) : (
                            availableSubjects.map(subject => (
                              <option key={subject} value={subject}>{subject}</option>
                            ))
                          )}
                        </select>
                      </div>
                    </div>

                    <div className="form-group">
                      <label>题目图片:</label>
                      <div 
                        className="file-upload-area"
                        onClick={() => {
                          if (!(isProcessing || task.status !== 'pending')) {
                            createFileUploadClickHandler(task.id, false)();
                          }
                        }}
                        onDrop={(e) => {
                          e.preventDefault();
                          e.currentTarget.classList.remove('drag-over');
                          const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                          const remainingSlots = 9 - task.questionImages.length;
                          const filesToAdd = files.slice(0, remainingSlots);
                          if (filesToAdd.length > 0 && !(isProcessing || task.status !== 'pending')) {
                            handleImageUpload(task.id, filesToAdd);
                          }
                        }}
                        onDragOver={(e) => {
                          e.preventDefault();
                          if (!(isProcessing || task.status !== 'pending')) {
                            e.currentTarget.classList.add('drag-over');
                          }
                        }}
                        onDragEnter={(e) => {
                          e.preventDefault();
                          if (!(isProcessing || task.status !== 'pending')) {
                            e.currentTarget.classList.add('drag-over');
                          }
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
                            id={`file-input-${task.id}`}
                            type="file"
                            multiple
                            accept="image/*"
                            onChange={(e) => handleImageUpload(task.id, e.target.files)}
                            disabled={isProcessing || task.status !== 'pending'}
                            className="file-input"
                            style={{ display: 'none' }}
                          />
                        </div>
                      </div>
                      {task.questionImages.length > 0 && (
                        <div 
                          className="image-grid-container"
                          onDrop={(e) => {
                            e.preventDefault();
                            e.currentTarget.classList.remove('drag-over');
                            const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                            const remainingSlots = 9 - task.questionImages.length;
                            const filesToAdd = files.slice(0, remainingSlots);
                            if (filesToAdd.length > 0 && !(isProcessing || task.status !== 'pending')) {
                              handleImageUpload(task.id, filesToAdd);
                            }
                          }}
                          onDragOver={(e) => {
                            e.preventDefault();
                            if (!(isProcessing || task.status !== 'pending')) {
                              e.currentTarget.classList.add('drag-over');
                            }
                          }}
                          onDragEnter={(e) => {
                            e.preventDefault();
                            if (!(isProcessing || task.status !== 'pending')) {
                              e.currentTarget.classList.add('drag-over');
                            }
                          }}
                          onDragLeave={(e) => {
                            e.preventDefault();
                            if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                              e.currentTarget.classList.remove('drag-over');
                            }
                          }}
                        >
                          <div className="image-grid-scroll">
                            {task.questionImages.map((file, imgIndex) => {
                              const imageUrl = URL.createObjectURL(file);
                              return (
                                <div key={imgIndex} className="image-thumbnail-container">
                                  <img
                                    src={imageUrl}
                                    alt={`题目图片 ${imgIndex + 1}`}
                                    className="image-thumbnail"
                                    onLoad={() => URL.revokeObjectURL(imageUrl)}
                                  />
                                  <button 
                                    className="remove-image-btn tooltip-test"
                                    onClick={() => removeImage(task.id, imgIndex)}
                                    disabled={isProcessing || task.status !== 'pending'}
                                    data-tooltip="删除图片"
                                  >
                                    ✕
                                  </button>
                                </div>
                              );
                            })}
                            {task.questionImages.length < 9 && !(isProcessing || task.status !== 'pending') && (
                              <div className="add-image-placeholder">
                                <input
                                  type="file"
                                  multiple
                                  accept="image/*"
                                  onChange={(e) => handleImageUpload(task.id, e.target.files)}
                                  className="add-image-input"
                                  id={`add-more-question-images-${task.id}`}
                                />
                                <label htmlFor={`add-more-question-images-${task.id}`} className="add-image-label">
                                  <div className="add-image-icon">➕</div>
                                  <div className="add-image-text">添加图片</div>
                                </label>
                              </div>
                            )}
                          </div>
                        </div>
                      )}
                    </div>

                    <div className="form-group" style={{ display: 'none' }}>
                      <label>解析图片 (可选):</label>
                      <div 
                        className="file-upload-area"
                        onClick={() => {
                          if (!(isProcessing || task.status !== 'pending')) {
                            createFileUploadClickHandler(task.id, true)();
                          }
                        }}
                        onDrop={(e) => {
                          e.preventDefault();
                          e.currentTarget.classList.remove('drag-over');
                          const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                          const remainingSlots = 9 - task.analysisImages.length;
                          const filesToAdd = files.slice(0, remainingSlots);
                          if (filesToAdd.length > 0 && !(isProcessing || task.status !== 'pending')) {
                            handleImageUpload(task.id, filesToAdd, true);
                          }
                        }}
                        onDragOver={(e) => {
                          e.preventDefault();
                          if (!(isProcessing || task.status !== 'pending')) {
                            e.currentTarget.classList.add('drag-over');
                          }
                        }}
                        onDragEnter={(e) => {
                          e.preventDefault();
                          if (!(isProcessing || task.status !== 'pending')) {
                            e.currentTarget.classList.add('drag-over');
                          }
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
                          <div className="upload-text">拖拽解析图片到此处或点击上传</div>
                          <input
                            id={`analysis-input-${task.id}`}
                            type="file"
                            multiple
                            accept="image/*"
                            onChange={(e) => handleImageUpload(task.id, e.target.files, true)}
                            disabled={isProcessing || task.status !== 'pending'}
                            className="file-input"
                            style={{ display: 'none' }}
                          />
                        </div>
                      </div>
                      {task.analysisImages.length > 0 && (
                        <div 
                          className="image-grid-container"
                          onDrop={(e) => {
                            e.preventDefault();
                            e.currentTarget.classList.remove('drag-over');
                            const files = Array.from(e.dataTransfer.files).filter(file => file.type.startsWith('image/'));
                            const remainingSlots = 9 - task.analysisImages.length;
                            const filesToAdd = files.slice(0, remainingSlots);
                            if (filesToAdd.length > 0 && !(isProcessing || task.status !== 'pending')) {
                              handleImageUpload(task.id, filesToAdd, true);
                            }
                          }}
                          onDragOver={(e) => {
                            e.preventDefault();
                            if (!(isProcessing || task.status !== 'pending')) {
                              e.currentTarget.classList.add('drag-over');
                            }
                          }}
                          onDragEnter={(e) => {
                            e.preventDefault();
                            if (!(isProcessing || task.status !== 'pending')) {
                              e.currentTarget.classList.add('drag-over');
                            }
                          }}
                          onDragLeave={(e) => {
                            e.preventDefault();
                            if (!e.currentTarget.contains(e.relatedTarget as Node)) {
                              e.currentTarget.classList.remove('drag-over');
                            }
                          }}
                        >
                          <div className="image-grid-scroll">
                            {task.analysisImages.map((file, imgIndex) => {
                              const imageUrl = URL.createObjectURL(file);
                              return (
                                <div key={imgIndex} className="image-thumbnail-container">
                                  <img
                                    src={imageUrl}
                                    alt={`解析图片 ${imgIndex + 1}`}
                                    className="image-thumbnail"
                                    onLoad={() => URL.revokeObjectURL(imageUrl)}
                                  />
                                  <button 
                                    className="remove-image-btn tooltip-test"
                                    onClick={() => removeImage(task.id, imgIndex, true)}
                                    disabled={isProcessing || task.status !== 'pending'}
                                    data-tooltip="删除图片"
                                  >
                                    ✕
                                  </button>
                                </div>
                              );
                            })}
                            {task.analysisImages.length < 9 && !(isProcessing || task.status !== 'pending') && (
                              <div className="add-image-placeholder">
                                <input
                                  type="file"
                                  multiple
                                  accept="image/*"
                                  onChange={(e) => handleImageUpload(task.id, e.target.files, true)}
                                  className="add-image-input"
                                  id={`add-more-analysis-images-${task.id}`}
                                />
                                <label htmlFor={`add-more-analysis-images-${task.id}`} className="add-image-label">
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
                      <label>问题描述:</label>
                      <textarea
                        value={task.userQuestion}
                        onChange={(e) => updateTask(task.id, { userQuestion: e.target.value })}
                        placeholder="请描述你对这道题的疑问..."
                        disabled={isProcessing || task.status !== 'pending'}
                        rows={2}
                      />
                    </div>

                    {/* 简化的状态显示 */}
                    {task.status.startsWith('error_') && (
                      <div className="task-error-simple">
                        <p>❌ 处理失败 - 点击查看详情了解错误信息</p>
                      </div>
                    )}

                    {task.status === 'streaming_answer' && (
                      <div className="task-streaming-simple">
                        <div className="streaming-indicator">
                          <span className="spinner">⏳</span>
                          <span>AI正在生成解答...</span>
                        </div>
                      </div>
                    )}

                    {task.status === 'completed' && (
                      <div className="task-completed-simple">
                        <p>✅ 分析完成 - 点击查看详情查看完整结果</p>
                      </div>
                    )}
                  </div>
                </div>
              ))}
            </div>

            <div className="batch-footer">
              <button onClick={clearAllTasks} className="clear-button">
                🗑️ 清空所有
              </button>
              <button 
                onClick={saveAllToLibrary}
                disabled={tasks.filter(t => t.chatHistory.length > 0 || t.status === 'completed').length === 0}
                className="save-all-button"
              >
                💾 保存所有可用的题目
              </button>
            </div>
          </>
        )}
      </div>

    </div>
  );
}