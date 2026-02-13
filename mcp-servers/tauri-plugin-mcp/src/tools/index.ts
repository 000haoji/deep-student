/**
 * MCP Tools Index
 * 注册所有 MCP 工具到服务器
 */

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { z } from "zod";
import { socketClient } from "./client.js";

// ============================================================================
// 工具注册
// ============================================================================

export function registerAllTools(server: McpServer) {
  // 调试工具 - 错误捕获
  registerDebugErrorTools(server);
  
  // 调试工具 - 网络监控
  registerDebugNetworkTools(server);
  
  // 调试工具 - 状态调试
  registerDebugStoreTools(server);
  
  // 调试工具 - 操作录制
  registerDebugActionTools(server);
  
  // 调试工具 - 性能监控
  registerDebugPerformanceTools(server);
  
  // 调试工具 - 元素高亮
  registerDebugHighlightTools(server);
  
  // 调试工具 - 断言验证
  registerDebugAssertTools(server);
  
  // 调试工具 - 综合状态
  registerDebugStatusTools(server);
  
  // ⭐ AI 自动调试核心工具
  registerSmartActionTools(server);
  
  console.error("All MCP debug tools registered");
}

// ============================================================================
// 初始化 Socket 连接
// ============================================================================

export async function initializeSocket() {
  try {
    await socketClient.connect();
    console.error("Socket/WebSocket connection initialized");
  } catch (error) {
    console.error("Failed to initialize socket:", error);
    throw error;
  }
}

export function isSocketConnected(): boolean {
  return socketClient.isConnected();
}

// ============================================================================
// 辅助函数：执行 WebView JS
// ============================================================================

async function executeDebugCommand(command: string, windowId: string = "main"): Promise<any> {
  // Use async IIFE to support await in command
  const script = `(async () => {
    if (!window.__MCP_DEBUG__) {
      return { __mcpError: 'MCP Debug module not initialized. Make sure the app is running in debug mode.' };
    }
    try {
      const result = ${command};
      return { __mcpSuccess: true, __mcpData: result };
    } catch (e) {
      return { __mcpError: e.message || String(e) };
    }
  })()`;
  
  const result = await socketClient.sendCommand("execute_js", {
    script,
    windowLabel: windowId,
  });
  
  // Unwrap the WebSocket response
  const jsResult = unwrapResult(result);
  
  // Check for our wrapper markers
  if (jsResult?.__mcpError) {
    throw new Error(jsResult.__mcpError);
  }
  
  if (jsResult?.__mcpSuccess) {
    return jsResult.__mcpData;
  }
  
  // Fallback: return as-is
  return jsResult;
}

async function executeRawJS(script: string, windowId: string = "main"): Promise<any> {
  const result = await socketClient.sendCommand("execute_js", {
    script,
    windowLabel: windowId,
  });
  return unwrapResult(result);
}

// Unwrap the nested result from WebSocket response
// tauri-plugin-mcp-bridge may return: value, { data: value }, or { data: { data: value } }
function unwrapResult(result: any): any {
  if (result === null || result === undefined) {
    return result;
  }
  
  // Unwrap nested data fields (max 2 levels)
  let current = result;
  for (let i = 0; i < 2; i++) {
    if (typeof current === 'object' && current !== null && 'data' in current && Object.keys(current).length <= 3) {
      current = current.data;
    } else {
      break;
    }
  }
  
  return current;
}

async function waitForReadyState(
  target: "domcontentloaded" | "load",
  windowId: string = "main",
  timeoutMs: number = 10000,
  intervalMs: number = 100
): Promise<{ state: string; elapsed: number }> {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const state = await executeRawJS("document.readyState", windowId);
      if (target === "domcontentloaded" && state !== "loading") {
        return { state, elapsed: Date.now() - start };
      }
      if (target === "load" && state === "complete") {
        return { state, elapsed: Date.now() - start };
      }
    } catch {
      // 页面切换中，忽略错误继续等待
    }
    await new Promise(resolve => setTimeout(resolve, intervalMs));
  }
  return { state: "timeout", elapsed: Date.now() - start };
}

// ============================================================================
// 错误捕获工具
// ============================================================================

function registerDebugErrorTools(server: McpServer) {
  // 启动错误捕获
  server.tool(
    "debug_error_start",
    "Start capturing JavaScript errors, unhandled Promise rejections, and console.error calls",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.errorCapture.start()");
      return { content: [{ type: "text", text: "Error capture started" }] };
    }
  );
  
  // 停止错误捕获
  server.tool(
    "debug_error_stop",
    "Stop capturing errors",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.errorCapture.stop()");
      return { content: [{ type: "text", text: "Error capture stopped" }] };
    }
  );
  
  // 获取错误列表
  server.tool(
    "debug_error_get",
    "Get captured errors. Optionally filter by keyword",
    {
      filter: z.string().optional().describe("Filter errors by keyword in message, type, or stack"),
    },
    async ({ filter }) => {
      const filterArg = filter ? `"${filter}"` : "";
      const errors = await executeDebugCommand(`window.__MCP_DEBUG__.errorCapture.get(${filterArg})`);
      
      if (!errors || errors.length === 0) {
        return { content: [{ type: "text", text: "No errors captured" }] };
      }
      
      const formatted = errors.map((e: any, i: number) => 
        `[${i + 1}] ${e.type} - ${e.message}\n` +
        `    Time: ${new Date(e.timestamp).toISOString()}\n` +
        (e.stack ? `    Stack: ${e.stack.split('\n').slice(0, 3).join('\n    ')}\n` : '')
      ).join('\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `Captured ${errors.length} error(s):\n\n${formatted}` 
        }] 
      };
    }
  );
  
  // 清除错误
  server.tool(
    "debug_error_clear",
    "Clear all captured errors",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.errorCapture.clear()");
      return { content: [{ type: "text", text: "Errors cleared" }] };
    }
  );
}

// ============================================================================
// 网络监控工具
// ============================================================================

