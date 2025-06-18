// Tauri API调用模块 - 真实的后端API调用
import { invoke } from '@tauri-apps/api/core';
import { MistakeItem, ChatMessage, RagSourceInfo } from '../types';

// 重新导出类型以保持兼容性
export type { MistakeItem, ChatMessage, RagSourceInfo };

// 回顾分析相关类型 - 复用错题分析结构
export interface ReviewAnalysisItem {
  id: string;
  name: string;
  subject: string;
  created_at: string;
  updated_at: string;
  mistake_ids: string[];
  consolidated_input: string;
  user_question: string;
  status: string;
  tags: string[];
  analysis_type: string;
  chat_history: ChatMessage[];
}

export interface AnalysisRequest {
  subject: string;
  question_image_files: string[]; // Base64编码的图片字符串
  analysis_image_files: string[]; // Base64编码的图片字符串
  user_question: string;
}

export interface AnalysisResponse {
  temp_id: string;
  initial_data: {
    ocr_text: string;
    tags: string[];
    mistake_type: string;
    first_answer: string;
  };
}

export interface ContinueChatRequest {
  temp_id: string;
  chat_history: ChatMessage[];
}

export interface ContinueChatResponse {
  new_assistant_message: string;
}

export interface SaveMistakeRequest {
  temp_id: string;
  final_chat_history: ChatMessage[];
}

export interface SaveMistakeResponse {
  success: boolean;
  final_mistake_item?: MistakeItem;
}

// 工具函数：将File对象转换为Base64字符串
export const fileToBase64 = (file: File): Promise<string> => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => {
      const result = reader.result as string;
      // 移除 "data:image/jpeg;base64," 前缀，只保留Base64数据
      const base64Data = result.split(',')[1];
      resolve(base64Data);
    };
    reader.onerror = error => reject(error);
  });
};

// 工具函数：批量转换文件为Base64
export const filesToBase64 = async (files: File[]): Promise<string[]> => {
  const promises = files.map(file => fileToBase64(file));
  return Promise.all(promises);
};

// Tauri API调用类
export class TauriAPI {
  // 错题分析相关API
  static async analyzeNewMistake(request: {
    subject: string;
    question_image_files: File[];
    analysis_image_files: File[];
    user_question: string;
  }): Promise<AnalysisResponse> {
    try {
      // 转换图片文件为Base64
      const questionImageBase64 = await filesToBase64(request.question_image_files);
      const analysisImageBase64 = await filesToBase64(request.analysis_image_files);

      // 根据后端AnalysisRequest结构，需要传递request对象
      const analysisRequest = {
        subject: request.subject,
        question_image_files: questionImageBase64,
        analysis_image_files: analysisImageBase64,
        user_question: request.user_question,
      };
      
      const response = await invoke<AnalysisResponse>('analyze_new_mistake', {
        request: analysisRequest,
      });
      return response;
    } catch (error) {
      console.error('分析新错题失败:', error);
      throw new Error(`分析失败: ${error}`);
    }
  }

  // 分析新错题 - 流式版本
  static async analyzeNewMistakeStream(request: {
    subject: string;
    question_image_files: File[];
    analysis_image_files: File[];
    user_question: string;
  }): Promise<AnalysisResponse> {
    try {
      // 转换图片文件为Base64
      const questionImageBase64 = await filesToBase64(request.question_image_files);
      const analysisImageBase64 = await filesToBase64(request.analysis_image_files);

      // 根据后端AnalysisRequest结构，需要传递request对象
      const analysisRequest = {
        subject: request.subject,
        question_image_files: questionImageBase64,
        analysis_image_files: analysisImageBase64,
        user_question: request.user_question,
      };
      
      const response = await invoke<AnalysisResponse>('analyze_new_mistake_stream', {
        request: analysisRequest,
      });
      return response;
    } catch (error) {
      console.error('流式分析新错题失败:', error);
      throw new Error(`流式分析失败: ${error}`);
    }
  }

  // 分步骤分析：先OCR，再流式AI解答
  static async analyzeStepByStep(request: {
    subject: string;
    question_image_files: File[];
    analysis_image_files: File[];
    user_question: string;
    enable_chain_of_thought?: boolean;
  }): Promise<{
    temp_id: string;
    ocr_result: {
      ocr_text: string;
      tags: string[];
      mistake_type: string;
    };
  }> {
    try {
      // 转换图片文件为Base64
      const questionImageBase64 = await filesToBase64(request.question_image_files);
      const analysisImageBase64 = await filesToBase64(request.analysis_image_files);

      // 调用后端的分步骤分析命令
      const analysisRequest = {
        subject: request.subject,
        question_image_files: questionImageBase64,
        analysis_image_files: analysisImageBase64,
        user_question: request.user_question,
        enable_chain_of_thought: request.enable_chain_of_thought || false,
      };
      
      const response = await invoke<{
        temp_id: string;
        ocr_result: {
          ocr_text: string;
          tags: string[];
          mistake_type: string;
        };
      }>('analyze_step_by_step', {
        request: analysisRequest,
      });
      return response;
    } catch (error) {
      console.error('分步骤分析失败:', error);
      
      // 更好的错误处理
      if (error && typeof error === 'object') {
        const errorMessage = (error as any).message || JSON.stringify(error);
        throw new Error(`分步骤分析失败: ${errorMessage}`);
      } else {
        throw new Error(`分步骤分析失败: ${String(error)}`);
      }
    }
  }

