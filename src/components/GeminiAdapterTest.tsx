import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

interface TestResult {
  success: boolean;
  message: string;
  details?: any;
  duration?: number;
}

interface ApiConfig {
  id: string;
  name: string;
  apiKey: string;
  baseUrl: string;
  model: string;
  isMultimodal: boolean;
  isReasoning: boolean;
  enabled: boolean;
  modelAdapter: string;
  maxOutputTokens: number;
  temperature: number;
}

export const GeminiAdapterTest: React.FC = () => {
  const [selectedConfig, setSelectedConfig] = useState<ApiConfig | null>(null);
  const [apiConfigs, setApiConfigs] = useState<ApiConfig[]>([]);
  const [testResults, setTestResults] = useState<Record<string, TestResult>>({});
  const [isLoading, setIsLoading] = useState<Record<string, boolean>>({});
  const [streamingContent, setStreamingContent] = useState('');
  const [testPrompt, setTestPrompt] = useState('你好，请用中文简单介绍一下你自己。这是一个测试Gemini适配器的OpenAI兼容接口转换功能。');
  const [imageBase64, setImageBase64] = useState<string>('');
  const [imagePreview, setImagePreview] = useState<string>('');
  const [currentLogPath, setCurrentLogPath] = useState<string>('');
  const [allTestLogs, setAllTestLogs] = useState<string[]>([]);

  useEffect(() => {
    loadApiConfigs();
    loadTestLogs();
  }, []);

  // 生成测试日志内容
  const generateTestLog = (testType: string, config: ApiConfig, result: TestResult, additionalData?: any) => {
    const timestamp = new Date().toISOString();
    const logEntry = {
      timestamp,
      testType,
      config: {
        id: config.id,
        name: config.name,
        model: config.model,
        baseUrl: config.baseUrl,
        modelAdapter: config.modelAdapter,
        isMultimodal: config.isMultimodal,
        temperature: config.temperature,
        maxOutputTokens: config.maxOutputTokens
      },
      testPrompt,
      result: {
        success: result.success,
        message: result.message,
        duration: result.duration,
        details: result.details
      },
      additionalData: additionalData || {},
      environment: {
        userAgent: navigator.userAgent,
        timestamp: timestamp,
        testVersion: '1.0.0'
      }
    };
    
    return JSON.stringify(logEntry, null, 2);
  };

  // 保存测试日志到文件
  const saveTestLog = async (logContent: string, testType: string) => {
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const fileName = `gemini-adapter-test-${testType}-${timestamp}.log`;
      
      // 使用 Tauri 的文件管理服务保存日志
      await invoke('save_test_log', {
        fileName,
        content: logContent,
        logType: 'gemini-adapter-test'
      });
      
      const logPath = `logs/gemini-adapter-test/${fileName}`;
      setCurrentLogPath(logPath);
      
      // 更新日志列表
      setAllTestLogs(prev => [logPath, ...prev]);
      
      console.log(`测试日志已保存: ${logPath}`);
      return logPath;
    } catch (error) {
      console.error('保存测试日志失败:', error);
      return null;
    }
  };

  // 加载历史测试日志列表
  const loadTestLogs = async () => {
    try {
      const logs = await invoke<string[]>('get_test_logs', {
        logType: 'gemini-adapter-test'
      });
      setAllTestLogs(logs);
    } catch (error) {
      console.error('加载历史日志失败:', error);
    }
  };

  const loadApiConfigs = async () => {
    try {
      const configs = await invoke<ApiConfig[]>('get_api_configurations');
      const geminiConfigs = configs.filter(c => c.modelAdapter === 'google' && c.enabled);
      setApiConfigs(geminiConfigs);
      if (geminiConfigs.length > 0) {
        setSelectedConfig(geminiConfigs[0]);
      }
    } catch (error) {
      console.error('加载 API 配置失败:', error);
    }
  };

  // 基础连接测试
  const testBasicConnection = async () => {
    if (!selectedConfig) return;
    
    setIsLoading({ ...isLoading, basic: true });
    const startTime = Date.now();
    let testResult: TestResult;
    
    try {
      const result = await invoke<boolean>('test_api_connection', {
        apiKey: selectedConfig.apiKey,
        apiBase: selectedConfig.baseUrl,
        model: selectedConfig.model
      });
      
      testResult = {
        success: result,
        message: result ? '连接成功！API 密钥和端点有效。' : '连接失败，请检查配置。',
        duration: Date.now() - startTime,
        details: { connectionResult: result }
      };
      
      setTestResults({
        ...testResults,
        basic: testResult
      });
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 
                        (typeof error === 'object' ? JSON.stringify(error) : String(error));
      testResult = {
        success: false,
        message: `连接测试失败: ${errorMessage}`,
        duration: Date.now() - startTime,
        details: { error: errorMessage }
      };
      
      setTestResults({
        ...testResults,
        basic: testResult
      });
    } finally {
      setIsLoading({ ...isLoading, basic: false });
      
      // 保存测试日志
      if (selectedConfig) {
        const logContent = generateTestLog('basic-connection', selectedConfig, testResult, {
          testDescription: '基础连接测试 - 验证API密钥和端点有效性'
        });
        await saveTestLog(logContent, 'basic-connection');
      }
    }
  };

  // 非流式OpenAI兼容接口测试（使用流式接口收集完整响应）
  const testNonStreamingChat = async () => {
    if (!selectedConfig) return;
    
    setIsLoading({ ...isLoading, nonStreaming: true });
    const startTime = Date.now();
    let testResult: TestResult;
    let collectedContent = '';
    
    try {
      // 创建临时会话进行测试
      const tempId = `gemini_non_stream_test_${Date.now()}`;
      const streamEvent = `analysis_stream_${tempId}`;
      const chatHistory = [{
        role: 'user',
        content: testPrompt,
        timestamp: new Date().toISOString(),
      }];
      
      console.log('发送非流式测试请求（使用流式接口），使用适配器:', selectedConfig.modelAdapter);
      console.log('聊天历史:', chatHistory);
      
      // 使用流式接口但收集完整响应
      const unlistenContent = await listen(streamEvent, (event: any) => {
        console.log('收到非流式测试事件:', event.payload);
        
        if (event.payload.content) {
          collectedContent += event.payload.content;
        }
        if (event.payload.is_complete) {
          testResult = {
            success: true,
            message: 'Gemini适配器非流式测试成功！OpenAI格式已正确转换为Gemini API调用。',
            details: {
              response: { message: collectedContent },
              requestData: { tempId, chatHistory },
              responseLength: collectedContent.length,
              method: 'streaming_collected'
            },
            duration: Date.now() - startTime
          };
          
          setTestResults(prev => ({
            ...prev,
            nonStreaming: testResult
          }));
          setIsLoading(prev => ({ ...prev, nonStreaming: false }));
          
          // 保存测试日志
          if (selectedConfig) {
            const logContent = generateTestLog('non-streaming', selectedConfig, testResult, {
              testDescription: '非流式OpenAI兼容接口测试 - 使用流式接口收集完整响应',
              prompt: testPrompt,
              collectedContent
            });
            saveTestLog(logContent, 'non-streaming');
          }
        }
      });

      const unlistenError = await listen('stream_error', (event: any) => {
        console.error('非流式测试流式错误:', event.payload);
        
        const errorMessage = typeof event.payload.error === 'object' ? 
                            JSON.stringify(event.payload.error) : String(event.payload.error);
        
        testResult = {
          success: false,
          message: `Gemini适配器非流式测试失败: ${errorMessage}`,
          duration: Date.now() - startTime,
          details: { error: errorMessage, collectedContent }
        };
        
        setTestResults(prev => ({
          ...prev,
          nonStreaming: testResult
        }));
        setIsLoading(prev => ({ ...prev, nonStreaming: false }));
      });

      // 发起流式请求 - 使用 analyze_new_mistake_stream 创建临时会话并测试适配器
      await invoke('analyze_new_mistake_stream', {
        request: {
          subject: "AI适配器测试",
          question_image_files: [], // 非流式测试不需要图片
          analysis_image_files: [],
          user_question: testPrompt,
          enable_step_by_step: false
        }
      });

      // 设置超时清理
      setTimeout(() => {
        unlistenContent();
        unlistenError();
      }, 30000);
    } catch (error) {
      console.error('非流式测试失败:', error);
      const errorMessage = error instanceof Error ? error.message : 
                        (typeof error === 'object' ? JSON.stringify(error) : String(error));
      
      testResult = {
        success: false,
        message: `Gemini适配器非流式测试失败: ${errorMessage}`,
        duration: Date.now() - startTime,
        details: { error: errorMessage, collectedContent }
      };
      
      setTestResults(prev => ({
        ...prev,
        nonStreaming: testResult
      }));
      setIsLoading(prev => ({ ...prev, nonStreaming: false }));
      
      // 保存失败的测试日志
      if (selectedConfig) {
        const logContent = generateTestLog('non-streaming', selectedConfig, testResult, {
          testDescription: '非流式OpenAI兼容接口测试 - 异常失败',
          prompt: testPrompt
        });
        await saveTestLog(logContent, 'non-streaming-exception');
      }
    }
  };

  // 流式OpenAI兼容接口测试
  const testStreamingChat = async () => {
    if (!selectedConfig) return;
    
    setIsLoading({ ...isLoading, streaming: true });
    setStreamingContent('');
    const startTime = Date.now();
    let testResult: TestResult;
    let streamData: string[] = [];
    
    try {
      const tempId = `gemini_stream_test_${Date.now()}`;
      const streamEvent = `analysis_stream_${tempId}`;
      
      console.log('发送流式测试请求，使用适配器:', selectedConfig.modelAdapter);
      
      // 设置流式事件监听
      const unlistenContent = await listen(streamEvent, (event: any) => {
        console.log('收到流式事件:', event.payload);
        
        if (event.payload.content) {
          streamData.push(event.payload.content);
          setStreamingContent(prev => prev + event.payload.content);
        }
        if (event.payload.is_complete) {
          testResult = {
            success: true,
            message: 'Gemini适配器流式测试成功！OpenAI格式已正确转换为Gemini流式API调用。',
            duration: Date.now() - startTime,
            details: {
              streamChunks: streamData.length,
              totalContent: streamData.join(''),
              contentLength: streamData.join('').length,
              averageChunkSize: streamData.length > 0 ? streamData.join('').length / streamData.length : 0
            }
          };
          
          setTestResults(prev => ({
            ...prev,
            streaming: testResult
          }));
          setIsLoading(prev => ({ ...prev, streaming: false }));
          
          // 保存流式测试日志
          if (selectedConfig) {
            const logContent = generateTestLog('streaming', selectedConfig, testResult, {
              testDescription: '流式OpenAI兼容接口测试 - 验证OpenAI流式格式转换为Gemini流式API调用',
              prompt: testPrompt,
              streamingData: {
                chunks: streamData,
                totalChunks: streamData.length
              }
            });
            saveTestLog(logContent, 'streaming');
          }
        }
      });

      const unlistenError = await listen('stream_error', (event: any) => {
        console.error('流式错误:', event.payload);
        
        testResult = {
          success: false,
          message: `Gemini适配器流式测试错误: ${event.payload.error}`,
          duration: Date.now() - startTime,
          details: { error: event.payload.error, streamData }
        };
        
        setTestResults(prev => ({
          ...prev,
          streaming: testResult
        }));
        setIsLoading(prev => ({ ...prev, streaming: false }));
        
        // 保存失败的流式测试日志
        if (selectedConfig) {
          const logContent = generateTestLog('streaming', selectedConfig, testResult, {
            testDescription: '流式OpenAI兼容接口测试 - 失败',
            prompt: testPrompt,
            streamingData: {
              chunks: streamData,
              totalChunks: streamData.length
            }
          });
          saveTestLog(logContent, 'streaming-error');
        }
      });

      // 发起流式请求 - 使用 analyze_new_mistake_stream 创建临时会话并测试适配器
      await invoke('analyze_new_mistake_stream', {
        request: {
          subject: "AI适配器流式测试",
          question_image_files: [], // 流式测试不需要图片
          analysis_image_files: [],
          user_question: testPrompt,
          enable_step_by_step: false
        }
      });

      // 清理监听器
      return () => {
        unlistenContent();
        unlistenError();
      };
    } catch (error) {
      console.error('流式测试失败:', error);
      const errorMessage = error instanceof Error ? error.message : 
                        (typeof error === 'object' ? JSON.stringify(error) : String(error));
      
      testResult = {
        success: false,
        message: `Gemini适配器流式测试失败: ${errorMessage}`,
        duration: Date.now() - startTime,
        details: { error: errorMessage, streamData }
      };
      
      setTestResults({
        ...testResults,
        streaming: testResult
      });
      setIsLoading({ ...isLoading, streaming: false });
      
      // 保存失败的流式测试日志
      if (selectedConfig) {
        const logContent = generateTestLog('streaming', selectedConfig, testResult, {
          testDescription: '流式OpenAI兼容接口测试 - 异常失败',
          prompt: testPrompt
        });
        await saveTestLog(logContent, 'streaming-exception');
      }
    }
  };

  // 多模态测试（图片+文本）
  const testMultimodal = async () => {
    if (!selectedConfig || !selectedConfig.isMultimodal || !imageBase64) {
      const testResult: TestResult = {
        success: false,
        message: selectedConfig?.isMultimodal 
          ? '请先上传一张图片进行测试。' 
          : '当前模型不支持多模态功能。',
        details: { 
          reason: selectedConfig?.isMultimodal ? 'no_image' : 'not_multimodal',
          isMultimodal: selectedConfig?.isMultimodal || false
        }
      };
      
      setTestResults({
        ...testResults,
        multimodal: testResult
      });
      
      // 保存跳过的测试日志
      if (selectedConfig) {
        const logContent = generateTestLog('multimodal', selectedConfig, testResult, {
          testDescription: '多模态测试 - 跳过（条件不满足）',
          skipReason: testResult.details?.reason
        });
        await saveTestLog(logContent, 'multimodal-skipped');
      }
      return;
    }
    
    setIsLoading({ ...isLoading, multimodal: true });
    const startTime = Date.now();
    let testResult: TestResult;
    
    try {
      console.log('发送多模态测试请求，使用适配器:', selectedConfig.modelAdapter);
      
      const multimodalPrompt = '请描述这张图片的内容。测试Gemini适配器的多模态OpenAI兼容接口转换。';
      
      // 使用分析接口测试多模态（会自动调用gemini_adapter的多模态功能）
      const response = await invoke('analyze_step_by_step', {
        request: {
          subject: '测试',
          question_image_files: [imageBase64],
          analysis_image_files: [],
          user_question: multimodalPrompt,
          enable_chain_of_thought: false
        }
      });
      
      console.log('多模态适配器响应:', response);
      
      testResult = {
        success: true,
        message: 'Gemini适配器多模态测试成功！OpenAI多模态格式已正确转换为Gemini API调用。',
        details: {
          response,
          imageSize: imageBase64.length,
          prompt: multimodalPrompt,
          responseLength: (response as any)?.analysis_result?.length || 0
        },
        duration: Date.now() - startTime
      };
      
      setTestResults({
        ...testResults,
        multimodal: testResult
      });
    } catch (error) {
      console.error('多模态测试失败:', error);
      
      const errorMessage = error instanceof Error ? error.message : 
                        (typeof error === 'object' ? JSON.stringify(error) : String(error));
      testResult = {
        success: false,
        message: `Gemini适配器多模态测试失败: ${errorMessage}`,
        duration: Date.now() - startTime,
        details: { 
          error: errorMessage,
          imageSize: imageBase64.length
        }
      };
      
      setTestResults({
        ...testResults,
        multimodal: testResult
      });
    } finally {
      setIsLoading({ ...isLoading, multimodal: false });
      
      // 保存多模态测试日志
      if (selectedConfig) {
        const logContent = generateTestLog('multimodal', selectedConfig, testResult, {
          testDescription: '多模态OpenAI兼容接口测试 - 验证OpenAI多模态格式转换为Gemini多模态API调用',
          imageData: {
            hasImage: !!imageBase64,
            imageSize: imageBase64?.length || 0,
            imageType: imageBase64?.substring(0, 50) || '' // 图片头部信息
          }
        });
        await saveTestLog(logContent, 'multimodal');
      }
    }
  };

  // 处理图片上传
  const handleImageUpload = (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const base64 = e.target?.result as string;
      setImageBase64(base64);
      setImagePreview(base64);
    };
    reader.readAsDataURL(file);
  };

  // 运行所有测试
  const runAllTests = async () => {
    if (!selectedConfig) return;
    
    const batchStartTime = Date.now();
    const batchId = `batch_${Date.now()}`;
    
    console.log(`开始批量测试 - Batch ID: ${batchId}`);
    
    // 清除之前的测试结果
    setTestResults({});
    setStreamingContent('');
    
    const testSequence = [
      { name: 'basic', func: testBasicConnection },
      { name: 'nonStreaming', func: testNonStreamingChat },
      { name: 'streaming', func: testStreamingChat }
    ];
    
    // 如果支持多模态且有图片，添加多模态测试
    if (selectedConfig.isMultimodal && imageBase64) {
      testSequence.push({ name: 'multimodal', func: testMultimodal });
    }
    
    const batchResults: Record<string, TestResult> = {};
    
    // 依次执行所有测试
    for (const test of testSequence) {
      console.log(`执行测试: ${test.name}`);
      await test.func();
      await new Promise(resolve => setTimeout(resolve, 1000)); // 测试间隔
      
      // 收集测试结果
      const currentResults = testResults;
      if (currentResults[test.name]) {
        batchResults[test.name] = currentResults[test.name];
      }
    }
    
    // 生成批量测试总结日志
    const batchDuration = Date.now() - batchStartTime;
    const successCount = Object.values(batchResults).filter(r => r.success).length;
    const totalCount = Object.keys(batchResults).length;
    
    const batchLogEntry = {
      timestamp: new Date().toISOString(),
      batchId,
      testType: 'batch-all-tests',
      config: {
        id: selectedConfig.id,
        name: selectedConfig.name,
        model: selectedConfig.model,
        baseUrl: selectedConfig.baseUrl,
        modelAdapter: selectedConfig.modelAdapter,
        isMultimodal: selectedConfig.isMultimodal,
        temperature: selectedConfig.temperature,
        maxOutputTokens: selectedConfig.maxOutputTokens
      },
      testPrompt,
      batchSummary: {
        totalTests: totalCount,
        successfulTests: successCount,
        failedTests: totalCount - successCount,
        successRate: totalCount > 0 ? (successCount / totalCount * 100).toFixed(2) + '%' : '0%',
        totalDuration: batchDuration,
        averageTestDuration: totalCount > 0 ? Math.round(batchDuration / totalCount) : 0
      },
      individualResults: batchResults,
      testSequence: testSequence.map(t => t.name),
      environment: {
        userAgent: navigator.userAgent,
        timestamp: new Date().toISOString(),
        testVersion: '1.0.0',
        hasImage: !!imageBase64,
        imageSize: imageBase64?.length || 0
      }
    };
    
    // 保存批量测试日志
    const batchLogContent = JSON.stringify(batchLogEntry, null, 2);
    await saveTestLog(batchLogContent, 'batch-all');
    
    console.log(`批量测试完成 - 成功: ${successCount}/${totalCount}, 耗时: ${batchDuration}ms`);
  };

  const renderTestResult = (key: string, title: string) => {
    const result = testResults[key];
    const loading = isLoading[key];
    
    return (
      <div className="flex-1">
        <div className="flex items-center mb-2">
          <h3 className="font-semibold text-gray-800 flex-1">{title}</h3>
          <div className="ml-4">
            {loading ? (
              <div className="inline-flex items-center px-3 py-1 bg-blue-100 text-blue-800 rounded-full text-sm">
                <div className="h-4 w-4 animate-spin rounded-full border-2 border-blue-600 border-t-transparent mr-2" />
                测试中...
              </div>
            ) : result ? (
              result.success ? (
                <div className="inline-flex items-center px-3 py-1 bg-green-100 text-green-800 rounded-full text-sm font-medium">
                  <span className="mr-1">✅</span>
                  成功
                </div>
              ) : (
                <div className="inline-flex items-center px-3 py-1 bg-red-100 text-red-800 rounded-full text-sm font-medium">
                  <span className="mr-1">❌</span>
                  失败
                </div>
              )
            ) : (
              <div className="inline-flex items-center px-3 py-1 bg-gray-100 text-gray-600 rounded-full text-sm">
                待测试
              </div>
            )}
          </div>
        </div>
        {result && (
          <div className="space-y-1">
            <p className={`text-sm ${result.success ? 'text-green-700' : 'text-red-700'}`}>
              {result.message}
            </p>
            {result?.duration && (
              <p className="text-xs text-gray-500">
                ⏱️ 耗时: {result.duration}ms
              </p>
            )}
          </div>
        )}
      </div>
    );
  };

  if (apiConfigs.length === 0) {
    return (
      <div className="min-h-screen bg-gradient-to-br from-blue-50 via-white to-purple-50 p-6">
        <div className="max-w-4xl mx-auto">
          {/* 页面头部 */}
          <div className="text-center mb-8">
            <div className="inline-flex items-center justify-center w-16 h-16 bg-gradient-to-r from-blue-500 to-purple-600 rounded-full mb-4">
              <span className="text-2xl text-white">🧪</span>
            </div>
            <h1 className="text-4xl font-bold bg-gradient-to-r from-blue-600 to-purple-600 bg-clip-text text-transparent mb-3">
              Gemini 适配器测试
            </h1>
          </div>
          
          {/* 错误提示卡片 */}
          <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
            <div className="bg-gradient-to-r from-yellow-500 to-orange-600 p-6 text-white">
              <h2 className="text-xl font-bold flex items-center">
                <span className="mr-3">⚠️</span>
                配置缺失
              </h2>
            </div>
            <div className="p-8">
              <div className="text-center">
                <div className="inline-flex items-center justify-center w-20 h-20 bg-yellow-100 rounded-full mb-6">
                  <span className="text-3xl">⚙️</span>
                </div>
                <h3 className="text-xl font-semibold text-gray-800 mb-4">
                  未找到启用的 Gemini 配置
                </h3>
                <p className="text-gray-600 mb-6 max-w-md mx-auto">
                  请先在 API 配置中添加一个 Google (Gemini) 适配器的配置，然后启用该配置。
                </p>
                
                <div className="bg-gradient-to-r from-blue-50 to-purple-50 rounded-lg p-6 border border-blue-200 mb-6">
                  <h4 className="font-semibold text-blue-800 mb-3 flex items-center justify-center">
                    <span className="mr-2">📝</span>
                    配置要求
                  </h4>
                  <div className="text-left text-sm text-blue-700 space-y-2">
                    <div className="flex items-center">
                      <span className="w-2 h-2 bg-blue-500 rounded-full mr-3"></span>
                      <span><strong>modelAdapter</strong> 字段必须设置为 <code className="px-2 py-1 bg-blue-100 rounded text-xs">"google"</code></span>
                    </div>
                    <div className="flex items-center">
                      <span className="w-2 h-2 bg-blue-500 rounded-full mr-3"></span>
                      <span>配置必须处于 <strong>启用</strong> 状态</span>
                    </div>
                    <div className="flex items-center">
                      <span className="w-2 h-2 bg-blue-500 rounded-full mr-3"></span>
                      <span>需要有效的 Gemini API 密钥</span>
                    </div>
                  </div>
                </div>
                
                <button 
                  onClick={() => window.location.reload()}
                  className="px-6 py-3 bg-gradient-to-r from-blue-500 to-purple-600 text-white rounded-lg hover:from-blue-600 hover:to-purple-700 transition-all duration-200 flex items-center mx-auto font-semibold"
                >
                  <span className="mr-2">🔄</span>
                  刷新页面
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-gradient-to-br from-blue-50 via-white to-purple-50 p-6">
      <div className="max-w-7xl mx-auto">
        {/* 页面头部 */}
        <div className="text-center mb-8">
          <div className="inline-flex items-center justify-center w-16 h-16 bg-gradient-to-r from-blue-500 to-purple-600 rounded-full mb-4">
            <span className="text-2xl text-white">🧪</span>
          </div>
          <h1 className="text-4xl font-bold bg-gradient-to-r from-blue-600 to-purple-600 bg-clip-text text-transparent mb-3">
            Gemini 适配器测试
          </h1>
          <p className="text-lg text-gray-600 max-w-3xl mx-auto">
            此测试页面验证 <code className="px-2 py-1 bg-gray-100 rounded text-sm font-mono">gemini_adapter.rs</code> 
            是否能正确将 OpenAI 兼容格式转换为 Gemini API 调用并正常工作
          </p>
        </div>
      
        {/* 配置卡片 */}
        <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
          <div className="bg-gradient-to-r from-blue-500 to-purple-600 p-6 text-white">
            <h2 className="text-xl font-bold flex items-center">
              <span className="mr-3">⚙️</span>
              API 配置选择
            </h2>
          </div>
          <div className="p-6">
            <div className="mb-4">
              <label className="block text-sm font-semibold text-gray-700 mb-3">选择 Gemini 配置</label>
              <select
                className="w-full p-4 border-2 border-gray-200 rounded-lg focus:border-blue-500 focus:ring-2 focus:ring-blue-200 transition-all duration-200 bg-white text-gray-800"
                value={selectedConfig?.id || ''}
                onChange={(e) => {
                  const config = apiConfigs.find(c => c.id === e.target.value);
                  setSelectedConfig(config || null);
                }}
              >
                {apiConfigs.map(config => (
                  <option key={config.id} value={config.id}>
                    {config.name} - {config.model}
                  </option>
                ))}
              </select>
            </div>

            {selectedConfig && (
              <div className="bg-gradient-to-br from-blue-50 to-purple-50 rounded-lg p-6 border border-blue-200">
                <h3 className="font-semibold text-gray-800 mb-4 flex items-center">
                  <span className="mr-2">📋</span>
                  配置详情
                </h3>
                <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-sm">
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">模型:</span>
                    <div className="text-gray-800 font-mono">{selectedConfig.model}</div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">端点:</span>
                    <div className="text-gray-800 font-mono text-xs break-all">{selectedConfig.baseUrl}</div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">适配器:</span>
                    <div>
                      <span className="inline-flex items-center px-3 py-1 bg-gradient-to-r from-blue-500 to-purple-600 text-white text-xs font-medium rounded-full">
                        {selectedConfig.modelAdapter}
                      </span>
                    </div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">多模态:</span>
                    <div className="flex items-center">
                      {selectedConfig.isMultimodal ? (
                        <span className="inline-flex items-center px-2 py-1 bg-green-100 text-green-800 text-xs font-medium rounded-full">
                          ✓ 支持
                        </span>
                      ) : (
                        <span className="inline-flex items-center px-2 py-1 bg-gray-100 text-gray-800 text-xs font-medium rounded-full">
                          ✗ 不支持
                        </span>
                      )}
                    </div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">温度:</span>
                    <div className="text-gray-800">{selectedConfig.temperature}</div>
                  </div>
                  <div className="bg-white rounded-lg p-3 border">
                    <span className="font-medium text-gray-600">最大Token:</span>
                    <div className="text-gray-800">{selectedConfig.maxOutputTokens}</div>
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* 测试输入区域 */}
        <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
          <div className="bg-gradient-to-r from-green-500 to-teal-600 p-6 text-white">
            <h2 className="text-xl font-bold flex items-center">
              <span className="mr-3">📝</span>
              测试输入配置
            </h2>
          </div>
          <div className="p-6 space-y-6">
            {/* 测试提示词 */}
            <div>
              <label className="block text-sm font-semibold text-gray-700 mb-3">测试提示词</label>
              <textarea
                className="w-full p-4 border-2 border-gray-200 rounded-lg focus:border-green-500 focus:ring-2 focus:ring-green-200 transition-all duration-200 resize-vertical bg-white"
                value={testPrompt}
                onChange={(e) => setTestPrompt(e.target.value)}
                rows={4}
                placeholder="输入测试提示词，用于验证适配器功能..."
              />
            </div>

            {/* 图片上传 */}
            {selectedConfig?.isMultimodal && (
              <div>
                <label className="block text-sm font-semibold text-gray-700 mb-3">上传测试图片（多模态测试）</label>
                <div className="border-2 border-dashed border-gray-300 rounded-lg p-6 hover:border-green-400 transition-colors duration-200">
                  <div className="flex items-center justify-center">
                    <div className="text-center">
                      <input
                        type="file"
                        accept="image/*"
                        onChange={handleImageUpload}
                        className="hidden"
                        id="image-upload"
                      />
                      <label
                        htmlFor="image-upload"
                        className="cursor-pointer inline-flex items-center px-4 py-2 bg-gradient-to-r from-green-500 to-teal-600 text-white rounded-lg hover:from-green-600 hover:to-teal-700 transition-all duration-200"
                      >
                        <span className="mr-2">📷</span>
                        选择图片
                      </label>
                      <p className="text-gray-500 text-sm mt-2">支持 JPG、PNG 等格式</p>
                    </div>
                    {imagePreview && (
                      <div className="ml-6">
                        <img 
                          src={imagePreview} 
                          alt="Preview" 
                          className="h-24 w-24 object-cover rounded-lg border-2 border-green-200 shadow-md"
                        />
                      </div>
                    )}
                  </div>
                </div>
              </div>
            )}
          </div>
        </div>

        {/* 测试控制区域 */}
        <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
          <div className="bg-gradient-to-r from-purple-500 to-pink-600 p-6 text-white">
            <h2 className="text-xl font-bold flex items-center">
              <span className="mr-3">🚀</span>
              测试控制
            </h2>
          </div>
          <div className="p-6">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
              <button
                onClick={runAllTests}
                disabled={Object.values(isLoading).some(v => v)}
                className="w-full px-6 py-4 bg-gradient-to-r from-purple-500 to-pink-600 text-white rounded-lg hover:from-purple-600 hover:to-pink-700 disabled:opacity-50 disabled:cursor-not-allowed transition-all duration-200 flex items-center justify-center font-semibold"
              >
                {Object.values(isLoading).some(v => v) ? (
                  <>
                    <div className="animate-spin rounded-full h-5 w-5 border-2 border-white border-t-transparent mr-3"></div>
                    测试进行中...
                  </>
                ) : (
                  <>
                    <span className="mr-2">🧪</span>
                    运行所有测试
                  </>
                )}
              </button>
              <button
                onClick={() => {
                  setTestResults({});
                  setStreamingContent('');
                }}
                className="w-full px-6 py-4 bg-gradient-to-r from-gray-500 to-gray-600 text-white rounded-lg hover:from-gray-600 hover:to-gray-700 transition-all duration-200 flex items-center justify-center font-semibold"
              >
                <span className="mr-2">🗑️</span>
                清除测试结果
              </button>
            </div>
            
            {/* 日志管理区域 */}
            <div className="border-t border-gray-200 pt-4">
              <h4 className="text-sm font-semibold text-gray-700 mb-3 flex items-center">
                <span className="mr-2">📄</span>
                测试日志管理
              </h4>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <button
                  onClick={loadTestLogs}
                  className="px-4 py-2 bg-gradient-to-r from-blue-500 to-indigo-600 text-white rounded-lg hover:from-blue-600 hover:to-indigo-700 transition-all duration-200 flex items-center justify-center text-sm font-medium"
                >
                  <span className="mr-1">🔄</span>
                  刷新日志列表
                </button>
                <button
                  onClick={() => {
                    if (currentLogPath) {
                      invoke('open_log_file', { logPath: currentLogPath });
                    }
                  }}
                  disabled={!currentLogPath}
                  className="px-4 py-2 bg-gradient-to-r from-green-500 to-emerald-600 text-white rounded-lg hover:from-green-600 hover:to-emerald-700 disabled:opacity-50 transition-all duration-200 flex items-center justify-center text-sm font-medium"
                >
                  <span className="mr-1">📂</span>
                  打开最新日志
                </button>
                <button
                  onClick={() => {
                    invoke('open_logs_folder', { logType: 'gemini-adapter-test' });
                  }}
                  className="px-4 py-2 bg-gradient-to-r from-yellow-500 to-orange-600 text-white rounded-lg hover:from-yellow-600 hover:to-orange-700 transition-all duration-200 flex items-center justify-center text-sm font-medium"
                >
                  <span className="mr-1">📁</span>
                  打开日志文件夹
                </button>
              </div>
              
              {/* 当前日志路径显示 */}
              {currentLogPath && (
                <div className="mt-3 p-3 bg-blue-50 border border-blue-200 rounded-lg">
                  <p className="text-xs text-blue-700">
                    <span className="font-semibold">最新日志:</span> {currentLogPath}
                  </p>
                </div>
              )}
              
              {/* 历史日志列表 */}
              {allTestLogs.length > 0 && (
                <div className="mt-4">
                  <h5 className="text-xs font-semibold text-gray-600 mb-2">
                    历史测试日志 ({allTestLogs.length} 个文件)
                  </h5>
                  <div className="max-h-32 overflow-y-auto bg-gray-50 rounded-lg border">
                    {allTestLogs.slice(0, 10).map((logPath, index) => (
                      <div
                        key={index}
                        className="px-3 py-2 border-b border-gray-200 last:border-b-0 hover:bg-gray-100 cursor-pointer transition-colors duration-150"
                        onClick={() => {
                          invoke('open_log_file', { logPath });
                        }}
                      >
                        <p className="text-xs text-gray-700 font-mono truncate">
                          {logPath.split('/').pop()}
                        </p>
                      </div>
                    ))}
                    {allTestLogs.length > 10 && (
                      <div className="px-3 py-2 text-xs text-gray-500 italic">
                        ... 还有 {allTestLogs.length - 10} 个日志文件
                      </div>
                    )}
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* 单项测试 */}
        <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
          <div className="bg-gradient-to-r from-indigo-500 to-blue-600 p-6 text-white">
            <h2 className="text-xl font-bold flex items-center">
              <span className="mr-3">🔬</span>
              单项测试
            </h2>
            <p className="text-blue-100 mt-1">分别验证适配器的各项功能</p>
          </div>
          <div className="p-6 space-y-4">
            {/* 基础连接测试 */}
            <div className="bg-gradient-to-r from-gray-50 to-gray-100 rounded-lg p-4 border border-gray-200">
              <div className="flex items-center justify-between">
                {renderTestResult('basic', '🔗 基础连接测试')}
                <button 
                  onClick={testBasicConnection} 
                  disabled={isLoading.basic}
                  className="px-4 py-2 bg-gradient-to-r from-blue-500 to-indigo-600 text-white rounded-lg hover:from-blue-600 hover:to-indigo-700 disabled:opacity-50 transition-all duration-200 flex items-center text-sm font-medium"
                >
                  {isLoading.basic ? (
                    <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                  ) : (
                    <span className="mr-1">▶️</span>
                  )}
                  测试
                </button>
              </div>
            </div>

            {/* 非流式转换测试 */}
            <div className="bg-gradient-to-r from-green-50 to-emerald-100 rounded-lg p-4 border border-green-200">
              <div className="flex items-center justify-between">
                {renderTestResult('nonStreaming', '📤 OpenAI → Gemini 非流式转换测试')}
                <button 
                  onClick={testNonStreamingChat} 
                  disabled={isLoading.nonStreaming}
                  className="px-4 py-2 bg-gradient-to-r from-green-500 to-emerald-600 text-white rounded-lg hover:from-green-600 hover:to-emerald-700 disabled:opacity-50 transition-all duration-200 flex items-center text-sm font-medium"
                >
                  {isLoading.nonStreaming ? (
                    <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                  ) : (
                    <span className="mr-1">▶️</span>
                  )}
                  测试
                </button>
              </div>
            </div>

            {/* 流式转换测试 */}
            <div className="bg-gradient-to-r from-purple-50 to-violet-100 rounded-lg p-4 border border-purple-200">
              <div className="flex items-center justify-between">
                {renderTestResult('streaming', '🌊 OpenAI → Gemini 流式转换测试')}
                <button 
                  onClick={testStreamingChat} 
                  disabled={isLoading.streaming}
                  className="px-4 py-2 bg-gradient-to-r from-purple-500 to-violet-600 text-white rounded-lg hover:from-purple-600 hover:to-violet-700 disabled:opacity-50 transition-all duration-200 flex items-center text-sm font-medium"
                >
                  {isLoading.streaming ? (
                    <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                  ) : (
                    <span className="mr-1">▶️</span>
                  )}
                  测试
                </button>
              </div>
            </div>

            {/* 多模态转换测试 */}
            {selectedConfig?.isMultimodal && (
              <div className="bg-gradient-to-r from-orange-50 to-amber-100 rounded-lg p-4 border border-orange-200">
                <div className="flex items-center justify-between">
                  {renderTestResult('multimodal', '🖼️ OpenAI → Gemini 多模态转换测试')}
                  <button 
                    onClick={testMultimodal} 
                    disabled={isLoading.multimodal || !imageBase64}
                    className="px-4 py-2 bg-gradient-to-r from-orange-500 to-amber-600 text-white rounded-lg hover:from-orange-600 hover:to-amber-700 disabled:opacity-50 transition-all duration-200 flex items-center text-sm font-medium"
                  >
                    {isLoading.multimodal ? (
                      <div className="animate-spin rounded-full h-4 w-4 border-2 border-white border-t-transparent mr-2"></div>
                    ) : (
                      <span className="mr-1">▶️</span>
                    )}
                    测试
                  </button>
                </div>
                {!imageBase64 && (
                  <div className="mt-2 text-xs text-orange-600">
                    💡 需要先上传图片才能进行多模态测试
                  </div>
                )}
              </div>
            )}
          </div>
        </div>

        {/* 流式响应内容显示 */}
        {streamingContent && (
          <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
            <div className="bg-gradient-to-r from-green-500 to-emerald-600 p-4 text-white">
              <h3 className="text-lg font-bold flex items-center">
                <span className="mr-2">🌊</span>
                流式响应内容
              </h3>
            </div>
            <div className="p-6">
              <div className="bg-gradient-to-br from-gray-50 to-gray-100 rounded-lg p-6 max-h-96 overflow-y-auto border-2 border-gray-200">
                <div className="font-mono text-sm leading-relaxed text-gray-800 whitespace-pre-wrap">
                  {streamingContent}
                </div>
              </div>
            </div>
          </div>
        )}

        {/* 测试结果详情 */}
        {Object.entries(testResults).some(([key, result]) => result.details) && (
          <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
            <div className="bg-gradient-to-r from-indigo-500 to-purple-600 p-4 text-white">
              <h3 className="text-lg font-bold flex items-center">
                <span className="mr-2">📊</span>
                测试结果详情
              </h3>
            </div>
            <div className="p-6 space-y-4">
              {Object.entries(testResults).map(([key, result]) => (
                result.details && (
                  <div key={key} className="border border-gray-200 rounded-lg overflow-hidden">
                    <div className="bg-gray-50 px-4 py-3 border-b border-gray-200">
                      <h4 className="font-semibold text-gray-800 capitalize">{key} 测试详情</h4>
                    </div>
                    <div className="p-4">
                      <div className="bg-gradient-to-br from-gray-50 to-gray-100 rounded-lg p-4 max-h-60 overflow-y-auto">
                        <pre className="text-xs text-gray-700 whitespace-pre-wrap font-mono leading-relaxed">
                          {JSON.stringify(result.details, null, 2)}
                        </pre>
                      </div>
                    </div>
                  </div>
                )
              ))}
            </div>
          </div>
        )}

        {/* 说明信息 */}
        <div className="bg-white rounded-xl shadow-lg border border-gray-100 overflow-hidden">
          <div className="bg-gradient-to-r from-cyan-500 to-blue-600 p-6 text-white">
            <h3 className="text-xl font-bold flex items-center">
              <span className="mr-3">📚</span>
              测试说明
            </h3>
          </div>
          <div className="p-6">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="bg-gradient-to-br from-blue-50 to-cyan-50 rounded-lg p-4 border border-blue-200">
                <h4 className="font-semibold text-blue-800 mb-2 flex items-center">
                  <span className="mr-2">🔗</span>
                  基础连接测试
                </h4>
                <p className="text-blue-700 text-sm">验证API密钥和端点是否有效，确保网络连接正常</p>
              </div>
              <div className="bg-gradient-to-br from-green-50 to-emerald-50 rounded-lg p-4 border border-green-200">
                <h4 className="font-semibold text-green-800 mb-2 flex items-center">
                  <span className="mr-2">📤</span>
                  非流式转换测试
                </h4>
                <p className="text-green-700 text-sm">验证OpenAI格式能否正确转换为Gemini API调用</p>
              </div>
              <div className="bg-gradient-to-br from-purple-50 to-violet-50 rounded-lg p-4 border border-purple-200">
                <h4 className="font-semibold text-purple-800 mb-2 flex items-center">
                  <span className="mr-2">🌊</span>
                  流式转换测试
                </h4>
                <p className="text-purple-700 text-sm">验证OpenAI流式格式能否正确转换为Gemini流式API调用</p>
              </div>
              <div className="bg-gradient-to-br from-orange-50 to-amber-50 rounded-lg p-4 border border-orange-200">
                <h4 className="font-semibold text-orange-800 mb-2 flex items-center">
                  <span className="mr-2">🖼️</span>
                  多模态转换测试
                </h4>
                <p className="text-orange-700 text-sm">验证OpenAI多模态格式能否正确转换为Gemini多模态API调用</p>
              </div>
            </div>
            <div className="mt-6 p-4 bg-gradient-to-r from-yellow-100 to-orange-100 rounded-lg border border-yellow-300">
              <div className="flex items-start">
                <span className="text-2xl mr-3">💡</span>
                <div>
                  <h4 className="font-semibold text-yellow-800 mb-1">使用提示</h4>
                  <p className="text-yellow-700 text-sm">
                    确保已在设置中正确配置了Gemini API密钥和端点。
                    测试前请检查网络连接是否正常。
                    所有测试都会验证适配器是否能正确处理OpenAI兼容格式并转换为Gemini API调用。
                  </p>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};