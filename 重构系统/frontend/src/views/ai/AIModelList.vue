<template>
  <div class="ai-model-management">
    <el-card>
      <template #header>
        <div class="card-header">
          <span class="title">AI模型管理</span>
          <el-button type="primary" @click="showAddDialog">
            <el-icon><Plus /></el-icon>
            添加模型
          </el-button>
        </div>
      </template>

      <!-- 筛选器 -->
      <div class="filters">
        <el-form :model="filters" inline>
          <el-form-item label="提供商:">
            <el-select v-model="filters.provider" placeholder="选择提供商" clearable>
              <el-option label="OpenAI" value="openai" />
              <el-option label="Gemini" value="gemini" />
              <el-option label="DeepSeek" value="deepseek" />
              <el-option label="Claude" value="claude" />
              <el-option label="Qwen" value="qwen" />
            </el-select>
          </el-form-item>
          <el-form-item label="状态:">
            <el-select v-model="filters.is_active" placeholder="选择状态" clearable>
              <el-option label="启用" :value="true" />
              <el-option label="禁用" :value="false" />
            </el-select>
          </el-form-item>
          <el-form-item>
            <el-button type="primary" @click="loadModels">查询</el-button>
            <el-button @click="resetFilters">重置</el-button>
          </el-form-item>
        </el-form>
      </div>

      <!-- 模型列表 -->
      <el-table 
        :data="models" 
        v-loading="loading"
        style="width: 100%"
        row-key="id"
      >
        <el-table-column prop="provider" label="提供商" width="120">
          <template #default="{ row }">
            <el-tag :type="getProviderTagType(row.provider)">
              {{ getProviderName(row.provider) }}
            </el-tag>
          </template>
        </el-table-column>
        
        <el-table-column prop="model_name" label="模型名称" width="200" />
        
        <el-table-column label="能力" width="200">
          <template #default="{ row }">
            <el-tag 
              v-for="capability in row.capabilities" 
              :key="capability"
              size="small"
              style="margin-right: 5px; margin-bottom: 2px;"
            >
              {{ getCapabilityName(capability) }}
            </el-tag>
          </template>
        </el-table-column>
        
        <el-table-column prop="priority" label="优先级" width="80" align="center" />
        
        <el-table-column label="成功率" width="100" align="center">
          <template #default="{ row }">
            <span :class="getSuccessRateClass(row.success_rate)">
              {{ (row.success_rate * 100).toFixed(1) }}%
            </span>
          </template>
        </el-table-column>
        
        <el-table-column label="状态" width="100" align="center">
          <template #default="{ row }">
            <el-switch
              v-model="row.is_active"
              @change="toggleModelStatus(row)"
              :loading="row.statusLoading"
            />
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
        
        <el-table-column label="操作" width="200" fixed="right">
          <template #default="{ row }">
            <el-button 
              type="primary" 
              size="small" 
              @click="testModel(row)"
              :loading="row.testLoading"
            >
              测试
            </el-button>
            <el-button 
              type="warning" 
              size="small" 
              @click="editModel(row)"
            >
              编辑
            </el-button>
            <el-button 
              type="danger" 
              size="small" 
              @click="deleteModel(row)"
            >
              删除
            </el-button>
          </template>
        </el-table-column>
      </el-table>
    </el-card>

    <!-- 添加/编辑模型对话框 -->
    <el-dialog 
      v-model="dialogVisible" 
      :title="isEdit ? '编辑模型' : '添加模型'" 
      width="600px"
    >
      <el-form 
        :model="form" 
        :rules="rules" 
        ref="formRef" 
        label-width="120px"
      >
        <el-form-item label="提供商" prop="provider">
          <el-select v-model="form.provider" placeholder="选择提供商">
            <el-option label="OpenAI" value="openai" />
            <el-option label="Gemini" value="gemini" />
            <el-option label="DeepSeek" value="deepseek" />
            <el-option label="Claude" value="claude" />
            <el-option label="Qwen" value="qwen" />
          </el-select>
        </el-form-item>
        
        <el-form-item label="模型名称" prop="model_name">
          <el-input 
            v-model="form.model_name" 
            placeholder="如: gpt-4, gpt-3.5-turbo"
          />
        </el-form-item>
        
        <el-form-item label="API密钥" prop="api_key">
          <el-input 
            v-model="form.api_key" 
            type="password" 
            placeholder="输入API密钥"
            show-password
          />
        </el-form-item>
        
        <el-form-item label="API地址" prop="api_url">
          <el-input 
            v-model="form.api_url" 
            placeholder="API基础地址（可选）"
          />
        </el-form-item>
        
        <el-form-item label="模型能力" prop="capabilities">
          <el-checkbox-group v-model="form.capabilities">
            <el-checkbox value="text">文本处理</el-checkbox>
            <el-checkbox value="vision">图像识别</el-checkbox>
            <el-checkbox value="code">代码生成</el-checkbox>
            <el-checkbox value="function_calling">函数调用</el-checkbox>
          </el-checkbox-group>
        </el-form-item>
        
        <el-form-item label="优先级" prop="priority">
          <el-input-number 
            v-model="form.priority" 
            :min="1" 
            :max="10" 
            controls-position="right"
          />
          <div class="form-tip">数值越大优先级越高</div>
        </el-form-item>
        
        <el-form-item label="超时时间" prop="timeout">
          <el-input-number 
            v-model="form.timeout" 
            :min="5" 
            :max="300" 
            controls-position="right"
          />
          <span class="form-unit">秒</span>
        </el-form-item>
        
        <el-form-item label="最大重试" prop="max_retries">
          <el-input-number 
            v-model="form.max_retries" 
            :min="0" 
            :max="5" 
            controls-position="right"
          />
          <span class="form-unit">次</span>
        </el-form-item>
        
        <el-form-item label="启用状态">
          <el-switch v-model="form.is_active" />
        </el-form-item>
      </el-form>
      
      <template #footer>
        <el-button @click="dialogVisible = false">取消</el-button>
        <el-button type="primary" @click="submitForm" :loading="submitLoading">
          {{ isEdit ? '保存' : '创建' }}
        </el-button>
      </template>
    </el-dialog>
  </div>
