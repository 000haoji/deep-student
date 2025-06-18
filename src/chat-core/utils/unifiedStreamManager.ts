/**
 * 统一流式处理管理器
 * 
 * 替代项目中的多种流式处理实现，提供统一、一致的流式数据处理方案
 * 支持所有现有的流式场景：分析、对话、回顾分析等
 */

import { listen } from '@tauri-apps/api/event';
import React from 'react';

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
  type: 'analysis' | 'chat' | 'continue_chat' | 'review_analysis' | 'review_chat';
  tempId?: string;
  mistakeId?: string;
  reviewId?: string;
  imageData?: string[];
  userInput?: string;
  chatHistory?: StreamMessage[];
  subject?: string;
  enableChainOfThought?: boolean;
}

export interface StreamState {
  isStreaming: boolean;
  content: string;
  thinking: string;
  error?: string;
}

/**
 * 统一流式处理管理器
 * 集中管理所有流式监听器，避免内存泄漏
 */
export class UnifiedStreamManager {
  private activeListeners: Map<string, () => void> = new Map();
  private streamStates: Map<string, StreamState> = new Map();
  
  /**
   * 开始流式处理
   */
  async startStream(
    streamId: string,
    eventName: string,
    options: StreamOptions = {}
  ): Promise<void> {
    console.log(`🚀 开始流式处理 [${streamId}]: ${eventName}`);
    
    // 清理现有的流
    this.stopStream(streamId);
    
    // 初始化流状态
    this.streamStates.set(streamId, {
      isStreaming: true,
      content: '',
      thinking: ''
    });
    
    let fullContent = '';
    let fullThinking = '';
    let contentListenerActive = true;
    let thinkingListenerActive = true;
    
    try {
      // 监听主内容流
      const unlistenContent = await listen(eventName, (event: any) => {
        if (!contentListenerActive) return;
        
        const payload = event.payload;
        console.log(`💬 收到流式内容 [${streamId}]:`, payload);
        
        if (payload) {
          // 检查是否完成
          if (payload.is_complete) {
            console.log(`🎉 流式内容完成 [${streamId}]，总长度: ${fullContent.length}`);
            contentListenerActive = false;
            unlistenContent();
            
            // 如果思维链监听器也不活跃了，则设置整体完成状态
            if (!thinkingListenerActive) {
              this.completeStream(streamId, fullContent, fullThinking, options);
            }
            return;
          }
          
          // 累积内容
          if (payload.content) {
            fullContent += payload.content;
            
            // 更新状态
            const state = this.streamStates.get(streamId);
            if (state) {
              state.content = fullContent;
              this.streamStates.set(streamId, state);
            }
            
            // 触发回调
            if (options.onChunk) {
              options.onChunk(payload.content);
            }
            if (options.onProgress) {
              options.onProgress({ content: fullContent, thinking: fullThinking || undefined });
            }
            
            console.log(`📝 累积内容长度 [${streamId}]: ${fullContent.length} 字符`);
          }
        }
      });
      
      this.activeListeners.set(`${streamId}_content`, unlistenContent);
      
      // 如果启用思维链，监听思维链事件
      if (options.enableChainOfThought) {
        const reasoningEvent = `${eventName}_reasoning`;
        console.log(`🧠 监听思维链事件 [${streamId}]: ${reasoningEvent}`);
        
        const unlistenThinking = await listen(reasoningEvent, (event: any) => {
          if (!thinkingListenerActive) return;
          
          const payload = event.payload;
          console.log(`🧠 思维链内容 [${streamId}]:`, payload);
          
          if (payload) {
            // 检查是否完成
            if (payload.is_complete) {
              console.log(`🎉 思维链完成 [${streamId}]，总长度: ${fullThinking.length}`);
              thinkingListenerActive = false;
              unlistenThinking();
              
              // 如果主内容监听器也不活跃了，则设置整体完成状态
              if (!contentListenerActive) {
                this.completeStream(streamId, fullContent, fullThinking, options);
              }
              return;
            }
            
            // 累积思维链内容
            if (payload.content) {
              fullThinking += payload.content;
              
              // 更新状态
              const state = this.streamStates.get(streamId);
              if (state) {
                state.thinking = fullThinking;
                this.streamStates.set(streamId, state);
              }
              
              // 触发回调
              if (options.onThinking) {
                options.onThinking(payload.content);
              }
              if (options.onProgress) {
                options.onProgress({ content: fullContent, thinking: fullThinking || undefined });
              }
              
              console.log(`🧠 思维链累积长度 [${streamId}]: ${fullThinking.length} 字符`);
            }
          }
        });
        
        this.activeListeners.set(`${streamId}_thinking`, unlistenThinking);
      } else {
        // 如果没有启用思维链，标记思维链监听器为不活跃
        thinkingListenerActive = false;
      }
      
    } catch (error) {
      console.error(`❌ 流式处理启动失败 [${streamId}]:`, error);
      this.handleStreamError(streamId, error as string, options);
    }
  }
  