  // 开始流式AI解答（在OCR完成后调用）
  static async startStreamingAnswer(tempId: string, enableChainOfThought: boolean = false): Promise<void> {
    try {
      await invoke('start_streaming_answer', {
        request: {
          temp_id: tempId,
          enable_chain_of_thought: enableChainOfThought,
        },
      });
    } catch (error) {
      console.error('开始流式解答失败:', error);
      throw new Error(`开始流式解答失败: ${error}`);
    }
  }

  static async continueChat(request: ContinueChatRequest): Promise<ContinueChatResponse> {
    try {
      const response = await invoke<ContinueChatResponse>('continue_chat', {
        request: request,
      });
      return response;
    } catch (error) {
      console.error('继续对话失败:', error);
      throw new Error(`对话失败: ${error}`);
    }
  }

  // 继续对话 - 流式版本
  static async continueChatStream(request: {
    temp_id: string;
    chat_history: ChatMessage[];
    enable_chain_of_thought?: boolean;
  }): Promise<ContinueChatResponse> {
    try {
      const response = await invoke<ContinueChatResponse>('continue_chat_stream', {
        request: {
          temp_id: request.temp_id,
          chat_history: request.chat_history,
          enable_chain_of_thought: request.enable_chain_of_thought || false,
        },
      });
      return response;
    } catch (error) {
      console.error('流式继续对话失败:', error);
      throw new Error(`流式对话失败: ${error}`);
    }
  }

  static async saveMistakeFromAnalysis(request: SaveMistakeRequest): Promise<SaveMistakeResponse> {
    try {
      const response = await invoke<SaveMistakeResponse>('save_mistake_from_analysis', {
        request: request,
      });
      return response;
    } catch (error) {
      console.error('保存错题失败:', error);
      let errorMessage = '未知错误';
      if (error && typeof error === 'object') {
        if ('message' in error) {
          errorMessage = (error as any).message;
        } else {
          errorMessage = JSON.stringify(error);
        }
      } else if (typeof error === 'string') {
        errorMessage = error;
      }
      throw new Error(`保存失败: ${errorMessage}`);
    }
  }

  // 错题库管理API
  static async getMistakes(filter?: {
    subject?: string;
    mistake_type?: string;
    tags?: string[];
  }): Promise<MistakeItem[]> {
    try {
      const response = await invoke<MistakeItem[]>('get_mistakes', {
        subject: filter?.subject || null,
        mistake_type: filter?.mistake_type || null,
        tags: filter?.tags || null,
      });
      return response;
    } catch (error) {
      console.error('获取错题列表失败:', error);
      throw new Error(`获取错题列表失败: ${error}`);
    }
  }

  // 回顾分析列表API（复用错题分析的列表模式）
  static async getReviewAnalyses(filter?: {
    subject?: string;
    status?: string;
  }): Promise<ReviewAnalysisItem[]> {
    try {
      const response = await invoke<ReviewAnalysisItem[]>('get_review_analyses', {
        subject: filter?.subject || null,
        status: filter?.status || null,
      });
      return response;
    } catch (error) {
      console.error('获取回顾分析列表失败:', error);
      throw new Error(`获取回顾分析列表失败: ${error}`);
    }
  }

  static async getMistakeDetails(id: string): Promise<MistakeItem | null> {
    try {
      const response = await invoke<MistakeItem | null>('get_mistake_details', { id });
      return response;
    } catch (error) {
      console.error('获取错题详情失败:', error);
      throw new Error(`获取错题详情失败: ${error}`);
    }
  }

  static async updateMistake(mistake: MistakeItem): Promise<MistakeItem> {
    try {
      const response = await invoke<MistakeItem>('update_mistake', { mistake });
      return response;
    } catch (error) {
      console.error('更新错题失败:', error);
      throw new Error(`更新错题失败: ${error}`);
    }
  }

  static async deleteMistake(id: string): Promise<boolean> {
    try {
      const response = await invoke<boolean>('delete_mistake', { id });
      return response;
    } catch (error) {
      console.error('删除错题失败:', error);
      throw new Error(`删除错题失败: ${error}`);
    }
  }

  static async getMistakeQuestionImages(mistakeId: string): Promise<string[]> {
    try {
      const mistake = await this.getMistakeDetails(mistakeId);
      if (!mistake) {
        return [];
      }
      
      const images: string[] = [];
      for (const imagePath of mistake.question_images) {
        try {
          const base64 = await this.getImageAsBase64(imagePath);
          images.push(base64);
        } catch (error) {
          console.warn(`获取题目图片失败: ${imagePath}`, error);
        }
      }
      return images;
    } catch (error) {
      console.error('获取错题题目图片失败:', error);
      throw new Error(`获取错题题目图片失败: ${error}`);
    }
  }

  static async getMistakeAnalysisImages(mistakeId: string): Promise<string[]> {
    try {
      const mistake = await this.getMistakeDetails(mistakeId);
      if (!mistake) {
        return [];
      }
      
      const images: string[] = [];
      for (const imagePath of mistake.analysis_images) {
        try {
          const base64 = await this.getImageAsBase64(imagePath);
          images.push(base64);
        } catch (error) {
          console.warn(`获取解析图片失败: ${imagePath}`, error);
        }
      }
      return images;
    } catch (error) {
      console.error('获取错题解析图片失败:', error);
      throw new Error(`获取错题解析图片失败: ${error}`);
    }
  }

