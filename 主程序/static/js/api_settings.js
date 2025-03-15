// 页面加载完成后执行
$(document).ready(function() {
    // 加载当前API配置
    loadAPISettings();
    
    // 绑定表单提交事件
    $('#endpoints-form').on('submit', function(e) {
        e.preventDefault();
        saveEndpoints();
    });
    
    $('#models-form').on('submit', function(e) {
        e.preventDefault();
        saveDefaultModels();
    });
    
    // 绑定API测试按钮
    $('.test-api').on('click', function() {
        const apiType = $(this).data('api-type');
        testAPIConnection(apiType);
    });
    
    // 绑定测试所有API按钮
    $('#test-all-endpoints').on('click', function() {
        testAllAPIConnections();
    });
});

// 加载API配置
function loadAPISettings() {
    $.ajax({
        url: '/api/settings',
        type: 'GET',
        success: function(data) {
            // 填充API端点表单
            if (data.endpoints) {
                // OpenAI API
                if (data.endpoints.openai) {
                    $('#openai-api-url').val(data.endpoints.openai.url || '');
                    $('#openai-api-key').val(data.endpoints.openai.key || '');
                    $('#openai-model').val(data.endpoints.openai.model || 'gpt-3.5-turbo');
                }
                
                // DeepSeek API
                if (data.endpoints.deepseek) {
                    $('#deepseek-api-url').val(data.endpoints.deepseek.url || '');
                    $('#deepseek-api-key').val(data.endpoints.deepseek.key || '');
                    $('#deepseek-model').val(data.endpoints.deepseek.model || 'deepseek-chat');
                }
                
                // 千问VL多模态API
                if (data.endpoints['qwen-vl']) {
                    $('#qwen-vl-api-url').val(data.endpoints['qwen-vl'].url || '');
                    $('#qwen-vl-api-key').val(data.endpoints['qwen-vl'].key || '');
                    $('#qwen-vl-model').val(data.endpoints['qwen-vl'].model || 'qwen-vl-plus');
                }
                
                // Google Gemini API
                if (data.endpoints.gemini) {
                    $('#gemini-api-url').val(data.endpoints.gemini.url || '');
                    $('#gemini-api-key').val(data.endpoints.gemini.key || '');
                    $('#gemini-model').val(data.endpoints.gemini.model || 'gemini-pro');
                }
            }
            
            // 设置默认模型选项
            if (data.default_models) {
                $('#default-extraction-model').val(data.default_models.extraction || 'openai');
                $('#default-analysis-model').val(data.default_models.analysis || 'deepseek');
            }
            
            // 加载API状态卡片
            if (data.api_status) {
                // 将状态转换为加载卡片所需格式
                const statuses = {};
                
                // OpenAI状态
                if (data.api_status.openai !== undefined) {
                    statuses.openai = {
                        success: data.api_status.openai,
                        url: data.endpoints?.openai?.url || '',
                        response_time: 0, // 默认值
                        message: data.api_status.openai ? '连接正常' : '未测试或连接失败'
                    };
                }
                
                // DeepSeek状态
                if (data.api_status.deepseek !== undefined) {
                    statuses.deepseek = {
                        success: data.api_status.deepseek,
                        url: data.endpoints?.deepseek?.url || '',
                        response_time: 0, // 默认值
                        message: data.api_status.deepseek ? '连接正常' : '未测试或连接失败'
                    };
                }
                
                // 千问VL状态
                if (data.api_status['qwen-vl'] !== undefined) {
                    statuses['qwen-vl'] = {
                        success: data.api_status['qwen-vl'],
                        url: data.endpoints?.['qwen-vl']?.url || '',
                        response_time: 0, // 默认值
                        message: data.api_status['qwen-vl'] ? '连接正常' : '未测试或连接失败'
                    };
                }
                
                // Gemini状态
                if (data.api_status.gemini !== undefined) {
                    statuses.gemini = {
                        success: data.api_status.gemini,
                        url: data.endpoints?.gemini?.url || '',
                        response_time: 0, // 默认值
                        message: data.api_status.gemini ? '连接正常' : '未测试或连接失败'
                    };
                }
                
                loadAPIStatusCards(statuses);
            }
        },
        error: function() {
            showToast('加载API配置失败', 'danger');
        }
    });
}

