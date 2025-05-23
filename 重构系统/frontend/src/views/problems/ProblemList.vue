<template>
  <div class="problem-list">
    <div class="page-header">
      <h1>错题管理</h1>
      <div class="header-actions">
        <el-button type="primary" @click="$router.push('/problems/create')">
          <el-icon><Plus /></el-icon>
          添加错题
        </el-button>
      </div>
    </div>

    <!-- 搜索和筛选 -->
    <el-card class="filter-card">
      <el-form :inline="true">
        <el-form-item label="学科">
          <el-select v-model="filters.subject" placeholder="全部学科" clearable>
            <el-option label="数学" value="math" />
            <el-option label="英语" value="english" />
            <el-option label="政治" value="politics" />
            <el-option label="专业课" value="professional" />
          </el-select>
        </el-form-item>
        <el-form-item label="分类">
          <el-input v-model="filters.category" placeholder="输入分类" clearable />
        </el-form-item>
        <el-form-item>
          <el-button type="primary" @click="fetchProblems">
            <el-icon><Search /></el-icon>
            搜索
          </el-button>
          <el-button @click="resetFilters">重置</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- 批量操作工具栏 -->
    <el-card class="batch-actions" v-if="selectedProblems.length > 0">
      <div class="batch-info">
        <span>已选择 {{ selectedProblems.length }} 道题目</span>
        <el-button @click="clearSelection" link>清空选择</el-button>
      </div>
      <div class="batch-buttons">
        <el-button 
          type="primary" 
          @click="batchAIAnalysis" 
          :loading="batchAnalysisLoading"
          :disabled="selectedProblems.length === 0"
        >
          <el-icon><MagicStick /></el-icon>
          {{ batchAnalysisLoading ? 'AI分析中...' : 'AI批量分析' }}
        </el-button>
        <el-button 
          type="warning" 
          @click="batchReview"
          :disabled="selectedProblems.length === 0"
        >
          <el-icon><RefreshRight /></el-icon>
          批量复习
        </el-button>
        <el-button 
          type="danger" 
          @click="batchDelete"
          :disabled="selectedProblems.length === 0"
        >
          <el-icon><Delete /></el-icon>
          批量删除
        </el-button>
      </div>
    </el-card>

    <!-- 错题列表 -->
    <el-card>
      <el-table
        v-loading="loading"
        :data="problems"
        style="width: 100%"
        @selection-change="handleSelectionChange"
      >
        <el-table-column type="selection" width="55" />
        
        <el-table-column prop="title" label="题目" min-width="200" />
        <el-table-column prop="subject" label="学科" width="100">
          <template #default="{ row }">
            <el-tag>{{ getSubjectName(row.subject) }}</el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="category" label="分类" width="120" />
        <el-table-column prop="difficulty_level" label="难度" width="120">
          <template #default="{ row }">
            <el-rate
              v-model="row.difficulty_level"
              disabled
              :max="5"
              :colors="['#99A9BF', '#F7BA2A', '#FF9900']"
            />
          </template>
        </el-table-column>
        <el-table-column prop="mastery_level" label="掌握度" width="120">
          <template #default="{ row }">
            <el-progress
              :percentage="Math.round(row.mastery_level * 100)"
              :color="getMasteryColor(row.mastery_level)"
            />
          </template>
        </el-table-column>
        <el-table-column prop="review_count" label="复习次数" width="100" />
        
        <!-- AI分析状态 -->
        <el-table-column label="AI分析" width="100" align="center">
          <template #default="{ row }">
            <el-tag 
              v-if="row.ai_analyzed" 
              type="success" 
              size="small"
            >
              已分析
            </el-tag>
            <el-tag 
              v-else 
              type="info" 
              size="small"
            >
              未分析
            </el-tag>
          </template>
        </el-table-column>
        
        <el-table-column prop="created_at" label="创建时间" width="180">
          <template #default="{ row }">
            {{ formatDate(row.created_at) }}
          </template>
        </el-table-column>
        
        <el-table-column label="操作" width="250" fixed="right">
          <template #default="{ row }">
            <el-button link type="primary" @click="viewProblem(row.id)">
              查看
            </el-button>
            <el-button link type="primary" @click="editProblem(row.id)">
              编辑
            </el-button>
            <el-button link type="primary" @click="reviewProblem(row)">
              复习
            </el-button>
            <el-button 
              link 
              type="warning" 
              @click="singleAIAnalysis(row)"
              :loading="row.analyzing"
            >
              {{ row.analyzing ? '分析中' : 'AI分析' }}
            </el-button>
            <el-button link type="danger" @click="deleteProblem(row)">
              删除
            </el-button>
          </template>
        </el-table-column>
      </el-table>

      <!-- 分页 -->
      <el-pagination
        v-model:current-page="pagination.current"
        v-model:page-size="pagination.pageSize"
        :total="pagination.total"
        :page-sizes="[10, 20, 50, 100]"
        layout="total, sizes, prev, pager, next, jumper"
        @size-change="fetchProblems"
        @current-change="fetchProblems"
        style="margin-top: 20px"
      />
    </el-card>

    <!-- 批量AI分析结果对话框 -->
    <el-dialog
      v-model="analysisResultVisible"
      title="批量AI分析结果"
      width="800px"
      :close-on-click-modal="false"
    >
      <div class="analysis-results">
        <div class="analysis-overview">
          <el-statistic title="成功分析" :value="analysisResult.success_count" />
          <el-statistic title="失败分析" :value="analysisResult.fail_count" />
          <el-statistic title="总耗时" :value="analysisResult.total_time" suffix="秒" />
        </div>

        <el-tabs v-model="resultActiveTab">
          <el-tab-pane label="分析概览" name="overview">
            <div class="overview-content">
              <h4>整体分析结果</h4>
              <div class="analysis-summary">
                <div class="summary-item">
                  <span class="label">主要薄弱知识点：</span>
                  <el-tag 
                    v-for="point in analysisResult.weak_knowledge_points" 
                    :key="point"
                    class="knowledge-tag"
                  >
                    {{ point }}
                  </el-tag>
                </div>
                
                <div class="summary-item">
                  <span class="label">错误类型分布：</span>
                  <div class="error-distribution">
                    <div 
                      v-for="(count, type) in analysisResult.error_types" 
                      :key="type"
                      class="error-type-item"
                    >
                      <span>{{ type }}: </span>
                      <el-progress 
                        :percentage="(count / selectedProblems.length) * 100" 
                        :show-text="false"
                        class="error-progress"
                      />
                      <span>{{ count }}道</span>
                    </div>
                  </div>
                </div>

                <div class="summary-item">
                  <span class="label">AI学习建议：</span>
                  <ul class="ai-suggestions">
                    <li v-for="suggestion in analysisResult.study_suggestions" :key="suggestion">
                      {{ suggestion }}
                    </li>
                  </ul>
                </div>
              </div>
            </div>
          </el-tab-pane>

          <el-tab-pane label="详细结果" name="details">
            <el-table :data="analysisResult.details" stripe>
              <el-table-column prop="title" label="题目" width="200" />
              <el-table-column prop="status" label="分析状态" width="100">
                <template #default="{ row }">
                  <el-tag :type="row.status === 'success' ? 'success' : 'danger'">
                    {{ row.status === 'success' ? '成功' : '失败' }}
                  </el-tag>
                </template>
              </el-table-column>
              <el-table-column prop="knowledge_points" label="提取知识点" min-width="150">
                <template #default="{ row }">
                  <el-tag 
                    v-for="point in row.knowledge_points" 
                    :key="point"
                    size="small"
                    class="knowledge-tag-small"
                  >
                    {{ point }}
                  </el-tag>
                </template>
              </el-table-column>
              <el-table-column prop="error_reason" label="错误原因" min-width="200" />
              <el-table-column prop="analysis_time" label="分析耗时" width="100">
                <template #default="{ row }">
                  {{ row.analysis_time }}ms
                </template>
              </el-table-column>
            </el-table>
          </el-tab-pane>

          <el-tab-pane label="数据统计" name="statistics">
            <div class="statistics-content">
              <el-row :gutter="20">
                <el-col :span="12">
                  <div class="chart-container">
                    <h4>学科分布</h4>
                    <canvas ref="subjectChartRef" width="300" height="200"></canvas>
                  </div>
                </el-col>
                <el-col :span="12">
                  <div class="chart-container">
                    <h4>难度分布</h4>
                    <canvas ref="difficultyChartRef" width="300" height="200"></canvas>
                  </div>
                </el-col>
              </el-row>
            </div>
          </el-tab-pane>
        </el-tabs>
      </div>

      <template #footer>
        <div class="dialog-footer">
          <el-button @click="analysisResultVisible = false">关闭</el-button>
          <el-button type="primary" @click="applyBatchAnalysisResults">
            应用分析结果
          </el-button>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted, nextTick } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { 
  Plus, 
  Search, 
  MagicStick, 
  RefreshRight, 
  Delete 
} from '@element-plus/icons-vue'
import { Chart, registerables } from 'chart.js'
import { problemAPI, aiAPI } from '@/utils/api'
import dayjs from 'dayjs'

