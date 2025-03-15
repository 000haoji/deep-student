"""
API设置接口
"""
import logging
import traceback
import time
import requests
from flask import Blueprint, jsonify, request
# 添加config模块导入
import config
import json
import os
from core.database import get_db, get_api_config, update_api_config
from datetime import datetime

# 避免循环导入，在需要的地方导入
# from core import database, config_manager, config_ini_manager

logger = logging.getLogger(__name__)

# 创建蓝图
settings_api = Blueprint('settings_api', __name__)

# 新增：重新加载配置端点
@settings_api.route('/api/reload_config', methods=['POST'])
def reload_config():
    """强制从数据库重新加载配置"""
    try:
        logging.info("收到重新加载配置请求")
        
        # 重新加载配置
        import config
        config.API_CONFIG = config.load_api_config_from_db()
        
        # 检查重新加载后的配置
        if 'api_alternatives' in config.API_CONFIG:
            api_alternatives = config.API_CONFIG['api_alternatives']
            logging.info(f"重新加载后的api_alternatives类型: {type(api_alternatives)}")
            
            if isinstance(api_alternatives, dict) and 'deepseek' in api_alternatives:
                deepseek_type = type(api_alternatives['deepseek'])
                logging.info(f"重新加载后的deepseek替代API类型: {deepseek_type}")
                
                if deepseek_type is list:
                    logging.info(f"列表格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个项目")
                    for i, item in enumerate(api_alternatives['deepseek']):
                        logging.info(f"DeepSeek替代API项目 {i+1}: {json.dumps(item, ensure_ascii=False)}")
                elif deepseek_type is dict:
                    logging.info(f"字典格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个键值对")
        
        return jsonify({
            "success": True,
            "message": "配置已重新加载"
        })
    except Exception as e:
        logging.error(f"重新加载配置失败: {str(e)}")
        traceback.print_exc()
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_api.route('/api/settings', methods=['GET'])
def get_api_settings():
    """获取所有API设置"""
    try:
        # 导入配置模块
        import config
        import sqlite3
        import json
        
        # 记录config.API_CONFIG的内容
        logging.info(f"当前API_CONFIG配置键: {list(config.API_CONFIG.keys())}")
        
        # 构建响应数据结构
        settings = {
            'default_extraction_model': config.API_CONFIG.get('default_extraction_model', 'multimodal_qwen-vl'),
            'default_analysis_model': config.API_CONFIG.get('default_analysis_model', 'deepseek'),
            'openai': config.API_CONFIG.get('openai', {}),
            'deepseek': config.API_CONFIG.get('deepseek', {}),
            'qwen_vl': config.API_CONFIG.get('qwen_vl', {}),
            'gemini': config.API_CONFIG.get('gemini', {}),
            'claude': config.API_CONFIG.get('claude', {}),
            'aliyun_ocr': config.API_CONFIG.get('aliyun_ocr', {})
        }
        
        # 直接从数据库读取api_alternatives配置
        try:
            conn = sqlite3.connect('math_errors.db')
            conn.row_factory = sqlite3.Row
            cursor = conn.cursor()
            
            # 查询api_alternatives配置
            cursor.execute("SELECT value FROM api_config WHERE key='api_alternatives'")
            row = cursor.fetchone()
            
            if row and row['value']:
                try:
                    settings['api_alternatives'] = json.loads(row['value'])
                    logging.info(f"从数据库加载api_alternatives成功，类型: {type(settings['api_alternatives'])}")
                    
                    # 检查deepseek配置
                    if 'deepseek' in settings['api_alternatives']:
                        deepseek_type = type(settings['api_alternatives']['deepseek'])
                        logging.info(f"数据库中的deepseek替代API类型: {deepseek_type}")
                        
                        if deepseek_type is list:
                            logging.info(f"列表格式的DeepSeek替代API包含 {len(settings['api_alternatives']['deepseek'])} 个项目")
                        elif deepseek_type is dict:
                            logging.info(f"字典格式的DeepSeek替代API包含 {len(settings['api_alternatives']['deepseek'])} 个键值对")
                except Exception as e:
                    logging.error(f"解析数据库中的api_alternatives失败: {e}")
                    settings['api_alternatives'] = {'deepseek': []}
            else:
                logging.warning("数据库中不存在api_alternatives配置")
                settings['api_alternatives'] = {'deepseek': []}
                
            conn.close()
        except Exception as e:
            logging.error(f"从数据库读取api_alternatives失败: {e}")
            # 使用config中的备用值
            settings['api_alternatives'] = config.API_CONFIG.get('api_alternatives', {'deepseek': []})
        
        # 记录将要返回的数据
        logging.info(f"返回API设置: {json.dumps(settings, ensure_ascii=False)[:200]}...")
        if 'api_alternatives' in settings and 'deepseek' in settings['api_alternatives']:
            deepseek_type = type(settings['api_alternatives']['deepseek'])
            logging.info(f"返回的deepseek替代API类型: {deepseek_type}")
            if deepseek_type is list:
                logging.info(f"返回的deepseek替代API项目数量: {len(settings['api_alternatives']['deepseek'])}")
            elif deepseek_type is dict:
                logging.info(f"返回的deepseek替代API键值对数量: {len(settings['api_alternatives']['deepseek'])}")
        
        return jsonify({
            "success": True,
            "data": settings
        })
        
    except Exception as e:
        logging.error(f"获取API设置时出错: {str(e)}")
        import traceback
        traceback.print_exc()
        return jsonify({
            "success": False,
            "error": str(e),
            "data": {
                'openai': {},
                'deepseek': {},
                'qwen_vl': {},
                'gemini': {},
                'claude': {},
                'aliyun_ocr': {},
                'api_alternatives': {'deepseek': []}
            }
        })

