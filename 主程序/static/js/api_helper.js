/**
 * API响应处理助手函数
 */

/**
 * 安全地处理标签数据，确保返回数组
 * 
 * @param {any} tagsData - 从API接收的tags数据
 * @return {Array} - 处理后的标签数组
 */
function processTags(tagsData) {
    // 如果为空，返回空数组
    if (!tagsData) return [];
    
    // 如果已经是数组，直接返回
    if (Array.isArray(tagsData)) return tagsData;
    
    // 如果是字符串，尝试解析为JSON
    if (typeof tagsData === 'string') {
        try {
            const parsedTags = JSON.parse(tagsData);
            // 如果解析后是数组，返回该数组
            if (Array.isArray(parsedTags)) {
                return parsedTags;
            }
            // 如果解析后不是数组，将其作为单个标签返回
            return [tagsData];
        } catch (e) {
            // 解析失败，将原字符串作为单个标签
            return [tagsData];
        }
    }
    
    // 如果是其他类型，转为字符串作为单个标签
    return [String(tagsData)];
}

/**
 * 格式化日期时间
 * 
 * @param {string|Date} dateTime - 日期时间字符串或Date对象
 * @param {boolean} includeTime - 是否包含时间部分
 * @return {string} - 格式化后的日期时间字符串
 */
function formatDateTime(dateTime, includeTime = true) {
    if (!dateTime) return '未知时间';
    
    try {
        const date = new Date(dateTime);
        if (isNaN(date.getTime())) return dateTime;
        
        if (includeTime) {
            return date.toLocaleString();
        } else {
            return date.toLocaleDateString();
        }
    } catch (e) {
        console.error('日期格式化错误:', e);
        return dateTime;
    }
}

/**
 * 安全地处理API响应中的文本内容
 * 
 * @param {string} text - 文本内容
 * @param {number} maxLength - 截断长度
 * @param {boolean} usePlaceholder - 如果为空是否使用占位符
 * @return {string} - 处理后的文本
 */
function safeText(text, maxLength = -1, usePlaceholder = true) {
    if (!text && usePlaceholder) return '无内容';
    if (!text) return '';
    
    if (maxLength > 0 && text.length > maxLength) {
        return text.substring(0, maxLength) + '...';
    }
    
    return text;
}

/**
 * API助手工具库 - 提供常用的API操作函数
 */

/**
 * 获取当前系统默认模型配置
 * @returns {Promise} 包含默认模型配置的Promise
 */
function getDefaultModels() {
    return fetch('/api/settings', {
        method: 'GET'
    })
    .then(response => {
        if (!response.ok) {
            throw new Error('获取API配置失败');
        }
        return response.json();
    })
    .then(data => {
        return data.default_models;
    });
}

/**
 * 检查API状态
 * @returns {Promise} 包含API状态的Promise
 */
function checkApiStatus() {
    return fetch('/api/settings', {
        method: 'GET'
    })
    .then(response => {
        if (!response.ok) {
            throw new Error('获取API状态失败');
        }
        return response.json();
    })
    .then(data => {
        return data.statuses;
    });
}

/**
 * 获取与模型类型匹配的API状态
 * @param {string} modelType - 模型类型，例如 'openai', 'deepseek', 'gemini' 等
 * @returns {Promise} API状态信息
 */
function getModelApiStatus(modelType) {
    return checkApiStatus()
    .then(statuses => {
        // 根据模型类型映射到API类型
        let apiType = modelType;
        if (modelType.startsWith('multimodal_')) {
            apiType = modelType.replace('multimodal_', '');
        }
        
        // 针对特殊情况处理
        if (apiType === 'qwen-vl') {
            return statuses['openai'] || { success: false, message: '未配置API' };
        }
        
        return statuses[apiType] || { success: false, message: '未配置API' };
    });
}

// 导出API助手
window.ApiHelper = {
    getDefaultModels,
    checkApiStatus,
    getModelApiStatus
};