  static async continueMistakeChat(mistakeId: string, chatHistory: ChatMessage[]): Promise<ContinueChatResponse> {
    try {
      const response = await invoke<ContinueChatResponse>('continue_mistake_chat', {
        mistakeId: mistakeId,
        chatHistory: chatHistory,
      });
      return response;
    } catch (error) {
      console.error('在错题详情页继续对话失败:', error);
      throw new Error(`对话失败: ${error}`);
    }
  }

  // 继续错题对话 - 流式版本
  static async continueMistakeChatStream(request: {
    mistakeId: string;
    chatHistory: ChatMessage[];
    enableChainOfThought?: boolean;
  }): Promise<void> {
    try {
      await invoke('continue_mistake_chat_stream', {
        mistakeId: request.mistakeId,
        chatHistory: request.chatHistory,
        enableChainOfThought: request.enableChainOfThought || false,
      });
    } catch (error) {
      console.error('流式错题对话失败:', error);
      throw new Error(`流式对话失败: ${error}`);
    }
  }

  // 回顾分析API (流式版本)
  static async analyzeReviewSession(subject: string, mistakeIds: string[]): Promise<AnalysisResponse> {
    try {
      const response = await invoke<AnalysisResponse>('analyze_review_session_stream', {
        subject,
        mistake_ids: mistakeIds,
      });
      return response;
    } catch (error) {
      console.error('回顾分析失败:', error);
      throw new Error(`回顾分析失败: ${error}`);
    }
  }

  // 配置管理API
  static async saveSetting(key: string, value: string): Promise<void> {
    try {
      await invoke<void>('save_setting', { key, value });
    } catch (error) {
      console.error('保存设置失败:', error);
      throw new Error(`保存设置失败: ${error}`);
    }
  }

  static async getSetting(key: string): Promise<string | null> {
    try {
      const response = await invoke<string | null>('get_setting', { key });
      return response;
    } catch (error) {
      console.error('获取设置失败:', error);
      throw new Error(`获取设置失败: ${error}`);
    }
  }

  static async testApiConnection(apiKey: string, apiBase: string, model?: string): Promise<boolean> {
    try {
      const response = await invoke<boolean>('test_api_connection', {
        api_key: apiKey,
        api_base: apiBase,
        model: model || null,
      });
      return response;
    } catch (error) {
      console.error('测试API连接失败:', error);
      throw new Error(`测试API连接失败: ${error}`);
    }
  }

  // 统计信息API
  static async getStatistics(): Promise<any> {
    try {
      const response = await invoke<any>('get_statistics');
      return response;
    } catch (error) {
      console.error('获取统计信息失败:', error);
      throw new Error(`获取统计信息失败: ${error}`);
    }
  }

  // 支持的科目API
  static async getSupportedSubjects(): Promise<string[]> {
    try {
      const response = await invoke<string[]>('get_supported_subjects');
      return response;
    } catch (error) {
      console.error('获取支持的科目失败:', error);
      throw new Error(`获取支持的科目失败: ${error}`);
    }
  }

  // 文件管理API
  static async getImageAsBase64(relativePath: string): Promise<string> {
    try {
      const response = await invoke<string>('get_image_as_base64', { relativePath: relativePath });
      return response;
    } catch (error) {
      console.error('获取图片Base64失败:', error);
      throw new Error(`获取图片失败: ${error}`);
    }
  }

  static async saveImageFromBase64(base64Data: string, originalPath: string): Promise<string> {
    try {
      // 从原路径中提取文件名或生成新的文件名
      const pathParts = originalPath.split('/');
      const fileName = pathParts[pathParts.length - 1];
      
      const response = await invoke<string>('save_image_from_base64_path', { 
        base64Data: base64Data,
        fileName: fileName
      });
      return response;
    } catch (error) {
      console.error('保存图片Base64失败:', error);
      throw new Error(`保存图片失败: ${error}`);
    }
  }

  static async cleanupOrphanedImages(): Promise<string[]> {
    try {
      const response = await invoke<string[]>('cleanup_orphaned_images');
      return response;
    } catch (error) {
      console.error('清理孤立图片失败:', error);
      throw new Error(`清理孤立图片失败: ${error}`);
    }
  }

  // API配置管理API
  static async getApiConfigurations(): Promise<any[]> {
    try {
      const response = await invoke<any[]>('get_api_configurations');
      return response;
    } catch (error) {
      console.error('获取API配置失败:', error);
      throw new Error(`获取API配置失败: ${error}`);
    }
  }

  static async saveApiConfigurations(configs: any[]): Promise<void> {
    try {
      await invoke<void>('save_api_configurations', { configs });
    } catch (error) {
      console.error('保存API配置失败:', error);
      throw new Error(`保存API配置失败: ${error}`);
    }
  }

  static async getModelAssignments(): Promise<any> {
    try {
      const response = await invoke<any>('get_model_assignments');
      return response;
    } catch (error) {
      console.error('获取模型分配失败:', error);
      throw new Error(`获取模型分配失败: ${error}`);
    }
  }

