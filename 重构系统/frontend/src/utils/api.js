import axios from 'axios'
import { ElMessage } from 'element-plus'

const api = axios.create({
  baseURL: '/api',
  timeout: 30000,
  headers: {
    'Content-Type': 'application/json'
  }
})

// 响应拦截器
api.interceptors.response.use(
  response => {
    return response
  },
  error => {
    if (error.response) {
      switch (error.response.status) {
        case 404:
          ElMessage.error('请求的资源不存在')
          break
        case 500:
          ElMessage.error('服务器错误')
          break
        default:
          ElMessage.error(error.response.data.detail || '请求失败')
      }
    } else if (error.request) {
      ElMessage.error('网络错误，请检查网络连接')
    } else {
      ElMessage.error('请求配置错误')
    }
    return Promise.reject(error)
  }
)

export default api

// API方法封装
export const problemAPI = {
  getList: (params) => api.get('/v1/problems', { params }),
  getDetail: (id) => api.get(`/v1/problems/${id}`),
  create: (data) => api.post('/v1/problems', data),
  update: (id, data) => api.put(`/v1/problems/${id}`, data),
  delete: (id) => api.delete(`/v1/problems/${id}`),
  review: (id, masteryLevel) => api.post(`/v1/problems/${id}/review`, { mastery_level: masteryLevel }),
  getStats: () => api.get('/v1/problems/stats/overview'),
  getKnowledgeStats: () => api.get('/v1/problems/stats/knowledge-points'),

  // AI-Driven Problem Creation
  initiateAICreation: (data) => api.post('/v1/problems/ai-create/initiate', data), // data should be { image_base64: "...", subject_hint: "..." } or { image_url: "...", ... }
  // streamAIAnalysis: Handled directly by EventSource in components.
  // Helper to build stream URL:
  getAIInteractiveStreamURL: (sessionId, params) => {
    const queryParams = new URLSearchParams(params).toString();
    return `/api/v1/problems/ai-create/interactive-stream/${sessionId}?${queryParams}`;
  },
  finalizeAICreation: (data) => api.post('/v1/problems/ai-create/finalize', data),
  listAISessions: (params) => api.get('/v1/problems/ai-create/sessions', { params }),
  getAISessionDetail: (sessionId) => api.get(`/v1/problems/ai-create/session/${sessionId}`),
  getAISessionChatHistory: (sessionId) => api.get(`/v1/problems/ai-create/chat-history/${sessionId}`)
}

export const analysisAPI = {
  getList: (params) => api.get('/v1/reviews', { params }),
  getDetail: (id) => api.get(`/v1/reviews/${id}`),
  create: (data) => api.post('/v1/reviews', data)
}

export const statisticsAPI = {
  getStatistics: () => api.get('/v1/problems/stats/overview')
}

// AI API方法封装
export const aiAPI = {
  // 模型管理
  getModels: (params) => api.get('/v1/ai/models', { params }),
  getModelDetail: (id) => api.get(`/v1/ai/models/${id}`),
  createModel: (data) => api.post('/v1/ai/models', data),
  updateModel: (id, data) => api.put(`/v1/ai/models/${id}`, data),
  deleteModel: (id) => api.delete(`/v1/ai/models/${id}`),
  testModel: (id) => api.post(`/v1/ai/models/${id}/test`),
  
  // AI调用
  callAI: (data) => api.post('/v1/ai/call', data),
  
  // 统计数据
  getStats: () => api.get('/v1/ai/stats'),
  checkHealth: () => api.get('/v1/ai/health')
}