// 注册Chart.js组件
Chart.register(...registerables)

const router = useRouter()

const loading = ref(false)
const problems = ref([])
const selectedProblems = ref([])
const batchAnalysisLoading = ref(false)
const analysisResultVisible = ref(false)
const resultActiveTab = ref('overview')
const subjectChartRef = ref()
const difficultyChartRef = ref()

const filters = reactive({
  subject: '',
  category: ''
})

const pagination = reactive({
  current: 1,
  pageSize: 20,
  total: 0
})

const analysisResult = reactive({
  success_count: 0,
  fail_count: 0,
  total_time: 0,
  weak_knowledge_points: [],
  error_types: {},
  study_suggestions: [],
  details: []
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

const getMasteryColor = (mastery) => {
  if (mastery >= 0.8) return '#67C23A'
  if (mastery >= 0.6) return '#E6A23C'
  return '#F56C6C'
}

const fetchProblems = async () => {
  loading.value = true
  try {
    const { data } = await problemAPI.getList({
      ...filters,
      skip: (pagination.current - 1) * pagination.pageSize,
      limit: pagination.pageSize
    })
    
    // 为每个问题添加分析状态
    problems.value = data.map(problem => ({
      ...problem,
      analyzing: false,
      ai_analyzed: problem.ai_analyzed || false
    }))
    
    // 实际应该从后端返回总数
    pagination.total = data.length * 5 // 模拟总数
  } catch (error) {
    ElMessage.error('获取错题列表失败')
  } finally {
    loading.value = false
  }
}

const resetFilters = () => {
  filters.subject = ''
  filters.category = ''
  fetchProblems()
}

// 处理选择变化
const handleSelectionChange = (selection) => {
  selectedProblems.value = selection
}

// 清空选择
const clearSelection = () => {
  selectedProblems.value = []
}

// 单个AI分析
const singleAIAnalysis = async (problem) => {
  problem.analyzing = true
  
  try {
    const analysisRequest = {
      task_type: 'PROBLEM_ANALYSIS',
      content: {
        problem_content: problem.content,
        subject: problem.subject,
        user_answer: problem.user_answer,
        correct_answer: problem.correct_answer
      },
      max_tokens: 1000,
      temperature: 0.7
    }

    const response = await aiAPI.callAI(analysisRequest)
    
    if (response.data.success) {
      ElMessage.success(`"${problem.title}" AI分析完成`)
      problem.ai_analyzed = true
      
      // 这里可以更新问题的AI分析结果到后端
      // await problemAPI.updateAIAnalysis(problem.id, response.data.content)
      
    } else {
      ElMessage.error(`"${problem.title}" AI分析失败`)
    }
    
  } catch (error) {
    ElMessage.error(`"${problem.title}" AI分析出错`)
  } finally {
    problem.analyzing = false
  }
}

// 批量AI分析
const batchAIAnalysis = async () => {
  if (selectedProblems.value.length === 0) {
    ElMessage.warning('请先选择要分析的题目')
    return
  }

  batchAnalysisLoading.value = true
  const startTime = Date.now()
  
  // 重置分析结果
  Object.assign(analysisResult, {
    success_count: 0,
    fail_count: 0,
    total_time: 0,
    weak_knowledge_points: [],
    error_types: {},
    study_suggestions: [],
    details: []
  })

  try {
    // 构建批量分析请求
    const batchRequest = {
      task_type: 'BATCH_PROBLEM_ANALYSIS',
      content: {
        problems: selectedProblems.value.map(p => ({
          id: p.id,
          title: p.title,
          content: p.content,
          subject: p.subject,
          category: p.category,
          user_answer: p.user_answer,
          correct_answer: p.correct_answer
        }))
      },
      max_tokens: 3000,
      temperature: 0.7
    }

    const response = await aiAPI.callAI(batchRequest)
    
    if (response.data.success) {
      try {
        const result = JSON.parse(response.data.content)
        
        // 更新分析结果
        const endTime = Date.now()
        Object.assign(analysisResult, {
          success_count: result.success_count || 0,
          fail_count: result.fail_count || 0,
          total_time: ((endTime - startTime) / 1000).toFixed(1),
          weak_knowledge_points: result.weak_knowledge_points || [],
          error_types: result.error_types || {},
          study_suggestions: result.study_suggestions || [],
          details: result.details || []
        })

        // 更新问题状态
        selectedProblems.value.forEach(problem => {
          const detail = result.details?.find(d => d.id === problem.id)
          if (detail && detail.status === 'success') {
            problem.ai_analyzed = true
          }
        })

        analysisResultVisible.value = true
        
        // 绘制图表
        await nextTick()
        drawCharts()
        
        ElMessage.success('批量AI分析完成')
        
      } catch (parseError) {
        ElMessage.error('AI响应格式错误')
      }
    } else {
      ElMessage.error('批量AI分析失败: ' + response.data.error)
    }
    
  } catch (error) {
    console.error('批量AI分析错误:', error)
    ElMessage.error('批量AI分析失败，请稍后重试')
  } finally {
    batchAnalysisLoading.value = false
  }
}

// 绘制统计图表
const drawCharts = () => {
  // 学科分布图
  if (subjectChartRef.value) {
    const subjectData = {}
    selectedProblems.value.forEach(p => {
      subjectData[p.subject] = (subjectData[p.subject] || 0) + 1
    })

    new Chart(subjectChartRef.value, {
      type: 'pie',
      data: {
        labels: Object.keys(subjectData).map(s => getSubjectName(s)),
        datasets: [{
          data: Object.values(subjectData),
          backgroundColor: ['#409EFF', '#67C23A', '#E6A23C', '#F56C6C']
        }]
      },
      options: {
        responsive: true,
        maintainAspectRatio: false
      }
    })
  }

  // 难度分布图
  if (difficultyChartRef.value) {
    const difficultyData = {}
    selectedProblems.value.forEach(p => {
      const level = Math.floor(p.difficulty_level || 1)
      difficultyData[level] = (difficultyData[level] || 0) + 1
    })

    new Chart(difficultyChartRef.value, {
      type: 'bar',
      data: {
        labels: Object.keys(difficultyData).map(d => `${d}星`),
        datasets: [{
          label: '题目数量',
          data: Object.values(difficultyData),
          backgroundColor: '#409EFF'
        }]
      },
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
}

// 应用批量分析结果
const applyBatchAnalysisResults = () => {
  // 这里可以将分析结果应用到对应的题目上
  ElMessage.success('分析结果已应用')
  analysisResultVisible.value = false
  fetchProblems() // 刷新列表
}

// 批量复习
const batchReview = async () => {
  try {
    const { value: mastery } = await ElMessageBox.prompt(
      `为选中的 ${selectedProblems.value.length} 道题目设置掌握程度（0-1）`,
      '批量复习',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        inputPattern: /^(0(\.\d+)?|1(\.0+)?)$/,
        inputErrorMessage: '请输入0到1之间的数字'
      }
    )

    // 批量更新掌握程度
    const promises = selectedProblems.value.map(p => 
      problemAPI.review(p.id, parseFloat(mastery))
    )
    
    await Promise.all(promises)
    ElMessage.success('批量复习记录已更新')
    clearSelection()
    fetchProblems()
    
  } catch (error) {
    // 用户取消
  }
}

// 批量删除
const batchDelete = async () => {
  try {
    await ElMessageBox.confirm(
      `确定要删除选中的 ${selectedProblems.value.length} 道题目吗？`,
      '批量删除确认',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )

    const promises = selectedProblems.value.map(p => problemAPI.delete(p.id))
    await Promise.all(promises)
    
    ElMessage.success('批量删除成功')
    clearSelection()
    fetchProblems()
    
  } catch (error) {
    // 用户取消
  }
}

const viewProblem = (id) => {
  router.push(`/problems/${id}`)
}

const editProblem = (id) => {
  router.push(`/problems/${id}/edit`)
}

const reviewProblem = async (problem) => {
  try {
    const { value: mastery } = await ElMessageBox.prompt(
      '请输入掌握程度（0-1）',
      '复习错题',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        inputPattern: /^(0(\.\d+)?|1(\.0+)?)$/,
        inputErrorMessage: '请输入0到1之间的数字'
      }
    )
    
    await problemAPI.review(problem.id, parseFloat(mastery))
    ElMessage.success('复习记录已更新')
    fetchProblems()
  } catch {
    // 用户取消
  }
}

const deleteProblem = async (problem) => {
  try {
    await ElMessageBox.confirm(
      `确定要删除"${problem.title}"吗？`,
      '删除确认',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )
    
    await problemAPI.delete(problem.id)
    ElMessage.success('删除成功')
    fetchProblems()
  } catch {
    // 用户取消
  }
}

onMounted(() => {
  fetchProblems()
})
</script>

<style scoped>
.problem-list {
  padding: 20px;
}

.page-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 20px;
}

.page-header h1 {
  margin: 0;
  color: #303133;
}

.header-actions {
  display: flex;
  gap: 12px;
}

.filter-card {
  margin-bottom: 20px;
}

.batch-actions {
  margin-bottom: 20px;
  padding: 16px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  color: white;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.batch-info {
  display: flex;
  align-items: center;
  gap: 12px;
}

.batch-buttons {
  display: flex;
  gap: 12px;
}

.analysis-results {
  max-height: 600px;
  overflow-y: auto;
}

.analysis-overview {
  display: flex;
  justify-content: space-around;
  margin-bottom: 20px;
  padding: 20px;
  background: #f5f7fa;
  border-radius: 8px;
}

.overview-content h4 {
  margin: 0 0 16px 0;
  color: #303133;
}

.analysis-summary {
  space-y: 16px;
}

.summary-item {
  margin-bottom: 16px;
}

.label {
  font-weight: bold;
  color: #303133;
  display: block;
  margin-bottom: 8px;
}

.knowledge-tag {
  margin-right: 8px;
  margin-bottom: 4px;
}

.knowledge-tag-small {
  margin-right: 4px;
  margin-bottom: 2px;
}

.error-distribution {
  space-y: 8px;
}

.error-type-item {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 8px;
}

.error-progress {
  flex: 1;
  max-width: 200px;
}

.ai-suggestions {
  margin: 0;
  padding-left: 20px;
}

.ai-suggestions li {
  margin-bottom: 8px;
  line-height: 1.5;
}

.statistics-content h4 {
  text-align: center;
  margin-bottom: 16px;
}

.chart-container {
  height: 200px;
  position: relative;
}

.dialog-footer {
  text-align: right;
}
</style> 