  /**
   * 停止指定流
   */
  stopStream(streamId: string): void {
    console.log(`🛑 停止流式处理 [${streamId}]`);
    
    // 清理内容监听器
    const contentListener = this.activeListeners.get(`${streamId}_content`);
    if (contentListener) {
      contentListener();
      this.activeListeners.delete(`${streamId}_content`);
    }
    
    // 清理思维链监听器
    const thinkingListener = this.activeListeners.get(`${streamId}_thinking`);
    if (thinkingListener) {
      thinkingListener();
      this.activeListeners.delete(`${streamId}_thinking`);
    }
    
    // 清理状态
    this.streamStates.delete(streamId);
  }
  
  /**
   * 停止所有流
   */
  stopAllStreams(): void {
    console.log(`🧹 清理所有流式处理，共 ${this.activeListeners.size} 个监听器`);
    
    for (const [key, unlisten] of this.activeListeners) {
      try {
        unlisten();
      } catch (error) {
        console.warn(`清理监听器失败 [${key}]:`, error);
      }
    }
    
    this.activeListeners.clear();
    this.streamStates.clear();
  }
  
  /**
   * 获取流状态
   */
  getStreamState(streamId: string): StreamState | undefined {
    return this.streamStates.get(streamId);
  }
  
  /**
   * 检查流是否活跃
   */
  isStreamActive(streamId: string): boolean {
    const state = this.streamStates.get(streamId);
    return state?.isStreaming || false;
  }
  
  /**
   * 完成流处理
   */
  private completeStream(
    streamId: string,
    content: string,
    thinking: string,
    options: StreamOptions
  ): void {
    console.log(`🎉 流式处理完成 [${streamId}]`);
    
    // 更新状态
    const state = this.streamStates.get(streamId);
    if (state) {
      state.isStreaming = false;
      this.streamStates.set(streamId, state);
    }
    
    // 触发完成回调
    if (options.onComplete) {
      options.onComplete(content, thinking || undefined);
    }
    
    // 延迟清理（给UI一些时间更新）
    setTimeout(() => {
      this.stopStream(streamId);
    }, 100);
  }
  
  /**
   * 处理流错误
   */
  private handleStreamError(
    streamId: string,
    error: string,
    options: StreamOptions
  ): void {
    console.error(`❌ 流式处理错误 [${streamId}]:`, error);
    
    // 更新状态
    const state = this.streamStates.get(streamId);
    if (state) {
      state.isStreaming = false;
      state.error = error;
      this.streamStates.set(streamId, state);
    }
    
    // 触发错误回调
    if (options.onError) {
      options.onError(error);
    }
    
    // 清理流
    this.stopStream(streamId);
  }
}

// 创建全局实例
export const unifiedStreamManager = new UnifiedStreamManager();

/**
 * React Hook for unified streaming
 * 提供React组件使用的统一流式处理Hook
 */
export function useUnifiedStream() {
  const [activeStreams, setActiveStreams] = React.useState<Set<string>>(new Set());
  const [streamStates, setStreamStates] = React.useState<Map<string, StreamState>>(new Map());
  
  // 组件卸载时清理所有流
  React.useEffect(() => {
    return () => {
      console.log('🧹 组件卸载，清理所有流式处理');
      unifiedStreamManager.stopAllStreams();
    };
  }, []);
  
  const startStream = React.useCallback(async (
    streamId: string,
    eventName: string,
    options: StreamOptions = {}
  ) => {
    setActiveStreams(prev => new Set(prev.add(streamId)));
    
    const enhancedOptions: StreamOptions = {
      ...options,
      onProgress: (progress) => {
        setStreamStates(prev => new Map(prev.set(streamId, {
          isStreaming: true,
          content: progress.content,
          thinking: progress.thinking || ''
        })));
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
        setStreamStates(prev => new Map(prev.set(streamId, {
          isStreaming: false,
          content,
          thinking: thinking || ''
        })));
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
        setStreamStates(prev => new Map(prev.set(streamId, {
          isStreaming: false,
          content: '',
          thinking: '',
          error
        })));
        if (options.onError) {
          options.onError(error);
        }
      }
    };
    
    await unifiedStreamManager.startStream(streamId, eventName, enhancedOptions);
  }, []);
  
  const stopStream = React.useCallback((streamId: string) => {
    unifiedStreamManager.stopStream(streamId);
    setActiveStreams(prev => {
      const newSet = new Set(prev);
      newSet.delete(streamId);
      return newSet;
    });
    setStreamStates(prev => {
      const newMap = new Map(prev);
      newMap.delete(streamId);
      return newMap;
    });
  }, []);
  
  const stopAllStreams = React.useCallback(() => {
    unifiedStreamManager.stopAllStreams();
    setActiveStreams(new Set());
    setStreamStates(new Map());
  }, []);
  
  const isStreamActive = React.useCallback((streamId: string) => {
    return activeStreams.has(streamId);
  }, [activeStreams]);
  
  const getStreamState = React.useCallback((streamId: string) => {
    return streamStates.get(streamId);
  }, [streamStates]);
  
  return {
    startStream,
    stopStream,
    stopAllStreams,
    isStreamActive,
    getStreamState,
    activeStreams: Array.from(activeStreams),
    streamStates
  };
}