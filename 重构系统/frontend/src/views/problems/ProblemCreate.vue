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

        <el-form-item label="错误分析">
          <el-input
            v-model="form.error_analysis"
            type="textarea"
            :rows="4"
            placeholder="分析错误原因"
          />
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
  </div>
</template>

<script setup>
import { ref, reactive } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage } from 'element-plus'
import { problemAPI } from '@/utils/api'

const router = useRouter()
const formRef = ref()
const loading = ref(false)

const form = reactive({
  title: '',
  subject: '',
  category: '',
  content: '',
  user_answer: '',
  correct_answer: '',
  error_analysis: '',
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
</style> 