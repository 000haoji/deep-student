<template>
  <div class="problem-statistics">
    <el-row :gutter="20">
      <!-- 概览卡片 -->
      <el-col :span="6" v-for="(stat, index) in overviewStats" :key="index">
        <el-card class="stat-card" :body-style="{ padding: '20px' }">
          <div class="stat-icon" :class="stat.type">
            <el-icon><component :is="stat.icon" /></el-icon>
          </div>
          <div class="stat-content">
            <div class="stat-value">{{ stat.value }}</div>
            <div class="stat-label">{{ stat.label }}</div>
          </div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="20" class="chart-row">
      <!-- 知识点分布 -->
      <el-col :span="12">
        <el-card class="chart-card">
          <template #header>
            <div class="card-header">
              <span>知识点分布</span>
              <el-radio-group v-model="knowledgePointTimeRange" size="small">
                <el-radio-button label="week">周</el-radio-button>
                <el-radio-button label="month">月</el-radio-button>
                <el-radio-button label="year">年</el-radio-button>
              </el-radio-group>
            </div>
          </template>
          <div class="chart-container">
            <v-chart class="chart" :option="knowledgePointOption" autoresize />
          </div>
        </el-card>
      </el-col>

      <!-- 难度分布 -->
      <el-col :span="12">
        <el-card class="chart-card">
          <template #header>
            <div class="card-header">
              <span>难度分布</span>
              <el-select v-model="difficultySubject" size="small" placeholder="选择科目">
                <el-option
                  v-for="subject in subjects"
                  :key="subject.value"
                  :label="subject.label"
                  :value="subject.value"
                />
              </el-select>
            </div>
          </template>
          <div class="chart-container">
            <v-chart class="chart" :option="difficultyOption" autoresize />
          </div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="20" class="chart-row">
      <!-- 掌握度趋势 -->
      <el-col :span="24">
        <el-card class="chart-card">
          <template #header>
            <div class="card-header">
              <span>掌握度趋势</span>
              <div class="header-actions">
                <el-select v-model="masterySubject" size="small" placeholder="选择科目">
                  <el-option
                    v-for="subject in subjects"
                    :key="subject.value"
                    :label="subject.label"
                    :value="subject.value"
                  />
                </el-select>
                <el-date-picker
                  v-model="masteryDateRange"
                  type="daterange"
                  range-separator="至"
                  start-placeholder="开始日期"
                  end-placeholder="结束日期"
                  size="small"
                  :shortcuts="dateShortcuts"
                />
              </div>
            </div>
          </template>
          <div class="chart-container">
            <v-chart class="chart" :option="masteryOption" autoresize />
          </div>
        </el-card>
      </el-col>
    </el-row>

    <el-row :gutter="20" class="chart-row">
      <!-- 错误模式分析 -->
      <el-col :span="12">
        <el-card class="chart-card">
          <template #header>
            <div class="card-header">
              <span>错误模式分析</span>
              <el-select v-model="errorPatternSubject" size="small" placeholder="选择科目">
                <el-option
                  v-for="subject in subjects"
                  :key="subject.value"
                  :label="subject.label"
                  :value="subject.value"
                />
              </el-select>
            </div>
          </template>
          <div class="chart-container">
            <v-chart class="chart" :option="errorPatternOption" autoresize />
          </div>
        </el-card>
      </el-col>

      <!-- 学习时间分布 -->
      <el-col :span="12">
        <el-card class="chart-card">
          <template #header>
            <div class="card-header">
              <span>学习时间分布</span>
              <el-radio-group v-model="timeDistributionType" size="small">
                <el-radio-button label="day">日</el-radio-button>
                <el-radio-button label="week">周</el-radio-button>
                <el-radio-button label="month">月</el-radio-button>
              </el-radio-group>
            </div>
          </template>
          <div class="chart-container">
            <v-chart class="chart" :option="timeDistributionOption" autoresize />
          </div>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup>
