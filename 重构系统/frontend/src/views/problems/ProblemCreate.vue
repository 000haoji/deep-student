<template>
  <div class="problem-create">
    <div class="page-header">
      <h1>创建错题</h1>
    </div>

    <el-card>
      <el-form
        ref="formRef"
        :model="form"
        :rules="rules"
        label-width="100px"
      >
        <el-form-item label="题目标题" prop="title">
          <el-input
            v-model="form.title"
            placeholder="请输入题目标题"
            maxlength="200"
            show-word-limit
          />
        </el-form-item>

        <el-form-item label="学科" prop="subject">
          <el-select v-model="form.subject" placeholder="请选择学科">
            <el-option label="数学" value="math" />
            <el-option label="英语" value="english" />
            <el-option label="政治" value="politics" />
            <el-option label="专业课" value="professional" />
          </el-select>
        </el-form-item>

        <el-form-item label="分类" prop="category">
          <el-input
            v-model="form.category"
            placeholder="如：微积分、语法等"
          />
        </el-form-item>

        <el-form-item label="题目内容" prop="content">
          <el-input
            v-model="form.content"
            type="textarea"
            :rows="5"
            placeholder="请输入题目内容"
          />
          <!-- AI分析按钮 -->
          <div class="ai-analysis-section" v-if="form.content.trim()">
            <el-button 
              type="primary" 
              :icon="aiAnalysisLoading ? '' : 'MagicStick'"
              @click="analyzeWithAI" 
              :loading="aiAnalysisLoading"
              class="ai-analysis-btn"
            >
              {{ aiAnalysisLoading ? 'AI分析中...' : 'AI智能分析' }}
            </el-button>
            <span class="ai-tip">AI将自动分析题目内容，提取知识点并生成错误分析</span>
          </div>
        </el-form-item>

        <el-form-item label="我的答案">
          <el-input
            v-model="form.user_answer"
            type="textarea"
            :rows="3"
            placeholder="记录你的错误答案"
          />
        </el-form-item>

        <el-form-item label="正确答案">
          <el-input
            v-model="form.correct_answer"
            type="textarea"
            :rows="3"
            placeholder="输入正确答案"
          />
        </el-form-item>

        <!-- 知识点字段 -->
        <el-form-item label="知识点">
          <el-select
            v-model="form.knowledge_points"
            multiple
            filterable
            allow-create
            default-first-option
            placeholder="选择或创建知识点"
          >
            <el-option
              v-for="point in commonKnowledgePoints"
              :key="point"
              :label="point"
              :value="point"
            />
          </el-select>
          <div class="form-tip">AI分析后会自动提取知识点</div>
        </el-form-item>

        <el-form-item label="错误分析">
          <el-input
            v-model="form.error_analysis"
            type="textarea"
            :rows="4"
            placeholder="分析错误原因"
          />
          <div class="form-tip">AI可以帮助生成详细的错误分析</div>
        </el-form-item>

        <!-- AI建议的相似题目 -->
        <el-form-item label="相似题目" v-if="similarProblems.length > 0">
          <div class="similar-problems">
            <div 
              v-for="(problem, index) in similarProblems" 
              :key="index"
              class="similar-problem-item"
            >
              <el-card class="similar-card">
                <div class="similar-content">
                  <h4>{{ problem.title }}</h4>
                  <p class="similar-preview">{{ problem.content.substring(0, 100) }}...</p>
                  <div class="similar-meta">
                    <el-tag size="small">{{ problem.subject }}</el-tag>
                    <el-tag size="small" type="info">{{ problem.category }}</el-tag>
                  </div>
                </div>
                <div class="similar-actions">
                  <el-button size="small" @click="viewSimilarProblem(problem)">查看</el-button>
                </div>
              </el-card>
            </div>
          </div>
        </el-form-item>

        <el-form-item label="标签">
          <el-select
            v-model="form.tags"
            multiple
            filterable
            allow-create
            default-first-option
            placeholder="选择或创建标签"
          >
            <el-option
              v-for="tag in commonTags"
              :key="tag"
              :label="tag"
              :value="tag"
            />
          </el-select>
        </el-form-item>

        <el-form-item>
          <el-button type="primary" @click="handleSubmit" :loading="loading">
            创建错题
          </el-button>
          <el-button @click="$router.back()">取消</el-button>
        </el-form-item>
      </el-form>
    </el-card>

    <!-- AI分析结果对话框 -->
    <el-dialog
      v-model="aiResultVisible"
      title="AI分析结果"
      width="600px"
      :close-on-click-modal="false"
    >
      <div class="ai-result-content">
        <el-tabs v-model="activeTab">
          <el-tab-pane label="知识点分析" name="knowledge">
            <div class="analysis-section">
              <h4>提取的知识点：</h4>
              <div class="knowledge-points">
                <el-tag 
                  v-for="point in aiAnalysisResult.knowledge_points" 
                  :key="point"
                  class="knowledge-tag"
                >
                  {{ point }}
                </el-tag>
              </div>
            </div>
          </el-tab-pane>
          
          <el-tab-pane label="错误分析" name="analysis">
            <div class="analysis-section">
              <h4>AI生成的错误分析：</h4>
              <div class="error-analysis-content">
                {{ aiAnalysisResult.error_analysis }}
              </div>
            </div>
          </el-tab-pane>
          
          <el-tab-pane label="学习建议" name="suggestions">
            <div class="analysis-section">
              <h4>学习建议：</h4>
              <ul class="suggestions-list">
                <li v-for="suggestion in aiAnalysisResult.study_suggestions" :key="suggestion">
                  {{ suggestion }}
                </li>
              </ul>
            </div>
          </el-tab-pane>
        </el-tabs>
      </div>
      
      <template #footer>
        <div class="dialog-footer">
          <el-button @click="aiResultVisible = false">取消</el-button>
          <el-button type="primary" @click="applyAIResult">应用分析结果</el-button>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { MagicStick } from '@element-plus/icons-vue'