</template>

<script setup>
import { ref, reactive, onMounted } from 'vue'
import { ElMessage, ElMessageBox } from 'element-plus'
import { Plus } from '@element-plus/icons-vue'
import { aiAPI } from '@/utils/api'

// 响应式数据
const loading = ref(false)
const models = ref([])
const dialogVisible = ref(false)
const isEdit = ref(false)
const submitLoading = ref(false)
const formRef = ref()

// 筛选器
const filters = reactive({
  provider: '',
  is_active: null
})

// 表单数据
const form = reactive({
  provider: '',
  model_name: '',
  api_key: '',
  api_url: '',
  capabilities: [],
  priority: 1,
  timeout: 30,
  max_retries: 3,
  is_active: true
})

// 自定义 API Key 验证器
const validateApiKey = (rule, value, callback) => {
  if (!isEdit.value && !value) { // 如果是创建模式且 API Key 为空
    callback(new Error('请输入API密钥'));
  } else {
    callback(); // 编辑模式下，空值代表不修改，或创建模式下有值
  }
};

// 表单验证规则
const rules = {
  provider: [
    { required: true, message: '请选择提供商', trigger: 'change' }
  ],
  model_name: [
    { required: true, message: '请输入模型名称', trigger: 'blur' }
  ],
  api_key: [
    { validator: validateApiKey, trigger: 'blur' }
  ],
  capabilities: [
    { required: true, message: '请选择至少一个能力', trigger: 'change' }
  ]
}

// 获取提供商标签类型
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

// 获取提供商名称
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

// 获取能力名称
const getCapabilityName = (capability) => {
  const nameMap = {
    'text': '文本',
    'vision': '视觉',
    'code': '代码',
    'function_calling': '函数调用'
  }
  return nameMap[capability] || capability
}

// 获取成功率样式类
const getSuccessRateClass = (rate) => {
  if (rate >= 0.9) return 'success-rate high'
  if (rate >= 0.7) return 'success-rate medium'
  return 'success-rate low'
}

// 加载模型列表
const loadModels = async () => {
  loading.value = true
  try {
    const response = await aiAPI.getModels(filters)
    models.value = response.data.data.map(model => ({
      ...model,
      statusLoading: false,
      testLoading: false,
      health_status: 'unknown'
    }))
    
    // 异步检查健康状态
    checkModelsHealth()
  } catch (error) {
    ElMessage.error('加载模型列表失败')
  } finally {
    loading.value = false
  }
}

// 检查所有模型健康状态
const checkModelsHealth = async () => {
  try {
    const response = await aiAPI.checkHealth()
    const healthData = response.data.data
    
    models.value.forEach(model => {
      const health = healthData.find(h => h.model_id === model.id)
      if (health) {
        model.health_status = health.status
      }
    })
  } catch (error) {
    console.warn('检查健康状态失败:', error)
  }
}

// 重置筛选器
const resetFilters = () => {
  filters.provider = ''
  filters.is_active = null
  loadModels()
}

// 显示添加对话框
const showAddDialog = () => {
  isEdit.value = false
  resetForm()
  dialogVisible.value = true
}

// 编辑模型
const editModel = async (model) => {
  try {
    const response = await aiAPI.getModelDetail(model.id)
    const modelData = response.data.data
    
    isEdit.value = true
    Object.assign(form, {
      id: modelData.id,
      provider: modelData.provider,
      model_name: modelData.model_name,
      api_key: '', // 不显示已存储的密钥
      api_url: modelData.api_url || '',
      capabilities: modelData.capabilities || [],
      priority: modelData.priority,
      timeout: modelData.timeout,
      max_retries: modelData.max_retries,
      is_active: modelData.is_active
    })
    
    dialogVisible.value = true
  } catch (error) {
    ElMessage.error('获取模型详情失败')
  }
}

