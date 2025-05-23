<template>
  <div class="ai-review-suggestion">
    <el-card>
      <template #header>
        <div class="card-header">
          <span class="title">
            <el-icon><MagicStick /></el-icon>
            AI智能复习建议
          </span>
          <div class="header-actions">
            <el-button 
              type="primary" 
              size="small" 
              @click="generateSuggestions" 
              :loading="loading"
            >
              {{ loading ? '分析中...' : '重新分析' }}
            </el-button>
          </div>
        </div>
      </template>

      <div v-loading="loading" class="suggestion-content">
        <!-- 复习概览 -->
        <div class="review-overview" v-if="suggestions.overview">
          <el-row :gutter="20">
            <el-col :span="6">
              <div class="overview-item">
                <div class="overview-icon needs-review">
                  <el-icon><Clock /></el-icon>
                </div>
                <div class="overview-content">
                  <div class="overview-value">{{ suggestions.overview.needs_review_count }}</div>
                  <div class="overview-label">待复习</div>
                </div>
              </div>
            </el-col>
            
            <el-col :span="6">
              <div class="overview-item">
                <div class="overview-icon urgent">
                  <el-icon><Warning /></el-icon>
                </div>
                <div class="overview-content">
                  <div class="overview-value">{{ suggestions.overview.urgent_count }}</div>
                  <div class="overview-label">紧急复习</div>
                </div>
              </div>
            </el-col>
            
            <el-col :span="6">
              <div class="overview-item">
                <div class="overview-icon weak-points">
                  <el-icon><TrendCharts /></el-icon>
                </div>
                <div class="overview-content">
                  <div class="overview-value">{{ suggestions.overview.weak_subjects_count }}</div>
                  <div class="overview-label">薄弱学科</div>
                </div>
              </div>
            </el-col>
            
            <el-col :span="6">
              <div class="overview-item">
                <div class="overview-icon estimated-time">
                  <el-icon><Timer /></el-icon>
                </div>
                <div class="overview-content">
                  <div class="overview-value">{{ suggestions.overview.estimated_time }}min</div>
                  <div class="overview-label">预计用时</div>
                </div>
              </div>
            </el-col>
          </el-row>
        </div>

        <!-- 复习计划 -->
        <div class="review-schedule" v-if="suggestions.schedule && suggestions.schedule.length > 0">
          <h3>
            <el-icon><Calendar /></el-icon>
            复习计划（基于艾宾浩斯遗忘曲线）
          </h3>
          
          <el-timeline>
            <el-timeline-item
              v-for="(plan, index) in suggestions.schedule"
              :key="index"
              :timestamp="plan.date"
              placement="top"
              :type="getTimelineType(plan.priority)"
            >
              <el-card class="schedule-card">
                <div class="schedule-header">
                  <span class="schedule-title">{{ plan.title }}</span>
                  <el-tag 
                    :type="getPriorityTagType(plan.priority)" 
                    size="small"
                  >
                    {{ getPriorityText(plan.priority) }}
                  </el-tag>
                </div>
                
                <div class="schedule-content">
                  <div class="schedule-subjects">
                    <span class="label">复习学科：</span>
                    <el-tag 
                      v-for="subject in plan.subjects" 
                      :key="subject"
                      size="small"
                      class="subject-tag"
                    >
                      {{ getSubjectName(subject) }}
                    </el-tag>
                  </div>
                  
                  <div class="schedule-problems">
                    <span class="label">题目数量：{{ plan.problem_count }}道</span>
                    <span class="time-estimate">预计用时：{{ plan.estimated_time }}分钟</span>
                  </div>
                  
                  <div class="schedule-description">
                    {{ plan.description }}
                  </div>
                </div>
                
                <div class="schedule-actions">
                  <el-button 
                    type="primary" 
                    size="small" 
                    @click="startReview(plan)"
                  >
                    开始复习
                  </el-button>
                  <el-button 
                    size="small" 
                    @click="viewProblems(plan)"
                  >
                    查看题目
                  </el-button>
                </div>
              </el-card>
            </el-timeline-item>
          </el-timeline>
        </div>

        <!-- 薄弱知识点分析 -->
        <div class="weak-analysis" v-if="suggestions.weak_points && suggestions.weak_points.length > 0">
          <h3>
            <el-icon><TrendCharts /></el-icon>
            薄弱知识点分析
          </h3>
          
          <div class="weak-points-grid">
            <div 
              v-for="point in suggestions.weak_points" 
              :key="point.knowledge_point"
              class="weak-point-card"
            >
              <div class="weak-point-header">
                <span class="knowledge-point">{{ point.knowledge_point }}</span>
                <div class="mastery-level">
                  <span class="mastery-text">掌握度</span>
                  <el-progress 
                    :percentage="point.mastery_percentage" 
                    :color="getMasteryColor(point.mastery_percentage)"
                    :show-text="false"
                    class="mastery-progress"
                  />
                  <span class="mastery-value">{{ point.mastery_percentage }}%</span>
                </div>
              </div>
              
              <div class="weak-point-content">
                <div class="error-stats">
                  <span>错误次数：{{ point.error_count }}</span>
                  <span>总次数：{{ point.total_count }}</span>
                </div>
                
                <div class="improvement-suggestion">
                  <el-icon><Lightbulb /></el-icon>
                  {{ point.suggestion }}
                </div>
                
                <div class="practice-recommendation">
                  <el-button 
                    type="warning" 
                    size="small" 
                    @click="practiceKnowledgePoint(point)"
                  >
                    针对性练习
                  </el-button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- 学习建议 -->
        <div class="study-tips" v-if="suggestions.study_tips && suggestions.study_tips.length > 0">
          <h3>
            <el-icon><Lightbulb /></el-icon>
            个性化学习建议
          </h3>
          
          <div class="tips-list">
            <el-card 
              v-for="(tip, index) in suggestions.study_tips" 
              :key="index"
              class="tip-card"
              shadow="hover"
            >
              <div class="tip-content">
                <div class="tip-category">
                  <el-tag :type="getTipCategoryType(tip.category)" size="small">
                    {{ tip.category }}
                  </el-tag>
                </div>
                <div class="tip-text">{{ tip.content }}</div>
                <div class="tip-reason" v-if="tip.reason">
                  <span class="reason-label">建议理由：</span>
                  {{ tip.reason }}
                </div>
              </div>
            </el-card>
          </div>
        </div>

        <!-- 空状态 -->
        <div v-if="!loading && !suggestions.overview" class="empty-state">
          <el-empty 
            description="暂无复习建议数据"
            :image-size="120"
          >
            <el-button type="primary" @click="generateSuggestions">
              生成AI复习建议
            </el-button>
          </el-empty>
        </div>
      </div>
    </el-card>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import {
  MagicStick,
  Clock,
  Warning,
  TrendCharts,
  Timer,
  Calendar,
  Lightbulb
} from '@element-plus/icons-vue'
import { aiAPI, problemAPI } from '@/utils/api'

