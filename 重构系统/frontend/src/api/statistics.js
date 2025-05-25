import request from '@/utils/request'

/**
 * 获取错题统计数据
 * @param {Object} params - 统计参数
 * @returns {Promise} 统计数据
 */
export function getProblemStatistics(params) {
  return request({
    url: '/api/statistics/problems',
    method: 'get',
    params
  })
}

/**
 * 获取学习进度统计
 * @param {Object} params - 统计参数
 * @returns {Promise} 进度统计数据
 */
export function getLearningProgress(params) {
  return request({
    url: '/api/statistics/progress',
    method: 'get',
    params
  })
}

/**
 * 获取复习计划统计
 * @param {Object} params - 统计参数
 * @returns {Promise} 复习计划统计数据
 */
export function getReviewPlanStatistics(params) {
  return request({
    url: '/api/statistics/review-plans',
    method: 'get',
    params
  })
} 