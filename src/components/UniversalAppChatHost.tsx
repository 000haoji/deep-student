import { useState, useEffect, useCallback, useRef } from 'react';
import { MessageWithThinking } from '../chat-core';
import { ImageViewer } from './ImageViewer';
import { 
  ChatMessage, 
  AnalysisResponse, 
  ContinueChatResponse 
} from '../types/index';
// 移除不再使用的streamHandler import
import { TauriAPI } from '../utils/tauriApi';
import { useSubject } from '../contexts/SubjectContext';
import { getCurrentWindow } from '@tauri-apps/api/window';
import '../App.css';
import '../chat-core/styles/index.css';
import ModernSelect from './ModernSelect';

// UniversalAppChatHost Props接口定义
export type HostedChatMode = 
  | 'NEW_MISTAKE_ANALYSIS'     // 对应原 App.tsx 的 'analysis' 视图，从零开始创建新错题分析
  | 'EXISTING_MISTAKE_DETAIL'  // 错题库详情：加载已有错题，进行追问
  | 'EXISTING_BATCH_TASK_DETAIL'// 批量分析任务详情：加载已有分析，进行追问
  | 'REVIEW_SESSION_DETAIL';   // 回顾分析会话详情：启动或继续回顾分析

// API 函数提供者接口，由父组件根据场景实现
export interface HostedChatApiProvider {
  // 阶段一：获取用于流交互的ID及初始数据 (如OCR结果)
  // 对于 NEW_MISTAKE_ANALYSIS: 调用原 App.tsx 中的 TauriAPI.analyzeStepByStep
  // 对于其他模式: 可能直接返回传入的业务ID作为流ID，或调用特定API获取会话详情
  initiateAndGetStreamId: (params: { 
    businessId?: string;          // 外部业务ID (mistakeId, batchId, reviewSessionId)
    subject?: string;
    questionImages?: File[];
    userQuestion?: string;
    enableChainOfThought: boolean;
    enableRag: boolean;
    ragTopK: number;
  }) => Promise<{ 
    streamIdForEvents: string;    // 后端用于流事件的ID (temp_id)
    ocrResultData?: any;          // OCR及初步分类结果
    initialMessages?: ChatMessage[];// 可能由此阶段返回的初始消息
  }>;

  // 阶段二：使用阶段一获取的 streamIdForEvents，正式启动AI回答的流
  startMainStreaming: (params: {
    streamIdForEvents: string;
    enableChainOfThought: boolean;
    enableRag: boolean;
    ragTopK: number;
  }) => Promise<void>;

  // 用于追问
  continueUserChat: (params: { 
    streamIdForEvents: string;     // 当前流ID
    businessId: string;            // 外部业务ID
    fullChatHistory: ChatMessage[];// 包含新用户消息的完整历史
    enableChainOfThought: boolean; 
    enableRag: boolean;
    ragTopK: number;
  }) => Promise<void>;

  // (可选) 获取指定业务ID的完整聊天记录和相关数据
  loadExistingSessionData?: (params: { businessId: string }) => Promise<{
    chatHistory: ChatMessage[];
    thinkingContent?: Map<number, string>;
    ocrResultData?: any;
    // ... other relevant data like subject, userQuestion for display
  }>;
}

export interface UniversalAppChatHostProps {
  mode: HostedChatMode;
  // 外部传入的、代表当前业务实体的唯一ID (如 mistake.id, batchTask.id, reviewSession.id)
  // 这个ID将传递给 apiProvider 中的函数。
  businessSessionId: string; 

  // 预加载的初始数据 (通常用于 EXISTING_* 和 REVIEW_SESSION_DETAIL 模式)
  // 如果未提供，组件可能会尝试通过 apiProvider.loadExistingSessionData 获取
  preloadedData?: {
    subject?: string;
    userQuestion?: string; // 作为标题或问题描述
    questionImages?: File[]; // 或图片URL列表
    questionImageUrls?: string[];
    ocrText?: string;
    tags?: string[];
    chatHistory?: ChatMessage[];
    thinkingContent?: Map<number, string>;
    status?: string; // 新增：允许传入会话状态
    mistake_summary?: string | null; // 错题总结内容
    user_error_analysis?: string | null; // 用户错误分析内容
    // 添加原始错题对象引用，用于更新操作
    originalMistake?: any; // 完整的原始错题对象
    // ... 其他特定于该业务实体的数据
  };

  // 后端交互配置
  serviceConfig: {
    apiProvider: HostedChatApiProvider;
    // 事件名生成函数，'id' 参数是 initiateAndGetStreamId 返回的 streamIdForEvents
    streamEventNames: {
      initialStream: (id: string) => { data: string; reasoning: string; ragSources?: string };
      continuationStream: (id: string) => { data: string; reasoning: string; ragSources?: string };
    };
    defaultEnableChainOfThought: boolean;
    defaultEnableRag: boolean;
    defaultRagTopK: number;
    defaultSelectedLibraries?: string[];
  };

  // 当内部核心状态（聊天记录、思维链、OCR结果）更新时回调
  onCoreStateUpdate?: (data: { 
    chatHistory: ChatMessage[]; 
    thinkingContent: Map<number, string>; 
    ocrResult?: any; 
    isAnalyzing?: boolean; // 当前分析状态
    isChatting?: boolean;  // 当前追问状态
  }) => void;

  // 当用户在组件内部触发保存操作时回调
  onSaveRequest: (data: { 
    businessSessionId: string; 
    chatHistory: ChatMessage[]; 
    thinkingContent: Map<number, string>;
    ocrResult?: any; // OCR结果
    temp_id?: string; // 直接传递的temp_id
    // 总结内容 - 新增
    summaryContent?: string;
    // 原始输入数据，如 subject, userQuestion, questionImages，也应包含，因为保存时可能需要
    originalInputs: { subject: string; userQuestion: string; questionImages: File[]; /*...*/ }
  }) => Promise<void>; 

  onExitRequest?: () => void; // 如果组件有退出/返回按钮
}

// 真实API调用
const analyzeNewMistake = TauriAPI.analyzeNewMistake;

const continueChat = async (request: any): Promise<ContinueChatResponse> => {
  return TauriAPI.continueChat(request);
};

