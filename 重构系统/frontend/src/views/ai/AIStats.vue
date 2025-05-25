<template>
  <div class="ai-stats">
    <!-- 概览卡片 -->
    <el-row :gutter="20" class="overview-cards">
      <el-col :span="6">
        <el-card class="stats-card">
          <div class="stats-item">
            <div class="stats-icon total-calls">
              <el-icon><Document /></el-icon>
            </div>
            <div class="stats-content">
              <div class="stats-value">{{ formatNumber(overview.total_calls) }}</div>
              <div class="stats-label">总调用次数</div>
            </div>
          </div>
        </el-card>
      </el-col>
      
      <el-col :span="6">
        <el-card class="stats-card">
          <div class="stats-item">
            <div class="stats-icon success-rate">
              <el-icon><SuccessFilled /></el-icon>
            </div>
            <div class="stats-content">
              <div class="stats-value">{{ formatPercent(overview.success_rate) }}</div>
              <div class="stats-label">成功率</div>
            </div>
          </div>
        </el-card>
      </el-col>
      
      <el-col :span="6">
        <el-card class="stats-card">
          <div class="stats-item">
            <div class="stats-icon token-usage">
              <el-icon><Tickets /></el-icon>
            </div>
            <div class="stats-content">
              <div class="stats-value">{{ formatNumber(overview.total_tokens) }}</div>
              <div class="stats-label">Token消耗</div>
            </div>
          </div>
        </el-card>
      </el-col>
      
      <el-col :span="6">
        <el-card class="stats-card">
          <div class="stats-item">
            <div class="stats-icon total-cost">
              <el-icon><Money /></el-icon>
            </div>
            <div class="stats-content">
              <div class="stats-value">${{ formatMoney(overview.total_cost) }}</div>
              <div class="stats-label">总成本</div>
            </div>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <!-- 错误或无数据提示 -->
    <el-alert
      v-if="errorMsg"
      :title="errorMsg"
      type="error"
      show-icon
      style="margin-bottom: 20px;"
    />
    <el-alert
      v-else-if="noData"
      title="暂无统计数据"
      type="info"
      show-icon
      style="margin-bottom: 20px;"
    />

    <!-- 图表区域 -->
    <el-row :gutter="20" class="chart-row">
      <!-- 调用趋势图 -->
      <el-col :span="16">
        <el-card>
          <template #header>
            <div class="card-header">
              <span>调用趋势</span>
              <el-date-picker
                v-model="dateRange"
                type="daterange"
                range-separator="至"
                start-placeholder="开始日期"
                end-placeholder="结束日期"
                @change="onDateRangeChange"
                size="small"
              />
            </div>
          </template>
          <div class="chart-container">
            <template v-if="noData">
              <div class="empty-chart">暂无趋势数据</div>
            </template>
            <canvas v-else ref="trendChartRef" width="600" height="300"></canvas>
          </div>
        </el-card>
      </el-col>

      <!-- 成本分布饼图 -->
      <el-col :span="8">
        <el-card>
          <template #header>
            <span>成本分布</span>
          </template>
          <div class="chart-container">
            <template v-if="noData">
              <div class="empty-chart">暂无成本分布数据</div>
            </template>
            <canvas v-else ref="costChartRef" width="300" height="300"></canvas>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <!-- 模型使用详情 -->
    <el-row>
      <el-col :span="24">
        <el-card>
          <template #header>
            <span>模型使用详情</span>
          </template>
          
          <el-table :data="modelStats" v-loading="loading" stripe>
            <template #empty>
              <div style="padding: 20px; text-align: center; color: #909399;">暂无模型统计数据</div>
            </template>
            <el-table-column prop="provider" label="提供商" width="120">
              <template #default="{ row }">
                <el-tag :type="getProviderTagType(row.provider)">
                  {{ getProviderName(row.provider) }}
                </el-tag>
              </template>
            </el-table-column>
            
            <el-table-column prop="model_name" label="模型名称" width="180" />
            
            <el-table-column prop="total_calls" label="调用次数" width="120" align="center">
              <template #default="{ row }">
                {{ formatNumber(row.total_calls) }}
              </template>
            </el-table-column>
            
            <el-table-column prop="success_rate" label="成功率" width="100" align="center">
              <template #default="{ row }">
                <span :class="getSuccessRateClass(row.success_rate)">
                  {{ formatPercent(row.success_rate) }}
                </span>
              </template>
            </el-table-column>
            
            <el-table-column prop="avg_response_time" label="平均响应时间" width="140" align="center">
              <template #default="{ row }">
                {{ row.avg_response_time }}ms
              </template>
            </el-table-column>
            
            <el-table-column prop="total_tokens" label="Token消耗" width="120" align="center">
              <template #default="{ row }">
                {{ formatNumber(row.total_tokens) }}
              </template>
            </el-table-column>
            
            <el-table-column prop="total_cost" label="成本" width="120" align="center">
              <template #default="{ row }">
                ${{ formatMoney(row.total_cost) }}
              </template>
            </el-table-column>
            
            <el-table-column label="健康状态" width="120" align="center">
              <template #default="{ row }">
                <el-tag 
                  :type="row.health_status === 'healthy' ? 'success' : 'danger'"
                  size="small"
                >
                  {{ row.health_status === 'healthy' ? '健康' : '异常' }}
                </el-tag>
              </template>
            </el-table-column>
            
            <el-table-column label="最后调用" width="160">
              <template #default="{ row }">
                {{ formatDateTime(row.last_used) }}
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>
    </el-row>

    <!-- 健康状态监控 -->
    <el-row>
      <el-col :span="24">
        <el-card>
          <template #header>
            <div class="card-header">
              <span>健康状态监控</span>
              <el-button type="primary" size="small" @click="refreshHealth" :loading="healthLoading">
                <el-icon><Refresh /></el-icon>
                刷新状态
              </el-button>
            </div>
          </template>
          
          <el-row :gutter="20">
            <el-col :span="8" v-for="health in healthStatus" :key="health.model_id">
              <div class="health-card" :class="health.status">
                <div class="health-header">
                  <span class="model-name">{{ health.model_name }}</span>
                  <el-tag 
                    :type="health.status === 'healthy' ? 'success' : 'danger'"
                    size="small"
                  >
                    {{ health.status === 'healthy' ? '健康' : '异常' }}
                  </el-tag>
                </div>
                <div class="health-metrics">
                  <div class="metric">
                    <span class="metric-label">响应时间:</span>
                    <span class="metric-value">{{ health.response_time || '--' }}ms</span>
                  </div>
                  <div class="metric">
                    <span class="metric-label">错误率:</span>
                    <span class="metric-value">{{ formatPercent(health.error_rate || 0) }}</span>
                  </div>
                  <div class="metric">
                    <span class="metric-label">最后检查:</span>
                    <span class="metric-value">{{ formatDateTime(health.last_check) }}</span>
                  </div>
                </div>
              </div>
            </el-col>
          </el-row>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, nextTick } from 'vue'