// 保存API端点配置
function saveEndpoints() {
    // 创建与后端期望一致的数据结构
    const endpoints = {
        openai: {
            url: $('#openai-api-url').val().trim(),
            key: $('#openai-api-key').val().trim(),
            model: $('#openai-model').val().trim()
        },
        deepseek: {
            url: $('#deepseek-api-url').val().trim(),
            key: $('#deepseek-api-key').val().trim(),
            model: $('#deepseek-model').val().trim()
        },
        'qwen-vl': {
            url: $('#qwen-vl-api-url').val().trim(),
            key: $('#qwen-vl-api-key').val().trim(),
            model: $('#qwen-vl-model').val().trim()
        },
        gemini: {
            url: $('#gemini-api-url').val().trim(),
            key: $('#gemini-api-key').val().trim(),
            model: $('#gemini-model').val().trim()
        }
    };
    
    $.ajax({
        url: '/api/settings/endpoints',
        type: 'POST',
        contentType: 'application/json',
        data: JSON.stringify(endpoints),
        success: function(data) {
            showToast('API配置已成功保存', 'success');
            
            // 重新加载API状态
            loadAPISettings();
        },
        error: function(xhr) {
            let message = '保存API配置失败';
            if (xhr.responseJSON && xhr.responseJSON.message) {
                message += ': ' + xhr.responseJSON.message;
            }
            showToast(message, 'danger');
        }
    });
}

// 测试API连接
function testAPIConnection(apiType) {
    // 显示测试中提示
    $('#connection-alert')
        .removeClass('d-none alert-success alert-danger')
        .addClass('alert-info')
        .text(`正在测试 ${getAPIName(apiType)} API 连接，请稍候...`);
    
    // 获取对应API的URL和密钥
    let apiUrl = '';
    let apiKey = '';
    let apiModel = '';
    
    switch(apiType) {
        case 'openai':
            apiUrl = $('#openai-api-url').val().trim();
            apiKey = $('#openai-api-key').val().trim();
            apiModel = $('#openai-model').val().trim() || 'gpt-3.5-turbo';
            break;
        case 'deepseek':
            apiUrl = $('#deepseek-api-url').val().trim();
            apiKey = $('#deepseek-api-key').val().trim();
            apiModel = $('#deepseek-model').val().trim() || 'deepseek-chat';
            break;
        case 'qwen-vl':
            apiUrl = $('#qwen-vl-api-url').val().trim();
            apiKey = $('#qwen-vl-api-key').val().trim();
            apiModel = $('#qwen-vl-model').val().trim() || 'qwen-vl-plus';
            break;
        case 'gemini':
            apiUrl = $('#gemini-api-url').val().trim();
            apiKey = $('#gemini-api-key').val().trim();
            apiModel = $('#gemini-model').val().trim() || 'gemini-pro';
            break;
    }
    
    // 发送测试请求
    $.ajax({
        url: '/api/settings/test-connection',
        type: 'POST',
        contentType: 'application/json',
        data: JSON.stringify({
            api_type: apiType,
            api_url: apiUrl,
            api_key: apiKey,
            api_model: apiModel
        }),
        success: function(data) {
            $('#connection-alert')
                .removeClass('alert-info alert-danger')
                .addClass('alert-success')
                .html(`<strong>${getAPIName(apiType)} API连接成功!</strong> 响应时间: ${data.response_time.toFixed(2)}秒`);
                
            // 3秒后自动隐藏提示
            setTimeout(() => {
                $('#connection-alert').addClass('d-none');
            }, 3000);
        },
        error: function(xhr) {
            let message = `测试 ${getAPIName(apiType)} API 连接失败`;
            if (xhr.responseJSON && xhr.responseJSON.message) {
                message += ': ' + xhr.responseJSON.message;
            }
            
            $('#connection-alert')
                .removeClass('alert-info alert-success')
                .addClass('alert-danger')
                .text(message);
        }
    });
}

// 测试所有API连接
function testAllAPIConnections() {
    // 显示测试中提示
    $('#connection-alert')
        .removeClass('d-none alert-success alert-danger')
        .addClass('alert-info')
        .text('正在测试所有API连接，请稍候...');
    
    $.ajax({
        url: '/api/settings/test-all-connections',
        type: 'POST',
        success: function(data) {
            // 创建结果消息
            let message = '<strong>API连接测试结果:</strong><ul>';
            Object.keys(data.results).forEach(apiType => {
                const result = data.results[apiType];
                const statusClass = result.success ? 'text-success' : 'text-danger';
                const statusIcon = result.success ? '✓' : '✗';
                const statusText = result.success ? '成功' : '失败';
                
                message += `<li><span class="${statusClass}">${statusIcon} ${apiType}: ${statusText}</span>`;
                if (result.success) {
                    message += ` (响应时间: ${result.response_time.toFixed(2)}秒)`;
                } else if (result.message) {
                    message += ` - ${result.message}`;
                }
                message += '</li>';
            });
            message += '</ul>';
            
            // 显示结果
            $('#connection-alert')
                .removeClass('alert-info alert-danger')
                .addClass('alert-success')
                .html(message);
                
            // 加载API状态卡片
            loadAPIStatusCards(data.results);
        },
        error: function() {
            $('#connection-alert')
                .removeClass('alert-info alert-success')
                .addClass('alert-danger')
                .text('测试API连接时发生错误');
        }
    });
}