@settings_api.route('/api/settings', methods=['POST'])
def update_api_settings():
    """更新API设置并保存到数据库"""
    try:
        # 获取请求数据
        data = request.get_json()
        if not data:
            logger.error("未收到有效的请求数据")
            return jsonify({"success": False, "error": "未接收到有效的JSON数据"}), 400
        
        logger.info(f"收到更新API设置请求: {json.dumps(data, ensure_ascii=False)[:200]}...")
        
        # 特殊处理api_alternatives.deepseek
        if 'api_alternatives' in data and 'deepseek' in data['api_alternatives']:
            deepseek_apis = data['api_alternatives']['deepseek']
            logger.info(f"处理API替代配置，原始类型: {type(deepseek_apis)}")
            
            # 确保deepseek_apis是字典格式
            if isinstance(deepseek_apis, list):
                logger.info("转换列表格式的DeepSeek API为字典格式")
                # 将列表转换为字典
                api_dict = {}
                for i, api in enumerate(deepseek_apis):
                    if not api or not isinstance(api, dict) or not api.get('api_key'):
                        logger.info(f"跳过无效的API配置: {api}")
                        continue
                    
                    # 生成API ID
                    api_id = api.get('id') or api.get('original_id') or f"api_{i+1}"
                    # 确保每个API都有优先级
                    if 'priority' not in api:
                        api['priority'] = i + 1
                    # 添加到字典
                    api_dict[api_id] = api
                    logger.info(f"添加API {api_id}, 优先级: {api['priority']}")
                
                data['api_alternatives']['deepseek'] = api_dict
                logger.info(f"转换完成，共有 {len(api_dict)} 个API")
            
            # 确保字典格式的情况下每个API都是正确的
            if isinstance(data['api_alternatives']['deepseek'], dict):
                deepseek_dict = data['api_alternatives']['deepseek']
                # 移除main_api_id字段
                if 'main_api_id' in deepseek_dict:
                    del deepseek_dict['main_api_id']
                    logger.info("已移除main_api_id字段")
                
                # 确保每个API有唯一的优先级
                priorities = {}
                for api_id, api in deepseek_dict.items():
                    if not isinstance(api, dict):
                        logger.warning(f"跳过非字典类型的API: {api_id}")
                        continue
                    
                    # 确保有必要的字段
                    if 'api_key' not in api and 'key' in api:
                        api['api_key'] = api['key']
                    if 'api_url' not in api and 'url' in api:
                        api['api_url'] = api['url']
                    if 'model' not in api and 'model_name' in api:
                        api['model'] = api['model_name']
                    
                    # 添加model_name字段以确保兼容性
                    if 'model' in api and 'model_name' not in api:
                        api['model_name'] = api['model']
                    
                    # 确保优先级是整数
                    try:
                        priority = int(api.get('priority', 999))
                    except (ValueError, TypeError):
                        priority = 999
                    
                    api['priority'] = priority
                    
                    # 处理优先级冲突
                    while priority in priorities:
                        priority += 1
                        api['priority'] = priority
                        logger.info(f"API {api_id} 优先级冲突，调整为 {priority}")
                    
                    priorities[priority] = api_id
                
                logger.info(f"处理完成，共有 {len(deepseek_dict)} 个API，优先级: {priorities}")
        
        # 使用config模块保存设置
        success = config.save_api_settings(data)
        
        if success:
            logger.info("API设置已成功保存")
            # 刷新配置
            config.API_CONFIG = config.load_api_config_from_db()
            return jsonify({"success": True})
        else:
            logger.error("保存API设置失败")
            return jsonify({"success": False, "error": "保存API设置失败"}), 500
    
    except Exception as e:
        logger.error(f"更新API设置时出错: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

@settings_api.route('/api/test_connection', methods=['POST'])
def test_api_connection():
    """测试API连接是否可用
    
    支持测试不同类型的API连接，包括:
    - OpenAI
    - DeepSeek
    - 千问VL (Qwen-VL)
    - Google Gemini
    - Claude
    - 阿里云OCR
    - DeepSeek备选API (通过设置api_type=deepseek-alternative 和 api_id 来指定)
    
    请求体格式:
    {
        "api_type": "openai",   // API类型
        "api_key": "sk-xxx",    // API密钥
        "api_url": "https://api.openai.com/v1", // API基础URL
        "model": "gpt-4"    // 模型名称
    }
    
    对于DeepSeek备选API:
    {
        "api_type": "deepseek-alternative",  // 指定为备选API类型
        "api_id": "api_1",  // API ID，与api_alternatives.deepseek中的键对应
        "api_key": "sk-xxx",    // 可选，如果不提供则使用配置中的值
        "api_url": "https://...",  // 可选，如果不提供则使用配置中的值
        "model": "deepseek-chat"  // 可选，如果不提供则使用配置中的值
    }
    
    对于阿里云OCR:
    {
        "api_type": "aliyun-ocr",
        "access_key_id": "xxx",
        "access_key_secret": "xxx",
        "region_id": "cn-shanghai"
    }
    
    返回:
    {
        "success": true,
        "message": "连接成功",
        "details": {
            "response_time": 0.85,
            "model_info": {...}  // 模型信息
        }
    }
    """
    try:
        data = request.get_json()
        if not data:
            return jsonify({"success": False, "error": "未提供请求数据"}), 400
        
        # 记录测试请求
        logging.info(f"测试API连接请求数据: {json.dumps(data, ensure_ascii=False)}")
        
        # 获取API类型，兼容新旧参数名
        api_type = data.get('api_type') or data.get('type')
        if not api_type:
            return jsonify({"success": False, "error": "未指定API类型 (缺少api_type参数)"}), 400
        
        # 记录测试请求类型
        logging.info(f"测试API连接: {api_type}")
        
        # 格式化数据，统一参数名称
        test_data = {}
        # API密钥参数
        if 'api_key' in data:
            test_data['key'] = data['api_key']
        elif 'key' in data:
            test_data['key'] = data['key']
            
        # API URL参数
        if 'api_url' in data:
            test_data['url'] = data['api_url']
        elif 'url' in data:
            test_data['url'] = data['url']
            
        # 模型参数
        if 'model' in data:
            test_data['model'] = data['model']
            
        # 特殊参数 - OCR
        if 'access_key_id' in data:
            test_data['access_key_id'] = data['access_key_id']
        if 'access_key_secret' in data:
            test_data['access_key_secret'] = data['access_key_secret']
        if 'region_id' in data:
            test_data['region_id'] = data['region_id']
            
        # API ID参数 - 用于备选API
        if 'api_id' in data:
            test_data['api_id'] = data['api_id']
            
        # 特殊处理DeepSeek备选API
        if api_type == 'deepseek-alternative':
            return test_deepseek_alternative_connection(test_data)
        # 根据API类型进行测试
        elif api_type == 'openai':
            return test_openai_connection(test_data)
        elif api_type == 'deepseek':
            return test_deepseek_connection(test_data)
        elif api_type == 'qwen-vl':
            return test_qwen_connection(test_data)
        elif api_type == 'gemini':
            return test_gemini_connection(test_data)
        elif api_type == 'claude':
            return test_claude_connection(test_data)
        elif api_type == 'aliyun-ocr':
            return test_aliyun_ocr_connection(test_data)
        else:
            return jsonify({
                "success": False,
                "error": f"不支持的API类型: {api_type}"
            }), 400
    
    except Exception as e:
        logging.exception(f"测试API连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"测试API连接时发生错误: {str(e)}"
        }), 500

