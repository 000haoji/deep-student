/**
 * 调试日志记录模块
 * 用于追踪RAG内容显示、聊天记录保存、聊天记录串号等关键问题
 */

import { invoke } from '@tauri-apps/api/core';

export type LogLevel = 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'TRACE';

export interface LogEntry {
  timestamp: string;
  level: LogLevel;
  module: string;
  operation: string;
  data: any;
  context?: {
    userId?: string;
    sessionId?: string;
    mistakeId?: string;
    streamId?: string;
    businessId?: string;
  };
  stackTrace?: string;
}

export interface ChatRecordDebugInfo {
  action: 'LOAD' | 'SAVE' | 'DISPLAY' | 'MISMATCH';
  mistakeId: string;
  expectedChatHistory?: any[];
  actualChatHistory?: any[];
  ragSources?: any[];
  thinkingContent?: Map<number, string>;
  streamingState?: any;
}

export interface RAGDebugInfo {
  action: 'QUERY' | 'RESPONSE' | 'DISPLAY' | 'MISSING';
  query?: string;
  sources?: any[];
  displayedSources?: any[];
  expectedCount?: number;
  actualCount?: number;
  ragEnabled?: boolean;
  ragTopK?: number;
}

class DebugLogger {
  private logQueue: LogEntry[] = [];
  private flushInterval: number | null = null;
  private maxQueueSize = 100;

  constructor() {
    this.startAutoFlush();
    this.setupErrorHandlers();
  }

