import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface TestResult {
  name: string;
  status: 'pending' | 'success' | 'error';
  message: string;
  duration?: number;
}

interface ApiTest {
  name: string;
  command: string;
  params?: any;
  description: string;
}

const BackendTest: React.FC = () => {
  const [testResults, setTestResults] = useState<TestResult[]>([]);
  const [isRunning, setIsRunning] = useState(false);
  const [isTauriAvailable, setIsTauriAvailable] = useState(false);
  const [testContext, setTestContext] = useState<{
    tempId?: string;
    mistakeId?: string;
    reviewId?: string;
  }>({});

  // 定义所有API测试
  const apiTests: ApiTest[] = [
    // 基础API测试
    {
      name: '获取支持的科目',
      command: 'get_supported_subjects',
      description: '测试获取系统支持的科目列表'
    },
    {
      name: '获取统计信息',
      command: 'get_statistics',
      description: '测试获取错题统计数据'
    },
    
    // 设置管理API
    {
      name: '保存设置',
      command: 'save_setting',
      params: { key: 'test_key', value: 'test_value_' + Date.now() },
      description: '测试保存设置功能'
    },
    {
      name: '获取设置',
      command: 'get_setting',
      params: { key: 'test_key' },
      description: '测试获取设置功能'
    },
    {
      name: '测试API连接',
      command: 'test_api_connection',
      params: { 
        apiKey: 'test-key', 
        apiBase: 'https://api.openai.com/v1' 
      },
      description: '测试外部AI API连接'
    },
    
    // 错题库管理API
    {
      name: '获取错题列表（无筛选）',
      command: 'get_mistakes',
      params: {},
      description: '测试获取错题列表（无筛选条件）'
    },
    {
      name: '获取错题列表（按科目筛选）',
      command: 'get_mistakes',
      params: { subject: '数学' },
      description: '测试按科目筛选错题列表'
    },
    {
      name: '获取错题详情',
      command: 'get_mistake_details',
      params: { id: 'test-mistake-id' },
      description: '测试获取单个错题的详细信息'
    },
    {
      name: '更新错题',
      command: 'update_mistake',
      params: {
        mistake: {
          id: 'test-mistake-id',
          subject: '数学',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
          question_images: [],
          analysis_images: [],
          user_question: '更新后的问题',
          ocr_text: '更新后的OCR文本',
          tags: ['测试标签'],
          mistake_type: '计算题',
          status: 'completed',
          chat_history: []
        }
      },
      description: '测试更新错题信息'
    },
    {
      name: '删除错题',
      command: 'delete_mistake',
      params: { id: 'test-mistake-id' },
      description: '测试删除错题及关联文件'
    },
    
    // 分析相关API
    {
      name: '分析新错题',
      command: 'analyze_new_mistake',
      params: {
        request: {
          subject: '数学',
          question_image_files: ['data:image/jpeg;base64,/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAYEBQYFBAYGBQYHBwYIChAKCgkJChQODwwQFxQYGBcUFhYaHSUfGhsjHBYWICwgIyYnKSopGR8tMC0oMCUoKSj/2wBDAQcHBwoIChMKChMoGhYaKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCgoKCj/wAARCAABAAEDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAv/xAAUEAEAAAAAAAAAAAAAAAAAAAAA/8QAFQEBAQAAAAAAAAAAAAAAAAAAAAX/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oADAMBAAIRAxEAPwCdABmX/9k='],
          analysis_image_files: [],
          user_question: '这是一个测试问题'
        }
      },
      description: '测试分析新错题功能'
    },
    {
      name: '继续对话（临时会话）',
      command: 'continue_chat',
      params: {
        request: {
          temp_id: 'test-temp-id',
          chat_history: [
            {
              role: 'user',
              content: '请详细解释一下这道题',
              timestamp: new Date().toISOString()
            }
          ]
        }
      },
      description: '测试在分析过程中继续对话'
    },
    {
      name: '保存分析结果到错题库',
      command: 'save_mistake_from_analysis',
      params: {
        request: {
          temp_id: 'test-temp-id',
          final_chat_history: [
            {
              role: 'assistant',
              content: '这是AI的回答',
              timestamp: new Date().toISOString()
            }
          ]
        }
      },
      description: '测试将分析结果保存到错题库'
    },
    {
      name: '继续错题对话',
      command: 'continue_mistake_chat',
      params: {
        mistakeId: 'test-mistake-id',
        chatHistory: [
          {
            role: 'user',
            content: '我还有疑问',
            timestamp: new Date().toISOString()
          }
        ]
      },
      description: '测试在错题详情页继续对话'
    },
    
    // 回顾分析API (流式版本)
    {
      name: '回顾分析',
      command: 'analyze_review_session_stream',
      params: {
        subject: '数学',
        mistake_ids: ['mistake-1', 'mistake-2', 'mistake-3']
      },
      description: '测试对多个错题进行关联分析（流式）'
    },
    
    // 文件管理API
    {
      name: '获取图片（base64）',
      command: 'get_image_as_base64',
      params: { relativePath: 'test-image.jpg' },
      description: '测试读取图片文件为base64格式'
    },
    {
      name: '清理孤立图片',
      command: 'cleanup_orphaned_images',
      description: '测试清理孤立图片文件'
    }
  ];

  useEffect(() => {
    // 检查Tauri环境 - 尝试多种检测方式
    const checkTauriEnvironment = async () => {
      try {
        // 方法1: 检查全局变量
        if (typeof (window as any).__TAURI__ !== 'undefined') {
          setIsTauriAvailable(true);
          return;
        }
        
        // 方法2: 检查Tauri API
        if (typeof (window as any).__TAURI_INTERNALS__ !== 'undefined') {
          setIsTauriAvailable(true);
          return;
        }
        
        // 方法3: 尝试调用一个简单的Tauri命令
        await invoke('get_supported_subjects');
        setIsTauriAvailable(true);
      } catch (error) {
        console.log('Tauri环境检测失败:', error);
        setIsTauriAvailable(false);
      }
    };
    
    checkTauriEnvironment();
  }, []);

  const runSingleTest = async (test: ApiTest): Promise<TestResult> => {
    const startTime = Date.now();
    
    try {
      console.log(`开始测试: ${test.name}`);
      
      // 动态替换参数中的测试上下文
      let params = test.params || {};
      if (params) {
        params = JSON.parse(JSON.stringify(params)); // 深拷贝
        
        // 替换temp_id
        if (params.request?.temp_id === 'test-temp-id' && testContext.tempId) {
          params.request.temp_id = testContext.tempId;
        }
        
        // 替换mistake_id
        if (params.mistakeId === 'test-mistake-id' && testContext.mistakeId) {
          params.mistakeId = testContext.mistakeId;
        }
        if (params.id === 'test-mistake-id' && testContext.mistakeId) {
          params.id = testContext.mistakeId;
        }
        if (params.mistake?.id === 'test-mistake-id' && testContext.mistakeId) {
          params.mistake.id = testContext.mistakeId;
        }
        
        // 替换review_id
        if (params.review_id === 'test-review-id' && testContext.reviewId) {
          params.review_id = testContext.reviewId;
        }
      }
      
      const result = await invoke(test.command, params);
      const duration = Date.now() - startTime;
      
      console.log(`测试成功: ${test.name}`, result);
      
      // 更新测试上下文
      if (test.command === 'analyze_new_mistake' && (result as any)?.temp_id) {
        setTestContext(prev => ({ ...prev, tempId: (result as any).temp_id }));
      }
      if (test.command === 'save_mistake_from_analysis' && (result as any)?.mistake_item?.id) {
        setTestContext(prev => ({ ...prev, mistakeId: (result as any).mistake_item.id }));
      }
      if (test.command === 'analyze_review_session' && (result as any)?.review_id) {
        setTestContext(prev => ({ ...prev, reviewId: (result as any).review_id }));
      }
      
      return {
        name: test.name,
        status: 'success',
        message: `成功 - 响应: ${JSON.stringify(result).substring(0, 100)}${JSON.stringify(result).length > 100 ? '...' : ''}`,
        duration
      };
    } catch (error: any) {
      const duration = Date.now() - startTime;
      console.error(`测试失败: ${test.name}`, error);
      
      return {
        name: test.name,
        status: 'error',
        message: `失败 - ${error.message || error}`,
        duration
      };
    }
  };

  const runAllTests = async () => {
    if (!isTauriAvailable) {
      alert('Tauri环境不可用，请在Tauri应用中运行测试');
      return;
    }

    setIsRunning(true);
    setTestResults([]);

    for (const test of apiTests) {
      // 添加pending状态
      setTestResults(prev => [...prev, {
        name: test.name,
        status: 'pending',
        message: '测试中...'
      }]);

      const result = await runSingleTest(test);
      
      // 更新结果
      setTestResults(prev => 
        prev.map(r => r.name === test.name ? result : r)
      );

      // 短暂延迟，避免过快的请求
      await new Promise(resolve => setTimeout(resolve, 500));
    }

    setIsRunning(false);
  };

  const runSingleTestById = async (testIndex: number) => {
    if (!isTauriAvailable) {
      alert('Tauri环境不可用，请在Tauri应用中运行测试');
      return;
    }

    const test = apiTests[testIndex];
    
    // 更新或添加pending状态
    setTestResults(prev => {
      const existing = prev.find(r => r.name === test.name);
      if (existing) {
        return prev.map(r => r.name === test.name ? 
          { ...r, status: 'pending' as const, message: '测试中...' } : r
        );
      } else {
        return [...prev, {
          name: test.name,
          status: 'pending' as const,
          message: '测试中...'
        }];
      }
    });

    const result = await runSingleTest(test);
    
    // 更新结果
    setTestResults(prev => 
      prev.map(r => r.name === test.name ? result : r)
    );
  };

  const clearResults = () => {
    setTestResults([]);
    setTestContext({});
  };

  const getStatusIcon = (status: TestResult['status']) => {
    switch (status) {
      case 'pending':
        return '⏳';
      case 'success':
        return '✅';
      case 'error':
        return '❌';
      default:
        return '⚪';
    }
  };

  const getStatusColor = (status: TestResult['status']) => {
    switch (status) {
      case 'pending':
        return '#fbbf24';
      case 'success':
        return '#10b981';
      case 'error':
        return '#ef4444';
      default:
        return '#6b7280';
    }
  };

  // 渲染测试卡片的函数
  const renderTestCard = (test: ApiTest, index: number, result?: TestResult) => {
    return (
      <div
        key={test.name}
        style={{
          border: '1px solid #e5e7eb',
          borderRadius: '8px',
          padding: '15px',
          backgroundColor: '#f9fafb'
        }}
      >
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start' }}>
          <div style={{ flex: 1 }}>
            <h4 style={{ margin: '0 0 5px 0', color: '#1f2937' }}>
              {test.name}
            </h4>
            <p style={{ margin: '0 0 10px 0', fontSize: '14px', color: '#6b7280' }}>
              {test.description}
            </p>
            <div style={{ fontSize: '12px', color: '#9ca3af' }}>
              命令: <code>{test.command}</code>
              {test.params && (
                <div>参数: <code>{JSON.stringify(test.params)}</code></div>
              )}
            </div>
          </div>
          
          <div style={{ marginLeft: '15px', textAlign: 'right' }}>
            <button
              onClick={() => runSingleTestById(index)}
              disabled={isRunning || !isTauriAvailable}
              style={{
                padding: '5px 10px',
                backgroundColor: '#10b981',
                color: 'white',
                border: 'none',
                borderRadius: '4px',
                cursor: isRunning ? 'not-allowed' : 'pointer',
                fontSize: '12px'
              }}
            >
              单独测试
            </button>
          </div>
        </div>
        
        {result && (
          <div style={{ 
            marginTop: '10px', 
            padding: '10px', 
            backgroundColor: 'white',
            borderRadius: '4px',
            border: `1px solid ${getStatusColor(result.status)}`
          }}>
            <div style={{ display: 'flex', alignItems: 'center', marginBottom: '5px' }}>
              <span style={{ marginRight: '8px', fontSize: '16px' }}>
                {getStatusIcon(result.status)}
              </span>
              <span style={{ 
                fontWeight: 'bold', 
                color: getStatusColor(result.status) 
              }}>
                {result.status === 'pending' ? '测试中' : 
                 result.status === 'success' ? '成功' : '失败'}
              </span>
              {result.duration && (
                <span style={{ marginLeft: '10px', fontSize: '12px', color: '#6b7280' }}>
                  ({result.duration}ms)
                </span>
              )}
            </div>
            <div style={{ fontSize: '14px', color: '#374151' }}>
              {result.message}
            </div>
          </div>
        )}
      </div>
    );
  };

  return (
    <div style={{ padding: '20px', maxWidth: '1000px', margin: '0 auto' }}>
      <h2>🔧 后端API测试</h2>
      
      {/* 环境状态 */}
      <div style={{ 
        marginBottom: '20px', 
        padding: '15px', 
        backgroundColor: isTauriAvailable ? '#dcfce7' : '#fef2f2',
        borderRadius: '8px',
        border: `1px solid ${isTauriAvailable ? '#16a34a' : '#dc2626'}`
      }}>
        <h3>环境状态</h3>
        <p>
          Tauri环境: {isTauriAvailable ? '✅ 可用' : '❌ 不可用'}
        </p>
        {!isTauriAvailable && (
          <p style={{ color: '#dc2626', fontSize: '14px' }}>
            请在Tauri桌面应用中运行此测试，浏览器环境无法调用后端API
          </p>
        )}
      </div>

      {/* 测试上下文 */}
      {(testContext.tempId || testContext.mistakeId || testContext.reviewId) && (
        <div style={{ 
          marginBottom: '20px', 
          padding: '15px', 
          backgroundColor: '#f0f9ff',
          borderRadius: '8px',
          border: '1px solid #0ea5e9'
        }}>
          <h3>测试上下文</h3>
          <div style={{ fontSize: '14px', fontFamily: 'monospace' }}>
            {testContext.tempId && <div>临时会话ID: {testContext.tempId}</div>}
            {testContext.mistakeId && <div>错题ID: {testContext.mistakeId}</div>}
            {testContext.reviewId && <div>回顾ID: {testContext.reviewId}</div>}
          </div>
          <p style={{ fontSize: '12px', color: '#6b7280', marginTop: '8px' }}>
            这些ID会自动用于相关的API测试中
          </p>
        </div>
      )}

      {/* 控制按钮 */}
      <div style={{ marginBottom: '20px' }}>
        <button
          onClick={runAllTests}
          disabled={isRunning || !isTauriAvailable}
          style={{
            padding: '10px 20px',
            marginRight: '10px',
            backgroundColor: isRunning ? '#9ca3af' : '#3b82f6',
            color: 'white',
            border: 'none',
            borderRadius: '6px',
            cursor: isRunning ? 'not-allowed' : 'pointer'
          }}
        >
          {isRunning ? '测试进行中...' : '运行所有测试'}
        </button>
        
        <button
          onClick={clearResults}
          disabled={isRunning}
          style={{
            padding: '10px 20px',
            backgroundColor: '#6b7280',
            color: 'white',
            border: 'none',
            borderRadius: '6px',
            cursor: isRunning ? 'not-allowed' : 'pointer'
          }}
        >
          清空结果
        </button>
        
        {(testContext.tempId || testContext.mistakeId || testContext.reviewId) && (
          <button
            onClick={() => setTestContext({})}
            disabled={isRunning}
            style={{
              padding: '10px 20px',
              marginLeft: '10px',
              backgroundColor: '#f59e0b',
              color: 'white',
              border: 'none',
              borderRadius: '6px',
              cursor: isRunning ? 'not-allowed' : 'pointer'
            }}
          >
            清空上下文
          </button>
        )}
      </div>

      {/* API测试列表 */}
      <div style={{ display: 'grid', gap: '20px' }}>
        {/* 基础API测试 */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>📊 基础API测试</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(0, 2).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index, result);
            })}
          </div>
        </div>

        {/* 设置管理API */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>⚙️ 设置管理API</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(2, 5).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index + 2, result);
            })}
          </div>
        </div>

        {/* 错题库管理API */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>📚 错题库管理API</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(5, 10).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index + 5, result);
            })}
          </div>
        </div>

        {/* 分析相关API */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>🔍 分析相关API</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(10, 15).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index + 10, result);
            })}
          </div>
        </div>

        {/* 回顾分析API */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>📈 回顾分析API</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(15, 16).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index + 15, result);
            })}
          </div>
        </div>

        {/* 文件管理API */}
        <div>
          <h3 style={{ marginBottom: '15px', color: '#1f2937' }}>📁 文件管理API</h3>
          <div style={{ display: 'grid', gap: '15px' }}>
            {apiTests.slice(16).map((test, index) => {
              const result = testResults.find(r => r.name === test.name);
              return renderTestCard(test, index + 16, result);
            })}
          </div>
        </div>
      </div>

      {/* 测试统计 */}
      {testResults.length > 0 && (
        <div style={{ 
          marginTop: '20px', 
          padding: '15px', 
          backgroundColor: '#f3f4f6',
          borderRadius: '8px'
        }}>
          <h3>测试统计</h3>
          <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(120px, 1fr))', gap: '10px' }}>
            <div>
              总数: <strong>{testResults.length}</strong>
            </div>
            <div style={{ color: '#10b981' }}>
              成功: <strong>{testResults.filter(r => r.status === 'success').length}</strong>
            </div>
            <div style={{ color: '#ef4444' }}>
              失败: <strong>{testResults.filter(r => r.status === 'error').length}</strong>
            </div>
            <div style={{ color: '#fbbf24' }}>
              进行中: <strong>{testResults.filter(r => r.status === 'pending').length}</strong>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};

export default BackendTest;