  static async saveModelAssignments(assignments: any): Promise<void> {
    try {
      await invoke<void>('save_model_assignments', { assignments });
    } catch (error) {
      console.error('保存模型分配失败:', error);
      throw new Error(`保存模型分配失败: ${error}`);
    }
  }

  // 科目配置管理API
  static async getAllSubjectConfigs(enabledOnly: boolean = false): Promise<any[]> {
    try {
      const response = await invoke<any[]>('get_all_subject_configs', { enabled_only: enabledOnly });
      return response;
    } catch (error) {
      console.error('获取科目配置列表失败:', error);
      throw new Error(`获取科目配置列表失败: ${error}`);
    }
  }

  static async getSubjectConfigById(id: string): Promise<any | null> {
    try {
      const response = await invoke<any | null>('get_subject_config_by_id', { id });
      return response;
    } catch (error) {
      console.error('获取科目配置失败:', error);
      throw new Error(`获取科目配置失败: ${error}`);
    }
  }

  static async getSubjectConfigByName(subjectName: string): Promise<any | null> {
    try {
      const response = await invoke<any | null>('get_subject_config_by_name', { subject_name: subjectName });
      return response;
    } catch (error) {
      console.error('获取科目配置失败:', error);
      throw new Error(`获取科目配置失败: ${error}`);
    }
  }

  static async createSubjectConfig(request: {
    subject_name: string;
    display_name: string;
    description?: string;
    prompts?: any;
    mistake_types?: string[];
    default_tags?: string[];
  }): Promise<any> {
    try {
      const response = await invoke<any>('create_subject_config', { request });
      return response;
    } catch (error) {
      console.error('创建科目配置失败:', error);
      throw new Error(`创建科目配置失败: ${error}`);
    }
  }

  static async updateSubjectConfig(request: {
    id: string;
    display_name?: string;
    description?: string;
    is_enabled?: boolean;
    prompts?: any;
    mistake_types?: string[];
    default_tags?: string[];
  }): Promise<any> {
    try {
      const response = await invoke<any>('update_subject_config', { request });
      return response;
    } catch (error) {
      console.error('更新科目配置失败:', error);
      throw new Error(`更新科目配置失败: ${error}`);
    }
  }

  static async deleteSubjectConfig(id: string): Promise<boolean> {
    try {
      const response = await invoke<boolean>('delete_subject_config', { id });
      return response;
    } catch (error) {
      console.error('删除科目配置失败:', error);
      throw new Error(`删除科目配置失败: ${error}`);
    }
  }

  static async initializeDefaultSubjectConfigs(): Promise<void> {
    try {
      await invoke<void>('initialize_default_subject_configs');
    } catch (error) {
      console.error('初始化默认科目配置失败:', error);
      throw new Error(`初始化默认科目配置失败: ${error}`);
    }
  }

  // 流式回顾分析API
  static async analyzeReviewSessionStream(subject: string, mistakeIds: string[]): Promise<{ review_id: string }> {
    try {
      const response = await invoke<{ review_id: string }>('analyze_review_session_stream', {
        subject,
        mistake_ids: mistakeIds
      });
      return response;
    } catch (error) {
      console.error('流式回顾分析失败:', error);
      throw new Error(`流式回顾分析失败: ${error}`);
    }
  }


  // 批量操作API
  static async batchSaveMistakes(mistakes: MistakeItem[]): Promise<void> {
    try {
      await invoke<void>('batch_save_mistakes', { mistakes });
    } catch (error) {
      console.error('批量保存错题失败:', error);
      throw new Error(`批量保存错题失败: ${error}`);
    }
  }

  static async batchDeleteMistakes(ids: string[]): Promise<void> {
    try {
      await invoke<void>('batch_delete_mistakes', { ids });
    } catch (error) {
      console.error('批量删除错题失败:', error);
      throw new Error(`批量删除错题失败: ${error}`);
    }
  }

  // ============================================================================
  // 回顾分析功能API
  // ============================================================================

  // 开始统一回顾分析 - 第一步：创建会话并缓存数据
  static async startConsolidatedReviewAnalysis(request: {
    subject: string;
    consolidatedInput: string;
    overallPrompt: string;
    enableChainOfThought: boolean;
    mistakeIds: string[]; // 🔧 修复：添加错题ID列表参数
  }): Promise<{ review_session_id: string }> {
    try {
      const response = await invoke<{ review_session_id: string }>('start_consolidated_review_analysis', {
        request: {
          subject: request.subject,
          consolidated_input: request.consolidatedInput,
          overall_prompt: request.overallPrompt,
          enable_chain_of_thought: request.enableChainOfThought,
          mistake_ids: request.mistakeIds, // 🔧 修复：传递错题ID列表
        }
      });
      return response;
    } catch (error) {
      console.error('开始统一回顾分析失败:', error);
      throw new Error(`开始统一回顾分析失败: ${error}`);
    }
  }