function registerDebugNetworkTools(server: McpServer) {
  // 启动网络监控
  server.tool(
    "debug_network_start",
    "Start monitoring network requests (Fetch, XHR, WebSocket)",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.networkMonitor.start()");
      return { content: [{ type: "text", text: "Network monitoring started" }] };
    }
  );
  
  // 停止网络监控
  server.tool(
    "debug_network_stop",
    "Stop network monitoring",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.networkMonitor.stop()");
      return { content: [{ type: "text", text: "Network monitoring stopped" }] };
    }
  );
  
  // 获取网络请求
  server.tool(
    "debug_network_get",
    "Get captured network requests. Optionally filter by URL, status, or method",
    {
      url: z.string().optional().describe("Filter by URL (substring match)"),
      status: z.number().optional().describe("Filter by HTTP status code"),
      method: z.string().optional().describe("Filter by HTTP method (GET, POST, etc.)"),
      includeWebSocket: z.boolean().optional().describe("Include WebSocket messages"),
    },
    async ({ url, status, method, includeWebSocket }) => {
      let filterObj: any = {};
      if (url) filterObj.url = url;
      if (status) filterObj.status = status;
      if (method) filterObj.method = method;
      
      const filterArg = Object.keys(filterObj).length > 0 
        ? JSON.stringify(filterObj) 
        : "";
      
      const requests = await executeDebugCommand(
        `window.__MCP_DEBUG__.networkMonitor.get(${filterArg})`
      );
      
      let result = "";
      
      if (requests && requests.length > 0) {
        result += `HTTP Requests (${requests.length}):\n`;
        result += requests.slice(-20).map((r: any, i: number) => {
          const statusEmoji = r.status >= 200 && r.status < 300 ? "✅" : 
                              r.status >= 400 ? "❌" : "⏳";
          return `${statusEmoji} [${r.method}] ${r.url.substring(0, 60)}... → ${r.status || 'pending'} (${r.duration || '?'}ms)`;
        }).join('\n');
      } else {
        result += "No HTTP requests captured\n";
      }
      
      if (includeWebSocket) {
        const wsMessages = await executeDebugCommand(
          "window.__MCP_DEBUG__.networkMonitor.getWebSocket()"
        );
        
        if (wsMessages && wsMessages.length > 0) {
          result += `\n\nWebSocket Messages (${wsMessages.length}):\n`;
          result += wsMessages.slice(-10).map((m: any) => {
            const dir = m.direction === 'sent' ? '→' : '←';
            const data = typeof m.data === 'string' 
              ? m.data.substring(0, 50) 
              : JSON.stringify(m.data).substring(0, 50);
            return `${dir} [${m.type}] ${data}...`;
          }).join('\n');
        }
      }
      
      return { content: [{ type: "text", text: result }] };
    }
  );
  
  // 清除网络记录
  server.tool(
    "debug_network_clear",
    "Clear all captured network requests",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.networkMonitor.clear()");
      return { content: [{ type: "text", text: "Network logs cleared" }] };
    }
  );
}

// ============================================================================
// 状态调试工具
// ============================================================================

function registerDebugStoreTools(server: McpServer) {
  // 获取状态快照
  server.tool(
    "debug_store_snapshot",
    "Get a snapshot of Zustand store state. Lists registered stores if no name provided",
    {
      storeName: z.string().optional().describe("Name of the store to snapshot. Omit to list all stores"),
    },
    async ({ storeName }) => {
      const arg = storeName ? `"${storeName}"` : "";
      const result = await executeDebugCommand(`window.__MCP_DEBUG__.storeDebugger.snapshot(${arg})`);
      
      if (!result || result.length === 0) {
        // 获取已注册的 stores
        const stores = await executeDebugCommand(
          "(() => { try { return window.__MCP_DEBUG__.storeDebugger.getRegisteredStores ? window.__MCP_DEBUG__.storeDebugger.getRegisteredStores() : []; } catch { return []; } })()"
        );
        
        if (stores && stores.length > 0) {
          return { 
            content: [{ 
              type: "text", 
              text: `No snapshot available. Registered stores: ${stores.join(', ')}\n\nUse debug_store_snapshot with a store name.` 
            }] 
          };
        }
        
        return { 
          content: [{ 
            type: "text", 
            text: "No stores registered. Use storeDebugger.registerStore('name', store) in your app to register stores for debugging." 
          }] 
        };
      }
      
      const formatted = result.map((s: any) => {
        const stateStr = JSON.stringify(s.state, null, 2);
        const truncated = stateStr.length > 2000 
          ? stateStr.substring(0, 2000) + '\n... (truncated)'
          : stateStr;
        return `Store: ${s.storeName}\nState:\n${truncated}`;
      }).join('\n\n---\n\n');
      
      return { content: [{ type: "text", text: formatted }] };
    }
  );
  
  // 订阅状态变化
  server.tool(
    "debug_store_subscribe",
    "Subscribe to state changes in a Zustand store",
    {
      storeName: z.string().describe("Name of the store to subscribe to"),
      selector: z.string().optional().describe("Optional path selector (e.g., 'user.profile')"),
    },
    async ({ storeName, selector }) => {
      const selectorArg = selector ? `, "${selector}"` : "";
      await executeDebugCommand(
        `window.__MCP_DEBUG__.storeDebugger.subscribe("${storeName}"${selectorArg})`
      );
      return { 
        content: [{ 
          type: "text", 
          text: `Subscribed to store: ${storeName}${selector ? ` (selector: ${selector})` : ''}` 
        }] 
      };
    }
  );
  
  // 获取状态变化
  server.tool(
    "debug_store_changes",
    "Get recorded state changes",
    {
      storeName: z.string().optional().describe("Filter by store name"),
    },
    async ({ storeName }) => {
      const arg = storeName ? `"${storeName}"` : "";
      const changes = await executeDebugCommand(
        `window.__MCP_DEBUG__.storeDebugger.getChanges(${arg})`
      );
      
      if (!changes || changes.length === 0) {
        return { content: [{ type: "text", text: "No state changes recorded" }] };
      }
      
      const formatted = changes.slice(-20).map((c: any, i: number) => {
        const path = c.path.join('.');
        const prev = JSON.stringify(c.previousValue)?.substring(0, 50) || 'undefined';
        const next = JSON.stringify(c.newValue)?.substring(0, 50) || 'undefined';
        return `[${c.storeName}] ${path}: ${prev} → ${next}`;
      }).join('\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `State changes (last ${Math.min(changes.length, 20)} of ${changes.length}):\n\n${formatted}` 
        }] 
      };
    }
  );
  
  // 清除状态调试数据
  server.tool(
    "debug_store_clear",
    "Clear all state debugging data (snapshots and changes)",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.storeDebugger.clear()");
      return { content: [{ type: "text", text: "Store debug data cleared" }] };
    }
  );
}

