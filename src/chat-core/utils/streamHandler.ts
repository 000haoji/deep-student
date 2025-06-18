import { listen } from '@tauri-apps/api/event';

export interface StreamChunk {
  content: string;
  is_complete: boolean;
  chunk_id: string;
}

export class StreamHandler {
  private listeners: Map<string, (chunk: StreamChunk) => void> = new Map();
  private unlistenFunctions: Map<string, () => void> = new Map();

  // 开始监听流式事件
  async startListening(eventName: string, onChunk: (chunk: StreamChunk) => void): Promise<void> {
    // 如果已经在监听这个事件，先停止
    if (this.unlistenFunctions.has(eventName)) {
      this.stopListening(eventName);
    }

    this.listeners.set(eventName, onChunk);

    try {
      const unlisten = await listen<StreamChunk>(eventName, (event) => {
        const chunk = event.payload;
        
        // 详细的控制台调试输出
        console.group(`🔄 流式数据 [${eventName}]`);
        console.log('📦 数据块ID:', chunk.chunk_id);
        console.log('📝 内容长度:', chunk.content.length);
        console.log('✅ 是否完成:', chunk.is_complete);
        if (chunk.content.length > 0) {
          console.log('📄 内容预览:', chunk.content.length > 100 ? 
            chunk.content.substring(0, 100) + '...' : chunk.content);
        }
        console.groupEnd();
        
        // 如果内容不为空，显示实时内容
        if (chunk.content.length > 0) {
          console.log(`💬 实时输出: ${chunk.content}`);
        }
        
        // 调用回调函数处理数据块
        onChunk(chunk);
        
        // 如果流完成，自动停止监听
        if (chunk.is_complete) {
          console.log(`🎉 流式输出完成 [${eventName}]`);
          this.stopListening(eventName);
        }
      });

      this.unlistenFunctions.set(eventName, unlisten);
      console.log(`🎧 开始监听流式事件: ${eventName}`);
    } catch (error) {
      console.error(`❌ 监听流式事件失败 [${eventName}]:`, error);
      throw error;
    }
  }

  // 停止监听流式事件
  stopListening(eventName: string): void {
    const unlisten = this.unlistenFunctions.get(eventName);
    if (unlisten) {
      unlisten();
      this.unlistenFunctions.delete(eventName);
      this.listeners.delete(eventName);
      console.log(`停止监听流式事件: ${eventName}`);
    }
  }

  // 停止所有监听
  stopAllListening(): void {
    for (const eventName of this.unlistenFunctions.keys()) {
      this.stopListening(eventName);
    }
  }

  // 检查是否正在监听某个事件
  isListening(eventName: string): boolean {
    return this.unlistenFunctions.has(eventName);
  }
}

// 创建全局实例
export const streamHandler = new StreamHandler(); 