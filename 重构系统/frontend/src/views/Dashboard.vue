<template>
  <div class="dashboard">
    <h1>数据概览</h1>
    
    <!-- 统计卡片 -->
    <el-row :gutter="20" class="stats-cards">
      <el-col :span="6">
        <el-card>
          <el-statistic title="总错题数" :value="statistics.total_problems" />
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card>
          <el-statistic title="已掌握" :value="masteredCount" />
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card>
          <el-statistic title="分析报告" :value="statistics.total_analyses" />
        </el-card>
      </el-col>
      <el-col :span="6">
        <el-card>
          <el-statistic title="平均掌握度" :value="avgMastery" suffix="%" />
        </el-card>
      </el-col>
    </el-row>

    <!-- 图表区域 -->
    <el-row :gutter="20" class="charts-row">
      <el-col :span="12">
        <el-card>
          <template #header>
            <div class="card-header">
              <span>学科分布</span>
            </div>
          </template>
          <div class="chart-container">
            <canvas ref="subjectChart"></canvas>
          </div>
        </el-card>
      </el-col>
      <el-col :span="12">
        <el-card>
          <template #header>
            <div class="card-header">
              <span>最近复习</span>
            </div>
          </template>
          <el-table :data="recentReviews" style="width: 100%">
            <el-table-column prop="title" label="题目" />
            <el-table-column prop="subject" label="学科" width="100">
              <template #default="{ row }">
                <el-tag>{{ getSubjectName(row.subject) }}</el-tag>
              </template>
            </el-table-column>
            <el-table-column prop="last_review_at" label="复习时间" width="180">
              <template #default="{ row }">
                {{ formatDate(row.last_review_at) }}
              </template>
            </el-table-column>
          </el-table>
        </el-card>
      </el-col>
    </el-row>

    <!-- 快捷操作 -->
    <el-card class="quick-actions">
      <template #header>
        <div class="card-header">
          <span>快捷操作</span>
        </div>
      </template>
      <el-space>
        <el-button type="primary" @click="$router.push('/problems/create')">
          <el-icon><Plus /></el-icon>
          添加错题
        </el-button>
        <el-button @click="$router.push('/analyses/create')">
          <el-icon><DataAnalysis /></el-icon>
          创建分析
        </el-button>
        <el-button @click="$router.push('/problems')">
          <el-icon><Document /></el-icon>
          查看所有错题
        </el-button>
      </el-space>
    </el-card>
  </div>
</template>

<script setup>
import { ref, onMounted, computed } from 'vue'
import { Chart, registerables } from 'chart.js'
import { statisticsAPI } from '@/utils/api'
import dayjs from 'dayjs'

Chart.register(...registerables)

const statistics = ref({
  total_problems: 0,
  subject_statistics: [],
  total_analyses: 0,
  recent_reviews: []
})

const subjectChart = ref(null)
let chartInstance = null

const recentReviews = computed(() => statistics.value.recent_reviews || [])

const masteredCount = computed(() => {
  const subjectStats = statistics.value.subject_statistics || []
  return subjectStats.reduce((sum, item) => {
    return sum + Math.round(item.count * item.avg_mastery)
  }, 0)
})

const avgMastery = computed(() => {
  const subjectStats = statistics.value.subject_statistics || []
  if (subjectStats.length === 0) return 0
  
  const totalMastery = subjectStats.reduce((sum, item) => {
    return sum + item.avg_mastery * item.count
  }, 0)
  
  const totalCount = subjectStats.reduce((sum, item) => sum + item.count, 0)
  
  return totalCount > 0 ? Math.round(totalMastery / totalCount * 100) : 0
})

const getSubjectName = (subject) => {
  const map = {
    math: '数学',
    english: '英语',
    politics: '政治',
    professional: '专业课'
  }
  return map[subject] || subject
}

const formatDate = (date) => {
  return dayjs(date).format('YYYY-MM-DD HH:mm')
}

const fetchStatistics = async () => {
  try {
    const { data } = await statisticsAPI.getStatistics()
    if (data.success) {
      const stats = data.data
      
      // 转换数据格式以适配组件需求
      statistics.value = {
        total_problems: stats.total_problems || 0,
        total_analyses: stats.total_review_count || 0,
        recent_reviews: [],  // 暂时为空，后端需要返回这个数据
        subject_statistics: Object.entries(stats.by_subject || {}).map(([subject, count]) => ({
          subject,
          count,
          avg_mastery: stats.avg_mastery_level || 0
        }))
      }
      
      updateChart()
    }
  } catch (error) {
    console.error('Failed to fetch statistics:', error)
  }
}

const updateChart = () => {
  if (!subjectChart.value) return
  
  const bySubject = statistics.value.subject_statistics || []
  
  const chartData = {
    labels: bySubject.map(item => getSubjectName(item.subject)),
    datasets: [{
      label: '错题数量',
      data: bySubject.map(item => item.count),
      backgroundColor: [
        'rgba(255, 99, 132, 0.5)',
        'rgba(54, 162, 235, 0.5)',
        'rgba(255, 205, 86, 0.5)',
        'rgba(75, 192, 192, 0.5)'
      ],
      borderColor: [
        'rgb(255, 99, 132)',
        'rgb(54, 162, 235)',
        'rgb(255, 205, 86)',
        'rgb(75, 192, 192)'
      ],
      borderWidth: 1
    }]
  }
  
  if (chartInstance) {
    chartInstance.destroy()
  }
  
  chartInstance = new Chart(subjectChart.value, {
    type: 'bar',
    data: chartData,
    options: {
      responsive: true,
      maintainAspectRatio: false,
      scales: {
        y: {
          beginAtZero: true
        }
      }
    }
  })
}

onMounted(() => {
  fetchStatistics()
})
</script>

<style scoped>
.dashboard {
  padding: 20px;
}

.dashboard h1 {
  margin: 0 0 20px 0;
  color: #303133;
}

.stats-cards {
  margin-bottom: 20px;
}

.charts-row {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.chart-container {
  height: 300px;
  position: relative;
}

.quick-actions {
  margin-top: 20px;
}
</style> 