"""
单一模型分析模块 - 包含OpenAI兼容API和Gemini API的分析函数
"""
import os
import json
import base64
import requests
import traceback
import logging
from config import API_CONFIG
from .utils import extract_field_from_text, extract_number_from_text, extract_tags_from_text
from .subject_prompts import get_prompt_for_subject

logger = logging.getLogger(__name__)

def analyze_with_openai_compat_api(image_path, user_notes='', subject='math'):
    """使用OpenAI兼容API分析错题图片"""
    logger.info(f"使用 OpenAI 兼容API进行图像分析，学科: {subject}")
    
    # 基本参数检查
    if not os.path.exists(image_path):
        error_msg = f"图片文件不存在: {image_path}"
        logger.error(error_msg)
        return {"error": error_msg}
        
    # 获取 API 配置
    try:
        # 检查vision_api配置是否存在
        if "vision_api" not in API_CONFIG:
            error_msg = "未找到vision_api配置"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查vision_api配置是否完整
        vision_api_config = API_CONFIG["vision_api"]
        required_fields = ["api_key", "api_url", "model_name"]
        
        if not all(field in vision_api_config for field in required_fields):
            error_msg = "vision_api配置不完整，缺少必要字段"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查api_key和api_url是否为空
        if not vision_api_config["api_key"] or not vision_api_config["api_url"]:
            error_msg = "vision_api的API密钥或URL为空"
            logger.error(error_msg)
            return {"error": error_msg}
            
        api_key = vision_api_config["api_key"]
        api_url = vision_api_config["api_url"]
        model_name = vision_api_config["model_name"]
        
        logger.info(f"使用图像分析 API URL: {api_url}")
        logger.info(f"使用图像分析模型: {model_name}")
    except KeyError as e:
        error_msg = f"API 配置缺失: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 读取图片文件并转为Base64
    try:
        with open(image_path, "rb") as image_file:
            image_bytes = image_file.read()
        
        base64_image = base64.b64encode(image_bytes).decode('utf-8')
        logger.info(f"成功将图片编码为 base64, 大小: {len(base64_image)}")
    except Exception as e:
        error_msg = f"图片编码错误: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 获取针对特定学科的分析提示
    prompt = get_prompt_for_subject(subject, user_notes)
    logger.info(f"使用{subject}学科的分析提示模板")
    
    # 构建OpenAI兼容API请求头
    headers = {
        "Content-Type": "application/json",
        "Authorization": f"Bearer {api_key}"
    }
    
    # OpenAI兼容API的请求体
    request_data = {
        "model": model_name,
        "messages": [
            {
                "role": "user", 
                "content": [
                    {"type": "text", "text": prompt},
                    {"type": "image_url", "image_url": {"url": f"data:image/jpeg;base64,{base64_image}"}}
                ]
            }
        ],
        "temperature": 0.3,
        "max_tokens": 2000
    }
    
    try:
        logger.info(f"正在向OpenAI兼容 API {api_url} 发送请求...")
        
        response = requests.post(api_url, headers=headers, json=request_data, timeout=60)
        logger.info(f"API 响应状态码: {response.status_code}")
        
        # 检查响应状态
        if response.status_code != 200:
            error_text = f"图像分析 API 请求失败，状态码: {response.status_code}"
            try:
                error_json = response.json()
                error_text += f", 错误详情: {json.dumps(error_json)}"
                logger.error(error_text)
                
                # 详细记录请求内容以便调试
                logger.error(f"API请求数据: {json.dumps({k: v if k != 'messages' else '...' for k, v in request_data.items()})}")
                
                # 返回一个包含错误信息的模拟数据
                return {
                    "题目原文": "API请求出错",
                    "题目类型": "高等数学",
                    "具体分支": "API错误",
                    "错误类型": "API配置错误",
                    "错误分析": f"API错误信息: {error_json.get('error', {}).get('message', '未知错误')}",
                    "正确解法": "请修复API配置后重试",
                    "难度评估": 1,
                    "知识点标签": ["API配置", "错误修复"]
                }
            except Exception as e:
                error_text += f", 响应内容: {response.text[:200]}"
                logger.error(error_text)
                logger.error(f"解析错误响应时出错: {str(e)}")
            
            # 返回一个默认的分析结果
            return {
                "题目原文": "示例题目: 已知函数f(x) = ln(x), 求f'(x)。",
                "题目类型": "高等数学",
                "具体分支": "微分学",
                "错误类型": "计算错误",
                "错误分析": "API调用失败，无法获取真实分析。这是一个示例分析。",
                "正确解法": "对于f(x) = ln(x，其导数为f'(x) = 1/x。",
                "难度评估": 2,
                "知识点标签": ["导数", "自然对数", "微分学"]
            }
        
        # 成功获取响应
        response_data = response.json()
        logger.info("成功收到 API 响应")
        
        # 解析 OpenAI 兼容格式的响应
        try:
            if "choices" in response_data and len(response_data["choices"]) > 0:
                result_text = response_data["choices"][0]["message"]["content"]
                logger.info(f"提取的文本: {result_text[:100]}...")
                
                # 尝试解析 JSON
                try:
                    # 直接解析整个文本
                    analysis = json.loads(result_text)
                    return analysis
                except json.JSONDecodeError:
                    # 尝试查找并提取 JSON 部分
                    json_start = result_text.find('{')
                    json_end = result_text.rfind('}') + 1
                    
                    if (json_start >= 0 and json_end > json_start):
                        try:
                            json_str = result_text[json_start:json_end]
                            analysis = json.loads(json_str)
                            return analysis
                        except:
                            logger.error("无法解析提取的 JSON 字符串")
                    
                    # 构建结构化数据
                    return {
                        "题目原文": extract_field_from_text(result_text, "题目原文") or result_text[:200],
                        "题目类型": extract_field_from_text(result_text, "题目类型") or "高等数学",
                        "具体分支": extract_field_from_text(result_text, "具体分支") or "未知",
                        "错误类型": extract_field_from_text(result_text, "错误类型") or "未知",
                        "错误分析": extract_field_from_text(result_text, "错误分析") or result_text[200:400],
                        "正确解法": extract_field_from_text(result_text, "正确解法") or "请查看完整分析",
                        "难度评估": extract_number_from_text(result_text, "难度评估") or 3,
                        "知识点标签": extract_tags_from_text(result_text) or ["数学", "考研"]
                    }
            else:
                logger.warning("API响应格式不符合预期")
                logger.debug(f"完整响应: {json.dumps(response_data)}")
                
                # 尝试处理非标准响应格式
                if "output" in response_data and isinstance(response_data["output"], dict) and "text" in response_data["output"]:
                    result_text = response_data["output"]["text"]
                    logger.info(f"从非标准格式中提取的文本: {result_text[:100]}...")
                    
                    # 尝试解析为 JSON
                    try:
                        analysis = json.loads(result_text)
                        return analysis
                    except:
                        # 构建简单的返回数据
                        return {
                            "题目原文": "API响应格式不符合预期",
                            "题目类型": "高等数学",
                            "具体分支": "未知",
                            "错误类型": "API响应错误",
                            "错误分析": f"API响应内容: {result_text[:300]}",
                            "正确解法": "请检查API配置",
                            "难度评估": 3,
                            "知识点标签": ["API", "错误修复"]
                        }
                
                # 返回一个表示未知响应格式的结果
                return {
                    "题目原文": "API响应格式未知",
                    "题目类型": "高等数学",
                    "具体分支": "未知",
                    "错误类型": "API响应格式错误",
                    "错误分析": "API返回了未知格式的响应",
                    "正确解法": "请检查API响应格式",
                    "难度评估": 3,
                    "知识点标签": ["API", "错误修复"]
                }
        except Exception as e:
            logger.error(f"解析 API 响应时出错: {str(e)}\n{traceback.format_exc()}")
            logger.debug(f"API响应内容: {json.dumps(response_data)[:1000]}")
            # 返回一个模拟数据
            return {
                "题目原文": "解析API响应失败",
                "题目类型": "高等数学",
                "具体分支": "未知",
                "错误类型": "未知",
                "错误分析": f"解析API响应时出错: {str(e)}",
                "正确解法": "无法提供",
                "难度评估": 3,
                "知识点标签": ["未知"]
            }
    except Exception as e:
        error_message = f"图像分析 API 请求异常: {str(e)}"
        logger.error(f"{error_message}\n{traceback.format_exc()}")
        # 返回一个模拟数据
        return {
            "题目原文": "API请求异常",
            "题目类型": "高等数学",
            "具体分支": "未知",
            "错误类型": "未知",
            "错误分析": f"API请求异常: {str(e)}",
            "正确解法": "无法提供",
            "难度评估": 3,
            "知识点标签": ["未知"]
        }

