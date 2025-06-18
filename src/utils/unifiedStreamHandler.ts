/**
 * Unified Streaming Handler
 * 
 * Consolidates all streaming approaches in the project into a single, 
 * consistent implementation that can be used across all components.
 */

import { listen } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export interface StreamChunk {
  content: string;
  is_complete: boolean;
  chunk_id: string;
}

export interface StreamMessage {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  thinking_content?: string;
  timestamp: string;
}

export interface StreamOptions {
  enableChainOfThought?: boolean;
  onChunk?: (chunk: string) => void;
  onThinking?: (thinking: string) => void;
  onComplete?: (fullResponse: string, thinkingContent?: string) => void;
  onError?: (error: string) => void;
  onProgress?: (progress: { content: string; thinking?: string }) => void;
}

export interface StreamRequest {
  type: 'analysis' | 'chat' | 'continue_chat';
  tempId?: string;
  mistakeId?: number;
  imageData?: string;
  userInput?: string;
  chatHistory?: StreamMessage[];
  enableChainOfThought?: boolean;
}

/**
 * Unified Streaming Manager
 * Replaces multiple streaming implementations with a single, consistent approach
 */
export class UnifiedStreamHandler {
  private activeStreams: Map<string, () => void> = new Map();
  private currentProgress: Map<string, { content: string; thinking: string }> = new Map();

  /**
   * Start a streaming request with unified handling
   */
  async startStream(request: StreamRequest, options: StreamOptions = {}): Promise<string> {
    const streamId = this.generateStreamId(request);
    
    // Stop any existing stream with the same ID
    this.stopStream(streamId);

    try {
      const cleanupFunctions = await this.initializeStreamListeners(streamId, request, options);
      this.activeStreams.set(streamId, () => {
        cleanupFunctions.forEach(cleanup => cleanup());
      });

      // Start the appropriate backend command
      await this.startBackendStream(request);
      
      return streamId;
    } catch (error) {
      console.error('Failed to start stream:', error);
      if (options.onError) {
        options.onError(error instanceof Error ? error.message : 'Unknown error');
      }
      throw error;
    }
  }

  /**
   * Stop a specific stream
   */
  stopStream(streamId: string): void {
    const cleanup = this.activeStreams.get(streamId);
    if (cleanup) {
      cleanup();
      this.activeStreams.delete(streamId);
      this.currentProgress.delete(streamId);
      console.log(`Stopped stream: ${streamId}`);
    }
  }

  /**
   * Stop all active streams
   */
  stopAllStreams(): void {
    for (const streamId of this.activeStreams.keys()) {
      this.stopStream(streamId);
    }
  }

  /**
   * Get current progress for a stream
   */
  getProgress(streamId: string): { content: string; thinking: string } | null {
    return this.currentProgress.get(streamId) || null;
  }

  /**
   * Check if a stream is active
   */
  isStreamActive(streamId: string): boolean {
    return this.activeStreams.has(streamId);
  }

  private generateStreamId(request: StreamRequest): string {
    const timestamp = Date.now();
    const type = request.type;
    const id = request.tempId || request.mistakeId || 'default';
    return `${type}_${id}_${timestamp}`;
  }

