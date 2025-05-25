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
          <!-- 图片上传 -->
          <el-upload
            class="problem-image-upload"
            :http-request="customUpload" 
            :auto-upload="false"
            :on-change="handleImageChange"
            :on-remove="handleImageRemove"
            :file-list="fileList"
            list-type="picture-card"
            accept="image/*"
          >
            <el-icon><Plus /></el-icon>
          </el-upload>
          <!-- AI分析按钮 -->
          <div class="ai-analysis-section" v-if="form.content.trim() || fileList.length > 0">
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
              <ul class="suggestions-list" v-if="aiAnalysisResult.study_suggestions && aiAnalysisResult.study_suggestions.length > 0">
                <li v-for="(suggestion, index) in aiAnalysisResult.study_suggestions" :key="index">
                  {{ suggestion }}
                </li>
              </ul>
              <p v-else>暂无学习建议。</p>
            </div>
          </el-tab-pane>

          <el-tab-pane label="其他信息" name="other_info">
            <div class="analysis-section">
              <h4>建议分类：</h4>
              <p>{{ aiAnalysisResult.suggested_category || '未提供' }}</p>
            </div>
            <div class="analysis-section">
              <h4>建议标签：</h4>
              <div class="knowledge-points" v-if="aiAnalysisResult.tags && aiAnalysisResult.tags.length > 0">
                <el-tag 
                  v-for="tag in aiAnalysisResult.tags" 
                  :key="tag"
                  type="success"
                  class="knowledge-tag"
                >
                  {{ tag }}
                </el-tag>
              </div>
              <p v-else>暂无建议标签。</p>
            </div>
            <div class="analysis-section">
              <h4>解题思路：</h4>
              <div class="error-analysis-content">
                {{ aiAnalysisResult.solution || '未提供' }}
              </div>
            </div>
            <div class="analysis-section">
              <h4>难度评估：</h4>
              <p>{{ aiAnalysisResult.difficulty_level ? `级别 ${aiAnalysisResult.difficulty_level}` : '未评估' }}</p>
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

    <!-- 分析方式选择对话框 -->
    <el-dialog
      v-model="analysisMethodVisible"
      title="选择分析方式"
      width="400px"
      :close-on-click-modal="false"
      :show-close="false"
    >
      <el-form label-width="100px">
        <el-form-item label="分析方式">
          <el-radio-group v-model="selectedAnalysisMethod">
            <el-radio label="multimodal_large">多模态大模型分析</el-radio>
            <el-radio label="multimodal_small_text">多模态小模型 + 纯文字模型</el-radio>
          </el-radio-group>
        </el-form-item>
      </el-form>
      <template #footer>
        <div class="dialog-footer">
          <el-button @click="analysisMethodVisible = false">取消</el-button>
          <el-button type="primary" @click="performAIAnalysis(selectedAnalysisMethod)">确定</el-button>
        </div>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { MagicStick, Plus } from '@element-plus/icons-vue'
import { problemAPI, aiAPI } from '@/utils/api'

const router = useRouter()
const formRef = ref()
const loading = ref(false)
const aiAnalysisLoading = ref(false)
const aiResultVisible = ref(false)
const activeTab = ref('knowledge')
const analysisMethodVisible = ref(false)
const selectedAnalysisMethod = ref('multimodal_large')

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
  study_suggestions: [],
  solution: '', 
  difficulty_level: null,
  tags: [], 
  suggested_category: ''
})

// 图片上传相关数据和方法
const fileList = ref([]);

const handleImageChange = (file, files) => {
  // 只保留最新上传的文件
  fileList.value = [file];
  // TODO: 将文件转换为Base64或准备上传
  console.log('文件已选择:', file);
};

const handleImageRemove = (file, files) => {
  fileList.value = [];
  console.log('文件已移除:', file);
};

// AI分析功能
const analyzeWithAI = async () => {
  if (!form.content.trim() && fileList.value.length === 0) {
    ElMessage.warning('请先输入题目内容或上传图片')
    return
  }

  if (fileList.value.length > 0) {
    // 如果有图片，显示分析方式选择对话框
    analysisMethodVisible.value = true;
  } else {
    // 如果没有图片，直接使用文本分析
    await performAIAnalysis('text_only');
  }
}

