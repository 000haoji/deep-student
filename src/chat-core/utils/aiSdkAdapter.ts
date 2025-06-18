/**
 * AI SDK Adapter for Tauri Backend
 * 
 * Simplified adapter focused on AI SDK integration.
 * Uses separated modules for stream handling and API communication.
 */

import { createAnalysisStream, createChatStream } from './tauriStreamAdapter';
import { tauriApiClient } from './tauriApiClient';
import { convertToTauriFormat, type ChatMessage } from './chatMessageConverter';

export interface AnalysisRequest {
  imageData?: string;
  userInput?: string;
  chatHistory?: ChatMessage[];
  enableChainOfThought?: boolean;
}

/**
 * Custom fetch implementation for AI SDK compatibility
 * Routes requests to appropriate stream handlers
 */
export async function createTauriStreamingFetch(
  url: string,
  options: RequestInit & { body?: string }
): Promise<Response> {
  const requestData = JSON.parse(options.body as string);
  
  if (url.includes('/api/analysis')) {
    const { imageData, userInput, enableChainOfThought = true } = requestData;
    return createAnalysisStream(imageData, userInput, enableChainOfThought);
  } 
  
  if (url.includes('/api/chat') || url.includes('/api/continue-chat')) {
    const { tempId, chatHistory, enableChainOfThought = true } = requestData;
    const tauriHistory = convertToTauriFormat(chatHistory || []);
    return createChatStream(tempId, tauriHistory, enableChainOfThought);
  }
  
  throw new Error(`Unsupported endpoint: ${url}`);
}

/**
 * Create a custom fetch function for AI SDK
 */
export function createAISdkFetch() {
  return async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === 'string' ? input : input.toString();
    const options = { ...init, body: init?.body as string };
    
    if (url.startsWith('/api/')) {
      return createTauriStreamingFetch(url, options);
    }
    
    return fetch(input, init);
  };
}

/**
 * Simplified API adapter using the dedicated client
 */
export const tauriApiAdapter = {
  async analyzeStepByStep(imageData: string, userInput: string, enableChainOfThought = true) {
    const base64Data = imageData.startsWith('data:') ? imageData : `data:image/jpeg;base64,${imageData}`;
    
    return tauriApiClient.analyzeStepByStep({
      subject: "数学",
      question_image_files: [base64Data],
      analysis_image_files: [],
      user_question: userInput,
      enable_chain_of_thought: enableChainOfThought
    });
  },
  
  async getChatHistory(tempId: string) {
    console.log('getChatHistory called for tempId:', tempId);
    return [];
  },
  
  async startStreamingAnswer(tempId: string, enableChainOfThought = true) {
    return tauriApiClient.startStreamingAnswer(tempId, enableChainOfThought);
  },
  
  async continueChatStream(tempId: string, chatHistory: any[], enableChainOfThought = true) {
    return tauriApiClient.continueChatStream(tempId, chatHistory, enableChainOfThought);
  },
  
  async getMistakeChatHistory(mistakeId: number) {
    return tauriApiClient.getMistakeChatHistory(mistakeId);
  },
  
  async getAllMistakes() {
    return tauriApiClient.getAllMistakes();
  }
};