  // 触发统一回顾分析流式处理 - 第二步：开始AI分析
  static async triggerConsolidatedReviewStream(request: {
    review_session_id: string;
    enable_chain_of_thought: boolean;
    enable_rag?: boolean;
    rag_options?: { top_k: number };
  }): Promise<void> {
    try {
      await invoke<void>('trigger_consolidated_review_stream', {
        request: {
          review_session_id: request.review_session_id,
          enable_chain_of_thought: request.enable_chain_of_thought,
          enable_rag: request.enable_rag,
          rag_options: request.rag_options
        }
      });
    } catch (error) {
      console.error('触发统一回顾分析流式处理失败 (raw):', error);
      let errorMessage = '未知错误';
      if (error && typeof error === 'object') {
        errorMessage = (error as any).message || JSON.stringify(error);
      } else if (typeof error === 'string') {
        errorMessage = error;
      }
      throw new Error(`触发统一回顾分析流式处理失败: ${errorMessage}`);
    }
  }

  // 继续统一回顾分析对话
  static async continueConsolidatedReviewStream(request: {
    review_session_id: string;
    chat_history: ChatMessage[];
    enable_chain_of_thought: boolean;
    enable_rag?: boolean;
    rag_options?: { top_k: number };
  }): Promise<void> {
    try {
      await invoke<void>('continue_consolidated_review_stream', {
        request: {
          review_session_id: request.review_session_id,
          chat_history: request.chat_history,
          enable_chain_of_thought: request.enable_chain_of_thought,
          enable_rag: request.enable_rag,
          rag_options: request.rag_options
        }
      });
    } catch (error) {
      console.error('继续统一回顾分析对话失败 (raw):', error);
      let errorMessage = '未知错误';
      if (error && typeof error === 'object') {
        errorMessage = (error as any).message || JSON.stringify(error);
      } else if (typeof error === 'string') {
        errorMessage = error;
      }
      throw new Error(`继续统一回顾分析对话失败: ${errorMessage}`);
    }
  }

  // 获取统一回顾分析会话数据
  static async getConsolidatedReviewSession(sessionId: string): Promise<any | null> {
    try {
      const session = await invoke<any | null>('get_consolidated_review_session', {
        sessionId: sessionId,
      });
      return session;
    } catch (error) {
      console.error('获取统一回顾分析会话失败:', error);
      throw new Error(`获取统一回顾分析会话失败: ${error}`);
    }
  }

  // ============================================================================
  // RAG知识库管理API
  // ============================================================================

  // 添加文档到知识库（通过文件路径）
  static async ragAddDocuments(filePaths: string[]): Promise<string> {
    try {
      const response = await invoke<string>('rag_add_documents', {
        request: { 
          file_paths: filePaths,
          chunk_size: null,
          chunk_overlap: null,
          enable_preprocessing: null
        }
      });
      return response;
    } catch (error) {
      console.error('添加文档到知识库失败:', error);
      throw new Error(`添加文档到知识库失败: ${error}`);
    }
  }

  // 添加文档到知识库（通过文件内容）
  static async ragAddDocumentsFromContent(documents: Array<{
    fileName: string;
    content: string;
  }>): Promise<string> {
    try {
      const response = await invoke<string>('rag_add_documents_from_content', {
        documents
      });
      return response;
    } catch (error) {
      console.error('从内容添加文档到知识库失败:', error);
      throw new Error(`从内容添加文档到知识库失败: ${error}`);
    }
  }

  // 获取知识库状态
  static async ragGetKnowledgeBaseStatus(): Promise<{
    total_documents: number;
    total_chunks: number;
    embedding_model_name?: string;
    vector_store_type: string;
  }> {
    try {
      const response = await invoke<{
        total_documents: number;
        total_chunks: number;
        embedding_model_name?: string;
        vector_store_type: string;
      }>('rag_get_knowledge_base_status');
      return response;
    } catch (error) {
      console.error('获取知识库状态失败:', error);
      throw new Error(`获取知识库状态失败: ${error}`);
    }
  }

  // 删除知识库中的文档
  static async ragDeleteDocument(documentId: string): Promise<void> {
    try {
      await invoke<void>('rag_delete_document', { documentId: documentId });
    } catch (error) {
      console.error('删除知识库文档失败:', error);
      throw new Error(`删除知识库文档失败: ${error}`);
    }
  }

  // 查询知识库
  static async ragQueryKnowledgeBase(query: string, options: {
    top_k: number;
    enable_reranking?: boolean;
  }): Promise<{
    retrieved_chunks: Array<{
      chunk: {
        id: string;
        document_id: string;
        chunk_index: number;
        text: string;
        metadata: Record<string, string>;
      };
      score: number;
    }>;
    query_vector_time_ms: number;
    search_time_ms: number;
    reranking_time_ms?: number;
    total_time_ms: number;
  }> {
    try {
      const response = await invoke<{
        retrieved_chunks: Array<{
          chunk: {
            id: string;
            document_id: string;
            chunk_index: number;
            text: string;
            metadata: Record<string, string>;
          };
          score: number;
        }>;
        query_vector_time_ms: number;
        search_time_ms: number;
        reranking_time_ms?: number;
        total_time_ms: number;
      }>('rag_query_knowledge_base', { query, options });
      return response;
    } catch (error) {
      console.error('查询知识库失败:', error);
      throw new Error(`查询知识库失败: ${error}`);
    }
  }