// ============================================================================
// 操作录制工具
// ============================================================================

function registerDebugActionTools(server: McpServer) {
  // 开始录制
  server.tool(
    "debug_action_record",
    "Start recording user actions (clicks, inputs, scrolls, etc.)",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.start()");
      return { content: [{ type: "text", text: "Action recording started. Perform actions in the app, then use debug_action_stop to get the recorded actions." }] };
    }
  );
  
  // 停止录制
  server.tool(
    "debug_action_stop",
    "Stop recording and get the recorded actions",
    {},
    async () => {
      const actions = await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.stop()");
      
      if (!actions || actions.length === 0) {
        return { content: [{ type: "text", text: "No actions recorded" }] };
      }
      
      const formatted = actions.map((a: any, i: number) => {
        const selector = a.target.selector.substring(0, 60);
        const data = a.data ? JSON.stringify(a.data).substring(0, 40) : '';
        return `${i + 1}. [${a.type}] ${selector} ${data}`;
      }).join('\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `Recorded ${actions.length} action(s):\n\n${formatted}\n\nUse debug_action_replay to replay these actions.` 
        }] 
      };
    }
  );
  
  // 获取录制的操作
  server.tool(
    "debug_action_get",
    "Get the currently recorded actions without stopping",
    {},
    async () => {
      const actions = await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.get()");
      
      if (!actions || actions.length === 0) {
        return { content: [{ type: "text", text: "No actions recorded yet" }] };
      }
      
      return { 
        content: [{ 
          type: "text", 
          text: `${actions.length} action(s) recorded so far` 
        }] 
      };
    }
  );
  
  // 回放操作
  server.tool(
    "debug_action_replay",
    "Replay previously recorded actions",
    {
      speed: z.number().optional().describe("Replay speed multiplier (default: 1)"),
    },
    async ({ speed }) => {
      const actions = await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.get()");
      
      if (!actions || actions.length === 0) {
        return { content: [{ type: "text", text: "No actions to replay. Record some actions first." }] };
      }
      
      const speedArg = speed || 1;
      await executeDebugCommand(
        `window.__MCP_DEBUG__.actionRecorder.replay(window.__MCP_DEBUG__.actionRecorder.get(), ${speedArg})`
      );
      
      return { 
        content: [{ 
          type: "text", 
          text: `Replaying ${actions.length} action(s) at ${speedArg}x speed` 
        }] 
      };
    }
  );
  
  // 清除录制
  server.tool(
    "debug_action_clear",
    "Clear all recorded actions",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.clear()");
      return { content: [{ type: "text", text: "Recorded actions cleared" }] };
    }
  );
}

// ============================================================================
// 性能监控工具
// ============================================================================