// 执行AI分析
const performAIAnalysis = async (method) => {
  analysisMethodVisible.value = false; // 关闭选择对话框
  aiAnalysisLoading.value = true;

  try {
    let imageData = null; 
    if (fileList.value.length > 0) {
      // 将 File 对象转换为 Base64 字符串
      imageData = await fileToBase64(fileList.value[0].raw); 
    }

    const requestData = {
      content: form.content,
      image_data: imageData, // 图片数据 (Base64)
      analysis_method: method, // 分析方法
      subject: form.subject, 
      category: form.category 
    };

    // 调用后端AI分析接口
    console.log('发送AI分析请求:', requestData);
    const response = await aiAPI.analyzeProblem(requestData); 
    
    if (response.data.success) {
      Object.assign(aiAnalysisResult, response.data.data);
      similarProblems.value = response.data.data.similar_problems || []; // 更新相似题目
      aiResultVisible.value = true;
    } else {
      ElMessage.error('AI分析失败: ' + response.data.message);
    }
  } catch (error) {
    ElMessage.error('AI分析请求异常: ' + error.message);
  } finally {
    aiAnalysisLoading.value = false;
  }
};

// 应用AI分析结果
const applyAIResult = () => {
  // 应用知识点
  if (aiAnalysisResult.knowledge_points && aiAnalysisResult.knowledge_points.length > 0) {
    form.knowledge_points = [...new Set([...form.knowledge_points, ...aiAnalysisResult.knowledge_points])];
  }
  
  // 应用错误分析
  if (aiAnalysisResult.error_analysis) {
    form.error_analysis = aiAnalysisResult.error_analysis;
  }

  // 应用建议分类
  if (aiAnalysisResult.suggested_category) {
    form.category = aiAnalysisResult.suggested_category;
    ElMessage.info(`AI建议分类 "${aiAnalysisResult.suggested_category}" 已填充`);
  }

  // 应用建议标签
  if (aiAnalysisResult.tags && aiAnalysisResult.tags.length > 0) {
    form.tags = [...new Set([...form.tags, ...aiAnalysisResult.tags])];
  }
  
  // 应用解题思路 (如果表单中有对应字段)
  // form.solution = aiAnalysisResult.solution || form.solution; 
  // 假设表单中没有 solution 字段，错误分析中可能包含

  // 应用难度等级 (如果表单中有对应字段)
  // form.difficulty_level = aiAnalysisResult.difficulty_level || form.difficulty_level;
  // 假设表单中没有 difficulty_level 字段

  // 旧的自动生成标签逻辑可以保留或与AI建议的标签合并
  const autoGeneratedTags = [];
  if (form.knowledge_points.length > 3 && !form.tags.includes('复杂')) {
    autoGeneratedTags.push('复杂');
  }
  if (form.error_analysis && form.error_analysis.includes('概念') && !form.tags.includes('概念题')) {
    autoGeneratedTags.push('概念题');
  }
  if (form.error_analysis && form.error_analysis.includes('计算') && !form.tags.includes('计算题')) {
    autoGeneratedTags.push('计算题');
  }
  if (autoGeneratedTags.length > 0) {
      form.tags = [...new Set([...form.tags, ...autoGeneratedTags])];
  }
  
  aiResultVisible.value = false;
  ElMessage.success('AI分析结果已应用到表单');
}

// 查看相似题目
const viewSimilarProblem = (problem) => {
  // TODO: 实现查看相似题目的逻辑，例如跳转到详情页或弹窗显示
  console.log('查看相似题目:', problem);
  ElMessageBox.alert(`相似题目内容：${problem.content}`, problem.title, { 
    confirmButtonText: '确定' 
  });
};

// 将 File 对象转换为 Base64 字符串 (需要异步)
const fileToBase64 = (file) => {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.readAsDataURL(file);
    reader.onload = () => resolve(reader.result.split(',')[1]); // 移除 data:image/...;base64,
    reader.onerror = error => reject(error);
  });
};

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
  margin-top: 15px;
  display: flex;
  align-items: center;
}

.ai-analysis-btn {
  margin-right: 10px;
}

.ai-tip {
  font-size: 12px;
  color: #909399;
}

.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 5px;
}

.similar-problems {
  width: 100%;
}

.similar-problem-item {
  margin-bottom: 15px;
}

.similar-card {
  width: 100%;
}

.similar-content {
  margin-bottom: 10px;
}

.similar-preview {
  font-size: 14px;
  color: #606266;
  margin-bottom: 10px;
}

.similar-meta .el-tag {
  margin-right: 5px;
}

.similar-actions {
  text-align: right;
}

.analysis-section h4 {
  margin-top: 0;
}

.knowledge-points .el-tag {
  margin-right: 5px;
  margin-bottom: 5px;
}

.error-analysis-content,
.suggestions-list {
  white-space: pre-wrap; /* 保留换行符 */
}

.suggestions-list {
  padding-left: 20px;
}

.dialog-footer {
  text-align: right;
}

/* 图片上传组件样式 */
.problem-image-upload ::v-deep .el-upload--picture-card {
  width: 100px;
  height: 100px;
  line-height: 100px;
}

.problem-image-upload ::v-deep .el-upload-list--picture-card .el-upload-list__item {
  width: 100px;
  height: 100px;
}
</style>
