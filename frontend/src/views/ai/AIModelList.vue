const toggleModelStatus = async (model) => {
  model.statusLoading = true
  try {
    // 补全所有必需字段
    const updateData = {
      provider: model.provider,
      modelName: model.model_name,
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
    await aiAPI.updateModel(model.id, updateData)
    ElMessage.success(`模型已${model.is_active ? '启用' : '禁用'}`)
  } catch (error) {
    // 回滚状态
    model.is_active = !model.is_active
    ElMessage.error('状态切换失败')
  } finally {
    model.statusLoading = false
  }
}

// 表单验证规则
const rules = {
  provider: [
    { required: true, message: '请选择提供商', trigger: 'change' }
  ],
  model_name: [
    { required: true, message: '请输入模型名称', trigger: 'blur' }
  ],
  api_key: [
    { required: true, message: '请输入API密钥', trigger: 'blur' }
  ],
  api_url: [
    { required: true, message: '请输入API地址', trigger: 'blur' }
  ],
  capabilities: [
    { required: true, message: '请选择至少一个能力', trigger: 'change' }
  ],
  priority: [
    { required: true, type: 'number', message: '请输入优先级', trigger: 'blur' }
  ],
  timeout: [
    { required: true, type: 'number', message: '请输入超时时间', trigger: 'blur' }
  ],
  max_retries: [
    { required: true, type: 'number', message: '请输入最大重试次数', trigger: 'blur' }
  ]
}

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
      is_active: modelData.is_active,
      cost_per_1k_tokens: modelData.cost_per_1k_tokens ?? 0,
      max_tokens: modelData.max_tokens ?? 4096,
      custom_headers: modelData.custom_headers ?? null
    })
    dialogVisible.value = true
  } catch (error) {
    ElMessage.error('获取模型详情失败')
  }
}

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
    is_active: true,
    cost_per_1k_tokens: 0,
    max_tokens: 4096,
    custom_headers: null
  })
  formRef.value?.clearValidate()
}

const submitForm = async () => {
  if (!formRef.value) return
  const valid = await formRef.value.validate().catch(() => false)
  if (!valid) return
  submitLoading.value = true
  try {
    const updateData = {
      provider: form.provider,
      modelName: form.model_name,
      api_key: form.api_key,
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
      await aiAPI.updateModel(form.id, updateData)
      ElMessage.success('模型更新成功')
    } else {
      await aiAPI.createModel(updateData)
      ElMessage.success('模型创建成功')
    }
    dialogVisible.value = false
    resetForm()
    loadModels()
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