import { problemAPI, aiAPI } from '@/utils/api'

const router = useRouter()
const formRef = ref()
const loading = ref(false)
const aiAnalysisLoading = ref(false)
const aiResultVisible = ref(false)
const activeTab = ref('knowledge')

const form = reactive({
  title: '',
  subject: '',
  category: '',
  content: '',
  user_answer: '',
  correct_answer: '',
  error_analysis: '',
  knowledge_points: [],
  tags: []
})

const rules = {
  title: [
    { required: true, message: '请输入题目标题', trigger: 'blur' }
  ],
  subject: [
    { required: true, message: '请选择学科', trigger: 'change' }
  ],
  content: [
    { required: true, message: '请输入题目内容', trigger: 'blur' }
  ]
}

const commonTags = [
  '重点',
  '难点',
  '易错',
  '常考',
  '基础',
  '提高',
  '概念题',
  '计算题',
  '应用题'
]

const commonKnowledgePoints = [
  '函数',
  '导数',
  '积分',
  '极限',
  '微分方程',
  '线性代数',
  '概率论',
  '统计学',
  '几何',
  '代数'
]

const similarProblems = ref([])
const aiAnalysisResult = reactive({
  knowledge_points: [],
  error_analysis: '',
  study_suggestions: []
})

// AI分析功能
const analyzeWithAI = async () => {
  if (!form.content.trim()) {
    ElMessage.warning('请先输入题目内容')
    return
  }

  aiAnalysisLoading.value = true
  
  try {
    const analysisRequest = {
      task_type: 'PROBLEM_ANALYSIS',
      content: {
        problem_content: form.content,
        subject: form.subject,
        user_answer: form.user_answer,
        correct_answer: form.correct_answer
      },
      max_tokens: 1500,
      temperature: 0.7
    }

    const response = await aiAPI.callAI(analysisRequest)
    
    if (response.data.success) {
      const result = response.data.content
      
      // 解析AI返回的结果
      try {
        const parsedResult = JSON.parse(result)
        
        Object.assign(aiAnalysisResult, {
          knowledge_points: parsedResult.knowledge_points || [],
          error_analysis: parsedResult.error_analysis || '',
          study_suggestions: parsedResult.study_suggestions || []
        })

        // 如果有相似题目推荐，也显示出来
        if (parsedResult.similar_problems) {
          similarProblems.value = parsedResult.similar_problems
        }

        aiResultVisible.value = true
        
      } catch (parseError) {
        // 如果返回的不是JSON格式，直接作为错误分析使用
        aiAnalysisResult.error_analysis = result
        aiAnalysisResult.knowledge_points = []
        aiAnalysisResult.study_suggestions = []
        aiResultVisible.value = true
      }
      
      ElMessage.success('AI分析完成')
    } else {
      ElMessage.error('AI分析失败: ' + response.data.error)
    }
    
  } catch (error) {
    console.error('AI分析错误:', error)
    ElMessage.error('AI分析失败，请稍后重试')
  } finally {
    aiAnalysisLoading.value = false
  }
}