import { ElMessage } from 'element-plus'
import { 
  Document, 
  SuccessFilled, 
  Tickets, 
  Money, 
  Refresh 
} from '@element-plus/icons-vue'
import { Chart, registerables } from 'chart.js'
import { aiAPI } from '@/utils/api'

// 注册Chart.js组件
Chart.register(...registerables)

// 响应式数据
const loading = ref(false)
const healthLoading = ref(false)
const trendChartRef = ref()
const costChartRef = ref()
const dateRange = ref([])
const errorMsg = ref("")
const noData = ref(false)

// 统计数据
const overview = reactive({
  total_calls: 0,
  success_rate: 0,
  total_tokens: 0,
  total_cost: 0
})

const modelStats = ref([])
const healthStatus = ref([])
const trendChart = ref(null)
const costChart = ref(null)

// 工具函数
const formatNumber = (num) => {
  if (num >= 1000000) {
    return (num / 1000000).toFixed(1) + 'M'
  } else if (num >= 1000) {
    return (num / 1000).toFixed(1) + 'K'
  }
  return num.toString()
}

const formatPercent = (rate) => {
  return (rate * 100).toFixed(1) + '%'
}

const formatMoney = (amount) => {
  return amount.toFixed(4)
}

const formatDateTime = (dateStr) => {
  if (!dateStr) return '--'
  return new Date(dateStr).toLocaleString('zh-CN')
}

const getProviderTagType = (provider) => {
  const typeMap = {
    'openai': 'success',
    'gemini': 'warning',
    'deepseek': 'info',
    'claude': 'danger',
    'qwen': 'primary'
  }
  return typeMap[provider] || 'default'
}

const getProviderName = (provider) => {
  const nameMap = {
    'openai': 'OpenAI',
    'gemini': 'Gemini',
    'deepseek': 'DeepSeek',
    'claude': 'Claude',
    'qwen': 'Qwen'
  }
  return nameMap[provider] || provider
}

const getSuccessRateClass = (rate) => {
  if (rate >= 0.9) return 'success-rate high'
  if (rate >= 0.7) return 'success-rate medium'
  return 'success-rate low'
}

