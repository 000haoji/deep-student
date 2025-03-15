"""
重新设计的组合模型分析模块 - 分离不同AI模型的职责
- Qwen负责OCR、格式化输出
- DeepseekR1负责深度错误分析（流式输出）
"""
import os
import base64
import json
import requests
import traceback
import logging
import time
from config import API_CONFIG, SUBJECT_ANALYSIS_PROMPTS, SUBJECTS
from .utils import extract_field_from_text, extract_number_from_text, extract_tags_from_text
from .subject_prompts import get_prompt_for_subject
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

logger = logging.getLogger(__name__)

# 新增：辅助函数 - 获取主DeepSeek API配置
def get_main_deepseek_api():
    """获取主要DeepseekAPI配置"""
    default_api = {
        "api_key": "",
        "api_url": "",
        "model": "deepseek-chat",
        "model_name": "deepseek-chat"  # 确保同时包含model和model_name字段
    }
    
    try:
        from config import API_CONFIG
        
        # 记录所有可用的API配置
        all_apis = []
        
        # 1. 收集所有API配置（不再区分主API和替代API）
        # 检查api_alternatives.deepseek配置
        if API_CONFIG and 'api_alternatives' in API_CONFIG and 'deepseek' in API_CONFIG['api_alternatives']:
            deepseek_config = API_CONFIG['api_alternatives']['deepseek']
            
            # 处理对象格式配置
            if isinstance(deepseek_config, dict):
                for api_id, api_config in deepseek_config.items():
                    if isinstance(api_config, dict):
                        api_key = api_config.get('api_key', api_config.get('key', ''))
                        api_url = api_config.get('api_url', api_config.get('url', ''))
                        model = api_config.get('model', api_config.get('model_name', 'deepseek-chat'))
                        name = api_config.get('name', f'API {api_id}')
                        priority = api_config.get('priority', 999)
                        
                        if api_key and api_url:
                            all_apis.append({
                                'id': api_id,
                                'name': name,
                                'api_key': api_key,
                                'api_url': api_url,
                                'model': model,
                                'model_name': model,  # 确保同时包含model和model_name字段
                                'priority': priority
                            })
                            logger.info(f"找到API配置: {name} (ID: {api_id}, 优先级: {priority})")
            
            # 处理数组格式配置
            elif isinstance(deepseek_config, list):
                for i, api_config in enumerate(deepseek_config):
                    if isinstance(api_config, dict):
                        api_id = api_config.get('id', f'api_{i+1}')
                        api_key = api_config.get('api_key', api_config.get('key', ''))
                        api_url = api_config.get('api_url', api_config.get('url', ''))
                        model = api_config.get('model', api_config.get('model_name', 'deepseek-chat'))
                        name = api_config.get('name', f'API {i+1}')
                        priority = api_config.get('priority', i+1)
                        
                        if api_key and api_url:
                            all_apis.append({
                                'id': api_id,
                                'name': name,
                                'api_key': api_key,
                                'api_url': api_url,
                                'model': model,
                                'model_name': model,  # 确保同时包含model和model_name字段
                                'priority': priority
                            })
                            logger.info(f"找到API配置: {name} (ID: {api_id}, 优先级: {priority})")
        
        # 2. 按优先级排序
        if all_apis:
            all_apis.sort(key=lambda x: x['priority'])
            
            # 记录排序后的API
            logger.info("按优先级排序后的API列表:")
            for api in all_apis:
                logger.info(f"- {api['name']} (ID: {api['id']}, 优先级: {api['priority']})")
            
            # 返回最高优先级的API配置
            highest_priority_api = all_apis[0]
            logger.info(f"选择最高优先级的API: {highest_priority_api['name']} (优先级: {highest_priority_api['priority']})")
            
            return {
                "api_key": highest_priority_api['api_key'],
                "api_url": highest_priority_api['api_url'],
                "model": highest_priority_api['model'],
                "model_name": highest_priority_api['model_name']  # 确保返回model_name字段
            }
        
        # 如果没有找到API，返回默认配置
        logger.warning("未找到有效的DeepseekAPI配置，返回默认空配置")
        return default_api
        
    except Exception as e:
        logger.error(f"获取DeepseekAPI配置时出错: {str(e)}\n{traceback.format_exc()}")
        return default_api

# 创建一个具有重试功能的Session对象
def create_retry_session(retries=3, backoff_factor=0.5, status_forcelist=(500, 502, 503, 504)):
    session = requests.Session()
    retry = Retry(
        total=retries,
        read=retries,
        connect=retries,
        backoff_factor=backoff_factor,
        status_forcelist=status_forcelist,
    )
    adapter = HTTPAdapter(max_retries=retry)
    session.mount('http://', adapter)
    session.mount('https://', adapter)
    return session

# 新增函数：安全地截取和记录包含Unicode字符的文本
def safe_text_logging(text, length=100):
    """安全地截取并处理可能包含Unicode字符的文本，以便日志记录"""
    if not text:
        return "[空文本]"
    
    # 将特殊Unicode字符替换为ASCII表示
    text = text.replace("\u2081", "_1").replace("\u2082", "_2").replace("\u2083", "_3") # 下标字符
    text = text.replace("∫", "integral").replace("∞", "inf").replace("→", "->")  # 常见数学符号
    
    # 截取指定长度
    if len(text) > length:
        return text[:length] + "..."
    return text