  // 获取所有文档列表
  static async ragGetAllDocuments(): Promise<Array<{
    id: string;
    file_name: string;
    file_path?: string;
    file_size?: number;
    total_chunks: number;
    created_at: string;
    updated_at: string;
  }>> {
    try {
      const response = await invoke<Array<{
        id: string;
        file_name: string;
        file_path?: string;
        file_size?: number;
        total_chunks: number;
        created_at: string;
        updated_at: string;
      }>>('rag_get_all_documents');
      return response;
    } catch (error) {
      console.error('获取文档列表失败:', error);
      throw new Error(`获取文档列表失败: ${error}`);
    }
  }

  // 清空知识库
  static async ragClearKnowledgeBase(): Promise<void> {
    try {
      await invoke<void>('rag_clear_knowledge_base');
    } catch (error) {
      console.error('清空知识库失败:', error);
      throw new Error(`清空知识库失败: ${error}`);
    }
  }


  // ============================================================================
  // RAG增强的AI分析API
  // ============================================================================

  // RAG增强的流式分析
  static async startRagEnhancedStreamingAnswer(request: {
    temp_id: string;
    enable_chain_of_thought: boolean;
    enable_rag?: boolean;
    rag_options?: {
      top_k: number;
      enable_reranking?: boolean;
    };
  }): Promise<void> {
    try {
      await invoke<void>('start_rag_enhanced_streaming_answer', { request });
    } catch (error) {
      console.error('RAG增强流式分析失败:', error);
      throw new Error(`RAG增强流式分析失败: ${error}`);
    }
  }

  // RAG增强的继续对话
  static async continueRagEnhancedChatStream(request: {
    temp_id: string;
    chat_history: ChatMessage[];
    enable_chain_of_thought?: boolean;
    enable_rag?: boolean;
    rag_options?: {
      top_k: number;
      enable_reranking?: boolean;
    };
  }): Promise<{ new_assistant_message: string }> {
    try {
      const response = await invoke<{ new_assistant_message: string }>('continue_rag_enhanced_chat_stream', { request });
      return response;
    } catch (error) {
      console.error('RAG增强对话失败:', error);
      throw new Error(`RAG增强对话失败: ${error}`);
    }
  }

  // ============================================================================
  // 独立RAG查询API
  // ============================================================================



  // RAG配置管理API

  // 获取RAG配置
  static async getRagSettings(): Promise<{
    chunk_size: number;
    chunk_overlap: number;
    chunking_strategy: string;
    min_chunk_size: number;
    default_top_k: number;
    default_rerank_enabled: boolean;
  }> {
    try {
      const response = await invoke<{
        chunk_size: number;
        chunk_overlap: number;
        chunking_strategy: string;
        min_chunk_size: number;
        default_top_k: number;
        default_rerank_enabled: boolean;
      }>('get_rag_settings');
      return response;
    } catch (error) {
      console.error('获取RAG配置失败:', error);
      throw new Error(`获取RAG配置失败: ${error}`);
    }
  }

  // 更新RAG配置
  static async updateRagSettings(settings: {
    chunk_size: number;
    chunk_overlap: number;
    chunking_strategy: string;
    min_chunk_size: number;
    default_top_k: number;
    default_rerank_enabled: boolean;
  }): Promise<void> {
    try {
      await invoke<void>('update_rag_settings', { settings });
    } catch (error) {
      console.error('更新RAG配置失败:', error);
      throw new Error(`更新RAG配置失败: ${error}`);
    }
  }

  // 重置RAG配置为默认值
  static async resetRagSettings(): Promise<void> {
    try {
      await invoke<void>('reset_rag_settings');
    } catch (error) {
      console.error('重置RAG配置失败:', error);
      throw new Error(`重置RAG配置失败: ${error}`);
    }
  }

  // ==================== RAG分库管理方法 ====================

  /**
   * 创建新的RAG分库
   */
  static async createRagSubLibrary(request: {
    name: string;
    description?: string;
  }): Promise<{
    id: string;
    name: string;
    description?: string;
    created_at: string;
    updated_at: string;
    document_count: number;
    chunk_count: number;
  }> {
    try {
      const response = await invoke<{
        id: string;
        name: string;
        description?: string;
        created_at: string;
        updated_at: string;
        document_count: number;
        chunk_count: number;
      }>('create_rag_sub_library', { request });
      
      console.log('✅ 分库创建成功:', response);
      return response;
    } catch (error) {
      console.error('创建分库失败:', error);
      throw new Error(`创建分库失败: ${error}`);
    }
  }

  /**
   * 获取所有RAG分库列表
   */
  static async getRagSubLibraries(): Promise<Array<{
    id: string;
    name: string;
    description?: string;
    created_at: string;
    updated_at: string;
    document_count: number;
    chunk_count: number;
  }>> {
    try {
      const response = await invoke<Array<{
        id: string;
        name: string;
        description?: string;
        created_at: string;
        updated_at: string;
        document_count: number;
        chunk_count: number;
      }>>('get_rag_sub_libraries');
      
      console.log('✅ 获取分库列表成功:', response);
      return response;
    } catch (error) {
      console.error('获取分库列表失败:', error);
      throw new Error(`获取分库列表失败: ${error}`);
    }
  }

