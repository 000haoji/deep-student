/**
 * API设置实用程序函数
 */

// 显示配置表单 - 强化版本确保表单可见
function showConfigForms() {
    console.log('尝试显示配置表单...');
    const configForms = document.getElementById('api-config-forms');
    if (configForms) {
        console.log('找到配置表单元素，准备显示');
        // 移除所有可能导致不可见的类
        configForms.classList.remove('d-none', 'invisible', 'collapse');
        
        // 添加可见性样式
        configForms.style.display = 'block';
        configForms.style.visibility = 'visible';
        configForms.style.opacity = '1';
    } else {
        console.error('找不到配置表单区域元素！检查HTML中是否有id为api-config-forms的元素');
    }
}

// 填充表单字段 - 添加默认值和错误处理
function populateFormFields(configs) {
    // 确保configs是一个对象
    configs = configs || {};
    
    // 填充API端点表单，使用默认值
    const elements = {
        'openai-api-url': configs['openai_api_url'] || '',
        'deepseek-api-url': configs['deepseek_api_url'] || '',
        'openai-api-key': configs['openai_api_key'] || '',
        'deepseek-api-key': configs['deepseek_api_key'] || ''
    };
    
    // 设置表单值
    Object.entries(elements).forEach(([id, value]) => {
        const element = document.getElementById(id);
        if (element) {
            element.value = value;
        }
    });
    
    // 填充模型默认设置，使用默认值
    const extractionModelSelect = document.getElementById('default-extraction-model');
    if (extractionModelSelect) {
        extractionModelSelect.value = configs['default_extraction_model'] || 'multimodal_qwen-vl';
    }
    
    const analysisModelSelect = document.getElementById('default-analysis-model');
    if (analysisModelSelect) {
        analysisModelSelect.value = configs['default_analysis_model'] || 'openai';
    }
    
    return true; // 返回填充状态
}

// 修改showToast函数，增强兼容性
function showToast(message, type = 'info') {
    try {
        // 检查是否已有toast容器
        let toastContainer = document.getElementById('toastContainer');
        if (!toastContainer) {
            // 创建toast容器
            toastContainer = document.createElement('div');
            toastContainer.id = 'toastContainer';
            toastContainer.className = 'toast-container position-fixed bottom-0 end-0 p-3';
            document.body.appendChild(toastContainer);
        }
        
        // 创建简单的内联toast，不依赖Bootstrap
        const toast = document.createElement('div');
        toast.className = `alert alert-${type} alert-dismissible fade show`;
        toast.setAttribute('role', 'alert');
        
        toast.innerHTML = `
            ${message}
            <button type="button" class="btn-close" data-bs-dismiss="alert" aria-label="Close"></button>
        `;
        
        toastContainer.appendChild(toast);
        
        // 如果Bootstrap可用，使用它；否则，使用简单的timeout
        if (typeof bootstrap !== 'undefined' && bootstrap.Toast) {
            const bsToast = new bootstrap.Toast(toast, { delay: 3000 });
            bsToast.show();
        } else {
            // 手动实现简单的淡出效果
            setTimeout(() => {
                toast.classList.remove('show');
                setTimeout(() => toast.remove(), 150);
            }, 3000);
        }
    } catch (error) {
        console.error('显示Toast通知失败:', error);
    }
}

// 切换密码显示/隐藏
function togglePasswordVisibility(e) {
    const button = e.currentTarget;
    const input = button.previousElementSibling;
    const icon = button.querySelector('i');
    
    if (input.type === 'password') {
        input.type = 'text';
        icon.className = 'bi bi-eye-slash';
    } else {
        input.type = 'password';
        icon.className = 'bi bi-eye';
    }
}