function registerDebugPerformanceTools(server: McpServer) {
  // 启动性能监控
  server.tool(
    "debug_perf_start",
    "Start performance monitoring (memory, FPS, long tasks)",
    {
      interval: z.number().optional().describe("Sampling interval in milliseconds (default: 1000)"),
    },
    async ({ interval }) => {
      const arg = interval ? interval.toString() : "";
      await executeDebugCommand(`window.__MCP_DEBUG__.performanceMonitor.start(${arg})`);
      return { content: [{ type: "text", text: `Performance monitoring started (interval: ${interval || 1000}ms)` }] };
    }
  );
  
  // 停止性能监控
  server.tool(
    "debug_perf_stop",
    "Stop performance monitoring",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.performanceMonitor.stop()");
      return { content: [{ type: "text", text: "Performance monitoring stopped" }] };
    }
  );
  
  // 获取性能数据
  server.tool(
    "debug_perf_get",
    "Get performance metrics and summary",
    {},
    async () => {
      try {
        const data = await executeDebugCommand(`(() => {
          const pm = window.__MCP_DEBUG__.performanceMonitor;
          return {
            metrics: pm.get(),
            summary: pm.getSummary ? pm.getSummary() : null,
            latest: pm.getLatest ? pm.getLatest() : null
          };
        })()`);
        
        let result = "";
        
        if (data?.latest) {
          result += "Latest Metrics:\n";
          if (data.latest.memory) {
            const usedMB = (data.latest.memory.usedJSHeapSize / 1024 / 1024).toFixed(1);
            const totalMB = (data.latest.memory.totalJSHeapSize / 1024 / 1024).toFixed(1);
            result += `  Memory: ${usedMB}MB / ${totalMB}MB\n`;
          }
          if (data.latest.fps !== undefined) {
            result += `  FPS: ${data.latest.fps}\n`;
          }
          if (data.latest.domNodes !== undefined) {
            result += `  DOM Nodes: ${data.latest.domNodes}\n`;
          }
        }
        
        if (data?.summary) {
          result += "\nSummary:\n";
          result += `  Samples: ${data.summary.samples}\n`;
          if (data.summary.memory) {
            result += `  Memory (avg): ${(data.summary.memory.avg / 1024 / 1024).toFixed(1)}MB\n`;
          }
          if (data.summary.fps) {
            result += `  FPS (avg): ${data.summary.fps.avg}\n`;
          }
          if (data.summary.longTasks) {
            result += `  Long Tasks: ${data.summary.longTasks.count} (total: ${data.summary.longTasks.totalDuration.toFixed(0)}ms)\n`;
          }
        }
        
        if (!result) {
          result = "No performance data available. Start monitoring first.";
        }
        
        return { content: [{ type: "text", text: result }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );
  
  // 触发 GC
  server.tool(
    "debug_perf_gc",
    "Trigger garbage collection (requires Chrome with --expose-gc flag)",
    {},
    async () => {
      const result = await executeDebugCommand("window.__MCP_DEBUG__.performanceMonitor.gc()");
      return { 
        content: [{ 
          type: "text", 
          text: result ? "Garbage collection triggered" : "GC not available" 
        }] 
      };
    }
  );
  
  // 清除性能数据
  server.tool(
    "debug_perf_clear",
    "Clear all performance data",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.performanceMonitor.clear()");
      return { content: [{ type: "text", text: "Performance data cleared" }] };
    }
  );
}

// ============================================================================
// 元素高亮工具
// ============================================================================

function registerDebugHighlightTools(server: McpServer) {
  // 高亮元素
  server.tool(
    "debug_highlight",
    "Highlight an element on the page for visual debugging",
    {
      selector: z.string().describe("CSS selector of the element to highlight"),
      color: z.string().optional().describe("Highlight color (default: #ff0000)"),
      duration: z.number().optional().describe("Duration in ms (0 = permanent, default: 3000)"),
      label: z.string().optional().describe("Label to show above the element"),
      pulse: z.boolean().optional().describe("Enable pulse animation"),
    },
    async ({ selector, color, duration, label, pulse }) => {
      try {
        const options = {
          selector,
          color: color || '#ff0000',
          duration: duration ?? 3000,
          label,
          pulse: pulse ?? false,
        };
        
        const id = await executeDebugCommand(
          `window.__MCP_DEBUG__.highlighter.show(${JSON.stringify(options)})`
        );
        
        if (!id) {
          return { content: [{ type: "text", text: `Element not found: ${selector}` }] };
        }
        
        return { content: [{ type: "text", text: `Highlighted element: ${selector} (id: ${id})` }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );
  
  // 隐藏高亮
  server.tool(
    "debug_highlight_hide",
    "Hide highlighted elements",
    {
      id: z.string().optional().describe("Highlight ID to hide. Omit to hide all"),
    },
    async ({ id }) => {
      try {
        const arg = id ? `"${id}"` : "";
        await executeDebugCommand(`window.__MCP_DEBUG__.highlighter.hide(${arg})`);
        return { content: [{ type: "text", text: id ? `Highlight ${id} hidden` : "All highlights hidden" }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );
  
  // 检查并高亮元素 - 使用原生 DOM 实现，不依赖 __MCP_DEBUG__
  server.tool(
    "debug_inspect",
    "Find, highlight, and get information about an element",
    {
      selector: z.string().describe("CSS selector of the element to inspect"),
    },
    async ({ selector }) => {
      try {
        const escapedSelector = escapeForJS(selector);
        const result = await executeRawJS(`(() => {
          const el = document.querySelector("${escapedSelector}");
          if (!el) return { found: false };
          
          const rect = el.getBoundingClientRect();
          const style = window.getComputedStyle(el);
          
          return {
            found: true,
            element: {
              tagName: el.tagName.toLowerCase(),
              id: el.id || null,
              className: el.className || null,
              textContent: (el.textContent || '').trim().substring(0, 200),
              rect: { x: rect.x, y: rect.y, width: rect.width, height: rect.height },
              computedStyle: {
                display: style.display,
                visibility: style.visibility,
                opacity: style.opacity
              }
            }
          };
        })()`);
        
        if (!result?.found) {
          return { content: [{ type: "text", text: `Element not found: ${selector}` }] };
        }
        
        const el = result.element;
        let info = `Element: <${el.tagName}>\n`;
        if (el.id) info += `  ID: ${el.id}\n`;
        if (el.className) info += `  Class: ${el.className.substring(0, 100)}\n`;
        if (el.textContent) info += `  Text: ${el.textContent.substring(0, 100)}\n`;
        info += `  Size: ${el.rect.width.toFixed(0)}x${el.rect.height.toFixed(0)}\n`;
        info += `  Position: (${el.rect.x.toFixed(0)}, ${el.rect.y.toFixed(0)})\n`;
        info += `  Display: ${el.computedStyle.display}\n`;
        info += `  Visibility: ${el.computedStyle.visibility}\n`;
        
        return { content: [{ type: "text", text: info }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );
  
  // 选择器建议
  server.tool(
    "debug_selector_suggest",
    "Get suggested selectors for an element at coordinates",
    {
      x: z.number().describe("X coordinate"),
      y: z.number().describe("Y coordinate"),
    },
    async ({ x, y }) => {
      const selectors = await executeDebugCommand(
        `window.__MCP_DEBUG__.selector.suggest(${x}, ${y})`
      );
      
      if (!selectors || selectors.length === 0) {
        return { content: [{ type: "text", text: `No element found at (${x}, ${y})` }] };
      }
      
      return { 
        content: [{ 
          type: "text", 
          text: `Suggested selectors:\n${selectors.map((s: string, i: number) => `${i + 1}. ${s}`).join('\n')}` 
        }] 
      };
    }
  );
  
  // 验证选择器
  server.tool(
    "debug_selector_validate",
    "Validate a CSS selector and count matching elements",
    {
      selector: z.string().describe("CSS selector to validate"),
    },
    async ({ selector }) => {
      const escapedSelector = escapeForJS(selector);
      const result = await executeDebugCommand(
        `window.__MCP_DEBUG__.selector.validate("${escapedSelector}")`
      );
      
      if (result.error) {
        return { content: [{ type: "text", text: `Invalid selector: ${result.error}` }] };
      }
      
      return { 
        content: [{ 
          type: "text", 
          text: `Selector "${selector}" is ${result.valid ? 'valid' : 'invalid'}. Matches ${result.count} element(s).` 
        }] 
      };
    }
  );
}

// ============================================================================
// 断言工具
// ============================================================================

function registerDebugAssertTools(server: McpServer) {
  // 单个断言
  server.tool(
    "debug_assert",
    "Run an assertion to verify UI state",
    {
      type: z.enum([
        'element-exists',
        'element-not-exists', 
        'element-visible',
        'element-hidden',
        'text-contains',
        'text-equals',
        'class-contains',
        'count-equals',
      ]).describe("Type of assertion"),
      selector: z.string().describe("CSS selector for the element"),
      expected: z.any().optional().describe("Expected value (for text/count assertions)"),
    },
    async ({ type, selector, expected }) => {
      const expectedArg = expected !== undefined ? JSON.stringify(expected) : "undefined";
      const result = await executeDebugCommand(
        `window.__MCP_DEBUG__.assert.check("${type}", "${selector}", ${expectedArg})`
      );
      
      const emoji = result.passed ? "✅" : "❌";
      return { 
        content: [{ 
          type: "text", 
          text: `${emoji} ${result.message}${result.actual !== undefined ? `\n   Actual: ${JSON.stringify(result.actual)}` : ''}` 
        }] 
      };
    }
  );
  
  // 批量断言
  server.tool(
    "debug_assert_batch",
    "Run multiple assertions at once",
    {
      assertions: z.array(z.object({
        type: z.string(),
        selector: z.string(),
        expected: z.any().optional(),
      })).describe("Array of assertions to run"),
    },
    async ({ assertions }) => {
      const results = await executeDebugCommand(
        `window.__MCP_DEBUG__.assert.batch(${JSON.stringify(assertions)})`
      );
      
      const passed = results.filter((r: any) => r.passed).length;
      const failed = results.length - passed;
      
      const formatted = results.map((r: any, i: number) => {
        const emoji = r.passed ? "✅" : "❌";
        return `${emoji} ${r.message}`;
      }).join('\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `Results: ${passed} passed, ${failed} failed\n\n${formatted}` 
        }] 
      };
    }
  );
}

// ============================================================================
// 综合状态工具
// ============================================================================

function registerDebugStatusTools(server: McpServer) {
  // 获取调试模块状态
  server.tool(
    "debug_status",
    "Get the status of all debug modules",
    {},
    async () => {
      try {
        // 使用简化的检查，只获取必要的状态信息
        const check = await executeRawJS(`(() => {
          if (!window.__MCP_DEBUG__) {
            return { initialized: false };
          }
          try {
            const api = window.__MCP_DEBUG__;
            return {
              initialized: true,
              version: api.version || '1.0.0',
              errorCapture: {
                enabled: api.errorCapture?.start ? true : false,
                count: (api.errorCapture?.get?.() || []).length
              },
              networkMonitor: {
                enabled: true,
                count: (api.networkMonitor?.get?.() || []).length
              },
              storeDebugger: {
                enabled: true,
                count: (api.storeDebugger?.getRegisteredStores?.() || []).length
              },
              actionRecorder: {
                recording: false,
                count: (api.actionRecorder?.get?.() || []).length
              },
              performanceMonitor: {
                enabled: true,
                count: (api.performanceMonitor?.get?.() || []).length
              }
            };
          } catch (e) {
            return { initialized: true, error: e.message };
          }
        })()`);
        
        if (!check?.initialized) {
          return { content: [{ type: "text", text: "⚠️ MCP Debug module not initialized. Make sure the app is running in development mode." }] };
        }
        
        if (check.error) {
          return { content: [{ type: "text", text: `MCP Debug initialized but status check failed: ${check.error}` }] };
        }
        
        let result = `MCP Debug v${check.version}\n\n`;
        result += `Error Capture: 🟢 Available (${check.errorCapture?.count || 0} errors)\n`;
        result += `Network Monitor: 🟢 Available (${check.networkMonitor?.count || 0} requests)\n`;
        result += `Store Debugger: 🟢 Available (${check.storeDebugger?.count || 0} stores)\n`;
        result += `Action Recorder: 🟢 Available (${check.actionRecorder?.count || 0} actions)\n`;
        result += `Performance Monitor: 🟢 Available (${check.performanceMonitor?.count || 0} samples)\n`;
        
        return { content: [{ type: "text", text: result }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );
  
  // 重置所有调试模块
  server.tool(
    "debug_reset",
    "Reset all debug modules to initial state",
    {},
    async () => {
      await executeDebugCommand("window.__MCP_DEBUG__.reset()");
      return { content: [{ type: "text", text: "All debug modules reset" }] };
    }
  );
}

// ============================================================================
// ⭐ AI 自动调试核心工具
// ============================================================================

/**
 * 安全转义字符串用于 JS 代码
 */
function escapeForJS(str: string): string {
  return str
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/'/g, "\\'")
    .replace(/\n/g, '\\n')
    .replace(/\r/g, '\\r')
    .replace(/\t/g, '\\t');
}

function registerSmartActionTools(server: McpServer) {
  // 等待元素出现
  server.tool(
    "wait_for_element",
    "Wait for an element to appear on the page. Returns when element is found or timeout.",
    {
      selector: z.string().describe("CSS selector for the element"),
      timeout: z.number().optional().describe("Timeout in milliseconds (default: 5000)"),
      visible: z.boolean().optional().describe("Require element to be visible (default: true)"),
      enabled: z.boolean().optional().describe("Require element to be enabled (default: false)"),
    },
    async ({ selector, timeout, visible, enabled }) => {
      try {
        const options = { timeout, visible, enabled };
        const escapedSelector = escapeForJS(selector);
        const result = await executeDebugCommand(
          `await window.__MCP_DEBUG__.smartActions.waitForElement("${escapedSelector}", ${JSON.stringify(options)})`
        );
        
        if (result?.found) {
          return { 
            content: [{ 
              type: "text", 
              text: `✅ Element found: ${selector} (${result.elapsed}ms)` 
            }] 
          };
        } else {
          return { 
            content: [{ 
              type: "text", 
              text: `❌ ${result?.error || 'Element not found'} (${result?.elapsed || 0}ms)` 
            }] 
          };
        }
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 等待文本出现
  server.tool(
    "wait_for_text",
    "Wait for specific text to appear on the page.",
    {
      text: z.string().describe("Text to wait for"),
      timeout: z.number().optional().describe("Timeout in milliseconds (default: 5000)"),
      exact: z.boolean().optional().describe("Require exact match (default: false, substring match)"),
    },
    async ({ text, timeout, exact }) => {
      try {
        const options = { timeout, exact };
        const escapedText = escapeForJS(text);
        const result = await executeDebugCommand(
          `await window.__MCP_DEBUG__.smartActions.waitForText("${escapedText}", ${JSON.stringify(options)})`
        );
        
        if (result?.found) {
          return { 
            content: [{ 
              type: "text", 
              text: `✅ Text found: "${text}" (${result.elapsed}ms)` 
            }] 
          };
        } else {
          return { 
            content: [{ 
              type: "text", 
              text: `❌ ${result?.error || 'Text not found'} (${result?.elapsed || 0}ms)` 
            }] 
          };
        }
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 按文本查找元素
  server.tool(
    "find_by_text",
    "Find elements by their text content. Returns matching elements with selectors.",
    {
      text: z.string().describe("Text to search for"),
      exact: z.boolean().optional().describe("Require exact match (default: false)"),
      tag: z.string().optional().describe("Limit to specific HTML tag (e.g., 'button', 'a')"),
    },
    async ({ text, exact, tag }) => {
      const options = { exact, tag };
      const escapedText = escapeForJS(text);
      const elements = await executeDebugCommand(
        `window.__MCP_DEBUG__.smartActions.findByText("${escapedText}", ${JSON.stringify(options)}).map(el => window.__MCP_DEBUG__.smartActions.getElementInfo(el))`
      );
      
      if (!elements || elements.length === 0) {
        return { content: [{ type: "text", text: `No elements found with text: "${text}"` }] };
      }
      
      const formatted = elements.slice(0, 10).map((el: any, i: number) => 
        `${i + 1}. <${el.tag}> "${el.text.substring(0, 50)}"\n   Selector: ${el.selector}`
      ).join('\n\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `Found ${elements.length} element(s):\n\n${formatted}` 
        }] 
      };
    }
  );

  // 按角色查找元素
  server.tool(
    "find_by_role",
    "Find elements by their ARIA role (button, link, textbox, checkbox, etc.).",
    {
      role: z.string().describe("ARIA role: button, link, textbox, checkbox, radio, heading, dialog, tab, menu, etc."),
      name: z.string().optional().describe("Filter by accessible name (text content, aria-label, title)"),
    },
    async ({ role, name }) => {
      const options = { name };
      const escapedRole = escapeForJS(role);
      const elements = await executeDebugCommand(
        `window.__MCP_DEBUG__.smartActions.findByRole("${escapedRole}", ${JSON.stringify(options)}).map(el => window.__MCP_DEBUG__.smartActions.getElementInfo(el))`
      );
      
      if (!elements || elements.length === 0) {
        return { content: [{ type: "text", text: `No elements found with role: ${role}${name ? ` and name: "${name}"` : ''}` }] };
      }
      
      const formatted = elements.slice(0, 10).map((el: any, i: number) => 
        `${i + 1}. <${el.tag}> "${el.text.substring(0, 50)}"\n   Selector: ${el.selector}`
      ).join('\n\n');
      
      return { 
        content: [{ 
          type: "text", 
          text: `Found ${elements.length} ${role}(s):\n\n${formatted}` 
        }] 
      };
    }
  );

  // 点击包含文本的元素
  server.tool(
    "click_text",
    "Click an element containing specific text. Waits for element and finds clickable ancestor.",
    {
      text: z.string().describe("Text of the element to click"),
      exact: z.boolean().optional().describe("Require exact text match (default: false)"),
      timeout: z.number().optional().describe("Timeout in milliseconds (default: 5000)"),
    },
    async ({ text, exact, timeout }) => {
      try {
        const options = { exact, timeout };
        const escapedText = escapeForJS(text);
        const result = await executeDebugCommand(
          `await window.__MCP_DEBUG__.smartActions.clickText("${escapedText}", ${JSON.stringify(options)})`
        );
        
        if (result?.success) {
          return { content: [{ type: "text", text: `✅ Clicked: "${text}"` }] };
        } else {
          return { content: [{ type: "text", text: `❌ Click failed: ${result?.error || 'Unknown error'}` }] };
        }
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 点击元素（按选择器）
  server.tool(
    "click_element",
    "Click an element by CSS selector. Waits for element to be visible and enabled.",
    {
      selector: z.string().describe("CSS selector for the element to click"),
      timeout: z.number().optional().describe("Timeout in milliseconds (default: 5000)"),
    },
    async ({ selector, timeout }) => {
      try {
        const options = { timeout };
        const escapedSelector = escapeForJS(selector);
        const result = await executeDebugCommand(
          `await window.__MCP_DEBUG__.smartActions.clickElement("${escapedSelector}", ${JSON.stringify(options)})`
        );
        
        if (result?.success) {
          return { content: [{ type: "text", text: `✅ Clicked: ${selector}` }] };
        } else {
          return { content: [{ type: "text", text: `❌ Click failed: ${result?.error || 'Unknown error'}` }] };
        }
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 填充输入框
  server.tool(
    "fill_input",
    "Fill an input field. Can find by selector, label, or placeholder.",
    {
      target: z.string().describe("CSS selector, label text, or placeholder text to identify the input"),
      value: z.string().describe("Value to fill"),
      clear: z.boolean().optional().describe("Clear existing value before filling (default: true)"),
      timeout: z.number().optional().describe("Timeout in milliseconds (default: 5000)"),
    },
    async ({ target, value, clear, timeout }) => {
      try {
        const options = { clear, timeout };
        const escapedTarget = escapeForJS(target);
        const escapedValue = escapeForJS(value);
        const result = await executeDebugCommand(
          `await window.__MCP_DEBUG__.smartActions.fillInput("${escapedTarget}", "${escapedValue}", ${JSON.stringify(options)})`
        );
        
        if (result?.success) {
          return { content: [{ type: "text", text: `✅ Filled "${target}" with "${value}"` }] };
        } else {
          return { content: [{ type: "text", text: `❌ Fill failed: ${result?.error || 'Unknown error'}` }] };
        }
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 导出录制的操作
  server.tool(
    "export_actions",
    "Export recorded actions as executable script (Playwright or JS format).",
    {
      format: z.enum(['playwright', 'js']).describe("Export format: 'playwright' or 'js'"),
    },
    async ({ format }) => {
      try {
        const actions = await executeDebugCommand("window.__MCP_DEBUG__.actionRecorder.get()");
        
        if (!actions || actions.length === 0) {
          return { content: [{ type: "text", text: "No actions recorded. Use debug_action_record first." }] };
        }
        
        const exportFn = format === 'playwright' ? 'exportToPlaywright' : 'exportToJS';
        const script = await executeDebugCommand(
          `window.__MCP_DEBUG__.smartActions.${exportFn}(window.__MCP_DEBUG__.actionRecorder.get())`
        );
        
        return { 
          content: [{ 
            type: "text", 
            text: `Exported ${actions.length} action(s) to ${format} format:\n\n\`\`\`${format === 'playwright' ? 'typescript' : 'javascript'}\n${script}\n\`\`\`` 
          }] 
        };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 获取页面元素概览 - 使用原生 DOM 实现，不依赖 __MCP_DEBUG__
  server.tool(
    "get_page_elements",
    "Get an overview of interactive elements on the page (buttons, links, inputs).",
    {
      type: z.enum(['all', 'buttons', 'links', 'inputs', 'forms']).optional().describe("Type of elements to list (default: all)"),
    },
    async ({ type = 'all' }) => {
      try {
        const script = `(() => {
          const getSelector = (el) => {
            if (el.id) return '#' + el.id;
            if (el.className && typeof el.className === 'string') {
              const cls = el.className.split(' ').filter(c => c && !c.includes(':'))[0];
              if (cls) return el.tagName.toLowerCase() + '.' + cls;
            }
            return el.tagName.toLowerCase();
          };
          
          const getInfo = (el) => ({
            tag: el.tagName.toLowerCase(),
            text: (el.textContent || '').trim().substring(0, 50),
            selector: getSelector(el)
          });
          
          const results = {};
          
          if ('${type}' === 'all' || '${type}' === 'buttons') {
            results.buttons = Array.from(document.querySelectorAll('button, [role="button"], input[type="button"], input[type="submit"]'))
              .filter(el => el.offsetParent !== null)
              .slice(0, 15)
              .map(getInfo);
          }
          if ('${type}' === 'all' || '${type}' === 'links') {
            results.links = Array.from(document.querySelectorAll('a[href]'))
              .filter(el => el.offsetParent !== null)
              .slice(0, 15)
              .map(getInfo);
          }
          if ('${type}' === 'all' || '${type}' === 'inputs') {
            results.inputs = Array.from(document.querySelectorAll('input:not([type="hidden"]):not([type="button"]):not([type="submit"]), textarea, [contenteditable="true"]'))
              .filter(el => el.offsetParent !== null)
              .slice(0, 15)
              .map(getInfo);
          }
          if ('${type}' === 'forms') {
            results.forms = Array.from(document.querySelectorAll('form'))
              .slice(0, 5)
              .map(getInfo);
          }
          
          return results;
        })()`;
        
        const elements = await executeRawJS(script);
        
        let output = "Page Elements Overview:\n";
        
        if (elements?.buttons?.length) {
          output += `\n📌 Buttons (${elements.buttons.length}):\n`;
          output += elements.buttons.map((el: any, i: number) => 
            `  ${i + 1}. "${el.text?.substring(0, 30) || '(no text)'}" → ${el.selector}`
          ).join('\n');
        }
        
        if (elements?.links?.length) {
          output += `\n\n🔗 Links (${elements.links.length}):\n`;
          output += elements.links.map((el: any, i: number) => 
            `  ${i + 1}. "${el.text?.substring(0, 30) || '(no text)'}" → ${el.selector}`
          ).join('\n');
        }
        
        if (elements?.inputs?.length) {
          output += `\n\n📝 Inputs (${elements.inputs.length}):\n`;
          output += elements.inputs.map((el: any, i: number) => 
            `  ${i + 1}. <${el.tag}> → ${el.selector}`
          ).join('\n');
        }
        
        if (elements?.forms?.length) {
          output += `\n\n📋 Forms (${elements.forms.length}):\n`;
          output += elements.forms.map((el: any, i: number) => 
            `  ${i + 1}. → ${el.selector}`
          ).join('\n');
        }
        
        if (!elements?.buttons?.length && !elements?.links?.length && !elements?.inputs?.length && !elements?.forms?.length) {
          output = "No interactive elements found on the page.";
        }
        
        return { content: [{ type: "text", text: output }] };
      } catch (e: any) {
        return { content: [{ type: "text", text: `❌ ${e.message}` }] };
      }
    }
  );

  // 页面导航
  server.tool(
    "navigate",
    "Navigate to a URL or use browser history (back/forward/reload).",
    {
      action: z.enum(['goto', 'back', 'forward', 'reload']).describe("Navigation action"),
      url: z.string().optional().describe("URL to navigate to (required for 'goto' action)"),
      waitUntil: z.enum(['load', 'domcontentloaded']).optional().describe("Wait until event (default: load)"),
      timeout: z.number().optional().describe("Wait timeout in milliseconds (default: 10000)"),
    },
    async ({ action, url, waitUntil = 'load', timeout }) => {
      let script: string;
      
      switch (action) {
        case 'goto':
          if (!url) {
            return { content: [{ type: "text", text: "❌ URL is required for 'goto' action" }] };
          }
          const escapedUrl = escapeForJS(url);
          script = `(() => {
            window.location.href = "${escapedUrl}";
            return { success: true, url: "${escapedUrl}" };
          })()`;
          break;
        case 'back':
          script = `(() => {
            window.history.back();
            return { success: true, action: 'back' };
          })()`;
          break;
        case 'forward':
          script = `(() => {
            window.history.forward();
            return { success: true, action: 'forward' };
          })()`;
          break;
        case 'reload':
          script = `(() => {
            window.location.reload();
            return { success: true, action: 'reload' };
          })()`;
          break;
        default:
          return { content: [{ type: "text", text: `❌ Unknown action: ${action}` }] };
      }
      
      await executeRawJS(script);

      const waitResult = await waitForReadyState(
        waitUntil === 'domcontentloaded' ? 'domcontentloaded' : 'load',
        "main",
        timeout ?? 10000
      );

      const statusPrefix = waitResult.state === 'timeout' ? '⚠️' : '✅';
      if (action === 'goto') {
        return { content: [{ type: "text", text: `${statusPrefix} Navigating to: ${url} (${waitResult.state}, ${waitResult.elapsed}ms)` }] };
      }
      return { content: [{ type: "text", text: `${statusPrefix} Navigation: ${action} (${waitResult.state}, ${waitResult.elapsed}ms)` }] };
    }
  );

  // 获取当前页面信息
  server.tool(
    "get_page_info",
    "Get current page URL, title, and basic info.",
    {},
    async () => {
      const info = await executeRawJS(`(() => {
        return {
          url: window.location.href,
          title: document.title,
          pathname: window.location.pathname,
          hash: window.location.hash,
          search: window.location.search,
          readyState: document.readyState,
          documentElement: {
            scrollHeight: document.documentElement.scrollHeight,
            scrollWidth: document.documentElement.scrollWidth,
            clientHeight: document.documentElement.clientHeight,
            clientWidth: document.documentElement.clientWidth,
          }
        };
      })()`);
      
      let output = `📄 Page Info:\n`;
      output += `  URL: ${info.url}\n`;
      output += `  Title: ${info.title}\n`;
      output += `  Path: ${info.pathname}\n`;
      if (info.hash) output += `  Hash: ${info.hash}\n`;
      if (info.search) output += `  Query: ${info.search}\n`;
      output += `  State: ${info.readyState}\n`;
      output += `  Viewport: ${info.documentElement.clientWidth}x${info.documentElement.clientHeight}\n`;
      output += `  Document: ${info.documentElement.scrollWidth}x${info.documentElement.scrollHeight}`;
      
      return { content: [{ type: "text", text: output }] };
    }
  );

  // 滚动页面
  server.tool(
    "scroll_page",
    "Scroll the page or an element.",
    {
      direction: z.enum(['up', 'down', 'top', 'bottom', 'to']).describe("Scroll direction or position"),
      amount: z.number().optional().describe("Scroll amount in pixels (for up/down)"),
      selector: z.string().optional().describe("Element selector to scroll (defaults to page)"),
      x: z.number().optional().describe("X position (for 'to' direction)"),
      y: z.number().optional().describe("Y position (for 'to' direction)"),
    },
    async ({ direction, amount = 300, selector, x, y }) => {
      const escapedSelector = selector ? escapeForJS(selector) : '';
      
      const script = `(() => {
        const target = ${selector ? `document.querySelector("${escapedSelector}")` : 'window'};
        if (!target) return { success: false, error: 'Element not found' };
        
        const scrollTarget = ${selector ? 'target' : 'window'};
        
        switch ('${direction}') {
          case 'up':
            scrollTarget.scrollBy({ top: -${amount}, behavior: 'smooth' });
            break;
          case 'down':
            scrollTarget.scrollBy({ top: ${amount}, behavior: 'smooth' });
            break;
          case 'top':
            scrollTarget.scrollTo({ top: 0, behavior: 'smooth' });
            break;
          case 'bottom':
            const maxScroll = ${selector ? 'target.scrollHeight' : 'document.documentElement.scrollHeight'};
            scrollTarget.scrollTo({ top: maxScroll, behavior: 'smooth' });
            break;
          case 'to':
            scrollTarget.scrollTo({ top: ${y || 0}, left: ${x || 0}, behavior: 'smooth' });
            break;
        }
        
        return { success: true, direction: '${direction}' };
      })()`;
      
      const result = await executeRawJS(script);
      
      if (result?.success) {
        return { content: [{ type: "text", text: `✅ Scrolled ${direction}${selector ? ` in ${selector}` : ''}` }] };
      } else {
        return { content: [{ type: "text", text: `❌ Scroll failed: ${result?.error || 'Unknown error'}` }] };
      }
    }
  );

  // 按键操作
  server.tool(
    "press_key",
    "Press a keyboard key or key combination.",
    {
      key: z.string().describe("Key to press (e.g., 'Enter', 'Escape', 'Tab', 'a', 'Control+a')"),
      selector: z.string().optional().describe("Element to focus before pressing (optional)"),
    },
    async ({ key, selector }) => {
      const escapedSelector = selector ? escapeForJS(selector) : '';
      const escapedKey = escapeForJS(key);
      
      const script = `(() => {
        let target = document.activeElement || document.body;
        
        ${selector ? `
        const el = document.querySelector("${escapedSelector}");
        if (!el) return { success: false, error: 'Element not found' };
        el.focus();
        target = el;
        ` : ''}
        
        const keyParts = "${escapedKey}".split('+');
        const mainKey = keyParts.pop();
        const modifiers = {
          ctrlKey: keyParts.includes('Control') || keyParts.includes('Ctrl'),
          shiftKey: keyParts.includes('Shift'),
          altKey: keyParts.includes('Alt'),
          metaKey: keyParts.includes('Meta') || keyParts.includes('Cmd'),
        };
        
        const event = new KeyboardEvent('keydown', {
          key: mainKey,
          code: mainKey.length === 1 ? 'Key' + mainKey.toUpperCase() : mainKey,
          bubbles: true,
          cancelable: true,
          ...modifiers
        });
        
        target.dispatchEvent(event);

        const keypress = new KeyboardEvent('keypress', {
          key: mainKey,
          code: mainKey.length === 1 ? 'Key' + mainKey.toUpperCase() : mainKey,
          bubbles: true,
          cancelable: true,
          ...modifiers
        });
        
        target.dispatchEvent(keypress);
        
        // Also dispatch keyup
        target.dispatchEvent(new KeyboardEvent('keyup', {
          key: mainKey,
          bubbles: true,
          ...modifiers
        }));
        
        return { success: true, key: "${escapedKey}" };
      })()`;
      
      const result = await executeRawJS(script);
      
      if (result.success) {
        return { content: [{ type: "text", text: `✅ Pressed: ${key}` }] };
      } else {
        return { content: [{ type: "text", text: `❌ Key press failed: ${result.error}` }] };
      }
    }
  );
}