  /**
   * 根据ID获取RAG分库详情
   */
  static async getRagSubLibraryById(libraryId: string): Promise<{
    id: string;
    name: string;
    description?: string;
    created_at: string;
    updated_at: string;
    document_count: number;
    chunk_count: number;
  } | null> {
    try {
      const response = await invoke<{
        id: string;
        name: string;
        description?: string;
        created_at: string;
        updated_at: string;
        document_count: number;
        chunk_count: number;
      } | null>('get_rag_sub_library_by_id', { libraryId: libraryId });
      
      console.log('✅ 获取分库详情成功:', response);
      return response;
    } catch (error) {
      console.error('获取分库详情失败:', error);
      throw new Error(`获取分库详情失败: ${error}`);
    }
  }

  /**
   * 更新RAG分库信息
   */
  static async updateRagSubLibrary(libraryId: string, request: {
    name?: string;
    description?: string;
  }): Promise<{
    id: string;
    name: string;
    description?: string;
    created_at: string;
    updated_at: string;
    document_count: number;
    chunk_count: number;
  }> {
    try {
      const response = await invoke<{
        id: string;
        name: string;
        description?: string;
        created_at: string;
        updated_at: string;
        document_count: number;
        chunk_count: number;
      }>('update_rag_sub_library', { 
        libraryId: libraryId, 
        request 
      });
      
      console.log('✅ 分库更新成功:', response);
      return response;
    } catch (error) {
      console.error('更新分库失败:', error);
      throw new Error(`更新分库失败: ${error}`);
    }
  }

  /**
   * 删除RAG分库
   */
  static async deleteRagSubLibrary(libraryId: string, deleteDocuments: boolean = false): Promise<void> {
    try {
      await invoke<void>('delete_rag_sub_library', { 
        libraryId: libraryId,
        deleteDocuments: deleteDocuments
      });
      
      console.log('✅ 分库删除成功');
    } catch (error) {
      console.error('删除分库失败:', error);
      throw new Error(`删除分库失败: ${error}`);
    }
  }

  /**
   * 向指定分库添加文档
   */
  static async ragAddDocumentsToLibrary(request: {
    file_paths: string[];
    sub_library_id?: string;
  }): Promise<string> {
    try {
      const response = await invoke<string>('rag_add_documents_to_library', { request });
      
      console.log('✅ 文档添加到分库成功:', response);
      return response;
    } catch (error) {
      console.error('添加文档到分库失败:', error);
      throw new Error(`添加文档到分库失败: ${error}`);
    }
  }

  /**
   * 从Base64内容向指定分库添加文档
   */
  static async ragAddDocumentsFromContentToLibrary(request: {
    documents: Array<{
      file_name: string;
      base64_content: string;
    }>;
    sub_library_id?: string;
  }): Promise<string> {
    try {
      const response = await invoke<string>('rag_add_documents_from_content_to_library', { request });
      
      console.log('✅ 从内容添加文档到分库成功:', response);
      return response;
    } catch (error) {
      console.error('从内容添加文档到分库失败:', error);
      throw new Error(`从内容添加文档到分库失败: ${error}`);
    }
  }

  /**
   * 获取指定分库的文档列表
   */
  static async getRagDocumentsByLibrary(request: {
    sub_library_id?: string;
    page?: number;
    page_size?: number;
  }): Promise<Array<{
    id: string;
    file_name: string;
    file_path?: string;
    file_size?: number;
    total_chunks: number;
    sub_library_id: string;
    created_at: string;
    updated_at: string;
  }>> {
    try {
      const response = await invoke<Array<{
        id: string;
        file_name: string;
        file_path?: string;
        file_size?: number;
        total_chunks: number;
        sub_library_id: string;
        created_at: string;
        updated_at: string;
      }>>('get_rag_documents_by_library', { request });
      
      console.log('✅ 获取分库文档列表成功:', response);
      return response;
    } catch (error) {
      console.error('获取分库文档列表失败:', error);
      throw new Error(`获取分库文档列表失败: ${error}`);
    }
  }

  /**
   * 将文档移动到指定分库
   */
  static async moveDocumentToRagLibrary(documentId: string, targetLibraryId: string): Promise<void> {
    try {
      await invoke<void>('move_document_to_rag_library', { 
        documentId: documentId,
        targetLibraryId: targetLibraryId
      });
      
      console.log('✅ 文档移动成功');
    } catch (error) {
      console.error('移动文档失败:', error);
      throw new Error(`移动文档失败: ${error}`);
    }
  }

  /**
   * 在指定分库中查询知识库
   */
  static async ragQueryKnowledgeBaseInLibraries(query: string, options: {
    top_k: number;
    enable_reranking?: boolean;
    target_sub_library_ids?: string[];
  }): Promise<{
    retrieved_chunks: Array<{
      chunk: {
        id: string;
        document_id: string;
        chunk_index: number;
        text: string;
        metadata: Record<string, string>;
      };
      score: number;
    }>;
    query_vector_time_ms: number;
    search_time_ms: number;
    reranking_time_ms?: number;
    total_time_ms: number;
  }> {
    try {
      const response = await invoke<{
        retrieved_chunks: Array<{
          chunk: {
            id: string;
            document_id: string;
            chunk_index: number;
            text: string;
            metadata: Record<string, string>;
          };
          score: number;
        }>;
        query_vector_time_ms: number;
        search_time_ms: number;
        reranking_time_ms?: number;
        total_time_ms: number;
      }>('rag_query_knowledge_base_in_libraries', { query, options });
      
      console.log('✅ 分库查询成功:', response);
      return response;
    } catch (error) {
      console.error('分库查询失败:', error);
      throw new Error(`分库查询失败: ${error}`);
    }
  }