// 重置表单
const resetForm = () => {
  Object.assign(form, {
    provider: '',
    model_name: '',
    api_key: '',
    api_url: '',
    capabilities: [],
    priority: 1,
    timeout: 30,
    max_retries: 3,
    is_active: true
  })
  formRef.value?.clearValidate()
}

// 提交表单
const submitForm = async () => {
  if (!formRef.value) return
  const valid = await formRef.value.validate().catch(() => false)
  if (!valid) return
  submitLoading.value = true
  try {
    // 构建更新数据，确保字段名称与后端一致
    const updateData = {
      provider: form.provider,
      model_name: form.model_name,  // 使用 model_name 而不是 modelName
      api_key: form.api_key || undefined,  // 如果为空则设为 undefined
      api_url: form.api_url,
      capabilities: form.capabilities,
      priority: form.priority,
      timeout: form.timeout,
      max_retries: form.max_retries,
      is_active: form.is_active,
      cost_per_1k_tokens: form.cost_per_1k_tokens ?? 0,
      max_tokens: form.max_tokens ?? 4096,
      custom_headers: form.custom_headers ?? null
    }

    if (isEdit.value) {
      console.log('提交更新的模型数据:', updateData)  // 添加日志
      await aiAPI.updateModel(form.id, updateData)
      ElMessage.success('模型更新成功')
    } else {
      await aiAPI.createModel(updateData)
      ElMessage.success('模型创建成功')
    }
    dialogVisible.value = false
    resetForm()
    await loadModels()  // 重新加载模型列表
  } catch (error) {
    let msg = isEdit.value ? '模型更新失败' : '模型创建失败'
    if (error?.response?.data?.detail) {
      msg += ': ' + error.response.data.detail
    } else if (error?.message) {
      msg += ': ' + error.message
    }
    console.error('模型保存失败', error, error?.response?.data)
    ElMessage.error(msg)
  } finally {
    submitLoading.value = false
  }
}

// 切换模型状态
const toggleModelStatus = async (model) => {
  model.statusLoading = true
  try {
    // 补全所有必需字段
    const updateData = {
      provider: model.provider,
      model_name: model.model_name,  // 修改为 model_name
      api_key: '', // 不更新密钥
      api_url: model.api_url,
      capabilities: model.capabilities,
      priority: model.priority,
      timeout: model.timeout,
      max_retries: model.max_retries,
      is_active: model.is_active,
      cost_per_1k_tokens: model.cost_per_1k_tokens ?? 0,
      max_tokens: model.max_tokens ?? 4096,
      custom_headers: model.custom_headers ?? null
    }
    console.log('切换模型状态:', updateData)  // 添加日志
    await aiAPI.updateModel(model.id, updateData)
    ElMessage.success(`模型已${model.is_active ? '启用' : '禁用'}`)
  } catch (error) {
    // 回滚状态
    model.is_active = !model.is_active
    let msg = '状态切换失败'
    if (error?.response?.data?.detail) {
      msg += ': ' + error.response.data.detail
    } else if (error?.message) {
      msg += ': ' + error.message
    }
    console.error('状态切换失败', error, error?.response?.data)
    ElMessage.error(msg)
  } finally {
    model.statusLoading = false
  }
}

// 测试模型
const testModel = async (model) => {
  model.testLoading = true
  try {
    const response = await aiAPI.testModel(model.id)
    if (response.data.success) {
      ElMessage.success(`测试成功，响应时间: ${response.data.data.response_time}ms`)
      model.health_status = 'healthy'
    } else {
      ElMessage.error(`测试失败: ${response.data.message}`)
      model.health_status = 'unhealthy'
    }
  } catch (error) {
    ElMessage.error('测试连接失败')
    model.health_status = 'unhealthy'
  } finally {
    model.testLoading = false
  }
}

// 删除模型
const deleteModel = async (model) => {
  try {
    await ElMessageBox.confirm(
      `确定要删除模型 "${model.model_name}" 吗？`,
      '确认删除',
      {
        confirmButtonText: '确定',
        cancelButtonText: '取消',
        type: 'warning',
      }
    )
    
    await aiAPI.deleteModel(model.id)
    ElMessage.success('模型删除成功')
    loadModels()
  } catch (error) {
    if (error !== 'cancel') {
      ElMessage.error('模型删除失败')
    }
  }
}

// 生命周期
onMounted(() => {
  loadModels()
})
</script>

<style scoped>
.ai-model-management {
  padding: 20px;
}

.card-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.title {
  font-size: 18px;
  font-weight: bold;
}

.filters {
  margin-bottom: 20px;
  padding: 20px;
  background: #f5f7fa;
  border-radius: 8px;
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

.form-tip {
  font-size: 12px;
  color: #909399;
  margin-top: 5px;
}

.form-unit {
  margin-left: 10px;
  color: #909399;
}
</style> 