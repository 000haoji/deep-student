/**
 * Tauri API Client
 * 
 * Simple client for direct Tauri API calls without streaming.
 * Single responsibility: Non-streaming API communication.
 */

import { invoke } from '@tauri-apps/api/core';

export interface ChatMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking_content?: string;
  timestamp?: string;
}

export interface AnalysisRequest {
  subject?: string;
  question_image_files: string[];
  analysis_image_files?: string[];
  user_question: string;
  enable_chain_of_thought?: boolean;
}

/**
 * Tauri API Client - provides direct access to backend commands
 */
export class TauriApiClient {
  /**
   * Perform step-by-step analysis
   */
  async analyzeStepByStep(request: AnalysisRequest): Promise<any> {
    return invoke('analyze_step_by_step', { request });
  }

  /**
   * Start streaming answer for existing analysis
   */
  async startStreamingAnswer(tempId: string, enableChainOfThought: boolean = true): Promise<any> {
    return invoke('start_streaming_answer', {
      request: {
        temp_id: tempId,
        enable_chain_of_thought: enableChainOfThought
      }
    });
  }

  /**
   * Continue chat stream
   */
  async continueChatStream(tempId: string, chatHistory: any[], enableChainOfThought: boolean = true): Promise<any> {
    return invoke('continue_chat_stream', {
      request: {
        temp_id: tempId,
        chat_history: chatHistory,
        enable_chain_of_thought: enableChainOfThought
      }
    });
  }

  /**
   * Get chat history for a mistake
   */
  async getMistakeChatHistory(mistakeId: number): Promise<any> {
    return invoke('get_mistake_chat_history', { mistakeId });
  }

  /**
   * Get all mistakes
   */
  async getAllMistakes(): Promise<any> {
    return invoke('get_all_mistakes');
  }

  /**
   * Get mistake by ID
   */
  async getMistakeById(id: number): Promise<any> {
    return invoke('get_mistake_by_id', { id });
  }

  /**
   * Save mistake
   */
  async saveMistake(mistake: any): Promise<any> {
    return invoke('save_mistake', { mistake });
  }

  /**
   * Delete mistake
   */
  async deleteMistake(id: number): Promise<any> {
    return invoke('delete_mistake', { id });
  }

  /**
   * Batch save mistakes
   */
  async batchSaveMistakes(mistakes: any[]): Promise<any> {
    return invoke('batch_save_mistakes', { mistakes });
  }

  /**
   * Batch delete mistakes
   */
  async batchDeleteMistakes(ids: number[]): Promise<any> {
    return invoke('batch_delete_mistakes', { ids });
  }

  /**
   * Get supported subjects
   */
  async getSupportedSubjects(): Promise<string[]> {
    return invoke('get_supported_subjects');
  }

  /**
   * Get subject configs
   */
  async getSubjectConfigs(): Promise<any[]> {
    return invoke('get_all_subject_configs', { active_only: true });
  }
}

/**
 * Default client instance
 */
export const tauriApiClient = new TauriApiClient();