import { ref, onMounted, computed, watch } from 'vue'
import { use } from 'echarts/core'
import { CanvasRenderer } from 'echarts/renderers'
import { PieChart, BarChart, LineChart } from 'echarts/charts'
import {
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
  DataZoomComponent
} from 'echarts/components'
import VChart from 'vue-echarts'
import { ElMessage } from 'element-plus'
import { getProblemStatistics } from '@/api/statistics'

// 注册 ECharts 组件
use([
  CanvasRenderer,
  PieChart,
  BarChart,
  LineChart,
  TitleComponent,
  TooltipComponent,
  LegendComponent,
  GridComponent,
  DataZoomComponent
])

// 状态定义
const overviewStats = ref([
  { label: '总题数', value: 0, icon: 'Document', type: 'total' },
  { label: '待复习', value: 0, icon: 'Clock', type: 'review' },
  { label: '平均掌握度', value: '0%', icon: 'TrendCharts', type: 'mastery' },
  { label: '本周新增', value: 0, icon: 'Plus', type: 'new' }
])

const knowledgePointTimeRange = ref('month')
const difficultySubject = ref('all')
const masterySubject = ref('all')
const masteryDateRange = ref([])
const errorPatternSubject = ref('all')
const timeDistributionType = ref('week')

const subjects = [
  { label: '全部', value: 'all' },
  { label: '数学', value: 'math' },
  { label: '英语', value: 'english' },
  { label: '政治', value: 'politics' },
  { label: '专业课', value: 'major' }
]

const dateShortcuts = [
  {
    text: '最近一周',
    value: () => {
      const end = new Date()
      const start = new Date()
      start.setTime(start.getTime() - 3600 * 1000 * 24 * 7)
      return [start, end]
    }
  },
  {
    text: '最近一月',
    value: () => {
      const end = new Date()
      const start = new Date()
      start.setTime(start.getTime() - 3600 * 1000 * 24 * 30)
      return [start, end]
    }
  },
  {
    text: '最近三月',
    value: () => {
      const end = new Date()
      const start = new Date()
      start.setTime(start.getTime() - 3600 * 1000 * 24 * 90)
      return [start, end]
    }
  }
]

// 图表配置
const knowledgePointOption = computed(() => ({
  tooltip: {
    trigger: 'item',
    formatter: '{b}: {c} ({d}%)'
  },
  legend: {
    orient: 'vertical',
    right: 10,
    top: 'center',
    type: 'scroll'
  },
  series: [
    {
      type: 'pie',
      radius: ['40%', '70%'],
      avoidLabelOverlap: false,
      itemStyle: {
        borderRadius: 10,
        borderColor: '#fff',
        borderWidth: 2
      },
      label: {
        show: false
      },
      emphasis: {
        label: {
          show: true,
          fontSize: '14',
          fontWeight: 'bold'
        }
      },
      labelLine: {
        show: false
      },
      data: knowledgePointData.value
    }
  ]
}))

const difficultyOption = computed(() => ({
  tooltip: {
    trigger: 'axis',
    axisPointer: {
      type: 'shadow'
    }
  },
  grid: {
    left: '3%',
    right: '4%',
    bottom: '3%',
    containLabel: true
  },
  xAxis: {
    type: 'category',
    data: ['简单', '较易', '中等', '较难', '困难'],
    axisTick: {
      alignWithLabel: true
    }
  },
  yAxis: {
    type: 'value'
  },
  series: [
    {
      type: 'bar',
      barWidth: '60%',
      data: difficultyData.value,
      itemStyle: {
        color: function(params) {
          const colorList = ['#91cc75', '#fac858', '#ee6666', '#73c0de', '#3ba272']
          return colorList[params.dataIndex]
        }
      }
    }
  ]
}))

const masteryOption = computed(() => ({
  tooltip: {
    trigger: 'axis'
  },
  grid: {
    left: '3%',
    right: '4%',
    bottom: '3%',
    containLabel: true
  },
  xAxis: {
    type: 'category',
    boundaryGap: false,
    data: masteryTrendData.value.dates
  },
  yAxis: {
    type: 'value',
    min: 0,
    max: 100,
    axisLabel: {
      formatter: '{value}%'
    }
  },
  series: [
    {
      name: '掌握度',
      type: 'line',
      smooth: true,
      data: masteryTrendData.value.values,
      areaStyle: {
        opacity: 0.1
      },
      lineStyle: {
        width: 3
      },
      itemStyle: {
        borderWidth: 2
      }
    }
  ]
}))

