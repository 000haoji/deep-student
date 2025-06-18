/**
 * Tauri Stream Adapter
 * 
 * Focused adapter for converting Tauri backend streaming to standard streaming protocols.
 * Single responsibility: Stream protocol conversion.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export interface StreamControllerManager {
  controller: ReadableStreamDefaultController;
  listeners: Array<() => void>;
  responseStarted: boolean;
}

/**
 * Create a streaming response for Tauri analysis requests
 */
export async function createAnalysisStream(
  imageData: string,
  userInput: string,
  enableChainOfThought: boolean = true
): Promise<Response> {
  const stream = new ReadableStream({
    start(controller) {
      const manager: StreamControllerManager = {
        controller,
        listeners: [],
        responseStarted: false
      };

      handleAnalysisStream(imageData, userInput, enableChainOfThought, manager);
      
      return () => {
        manager.listeners.forEach(cleanup => cleanup());
      };
    }
  });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/plain; charset=utf-8',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive'
    }
  });
}

/**
 * Create a streaming response for Tauri chat requests
 */
export async function createChatStream(
  tempId: string,
  chatHistory: any[],
  enableChainOfThought: boolean = true
): Promise<Response> {
  const stream = new ReadableStream({
    start(controller) {
      const manager: StreamControllerManager = {
        controller,
        listeners: [],
        responseStarted: false
      };

      handleChatStream(tempId, chatHistory, enableChainOfThought, manager);
      
      return () => {
        manager.listeners.forEach(cleanup => cleanup());
      };
    }
  });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/plain; charset=utf-8',
      'Cache-Control': 'no-cache',
      'Connection': 'keep-alive'
    }
  });
}

/**
 * Handle analysis streaming events
 */
async function handleAnalysisStream(
  imageData: string,
  userInput: string,
  enableChainOfThought: boolean,
  manager: StreamControllerManager
): Promise<void> {
  try {
    // Set up event listeners
    const unlistenContent = await listen('stream_chunk', (event: any) => {
      const chunk = formatStreamChunk(event.payload.content);
      manager.controller.enqueue(new TextEncoder().encode(chunk));
    });

    const unlistenThinking = enableChainOfThought 
      ? await listen('stream_thinking', (event: any) => {
          const chunk = formatThinkingChunk(event.payload.thinking);
          manager.controller.enqueue(new TextEncoder().encode(chunk));
        })
      : null;

    const unlistenComplete = await listen('stream_complete', () => {
      finishStream(manager.controller);
    });

    const unlistenError = await listen('stream_error', (event: any) => {
      manager.controller.error(new Error(event.payload.error));
    });

    // Store cleanup functions
    manager.listeners.push(unlistenContent, unlistenComplete, unlistenError);
    if (unlistenThinking) {
      manager.listeners.push(unlistenThinking);
    }

    // Start the backend analysis
    const base64Data = formatImageData(imageData);
    await invoke('analyze_step_by_step', {
      request: {
        subject: "数学",
        question_image_files: [base64Data],
        analysis_image_files: [],
        user_question: userInput,
        enable_chain_of_thought: enableChainOfThought
      }
    });

  } catch (error) {
    manager.controller.error(error);
  }
}

/**
 * Handle chat streaming events
 */
async function handleChatStream(
  tempId: string,
  chatHistory: any[],
  enableChainOfThought: boolean,
  manager: StreamControllerManager
): Promise<void> {
  try {
    // Set up event listeners
    const unlistenContent = await listen('stream_chunk', (event: any) => {
      const chunk = formatStreamChunk(event.payload.content);
      manager.controller.enqueue(new TextEncoder().encode(chunk));
    });

    const unlistenThinking = enableChainOfThought 
      ? await listen('stream_thinking', (event: any) => {
          const chunk = formatThinkingChunk(event.payload.thinking);
          manager.controller.enqueue(new TextEncoder().encode(chunk));
        })
      : null;

    const unlistenComplete = await listen('stream_complete', () => {
      finishStream(manager.controller);
    });

    const unlistenError = await listen('stream_error', (event: any) => {
      manager.controller.error(new Error(event.payload.error));
    });

    // Store cleanup functions
    manager.listeners.push(unlistenContent, unlistenComplete, unlistenError);
    if (unlistenThinking) {
      manager.listeners.push(unlistenThinking);
    }

    // Start the chat stream
    await invoke('continue_chat_stream', {
      request: {
        temp_id: tempId,
        chat_history: chatHistory,
        enable_chain_of_thought: enableChainOfThought
      }
    });

  } catch (error) {
    manager.controller.error(error);
  }
}

/**
 * Format streaming content chunk
 */
function formatStreamChunk(content: string): string {
  return `data: ${JSON.stringify({
    choices: [{
      delta: {
        content: content
      }
    }]
  })}\n\n`;
}

/**
 * Format thinking chain chunk
 */
function formatThinkingChunk(thinking: string): string {
  return `data: ${JSON.stringify({
    choices: [{
      delta: {
        thinking: thinking
      }
    }]
  })}\n\n`;
}

/**
 * Finish the stream
 */
function finishStream(controller: ReadableStreamDefaultController): void {
  const finishChunk = `data: ${JSON.stringify({
    choices: [{
      finish_reason: 'stop'
    }]
  })}\n\n`;
  
  controller.enqueue(new TextEncoder().encode(finishChunk));
  controller.enqueue(new TextEncoder().encode('data: [DONE]\n\n'));
  controller.close();
}

/**
 * Format image data for backend
 */
function formatImageData(imageData: string): string {
  return imageData.startsWith('data:') ? imageData : `data:image/jpeg;base64,${imageData}`;
}