const props = defineProps({
  userId: {
    type: String,
    default: null
  }
})

const emit = defineEmits(['startReview', 'viewProblems'])

const loading = ref(false)
const suggestions = reactive({
  overview: null,
  schedule: [],
  weak_points: [],
  study_tips: []
})

// 工具函数
const getSubjectName = (subject) => {
  const nameMap = {
    'math': '数学',
    'english': '英语',
    'politics': '政治',
    'professional': '专业课'
  }
  return nameMap[subject] || subject
}

const getPriorityText = (priority) => {
  const textMap = {
    'urgent': '紧急',
    'high': '高',
    'medium': '中',
    'low': '低'
  }
  return textMap[priority] || priority
}

const getPriorityTagType = (priority) => {
  const typeMap = {
    'urgent': 'danger',
    'high': 'warning',
    'medium': 'primary',
    'low': 'info'
  }
  return typeMap[priority] || 'info'
}

const getTimelineType = (priority) => {
  const typeMap = {
    'urgent': 'danger',
    'high': 'warning',
    'medium': 'primary',
    'low': 'success'
  }
  return typeMap[priority] || 'primary'
}

const getMasteryColor = (percentage) => {
  if (percentage >= 80) return '#67c23a'
  if (percentage >= 60) return '#e6a23c'
  return '#f56c6c'
}

const getTipCategoryType = (category) => {
  const typeMap = {
    '学习方法': 'primary',
    '时间安排': 'success',
    '复习策略': 'warning',
    '注意事项': 'info'
  }
  return typeMap[category] || 'default'
}

// 生成复习建议
const generateSuggestions = async () => {
  loading.value = true
  
  try {
    // 首先获取用户的错题数据
    const problemsResponse = await problemAPI.getList({ 
      page: 1, 
      size: 100,
      user_id: props.userId 
    })
    
    const problems = problemsResponse.data.data || []
    
    if (problems.length === 0) {
      ElMessage.info('暂无错题数据，无法生成复习建议')
      return
    }

    // 构建AI分析请求
    const analysisRequest = {
      task_type: 'REVIEW_ANALYSIS',
      content: {
        problems: problems.map(p => ({
          id: p.id,
          subject: p.subject,
          category: p.category,
          knowledge_points: p.knowledge_points || [],
          mastery_level: p.mastery_level,
          last_reviewed: p.last_reviewed,
          created_at: p.created_at,
          error_count: p.error_count || 1
        })),
        user_preferences: {
          daily_study_time: 60, // 可以从用户设置获取
          preferred_subjects: [],
          difficulty_preference: 'mixed'
        }
      },
      max_tokens: 2000,
      temperature: 0.7
    }

    const response = await aiAPI.callAI(analysisRequest)
    
    if (response.data.success) {
      try {
        const result = JSON.parse(response.data.content)
        
        // 更新建议数据
        Object.assign(suggestions, {
          overview: result.overview || null,
          schedule: result.schedule || [],
          weak_points: result.weak_points || [],
          study_tips: result.study_tips || []
        })
        
        ElMessage.success('AI复习建议生成完成')
        
      } catch (parseError) {
        console.error('解析AI响应失败:', parseError)
        ElMessage.error('AI响应格式错误')
      }
    } else {
      ElMessage.error('生成复习建议失败: ' + response.data.error)
    }
    
  } catch (error) {
    console.error('生成复习建议错误:', error)
    ElMessage.error('生成复习建议失败，请稍后重试')
  } finally {
    loading.value = false
  }
}

