"""
设置相关路由，包含API配置管理接口
"""
import os
import json
import logging
import traceback
import time
import requests
import copy
from flask import Blueprint, jsonify, request, render_template
from core import database
from core import config_manager
import config  # 导入config模块
from core.database import update_api_config, get_api_config as get_db_api_config
from config import API_CONFIG

logger = logging.getLogger(__name__)

# 创建蓝图
settings_blueprint = Blueprint('settings', __name__, url_prefix='')

# 从config模块导入API_CONFIG
API_CONFIG = config.API_CONFIG

@settings_blueprint.route('/api/config-file', methods=['GET'])
def get_config_file():
    """获取config.py文件中的配置"""
    try:
        # 读取配置文件内容
        content = config_manager.read_config_file()
        if not content:
            return jsonify({
                "success": False,
                "error": "无法读取配置文件"
            }), 500
        
        # 提取API配置
        api_config = config_manager.extract_api_config(content)
        if not api_config:
            return jsonify({
                "success": False,
                "error": "无法提取API配置"
            }), 500
        
        # 处理API密钥，不直接返回完整密钥
        if 'openai' in api_config and 'api_key' in api_config['openai']:
            api_key = api_config['openai']['api_key']
            if api_key and len(api_key) > 8:
                api_config['openai']['api_key'] = api_key[:4] + '*' * (len(api_key) - 8) + api_key[-4:]
        
        if 'deepseek' in api_config and 'api_key' in api_config['deepseek']:
            api_key = api_config['deepseek']['api_key']
            if api_key and len(api_key) > 8:
                api_config['deepseek']['api_key'] = api_key[:4] + '*' * (len(api_key) - 8) + api_key[-4:]
        
        return jsonify({
            "success": True,
            "config_file": api_config
        })
    except Exception as e:
        logger.error(f"获取配置文件设置失败: {str(e)}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_blueprint.route('/api/config-file', methods=['POST'])
def update_config_file():
    """更新config.py文件中的API配置"""
    try:
        data = request.json
        if not data:
            return jsonify({
                "success": False,
                "error": "未提供配置数据"
            }), 400
        
        # 获取当前配置，用于处理API密钥的特殊情况
        content = config_manager.read_config_file()
        current_config = config_manager.extract_api_config(content)
        
        # 处理API密钥
        if 'openai' in data and 'api_key' in data['openai']:
            api_key = data['openai']['api_key']
            if api_key and api_key.startswith('*'):
                # 使用当前配置中的密钥
                if current_config and 'openai' in current_config and 'api_key' in current_config['openai']:
                    data['openai']['api_key'] = current_config['openai']['api_key']
        
        if 'deepseek' in data and 'api_key' in data['deepseek']:
            api_key = data['deepseek']['api_key']
            if api_key and api_key.startswith('*'):
                # 使用当前配置中的密钥
                if current_config and 'deepseek' in current_config and 'api_key' in current_config['deepseek']:
                    data['deepseek']['api_key'] = current_config['deepseek']['api_key']
        
        # 更新配置文件
        if config_manager.update_config_file(data):
            return jsonify({
                "success": True,
                "message": "配置文件更新成功"
            })
        else:
            return jsonify({
                "success": False,
                "error": "配置文件更新失败"
            }), 500
    except Exception as e:
        logger.error(f"更新配置文件失败: {str(e)}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_blueprint.route('/api-settings')
def api_settings_page():
    """渲染API设置页面"""
    return render_template('api_settings.html')

@settings_blueprint.route('/api/settings', methods=['GET'])
def get_settings():
    """获取API设置配置"""
    try:
        # 从数据库获取配置
        db_config = get_db_api_config()
        
        # 构建前端所需的配置结构
        response = {
            'endpoints': {
                'openai': {
                    'url': db_config.get('openai', {}).get('api_url', ''),
                    'key': db_config.get('openai', {}).get('api_key', ''),
                    'model': db_config.get('openai', {}).get('model', 'gpt-3.5-turbo')
                },
                'deepseek': {
                    'url': db_config.get('deepseek', {}).get('api_url', ''),
                    'key': db_config.get('deepseek', {}).get('api_key', ''),
                    'model': db_config.get('deepseek', {}).get('model', 'deepseek-chat')
                },
                'qwen-vl': {
                    'url': db_config.get('qwen-vl', {}).get('api_url', ''),
                    'key': db_config.get('qwen-vl', {}).get('api_key', ''),
                    'model': db_config.get('qwen-vl', {}).get('model', 'qwen-vl-plus')
                },
                'gemini': {
                    'url': db_config.get('gemini', {}).get('api_url', ''),
                    'key': db_config.get('gemini', {}).get('api_key', ''),
                    'model': db_config.get('gemini', {}).get('model', 'gemini-pro')
                }
            },
            'default_models': db_config.get('default_models', {
                'extraction': '',
                'analysis': ''
            }),
            'api_status': {}
        }
        
        # 获取API状态
        api_status_config = db_config.get('api_status', {})
        
        # 转换为前端所需的简单格式
        for api_type, status_info in api_status_config.items():
            if isinstance(status_info, dict):
                response['api_status'][api_type] = status_info.get('success', False)
            else:
                # 向后兼容，如果状态不是字典
                response['api_status'][api_type] = bool(status_info)
        
        # 确保至少包含所有API类型
        all_api_types = ['openai', 'deepseek', 'qwen-vl', 'gemini']
        for api_type in all_api_types:
            if api_type not in response['api_status']:
                response['api_status'][api_type] = False
        
        # 返回JSON响应
        return jsonify({'success': True, 'data': db_config})
    except Exception as e:
        logger.error(f"获取API设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({'success': False, 'error': '获取设置失败，请检查日志'}), 500

@settings_blueprint.route('/api/settings', methods=['POST'])
def save_settings():
    """保存API设置"""
    try:
        data = request.json
        if not data:
            return jsonify({
                "success": False,
                "error": "未提供设置数据"
            }), 400
        
        logger.info(f"接收到API设置保存请求")
        
        # 获取当前配置用于日志比较
        current_config = get_db_api_config()
        
        # 处理默认模型
        if 'default_models' in data:
            # 使用config中的专用函数进行保存，以确保正确处理JSON数据
            extraction_model = data['default_models'].get('extraction', '')
            analysis_model = data['default_models'].get('analysis', '')
            config.set_default_models(extraction_model, analysis_model)
            logger.info(f"保存默认模型设置: 提取={extraction_model}, 分析={analysis_model}")
        
        # 处理DeepSeek API设置 - 使用专用函数保存
        apis = {}
        
        # 获取替代API配置
        if 'api_alternatives' in data and 'deepseek' in data['api_alternatives']:
            deepseek_alts = data['api_alternatives']['deepseek']
            
            # 根据类型处理
            if isinstance(deepseek_alts, list):
                # 将列表转换为字典
                for i, api in enumerate(deepseek_alts):
                    if not api or not isinstance(api, dict) or not api.get('api_key'):
                        continue
                    api_id = api.get('id') or f"api_{i+1}"
                    # 设置优先级
                    if 'priority' not in api:
                        api['priority'] = i + 1
                    apis[api_id] = api
                    
            elif isinstance(deepseek_alts, dict):
                apis = deepseek_alts
        
        # 如果'deepseek'键存在（原主API），将其添加为优先API
        if 'deepseek' in data and isinstance(data['deepseek'], dict) and data['deepseek'].get('api_key'):
            main_api = data['deepseek']
            
            # 生成唯一ID
            api_id = main_api.get('id') or f"main_api_{int(time.time())}"
            
            # 确保设置优先级 - 前端可能已经设置了优先级
            if 'priority' not in main_api:
                main_api['priority'] = 1  # 默认优先级为1
                
            # 添加到APIs字典
            apis[api_id] = main_api
        
        # 保存所有API
        config.save_deepseek_api_config(apis)
        logger.info(f"保存DeepSeek API设置: {len(apis)} 个API")
        
        # 处理其他主要API配置
        if 'openai' in data:
            update_api_config('openai', data['openai'])
        
        if 'qwen_vl' in data:
            update_api_config('qwen_vl', data['qwen_vl'])
        
        if 'gemini' in data:
            update_api_config('gemini', data['gemini'])
        
        if 'claude' in data:
            update_api_config('claude', data['claude'])
        
        if 'aliyun_ocr' in data:
            update_api_config('aliyun_ocr', data['aliyun_ocr'])
        
        # 获取更新后配置用于日志比较
        updated_config = get_db_api_config()
        
        # 检查DeepSeek API配置
        if 'api_alternatives' in updated_config and 'deepseek' in updated_config['api_alternatives']:
            # 检查deepseek值是否是字典
            deepseek_apis = updated_config['api_alternatives']['deepseek']
            if isinstance(deepseek_apis, dict):
                logger.info(f"更新后有 {len(deepseek_apis)} 个DeepSeek API")
                
                # 记录所有API的优先级
                priority_list = []
                for api_id, api in deepseek_apis.items():
                    if isinstance(api, dict) and 'priority' in api:
                        priority_list.append((api.get('name', api_id), api['priority']))
                
                # 按优先级排序
                priority_list.sort(key=lambda x: x[1])
                
                # 记录排序后的API
                logger.info("按优先级排序后的DeepSeek API:")
                for name, priority in priority_list:
                    logger.info(f"- {name} (优先级: {priority})")
            elif isinstance(deepseek_apis, list):
                logger.info(f"更新后有 {len(deepseek_apis)} 个DeepSeek API (列表格式)")
            else:
                logger.warning(f"DeepSeek API 配置格式异常: {type(deepseek_apis)}")
        
        # 检查默认模型配置是否正确保存
        try:
            default_models = config.get_default_models()
            logger.info(f"更新后的默认模型配置: 提取={default_models.get('extraction', '')}, 分析={default_models.get('analysis', '')}")
        except Exception as e:
            logger.error(f"获取保存后的默认模型配置失败: {str(e)}")
        
        logger.info("API设置保存成功")
        
        # 刷新全局配置以确保变更立即生效
        config.API_CONFIG = config.load_api_config_from_db()
        
        # 打印获取可用API信息用于调试
        try:
            available_apis = config.get_available_deepseek_api()
            if available_apis:
                logger.info(f"刷新后可用的DeepSeek API: {len(available_apis)}个")
                for i, api in enumerate(available_apis):
                    logger.info(f"API {i+1}: {api.get('name', 'Unknown')} (优先级: {api.get('priority', 999)})")
            else:
                logger.warning("刷新后没有可用的DeepSeek API")
        except Exception as e:
            logger.error(f"获取可用API信息失败: {str(e)}")
        
        return jsonify({
            "success": True,
            "message": "API设置保存成功"
        })
    except Exception as e:
        logger.error(f"保存API设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_blueprint.route('/api/settings/endpoints', methods=['POST'])
def save_endpoints():
    """保存API端点设置"""
    try:
        # 获取请求数据
        data = request.json
        
        # 更新所有API配置
        all_api_types = ['openai', 'deepseek', 'qwen-vl', 'gemini']
        
        for api_type in all_api_types:
            if api_type in data:
                api_updates = {
                    'api_url': data[api_type].get('url', ''),
                    'api_key': data[api_type].get('key', ''),
                    'model': data[api_type].get('model', '')
                }
                
                # 如果是以*开头的键，则保留原值
                if api_updates['api_key'].startswith('*') and api_type in API_CONFIG:
                    api_updates['api_key'] = API_CONFIG[api_type].get('api_key', '')
                
                # 保存到数据库
                update_api_config(api_type, api_updates)
        
        return jsonify({'success': True, 'message': '保存成功'})
    except Exception as e:
        logger.error(f"保存API端点设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({'error': '保存失败，请检查日志'}), 500

@settings_blueprint.route('/api/settings/models', methods=['POST'])
def save_models():
    """保存默认模型设置"""
    try:
        # 获取请求数据
        data = request.json
        
        # 构建默认模型更新
        default_models_updates = {
            'extraction': data.get('extraction', ''),
            'analysis': data.get('analysis', '')
        }
        
        # 保存到数据库
        update_api_config('default_models', default_models_updates)
        
        # 更新全局配置变量
        # 注意：数据库更新后，API_CONFIG应该自动更新或重新加载
        # 这里可以添加额外的逻辑确保全局变量与数据库保持同步
        
        return jsonify({'success': True, 'message': '默认模型保存成功'})
    except Exception as e:
        logger.error(f"保存默认模型设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({'error': '保存失败，请检查日志'}), 500

def _update_api_status(api_type, status, response_time=None):
    """更新API连接状态"""
    try:
        # 获取当前API状态
        db_config = get_db_api_config()
        api_status = db_config.get('api_status', {})
        
        # 更新状态
        api_status[api_type] = {
            'success': status,
            'last_check': time.time(),
            'response_time': response_time if response_time is not None else 0
        }
        
        # 保存到数据库
        update_api_config('api_status', api_status)
        
        return True
    except Exception as e:
        logger.error(f"更新API状态失败: {str(e)}\n{traceback.format_exc()}")
        return False

@settings_blueprint.route('/api/settings/test-connection', methods=['POST'])
def test_connection():
    """测试API连接"""
    try:
        # 获取请求数据
        data = request.json
        api_type = data.get('api_type', '')
        api_url = data.get('api_url', '')
        api_key = data.get('api_key', '')
        api_model = data.get('api_model', '')
        
        if not api_type or not api_url or not api_key:
            return jsonify({
                'success': False, 
                'message': '缺少API类型、URL或密钥'
            }), 400
        
        # 如果密钥以*开头，使用配置中的密钥
        if api_key.startswith('*') and api_type in API_CONFIG:
            api_key = API_CONFIG[api_type].get('api_key', '')
        
        # 构建请求头部
        headers = {
            'Content-Type': 'application/json',
            'Authorization': f'Bearer {api_key}'
        }
        
        # 根据API类型构建测试请求
        if api_type == 'openai':
            payload = {
                'model': api_model or 'gpt-3.5-turbo',
                'messages': [{'role': 'user', 'content': 'Hello'}],
                'max_tokens': 5
            }
            endpoint = '/v1/chat/completions'
        elif api_type == 'deepseek':
            payload = {
                'model': api_model or 'deepseek-chat',
                'messages': [{'role': 'user', 'content': 'Hello'}],
                'max_tokens': 5
            }
            endpoint = '/v1/chat/completions'
        elif api_type == 'qwen-vl':
            payload = {
                'model': api_model or 'qwen-vl-plus',
                'messages': [{'role': 'user', 'content': 'Hello'}],
                'max_tokens': 5
            }
            endpoint = '/v1/chat/completions'
        elif api_type == 'gemini':
            # Gemini API有不同的请求格式
            payload = {
                'contents': [{'parts': [{'text': 'Hello'}]}],
                'generationConfig': {'maxOutputTokens': 5}
            }
            endpoint = '/v1beta/models/' + (api_model or 'gemini-pro') + ':generateContent'
            # 更新Gemini的Authorization头格式
            headers = {
                'Content-Type': 'application/json',
                'x-goog-api-key': api_key  # Gemini使用不同的认证方式
            }
        else:
            return jsonify({
                'success': False, 
                'message': f'不支持的API类型: {api_type}'
            }), 400
        
        # 清理API URL
        if api_url.endswith('/'):
            api_url = api_url[:-1]
        
        # 构建完整的请求URL
        if api_type == 'gemini':
            # Gemini API的URL格式不同
            request_url = f"{api_url}{endpoint}"
        else:
            # 其他API的URL格式
            if '/v1' not in api_url:
                request_url = f"{api_url}/v1/chat/completions"
            else:
                request_url = f"{api_url}/chat/completions"
        
        # 发送测试请求并计时
        start_time = time.time()
        
        try:
            response = requests.post(
                request_url,
                headers=headers,
                json=payload,
                timeout=10
            )
            response_time = time.time() - start_time
            
            # 检查响应
            if response.status_code in [200, 201]:
                # 更新API状态
                _update_api_status(api_type, True, response_time)
                
                return jsonify({
                    'success': True,
                    'message': f'{api_type} API连接成功',
                    'response_time': response_time
                })
            else:
                # 更新API状态
                _update_api_status(api_type, False)
                
                error_detail = response.text[:200] if response.text else '无详细信息'
                return jsonify({
                    'success': False,
                    'message': f'API响应错误 (状态码: {response.status_code}): {error_detail}'
                }), 400
                
        except requests.exceptions.Timeout:
            # 更新API状态
            _update_api_status(api_type, False)
            
            return jsonify({
                'success': False,
                'message': 'API请求超时'
            }), 408
        except requests.exceptions.ConnectionError:
            # 更新API状态
            _update_api_status(api_type, False)
            
            return jsonify({
                'success': False,
                'message': f'无法连接到 {api_type} API服务器'
            }), 503
    
    except Exception as e:
        logger.error(f"API连接测试失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            'success': False, 
            'message': f'连接测试失败: {str(e)}'
        }), 500

@settings_blueprint.route('/api/settings/test-all-connections', methods=['POST'])
def test_all_connections():
    """测试所有API连接"""
    try:
        # 从数据库获取配置
        db_config = get_db_api_config()
        
        # 测试结果
        results = {}
        
        # 测试所有类型的API
        api_types = ['openai', 'deepseek', 'qwen-vl', 'gemini']
        
        for api_type in api_types:
            if api_type in db_config:
                api_config = db_config[api_type]
                api_url = api_config.get('api_url', '')
                api_key = api_config.get('api_key', '')
                api_model = api_config.get('model', '')
                
                if api_url and api_key:
                    try:
                        # 测试API连接
                        api_result = _test_api_connection(api_type, api_url, api_key, api_model)
                        results[api_type] = api_result
                        
                        # 更新API状态
                        _update_api_status(api_type, api_result['success'], 
                                         api_result.get('response_time', 0) if api_result['success'] else None)
                    except Exception as e:
                        logger.error(f"测试{api_type} API失败: {str(e)}")
                        results[api_type] = {
                            'success': False,
                            'message': str(e)
                        }
                        # 更新API状态
                        _update_api_status(api_type, False)
                else:
                    results[api_type] = {
                        'success': False,
                        'message': '未配置API URL或密钥'
                    }
                    # 更新API状态
                    _update_api_status(api_type, False)
        
        return jsonify({'results': results})
    except Exception as e:
        logger.error(f"测试所有API连接失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({'error': '测试连接失败，请检查日志'}), 500

def _test_api_connection(api_type, api_url, api_key, api_model=None):
    """测试单个API连接并返回结果"""
    # 构建请求头部
    headers = {
        'Content-Type': 'application/json',
        'Authorization': f'Bearer {api_key}'
    }
    
    # 根据API类型构建测试请求
    if api_type == 'openai':
        payload = {
            'model': api_model or 'gpt-3.5-turbo',
            'messages': [{'role': 'user', 'content': 'Hello'}],
            'max_tokens': 5
        }
        endpoint = '/chat/completions'
    elif api_type == 'deepseek':
        payload = {
            'model': api_model or 'deepseek-chat',
            'messages': [{'role': 'user', 'content': 'Hello'}],
            'max_tokens': 5
        }
        endpoint = '/chat/completions'
    elif api_type == 'qwen-vl':
        payload = {
            'model': api_model or 'qwen-vl-plus',
            'messages': [{'role': 'user', 'content': 'Hello'}],
            'max_tokens': 5
        }
        endpoint = '/chat/completions'
    elif api_type == 'gemini':
        # Gemini API有不同的请求格式
        payload = {
            'contents': [{'parts': [{'text': 'Hello'}]}],
            'generationConfig': {'maxOutputTokens': 5}
        }
        endpoint = '/v1beta/models/' + (api_model or 'gemini-pro') + ':generateContent'
        # 更新Gemini的Authorization头格式
        headers = {
            'Content-Type': 'application/json',
            'x-goog-api-key': api_key  # Gemini使用不同的认证方式
        }
    else:
        return {'success': False, 'message': f'不支持的API类型: {api_type}'}
    
    # 清理API URL
    if api_url.endswith('/'):
        api_url = api_url[:-1]
    
    # 构建完整的请求URL
    if api_type == 'gemini':
        # Gemini API的URL格式不同
        request_url = f"{api_url}{endpoint}"
    else:
        # 如果URL不包含v1，添加它
        if '/v1' not in api_url:
            request_url = f"{api_url}/v1{endpoint}"
        else:
            request_url = f"{api_url}{endpoint}"
    
    # 发送测试请求并计时
    start_time = time.time()
    
    try:
        response = requests.post(
            request_url,
            headers=headers,
            json=payload,
            timeout=10
        )
        response_time = time.time() - start_time
        
        # 检查响应
        if response.status_code in [200, 201]:
            return {
                'success': True,
                'message': '连接成功',
                'response_time': response_time
            }
        else:
            error_detail = response.text[:200] if response.text else '无详细信息'
            return {
                'success': False,
                'message': f'API响应错误 (状态码: {response.status_code}): {error_detail}'
            }
            
    except requests.exceptions.Timeout:
        return {
            'success': False,
            'message': 'API请求超时'
        }
    except requests.exceptions.ConnectionError:
        return {
            'success': False,
            'message': f'无法连接到API服务器'
        }
    except Exception as e:
        return {
            'success': False,
            'message': str(e)
        }