def analyze_with_gemini_api(image_path, user_notes='', subject='math'):
    """使用Google Gemini API分析错题图片"""
    logger.info(f"使用Google Gemini API进行图像分析，学科: {subject}")
    
    # 基本参数检查
    if not os.path.exists(image_path):
        error_msg = f"图片文件不存在: {image_path}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 获取 API 配置
    try:
        # 检查gemini_api配置是否存在
        if "gemini_api" not in API_CONFIG:
            error_msg = "未找到gemini_api配置"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查gemini_api配置是否完整
        gemini_api_config = API_CONFIG["gemini_api"]
        required_fields = ["api_key", "model_name"]
        
        if not all(field in gemini_api_config for field in required_fields):
            error_msg = "gemini_api配置不完整，缺少必要字段"
            logger.error(error_msg)
            return {"error": error_msg}
            
        # 检查api_key是否为空
        if not gemini_api_config["api_key"]:
            error_msg = "gemini_api的API密钥为空"
            logger.error(error_msg)
            return {"error": error_msg}
            
        api_key = gemini_api_config["api_key"]
        model_name = gemini_api_config["model_name"]
        
        logger.info(f"使用 Gemini 模型: {model_name}")
    except KeyError as e:
        error_msg = f"API 配置缺失: {str(e)}"
        logger.error(error_msg)
        return {"error": error_msg}
    
    # 读取图片文件
    try:
        # 动态导入google.generativeai，避免全局依赖
        logger.info("导入 Google GenAI 客户端库")
        try:
            from google import generativeai as genai
            import PIL.Image
            logger.info("成功导入 Google GenAI 客户端库和PIL")
        except ImportError:
            logger.error("未安装 Google GenAI 客户端库或PIL，请执行: pip install google-generativeai pillow")
            return {"error": "未安装必要库，请执行: pip install google-generativeai pillow"}
        
        # 配置API
        logger.info("配置 Gemini API")
        genai.configure(api_key=api_key)
        
        # 使用 gemini-pro-vision 模型
        model = genai.GenerativeModel('gemini-pro-vision')
        
        # 打开图像文件
        image = PIL.Image.open(image_path)
        logger.info(f"成功读取图片, 大小: {image.size}")
        
        # 获取针对特定学科的分析提示
        prompt = get_prompt_for_subject(subject, user_notes)
        logger.info(f"使用{subject}学科的分析提示模板")
        
        # 发送请求 - 简化为更通用的API调用方式
        logger.info(f"向 Gemini API 发送请求，使用模型: {model_name}")
        response = model.generate_content(
            [prompt, image],
            generation_config={"temperature": 0.3, "top_p": 0.95, "max_output_tokens": 2000}
        )
        
        logger.info("成功收到 Gemini API 响应")
        
        # 提取文本内容
        result_text = response.text
        logger.info(f"提取的Gemini响应文本: {result_text[:100]}...")
        
        # 尝试解析为JSON
        try:
            # 直接解析整个文本
            analysis = json.loads(result_text)
            return analysis
        except json.JSONDecodeError:
            # 查找JSON部分
            json_start = result_text.find('{')
            json_end = result_text.rfind('}') + 1
            
            if json_start >= 0 and json_end > json_start:
                json_str = result_text[json_start:json_end]
                try:
                    analysis = json.loads(json_str)
                    return analysis
                except:
                    logger.error("无法解析提取的 JSON 字符串")
            
            # 构建结构化数据
            return {
                "题目原文": extract_field_from_text(result_text, "题目原文") or result_text[:200],
                "题目类型": extract_field_from_text(result_text, "题目类型") or "高等数学",
                "具体分支": extract_field_from_text(result_text, "具体分支") or "未知",
                "错误类型": extract_field_from_text(result_text, "错误类型") or "未知",
                "错误分析": extract_field_from_text(result_text, "错误分析") or result_text[200:400],
                "正确解法": extract_field_from_text(result_text, "正确解法") or "请查看完整分析",
                "难度评估": extract_number_from_text(result_text, "难度评估") or 3,
                "知识点标签": extract_tags_from_text(result_text) or ["数学", "考研"]
            }
    except Exception as e:
        error_message = f"Gemini API 请求异常: {str(e)}"
        logger.error(f"{error_message}\n{traceback.format_exc()}")
        return {
            "题目原文": "API请求异常",
            "题目类型": "高等数学",
            "具体分支": "未知",
            "错误类型": "未知",
            "错误分析": f"API请求异常: {str(e)}",
            "正确解法": "无法提供",
            "难度评估": 3,
            "知识点标签": ["未知"]
        }