function UniversalAppChatHost(props: UniversalAppChatHostProps) {
  // 调试日志：检查接收到的props数据
  console.log('🔍 UniversalAppChatHost 接收到的 props:', {
    mode: props.mode,
    businessSessionId: props.businessSessionId,
    preloadedDataExists: !!props.preloadedData,
    preloadedChatHistoryLength: props.preloadedData?.chatHistory?.length || 0,
    preloadedChatHistoryData: props.preloadedData?.chatHistory,
    preloadedOcrText: props.preloadedData?.ocrText?.substring(0, 100) + '...',
    questionImageUrls: props.preloadedData?.questionImageUrls?.length || 0
  });
  
  console.log('🔍 [streamId] 组件模式和业务ID:', { mode: props.mode, businessSessionId: props.businessSessionId });
  
  const [isChatFullscreen, setIsChatFullscreen] = useState(false);
  
  // 使用全局科目状态
  const { currentSubject, setCurrentSubject, getEnabledSubjects, loading: isLoadingSubjects } = useSubject();
  
  // 分析相关状态 - 从props或全局状态获取初始值，但优先使用全局状态
  const subject = currentSubject || props.preloadedData?.subject || '未选择科目';
  const availableSubjects = getEnabledSubjects();
  
  // 调试：监听科目变化
  useEffect(() => {
    console.log('📚 [UniversalAppChatHost] 科目状态变化:', {
      currentSubject,
      computedSubject: subject,
      preloadedSubject: props.preloadedData?.subject,
      mode: props.mode
    });
  }, [currentSubject, subject, props.preloadedData?.subject, props.mode]);
  const [userQuestion, setUserQuestion] = useState(props.preloadedData?.userQuestion || '');
  const [questionImages, setQuestionImages] = useState<File[]>(props.preloadedData?.questionImages || []);
  const [questionImageUrls, setQuestionImageUrls] = useState<string[]>(props.preloadedData?.questionImageUrls || []);
  const [imageViewerOpen, setImageViewerOpen] = useState(false);
  const [currentImageIndex, setCurrentImageIndex] = useState(0);
  const [isAnalyzing, setIsAnalyzing] = useState(false);
  const [analysisResult, setAnalysisResult] = useState<AnalysisResponse | null>(
    // 🎯 修复：为已存在的详情页面创建虚拟的analysisResult，确保输入框显示
    props.mode !== 'NEW_MISTAKE_ANALYSIS' && props.preloadedData?.ocrText ? {
      temp_id: props.businessSessionId,
      initial_data: {
        ocr_text: props.preloadedData.ocrText,
        tags: props.preloadedData.tags || [],
        mistake_type: '已加载的分析',
        first_answer: typeof props.preloadedData.chatHistory?.[0]?.content === 'string'
          ? props.preloadedData.chatHistory[0].content
          : '[多模态内容]'
      }
    } : null
  );
  // 过滤掉总结相关的消息，避免在页面刷新时显示
  const filterSummaryMessages = (messages: ChatMessage[]) => {
    console.log('🔍 [过滤器] 输入消息数量:', messages.length);
    console.log('🔍 [过滤器] 原始消息列表:', messages.map((msg, i) => ({
      index: i,
      role: msg.role,
      contentPreview: typeof msg.content === 'string' ? msg.content.substring(0, 100) + '...' : '[非文本内容]',
      contentLength: typeof msg.content === 'string' ? msg.content.length : 0
    })));
    
    const filtered = messages.filter(msg => {
      // 过滤掉总结请求消息
      if (msg.role === 'user' && typeof msg.content === 'string' && msg.content.includes('[SUMMARY_REQUEST]')) {
        console.log('🚫 [过滤] 移除总结请求消息:', msg.content.substring(0, 50) + '...');
        return false;
      }
      // 过滤掉总结响应消息（通过内容特征识别）
      if (msg.role === 'assistant' && typeof msg.content === 'string' && 
          (msg.content.includes('核心知识点') && msg.content.includes('错误分析') && msg.content.includes('学习建议'))) {
        console.log('🚫 [过滤] 移除总结响应消息:', msg.content.substring(0, 50) + '...');
        return false;
      }
      return true;
    });
    
    console.log('✅ [过滤器] 过滤后消息数量:', filtered.length);
    console.log('✅ [过滤器] 过滤后消息列表:', filtered.map((msg, i) => ({
      index: i,
      role: msg.role,
      contentPreview: typeof msg.content === 'string' ? msg.content.substring(0, 100) + '...' : '[非文本内容]'
    })));
    
    return filtered;
  };
  
  const [chatHistory, setChatHistory] = useState<ChatMessage[]>(
    filterSummaryMessages(props.preloadedData?.chatHistory || [])
  );
  
  // 调试日志：检查chatHistory初始化状态
  console.log('🔍 UniversalAppChatHost chatHistory 初始化:', {
    mode: props.mode,
    businessSessionId: props.businessSessionId,
    chatHistoryLength: chatHistory.length,
    chatHistoryData: chatHistory,
    preloadedDataChatHistory: props.preloadedData?.chatHistory,
    preloadedDataChatHistoryLength: props.preloadedData?.chatHistory?.length || 0,
    initialInputAllowed: props.mode !== 'NEW_MISTAKE_ANALYSIS' && props.preloadedData?.chatHistory && props.preloadedData.chatHistory.length > 0
  });
  
  const [currentMessage, setCurrentMessage] = useState('');
  const [isChatting, setIsChatting] = useState(false);
  const [streamingMessageIndex, setStreamingMessageIndex] = useState<number | null>(null);
  const [isInputAllowed, setIsInputAllowed] = useState(
    // 对于非NEW_MISTAKE_ANALYSIS模式，初始允许输入（等待流式处理完成后可以对话）
    props.mode !== 'NEW_MISTAKE_ANALYSIS'
  );
  const [useStreamMode] = useState(true); // 固定启用流式模式
  
  // 新增状态：用于立即显示OCR结果 - 从props获取初始值
  const [ocrResult, setOcrResult] = useState<{
    ocr_text: string;
    tags: string[];
    mistake_type: string;
  } | null>(props.preloadedData?.ocrText ? {
    ocr_text: props.preloadedData.ocrText,
    tags: props.preloadedData.tags || [],
    mistake_type: '已加载的分析'
  } : null);
  const [isOcrComplete, setIsOcrComplete] = useState(() => {
    // 对于错题详情页面，如果有聊天记录，直接认为OCR已完成
    // 🎯 修复：即使OCR结果为空，只要有聊天记录就应该显示聊天界面
    const result = props.mode === 'EXISTING_MISTAKE_DETAIL' ? 
      !!(props.preloadedData?.chatHistory?.length) || !!props.preloadedData?.ocrText :
      !!props.preloadedData?.ocrText;
    
    console.log('🔍 [OCR状态] isOcrComplete 初始化:', {
      mode: props.mode,
      result,
      hasOcrText: !!props.preloadedData?.ocrText,
      chatHistoryLength: props.preloadedData?.chatHistory?.length || 0,
      ocrText: props.preloadedData?.ocrText?.substring(0, 50) + '...',
      logic: props.mode === 'EXISTING_MISTAKE_DETAIL' ? 
        `chatHistory(${props.preloadedData?.chatHistory?.length || 0}) || ocrText(${!!props.preloadedData?.ocrText})` :
        `ocrText(${!!props.preloadedData?.ocrText})`
    });
    
    return result;
  });
  const [isOcrInProgress, setIsOcrInProgress] = useState(false); // 新增：OCR进行状态
  const [enableChainOfThought] = useState(props.serviceConfig.defaultEnableChainOfThought);
  const [thinkingContent, setThinkingContent] = useState<Map<number, string>>(props.preloadedData?.thinkingContent || new Map()); // 存储每条消息的思维链内容
  
  // 总结流式内容状态 - 从数据库加载已有总结内容
  const getInitialSummaryContent = () => {
    if (props.mode === 'EXISTING_MISTAKE_DETAIL' && props.preloadedData) {
      // 合并 mistake_summary 和 user_error_analysis 为显示内容
      const mistakeSummary = props.preloadedData.mistake_summary;
      const userErrorAnalysis = props.preloadedData.user_error_analysis;
      
      console.log('📄 [总结加载] 检查数据库总结内容:', {
        hasMistakeSummary: !!mistakeSummary,
        hasUserErrorAnalysis: !!userErrorAnalysis,
        mistakeSummaryLength: mistakeSummary?.length || 0,
        userErrorAnalysisLength: userErrorAnalysis?.length || 0
      });
      
      if (mistakeSummary || userErrorAnalysis) {
        let combined = '';
        
        // 🎯 修复：保持原始格式，不添加硬编码标题
        if (mistakeSummary) {
          combined += mistakeSummary;
          if (userErrorAnalysis) {
            combined += '\n\n'; // 只在两部分都存在时添加分隔
          }
        }
        if (userErrorAnalysis) {
          combined += userErrorAnalysis;
        }
        
        console.log('📄 [总结加载] 合并后的总结内容:', {
          combinedLength: combined.length,
          combinedPreview: combined.substring(0, 200) + '...'
        });
        
        return combined.trim();
      }
    }
    return '';
  };

  const [summaryStreamContent, setSummaryStreamContent] = useState(getInitialSummaryContent());
  const [summaryStreamComplete, setSummaryStreamComplete] = useState(getInitialSummaryContent() !== '');
  const [isGeneratingSummary, setIsGeneratingSummary] = useState(false);
  
  // 使用useRef来确保事件处理器能访问到最新的isGeneratingSummary状态
  const isGeneratingSummaryRef = useRef(isGeneratingSummary);
  useEffect(() => {
    isGeneratingSummaryRef.current = isGeneratingSummary;
    console.log('🔄 [状态同步] isGeneratingSummary更新为:', isGeneratingSummary);
  }, [isGeneratingSummary]);
  
  // 当前流ID状态 - 用于API调用
  const [currentStreamId, setCurrentStreamId] = useState<string | null>(null);
  
  // 初始化streamId - 对于非NEW_MISTAKE_ANALYSIS模式，使用businessSessionId作为streamId
  useEffect(() => {
    if (props.mode !== 'NEW_MISTAKE_ANALYSIS' && !currentStreamId && props.businessSessionId) {
      console.log('🔧 [初始化] 设置非新分析模式的streamId:', props.businessSessionId);
      setCurrentStreamId(props.businessSessionId);
    }
  }, [props.mode, props.businessSessionId, currentStreamId]);
  
  // RAG相关状态 - 从props获取初始值
  const [enableRag, setEnableRag] = useState(props.serviceConfig.defaultEnableRag);
  const [ragTopK, setRagTopK] = useState(props.serviceConfig.defaultRagTopK);

  // 状态更新回调 - 使用useRef避免过度调用
  const lastStateRef = useRef<string>('');
  const updateTimeoutRef = useRef<number | null>(null);
  useEffect(() => {
    // 在总结生成期间，暂停状态更新回调以避免不必要的重渲染
    if (isGeneratingSummary) {
      console.log('📝 [状态更新] 总结生成中，暂停状态更新回调');
      return;
    }
    
    // 创建状态指纹，只在真正有变化时才调用回调
    const stateFingerprint = JSON.stringify({
      chatHistoryLength: chatHistory.length,
      thinkingContentSize: thinkingContent.size,
      hasOcrResult: !!ocrResult,
      isAnalyzing,
      isChatting
    });
    
    if (lastStateRef.current !== stateFingerprint) {
      lastStateRef.current = stateFingerprint;
      
      // 恢复状态更新回调，但增加防抖机制
      if (updateTimeoutRef.current) {
        clearTimeout(updateTimeoutRef.current);
      }
      
      updateTimeoutRef.current = window.setTimeout(() => {
        props.onCoreStateUpdate?.({
          chatHistory,
          thinkingContent,
          ocrResult,
          isAnalyzing,
          isChatting
        });
        updateTimeoutRef.current = null;
      }, 500); // 500ms防抖
    }
  }, [chatHistory, thinkingContent, ocrResult, isAnalyzing, isChatting, isGeneratingSummary]);

  // 科目现在由全局状态管理，不再需要本地加载

  // 监听preloadedData的变化，特别是chatHistory的实时更新
  useEffect(() => {
    // 🎯 关键修复：防止在流式处理期间或刚完成后错误同步状态
    
    // 1. 仅当流式渲染未在进行时，才考虑从props同步chatHistory
    if (streamingMessageIndex !== null) {
      console.log('⚠️ [同步跳过] 正在流式渲染，暂不更新chatHistory');
      return;
    }
    
    // 2. 如果正在分析或对话中，也跳过同步
    if (isAnalyzing || isChatting) {
      console.log('⚠️ [同步跳过] 正在分析或对话中，暂不更新chatHistory');
      return;
    }
    
    // 3. 对于回顾分析模式，如果当前已有内容且preloaded为空，说明是流式处理完成后的状态，不应该同步
    if (props.mode === 'REVIEW_SESSION_DETAIL' && 
        chatHistory.length > 0 && 
        (!props.preloadedData?.chatHistory || props.preloadedData.chatHistory.length === 0)) {
      console.log('⚠️ [同步跳过] 回顾分析模式，保护已生成的内容');
      return;
    }

    if (props.preloadedData?.chatHistory) {
      const filteredPreloadedHistory = filterSummaryMessages(props.preloadedData.chatHistory);
      
      // 使用JSON字符串比较来检测preloadedData和当前state是否有实质性差异
      // 这比比较长度或时间戳更可靠
      const preloadedHistoryStr = JSON.stringify(filteredPreloadedHistory);
      const currentHistoryStr = JSON.stringify(chatHistory);

      if (preloadedHistoryStr !== currentHistoryStr) {
        console.log('🔄 [状态同步] 检测到 preloadedData.chatHistory 与内部状态不一致，检查是否需要同步。');
        console.log('   - Preloaded 长度:', filteredPreloadedHistory.length);
        console.log('   - 当前 State 长度:', chatHistory.length);
        console.log('   - 模式:', props.mode);
        console.log('   - 分析状态:', { isAnalyzing, isChatting });
        console.log('   - 流式状态:', { streamingMessageIndex });
        
        // 🎯 修复：智能同步逻辑，防止覆盖用户的追问对话
        let shouldSync = false;
        
        if (props.mode === 'EXISTING_MISTAKE_DETAIL') {
          // 🎯 时间窗口保护：如果刚刚完成流式传输，给3秒保护时间
          const now = Date.now();
          const lastCompletionTime = eventListenersRef.current.lastStreamCompletionTime || 0;
          const timeSinceCompletion = now - lastCompletionTime;
          
          if (timeSinceCompletion < 3000) { // 3秒保护窗口
            shouldSync = false;
            console.log('🛡️ [状态同步] 流式完成后的保护窗口期，拒绝同步', {
              timeSinceCompletion,
              lastCompletionTime
            });
          } else if (chatHistory.length === 0) {
            // 完全空的聊天记录，这是真正的初始加载
            shouldSync = true;
            console.log('🔄 [状态同步] 检测到初始加载，执行同步');
          } else if (chatHistory.length < filteredPreloadedHistory.length) {
            // 当前状态记录数少于预加载数据，可能是页面重新加载后需要恢复
            shouldSync = true;
            console.log('🔄 [状态同步] 检测到数据缺失，执行恢复同步');
          } else {
            // 🎯 关键：检查是否有用户追问的新消息
            const currentUserMessages = chatHistory.filter(msg => msg.role === 'user').length;
            const preloadedUserMessages = filteredPreloadedHistory.filter(msg => msg.role === 'user').length;
            
            if (currentUserMessages > preloadedUserMessages) {
              // 当前有更多用户消息，说明有新的追问，绝对不要同步
              shouldSync = false;
              console.log('🛡️ [状态同步] 检测到新的用户追问，保护追问对话，拒绝同步', {
                currentUserMessages,
                preloadedUserMessages,
                currentTotal: chatHistory.length,
                preloadedTotal: filteredPreloadedHistory.length
              });
            } else if (!isAnalyzing && !isChatting && streamingMessageIndex === null) {
              // 没有新的用户消息，但仍有差异，且不在活动状态
              shouldSync = false;
              console.log('🛡️ [状态同步] 检测到其他内容差异，保护现有内容，暂不同步');
            } else {
              // 其他情况（如正在分析/聊天），保持不变
              shouldSync = false;
              console.log('🛡️ [状态同步] 检测到活动状态，暂不同步');
            }
          }
        } else if (props.mode === 'REVIEW_SESSION_DETAIL') {
          // 回顾分析：只有在初始加载时才同步
          shouldSync = chatHistory.length === 0;
          console.log(`🔄 [状态同步] 回顾分析模式：${shouldSync ? '执行' : '跳过'}同步`);
        } else {
          // 其他模式：按原逻辑，只有初始加载时同步
          shouldSync = chatHistory.length === 0;
          console.log(`🔄 [状态同步] 默认模式：${shouldSync ? '执行' : '跳过'}同步`);
        }
        
        if (shouldSync) {
          setChatHistory(filteredPreloadedHistory);
          console.log('✅ [状态同步] 已执行同步');
        } else {
          console.log('🛡️ [状态同步] 保护现有内容，跳过同步');
        }
      }
    }
  }, [props.preloadedData?.chatHistory, streamingMessageIndex, isAnalyzing, isChatting, props.mode]); // 🎯 移除chatHistory.length依赖，避免循环更新

  // 🎯 新增：监听preloadedData的变化，同步总结状态 - 修复切换错题时总结状态持久化问题
  useEffect(() => {
    // 当切换到不同错题时，重新初始化总结状态
    const newSummaryContent = getInitialSummaryContent();
    const currentSummaryStr = summaryStreamContent;
    
    // 只有当总结内容发生实质性变化时才更新
    if (newSummaryContent !== currentSummaryStr) {
      console.log('🔄 [总结同步] 检测到错题数据变化，更新总结状态');
      console.log('   - 新总结内容长度:', newSummaryContent.length);
      console.log('   - 当前总结内容长度:', currentSummaryStr.length);
      console.log('   - 错题ID:', props.businessSessionId);
      
      setSummaryStreamContent(newSummaryContent);
      setSummaryStreamComplete(newSummaryContent !== '');
      
      // 如果没有新的总结内容，确保生成状态被重置
      if (newSummaryContent === '') {
        setIsGeneratingSummary(false);
        console.log('🔄 [总结同步] 重置生成状态，因为新错题无总结内容');
      }
    }
  }, [props.preloadedData?.mistake_summary, props.preloadedData?.user_error_analysis, props.businessSessionId]);

  // 🎯 新增：监听preloadedData的变化，同步图片URL - 修复切换错题时图片不显示问题
  useEffect(() => {
    console.log('🔄 [图片同步] preloadedData变化检测:', {
      hasQuestionImageUrls: !!props.preloadedData?.questionImageUrls,
      newImageUrlsLength: props.preloadedData?.questionImageUrls?.length || 0,
      currentImageUrlsLength: questionImageUrls.length,
      businessSessionId: props.businessSessionId,
      newImageUrls: props.preloadedData?.questionImageUrls
    });

    if (props.preloadedData?.questionImageUrls && props.preloadedData.questionImageUrls.length > 0) {
      const newImageUrls = props.preloadedData.questionImageUrls;

      // 只有当图片URLs发生变化时才更新
      if (JSON.stringify(newImageUrls) !== JSON.stringify(questionImageUrls)) {
        console.log('🔄 [图片同步] 检测到图片数据变化，更新图片URLs');
        console.log('   - 新图片数量:', newImageUrls.length);
        console.log('   - 新图片URLs预览:', newImageUrls.map((url, i) => `${i+1}: ${url.substring(0, 50)}...`));
        console.log('   - 当前图片数量:', questionImageUrls.length);
        console.log('   - 错题ID:', props.businessSessionId);

        setQuestionImageUrls(newImageUrls);
      }
    } else if (questionImageUrls.length > 0) {
      // 如果新的错题没有图片，清空当前图片
      console.log('🔄 [图片同步] 新错题无图片，清空图片URLs');
      setQuestionImageUrls([]);
    }
  }, [props.preloadedData?.questionImageUrls, props.businessSessionId]);

  // 🎯 新增：监听businessSessionId变化，重置关键状态 - 修复切换错题时状态混乱问题
  // 这是保证组件在切换不同错题时能够完全重置的核心逻辑
  useEffect(() => {
    console.log('🔄 [会话重置] businessSessionId 变化，重置所有内部状态:', {
      newId: props.businessSessionId,
      mode: props.mode,
    });

    // 强制重置所有从 preloadedData 派生的状态
    const { preloadedData } = props;
    
    // 1. 重置聊天记录和思维链
    const initialChatHistory = filterSummaryMessages(preloadedData?.chatHistory || []);
    setChatHistory(initialChatHistory);
    
    // 🎯 修复：正确恢复思维链数据
    if (preloadedData?.thinkingContent && preloadedData.thinkingContent instanceof Map) {
      console.log('🧠 [状态重置] 恢复思维链数据，条目数:', preloadedData.thinkingContent.size);
      setThinkingContent(new Map(preloadedData.thinkingContent));
    } else if (preloadedData?.chatHistory) {
      // 🎯 修复：从聊天历史中恢复思维链数据
      const recoveredThinkingContent = new Map<number, string>();
      preloadedData.chatHistory.forEach((message, index) => {
        if (message.role === 'assistant' && message.thinking_content) {
          console.log(`🧠 [状态重置] 从聊天历史恢复思维链，索引${index}:`, message.thinking_content.substring(0, 50) + '...');
          recoveredThinkingContent.set(index, message.thinking_content);
        }
      });
      setThinkingContent(recoveredThinkingContent);
      console.log('🧠 [状态重置] 从聊天历史恢复的思维链条目数:', recoveredThinkingContent.size);
    } else {
      console.log('🧠 [状态重置] 无思维链数据，初始化为空Map');
      setThinkingContent(new Map());
    }
    
    // 2. 重置基础信息
    // 科目现在由全局状态管理，不再需要本地设置
    setUserQuestion(preloadedData?.userQuestion || '');

    // 3. 重置图片
    setQuestionImageUrls(preloadedData?.questionImageUrls || []);
    setQuestionImages(preloadedData?.questionImages || []);

    // 4. 重置OCR和分析结果
    if (preloadedData?.ocrText) {
      setOcrResult({
        ocr_text: preloadedData.ocrText,
        tags: preloadedData.tags || [],
        mistake_type: '已加载的分析'
      });
      setIsOcrComplete(true);
      // 为错题详情页面创建虚拟的analysisResult，确保输入框可用
      setAnalysisResult({
        temp_id: props.businessSessionId,
        initial_data: {
          ocr_text: preloadedData.ocrText,
          tags: preloadedData.tags || [],
          mistake_type: '已加载的分析',
          first_answer: typeof preloadedData.chatHistory?.[0]?.content === 'string'
            ? preloadedData.chatHistory[0].content
            : '[多模态内容]'
        }
      });
    } else {
      // 🎯 修复：即使OCR结果为空，如果有聊天记录也要设置为完成状态
      if (props.mode === 'EXISTING_MISTAKE_DETAIL' && preloadedData?.chatHistory?.length) {
        setOcrResult({
          ocr_text: '',
          tags: preloadedData.tags || [],
          mistake_type: '已保存的错题'
        });
        setIsOcrComplete(true);
        setAnalysisResult({
          temp_id: props.businessSessionId,
          initial_data: {
            ocr_text: '',
            tags: preloadedData.tags || [],
            mistake_type: '已保存的错题',
            first_answer: typeof preloadedData.chatHistory?.[0]?.content === 'string'
              ? preloadedData.chatHistory[0].content
              : '[多模态内容]'
          }
        });
        console.log('🔧 [OCR修复] OCR结果为空但有聊天记录，设置为完成状态以显示聊天界面');
      } else {
        setOcrResult(null);
        setIsOcrComplete(false);
        setAnalysisResult(null);
      }
    }
    
    // 5. 重置总结状态
    const newSummaryContent = getInitialSummaryContent();
    setSummaryStreamContent(newSummaryContent);
    setSummaryStreamComplete(newSummaryContent !== '');
    setIsGeneratingSummary(false);

    // 6. 重置流式处理和交互状态
    setIsAnalyzing(false);
    setIsChatting(false);
    setStreamingMessageIndex(null);
    setIsInputAllowed(props.mode !== 'NEW_MISTAKE_ANALYSIS'); // 允许非新分析模式初始输入
    
    // 7. 重置流ID和自动启动标记
    setCurrentStreamId(props.businessSessionId);
    autoStartExecutedRef.current = false;
    setAutoStartTriggered(false);
    
    console.log('✅ [会话重置] 所有状态已根据新的 businessSessionId 重置完毕。');

  }, [props.businessSessionId, props.preloadedData]); // 依赖项为 businessSessionId 和 preloadedData

  // 自动启动分析逻辑 - 对于REVIEW_SESSION_DETAIL模式且聊天历史为空的情况
  const [autoStartTriggered, setAutoStartTriggered] = useState(false);

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

  // 保留聊天相关的键盘快捷键
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // ESC键退出全屏
      if (e.key === 'Escape' && isChatFullscreen) {
        setIsChatFullscreen(false);
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isChatFullscreen]);

  // Tauri文件拖拽事件监听
  useEffect(() => {
    if (props.mode !== 'NEW_MISTAKE_ANALYSIS') {
      return; // 只在新分析模式下启用拖拽
    }

    let unlisten: (() => void) | undefined;
    
    const setupDragDropListener = async () => {
      try {
        const appWindow = getCurrentWindow();
        console.log('🎯 设置Tauri文件拖拽监听器');
        
        unlisten = await appWindow.onFileDropEvent(async (event) => {
          console.log('🎯 Tauri拖拽事件:', event);
          
          if (event.payload.type === 'drop' && event.payload.paths) {
            console.log('🎯 文件拖拽路径:', event.payload.paths);
            
            // 过滤图片文件
            const imageFiles = event.payload.paths.filter((path: string) => {
              const ext = path.toLowerCase().split('.').pop();
              return ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'].includes(ext || '');
            });
            
            console.log('🎯 过滤后的图片文件:', imageFiles);
            
            if (imageFiles.length > 0) {
              console.log('🎯 处理拖拽的图片文件:', imageFiles);
              
              // 由于Tauri的文件系统API在当前配置下不可用，
              // 我们暂时记录文件路径，提示用户使用文件选择器
              alert(`检测到 ${imageFiles.length} 个图片文件，但拖拽功能需要额外配置。请使用"选择文件"按钮上传图片。`);
              
              // 可以在这里添加将文件路径转换为File对象的逻辑
              // 但需要先在Tauri配置中启用文件系统权限
            }
          } else if (event.payload.type === 'enter') {
            console.log('🎯 文件进入拖拽区域');
            // 可以在这里添加悬停效果
            const uploadArea = document.getElementById('uploadArea');
            if (uploadArea) {
              uploadArea.classList.add('active');
            }
          } else if (event.payload.type === 'leave') {
            console.log('🎯 文件离开拖拽区域');
            // 移除悬停效果
            const uploadArea = document.getElementById('uploadArea');
            if (uploadArea) {
              uploadArea.classList.remove('active');
            }
          }
        });
      } catch (error) {
        console.error('🎯 设置拖拽监听器失败:', error);
      }
    };
    
    setupDragDropListener();
    
    return () => {
      if (unlisten) {
        unlisten();
        console.log('🎯 清理Tauri文件拖拽监听器');
      }
    };
  }, [props.mode, questionImages.length]);


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

  // 清理timeout防止内存泄漏
  useEffect(() => {
    return () => {
      if (updateTimeoutRef.current) {
        clearTimeout(updateTimeoutRef.current);
      }
    };
  }, []);

  // 开始分析 - 使用useCallback确保函数引用稳定
  const handleAnalyze = useCallback(async () => {
    // 对于非NEW_MISTAKE_ANALYSIS模式，可能不需要验证输入
    if (props.mode === 'NEW_MISTAKE_ANALYSIS') {
      if (!userQuestion.trim()) {
        alert('请输入问题描述');
        return;
      }
      if (questionImages.length === 0) {
        alert('请上传至少一张题目图片');
        return;
      }
    }
    
    try {
      if (useStreamMode) {
        console.log('🚀 开始分步骤分析...');
        
        // 设置初始状态：开始OCR
        setIsOcrInProgress(true);
        setIsAnalyzing(false); // OCR阶段还不是分析阶段
        setStreamingMessageIndex(null);
        setOcrResult(null);
        setIsOcrComplete(false);
        setAnalysisResult(null);
        setChatHistory([]);
        
        // 第一步：通过props的apiProvider获取流ID和初始数据
        console.log('📝 第一步：OCR和获取流ID...');
        const stepResult = await props.serviceConfig.apiProvider.initiateAndGetStreamId({
          businessId: props.businessSessionId,
          subject,
          questionImages,
          userQuestion,
          enableChainOfThought,
          enableRag,
          ragTopK
        });

        // 关键修复：先完成异步操作，拿到所有数据后再统一更新状态，避免竞态条件
        console.log('✅ 第一步完成，获得流ID:', stepResult.streamIdForEvents);
        
        // OCR完成，开始AI分析阶段
        setIsOcrInProgress(false);
        setIsAnalyzing(true);
        setCurrentStreamId(stepResult.streamIdForEvents);
        
        // 🎯 重要：重置总结状态，避免显示上一个分析的总结内容
        setSummaryStreamContent('');
        setSummaryStreamComplete(false);
        setIsGeneratingSummary(false);
        console.log('🔄 [新分析] 总结状态已重置');
        
        // 处理OCR结果（如果有）
        if (stepResult.ocrResultData) {
          setOcrResult(stepResult.ocrResultData);
          setIsOcrComplete(true);
          console.log('✅ OCR分析完成:', stepResult.ocrResultData);
        } else if (props.mode === 'REVIEW_SESSION_DETAIL') {
          // 关键修复：对于回顾分析模式，即使没有OCR数据也要创建虚拟结果以启用聊天界面
          const virtualOcrResult = {
            ocr_text: props.preloadedData?.ocrText || '统一回顾分析',
            tags: props.preloadedData?.tags || ['回顾分析'],
            mistake_type: '统一回顾分析'
          };
          setOcrResult(virtualOcrResult);
          setIsOcrComplete(true);
          console.log('✅ 回顾分析虚拟OCR结果已创建，启用聊天界面');
        }
        
        // 处理初始消息（如果有）
        if (stepResult.initialMessages && stepResult.initialMessages.length > 0) {
          setChatHistory(stepResult.initialMessages);
        }
        
        // 创建临时的分析结果对象
        const tempAnalysisResult: AnalysisResponse = {
          temp_id: stepResult.streamIdForEvents,
          initial_data: {
            ocr_text: stepResult.ocrResultData?.ocr_text || (props.mode === 'REVIEW_SESSION_DETAIL' ? '统一回顾分析' : ''),
            tags: stepResult.ocrResultData?.tags || (props.mode === 'REVIEW_SESSION_DETAIL' ? ['回顾分析'] : []),
            mistake_type: stepResult.ocrResultData?.mistake_type || (props.mode === 'REVIEW_SESSION_DETAIL' ? '统一回顾分析' : ''),
            first_answer: '', // 暂时为空，等待流式填充
          },
        };
        setAnalysisResult(tempAnalysisResult);
        
        // 第二步：开始流式AI解答
        console.log('🤖 第二步：开始流式AI解答...');
        
        // 如果没有预设的初始消息，创建一个空的助手消息等待流式填充
        if (!stepResult.initialMessages || stepResult.initialMessages.length === 0) {
          const initialMessage: ChatMessage = {
            role: 'assistant',
            content: '',
            timestamp: new Date().toISOString(),
          };
          console.log('🔧 [初始化] 创建空的助手消息等待流式填充');
          setChatHistory([initialMessage]);
          setStreamingMessageIndex(0);
        } else {
          console.log('🔧 [初始化] 使用预设消息，消息数量:', stepResult.initialMessages.length);
          setChatHistory(stepResult.initialMessages);
          // 如果有预设消息，准备流式更新最后一条助手消息
          const lastIndex = stepResult.initialMessages.length - 1;
          if (stepResult.initialMessages[lastIndex]?.role === 'assistant') {
            console.log('🔧 [初始化] 设置streamingMessageIndex为:', lastIndex);
            setStreamingMessageIndex(lastIndex);
          } else {
            // 如果最后一条不是助手消息，添加一条空的助手消息
            const newMessage: ChatMessage = {
              role: 'assistant',
              content: '',
              timestamp: new Date().toISOString(),
            };
            console.log('🔧 [初始化] 最后一条不是助手消息，添加新的助手消息');
            setChatHistory([...stepResult.initialMessages, newMessage]);
            setStreamingMessageIndex(stepResult.initialMessages.length);
          }
        }
        
        // 启动流式解答
        console.log(`🚀 启动流式解答，streamId: ${stepResult.streamIdForEvents}, enable_chain_of_thought: ${enableChainOfThought}, enable_rag: ${enableRag}`);
        
        await props.serviceConfig.apiProvider.startMainStreaming({
          streamIdForEvents: stepResult.streamIdForEvents,
          enableChainOfThought,
          enableRag,
          ragTopK
        });
        
        // 注意：事件监听逻辑将被移到下面的useEffect中
        
      } else {
        // 使用传统非流式分析
        console.log('📊 使用传统分析模式...');
        // 传统分析模式暂不支持
        throw new Error('传统分析模式暂不支持，请使用流式模式');
      }
    } catch (error) {
      console.error('❌ 分析失败:', error);
      alert('分析失败: ' + error);
      setStreamingMessageIndex(null);
      setOcrResult(null);
      setIsOcrComplete(false);
      setIsOcrInProgress(false);
      setIsAnalyzing(false);
    } finally {
      // setIsAnalyzing(false); // isAnalyzing状态由事件监听器在流结束后设置
    }
  }, [
    props.mode, 
    userQuestion, 
    questionImages, 
    useStreamMode, 
    subject, 
    enableChainOfThought, 
    enableRag, 
    ragTopK,
    props.serviceConfig.apiProvider,
    props.businessSessionId
  ]);

  // 自动启动分析逻辑 - 🎯 修复：使用稳定的函数引用避免重复启动
  const autoStartExecutedRef = useRef(false);
  const handleAnalyzeRef = useRef(handleAnalyze);
  handleAnalyzeRef.current = handleAnalyze; // 保持引用最新
  
  useEffect(() => {
    // 调试日志：检查自动启动条件
    console.log('🔍 检查自动启动条件:', {
      mode: props.mode,
      status: props.preloadedData?.status,
      autoStartTriggered,
      autoStartExecutedRef: autoStartExecutedRef.current,
      chatHistoryLength: chatHistory.length,
      isAnalyzing,
      isChatting
    });

    if (props.mode === 'REVIEW_SESSION_DETAIL' && 
        props.preloadedData?.status === 'pending' &&
        !autoStartTriggered &&
        !autoStartExecutedRef.current &&
        !isAnalyzing && // 🎯 新增：确保不在分析中
        !isChatting && // 🎯 新增：确保不在对话中
        chatHistory.length === 0) { // 🎯 新增：确保聊天记录为空（初始状态）
      console.log('🚀 自动启动回顾分析流式处理 (条件满足)...');
      setAutoStartTriggered(true);
      autoStartExecutedRef.current = true;
      
      // 🎯 使用 ref 调用函数，避免依赖不稳定的函数引用
      console.log('🎯 执行自动启动handleAnalyze');
      handleAnalyzeRef.current().catch(error => {
        console.error('❌ 自动启动分析失败:', error);
        setAutoStartTriggered(false); // 重置状态允许重试
        autoStartExecutedRef.current = false; // 重置执行标记
      });
    } else if (autoStartExecutedRef.current && chatHistory.length > 0) {
      // 🎯 新增：如果已经执行过自动启动且有聊天记录，说明流式处理已完成
      console.log('✅ 自动启动的流式处理已完成，聊天记录长度:', chatHistory.length);
    }
  }, [
    props.mode, 
    props.preloadedData?.status,
    autoStartTriggered,
    isAnalyzing, // 🎯 新增依赖：确保状态变化时重新检查
    isChatting,  // 🎯 新增依赖：确保状态变化时重新检查
    chatHistory.length // 🎯 新增依赖：聊天记录变化时重新检查
    // 🚨 移除 handleAnalyze 依赖！这是导致重复启动的根本原因
    // handleAnalyze 函数因为依赖项变化会不断重新创建，导致这个 useEffect 重复触发
  ]);

  // 使用useRef来保持事件监听器的稳定性
  const eventListenersRef = useRef<{
    unlistenContent?: () => void;
    unlistenThinking?: () => void;
    unlistenRag?: () => void;
    currentStreamId?: string;
    lastEventType?: 'initial' | 'continuation';
    lastEventId?: string; // 🎯 新增：用于防止重复事件处理
    isActivelyStreaming?: boolean; // 🎯 新增：用于保护正在进行的流式传输
    lastStreamCompletionTime?: number; // 🎯 新增：记录上次流式完成时间，用于保护窗口
    streamTimeoutId?: number; // 🎯 新增：流式超时定时器ID
  }>({});

  // 稳定的事件处理函数
  const handleStreamEvent = useCallback((event: any, isThinking: boolean = false) => {
    const currentIsGeneratingSummary = isGeneratingSummaryRef.current;
    console.log(`💬 [Stable] 收到${isThinking ? '思维链' : '主内容'}流:`, event.payload);
    console.log(`🔍 [Stable] 当前isGeneratingSummary状态 (ref):`, currentIsGeneratingSummary);
    console.log(`🔍 [Stable] 事件类型: ${event.payload.is_complete ? '完成信号' : '流式内容'}`);
    console.log(`🔍 [事件处理详情] 处理状态:`, {
      isThinking,
      streamingMessageIndex,
      chatHistoryLength: chatHistory.length,
      targetIndex: streamingMessageIndex ?? (chatHistory.length > 0 ? chatHistory.length - 1 : 0),
      eventContentLength: event.payload.content?.length || 0,
      isComplete: event.payload.is_complete || false,
      mode: props.mode
    });
    
    if (isThinking) {
      // 🎯 修复：改进思维链索引计算逻辑
      let targetIndex;
      
      // 🎯 关键修复：在追问场景中，优先验证streamingMessageIndex的有效性
      if (streamingMessageIndex !== null && streamingMessageIndex >= 0 && streamingMessageIndex < chatHistory.length) {
        // 🎯 进一步验证：确保目标索引对应的是assistant消息
        const targetMessage = chatHistory[streamingMessageIndex];
        if (targetMessage && targetMessage.role === 'assistant') {
          targetIndex = streamingMessageIndex;
          console.log(`🧠 [思维链] 使用已验证的streamingMessageIndex: ${targetIndex}, 对应消息角色: ${targetMessage.role}`);
        } else {
          console.warn(`🧠 [思维链] streamingMessageIndex(${streamingMessageIndex})对应的消息不是assistant或不存在，寻找替代索引`);
          targetIndex = null; // 强制进入查找逻辑
        }
      } else {
        console.log(`🧠 [思维链] streamingMessageIndex无效或为null: ${streamingMessageIndex}, 聊天历史长度: ${chatHistory.length}`);
        targetIndex = null; // 强制进入查找逻辑
      }
      
      // 如果streamingMessageIndex无效，则查找最后一条assistant消息
      if (targetIndex === null) {
        console.log('🧠 [思维链] 开始查找最后一条assistant消息...');
        let lastAssistantIndex = -1;
        for (let i = chatHistory.length - 1; i >= 0; i--) {
          console.log(`🧠 [思维链] 检查索引${i}: 角色=${chatHistory[i]?.role}, 内容长度=${chatHistory[i]?.content?.length || 0}`);
          if (chatHistory[i]?.role === 'assistant') {
            lastAssistantIndex = i;
            console.log(`🧠 [思维链] 找到最后一条assistant消息，索引: ${lastAssistantIndex}`);
            break;
          }
        }
        
        if (lastAssistantIndex >= 0) {
          targetIndex = lastAssistantIndex;
          console.log(`🧠 [思维链] 使用找到的assistant消息索引: ${targetIndex}`);
        } else {
          // 兜底：如果没找到assistant消息，使用末尾索引，但要确保不越界
          targetIndex = Math.max(0, Math.min(chatHistory.length - 1, chatHistory.length - 1));
          console.warn(`🧠 [思维链] 未找到assistant消息，使用兜底索引: ${targetIndex}`);
        }
      }
      
      // 🎯 新增：最终验证目标索引的合理性
      if (targetIndex < 0 || targetIndex >= chatHistory.length) {
        console.error(`🧠 [思维链] 最终计算的索引${targetIndex}超出范围[0, ${chatHistory.length-1}]，丢弃此思维链事件`);
        return;
      }
      
      const finalTargetMessage = chatHistory[targetIndex];
      console.log(`🧠 [思维链] 最终使用索引: ${targetIndex}, 对应消息:`, {
        role: finalTargetMessage?.role,
        contentLength: finalTargetMessage?.content?.length || 0,
        messageExists: !!finalTargetMessage,
        isAssistant: finalTargetMessage?.role === 'assistant'
      });
      
      // 🎯 安全检查：如果目标消息不是assistant，记录警告但仍然处理
      if (finalTargetMessage?.role !== 'assistant') {
        console.warn(`🧠 [思维链] 警告：索引${targetIndex}对应的消息角色是'${finalTargetMessage?.role}'，不是'assistant'`);
      }
      
      setThinkingContent(prev => {
        const newMap = new Map(prev);
        
        // 🎯 修复：根据事件类型决定是追加还是替换
        if (event.payload.is_complete) {
          // 如果是完成事件，使用完整内容
          if (event.payload.content) {
            console.log(`🧠 [思维链完成] 索引${targetIndex}设置完整内容，长度: ${event.payload.content.length}`);
            newMap.set(targetIndex, event.payload.content);
          }
        } else {
          // 如果是流式事件，追加内容
          const currentContent = newMap.get(targetIndex) || '';
          const newContent = currentContent + (event.payload.content || '');
          console.log(`🧠 [思维链流式] 索引${targetIndex}追加内容，从${currentContent.length}到${newContent.length}`);
          newMap.set(targetIndex, newContent);
        }
        
        console.log(`🧠 [思维链状态] 当前思维链映射:`, Array.from(newMap.entries()).map(([idx, content]) => `${idx}: ${content.length}字符`));
        return newMap;
      });
      return;
    }

    // 处理主内容
    if (event.payload.is_complete) {
      console.log('🎉 [Stable] 流式完成');
      
      // 检查是否为总结生成完成
      if (currentIsGeneratingSummary) {
        console.log('📝 [Stable] 总结生成完成');
        if (event.payload.content) {
          setSummaryStreamContent(event.payload.content);
          
          // 🎯 新增：保存总结内容到数据库
          handleSaveSummaryToDatabase(event.payload.content);
        }
        setSummaryStreamComplete(true);
        setIsGeneratingSummary(false);
        console.log('📝 [Stable] 总结状态重置完成');
        return;
      }
      
      // 普通聊天完成处理 - 🎯 新策略：优先保护已累积的内容
      console.log('🎉 [完成处理] 流式传输完成:', {
        targetIndex: streamingMessageIndex ?? (chatHistory.length - 1),
        hasCompletionContent: !!event.payload.content,
        completionContentLength: event.payload.content?.length || 0,
        completionContentPreview: event.payload.content?.substring(0, 100) || '[无内容]'
      });
      
      // 🎯 关键策略：完成事件主要用于标记流式结束，内容保护优先
      setChatHistory(prev => {
        const newHistory = [...prev];
        const targetIndex = streamingMessageIndex ?? newHistory.length - 1;
        if (newHistory[targetIndex]) {
          const currentContent = newHistory[targetIndex].content as string || '';
          let finalContent = currentContent; // 默认保持当前内容
          
          // 只有在特定条件下才使用完成事件的内容
          if (event.payload.content) {
            // 策略1：如果当前内容为空，使用完成事件内容
            if (currentContent.length === 0) {
              finalContent = event.payload.content;
              console.log('🔧 [完成处理] 当前内容为空，使用完成事件内容');
            }
            // 策略2：如果完成事件内容明显更完整（超过当前内容1.5倍），才考虑使用
            else if (event.payload.content.length > currentContent.length * 1.5) {
              finalContent = event.payload.content;
              console.log('🔧 [完成处理] 完成事件内容更完整，使用完成事件内容');
            }
            // 策略3：默认保持当前累积内容
            else {
              console.log('🔧 [完成处理] 保护当前累积内容，忽略完成事件内容');
            }
          } else {
            console.log('🔧 [完成处理] 完成事件无内容，保持当前累积内容');
          }
          
          console.log('🔧 [完成处理] 最终选择:', {
            currentLength: currentContent.length,
            completionLength: event.payload.content?.length || 0,
            finalLength: finalContent.length,
            strategy: finalContent === currentContent ? 'KEEP_CURRENT' : 'USE_COMPLETION'
          });
          
          newHistory[targetIndex] = { ...newHistory[targetIndex], content: finalContent };
        } else {
          console.warn('🚨 [完成处理] 目标索引不存在:', targetIndex);
        }
        return newHistory;
      });
      
      setIsAnalyzing(false);
      setIsChatting(false);
      setIsInputAllowed(true);
      setStreamingMessageIndex(null);
      
      // 🎯 记录流式完成时间，用于状态同步保护
      eventListenersRef.current.lastStreamCompletionTime = Date.now();
      
      // 🎯 清理流式超时定时器
      if (eventListenersRef.current.streamTimeoutId) {
        clearTimeout(eventListenersRef.current.streamTimeoutId);
        eventListenersRef.current.streamTimeoutId = undefined;
        console.log('🧹 [流式完成] 清理超时定时器');
      }
      
    } else if (event.payload.content) {
      // 流式更新
      if (currentIsGeneratingSummary) {
        console.log(`📝 [Stable] 总结流式更新: "${event.payload.content.substring(0, 50)}..."`);
        setSummaryStreamContent(prev => prev + event.payload.content);
        return;
      }
      
      // 普通聊天流式更新
      console.log(`💬 [Stable] 普通聊天流式更新:`, {
        streamingMessageIndex,
        eventContent: event.payload.content.substring(0, 50) + '...',
        eventContentLength: event.payload.content.length
      });
      
      // 🎯 流式事件到达，清理超时定时器（避免误报）
      if (eventListenersRef.current.streamTimeoutId) {
        clearTimeout(eventListenersRef.current.streamTimeoutId);
        eventListenersRef.current.streamTimeoutId = undefined;
        console.log('🧹 [流式到达] 清理超时定时器');
      }
      
      setChatHistory(prev => {
        const newHistory = [...prev];
        const targetIndex = streamingMessageIndex ?? newHistory.length - 1;
        console.log(`💬 [Stable] 更新目标索引: ${targetIndex}, 历史长度: ${newHistory.length}`);
        
        if (newHistory[targetIndex]) {
          const currentContent = newHistory[targetIndex].content as string || '';
          newHistory[targetIndex] = { 
            ...newHistory[targetIndex], 
            content: currentContent + event.payload.content 
          };
          console.log(`💬 [Stable] 更新成功，新内容长度: ${newHistory[targetIndex].content.length}`);
        } else {
          console.warn(`💬 [Stable] 目标索引 ${targetIndex} 不存在于历史记录中`);
        }
        return newHistory;
      });
    }
  }, [streamingMessageIndex, chatHistory.length]);

  // 🎯 新增：保存总结内容到数据库的函数
  const handleSaveSummaryToDatabase = useCallback(async (summaryContent: string) => {
    try {
      // 只有在错题详情模式下才保存总结内容
      if (props.mode !== 'EXISTING_MISTAKE_DETAIL') {
        console.log('📝 [总结保存] 非错题详情模式，跳过保存');
        return;
      }

      // 确保有 businessSessionId（即错题ID）
      if (!props.businessSessionId) {
        console.error('❌ [总结保存] 缺少错题ID，无法保存');
        return;
      }

      console.log('💾 [总结保存] 开始保存总结内容到数据库...');
      console.log('📄 [总结保存] 总结内容长度:', summaryContent.length);
      console.log('🆔 [总结保存] 错题ID:', props.businessSessionId);

      // 🎯 修复：解析总结内容，保持原始格式
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

        for (const line of lines) {
          const trimmedLine = line.trim();
          
          // 检测章节标题
          if (/^\s*\d+\.\s*核心知识点|^#+\s*核心知识点|题目解析|正确解法/.test(trimmedLine)) {
            currentSection = 'mistake_summary';
          } else if (/^\s*\d+\.\s*错误分析|^#+\s*错误分析|^\s*\d+\.\s*学习建议|^#+\s*学习建议|薄弱环节/.test(trimmedLine)) {
            currentSection = 'user_error_analysis';
          }
          
          if (currentSection === 'mistake_summary') {
            mistakeSummary += line + '\n';
          } else if (currentSection === 'user_error_analysis') {
            userErrorAnalysis += line + '\n';
          } else if (!currentSection) {
            // 如果还没有检测到分段，先放到第一个字段
            mistakeSummary += line + '\n';
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

      // 解析总结内容，分离 mistake_summary 和 user_error_analysis
      const { mistakeSummary, userErrorAnalysis } = parseSummaryContent(summaryContent);

      // 使用原始错题对象作为基础，只更新总结字段
      const originalMistake = props.preloadedData?.originalMistake;
      if (!originalMistake) {
        console.error('❌ [总结保存] 缺少原始错题对象，无法更新');
        return;
      }

      const mistakeUpdate = {
        ...originalMistake,
        // 更新总结字段
        mistake_summary: mistakeSummary,
        user_error_analysis: userErrorAnalysis,
        status: "completed", // 🎯 修复：设置状态为已完成
        updated_at: new Date().toISOString(),
      };

      // 调用后端 API 更新错题记录
      const response = await TauriAPI.updateMistake(mistakeUpdate);

      console.log('✅ [总结保存] 总结内容已成功保存到数据库');
      console.log('📊 [总结保存] 更新后的错题数据:', response);

      // 🎯 通过回调通知父组件更新 selectedMistake 状态
      if (props.onSaveRequest) {
        console.log('📢 [总结保存] 通知父组件更新错题状态...');
        await props.onSaveRequest({
          businessSessionId: props.businessSessionId,
          chatHistory,
          thinkingContent,
          summaryContent, // 传递总结内容
          originalInputs: {
            subject: props.preloadedData?.subject || '',
            userQuestion: props.preloadedData?.userQuestion || '',
            questionImages: []
          }
        });
        console.log('✅ [总结保存] 父组件状态更新完成');
      }

    } catch (error) {
      console.error('❌ [总结保存] 保存总结内容失败:', error);
    }
  }, [props.mode, props.businessSessionId, props.preloadedData, chatHistory, thinkingContent, props.onSaveRequest]);

  // 设置事件监听器（仅在streamId变化时）
  useEffect(() => {
    if (!currentStreamId) {
      return;
    }

    // 检查是否需要重新设置监听器（考虑总结生成状态变化）
    const listenerKey = `${currentStreamId}_${isGeneratingSummary ? 'summary' : 'normal'}`;
    if (eventListenersRef.current.currentStreamId === listenerKey) {
      console.log('🎧 [Stable] 监听器已存在且状态匹配，跳过重复设置:', {
        listenerKey,
        isGeneratingSummary
      });
      return;
    }
    
    // 🎯 错题详情和回顾分析页面保护：如果正在流式传输，不要重设监听器
    if ((props.mode === 'EXISTING_MISTAKE_DETAIL' || props.mode === 'REVIEW_SESSION_DETAIL') && 
        eventListenersRef.current.isActivelyStreaming) {
      console.log('🛡️ [流式传输保护] 正在流式传输，跳过监听器重设:', {
        mode: props.mode,
        currentStreamId,
        isActivelyStreaming: eventListenersRef.current.isActivelyStreaming,
        streamingMessageIndex,
        isChatting
      });
      return;
    }
    
    console.log('🎧 [Stable] 需要重新设置监听器:', {
      oldListenerKey: eventListenersRef.current.currentStreamId,
      newListenerKey: listenerKey,
      currentStreamId,
      isGeneratingSummary,
      chatHistoryLength: chatHistory.length
    });

    const setupListeners = async () => {
      // 清理旧的监听器
      eventListenersRef.current.unlistenContent?.();
      eventListenersRef.current.unlistenThinking?.();
      eventListenersRef.current.unlistenRag?.();

      const { listen } = await import('@tauri-apps/api/event');
      
      // 🎯 重要修复：使用稳定的判断逻辑避免事件名称混乱
      let shouldUseContinuation = false;
      
      if (props.mode === 'NEW_MISTAKE_ANALYSIS') {
        // 错题分析：基于聊天历史长度判断
        const isInitialStream = chatHistory.length <= 1;
        shouldUseContinuation = !isInitialStream || isGeneratingSummary;
        
        console.log('🎯 [事件类型判断] 错题分析模式:', {
          chatHistoryLength: chatHistory.length,
          isInitialStream,
          isGeneratingSummary,
          shouldUseContinuation,
          mode: props.mode
        });
      } else if (props.mode === 'REVIEW_SESSION_DETAIL') {
        // 🎯 回顾分析：使用更精确的判断条件
        // 如果是总结生成，总是使用continuation流
        if (isGeneratingSummary) {
          shouldUseContinuation = true;
        } else {
          // 🎯 修复：基于更可靠的条件判断事件类型
          const hasUserMessages = chatHistory.filter(msg => msg.role === 'user').length > 0;
          const hasAssistantContent = chatHistory.some(msg => 
            msg.role === 'assistant' && 
            typeof msg.content === 'string' && 
            msg.content.trim().length > 0
          );
          
          // 🎯 关键修复：对于回顾分析，只有当确实需要追问时才使用continuation流
          // 当有用户消息时，说明是追问场景，应该使用 review_chat_stream_*
          // 当只有助手内容但没有用户消息时，说明是初始分析完成，仍应使用 review_analysis_stream_*
          shouldUseContinuation = hasUserMessages;
          
          console.log('🎯 [事件类型判断] 回顾分析模式（修复版）:', {
            hasUserMessages,
            hasAssistantContent,
            shouldUseContinuation,
            chatHistoryLength: chatHistory.length,
            autoStartExecutedRef: autoStartExecutedRef.current,
            eventType: shouldUseContinuation ? 'review_chat_stream' : 'review_analysis_stream'
          });
        }
      } else if (props.mode === 'EXISTING_MISTAKE_DETAIL') {
        // 🎯 错题详情页面修复：使用更稳定的判断逻辑，避免监听器频繁重设
        if (isGeneratingSummary) {
          // 总结生成总是使用continuation流
          shouldUseContinuation = true;
        } else {
          // 🎯 关键修复：使用预加载数据来判断是否为初始加载
          // 如果有预加载的聊天记录，说明这是已存在的错题，应该使用continuation流
          const hasPreloadedChat = props.preloadedData?.chatHistory && props.preloadedData.chatHistory.length > 0;
          // 或者当前聊天记录已经有用户消息（追问场景）
          const hasUserMessages = chatHistory.filter(msg => msg.role === 'user').length > 0;
          
          // 错题详情页面几乎总是使用continuation流，因为：
          // 1. 如果有预加载聊天记录，说明已有对话历史
          // 2. 如果用户发起追问，也应该使用continuation流
          shouldUseContinuation = hasPreloadedChat || hasUserMessages || chatHistory.length > 1;
          
          console.log('🎯 [事件类型判断] 错题详情模式（修复版）:', {
            hasPreloadedChat,
            hasUserMessages,
            chatHistoryLength: chatHistory.length,
            isGeneratingSummary,
            shouldUseContinuation,
            mode: props.mode
          });
        }
      } else {
        // 其他模式（EXISTING_BATCH_TASK_DETAIL等）：基于聊天历史长度判断
        const isInitialStream = chatHistory.length <= 1;
        shouldUseContinuation = !isInitialStream || isGeneratingSummary;
        
        console.log('🎯 [事件类型判断] 其他模式:', {
          chatHistoryLength: chatHistory.length,
          isInitialStream,
          isGeneratingSummary,
          shouldUseContinuation,
          mode: props.mode
        });
      }
      
      const eventNames = shouldUseContinuation
        ? props.serviceConfig.streamEventNames.continuationStream(currentStreamId)
        : props.serviceConfig.streamEventNames.initialStream(currentStreamId);
      
      console.log('🎧 [Stable] 设置新的事件监听器:', { 
        streamId: currentStreamId, 
        chatHistoryLength: chatHistory.length,
        shouldUseContinuation,
        isGeneratingSummary,
        eventNames,
        mode: props.mode,
        status: props.preloadedData?.status,
        eventType: shouldUseContinuation ? 'CONTINUATION' : 'INITIAL',
        autoStartExecuted: autoStartExecutedRef.current,
        isAnalyzing,
        isChatting
      });

      // 设置主内容监听器
      console.log('🎧 [Stable] 正在设置主内容监听器，事件名:', eventNames.data);
      console.log('🎧 [Stable] 监听器设置时的状态:', {
        currentStreamId,
        isGeneratingSummary: isGeneratingSummaryRef.current,
        mode: props.mode,
        chatHistoryLength: chatHistory.length,
        shouldUseContinuation,
        eventType: shouldUseContinuation ? 'CONTINUATION' : 'INITIAL'
      });
      
      eventListenersRef.current.unlistenContent = await listen(eventNames.data, (event: any) => {
        console.log('🎧 [Stable] 主内容监听器收到事件:', {
          eventName: eventNames.data,
          isGeneratingSummary: isGeneratingSummaryRef.current,
          payload: event.payload,
          payloadContentLength: event.payload?.content?.length || 0,
          isComplete: event.payload?.is_complete || false,
          timestamp: new Date().toISOString(),
          // 🎯 新增：追踪重复接收
          listenerKey: eventListenersRef.current.currentStreamId,
          mode: props.mode,
          // 🎯 新增：当前聊天状态
          currentChatHistoryLength: chatHistory.length,
          currentStreamingIndex: streamingMessageIndex,
          isChatting,
          isAnalyzing
        });
        
        // 🎯 新增：防止重复处理相同事件
        const eventId = `${eventNames.data}_${event.payload?.content?.substring(0, 10) || 'empty'}_${event.payload?.is_complete}`;
        if (eventListenersRef.current.lastEventId === eventId) {
          console.warn('🚨 [Stable] 检测到重复事件，跳过处理:', eventId);
          return;
        }
        eventListenersRef.current.lastEventId = eventId;
        
        // 🎯 错题详情和回顾分析页面：标记流式传输状态
        if (props.mode === 'EXISTING_MISTAKE_DETAIL' || props.mode === 'REVIEW_SESSION_DETAIL') {
          if (!event.payload?.is_complete) {
            eventListenersRef.current.isActivelyStreaming = true;
          } else {
            eventListenersRef.current.isActivelyStreaming = false;
          }
        }
        
        // 🎯 新增：验证事件处理条件
        console.log('🎯 [事件处理] 准备处理事件，当前状态:', {
          hasValidStreamingIndex: streamingMessageIndex !== null,
          chatHistoryLength: chatHistory.length,
          targetIndex: streamingMessageIndex ?? (chatHistory.length - 1),
          isActivelyStreaming: eventListenersRef.current.isActivelyStreaming
        });
        
        handleStreamEvent(event, false);
      });

      // 设置思维链监听器
      if (enableChainOfThought) {
        eventListenersRef.current.unlistenThinking = await listen(eventNames.reasoning, (event: any) => {
          handleStreamEvent(event, true);
        });
      }

      // 🎯 修复：重新添加被遗漏的RAG来源事件监听器
      if (enableRag && eventNames.ragSources) {
        console.log('🎧 [Stable] 正在设置RAG来源监听器，事件名:', eventNames.ragSources);
        eventListenersRef.current.unlistenRag = await listen(eventNames.ragSources, (event: any) => {
          console.log(`📚 [Stable] 收到RAG来源信息:`, event.payload);
          if (event.payload && event.payload.sources) {
            setChatHistory(prev => {
              const newHistory = [...prev];
              const targetIndex = streamingMessageIndex ?? newHistory.length - 1;
              if (newHistory[targetIndex]) {
                newHistory[targetIndex] = { 
                  ...newHistory[targetIndex], 
                  rag_sources: event.payload.sources 
                };
                console.log(`✅ [Stable] RAG来源信息已更新到消息索引: ${targetIndex}`);
              } else {
                console.warn(`⚠️ [Stable] RAG事件：找不到目标消息索引: ${targetIndex}`);
              }
              return newHistory;
            });
          }
        });
      }

      eventListenersRef.current.currentStreamId = listenerKey;
      // eventListenersRef.current.lastEventType = isInitialStream ? 'initial' : 'continuation'; // 临时简化
    };

    setupListeners();

    return () => {
      console.log('🧹 [Stable] 清理事件监听器:', { streamId: currentStreamId });
      eventListenersRef.current.unlistenContent?.();
      eventListenersRef.current.unlistenThinking?.();
      eventListenersRef.current.unlistenRag?.();
      eventListenersRef.current.currentStreamId = undefined;
      
      // 🎯 清理流式超时定时器
      if (eventListenersRef.current.streamTimeoutId) {
        clearTimeout(eventListenersRef.current.streamTimeoutId);
        eventListenersRef.current.streamTimeoutId = undefined;
        console.log('🧹 [监听器清理] 清理超时定时器');
      }
    };
  }, [
    currentStreamId, 
    isGeneratingSummary,
    // 🎯 修复：错题详情页面不再依赖chatHistory.length，但回顾分析页面需要依赖以确保事件名称切换
    // NEW_MISTAKE_ANALYSIS和REVIEW_SESSION_DETAIL需要基于chatHistory.length判断事件类型
    ...(props.mode === 'NEW_MISTAKE_ANALYSIS' || props.mode === 'EXISTING_BATCH_TASK_DETAIL' || props.mode === 'REVIEW_SESSION_DETAIL'
       ? [chatHistory.length] 
       : [])
  ]); // 🎯 REVIEW_SESSION_DETAIL重新加入chatHistory.length依赖，确保初次分析->追问时监听器重设

  // 处理总结生成请求 - 使用聊天API但添加特殊标识区分总结内容
  const handleGenerateSummary = useCallback(async (summaryPrompt: string) => {
    console.log('🔍 [总结生成] 检查streamId状态:', { 
      currentStreamId, 
      mode: props.mode, 
      businessSessionId: props.businessSessionId 
    });
    
    if (!currentStreamId) {
      console.error('❌ 无法生成总结：缺少streamId');
      return;
    }

    // 重置总结状态
    console.log('📝 [总结生成] 开始重置状态');
    setSummaryStreamContent('');
    setSummaryStreamComplete(false);
    setIsGeneratingSummary(true);
    console.log('📝 [总结生成] 状态重置完成，isGeneratingSummary=true');

    // 在总结提示词中添加特殊标识
    const specialSummaryPrompt = `[SUMMARY_REQUEST] ${summaryPrompt}`;
    
    // 创建总结请求消息（仅用于内部处理，不显示在聊天记录中）
    const summaryRequestMessage: ChatMessage = {
      role: 'user',
      content: specialSummaryPrompt,
      timestamp: new Date().toISOString(),
    };

    try {
      console.log('📝 开始流式生成总结（带特殊标识）...');
      
      // 调用后端API - 使用临时的聊天历史，不保存总结请求到正式记录
      console.log(`📡 准备调用 continueUserChat 生成总结，使用的 streamId: ${currentStreamId}`);
      console.log('🚫 [总结请求] 使用临时聊天历史，不会保存到数据库');
      
      // 创建临时聊天历史用于总结生成，但不影响实际的聊天记录
      const tempChatHistory = [...chatHistory, summaryRequestMessage];
      
      console.log('📡 [总结生成] 当前事件监听器状态:', {
        currentStreamId,
        chatHistoryLength: chatHistory.length,
        tempChatHistoryLength: tempChatHistory.length,
        currentEventType: eventListenersRef.current.lastEventType,
        isGeneratingSummary: isGeneratingSummaryRef.current,
        mode: props.mode,
        businessSessionId: props.businessSessionId,
        expectedEventName: `continue_chat_stream_${currentStreamId}`
      });
      
      await props.serviceConfig.apiProvider.continueUserChat({
        streamIdForEvents: currentStreamId,
        businessId: props.businessSessionId,
        fullChatHistory: tempChatHistory, // 使用临时历史，包含总结请求
        enableChainOfThought: false, // 总结不需要思维链
        enableRag: false, // 总结不需要RAG
        ragTopK: 0
      });
        
    } catch (error) {
      console.error('❌ 总结生成失败:', error);
      alert('总结生成失败: ' + error);
      setIsGeneratingSummary(false);
      setSummaryStreamComplete(true);
    }
  }, [currentStreamId, props.serviceConfig.apiProvider, props.businessSessionId, chatHistory]);

  // 发送聊天消息 - 完全重写，修复所有流式问题
  const handleSendMessage = async () => {
    if (!currentMessage.trim() || !currentStreamId) return;

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
        const newStreamingIndex = streamingHistory.length - 1;
        
        // 🎯 关键修复：先更新聊天历史，再设置streamingMessageIndex
        setChatHistory(streamingHistory);
        
        // 🎯 延迟设置streamingMessageIndex，确保聊天历史已经更新
        setTimeout(() => {
          console.log(`🎯 [追问修复] 设置streamingMessageIndex: ${newStreamingIndex}, 聊天历史长度: ${streamingHistory.length}`);
          console.log(`🎯 [追问修复] 目标消息验证:`, {
            targetMessage: streamingHistory[newStreamingIndex],
            role: streamingHistory[newStreamingIndex]?.role,
            isAssistant: streamingHistory[newStreamingIndex]?.role === 'assistant'
          });
          setStreamingMessageIndex(newStreamingIndex);
        }, 10); // 短暂延迟，让状态更新完成
        
        // 调用后端API
        console.log(`📡 准备调用 continueUserChat，使用的 streamId: ${currentStreamId}`);
        console.log(`📡 [追问调试] 当前状态:`, {
          newStreamingIndex: newStreamingIndex,
          chatHistoryLength: newChatHistory.length,
          fullHistoryLength: streamingHistory.length,
          currentStreamId,
          businessId: props.businessSessionId,
          // 🎯 新增：追问时的关键状态
          autoStartExecuted: autoStartExecutedRef.current,
          currentEventListenerKey: eventListenersRef.current.currentStreamId,
          expectedEventName: `review_chat_stream_${currentStreamId}`,
          mode: props.mode
        });
        
        // 🎯 新增：确保在发送请求前监听器已正确设置
        console.log(`🎧 [追问前检查] 监听器状态:`, {
          hasContentListener: !!eventListenersRef.current.unlistenContent,
          currentListenerKey: eventListenersRef.current.currentStreamId,
          expectedListenerKey: `${currentStreamId}_${isGeneratingSummary ? 'summary' : 'normal'}`
        });
        
        await props.serviceConfig.apiProvider.continueUserChat({
          streamIdForEvents: currentStreamId,
          businessId: props.businessSessionId,
          fullChatHistory: newChatHistory,
          enableChainOfThought,
          enableRag,
          ragTopK
        });
        
        console.log(`✅ [追问调试] continueUserChat 调用完成`);
        
        // 🎯 新增：设置超时机制，防止流式事件缺失导致永久等待
        const streamTimeoutId = setTimeout(() => {
          if (isChatting && streamingMessageIndex !== null) {
            console.warn('⚠️ [流式超时] 10秒内未收到流式事件，强制清理状态');
            setIsChatting(false);
            setStreamingMessageIndex(null);
            setIsInputAllowed(true);
          }
        }, 10000); // 10秒超时
        
        // 将超时ID存储到ref中，以便在事件处理中清理
        eventListenersRef.current.streamTimeoutId = streamTimeoutId;
      
        
      } else {
        // 传统非流式对话暂不支持
        throw new Error('传统对话模式暂不支持，请使用流式模式');
      }
    } catch (error) {
      console.error('❌ 对话失败:', error);
      alert('对话失败: ' + error);
      setStreamingMessageIndex(null);
      setIsChatting(false);
      setIsInputAllowed(true);
      
      // 🎯 清理流式超时定时器
      if (eventListenersRef.current.streamTimeoutId) {
        clearTimeout(eventListenersRef.current.streamTimeoutId);
        eventListenersRef.current.streamTimeoutId = undefined;
        console.log('🧹 [错误处理] 清理超时定时器');
      }
    } finally {
      // isChatting 状态由事件监听器在流结束后设置，或者在错误/超时时手动设置
    }
  };

  // 保存到错题库
  const handleSaveToLibrary = async () => {
    try {
      // 🎯 修复：正确处理思维链数据的保存
      const chatHistoryWithThinking = chatHistory.map((message, index) => {
        // 如果是assistant消息且有思维链，则添加thinking_content字段
        if (message.role === 'assistant' && thinkingContent.has(index)) {
          const thinkingText = thinkingContent.get(index);
          console.log(`💾 [保存] 为索引${index}的assistant消息添加思维链，长度:`, thinkingText?.length || 0);
          return {
            ...message,
            thinking_content: thinkingText
          };
        }
        return message;
      });
      
      console.log('💾 [保存] 处理后的聊天历史，包含思维链的消息数:', 
        chatHistoryWithThinking.filter(msg => msg.thinking_content).length);
      
      console.log('💾 准备保存数据:', {
        businessSessionId: props.businessSessionId,
        chatHistoryLength: chatHistoryWithThinking.length,
        thinkingContentSize: thinkingContent.size,
        ocrResult: ocrResult,
        temp_id: analysisResult?.temp_id || currentStreamId,
        originalInputs: { subject, userQuestion, questionImagesCount: questionImages.length }
      });
      
      // 确保传递正确的temp_id
      const temp_id = analysisResult?.temp_id || currentStreamId;
      console.log('💾 UniversalAppChatHost传递的temp_id:', temp_id);
      
      await props.onSaveRequest({
        businessSessionId: props.businessSessionId,
        chatHistory: chatHistoryWithThinking,
        thinkingContent,
        ocrResult: ocrResult || (analysisResult ? {
          ocr_text: analysisResult.initial_data.ocr_text,
          tags: analysisResult.initial_data.tags,
          mistake_type: analysisResult.initial_data.mistake_type,
          temp_id: temp_id === null ? undefined : temp_id
        } : null),
        temp_id: temp_id === null ? undefined : temp_id,
        // 传递总结内容
        summaryContent: summaryStreamContent && summaryStreamComplete ? summaryStreamContent : undefined,
        originalInputs: {
          subject,
          userQuestion,
          questionImages
        }
      });
      
      // 只在NEW_MISTAKE_ANALYSIS模式下重置状态
      if (props.mode === 'NEW_MISTAKE_ANALYSIS') {
        handleReset();
      }
    } catch (error) {
      console.error('保存失败:', error);
      alert('保存失败: ' + error);
    }
  };

  // 重置分析
  const handleReset = () => {
    // 清理timeout
    if (updateTimeoutRef.current) {
      clearTimeout(updateTimeoutRef.current);
      updateTimeoutRef.current = null;
    }
    
    setAnalysisResult(null);
    setChatHistory([]);
    setCurrentMessage('');
    setStreamingMessageIndex(null);
    setOcrResult(null);
    setIsOcrComplete(false);
    setIsOcrInProgress(false);
    setIsAnalyzing(false);
    setUserQuestion('');
    setQuestionImages([]);
    
    // 🎯 重要：重置总结状态
    setSummaryStreamContent('');
    setSummaryStreamComplete(false);
    setIsGeneratingSummary(false);
    console.log('🔄 [重置分析] 总结状态已重置');
    setThinkingContent(new Map());
    setIsInputAllowed(false);
  };

  // 分库接口类型
  interface SubLibrary {
    id: string;
    name: string;
    description?: string;
    created_at: string;
    updated_at: string;
    document_count: number;
    chunk_count: number;
  }

  // 分库选择相关状态
  const [subLibraries, setSubLibraries] = useState<SubLibrary[]>([]);
  const [selectedLibraries, setSelectedLibraries] = useState<string[]>([]);
  const [showLibrarySelector, setShowLibrarySelector] = useState(false);
  const librarySelectorRef = useRef<HTMLDivElement>(null);

  // 加载分库列表
  const loadSubLibraries = useCallback(async () => {
    try {
      const libraries = await TauriAPI.getRagSubLibraries();
      setSubLibraries(libraries);
      console.log('🎯 加载分库列表完成，数量:', libraries.length);
    } catch (error) {
      console.error('加载分库列表失败:', error);
    }
  }, []); // 移除selectedLibraries.length依赖，避免循环触发

  // 切换分库选择
  const toggleLibrarySelection = (libraryId: string) => {
    console.log('🎯 切换知识库选择:', libraryId);
    setSelectedLibraries(prev => {
      const newSelection = prev.includes(libraryId)
        ? prev.filter(id => id !== libraryId)
        : [...prev, libraryId];
      console.log('🎯 知识库选择更新:', prev, '->', newSelection);
      return newSelection;
    });
  };

  // 全选/取消全选分库
  const toggleAllLibraries = () => {
    if (selectedLibraries.length === subLibraries.length) {
      // 当前全选状态，执行取消全选
      setSelectedLibraries([]);
    } else {
      // 当前非全选状态，执行全选
      setSelectedLibraries(subLibraries.map(lib => lib.id));
    }
  };

  // 处理点击外部关闭分库选择器
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (librarySelectorRef.current && !librarySelectorRef.current.contains(event.target as Node)) {
        setShowLibrarySelector(false);
      }
    };

    if (showLibrarySelector) {
      document.addEventListener('mousedown', handleClickOutside);
    }
    
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
    };
  }, [showLibrarySelector]);

  // 组件挂载时加载分库列表
  useEffect(() => {
    if (enableRag) {
      loadSubLibraries();
    }
  }, [enableRag, loadSubLibraries]);

  // 初始化默认选中的分库 - 只在首次加载分库时执行
  useEffect(() => {
    if (subLibraries.length > 0 && selectedLibraries.length === 0) {
      // 优先使用props中指定的默认分库
      if (props.serviceConfig.defaultSelectedLibraries && props.serviceConfig.defaultSelectedLibraries.length > 0) {
        console.log('🎯 使用props指定的默认分库');
        setSelectedLibraries(props.serviceConfig.defaultSelectedLibraries);
      } else {
        // 否则默认选择所有分库（仅在初始化时）
        console.log('🎯 初始化时默认选择所有分库');
        setSelectedLibraries(subLibraries.map(lib => lib.id));
      }
    }
  }, [subLibraries.length]); // 只在分库列表变化时触发，不依赖selectedLibraries

  // 处理props传入的默认选中分库（用于组件重新加载的情况）
  useEffect(() => {
    if (props.serviceConfig.defaultSelectedLibraries && props.serviceConfig.defaultSelectedLibraries.length > 0) {
      console.log('🎯 检测到props中的默认分库设置');
      setSelectedLibraries(props.serviceConfig.defaultSelectedLibraries);
    }
  }, [props.serviceConfig.defaultSelectedLibraries]);

  // 渲染分析界面 - 左右分栏布局
  const renderAnalysisView = () => (
    <div className="analysis-layout">
      {/* 左侧上传栏 */}
      <div className="left-panel">
        <div className="cherry-app">
          <div className="app-header">
            <div className="app-title">
              <img src="/dslogo1.png" alt="DeepStudent 深度学者" className="app-logo" />
            </div>
            <p className="app-subtitle">
              {props.mode === 'NEW_MISTAKE_ANALYSIS' ? '上传多张题目图片获取AI增强解析' : '查看历史分析并进行深度追问'}
            </p>
          </div>

          {/* 科目显示 - 只读 */}
          <div className="readonly-subject-display">
            <span className="subject-badge">{subject || '未选择科目'}</span>
          </div>

          {/* 图片上传区域 - 完全按照HTML模板 */}
          {props.mode === 'NEW_MISTAKE_ANALYSIS' ? (
            <div 
              className={`upload-card ${questionImages.length > 0 ? 'has-files' : ''}`}
              id="uploadArea"
              onClick={handleFileUploadClick}
            >
              <div className="upload-icon">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <path d="M18 15V18H6V15H4V18C4 19.1 4.9 20 6 20H18C19.1 20 20 19.1 20 18V15H18Z" fill="currentColor"/>
                  <path d="M12 5L7 10H10V14H14V10H17L12 5Z" fill="currentColor"/>
                </svg>
              </div>
              <p className="upload-text">
                {questionImages.length > 0 ? '已添加题目图片' : '拖放题目图片到此处'}
              </p>
              <p className="upload-hint">
                {questionImages.length > 0 ? '可继续添加或拖放更多图片' : '或点击选择文件 (最多9张JPG/PNG)'}
              </p>
              {questionImages.length > 0 && (
                <div className="counter-badge">{questionImages.length}/9</div>
              )}
              <input
                type="file"
                multiple
                accept="image/*"
                onChange={handleImageUpload}
                className="file-input"
                id="fileInput"
              />
            </div>
          ) : (
            // 错题详情模式：显示现有图片
            <div className="upload-card readonly-mode">
              <div className="upload-icon">
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                  <path d="M18 15V18H6V15H4V18C4 19.1 4.9 20 6 20H18C19.1 20 20 19.1 20 18V15H18Z" fill="currentColor"/>
                  <path d="M12 5L7 10H10V14H14V10H17L12 5Z" fill="currentColor"/>
                </svg>
              </div>
              <p className="upload-text">题目图片</p>
              <p className="upload-hint">点击下方图片查看大图</p>
            </div>
          )}

          {/* 图片预览列表 */}
          <div className="preview-list" id="previewList">
            {(() => {
              const imagesToDisplay = props.mode === 'NEW_MISTAKE_ANALYSIS' 
                ? questionImageUrls 
                : (questionImageUrls.length > 0 ? questionImageUrls : (props.preloadedData?.questionImageUrls || []));
              
              return imagesToDisplay.map((url, index) => (
                <div key={index} className="preview-item">
                  <img
                    src={url}
                    alt={`题目图片 ${index + 1}`}
                    className="preview-image"
                    onClick={() => openImageViewer(index)}
                  />
                  {props.mode === 'NEW_MISTAKE_ANALYSIS' && (
                    <div 
                      className="preview-remove"
                      onClick={(e) => {
                        e.stopPropagation();
                        removeImage(index);
                      }}
                    >
                      <svg viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                        <path d="M18 6L6 18" stroke="white" strokeWidth="2" strokeLinecap="round"/>
                        <path d="M6 6L18 18" stroke="white" strokeWidth="2" strokeLinecap="round"/>
                      </svg>
                    </div>
                  )}
                </div>
              ));
            })()}
          </div>

          {/* 问题描述输入 */}
          <div className="input-container">
            <label className="input-label">问题描述 (可选)</label>
            {props.mode === 'NEW_MISTAKE_ANALYSIS' ? (
              <textarea
                className="cherry-textarea"
                value={userQuestion}
                onChange={(e) => setUserQuestion(e.target.value)}
                placeholder="例如：我需要这道微积分题的详细解题步骤..."
              />
            ) : (
              <div className="cherry-textarea readonly">
                {userQuestion || props.preloadedData?.userQuestion || '无问题描述'}
              </div>
            )}
          </div>

          {/* RAG开关 - 按照HTML模板 */}
          {props.mode === 'NEW_MISTAKE_ANALYSIS' && (
            <>
              <div className="rag-controls-container">
                <div className="rag-toggle" id="ragToggle" onClick={() => setEnableRag(!enableRag)}>
                  <input 
                    type="checkbox" 
                    id="ragCheckbox"
                    checked={enableRag}
                    readOnly
                  />
                  <span className="rag-switch">
                    <span className="rag-slider"></span>
                  </span>
                  <span className="rag-label">启用RAG知识库增强</span>
                </div>
                
                {/* 分库选择下拉框 */}
                {enableRag && (
                  <div className="rag-library-selector" ref={librarySelectorRef}>
                    <div 
                      className="library-selector-trigger"
                      onClick={() => setShowLibrarySelector(!showLibrarySelector)}
                    >
                      <span className="library-selector-label">
                        知识库范围 ({selectedLibraries.length}/{subLibraries.length})
                      </span>
                      <span className="library-selector-arrow">
                        {showLibrarySelector ? '▲' : '▼'}
                      </span>
                    </div>
                    
                    {showLibrarySelector && (
                      <div className="library-selector-dropdown">
                        <div className="library-selector-header">
                          <button 
                            className="library-selector-action"
                            onClick={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              toggleAllLibraries();
                            }}
                          >
                            {selectedLibraries.length === subLibraries.length ? '取消全选' : '全选'}
                          </button>
                        </div>
                        <div className="library-selector-list">
                          {subLibraries.length > 0 ? (
                            subLibraries.map(library => (
                              <div 
                                key={library.id}
                                className={`library-selector-item ${selectedLibraries.includes(library.id) ? 'selected' : ''}`}
                                onClick={(e) => {
                                  // 如果点击的是checkbox或其容器，不处理外层点击
                                  if ((e.target as HTMLElement).closest('.library-checkbox')) {
                                    console.log('🎯 点击了checkbox区域，跳过外层处理');
                                    return;
                                  }
                                  console.log('🎯 点击了知识库项目外层区域:', library.id);
                                  e.preventDefault();
                                  e.stopPropagation();
                                  toggleLibrarySelection(library.id);
                                }}
                              >
                                <div className="library-checkbox" onClick={(e) => e.stopPropagation()}>
                                  <input 
                                    type="checkbox" 
                                    checked={selectedLibraries.includes(library.id)}
                                    onChange={(e) => {
                                      console.log('🎯 checkbox onChange事件:', library.id);
                                      e.preventDefault();
                                      e.stopPropagation();
                                      toggleLibrarySelection(library.id);
                                    }}
                                  />
                                </div>
                                <div className="library-info">
                                  <div className="library-name">{library.name}</div>
                                  <div className="library-stats">
                                    📄 {library.document_count} | 📝 {library.chunk_count}
                                  </div>
                                </div>
                              </div>
                            ))
                          ) : (
                            <div className="library-selector-empty">
                              <div style={{ padding: '20px', textAlign: 'center', color: 'var(--cherry-text-secondary)', fontSize: '13px' }}>
                                暂无可用的知识库分库
                              </div>
                            </div>
                          )}
                        </div>
                      </div>
                    )}
                  </div>
                )}
              </div>
              <div className="rag-hint">
                {enableRag 
                  ? `连接RAG知识库获取更精准的解析 (已选择 ${selectedLibraries.length} 个分库)`
                  : '连接RAG知识库获取更精准的解析'
                }
              </div>
            </>
          )}

          {/* 主要操作按钮 */}
          {props.mode === 'NEW_MISTAKE_ANALYSIS' ? (
            <button
              onClick={handleAnalyze}
              disabled={isOcrInProgress || isAnalyzing}
              className="analyze-btn"
            >
              <svg className="btn-icon" viewBox="0 0 24 24" fill="none" xmlns="http://www.w3.org/2000/svg">
                <path d="M20 7H4V5H20V7Z" fill="#111"/>
                <path d="M20 11H4V9H20V11Z" fill="#111"/>
                <path d="M4 15H20V13H4V15Z" fill="#111"/>
                <path d="M4 19H20V17H4V19Z" fill="#111"/>
              </svg>
              {isOcrInProgress ? 'OCR进行中...' : isAnalyzing ? '深度分析中...' : '开始深度分析'}
            </button>
          ) : (
            <button
              onClick={props.onExitRequest}
              className="analyze-btn back-mode"
            >
              ← 返回列表
            </button>
          )}

          {/* 次要操作按钮 */}
          {analysisResult && props.mode === 'NEW_MISTAKE_ANALYSIS' && (
            <div className="secondary-buttons">
              <button onClick={handleSaveToLibrary} className="save-btn-secondary">
                💾 保存到错题库
              </button>
              <button onClick={handleReset} className="reset-btn-secondary">
                🔄 重新分析
              </button>
            </div>
          )}
        </div>
      </div>

      {/* 右侧结果栏 */}
      <div className="right-panel">
        {!ocrResult && !analysisResult && props.mode === 'NEW_MISTAKE_ANALYSIS' ? (
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
                
                <div className="chat-history">
                  {chatHistory.map((message, index) => {
                    const isStreaming = streamingMessageIndex === index;
                    const thinking = thinkingContent.get(index);
                    
                    // 检查是否为最后一条AI消息
                    const isLastAssistantMessage = message.role === 'assistant' && 
                      index === chatHistory.length - 1 && 
                      chatHistory.slice(index + 1).every(m => m.role !== 'assistant');
                    
                    // 调试：总结框显示条件和聊天历史
                    if (message.role === 'assistant') {
                      console.log(`🎯 [总结框] 检查消息 ${index}:`, {
                        isLastAssistantMessage,
                        messageRole: message.role,
                        messageIndex: index,
                        chatHistoryLength: chatHistory.length,
                        isLastIndex: index === chatHistory.length - 1,
                        messageContent: typeof message.content === 'string' ? message.content.substring(0, 50) + '...' : '[多媒体内容]',
                        mode: props.mode,
                        summaryStreamContent: summaryStreamContent.substring(0, 50) + '...',
                        summaryStreamComplete
                      });
                    }
                    return (
                      <MessageWithThinking
                        key={index}
                        content={message.content}
                        thinkingContent={thinking}
                        isStreaming={isStreaming}
                        role={message.role as 'user' | 'assistant'}
                        timestamp={message.timestamp}
                        ragSources={message.rag_sources}
                        showSummaryBox={isLastAssistantMessage && props.mode !== 'REVIEW_SESSION_DETAIL'}
                        chatHistory={chatHistory}
                        subject={props.preloadedData?.subject}
                        mistakeId={props.mode === 'EXISTING_MISTAKE_DETAIL' ? props.businessSessionId : 
                                  props.mode === 'NEW_MISTAKE_ANALYSIS' ? 'new_analysis' : undefined}
                        reviewSessionId={props.mode === 'REVIEW_SESSION_DETAIL' ? props.businessSessionId : undefined}
                        onGenerateSummary={(prompt) => handleGenerateSummary(prompt)}
                        currentStreamId={currentStreamId ?? undefined}
                        isGenerating={isGeneratingSummary}
                        summaryStreamContent={summaryStreamContent}
                        summaryStreamComplete={summaryStreamComplete}
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

                {/* 在OCR开始后就显示输入框，但在分析过程中禁用发送功能 */}
                {(analysisResult || isAnalyzing || isOcrInProgress) && (
                  <div className="chat-input">
                    <input
                      type="text"
                      value={currentMessage}
                      onChange={(e) => setCurrentMessage(e.target.value)}
                      placeholder={isOcrInProgress ? "OCR中，请等待..." : isAnalyzing ? "分析中，请等待..." : "继续提问..."}
                      onKeyDown={(e) => e.key === 'Enter' && !isOcrInProgress && !isAnalyzing && handleSendMessage()}
                      disabled={isChatting || isOcrInProgress || isAnalyzing}
                    />
                    <button
                      onClick={handleSendMessage}
                      disabled={isChatting || !currentMessage.trim() || isOcrInProgress || isAnalyzing}
                      className="send-button"
                    >
                      {isChatting ? '⏳' : (isOcrInProgress || isAnalyzing) ? '⏸️' : '📤'}
                    </button>
                    <button 
                      className="chat-fullscreen-toggle"
                      onClick={handleChatFullscreenToggle}
                      title={isChatFullscreen ? '退出全屏' : '全屏模式'}
                    >
                      {isChatFullscreen ? '🔲' : '📱'}
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
    <>
      {/* 始终渲染分析界面 */}
      {renderAnalysisView()}

      {/* 图片查看器 */}
      <ImageViewer
        images={questionImageUrls}
        currentIndex={currentImageIndex}
        isOpen={imageViewerOpen}
        onClose={() => setImageViewerOpen(false)}
        onNext={() => setCurrentImageIndex(prev => (prev + 1) % questionImageUrls.length)}
        onPrev={() => setCurrentImageIndex(prev => (prev - 1 + questionImageUrls.length) % questionImageUrls.length)}
      />
    </>
  );
}

export default UniversalAppChatHost;