// 加载API状态卡片
function loadAPIStatusCards(statuses) {
    const container = $('#api-cards-container');
    container.empty();
    
    // 如果没有状态信息，显示默认消息
    if (!statuses || Object.keys(statuses).length === 0) {
        container.html('<div class="col-12 text-center"><p>没有可用的API状态信息</p></div>');
        return;
    }
    
    // 创建状态卡片
    Object.keys(statuses).forEach(apiType => {
        const status = statuses[apiType];
        const isActive = status.success;
        
        const statusClass = isActive ? 'bg-success' : 'bg-danger';
        const statusText = isActive ? '在线' : '离线';
        const cardClass = isActive ? 'active' : 'inactive';
        
        let cardContent = `
            <div class="col-md-6 col-lg-4">
                <div class="card api-card ${cardClass}">
                    <div class="card-body">
                        <span class="badge ${statusClass} status-badge">${statusText}</span>
                        <h5 class="card-title">${getAPIName(apiType)}</h5>
                        <h6 class="card-subtitle mb-2 text-muted">${status.url || '未配置'}</h6>
                        <p class="card-text">`;
        
        if (isActive) {
            cardContent += `状态: 正常<br>
                            <span class="response-time">响应时间: ${status.response_time.toFixed(2)}秒</span>`;
        } else {
            cardContent += `状态: 不可用<br>
                            <span class="text-danger">${status.message || '无法连接到API端点'}</span>`;
        }
        
        cardContent += `</p>
                    </div>
                </div>
            </div>
        `;
        
        container.append(cardContent);
    });
}

// 获取API名称显示
function getAPIName(apiType) {
    const apiNames = {
        'openai': 'OpenAI 兼容API',
        'deepseek': 'DeepSeek API',
        'qwen-vl': '千问 VL 多模态',
        'gemini': 'Google Gemini'
    };
    
    return apiNames[apiType] || apiType;
}

// 显示通知提示
function showToast(message, type = 'info') {
    const toastId = 'toast-' + Date.now();
    const toast = `
        <div id="${toastId}" class="toast" role="alert" aria-live="assertive" aria-atomic="true">
            <div class="toast-header">
                <strong class="me-auto">系统通知</strong>
                <button type="button" class="btn-close" data-bs-dismiss="toast" aria-label="Close"></button>
            </div>
            <div class="toast-body bg-${type} text-white">
                ${message}
            </div>
        </div>
    `;
    
    $('#toastContainer').append(toast);
    
    const toastElement = new bootstrap.Toast(document.getElementById(toastId), {
        delay: 5000
    });
    toastElement.show();
}

document.addEventListener('DOMContentLoaded', function() {
    console.log('API设置页面加载');
    
    // 修改加载顺序和错误处理
    // 首先加载主要的API配置，然后尝试加载其他可选配置
    loadAPISettings();
    
    // 使用try-catch处理可能不存在的API
    setTimeout(() => {
        try {
            loadConfigIniSettings();
        } catch (error) {
            console.warn('加载INI配置出错，这可能不影响主要功能', error);
        }
        
        try {
            loadConfigFileSettings();
        } catch (error) {
            console.warn('加载Config.py配置出错，这可能不影响主要功能', error);
        }
    }, 500);
});

// 修改loadConfigFileSettings添加更好的错误处理
async function loadConfigFileSettings() {
    try {
        // 先检查endpoint是否存在
        const checkResponse = await fetch('/api/config-file', {
            method: 'HEAD'
        }).catch(() => {
            // 如果连请求都无法发送，说明endpoint不存在
            throw new Error('config-file API不可用');
        });
        
        if (checkResponse.status === 404) {
            console.warn('Config.py管理API不可用，隐藏相关UI');
            // 隐藏相关UI
            const configFileForm = document.getElementById('config-file-form');
            if (configFileForm && configFileForm.closest('.card')) {
                configFileForm.closest('.card').style.display = 'none';
            }
            return;
        }
        
        const response = await fetch('/api/config-file');
        
        if (!response.ok) {
            throw new Error(`服务器响应错误: ${response.status}`);
        }
        
        const data = await response.json();
        
        if (!data.success) {
            throw new Error(data.error || '加载配置文件失败');
        }
        
        // ...existing code...
    } catch (error) {
        console.error('加载config.py文件配置失败:', error);
        // 不显示toast，避免太多提示干扰用户
        
        // 隐藏相关UI
        const configFileForm = document.getElementById('config-file-form');
        if (configFileForm && configFileForm.closest('.card')) {
            configFileForm.closest('.card').style.display = 'none';
        }
    }
}