  private async initializeStreamListeners(
    streamId: string, 
    request: StreamRequest, 
    options: StreamOptions
  ): Promise<Array<() => void>> {
    const cleanupFunctions: Array<() => void> = [];
    let fullContent = '';
    let fullThinking = '';
    
    // Initialize progress tracking
    this.currentProgress.set(streamId, { content: '', thinking: '' });

    // Determine event names based on request type
    const baseEventName = this.getEventName(request);
    const contentEvent = baseEventName;
    const thinkingEvent = `${baseEventName}_reasoning`;

    console.log(`🔄 Initializing unified stream listeners for: ${streamId}`);
    console.log(`📡 Content event: ${contentEvent}`);
    console.log(`🧠 Thinking event: ${thinkingEvent}`);

    // Listen for main content stream
    try {
      const unlistenContent = await listen<StreamChunk>(contentEvent, (event) => {
        const chunk = event.payload;
        
        if (chunk.is_complete) {
          console.log(`✅ Content stream complete for ${streamId}`);
          this.checkStreamCompletion(streamId, fullContent, fullThinking, options);
          return;
        }

        if (chunk.content) {
          fullContent += chunk.content;
          
          // Update progress
          const progress = this.currentProgress.get(streamId);
          if (progress) {
            progress.content = fullContent;
            this.currentProgress.set(streamId, progress);
          }

          // Call callbacks
          if (options.onChunk) {
            options.onChunk(chunk.content);
          }
          if (options.onProgress) {
            options.onProgress({ content: fullContent, thinking: fullThinking || undefined });
          }
          
          console.log(`📝 Content update [${streamId}]: ${chunk.content.length} chars`);
        }
      });

      cleanupFunctions.push(unlistenContent);
    } catch (error) {
      console.error(`Failed to listen for content events: ${contentEvent}`, error);
    }

    // Listen for thinking chain (if enabled)
    if (request.enableChainOfThought !== false) {
      try {
        const unlistenThinking = await listen<StreamChunk>(thinkingEvent, (event) => {
          const chunk = event.payload;
          
          if (chunk.is_complete) {
            console.log(`✅ Thinking stream complete for ${streamId}`);
            this.checkStreamCompletion(streamId, fullContent, fullThinking, options);
            return;
          }

          if (chunk.content) {
            fullThinking += chunk.content;
            
            // Update progress
            const progress = this.currentProgress.get(streamId);
            if (progress) {
              progress.thinking = fullThinking;
              this.currentProgress.set(streamId, progress);
            }

            // Call callbacks
            if (options.onThinking) {
              options.onThinking(chunk.content);
            }
            if (options.onProgress) {
              options.onProgress({ content: fullContent, thinking: fullThinking || undefined });
            }
            
            console.log(`🧠 Thinking update [${streamId}]: ${chunk.content.length} chars`);
          }
        });

        cleanupFunctions.push(unlistenThinking);
      } catch (error) {
        console.error(`Failed to listen for thinking events: ${thinkingEvent}`, error);
      }
    }

    // Listen for errors
    try {
      const unlistenError = await listen<{error: string}>('stream_error', (event) => {
        console.error(`❌ Stream error [${streamId}]:`, event.payload.error);
        if (options.onError) {
          options.onError(event.payload.error);
        }
        this.stopStream(streamId);
      });

      cleanupFunctions.push(unlistenError);
    } catch (error) {
      console.error('Failed to listen for error events', error);
    }

    return cleanupFunctions;
  }

  private checkStreamCompletion(
    streamId: string, 
    content: string, 
    thinking: string, 
    options: StreamOptions
  ): void {
    // Check if both content and thinking (if enabled) are complete
    const progress = this.currentProgress.get(streamId);
    if (!progress) return;

    console.log(`🎉 Stream completion check [${streamId}]: content=${content.length}, thinking=${thinking.length}`);
    
    if (options.onComplete) {
      options.onComplete(content, thinking || "");
    }
    
    // Auto-cleanup after completion
    setTimeout(() => {
      this.stopStream(streamId);
    }, 100);
  }

  private getEventName(request: StreamRequest): string {
    switch (request.type) {
      case 'analysis':
        return request.tempId ? `analysis_stream_${request.tempId}` : 'stream_chunk';
      case 'chat':
        return 'stream_chunk';
      case 'continue_chat':
        return request.tempId ? `continue_chat_stream_${request.tempId}` : 'stream_chunk';
      default:
        return 'stream_chunk';
    }
  }