const errorPatternOption = computed(() => ({
  tooltip: {
    trigger: 'item',
    formatter: '{b}: {c} ({d}%)'
  },
  legend: {
    orient: 'vertical',
    right: 10,
    top: 'center',
    type: 'scroll'
  },
  series: [
    {
      type: 'pie',
      radius: '50%',
      data: errorPatternData.value,
      emphasis: {
        itemStyle: {
          shadowBlur: 10,
          shadowOffsetX: 0,
          shadowColor: 'rgba(0, 0, 0, 0.5)'
        }
      }
    }
  ]
}))

const timeDistributionOption = computed(() => ({
  tooltip: {
    trigger: 'axis',
    axisPointer: {
      type: 'shadow'
    }
  },
  grid: {
    left: '3%',
    right: '4%',
    bottom: '3%',
    containLabel: true
  },
  xAxis: {
    type: 'category',
    data: timeDistributionData.value.labels
  },
  yAxis: {
    type: 'value',
    name: '分钟'
  },
  series: [
    {
      type: 'bar',
      data: timeDistributionData.value.values,
      itemStyle: {
        color: '#409EFF'
      }
    }
  ]
}))

// 数据获取
const fetchStatistics = async () => {
  try {
    const response = await getProblemStatistics({
      timeRange: knowledgePointTimeRange.value,
      subject: difficultySubject.value,
      dateRange: masteryDateRange.value,
      errorPatternSubject: errorPatternSubject.value,
      timeDistributionType: timeDistributionType.value
    })
    
    if (response.success) {
      const data = response.data
      // 更新概览数据
      overviewStats.value[0].value = data.totalProblems
      overviewStats.value[1].value = data.pendingReview
      overviewStats.value[2].value = `${data.averageMastery}%`
      overviewStats.value[3].value = data.newThisWeek
      
      // 更新图表数据
      knowledgePointData.value = data.knowledgePointDistribution
      difficultyData.value = data.difficultyDistribution
      masteryTrendData.value = data.masteryTrend
      errorPatternData.value = data.errorPatterns
      timeDistributionData.value = data.timeDistribution
    } else {
      ElMessage.error('获取统计数据失败')
    }
  } catch (error) {
    console.error('获取统计数据出错:', error)
    ElMessage.error('获取统计数据出错')
  }
}

// 监听器
watch([
  knowledgePointTimeRange,
  difficultySubject,
  masterySubject,
  masteryDateRange,
  errorPatternSubject,
  timeDistributionType
], () => {
  fetchStatistics()
})

// 生命周期钩子
onMounted(() => {
  fetchStatistics()
})
</script>

<style scoped>
.problem-statistics {
  padding: 20px;
}

.stat-card {
  margin-bottom: 20px;
  display: flex;
  align-items: center;
}

.stat-icon {
  width: 48px;
  height: 48px;
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 16px;
}

.stat-icon :deep(svg) {
  width: 24px;
  height: 24px;
  color: #fff;
}

.stat-icon.total {
  background-color: #409EFF;
}

.stat-icon.review {
  background-color: #E6A23C;
}

.stat-icon.mastery {
  background-color: #67C23A;
}

.stat-icon.new {
  background-color: #F56C6C;
}

.stat-content {
  flex: 1;
}

.stat-value {
  font-size: 24px;
  font-weight: bold;
  line-height: 1;
  margin-bottom: 8px;
}

.stat-label {
  font-size: 14px;
  color: #909399;
}

.chart-row {
  margin-bottom: 20px;
}

.chart-card {
  height: 400px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.header-actions {
  display: flex;
  gap: 12px;
}

.chart-container {
  height: 320px;
}

.chart {
  height: 100%;
  width: 100%;
}
</style> 