def test_openai_connection(data):
    """测试OpenAI API连接"""
    key = data.get('key')
    url = data.get('url')
    model = data.get('model', 'gpt-3.5-turbo')
    
    if not key or not url:
        return jsonify({
            "success": False,
            "error": "未提供OpenAI API密钥或URL"
        }), 400
    
    try:
        import openai
        import time
        
        # 设置API密钥和基础URL
        client = openai.OpenAI(
            api_key=key,
            base_url=url
        )
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送简单请求
        response = client.chat.completions.create(
            model=model,
            messages=[
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello, are you working?"}
            ],
            max_tokens=10
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "OpenAI API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "response": response.choices[0].message.content if response.choices else None
            }
        })
    
    except Exception as e:
        logging.exception(f"测试OpenAI连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"OpenAI连接失败: {str(e)}"
        }), 500

def test_deepseek_connection(data):
    """测试DeepSeek API连接"""
    key = data.get('key')
    url = data.get('url')
    model = data.get('model', 'deepseek-chat')
    
    if not key or not url:
        return jsonify({
            "success": False,
            "error": "未提供DeepSeek API密钥或URL"
        }), 400
    
    try:
        import openai
        import time
        
        # 设置API密钥和基础URL
        client = openai.OpenAI(
            api_key=key,
            base_url=url
        )
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送简单请求
        response = client.chat.completions.create(
            model=model,
            messages=[
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello, are you working?"}
            ],
            max_tokens=10
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "DeepSeek API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "response": response.choices[0].message.content if response.choices else None
            }
        })
    
    except Exception as e:
        logging.exception(f"测试DeepSeek连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"DeepSeek连接失败: {str(e)}"
        }), 500