// 应用AI分析结果
const applyAIResult = () => {
  // 应用知识点
  if (aiAnalysisResult.knowledge_points.length > 0) {
    form.knowledge_points = [...aiAnalysisResult.knowledge_points]
  }
  
  // 应用错误分析
  if (aiAnalysisResult.error_analysis) {
    form.error_analysis = aiAnalysisResult.error_analysis
  }
  
  // 自动生成标签
  const aiTags = []
  if (aiAnalysisResult.knowledge_points.length > 3) {
    aiTags.push('复杂')
  }
  if (aiAnalysisResult.error_analysis.includes('概念')) {
    aiTags.push('概念题')
  }
  if (aiAnalysisResult.error_analysis.includes('计算')) {
    aiTags.push('计算题')
  }
  
  form.tags = [...new Set([...form.tags, ...aiTags])]
  
  aiResultVisible.value = false
  ElMessage.success('AI分析结果已应用到表单')
}

// 查看相似题目
const viewSimilarProblem = async (problem) => {
  try {
    await ElMessageBox.confirm(
      `题目：${problem.title}\n\n内容：${problem.content}\n\n是否要基于此题目创建新的错题记录？`,
      '相似题目详情',
      {
        confirmButtonText: '基于此题创建',
        cancelButtonText: '仅查看',
        type: 'info',
        customClass: 'similar-problem-dialog'
      }
    )
    
    // 用户选择基于相似题目创建
    Object.assign(form, {
      title: '（参考）' + problem.title,
      subject: problem.subject,
      category: problem.category,
      content: problem.content
    })
    
    ElMessage.success('已将相似题目信息填入表单')
    
  } catch (error) {
    // 用户点击了"仅查看"或取消
  }
}

const handleSubmit = async () => {
  const valid = await formRef.value.validate()
  if (!valid) return

  loading.value = true
  try {
    await problemAPI.create(form)
    ElMessage.success('创建成功')
    router.push('/problems')
  } catch (error) {
    ElMessage.error('创建失败')
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.problem-create {
  padding: 20px;
}

.page-header {
  margin-bottom: 20px;
}

.page-header h1 {
  margin: 0;
}

.ai-analysis-section {
  margin-top: 10px;
  padding: 12px;
  background: #f0f9ff;
  border: 1px solid #b3d8ff;
  border-radius: 6px;
  display: flex;
  align-items: center;
  gap: 12px;
}

.ai-analysis-btn {
  flex-shrink: 0;
}

.ai-tip {
  font-size: 12px;
  color: #409eff;
  flex: 1;
}

.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 5px;
}

.similar-problems {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
  gap: 12px;
  margin-top: 10px;
}

.similar-problem-item {
  border: 1px solid #e4e7ed;
  border-radius: 6px;
  overflow: hidden;
}

.similar-card {
  margin: 0;
}

.similar-content h4 {
  margin: 0 0 8px 0;
  font-size: 14px;
  color: #303133;
}

.similar-preview {
  font-size: 12px;
  color: #606266;
  margin: 8px 0;
  line-height: 1.4;
}

.similar-meta {
  display: flex;
  gap: 6px;
  margin-top: 8px;
}

.similar-actions {
  margin-top: 12px;
  text-align: right;
}

.ai-result-content {
  max-height: 400px;
  overflow-y: auto;
}

.analysis-section h4 {
  margin: 0 0 12px 0;
  color: #303133;
}

.knowledge-points {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.knowledge-tag {
  margin: 0;
}

.error-analysis-content {
  padding: 12px;
  background: #f5f7fa;
  border-radius: 6px;
  line-height: 1.6;
  color: #303133;
}

.suggestions-list {
  margin: 0;
  padding-left: 20px;
}

.suggestions-list li {
  margin-bottom: 8px;
  line-height: 1.5;
  color: #303133;
}

.dialog-footer {
  text-align: right;
}

:global(.similar-problem-dialog) {
  white-space: pre-line;
}
</style> 