  // ==================== 回顾分析API - 复用错题分析模式 ====================

  /**
   * 从数据库获取回顾分析（复用错题分析的加载模式）
   */
  static async getReviewAnalysisById(id: string): Promise<ReviewAnalysisItem | null> {
    try {
      const response = await invoke<ReviewAnalysisItem | null>('get_review_analysis_by_id', { id });
      console.log('✅ 获取回顾分析成功:', response);
      return response;
    } catch (error) {
      console.error('❌ 获取回顾分析失败:', error);
      throw new Error(`获取回顾分析失败: ${error}`);
    }
  }

  /**
   * 删除回顾分析（统一回顾分析功能）
   */
  static async deleteReviewAnalysis(id: string): Promise<boolean> {
    try {
      console.log('🗑️ 开始删除回顾分析:', id);
      const response = await invoke<boolean>('delete_review_analysis', { id });
      if (response) {
        console.log('✅ 回顾分析删除成功:', id);
      } else {
        console.warn('⚠️ 回顾分析不存在或删除失败:', id);
      }
      return response;
    } catch (error) {
      console.error('❌ 删除回顾分析失败:', error);
      throw new Error(`删除回顾分析失败: ${error}`);
    }
  }

  // 🎯 新增：更新回顾分析（主要用于保存聊天历史）
  static async updateReviewAnalysis(reviewAnalysis: ReviewAnalysisItem): Promise<ReviewAnalysisItem> {
    try {
      const response = await invoke<ReviewAnalysisItem>('update_review_analysis', { 
        review_analysis: reviewAnalysis 
      });
      console.log('✅ 回顾分析更新成功');
      return response;
    } catch (error) {
      console.error('❌ 更新回顾分析失败:', error);
      // 如果后端没有这个命令，使用变通方案
      console.log('ℹ️ 后端可能没有update_review_analysis命令，使用变通方案');
      throw new Error(`更新回顾分析失败: ${error}`);
    }
  }

  /**
   * 回顾分析聊天追问（复用错题分析的追问模式）
   */
  static async continueReviewChatStream(params: {
    reviewId: string;
    chatHistory: ChatMessage[];
    enableChainOfThought: boolean;
    enableRag?: boolean;
    ragTopK?: number;
  }): Promise<void> {
    try {
      await invoke('continue_consolidated_review_stream', {
        request: {
          review_session_id: params.reviewId,
          chat_history: params.chatHistory,
          enable_chain_of_thought: params.enableChainOfThought,
          enable_rag: params.enableRag || false,
          rag_options: params.enableRag ? { top_k: params.ragTopK || 5 } : undefined
        }
      });
      console.log('✅ 回顾分析聊天追问成功');
    } catch (error) {
      console.error('❌ 回顾分析聊天追问失败:', error);
      throw new Error(`回顾分析聊天追问失败: ${error}`);
    }
  }

  // ==================== 通用invoke方法 ====================

  // ============================================================================
  // 错题总结生成 API
  // ============================================================================

  /**
   * 生成错题总结 - 基于聊天记录生成结构化总结
   */
  static async generateMistakeSummary(request: {
    mistake_id: string;
    force_regenerate?: boolean;
  }): Promise<{
    success: boolean;
    mistake_summary?: string | null;
    user_error_analysis?: string | null;
    error_message?: string | null;
  }> {
    try {
      console.log('🧠 开始生成错题总结:', request.mistake_id);
      
      const response = await invoke<{
        success: boolean;
        mistake_summary?: string | null;
        user_error_analysis?: string | null;
        error_message?: string | null;
      }>('generate_mistake_summary', {
        request: {
          mistake_id: request.mistake_id,
          force_regenerate: request.force_regenerate || false,
        }
      });
      
      if (response.success) {
        console.log('✅ 错题总结生成成功');
      } else {
        console.warn('⚠️ 错题总结生成失败:', response.error_message);
      }
      
      return response;
    } catch (error) {
      console.error('❌ 生成错题总结失败:', error);
      throw new Error(`生成总结失败: ${error}`);
    }
  }

  /**
   * LLM基于上下文生成回答
   */
  static async llmGenerateAnswerWithContext(userQuery: string, retrievedChunksJson: string): Promise<string> {
    try {
      console.log('🤖 调用LLM生成API:', { userQuery: userQuery.substring(0, 100) + '...', retrievedChunksJson });
      const response = await invoke<string>('llm_generate_answer_with_context', {
        userQuery: userQuery,
        retrievedChunksJson: retrievedChunksJson,
      });
      console.log('✅ LLM生成成功');
      return response;
    } catch (error) {
      console.error('❌ LLM生成回答失败:', error);
      throw new Error(`LLM生成失败: ${error}`);
    }
  }


  /**
   * 通用的Tauri invoke方法
   */
  static async invoke<T>(command: string, args?: Record<string, any>): Promise<T> {
    try {
      const response = await invoke<T>(command, args);
      console.log(`✅ ${command} 调用成功:`, response);
      return response;
    } catch (error) {
      console.error(`❌ ${command} 调用失败:`, error);
      throw new Error(`${command} 失败: ${error}`);
    }
  }
}