def test_qwen_connection(data):
    """测试千问VL API连接"""
    key = data.get('key')
    url = data.get('url', 'https://dashscope.aliyuncs.com/v1')
    model = data.get('model', 'qwen-vl-plus')
    
    if not key:
        return jsonify({
            "success": False,
            "error": "未提供千问VL API密钥"
        }), 400
    
    try:
        import requests
        import time
        import json
        
        # API 端点
        api_url = f"{url.rstrip('/')}/services/aigc/multimodal-generation/generation"
        
        # 构建请求头
        headers = {
            "Authorization": f"Bearer {key}",
            "Content-Type": "application/json"
        }
        
        # 构建请求体 - 简单的文本查询
        payload = {
            "model": model,
            "input": {
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "text": "你好，请问你是什么模型?"
                            }
                        ]
                    }
                ]
            },
            "parameters": {}
        }
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送请求
        response = requests.post(
            api_url,
            headers=headers,
            data=json.dumps(payload)
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 检查响应状态
        response.raise_for_status()
        response_data = response.json()
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "千问VL API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "response": response_data
            }
        })
    
    except Exception as e:
        logging.exception(f"测试千问VL连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"千问VL连接失败: {str(e)}"
        }), 500

def test_gemini_connection(data):
    """测试Google Gemini API连接"""
    key = data.get('key')
    url = data.get('url', 'https://generativelanguage.googleapis.com')
    model = data.get('model', 'gemini-pro')
    
    if not key:
        return jsonify({
            "success": False,
            "error": "未提供Gemini API密钥"
        }), 400
    
    try:
        import requests
        import time
        import json
        
        # API 端点
        api_url = f"{url.rstrip('/')}/v1/models/{model}:generateContent?key={key}"
        
        # 构建请求头
        headers = {
            "Content-Type": "application/json"
        }
        
        # 构建请求体 - 简单的文本查询
        payload = {
            "contents": [
                {
                    "parts": [
                        {
                            "text": "Hello, are you working?"
                        }
                    ]
                }
            ]
        }
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送请求
        response = requests.post(
            api_url,
            headers=headers,
            data=json.dumps(payload)
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 检查响应状态
        response.raise_for_status()
        response_data = response.json()
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "Gemini API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "response": response_data
            }
        })
    
    except Exception as e:
        logging.exception(f"测试Gemini连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"Gemini连接失败: {str(e)}"
        }), 500

