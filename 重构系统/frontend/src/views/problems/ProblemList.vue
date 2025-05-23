<template>
  <div class="problem-list">
    <div class="page-header">
      <h1>错题管理</h1>
      <el-button type="primary" @click="$router.push('/problems/create')">
        <el-icon><Plus /></el-icon>
        添加错题
      </el-button>
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

    <!-- 错题列表 -->
    <el-card>
      <el-table
        v-loading="loading"
        :data="problems"
        style="width: 100%"
      >
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
        <el-table-column prop="created_at" label="创建时间" width="180">
          <template #default="{ row }">
            {{ formatDate(row.created_at) }}
          </template>
        </el-table-column>
        <el-table-column label="操作" width="200" fixed="right">
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
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { problemAPI } from '@/utils/api'
import dayjs from 'dayjs'

const router = useRouter()

const loading = ref(false)
const problems = ref([])

const filters = reactive({
  subject: '',
  category: ''
})

const pagination = reactive({
  current: 1,
  pageSize: 20,
  total: 0
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
    problems.value = data
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

.filter-card {
  margin-bottom: 20px;
}
</style> 