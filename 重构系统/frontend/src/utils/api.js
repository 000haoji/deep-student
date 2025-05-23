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
  getList: (params) => api.get('/problems', { params }),
  getDetail: (id) => api.get(`/problems/${id}`),
  create: (data) => api.post('/problems', data),
  update: (id, data) => api.put(`/problems/${id}`, data),
  delete: (id) => api.delete(`/problems/${id}`),
  review: (id, masteryLevel) => api.post(`/problems/${id}/review`, { mastery_level: masteryLevel })
}

export const analysisAPI = {
  getList: (params) => api.get('/analyses', { params }),
  getDetail: (id) => api.get(`/analyses/${id}`),
  create: (data) => api.post('/analyses', data)
}

export const statisticsAPI = {
  getStatistics: () => api.get('/statistics')
} 