def test_claude_connection(data):
    """测试Claude API连接"""
    key = data.get('key')
    url = data.get('url', 'https://api.anthropic.com/v1/messages')
    model = data.get('model', 'claude-3-opus-20240229')
    
    if not key:
        return jsonify({
            "success": False,
            "error": "未提供Claude API密钥"
        }), 400
    
    try:
        import requests
        import time
        import json
        
        # 构建请求头
        headers = {
            "x-api-key": key,
            "anthropic-version": "2023-06-01",
            "content-type": "application/json"
        }
        
        # 构建请求体
        payload = {
            "model": model,
            "max_tokens": 10,
            "messages": [
                {
                    "role": "user",
                    "content": "Hello, are you working?"
                }
            ]
        }
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送请求
        response = requests.post(
            url,
            headers=headers,
            data=json.dumps(payload)
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 检查响应状态
        response.raise_for_status()
        response_data = response.json()
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "Claude API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "response": response_data
            }
        })
    
    except Exception as e:
        logging.exception(f"测试Claude连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"Claude连接失败: {str(e)}"
        }), 500

def test_aliyun_ocr_connection(data):
    """测试阿里云OCR API连接"""
    access_key_id = data.get('access_key_id')
    access_key_secret = data.get('access_key_secret')
    region_id = data.get('region_id', 'cn-shanghai')
    
    if not access_key_id or not access_key_secret:
        return jsonify({
            "success": False,
            "error": "未提供阿里云OCR AccessKey ID或Secret"
        }), 400
    
    try:
        # 尝试导入阿里云SDK
        from aliyunsdkcore.client import AcsClient
        from aliyunsdkcore.request import CommonRequest
        import time
        
        # 创建ACS客户端
        client = AcsClient(access_key_id, access_key_secret, region_id)
        
        # 创建通用请求
        request = CommonRequest()
        request.set_domain('ocr.cn-shanghai.aliyuncs.com')
        request.set_version('2019-12-30')
        request.set_action_name('DescribeRegions')  # 使用简单的API调用来测试连接
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送请求
        response = client.do_action_with_exception(request)
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": "阿里云OCR API连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "region": region_id
            }
        })
    
    except ImportError:
        logging.exception("缺少阿里云SDK，请安装aliyunsdkcore")
        return jsonify({
            "success": False,
            "error": "缺少阿里云SDK，请安装aliyunsdkcore"
        }), 500
    
    except Exception as e:
        logging.exception(f"测试阿里云OCR连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"阿里云OCR连接失败: {str(e)}"
        }), 500