def analyze_with_multimodal_and_deepseek(image_path, multimodal_model="qwen-vl", user_notes="", subject="math"):
    """使用多模态模型分析图片，Qwen负责OCR和结构化输出，DeepseekR1负责深度分析

    Args:
        image_path (str): 图片路径
        multimodal_model (str): 多模态模型名称（qwen-vl、gpt4v、claude3、gemini）
        user_notes (str): 用户提供的补充文字
        subject (str): 学科（math、physics、chemistry、english、politics）
    """
    logger.info(f"使用 {multimodal_model} + DeepseekR1组合模式进行图像分析，学科: {subject}")
    
    # 基本参数检查
    if not os.path.exists(image_path):
        error_msg = f"图片文件不存在: {image_path}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 获取 API 配置
    try:
        # 检查多模态模型配置是否存在
        if "multimodal_models" not in API_CONFIG:
            logger.error("multimodal_models配置不存在")
            return {"error": "未找到multimodal_models配置"}
        
        # 检查指定的多模态模型配置是否存在
        if multimodal_model not in API_CONFIG["multimodal_models"]:
            logger.error(f"未配置的多模态模型: {multimodal_model}")
            return {"error": f"未配置的多模态模型: {multimodal_model}"}
            
        # 获取多模态模型配置
        mm_config = API_CONFIG["multimodal_models"][multimodal_model]
        
        # 验证多模态模型配置的完整性
        if not all(key in mm_config for key in ["api_key", "api_url", "model_name"]):
            logger.error(f"多模态模型 {multimodal_model} 配置不完整")
            return {"error": f"多模态模型 {multimodal_model} 配置不完整，缺少必要字段"}
            
        # 验证多模态模型配置的有效性
        if not mm_config["api_key"] or not mm_config["api_url"]:
            logger.error(f"多模态模型 {multimodal_model} 配置包含空值")
            return {"error": f"多模态模型 {multimodal_model} API密钥或URL为空"}
            
        # 获取多模态模型配置
        mm_api_key = mm_config["api_key"]
        mm_api_url = mm_config["api_url"]
        mm_model_name = mm_config["model_name"]
        
        # 获取DeepseekR1配置 - 使用新的辅助函数
        r1_config = get_main_deepseek_api()
        
        # 验证DeepseekR1配置的有效性
        if not r1_config["api_key"] or not r1_config["api_url"]:
            logger.error("DeepseekR1 API密钥或URL为空")
            return {"error": "DeepseekR1 API密钥或URL为空"}
            
        # 获取DeepseekR1配置
        r1_api_key = r1_config["api_key"]
        r1_api_url = r1_config["api_url"]
        r1_model_name = r1_config.get("model_name", r1_config.get("model", "deepseek-chat"))
        
        logger.info(f"使用多模态模型: {mm_model_name} + DeepseekR1")
    except KeyError as e:
        error_msg = f"API 配置缺失: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 第一步：读取图片文件并转为Base64
    try:
        with open(image_path, "rb") as image_file:
            image_bytes = image_file.read()
        
        base64_image = base64.b64encode(image_bytes).decode('utf-8')
        logger.info(f"成功将图片编码为 base64, 大小: {len(base64_image)}")
    except Exception as e:
        error_msg = f"图片编码错误: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 第二步：使用Qwen进行OCR和结构化分析
    try:
        # 获取对应学科的Qwen提示词
        qwen_prompt = ""
        if subject in SUBJECT_ANALYSIS_PROMPTS:
            qwen_prompt = SUBJECT_ANALYSIS_PROMPTS[subject].get('qwen_prompt', '')
        else:
            # 如果没有找到学科特定的提示词，返回错误而不是默认使用数学提示词
            logger.warning(f"未找到学科 '{subject}' 的提示词配置，请确保正确设置学科参数")
            return {"error": f"未找到学科 '{subject}' 的提示词配置，请确保正确设置学科参数"}
        
        # 如果有用户补充内容，加入提示，并增强其优先级
        if user_notes:
            qwen_prompt = f"""
            【用户特别说明】: {user_notes}
            
            请优先根据上述用户说明分析问题。如果用户指出了特定问题（如选择A而不是B的原因），请直接针对用户问题进行分析，这比遵循下面的标准分析框架更重要。
            
            {qwen_prompt}
            """
        
        # 使用通义千问VL模型进行OCR和结构化分析
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {mm_api_key}"
        }
        
        # 构建请求
        request_data = {
            "model": mm_model_name,
            "messages": [
                {"role": "system", "content": get_subject_system_prompt(subject)},  # 使用动态获取的学科提示词
                {
                    "role": "user", 
                    "content": [
                        {"type": "text", "text": qwen_prompt},
                        {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{base64_image}"}}
                    ]
                }
            ],
            "max_tokens": 1000,
            "temperature": 0.2  # 降低温度以获得更确定性的输出
        }
        
        session = create_retry_session()
        logger.info(f"向Qwen API发送OCR和结构化分析请求: {mm_api_url}")
        logger.info(f"Qwen API请求头: {headers}")
   
        response = session.post(mm_api_url, headers=headers, json=request_data, timeout=60)
        
        if response.status_code != 200:
            logger.error(f"Qwen API请求失败，状态码: {response.status_code}")
            return {"error": f"Qwen API请求失败，状态码: {response.status_code}, 响应: {response.text[:200]}"}
        
        # 解析响应
        response_data = response.json()
        if "choices" in response_data and len(response_data["choices"]) > 0:
            result_text = response_data["choices"][0]["message"]["content"]
            logger.info(f"Qwen提取的文本和结构: {safe_text_logging(result_text, 100)}...")
            
            # 尝试解析JSON
            try:
                analysis_result = json.loads(result_text)
                logger.info("成功解析Qwen返回的JSON结构")
            except json.JSONDecodeError:
                # 查找JSON部分
                json_start = result_text.find('{')
                json_end = result_text.rfind('}') + 1
                
                if json_start >= 0 and json_end > json_start:
                    try:
                        json_str = result_text[json_start:json_end]
                        analysis_result = json.loads(json_str)
                        logger.info("成功从文本中提取并解析JSON结构")
                    except:
                        logger.error("无法解析提取的JSON字符串")
                        # 创建基本结构
                        analysis_result = {
                            "题目原文": extract_field_from_text(result_text, "题目原文") or result_text[:200],
                            "题目类型": extract_field_from_text(result_text, "题目类型") or "高等数学",
                            "具体分支": extract_field_from_text(result_text, "具体分支") or "未知",
                            "错误类型": extract_field_from_text(result_text, "错误类型") or "未知",
                            "难度评估": extract_number_from_text(result_text, "难度评估") or 3,
                            "知识点标签": extract_tags_from_text(result_text) or ["数学", "考研"]
                        }
                else:
                    logger.error("无法从Qwen响应中找到JSON结构")
                    # 创建基本结构
                    analysis_result = {
                        "题目原文": result_text[:200],
                        "题目类型": "高等数学",
                        "具体分支": "未知",
                        "错误类型": "未知",
                        "难度评估": 3,
                        "知识点标签": ["数学", "考研"]
                    }
        else:
            logger.error("Qwen响应格式不符合预期")
            return {"error": "Qwen响应格式不符合预期"}
            
    except Exception as e:
        error_msg = f"调用Qwen模型失败: {str(e)}"
        logger.error(f"{error_msg}\n{traceback.format_exc()}")
        return {"error": error_msg}
    
    # 第三步：使用DeepseekR1进行分析
    try:
        # 从Qwen分析提取题目和基本信息
        problem_text = analysis_result.get("题目原文", "")
        problem_type = analysis_result.get("题目类型", "未知")
        problem_branch = analysis_result.get("具体分支", "未知")
        
        # 构建DeepseekR1分析提示，基于学科配置和用户补充信息
        # 获取该学科的分析提示词
        subject_prompt = SUBJECT_ANALYSIS_PROMPTS.get(subject, {}).get("full_prompt", "")
        if not subject_prompt:
            # 如果找不到指定学科的提示词，返回错误
            logger.warning(f"未找到学科 '{subject}' 的提示词配置，请确保正确设置学科参数")
            return {"error": f"未找到学科 '{subject}' 的提示词配置，请确保正确设置学科参数"}
            
        # 构建基础提示词
        deepseek_prompt = f"""
        我需要帮助分析一道{SUBJECTS.get(subject, {}).get('name', '学科')}错题，详细解释其中的错误并提供思路指导。
        （给你的信息中可能有题目的答案详解但也可能没有）
        题目信息:
        ---
        题目: {problem_text}
        题目类型: {problem_type}
        具体分支: {problem_branch}
        """
        
        # 加入用户补充信息，并强化其优先级
        if user_notes:
            deepseek_prompt = f"""
            {deepseek_prompt}
            
            【用户特别说明】: {user_notes}
            
            请注意：上述用户补充信息非常重要，请优先根据用户说明分析问题。如果用户提出了具体问题（如比较选项A和B的差异），请直接针对用户问题给出详细分析。
            """
        else:
            deepseek_prompt += """
            ---
            """

        deepseek_prompt += """
        请注意，你输出的信息中，markdown与latex务必使用正确语法。
        请提供以下分析，并使用LaTeX格式表示所有数学公式（使用$和$$分隔符）:
        """
        
        # 添加来自配置的学科特定提示
        deepseek_prompt += subject_prompt

        # 直接使用备选API
        logger.info("使用stream_analysis_api实现进行分析")
        
        # 添加一个标记，表明这个分析结果应该由stream_analysis_api处理
        analysis_result["stream_analysis"] = True
        
        # 这里不再将error_analysis设为空字符串
        # 而是让stream_analysis_api在前端填充内容，并在save时保存到数据库
        
        # 将错误分析添加到结果中 - 确保是字符串类型
        # 注意：实际内容会由stream_analysis_api给前端，前端save时会包含完整内容
        analysis_result["错误分析"] = "" # 此处故意留空，前端会从流式分析获取完整内容
        analysis_result["提取模型"] = multimodal_model
            
        return analysis_result
            
    except Exception as e:
        error_message = f"DeepseekR1分析失败: {str(e)}"
        logger.error(f"{error_message}")
        
        # 即使DeepseekR1分析失败，仍然返回Qwen的分析结果
        analysis_result["错误分析"] = f"无法获取详细分析: {str(e)}"
        analysis_result["提取模型"] = multimodal_model
        
        return analysis_result

def try_alternative_apis(processing_function, *args, **kwargs):
    """尝试使用替代API执行处理函数
    
    参数:
        processing_function: 可以是处理函数或者字符串提示词
        *args, **kwargs: 传递给处理函数的参数
    """
    
    try:
        from config import API_CONFIG
        
        all_apis = []  # 收集所有API配置
        main_api_id = None  # 主API ID
        results = []  # 存储所有尝试结果
        
        # 检查第一个参数是否为字符串
        is_prompt_string = isinstance(processing_function, str)
        if is_prompt_string:
            prompt = processing_function
            logger.info(f"检测到字符串提示词，长度为: {len(prompt)}")
        else:
            logger.info("检测到处理函数")
            
        logger.info("开始尝试使用替代API")
        
        # 1. 确定主API ID
        if API_CONFIG and 'api_alternatives' in API_CONFIG and 'deepseek' in API_CONFIG['api_alternatives']:
            deepseek_config = API_CONFIG['api_alternatives']['deepseek']
            
            # 检查main_api_id是否在deepseek配置中
            if isinstance(deepseek_config, dict) and 'main_api_id' in deepseek_config:
                main_api_id = deepseek_config['main_api_id']
                logger.info(f"主API ID (在deepseek中): {main_api_id}")
            
            # 2. 收集所有API
            # 处理对象格式
            if isinstance(deepseek_config, dict):
                for api_id, api_config in deepseek_config.items():
                    if api_id != 'main_api_id' and isinstance(api_config, dict):
                        # 标准化字段
                        api_key = api_config.get('api_key', api_config.get('key', ''))
                        if api_key:
                            priority = api_config.get('priority', 999)
                            all_apis.append({
                                'id': api_id,
                                'config': api_config,
                                'priority': priority,
                                'is_main': (api_id == main_api_id)
                            })
                            logger.info(f"找到替代API: {api_config.get('name', api_id)} (ID: {api_id}, 优先级: {priority})")
            # 处理数组格式
            elif isinstance(deepseek_config, list):
                for i, api_config in enumerate(deepseek_config):
                    if isinstance(api_config, dict):
                        api_id = api_config.get('id', api_config.get('original_id', f'api_{i+1}'))
                        api_key = api_config.get('api_key', api_config.get('key', ''))
                        if api_key:
                            priority = api_config.get('priority', i+1)
                            all_apis.append({
                                'id': api_id,
                                'config': api_config,
                                'priority': priority,
                                'is_main': (api_id == main_api_id)
                            })
                            logger.info(f"找到数组API: {api_config.get('name', f'API {i+1}')} (ID: {api_id}, 优先级: {priority})")
        
        # 3. 添加主API配置
        if API_CONFIG and 'deepseek' in API_CONFIG and isinstance(API_CONFIG['deepseek'], dict):
            main_config = API_CONFIG['deepseek']
            api_key = main_config.get('api_key', main_config.get('key', ''))
            
            if api_key:
                api_id = main_config.get('original_id', 'main')
                priority = main_config.get('priority', 1)  # 主API默认最高优先级
                all_apis.append({
                    'id': api_id,
                    'config': main_config,
                    'priority': priority,
                    'is_main': True
                })
                logger.info(f"添加主API: {main_config.get('name', '主DeepSeek API')} (ID: {api_id}, 优先级: {priority})")
        
        # 4. 按优先级排序
        all_apis.sort(key=lambda x: x['priority'])
        
        logger.info(f"按优先级排序后的API列表:")
        for api in all_apis:
            logger.info(f"- {api['config'].get('name', api['id'])} (优先级: {api['priority']})")
        
        # 5. 尝试每个API
        for api in all_apis:
            api_config = api['config']
            api_name = api_config.get('name', api['id'])
            
            logger.info(f"尝试使用API: {api_name} (优先级: {api['priority']})")
            
            try:
                # 根据参数类型执行不同的处理
                if is_prompt_string:
                    # 如果是字符串提示词，直接发送API请求
                    result = _process_prompt_with_api(prompt, api_config)
                else:
                    # 如果是处理函数，调用_try_single_alternative_api
                    result = _try_single_alternative_api(
                        processing_function,
                        api_config,
                        *args,
                        **kwargs
                    )
                
                # 如果成功，记录并返回结果
                if result.get('success', False):
                    logger.info(f"API {api_name} 调用成功")
                    result['api_name'] = api_name
                    return result.get('content', result)  # 返回内容或完整结果
                else:
                    # 记录失败
                    error = result.get('error', '未知错误')
                    logger.warning(f"API {api_name} 调用失败: {error}")
                    results.append(result)
            except Exception as e:
                logger.error(f"尝试API {api_name} 时出错: {str(e)}")
                results.append({
                    'success': False,
                    'error': str(e),
                    'api_name': api_name
                })
        
        # 所有API都失败，返回最后一个结果
        if results:
            logger.error("所有API尝试均失败")
            return results[-1].get('content', results[-1])  # 返回内容或完整结果
        else:
            logger.error("没有找到可用的API配置")
            return "无法获取分析内容：没有找到可用的API配置"
    except Exception as e:
        logger.error(f"尝试替代API时发生错误: {str(e)}")
        return f"分析失败: {str(e)}"
        
def _process_prompt_with_api(prompt, api_config):
    """使用API处理提示词并返回结果
    
    参数:
        prompt: 字符串提示词
        api_config: API配置字典
    
    返回:
        包含处理结果的字典
    """
    try:
        # 标准化字段名称
        api_key = api_config.get('api_key', api_config.get('key', ''))
        api_url = api_config.get('api_url', api_config.get('url', ''))
        model_name = api_config.get('model', api_config.get('model_name', 'deepseek-chat'))
        name = api_config.get('name', 'DeepSeek API')
        
        # 检查必要字段
        if not api_key or not api_url:
            logger.error(f"API {name} 缺少API密钥或URL")
            return {'success': False, 'error': 'API密钥或URL缺失'}
        
        # 构建请求头
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
        
        # 构建请求体
        request_data = {
            "model": model_name,
            "messages": [
                {"role": "system", "content": get_subject_system_prompt("math")},  # 使用动态获取的学科提示词
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 2000,
            "stream": True  # 启用流式输出
        }
        
        # 发送请求
        logger.info(f"向API发送请求: {api_url}, 模型: {model_name}")
        logger.info(f"API请求头: {headers}")
        logger.info(f"API请求详细内容: {json.dumps(request_data, ensure_ascii=False)}")
        
        response = requests.post(
            api_url,
            headers=headers,
            json=request_data,
            timeout=90,
            stream=True  # 启用流式响应
        )
        
        # 处理响应
        if response.status_code == 200:
            content = ""
            for line in response.iter_lines():
                if line:
                    line_str = line.decode('utf-8')
                    if line_str.startswith('data: '):
                        data_str = line_str[6:]  # 去掉 'data: ' 前缀
                        
                        if data_str == '[DONE]':
                            break
                        
                        try:
                            data = json.loads(data_str)
                            if 'choices' in data and len(data['choices']) > 0:
                                choice = data['choices'][0]
                                if 'delta' in choice and 'content' in choice['delta']:
                                    content_delta = choice['delta']['content']
                                    if content_delta is not None:  # 添加对None值的检查
                                        content += content_delta
                        except json.JSONDecodeError:
                            logger.warning(f"无法解析流式响应行: {data_str[:50]}...")
            
            logger.info(f"流式响应完成，共接收 {len(content)} 字符")
            return {
                'success': True,
                'content': content
            }
        else:
            logger.error(f"API请求失败，状态码: {response.status_code}")
            return {
                'success': False, 
                'error': f'API请求失败，状态码: {response.status_code}'
            }
    except Exception as e:
        logger.error(f"处理提示词时出错: {str(e)}")
        return {'success': False, 'error': str(e)}

def _try_single_alternative_api(processing_function, api_config, *args, **kwargs):
    """使用单个替代API执行处理函数"""
    try:
        # 标准化字段名称
        api_key = api_config.get('api_key', api_config.get('key', ''))
        api_url = api_config.get('api_url', api_config.get('url', ''))
        model = api_config.get('model', api_config.get('model_name', 'deepseek-chat'))
        name = api_config.get('name', 'DeepSeek API')
        
        # 检查必要字段
        if not api_key:
            logger.error(f"API {name} 缺少API密钥")
            return {'success': False, 'error': 'API密钥缺失', 'api_name': name}
        
        # 记录API请求
        logger.info(f"尝试使用API: {name}, URL: {api_url}, 模型: {model}")
        
        # 构造包含API配置的kwargs
        api_kwargs = kwargs.copy()
        api_kwargs['api_config'] = {
            'api_key': api_key,
            'api_url': api_url,
            'model': model,
            'model_name': model  # 确保同时包含model和model_name字段，且值保持一致
        }
        
        # 调用处理函数
        result = processing_function(*args, **api_kwargs)
        
        # 添加API信息到结果
        if isinstance(result, dict):
            result['api_name'] = name
            result['success'] = True
            
        return result
    except Exception as e:
        logger.error(f"单个替代API尝试时出错: {str(e)}")
        return {'success': False, 'error': str(e)}

# 其他函数保持不变
def analyze_with_ocr_and_deepseek(image_path, user_notes='', subject='math'):
    """使用阿里云OCR结合DeepSeekR1分析错题图片"""
    logger.info(f"使用阿里云OCR+DeepseekR1组合模式进行图像分析，学科: {subject}")
    
    # 基本参数检查
    if not os.path.exists(image_path):
        error_msg = f"图片文件不存在: {image_path}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 获取 API 配置
    try:
        # 检查aliyun_ocr配置是否存在
        if "aliyun_ocr" not in API_CONFIG:
            error_msg = "未找到aliyun_ocr配置"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查aliyun_ocr配置是否完整
        ocr_config = API_CONFIG["aliyun_ocr"]
        ocr_required_fields = ["access_key_id", "access_key_secret", "region_id"]
        
        if not all(field in ocr_config for field in ocr_required_fields):
            error_msg = "aliyun_ocr配置不完整，缺少必要字段"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查access_key_id和access_key_secret是否为空
        if not ocr_config["access_key_id"] or not ocr_config["access_key_secret"]:
            error_msg = "阿里云OCR的access_key_id或access_key_secret为空"
            logger.error(error_msg)
            return {"error": error_msg}
        
        # 获取DeepseekR1配置 - 使用新的辅助函数
        r1_config = get_main_deepseek_api()
        
        # 验证DeepseekR1配置的有效性
        if not r1_config["api_key"] or not r1_config["api_url"]:
            logger.error("DeepseekR1 API密钥或URL为空")
            return {"error": "DeepseekR1 API密钥或URL为空"}
            
        # 获取DeepseekR1配置
        r1_api_key = r1_config["api_key"]
        r1_api_url = r1_config["api_url"]
        r1_model_name = r1_config.get("model_name", r1_config.get("model", "deepseek-chat"))
        
        logger.info("已获取阿里云OCR和DeepseekR1配置")
    except KeyError as e:
        error_msg = f"API 配置缺失: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 第一步：读取图片文件并准备OCR
    try:
        # 读取图片文件
        with open(image_path, "rb") as image_file:
            image_bytes = image_file.read()
        
        base64_image = base64.b64encode(image_bytes).decode('utf-8')
        logger.info(f"成功读取图片: {len(image_bytes)} 字节")
        
    except Exception as e:
        error_msg = f"图片读取错误: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 第二步：调用阿里云OCR API
    try:
        # 动态导入阿里云OCR SDK
        try:
            from alibabacloud_ocr20191230.client import Client as OcrClient
            from alibabacloud_tea_openapi import models as open_api_models
            from alibabacloud_ocr20191230 import models as ocr_models
            from alibabacloud_tea_util import models as util_models
            logger.info("成功导入阿里云OCR SDK")
        except ImportError:
            logger.error("未安装阿里云OCR SDK，尝试使用通用HTTP请求方式")
            # 使用通用HTTP请求方式代替
            ocr_endpoint = f"https://ocr.{region_id}.aliyuncs.com"
            ocr_api = "/api/v1/ocr/general"
            
            # 准备请求头和请求体
            headers = {
                "Content-Type": "application/json",
                "Authorization": f"APPCODE {access_key_id}"  # 阿里云市场购买的OCR服务使用APPCODE认证
            }
            
            payload = {
                "image": base64_image,
                "configure": "{\"language\":\"zh\"}"
            }
            
            # 发送请求
            ocr_response = requests.post(
                f"{ocr_endpoint}{ocr_api}", 
                headers=headers, 
                json=payload, 
                timeout=30
            )
            
            if ocr_response.status_code != 200:
                logger.error(f"阿里云OCR请求失败，状态码: {ocr_response.status_code}")
                return {"error": f"阿里云OCR请求失败，状态码: {ocr_response.status_code}"}
            
            ocr_result = ocr_response.json()
            extracted_text = ""
            
            # 提取文本
            if "prism_wordsInfo" in ocr_result:
                for word_info in ocr_result["prism_wordsInfo"]:
                    if "word" in word_info:
                        extracted_text += word_info["word"] + " "
            
            logger.info(f"OCR提取的文本: {extracted_text[:100]}...")
        
        # 如果SDK导入成功，使用SDK方式调用
        else:
            # 创建客户端配置
            config = open_api_models.Config(
                access_key_id=access_key_id,
                access_key_secret=access_key_secret
            )
            config.endpoint = f'ocr.{region_id}.aliyuncs.com'
            
            # 创建OCR客户端
            client = OcrClient(config)
            
            # 创建识别请求
            request = ocr_models.RecognizeGeneralRequest(
                body=image_bytes
            )
            
            # 发送请求
            runtime = util_models.RuntimeOptions()
            response = client.recognize_general_with_options(request, runtime)
            
            # 提取文本内容
            extracted_text = ""
            if response.body.data and response.body.data.blocks:
                for block in response.body.data.blocks:
                    extracted_text += block.text + " "
            
            logger.info(f"OCR提取的文本: {extracted_text[:100]}...")
        
        # 检查是否成功提取文本
        if not extracted_text:
            logger.error("OCR未能提取任何文本")
            return {"error": "OCR未能提取任何文本"}
            
    except Exception as e:
        error_msg = f"调用阿里云OCR失败: {str(e)}"
        logger.error(f"{error_msg}\n{traceback.format_exc()}")
        # 构建一个模拟的提取结果，用于调试
        extracted_text = "OCR提取失败，无法获得文本内容。请检查阿里云OCR配置。"
    
    # 第三步：调用DeepSeek API进行分析
    try:
        prompt = f"""
        我给你提供一张数学错题的OCR提取内容，请分析这道题目并提供详细的解答和错误分析。
        
        ## 题目数据
        1. OCR提取内容：
        {extracted_text}
        
        2. 用户补充说明：
        {user_notes if user_notes else '无'}
        
        ## 分析任务
        请按照以下格式提供数学错题的详细分析：
        
        1. **题目原文**：重新组织和整理OCR文本，明确题目内容
        2. **题目类型**：确定题目所属的主要数学分类
        3. **具体分支**：题目的具体数学分支或子类型
        4. **错误类型**：推测学生常见的错误类型和错误原因
        5. **错误分析**：详细分析错误的本质、概念误解和解题盲点
        6. **正确解法**：提供清晰、详细的正确解题步骤
        7. **难度评估**：按1-5级评估题目难度并简要说明理由
        8. **知识点标签**：列出题目涉及的主要知识点，格式为标签数组
        
        注意：如果OCR提取的内容不完整或有明显错误，请尽量根据上下文和专业知识进行合理推测。
        """
        
        try:
            # 调用DeepSeek API进行分析
            logger.info("向DeepseekR1发送OCR分析请求")
            logger.info(f"DeepseekR1 API请求头: {headers}")
            logger.info(f"DeepseekR1 API请求详细内容: {json.dumps(request_data, ensure_ascii=False)}")
            
            # 定义请求头
            headers = {
                "Content-Type": "application/json",
                "Authorization": f"Bearer {r1_api_key}"
            }
            
            # 请求体
            request_data = {
                "model": r1_model_name,
                "messages": [
                    {"role": "system", "content": get_subject_system_prompt(subject)},  # 使用动态获取的学科提示词
                    {"role": "user", "content": prompt}
                ],
                "temperature": 0.1,
                "max_tokens": 4000,
                "stream": True  # 启用流式输出
            }
            
            # 发送请求
            logger.info(f"向DeepseekR1 API发送OCR流式分析请求: {r1_api_url}")
            
            try:
                response = requests.post(
                    r1_api_url,
                    headers=headers,
                    json=request_data,
                    timeout=90,  # 较长的超时时间
                    stream=True  # 启用流式响应
                )
                
                if response.status_code == 200:
                    content = ""
                    for line in response.iter_lines():
                        if line:
                            line_str = line.decode('utf-8')
                            if line_str.startswith('data: '):
                                data_str = line_str[6:]  # 去掉 'data: ' 前缀
                                
                                if data_str == '[DONE]':
                                    break
                                
                                try:
                                    data = json.loads(data_str)
                                    if 'choices' in data and len(data['choices']) > 0:
                                        choice = data['choices'][0]
                                        if 'delta' in choice and 'content' in choice['delta']:
                                            content_delta = choice['delta']['content']
                                            if content_delta is not None:  # 添加对None值的检查
                                                content += content_delta
                                except json.JSONDecodeError:
                                    logger.warning(f"无法解析流式响应行: {data_str[:50]}...")
                    
                    logger.info(f"流式响应完成，共接收 {len(content)} 字符")
                    analysis_text = content
                else:
                    logger.error(f"DeepseekR1 OCR分析请求失败，状态码: {response.status_code}")
                    # 尝试获取错误消息
                    try:
                        error_content = response.text
                        logger.error(f"错误内容: {error_content[:200]}...")
                    except:
                        error_content = "无法获取错误内容"
                    
                    analysis_text = f"分析失败：API请求返回状态码 {response.status_code}，请稍后重试。"
            except requests.exceptions.Timeout:
                logger.error("DeepseekR1 OCR分析请求超时")
                analysis_text = "分析超时：请求处理时间过长，请稍后重试。"
            except Exception as e:
                logger.error(f"DeepseekR1 OCR分析请求异常: {str(e)}")
                analysis_text = f"分析异常：{str(e)}，请稍后重试。"
        except Exception as e:
            # 记录其他异常
            logger.error(f"调用DeepseekR1分析OCR内容时发生异常: {str(e)}")
            analysis_text = f"处理异常：{str(e)}，请稍后重试。"
            
        # 使用正则表达式从文本中提取结构化数据
        try:
            # 提取题目原文
            problem_text_match = re.search(r'[*]*题目原文[*]*[:：]([\s\S]+?)(?=[*]*题目类型[*]*[:：]|$)', analysis_text)
            problem_text = problem_text_match.group(1).strip() if problem_text_match else extracted_text
            
            # 提取题目类型
            problem_type_match = re.search(r'[*]*题目类型[*]*[:：]([\s\S]+?)(?=[*]*具体分支[*]*[:：]|$)', analysis_text)
            problem_type = problem_type_match.group(1).strip() if problem_type_match else "未知"
            
            # 提取具体分支
            problem_branch_match = re.search(r'[*]*具体分支[*]*[:：]([\s\S]+?)(?=[*]*错误类型[*]*[:：]|$)', analysis_text)
            problem_branch = problem_branch_match.group(1).strip() if problem_branch_match else "未知"
            
            # 提取错误类型
            error_type_match = re.search(r'[*]*错误类型[*]*[:：]([\s\S]+?)(?=[*]*错误分析[*]*[:：]|$)', analysis_text)
            error_type = error_type_match.group(1).strip() if error_type_match else "未知"
            
            # 提取错误分析
            error_analysis_match = re.search(r'[*]*错误分析[*]*[:：]([\s\S]+?)(?=[*]*正确解法[*]*[:：]|$)', analysis_text)
            error_analysis = error_analysis_match.group(1).strip() if error_analysis_match else analysis_text
            
            # 提取正确解法
            correct_solution_match = re.search(r'[*]*正确解法[*]*[:：]([\s\S]+?)(?=[*]*难度评估[*]*[:：]|$)', analysis_text)
            correct_solution = correct_solution_match.group(1).strip() if correct_solution_match else ""
            
            # 提取难度评估
            difficulty_match = re.search(r'[*]*难度评估[*]*[:：]([\s\S]+?)(?=[*]*知识点标签[*]*[:：]|$)', analysis_text)
            difficulty_text = difficulty_match.group(1).strip() if difficulty_match else "3"
            # 从难度文本中提取数字
            difficulty_number = re.search(r'(\d+)', difficulty_text)
            difficulty = int(difficulty_number.group(1)) if difficulty_number else 3
            
            # 提取知识点标签
            tags_match = re.search(r'[*]*知识点标签[*]*[:：]([\s\S]+?)$', analysis_text)
            tags_text = tags_match.group(1).strip() if tags_match else ""
            # 标签可能是以各种格式列出的，尝试不同的分割方式
            if '、' in tags_text:
                tags = [tag.strip() for tag in tags_text.split('、')]
            elif '，' in tags_text:
                tags = [tag.strip() for tag in tags_text.split('，')]
            elif ',' in tags_text:
                tags = [tag.strip() for tag in tags_text.split(',')]
            elif '\n' in tags_text:
                tags = [tag.strip() for tag in tags_text.split('\n') if tag.strip()]
            else:
                tags = [tags_text] if tags_text else ["未知"]
            
            # 过滤空标签
            tags = [tag for tag in tags if tag]
            if not tags:
                tags = ["未知"]
            
            # 构建结果字典
            analysis_result = {
                "题目原文": problem_text,
                "题目类型": problem_type or "未知类型",  # 确保题目类型字段有值
                "具体分支": problem_branch or "未知分支",
                "错误类型": error_type or "未知错误类型",
                "错误分析": error_analysis or "无法获取分析",
                "正确解法": correct_solution or "无正确解法提供",
                "难度评估": difficulty or 3,
                "知识点标签": tags
            }
            
            return analysis_result
            
        except Exception as e:
            logger.error(f"解析DeepseekR1响应时出错: {str(e)}")
            
            # 返回基本结构
            return {
                "题目原文": "无法解析",
                "题目类型": "未知类型",  # 确保题目类型字段有值
                "具体分支": "未知分支", 
                "错误类型": "未知错误类型",
                "错误分析": f"分析处理过程中出错: {str(e)}",
                "正确解法": "无法提供",
                "难度评估": 3,
                "知识点标签": ["未知"]
            }
    except Exception as e:
        logger.error(f"OCR+DeepSeekR1分析整体过程出错: {str(e)}")
        logger.error(traceback.format_exc())
        
        # 返回带有错误信息的结果
        return {
            "题目原文": extracted_text[:200] + "..." if extracted_text else "无法提取文本",
            "题目类型": "未知类型",  # 确保题目类型字段有值
            "具体分支": "未知",
            "错误类型": "API错误",
            "错误分析": f"分析过程出错: {str(e)}，请稍后重试。",
            "正确解法": "",
            "难度评估": 3,
            "知识点标签": ["未知"]
        }

def analyze_with_deepseek(prompt, subject="math", model_name="deepseek-chat"):
    """使用DeepSeek API分析错题"""
    logger.info(f"使用DeepSeek API进行分析, 学科: {subject}, 模型: {model_name}")
    
    # 初始化默认结果字典 - 确保所有必需字段都有默认值
    default_result = {
        "题目类型": "未分类题型",
        "具体分支": "未知分支",
        "错误类型": "未知错误",
        "题目原文": "",
        "错误分析": "无法生成分析",
        "正确解法": "无法提供解法",
        "难度评估": 3,
        "知识点标签": []
    }
    
    try:
        # 获取API配置
        api_key = os.environ.get("DEEPSEEK_API_KEY") or API_CONFIG.get("deepseek_api_key", "")
        if not api_key:
            logger.error("未配置DeepSeek API Key")
            default_result["错误分析"] = "未配置DeepSeek API Key"
            return default_result
            
        api_url = API_CONFIG.get("deepseek_api_url", "https://api.deepseek.com/v1/chat/completions")

        # 设置请求头和数据
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
        
        data = {
            "model": model_name,
            "messages": [
                {"role": "system", "content": get_subject_system_prompt(subject)},  # 使用动态获取的学科提示词
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 2000
        }
        
        # 发送请求
        logger.info(f"向DeepSeek API发送请求")
        logger.info(f"DeepSeek API请求头: {headers}")
        logger.info(f"DeepSeek API请求详细内容: {json.dumps(data, ensure_ascii=False)}")
        
        response = requests.post(api_url, headers=headers, json=data, timeout=60)
        
        # 检查响应状态
        if response.status_code != 200:
            logger.error(f"DeepSeek API请求失败, 状态码: {response.status_code}")
            error_msg = f"API请求失败 ({response.status_code})"
            try:
                error_json = response.json()
                if "error" in error_json:
                    error_msg += f": {error_json['error']}"
            except:
                error_msg += f": {response.text[:100]}"
                
            default_result["错误分析"] = error_msg
            return default_result
            
        # 解析响应
        resp_json = response.json()
        if "choices" not in resp_json or len(resp_json["choices"]) == 0:
            logger.error("DeepSeek API响应格式错误，没有找到choices字段")
            default_result["错误分析"] = "API响应格式错误"
            return default_result
            
        content = resp_json["choices"][0].get("message", {}).get("content", "")
        if not content:
            logger.error("DeepSeek API响应内容为空")
            default_result["错误分析"] = "API响应内容为空"
            return default_result
            
        # 解析内容，尝试提取结构化信息
        try:
            # 提取JSON部分
            json_match = re.search(r'```json\s*([\s\S]*?)\s*```', content)
            if json_match:
                json_str = json_match.group(1).strip()
                parsed_result = json.loads(json_str)
                
                # 将解析出的结果与默认结果合并，确保所有字段都存在
                result = {**default_result, **parsed_result}
                
                # 验证并设置必要字段的默认值（如果缺失）
                for key in default_result:
                    if key not in result or result[key] is None or result[key] == "":
                        result[key] = default_result[key]
                        
                return result
                
            # 如果没有JSON格式，尝试从文本中提取结构化信息
            logger.info("未找到JSON格式响应，尝试从文本提取结构化信息")
            
            # 使用正则表达式提取字段
            result = dict(default_result)  # 复制默认结果
            
            # 提取题目类型
            type_match = re.search(r'题目类型[：:]\s*(.+?)[\n\r]', content)
            if type_match:
                result["题目类型"] = type_match.group(1).strip()
                
            # 提取具体分支
            branch_match = re.search(r'具体分支[：:]\s*(.+?)[\n\r]', content)
            if branch_match:
                result["具体分支"] = branch_match.group(1).strip()
                
            # 提取错误类型
            error_match = re.search(r'错误类型[：:]\s*(.+?)[\n\r]', content)
            if error_match:
                result["错误类型"] = error_match.group(1).strip()
                
            # 提取错误分析
            analysis_match = re.search(r'错误分析[：:]\s*([^#]+?)(?=\n\s*#|\Z)', content, re.DOTALL)
            if analysis_match:
                result["错误分析"] = analysis_match.group(1).strip()
                
            # 提取正确解法
            solution_match = re.search(r'正确解法[：:]\s*([^#]+?)(?=\n\s*#|\Z)', content, re.DOTALL)
            if solution_match:
                result["正确解法"] = solution_match.group(1).strip()
                
            # 确保所有必需字段都有值
            for key in default_result:
                if key not in result or result[key] is None or result[key] == "":
                    result[key] = default_result[key]
                
            return result
                
        except json.JSONDecodeError as e:
            logger.error(f"解析JSON内容失败: {str(e)}")
            default_result["错误分析"] = f"无法解析API响应: {str(e)}"
            return default_result
            
        except Exception as e:
            logger.error(f"提取结构化信息失败: {str(e)}")
            default_result["错误分析"] = f"提取结构化信息失败: {str(e)}"
            return default_result
            
    except requests.exceptions.Timeout:
        logger.error("DeepSeek API请求超时")
        default_result["错误分析"] = "API请求超时"
        return default_result
        
    except Exception as e:
        logger.error(f"DeepSeek API分析出错: {str(e)}")
        logger.debug(traceback.format_exc())
        default_result["错误分析"] = f"分析过程出错: {str(e)}"
        return default_result

def get_subject_system_prompt(subject_name=None):
    """获取特定学科的系统提示词
    
    Args:
        subject_name: 学科名称，例如'math', 'english'等
        
    Returns:
        str: 学科特定的系统提示词
    """
    import logging
    logger = logging.getLogger(__name__)
    
    # 添加日志记录收到的学科参数
    logger.info(f"获取学科提示词，传入学科名称: '{subject_name}'")
    
    # 标准化学科名称为小写
    if subject_name:
        subject_name = subject_name.lower()
    
    # 确保配置已加载
    if not SUBJECT_ANALYSIS_PROMPTS:
        logger.warning("学科提示词配置为空，尝试加载")
        load_prompts_from_file()
    
    # 添加日志记录当前配置的所有学科
    available_subjects = list(SUBJECT_ANALYSIS_PROMPTS.keys())
    logger.info(f"当前配置的学科列表: {available_subjects}")
    
    # 检查学科是否存在
    if subject_name in SUBJECT_ANALYSIS_PROMPTS:
        prompt = SUBJECT_ANALYSIS_PROMPTS[subject_name].get('teacher_prompt', '')
        logger.info(f"找到学科 '{subject_name}' 的提示词: {prompt[:50]}...")
        return prompt
    else:
        # 如果找不到指定学科，返回默认提示词（数学）
        default_subject = 'math'
        logger.warning(f"未找到学科 '{subject_name}' 的提示词，使用默认学科 '{default_subject}'")
        if default_subject in SUBJECT_ANALYSIS_PROMPTS:
            prompt = SUBJECT_ANALYSIS_PROMPTS[default_subject].get('teacher_prompt', '')
            return prompt
        else:
            # 兜底返回通用提示词
            logger.error(f"未找到默认学科 '{default_subject}' 的提示词，使用通用提示词")
            return "你是一位专业的教师，请分析学生提交的问题并给出专业建议。"

def get_subject_analysis_template(subject_name=None):
    """获取特定学科的分析模板
    
    Args:
        subject_name: 学科名称，例如'math', 'english'等
        
    Returns:
        str: 学科特定的分析模板
    """
    import logging
    logger = logging.getLogger(__name__)
    
    # 添加日志记录收到的学科参数
    logger.info(f"获取学科分析模板，传入学科名称: '{subject_name}'")
    
    # 标准化学科名称为小写
    if subject_name:
        subject_name = subject_name.lower()
    
    # 确保配置已加载
    if not SUBJECT_ANALYSIS_PROMPTS:
        logger.warning("学科提示词配置为空，尝试加载")
        load_prompts_from_file()
    
    # 通用分析模板
    default_template = """
请提供以下三个部分的分析:
1. 综合分析: 根据错题集合，分析这些错题反映出的整体学习情况、知识掌握程度和思维特点，尤其关注错题中反映出的系统性问题。

2. 错题模式识别: 识别这组错题中出现的模式和规律，包括:
   - 知识点理解方面的共性问题
   - 解题思路和方法上的常见缺陷
   - 计算和推理过程中的典型错误
   - 概念混淆或应用不当的情况

3. 针对性学习建议: 基于以上分析，提供实用的学习策略和针对性建议，包括:
   - 如何系统性地弥补知识漏洞
   - 改进思维方法的具体建议
   - 针对常见错误的练习方案
   - 提高解题效率和准确性的技巧
"""
    
    # 检查学科是否存在
    if subject_name in SUBJECT_ANALYSIS_PROMPTS:
        # 尝试获取学科特定的分析模板
        template = SUBJECT_ANALYSIS_PROMPTS[subject_name].get('analysis_template', '')
        if template:
            logger.info(f"找到学科 '{subject_name}' 的分析模板")
            return template
        else:
            logger.info(f"学科 '{subject_name}' 没有设置特定的分析模板，使用通用模板")
            return default_template
    else:
        # 如果找不到指定学科，返回默认模板
        logger.warning(f"未找到学科 '{subject_name}'，使用通用分析模板")
        return default_template

def get_subject_review_prompt(subject_name=None):
    """
    获取指定学科的回顾分析提示词
    
    Args:
        subject_name (str, optional): 学科名称. Defaults to None.
        
    Returns:
        str: 回顾分析提示词
    """
    try:
        # 默认回顾分析提示词
        DEFAULT_REVIEW_PROMPT = """
        我将提供一组错题的内容，包括题目、学生的错误答案、正确答案和分析。请根据这些信息进行深入分析，识别错误模式，评估知识薄弱点，并提供针对性学习建议。请在分析中包含以下内容：
        1. 错误模式概述：分析学生在哪些类型的问题上犯错较多
        2. 知识盲点识别：找出学生掌握不牢固的知识点
        3. 学习建议：提供具体有效的学习方法和练习策略
        4. 进步路径：设计一个短期学习计划，帮助学生克服这些问题
        请以清晰的结构呈现分析结果，帮助学生全面了解自己的学习情况。
        """
        
        if not subject_name:
            return DEFAULT_REVIEW_PROMPT
            
        # 直接从JSON文件读取提示词
        import json
        import os
        
        # 获取JSON文件路径
        current_dir = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
        json_file_path = os.path.join(current_dir, 'data', 'subject_prompts.json')
        
        # 读取JSON文件
        logger.info(f"从 {json_file_path} 加载学科提示词")
        with open(json_file_path, 'r', encoding='utf-8') as f:
            subject_prompts = json.load(f)
        
        # 检查是否存在该学科及其回顾提示词
        if subject_name in subject_prompts and 'review_prompt' in subject_prompts[subject_name]:
            logger.info(f"找到 {subject_name} 的回顾分析提示词")
            return subject_prompts[subject_name]['review_prompt']
        
        # 如果没有找到，返回默认提示词
        logger.warning(f"未找到 {subject_name} 的回顾分析提示词，使用默认提示词")
        return DEFAULT_REVIEW_PROMPT
    except Exception as e:
        logger.error(f"获取回顾分析提示词失败: {str(e)}")
        logger.error(f"错误详情: {traceback.format_exc()}")
        # 确保在任何情况下都返回一个可用的提示词
        DEFAULT_REVIEW_PROMPT = """
        请分析学生的错题模式，找出知识薄弱点，并提供针对性学习建议。包括错误模式概述、知识盲点识别、学习建议和进步路径。
        """
        return DEFAULT_REVIEW_PROMPT