  private async startBackendStream(request: StreamRequest): Promise<void> {
    switch (request.type) {
      case 'analysis':
        if (!request.imageData || !request.userInput) {
          throw new Error('Analysis requires imageData and userInput');
        }
        
        // First do OCR analysis
        const analysisResponse = await invoke('analyze_step_by_step', {
          request: {
            subject: "数学",
            question_image_files: [request.imageData.startsWith('data:') ? request.imageData : `data:image/jpeg;base64,${request.imageData}`],
            analysis_image_files: [],
            user_question: request.userInput,
            enable_chain_of_thought: request.enableChainOfThought !== false
          }
        });

        // If we got a temp_id, start the streaming answer
        if ((analysisResponse as any)?.temp_id) {
          await invoke('start_streaming_answer', {
            request: {
              temp_id: (analysisResponse as any).temp_id,
              enable_chain_of_thought: request.enableChainOfThought !== false
            }
          });
        }
        break;

      case 'chat':
        await invoke('start_streaming_answer', {
          request: {
            chat_history: request.chatHistory?.map(msg => ({
              role: msg.role,
              content: msg.content,
              thinking_content: msg.thinking_content
            })) || [],
            enable_chain_of_thought: request.enableChainOfThought !== false
          }
        });
        break;

      case 'continue_chat':
        if (!request.tempId) {
          throw new Error('Continue chat requires tempId');
        }
        
        await invoke('continue_chat_stream', {
          request: {
            temp_id: request.tempId,
            chat_history: request.chatHistory?.map(msg => ({
              role: msg.role,
              content: msg.content,
              thinking_content: msg.thinking_content
            })) || [],
            enable_chain_of_thought: request.enableChainOfThought !== false
          }
        });
        break;

      default:
        throw new Error(`Unsupported stream type: ${request.type}`);
    }
  }
}

// Create global instance
export const unifiedStreamHandler = new UnifiedStreamHandler();

/**
 * React Hook for Unified Streaming
 * Provides a simple interface for React components to use unified streaming
 */
export function useUnifiedStream() {
  const [activeStreams, setActiveStreams] = React.useState<Set<string>>(new Set());
  const [streamProgress, setStreamProgress] = React.useState<Map<string, { content: string; thinking: string }>>(new Map());

  React.useEffect(() => {
    return () => {
      // Cleanup all streams when component unmounts
      unifiedStreamHandler.stopAllStreams();
    };
  }, []);

  const startStream = React.useCallback(async (request: StreamRequest, options: StreamOptions = {}) => {
    const streamId = await unifiedStreamHandler.startStream(request, {
      ...options,
      onProgress: (progress) => {
        setStreamProgress(prev => new Map(prev.set(streamId, { content: progress.content, thinking: progress.thinking || "" })));
        if (options.onProgress) {
          options.onProgress(progress);
        }
      },
      onComplete: (content, thinking) => {
        setActiveStreams(prev => {
          const newSet = new Set(prev);
          newSet.delete(streamId);
          return newSet;
        });
        if (options.onComplete) {
          options.onComplete(content, thinking);
        }
      },
      onError: (error) => {
        setActiveStreams(prev => {
          const newSet = new Set(prev);
          newSet.delete(streamId);
          return newSet;
        });
        if (options.onError) {
          options.onError(error);
        }
      }
    });

    setActiveStreams(prev => new Set(prev.add(streamId)));
    return streamId;
  }, []);

  const stopStream = React.useCallback((streamId: string) => {
    unifiedStreamHandler.stopStream(streamId);
    setActiveStreams(prev => {
      const newSet = new Set(prev);
      newSet.delete(streamId);
      return newSet;
    });
    setStreamProgress(prev => {
      const newMap = new Map(prev);
      newMap.delete(streamId);
      return newMap;
    });
  }, []);

  const stopAllStreams = React.useCallback(() => {
    unifiedStreamHandler.stopAllStreams();
    setActiveStreams(new Set());
    setStreamProgress(new Map());
  }, []);

  return {
    startStream,
    stopStream,
    stopAllStreams,
    activeStreams: Array.from(activeStreams),
    streamProgress: Object.fromEntries(streamProgress),
    isStreaming: activeStreams.size > 0
  };
}

// Add React import for the hook
import React from 'react';