@settings_api.route('/api/config-file', methods=['GET'])
def get_config_file_settings():
    """获取config.py文件中的配置（已弃用）"""
    try:
        # 直接从内存中获取配置
        # 处理API密钥，不直接返回完整密钥
        sanitized_config = {
            'openai': {},
            'deepseek': {},
            'default_models': {}
        }
        
        # OpenAI配置
        if 'openai' in config.API_CONFIG:
            openai_config = config.API_CONFIG['openai'].copy()
            if 'api_key' in openai_config and openai_config['api_key']:
                api_key = openai_config['api_key']
                if len(api_key) > 8:
                    openai_config['api_key'] = api_key[:4] + '*' * (len(api_key) - 8) + api_key[-4:]
            sanitized_config['openai'] = openai_config
            
        # DeepSeek配置
        if 'deepseek' in config.API_CONFIG:
            deepseek_config = config.API_CONFIG['deepseek'].copy()
            if 'api_key' in deepseek_config and deepseek_config['api_key']:
                api_key = deepseek_config['api_key']
                if len(api_key) > 8:
                    deepseek_config['api_key'] = api_key[:4] + '*' * (len(api_key) - 8) + api_key[-4:]
            sanitized_config['deepseek'] = deepseek_config
            
        # 默认模型配置
        if 'default_models' in config.API_CONFIG:
            sanitized_config['default_models'] = config.API_CONFIG['default_models'].copy()
        
        return jsonify({
            "success": True,
            "config_file": sanitized_config
        })
    except Exception as e:
        logger.error(f"获取配置文件设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_api.route('/api/config-file', methods=['POST'])
def update_config_file_settings():
    """更新config.py文件中的API配置（已弃用）"""
    try:
        data = request.json
        if not data:
            return jsonify({
                "success": False,
                "error": "未提供配置数据"
            }), 400
        
        # 使用新的配置更新逻辑
        updates = {}
        
        # 处理openai配置
        if 'openai' in data:
            openai_config = data['openai']
            # 处理API密钥（如果以星号开头，表示未修改）
            if 'api_key' in openai_config and openai_config['api_key'] and '*' in openai_config['api_key']:
                # 获取当前的API密钥
                current_openai = config.API_CONFIG.get('openai', {})
                openai_config['api_key'] = current_openai.get('api_key', '')
            updates['openai'] = openai_config
        
        # 处理deepseek配置
        if 'deepseek' in data:
            deepseek_config = data['deepseek']
            # 处理API密钥（如果以星号开头，表示未修改）
            if 'api_key' in deepseek_config and deepseek_config['api_key'] and '*' in deepseek_config['api_key']:
                # 获取当前的API密钥
                current_deepseek = config.API_CONFIG.get('deepseek', {})
                deepseek_config['api_key'] = current_deepseek.get('api_key', '')
            updates['deepseek'] = deepseek_config
        
        # 处理default_models配置
        if 'default_models' in data:
            updates['default_models'] = data['default_models']
        
        # 更新配置到数据库
        success = all([update_api_config(key, value) for key, value in updates.items()])
        
        if success:
            # 无需更新全局配置变量，update_api_config已经完成了这项工作
            
            return jsonify({
                "success": True,
                "message": "API配置已更新"
            })
        else:
            return jsonify({
                "success": False,
                "error": "更新API配置失败"
            }), 500
    except Exception as e:
        logger.error(f"更新配置文件设置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_api.route('/api/config-ini', methods=['GET'])
def get_config_ini_settings():
    """获取config.ini文件中的配置"""
    try:
        # 在函数内部导入，避免循环引用
        from core import config_ini_manager
        
        # 获取API配置
        api_config = config_ini_manager.get_api_config()
        
        # 获取其他配置
        vision_api = config_ini_manager.get_vision_api_config()
        text_api = config_ini_manager.get_text_api_config()
        
        # 处理密钥，不直接返回完整密钥
        for section in [api_config, vision_api, text_api]:
            for key, value in section.items():
                if ('api_key' in key or key == 'api_key') and value:
                    # 只显示密钥的前4位和后4位
                    if len(value) > 8:
                        section[key] = value[:4] + '*' * (len(value) - 8) + value[-4:]
                    else:
                        section[key] = '****'
        
        # 返回所有配置
        return jsonify({
            "success": True,
            "config": {
                "API": api_config,
                "VISION_API": vision_api,
                "TEXT_API": text_api
            }
        })
    except Exception as e:
        logger.error(f"获取INI配置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

@settings_api.route('/api/config-ini', methods=['POST'])
def update_config_ini_settings():
    """更新config.ini文件中的配置"""
    try:
        data = request.json
        if not data:
            return jsonify({
                "success": False,
                "error": "未提供配置数据"
            }), 400
        
        # 获取当前配置，用于处理API密钥的特殊情况
        from core import config_ini_manager
        current_api_config = config_ini_manager.get_api_config()
        current_vision_api = config_ini_manager.get_vision_api_config()
        current_text_api = config_ini_manager.get_text_api_config()
        
        # 更新API配置
        if "API" in data:
            api_updates = data["API"]
            
            # 处理API密钥
            for key in api_updates:
                if ('api_key' in key or key == 'key') and api_updates[key] and api_updates[key].startswith('*'):
                    # 使用当前值
                    api_updates[key] = current_api_config.get(key, '')
            
            # 更新API配置
            config_ini_manager.update_api_config(api_updates)
        
        # 更新VISION_API配置
        if "VISION_API" in data:
            vision_updates = data["VISION_API"]
            
            # 处理API密钥
            if 'api_key' in vision_updates and vision_updates['api_key'].startswith('*'):
                vision_updates['api_key'] = current_vision_api.get('api_key', '')
            
            # 获取配置并更新
            config = config_ini_manager.get_config()
            if 'VISION_API' not in config:
                config['VISION_API'] = {}
                
            for key, value in vision_updates.items():
                config['VISION_API'][key] = value
                
            config_ini_manager.save_config(config)
        
        # 更新TEXT_API配置
        if "TEXT_API" in data:
            text_updates = data["TEXT_API"]
            
            # 处理API密钥
            if 'api_key' in text_updates and text_updates['api_key'].startswith('*'):
                text_updates['api_key'] = current_text_api.get('api_key', '')
            
            # 获取配置并更新
            config = config_ini_manager.get_config()
            if 'TEXT_API' not in config:
                config['TEXT_API'] = {}
                
            for key, value in text_updates.items():
                config['TEXT_API'][key] = value
                
            config_ini_manager.save_config(config)
        
        return jsonify({
            "success": True,
            "message": "配置文件更新成功"
        })
    except Exception as e:
        logger.error(f"更新INI配置失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            "success": False,
            "error": str(e)
        }), 500

# 获取API配置
@settings_api.route('/api/config', methods=['GET'])
def get_api_config():
    try:
        # 从数据库获取API配置
        db_config = get_db_api_config()
        
        # 构建前端所需的格式
        # 为兼容旧的UI，将新的多API配置转换为原UI期望的格式
        
        # 默认使用OpenAI配置作为主要配置
        api_key = ""
        endpoint = "https://api.openai.com/v1"
        model = "gpt-4"
        enabled = False
        
        if 'openai' in db_config:
            openai_config = db_config['openai']
            api_key = openai_config.get('api_key', '')
            endpoint = openai_config.get('api_url', endpoint)
            model = openai_config.get('model', model)
        
        # API状态
        status = "unknown"
        if 'api_status' in db_config and 'openai' in db_config['api_status']:
            openai_status = db_config['api_status']['openai']
            if isinstance(openai_status, dict):
                status = "connected" if openai_status.get('success', False) else "error"
            else:
                status = "connected" if openai_status else "error"
        
        # 使用统计（这部分可以从其他地方获取）
        usage = {
            "calls": 0,
            "cost": 0.0,
            "limit": 100
        }
        
        config = {
            "provider": "openai",
            "api_key": api_key,
            "endpoint": endpoint,
            "model": model,
            "enabled": enabled,
            "status": status,
            "usage": usage
        }
        
        return jsonify(config)
    except Exception as e:
        return jsonify({"error": str(e)}), 500

# 保存API配置
@settings_api.route('/api/config', methods=['POST'])
def save_api_config():
    try:
        data = request.json
        
        # 提取数据
        provider = data.get('provider', 'openai')
        api_key = data.get('api_key', '')
        endpoint = data.get('endpoint', '')
        model = data.get('model', '')
        enabled = data.get('enabled', False)
        
        # 构建API配置
        api_updates = {
            'api_url': endpoint,
            'api_key': api_key,
            'model': model
        }
        
        # 保存到数据库
        update_api_config(provider, api_updates)
        
        # 保存状态
        if enabled:
            db = get_db()
            cursor = db.cursor()
            
            try:
                cursor.execute(
                    "UPDATE settings SET value = ? WHERE category = 'api' AND key = ?",
                    (json.dumps({'enabled': enabled}), 'status')
                )
            except:
                cursor.execute(
                    "INSERT INTO settings (category, key, value) VALUES (?, ?, ?)",
                    ('api', 'status', json.dumps({'enabled': enabled}))
                )
            
            db.commit()
        
        return jsonify({"success": True, "message": "设置已保存"})
    except Exception as e:
        return jsonify({"error": str(e)}), 500

# 测试API连接
@settings_api.route('/api/config/test', methods=['POST'])
def test_api_connection_config():
    try:
        data = request.json
        provider = data.get('provider', 'openai')
        api_key = data.get('api_key', '')
        endpoint = data.get('endpoint', '')
        model = data.get('model', 'gpt-4')
        
        # 创建与新API测试兼容的请求
        api_config = {
            'api_type': provider,
            'api_url': endpoint,
            'api_key': api_key,
            'api_model': model
        }
        
        # 调用新的API测试函数
        # 导入测试函数
        from routes.settings_routes import _test_api_connection
        
        # 测试连接
        result = _test_api_connection(provider, endpoint, api_key, model)
        
        # 映射结果
        status = "connected" if result.get('success', False) else "error"
        
        return jsonify({
            "status": status,
            "message": result.get('message', ''),
            "response_time": result.get('response_time', 0)
        })
    except Exception as e:
        return jsonify({"status": "error", "message": str(e)}), 500

# 重置API使用统计
@settings_api.route('/api/config/reset-usage', methods=['POST'])
def reset_api_usage():
    try:
        db = get_db()
        cursor = db.cursor()
        
        # 获取当前使用情况
        cursor.execute("SELECT value FROM settings WHERE category = 'api' AND key = 'usage'")
        result = cursor.fetchone()
        
        # 默认使用情况
        usage = {
            "calls": 0,
            "cost": 0.0,
            "limit": 100
        }
        
        if result:
            try:
                current_usage = json.loads(result[0])
                # 只重置调用次数和成本，保留限制
                usage["limit"] = current_usage.get("limit", 100)
            except:
                pass
            
            # 更新使用情况
            cursor.execute(
                "UPDATE settings SET value = ? WHERE category = 'api' AND key = 'usage'",
                (json.dumps(usage),)
            )
        else:
            # 插入新的使用情况
            cursor.execute(
                "INSERT INTO settings (category, key, value) VALUES (?, ?, ?)",
                ('api', 'usage', json.dumps(usage))
            )
        
        db.commit()
        
        return jsonify({"success": True, "usage": usage})
    except Exception as e:
        return jsonify({"error": str(e)}), 500

def test_deepseek_alternative_connection(data):
    """测试DeepSeek备选API连接"""
    api_id = data.get('api_id')
    if not api_id:
        return jsonify({
            "success": False,
            "error": "未提供DeepSeek备选API的ID"
        }), 400
    
    try:
        # 从全局配置获取备选API配置
        import config
        
        if ('api_alternatives' not in config.API_CONFIG or 
            'deepseek' not in config.API_CONFIG['api_alternatives'] or 
            api_id not in config.API_CONFIG['api_alternatives']['deepseek']):
            return jsonify({
                "success": False,
                "error": f"找不到指定的备选API配置: {api_id}"
            }), 404
        
        # 获取备选API配置
        api_config = config.API_CONFIG['api_alternatives']['deepseek'][api_id]
        
        # 使用提供的参数覆盖配置中的值（如果有）
        key = data.get('key', api_config.get('api_key', ''))
        url = data.get('url', api_config.get('api_url', ''))
        model = data.get('model', api_config.get('model_name', 'deepseek-chat'))
        name = api_config.get('name', f"备选API {api_id}")
        
        if not key or not url:
            return jsonify({
                "success": False,
                "error": f"未提供API密钥或URL: {name}"
            }), 400
        
        # 使用OpenAI客户端测试连接（DeepSeek API兼容OpenAI接口）
        import openai
        import time
        
        # 设置API密钥和基础URL
        client = openai.OpenAI(
            api_key=key,
            base_url=url
        )
        
        # 记录开始时间
        start_time = time.time()
        
        # 发送简单请求
        response = client.chat.completions.create(
            model=model,
            messages=[
                {"role": "system", "content": "You are a helpful assistant."},
                {"role": "user", "content": "Hello, are you working?"}
            ],
            max_tokens=10
        )
        
        # 计算响应时间
        response_time = time.time() - start_time
        
        # 返回成功结果
        return jsonify({
            "success": True,
            "message": f"DeepSeek备选API({name})连接成功",
            "details": {
                "response_time": round(response_time, 2),
                "model": model,
                "name": name,
                "api_id": api_id,
                "response": response.choices[0].message.content if response.choices else None
            }
        })
    
    except Exception as e:
        logging.exception(f"测试DeepSeek备选API连接时发生错误: {str(e)}")
        return jsonify({
            "success": False,
            "error": f"DeepSeek备选API连接失败: {str(e)}"
        }), 500