// 加载统计数据
const loadStats = async () => {
  loading.value = true
  errorMsg.value = ""
  noData.value = false
  try {
    const response = await aiAPI.getStats()
    const data = response.data.data
    
    // 判断无数据
    if (!data || !data.overview || (
      data.overview.total_calls === 0 &&
      data.overview.success_rate === 0 &&
      data.overview.total_tokens === 0 &&
      data.overview.total_cost === 0
    )) {
      noData.value = true
    }
    
    // 更新概览数据
    Object.assign(overview, {
      total_calls: data.overview.total_calls || 0,
      success_rate: data.overview.success_rate || 0,
      total_tokens: data.overview.total_tokens || 0,
      total_cost: data.overview.total_cost || 0
    })
    
    // 更新模型统计
    modelStats.value = data.model_stats || []
    
    // 更新图表
    await nextTick()
    updateTrendChart(data.trend_data || [])
    updateCostChart(data.cost_distribution || [])
    
  } catch (error) {
    errorMsg.value = '加载统计数据失败'
  } finally {
    loading.value = false
  }
}

// 更新趋势图表
const updateTrendChart = (trendData) => {
  if (!trendChartRef.value) return
  
  const ctx = trendChartRef.value.getContext('2d')
  
  if (trendChart.value) {
    trendChart.value.destroy()
  }
  
  trendChart.value = new Chart(ctx, {
    type: 'line',
    data: {
      labels: trendData.map(item => item.date),
      datasets: [
        {
          label: '调用次数',
          data: trendData.map(item => item.calls),
          borderColor: '#409EFF',
          backgroundColor: 'rgba(64, 158, 255, 0.1)',
          tension: 0.4,
          fill: true
        },
        {
          label: '成功次数',
          data: trendData.map(item => item.success_calls),
          borderColor: '#67C23A',
          backgroundColor: 'rgba(103, 194, 58, 0.1)',
          tension: 0.4,
          fill: true
        }
      ]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        y: {
          beginAtZero: true
        }
      },
      plugins: {
        legend: {
          position: 'top'
        }
      }
    }
  })
}

// 更新成本分布图表
const updateCostChart = (costData) => {
  if (!costChartRef.value) return
  
  const ctx = costChartRef.value.getContext('2d')
  
  if (costChart.value) {
    costChart.value.destroy()
  }
  
  costChart.value = new Chart(ctx, {
    type: 'doughnut',
    data: {
      labels: costData.map(item => item.provider),
      datasets: [{
        data: costData.map(item => item.cost),
        backgroundColor: [
          '#409EFF',
          '#67C23A',
          '#E6A23C',
          '#F56C6C',
          '#909399'
        ]
      }]
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: {
          position: 'bottom'
        }
      }
    }
  })
}

// 刷新健康状态
const refreshHealth = async () => {
  healthLoading.value = true
  try {
    const response = await aiAPI.checkHealth()
    healthStatus.value = response.data.data || []
  } catch (error) {
    ElMessage.error('获取健康状态失败')
  } finally {
    healthLoading.value = false
  }
}

// 日期范围变化处理
const onDateRangeChange = (dates) => {
  // 重新加载指定时间范围的数据
  loadStats()
}

// 生命周期
onMounted(() => {
  loadStats()
  refreshHealth()
})
</script>

<style scoped>
.ai-stats {
  padding: 20px;
}

.overview-cards {
  margin-bottom: 20px;
}

.stats-card {
  margin-bottom: 20px;
}

.stats-item {
  display: flex;
  align-items: center;
}

.stats-icon {
  width: 60px;
  height: 60px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 16px;
  font-size: 24px;
  color: white;
}

.stats-icon.total-calls {
  background: linear-gradient(45deg, #409EFF, #66b3ff);
}

.stats-icon.success-rate {
  background: linear-gradient(45deg, #67C23A, #85ce61);
}

.stats-icon.token-usage {
  background: linear-gradient(45deg, #E6A23C, #ebb563);
}

.stats-icon.total-cost {
  background: linear-gradient(45deg, #F56C6C, #f78989);
}

.stats-content {
  flex: 1;
}

.stats-value {
  font-size: 28px;
  font-weight: bold;
  color: #303133;
  line-height: 1.2;
}

.stats-label {
  font-size: 14px;
  color: #909399;
  margin-top: 4px;
}

.chart-row {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.chart-container {
  height: 300px;
  position: relative;
}

.success-rate {
  font-weight: bold;
}

.success-rate.high {
  color: #67c23a;
}

.success-rate.medium {
  color: #e6a23c;
}

.success-rate.low {
  color: #f56c6c;
}

.health-card {
  border: 1px solid #EBEEF5;
  border-radius: 8px;
  padding: 16px;
  margin-bottom: 16px;
  transition: all 0.3s;
}

.health-card.healthy {
  border-color: #67C23A;
  background: #f0f9ff;
}

.health-card.unhealthy {
  border-color: #F56C6C;
  background: #fef0f0;
}

.health-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.model-name {
  font-weight: bold;
  color: #303133;
}

.health-metrics {
  space-y: 8px;
}

.metric {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
}

.metric-label {
  color: #909399;
  font-size: 14px;
}

.metric-value {
  color: #303133;
  font-size: 14px;
  font-weight: 500;
}

.empty-chart {
  width: 100%;
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: #909399;
  font-size: 16px;
}
</style> 