  /**
   * 记录聊天记录相关的调试信息
   */
  async logChatRecord(
    level: LogLevel,
    operation: string,
    debugInfo: ChatRecordDebugInfo,
    additionalData?: any
  ) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      module: 'CHAT_RECORD',
      operation,
      data: {
        debugInfo,
        additionalData,
        // 添加详细的聊天记录比较信息
        chatHistoryComparison: debugInfo.action === 'MISMATCH' ? {
          expectedLength: debugInfo.expectedChatHistory?.length || 0,
          actualLength: debugInfo.actualChatHistory?.length || 0,
          firstMismatchIndex: this.findFirstMismatch(
            debugInfo.expectedChatHistory || [],
            debugInfo.actualChatHistory || []
          )
        } : undefined
      },
      context: {
        mistakeId: debugInfo.mistakeId,
        sessionId: this.getCurrentSessionId(),
        businessId: debugInfo.mistakeId
      },
      stackTrace: level === 'ERROR' ? new Error().stack : undefined
    };

    await this.addLog(logEntry);

    // 立即输出严重问题
    if (level === 'ERROR' || debugInfo.action === 'MISMATCH') {
      console.error('🚨 [CHAT_RECORD_CRITICAL]', logEntry);
      await this.flushLogs();
    }
  }

  /**
   * 记录RAG相关的调试信息
   */
  async logRAG(
    level: LogLevel,
    operation: string,
    debugInfo: RAGDebugInfo,
    additionalData?: any
  ) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      module: 'RAG',
      operation,
      data: {
        debugInfo,
        additionalData,
        // 添加RAG状态检查
        ragStateCheck: {
          ragEnabled: debugInfo.ragEnabled,
          expectedSources: debugInfo.expectedCount || 0,
          actualSources: debugInfo.actualCount || 0,
          missingSourcesCount: (debugInfo.expectedCount || 0) - (debugInfo.actualCount || 0)
        }
      },
      context: {
        sessionId: this.getCurrentSessionId()
      },
      stackTrace: level === 'ERROR' ? new Error().stack : undefined
    };

    await this.addLog(logEntry);

    // 立即输出RAG问题
    if (level === 'ERROR' || debugInfo.action === 'MISSING') {
      console.error('🚨 [RAG_CRITICAL]', logEntry);
      await this.flushLogs();
    }
  }

  /**
   * 记录通用调试信息
   */
  async log(
    level: LogLevel,
    module: string,
    operation: string,
    data: any,
    context?: any
  ) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      module,
      operation,
      data,
      context: {
        ...context,
        sessionId: this.getCurrentSessionId()
      },
      stackTrace: level === 'ERROR' ? new Error().stack : undefined
    };

    await this.addLog(logEntry);

    if (level === 'ERROR') {
      console.error(`🚨 [${module}_ERROR]`, logEntry);
      await this.flushLogs();
    }
  }

  /**
   * 记录组件状态变化
   */
  async logStateChange(
    component: string,
    operation: string,
    oldState: any,
    newState: any,
    trigger?: string
  ) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level: 'TRACE',
      module: 'STATE_CHANGE',
      operation: `${component}.${operation}`,
      data: {
        component,
        oldState: this.sanitizeState(oldState),
        newState: this.sanitizeState(newState),
        stateDiff: this.calculateStateDiff(oldState, newState),
        trigger
      },
      context: {
        sessionId: this.getCurrentSessionId()
      }
    };

    await this.addLog(logEntry);
  }

  /**
   * 记录流式处理相关信息
   */
  async logStreaming(
    operation: string,
    streamId: string,
    eventType: string,
    payload: any,
    additionalInfo?: any
  ) {
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level: 'DEBUG',
      module: 'STREAMING',
      operation,
      data: {
        streamId,
        eventType,
        payload: this.sanitizePayload(payload),
        additionalInfo,
        payloadSize: JSON.stringify(payload).length
      },
      context: {
        streamId,
        sessionId: this.getCurrentSessionId()
      }
    };

    await this.addLog(logEntry);
  }

  /**
   * 记录API调用信息
   */
  async logApiCall(
    operation: string,
    method: string,
    url: string,
    request: any,
    response?: any,
    error?: any,
    duration?: number
  ) {
    const level: LogLevel = error ? 'ERROR' : 'INFO';
    const logEntry: LogEntry = {
      timestamp: new Date().toISOString(),
      level,
      module: 'API',
      operation,
      data: {
        method,
        url,
        request: this.sanitizeRequest(request),
        response: this.sanitizeResponse(response),
        error: error ? {
          message: error.message,
          code: error.code,
          stack: error.stack
        } : undefined,
        duration: duration ? `${duration}ms` : undefined
      },
      context: {
        sessionId: this.getCurrentSessionId()
      },
      stackTrace: error ? new Error().stack : undefined
    };

    await this.addLog(logEntry);

    if (error) {
      console.error('🚨 [API_ERROR]', logEntry);
      await this.flushLogs();
    }
  }

  private async addLog(logEntry: LogEntry) {
    this.logQueue.push(logEntry);
    
    // 队列满了就立即刷新
    if (this.logQueue.length >= this.maxQueueSize) {
      await this.flushLogs();
    }
  }

  private async flushLogs() {
    if (this.logQueue.length === 0) return;

    const logsToFlush = [...this.logQueue];
    this.logQueue = [];

    try {
      // 调用后端写入日志文件
      await invoke('write_debug_logs', { logs: logsToFlush });
    } catch (error) {
      console.error('Failed to write debug logs:', error);
      // 如果后端写入失败，至少在浏览器控制台输出
      console.group('📋 Debug Logs (Backend Write Failed)');
      logsToFlush.forEach(log => {
        const prefix = `[${log.timestamp}] [${log.level}] [${log.module}]`;
        switch (log.level) {
          case 'ERROR':
            console.error(prefix, log);
            break;
          case 'WARN':
            console.warn(prefix, log);
            break;
          case 'DEBUG':
          case 'TRACE':
            console.debug(prefix, log);
            break;
          default:
            console.log(prefix, log);
        }
      });
      console.groupEnd();
    }
  }

  private startAutoFlush() {
    // 每5秒自动刷新一次日志
    this.flushInterval = window.setInterval(() => {
      this.flushLogs();
    }, 5000);
  }

  private setupErrorHandlers() {
    // 捕获未处理的错误
    window.addEventListener('error', (event) => {
      this.log('ERROR', 'GLOBAL', 'UNHANDLED_ERROR', {
        message: event.message,
        filename: event.filename,
        lineno: event.lineno,
        colno: event.colno,
        error: event.error
      });
    });

    // 捕获Promise拒绝
    window.addEventListener('unhandledrejection', (event) => {
      this.log('ERROR', 'GLOBAL', 'UNHANDLED_REJECTION', {
        reason: event.reason,
        promise: event.promise
      });
    });
  }

  private findFirstMismatch(expected: any[], actual: any[]): number {
    const minLength = Math.min(expected.length, actual.length);
    for (let i = 0; i < minLength; i++) {
      if (JSON.stringify(expected[i]) !== JSON.stringify(actual[i])) {
        return i;
      }
    }
    return expected.length !== actual.length ? minLength : -1;
  }

  private calculateStateDiff(oldState: any, newState: any) {
    if (typeof oldState !== 'object' || typeof newState !== 'object') {
      return { changed: oldState !== newState };
    }

    const changes: any = {};
    const allKeys = new Set([...Object.keys(oldState || {}), ...Object.keys(newState || {})]);
    
    for (const key of allKeys) {
      if (oldState?.[key] !== newState?.[key]) {
        changes[key] = {
          from: oldState?.[key],
          to: newState?.[key]
        };
      }
    }

    return changes;
  }

  private sanitizeState(state: any) {
    if (!state) return state;
    
    // 避免记录过大的状态对象
    const sanitized = { ...state };
    
    // 限制聊天历史长度
    if (sanitized.chatHistory && Array.isArray(sanitized.chatHistory)) {
      if (sanitized.chatHistory.length > 10) {
        sanitized.chatHistory = [
          ...sanitized.chatHistory.slice(0, 5),
          { _truncated: `... ${sanitized.chatHistory.length - 10} items ...` },
          ...sanitized.chatHistory.slice(-5)
        ];
      }
    }

    // 限制思维链内容
    if (sanitized.thinkingContent instanceof Map) {
      sanitized.thinkingContent = Object.fromEntries(sanitized.thinkingContent);
    }

    return sanitized;
  }

  private sanitizePayload(payload: any) {
    if (!payload) return payload;
    
    // 限制payload大小
    const str = JSON.stringify(payload);
    if (str.length > 1000) {
      return {
        _truncated: true,
        _originalSize: str.length,
        _preview: str.substring(0, 500) + '...'
      };
    }
    
    return payload;
  }

  private sanitizeRequest(request: any) {
    if (!request) return request;
    
    const sanitized = { ...request };
    
    // 移除敏感信息
    if (sanitized.password) sanitized.password = '[REDACTED]';
    if (sanitized.apiKey) sanitized.apiKey = '[REDACTED]';
    if (sanitized.token) sanitized.token = '[REDACTED]';
    
    return sanitized;
  }

  private sanitizeResponse(response: any) {
    return this.sanitizePayload(response);
  }

  private getCurrentSessionId(): string {
    // 获取当前会话ID，可以从localStorage或其他地方获取
    return localStorage.getItem('debug-session-id') || 
           `session-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }

  /**
   * 清理资源
   */
  destroy() {
    if (this.flushInterval) {
      clearInterval(this.flushInterval);
      this.flushInterval = null;
    }
    this.flushLogs();
  }
}

// 导出单例实例
export const debugLogger = new DebugLogger();

// 便捷方法
export const logChatRecord = debugLogger.logChatRecord.bind(debugLogger);
export const logRAG = debugLogger.logRAG.bind(debugLogger);
export const logStateChange = debugLogger.logStateChange.bind(debugLogger);
export const logStreaming = debugLogger.logStreaming.bind(debugLogger);
export const logApiCall = debugLogger.logApiCall.bind(debugLogger);
export const log = debugLogger.log.bind(debugLogger);

// 确保页面卸载时清理日志
window.addEventListener('beforeunload', () => {
  debugLogger.destroy();
});