// 开始复习
const startReview = (plan) => {
  emit('startReview', plan)
}

// 查看题目
const viewProblems = (plan) => {
  emit('viewProblems', plan)
}

// 针对性练习
const practiceKnowledgePoint = async (point) => {
  try {
    await ElMessageBox.confirm(
      `针对知识点"${point.knowledge_point}"进行专项练习，这将帮助提高该知识点的掌握度。`,
      '开始针对性练习',
      {
        confirmButtonText: '开始练习',
        cancelButtonText: '取消',
        type: 'info'
      }
    )
    
    // 这里可以跳转到专项练习页面或筛选相关题目
    emit('practiceKnowledgePoint', point)
    
  } catch (error) {
    // 用户取消
  }
}

// 生命周期
onMounted(() => {
  generateSuggestions()
})

// 暴露方法给父组件
defineExpose({
  generateSuggestions
})
</script>

<style scoped>
.ai-review-suggestion {
  margin-bottom: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 16px;
  font-weight: bold;
  color: #303133;
}

.suggestion-content {
  min-height: 200px;
}

.review-overview {
  margin-bottom: 30px;
  padding: 20px;
  background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
  border-radius: 12px;
  color: white;
}

.overview-item {
  display: flex;
  align-items: center;
  padding: 16px;
  background: rgba(255, 255, 255, 0.1);
  border-radius: 8px;
  backdrop-filter: blur(10px);
}

.overview-icon {
  width: 48px;
  height: 48px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  margin-right: 12px;
  font-size: 20px;
}

.overview-icon.needs-review {
  background: rgba(64, 158, 255, 0.8);
}

.overview-icon.urgent {
  background: rgba(245, 108, 108, 0.8);
}

.overview-icon.weak-points {
  background: rgba(230, 162, 60, 0.8);
}

.overview-icon.estimated-time {
  background: rgba(103, 194, 58, 0.8);
}

.overview-content {
  flex: 1;
}

.overview-value {
  font-size: 24px;
  font-weight: bold;
  margin-bottom: 4px;
}

.overview-label {
  font-size: 12px;
  opacity: 0.9;
}

.review-schedule h3,
.weak-analysis h3,
.study-tips h3 {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 20px 0 16px 0;
  color: #303133;
  font-size: 16px;
}

.schedule-card {
  margin-bottom: 0;
}

.schedule-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 12px;
}

.schedule-title {
  font-weight: bold;
  color: #303133;
}

.schedule-content {
  margin-bottom: 16px;
}

.schedule-subjects {
  margin-bottom: 8px;
}

.subject-tag {
  margin-right: 6px;
}

.schedule-problems {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
  font-size: 14px;
  color: #666;
}

.schedule-description {
  font-size: 14px;
  color: #909399;
  line-height: 1.5;
}

.schedule-actions {
  text-align: right;
}

.weak-points-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
  gap: 16px;
  margin-bottom: 20px;
}

.weak-point-card {
  padding: 16px;
  border: 1px solid #e4e7ed;
  border-radius: 8px;
  background: #fafafa;
}

.weak-point-header {
  margin-bottom: 12px;
}

.knowledge-point {
  font-weight: bold;
  color: #303133;
  display: block;
  margin-bottom: 8px;
}

.mastery-level {
  display: flex;
  align-items: center;
  gap: 8px;
}

.mastery-text {
  font-size: 12px;
  color: #909399;
}

.mastery-progress {
  flex: 1;
}

.mastery-value {
  font-size: 12px;
  font-weight: bold;
}

.weak-point-content {
  font-size: 14px;
}

.error-stats {
  display: flex;
  justify-content: space-between;
  margin-bottom: 8px;
  color: #666;
  font-size: 12px;
}

.improvement-suggestion {
  display: flex;
  align-items: flex-start;
  gap: 6px;
  margin-bottom: 12px;
  color: #303133;
  line-height: 1.4;
}

.practice-recommendation {
  text-align: right;
}

.tips-list {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 16px;
}

.tip-card {
  margin: 0;
}

.tip-content {
  line-height: 1.5;
}

.tip-category {
  margin-bottom: 8px;
}

.tip-text {
  margin-bottom: 8px;
  color: #303133;
}

.tip-reason {
  font-size: 12px;
  color: #909399;
}

.reason-label {
  font-weight: bold;
}

.empty-state {
  text-align: center;
  padding: 40px 20px;
}

.label {
  font-size: 14px;
  color: #909399;
  margin-right: 8px;
}

.time-estimate {
  color: #67c23a;
  font-weight: bold;
}
</style> 