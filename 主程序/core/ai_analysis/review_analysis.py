"""
回顾分析模块 - 用于分析多个错题的综合情况
"""
import os
import json
import logging
import requests
import traceback
from config import API_CONFIG, SUBJECT_ANALYSIS_PROMPTS
from core.ai_analysis.deepseek_adapter import DeepSeekAdapter
from .utils import extract_field_from_text
from .combined_model import get_main_deepseek_api, get_subject_system_prompt  # 导入获取系统提示词的函数

logger = logging.getLogger(__name__)

def analyze_with_text_api(problems_data):
    """调用文本API进行回顾分析"""
    logger.info(f"开始使用文本API进行回顾分析，共有{len(problems_data)}道错题")
    
    try:
        # 获取API配置 - 使用get_main_deepseek_api函数
        deepseek_config = get_main_deepseek_api()
        api_key = deepseek_config["api_key"]
        api_url = deepseek_config["api_url"]
        model_name = deepseek_config["model_name"]
        
        # API配置检查
        if not api_key or not api_url:
            error_msg = "文本API的API密钥或URL为空"
            logger.error(error_msg)
            return {"error": error_msg, "summary": "无法获取有效的API配置"}
            
        logger.info(f"使用API URL: {api_url}, 模型: {model_name}")
        
        # 确定学科类型 - 从第一道题中获取
        subject = "math"  # 默认值
        if problems_data and len(problems_data) > 0:
            subject = problems_data[0].get('subject', 'math')
        logger.info(f"错题回顾分析使用学科: {subject}")
        
        # 获取学科特定的教师提示词
        def get_subject_teacher_prompt(subj):
            """获取特定学科的教师提示词"""
            # 使用全局函数获取学科系统提示词，确保一致性
            return get_subject_system_prompt(subj)
        
        # 构建错题内容摘要
        problems_summary = []
        for i, problem in enumerate(problems_data, 1):
            summary = f"错题{i}:\n"
            summary += f"题型: {problem.get('problem_category', '未知')} - {problem.get('problem_subcategory', '未知')}\n"
            summary += f"题目: {problem.get('problem_content', '未知')[:200]}\n"
            summary += f"错误类型: {problem.get('error_type', '未知')}\n"
            summary += f"错误分析: {problem.get('error_analysis', '未知')[:200]}\n"
            summary += f"难度: {problem.get('difficulty', 3)}/5\n"
            summary += f"知识点: {problem.get('tags', '[]')}\n"
            summary += "---\n"
            problems_summary.append(summary)
        
        # 合并错题摘要，限制长度
        all_problems_text = "\n".join(problems_summary)
        if len(all_problems_text) > 8000:  # 避免token过多
            all_problems_text = all_problems_text[:8000] + "...(内容过多已截断)"
        
        # 构建分析提示 - 根据学科选择提示词
        subject_prompts = {
            "math": {
                "title": "考研数学",
                "teacher": "专业的考研数学教师"
            },
            "english": {
                "title": "考研英语",
                "teacher": "专业的考研英语教师"
            },
            "physics": {
                "title": "考研物理",
                "teacher": "专业的考研物理教师"
            },
            "chemistry": {
                "title": "考研化学",
                "teacher": "专业的考研化学教师"
            },
            "politics": {
                "title": "考研政治",
                "teacher": "专业的考研政治教师"
            },
            "professional": {
                "title": "考研专业课",
                "teacher": "专业的考研专业课教师"
            }
        }
        
        # 获取当前学科的提示词信息，如果不存在则使用默认值
        subject_info = subject_prompts.get(subject, {"title": "考研", "teacher": "专业的考研教师"})
        
        prompt = f"""
        你是一位{subject_info["teacher"]}。以下是一些学生做错的{subject_info["title"]}题。请分析这些错题，总结学生的学习问题并给出改进建议。

        {all_problems_text}

        请按以下结构进行分析：
        2. 知识点薄弱区域：识别学生在哪些知识点或题型上存在明显不足
        3. 针对性学习策略：根据错题分析，提供2-3个具体的学习改进建议
        4. 习题推荐：推荐3-5个适合克服这些问题的习题类型或具体练习方法
        5. 时间规划建议：如何合理安排时间来弥补这些薄弱环节

        你的分析需要具体、实用，能帮助学生明确改进方向。请以JSON格式返回你的分析结果，包含以上5个部分。不要添加额外的解释。
        """
        
        # 构建API请求
        messages = [
            {"role": "system", "content": get_subject_teacher_prompt(subject)},
            {"role": "user", "content": prompt}
        ]
        
        request_data = {
            "model": model_name,
            "messages": messages,
            "temperature": 0.5
        }
        
        logger.info(f"发送回顾分析请求到API: {api_url}")
        response = requests.post(
            api_url,
            headers={"Content-Type": "application/json", "Authorization": f"Bearer {api_key}"},
            json=request_data,
            timeout=60
        )
        
        if response.status_code != 200:
            logger.error(f"文本分析API请求失败，状态码: {response.status_code}")
            logger.error(f"错误响应: {response.text}")
            return {
                "错误模式识别": "API请求失败，无法获取分析",
                "知识点薄弱区域": "请检查API配置",
                "针对性学习策略": "建议手动分析错题",
                "习题推荐": "无法提供",
                "时间规划建议": "无法提供"
            }
        
        # 提取回复内容
        response_data = response.json()
        
        # 解析响应
        if "choices" in response_data and len(response_data["choices"]) > 0:
            result_text = response_data["choices"][0]["message"]["content"]
            
            # 尝试解析为JSON
            try:
                analysis = json.loads(result_text)
                return analysis
            except json.JSONDecodeError:
                # 尝试提取JSON部分
                json_start = result_text.find('{')
                json_end = result_text.rfind('}') + 1
                
                if json_start >= 0 and json_end > json_start:
                    try:
                        json_str = result_text[json_start:json_end]
                        analysis = json.loads(json_str)
                        return analysis
                    except:
                        logger.error("无法解析提取的JSON字符串")
                
                # 使用正则表达式提取各个部分
                return {
                    "错误模式识别": extract_field_from_text(result_text, "错误模式识别") or "解析失败",
                    "知识点薄弱区域": extract_field_from_text(result_text, "知识点薄弱区域") or "解析失败",
                    "针对性学习策略": extract_field_from_text(result_text, "针对性学习策略") or "解析失败",
                    "习题推荐": extract_field_from_text(result_text, "习题推荐") or "解析失败",
                    "时间规划建议": extract_field_from_text(result_text, "时间规划建议") or "解析失败"
                }
        else:
            logger.error("API响应格式不符合预期")
            logger.error(f"完整响应: {json.dumps(response_data)}")
            return {
                "错误模式识别": "API响应格式错误",
                "知识点薄弱区域": "请检查API配置",
                "针对性学习策略": "无法提供",
                "习题推荐": "无法提供",
                "时间规划建议": "无法提供"
            }
    except Exception as e:
        logger.error(f"回顾分析失败: {str(e)}\n{traceback.format_exc()}")
        return {
            "错误模式识别": f"分析过程出错: {str(e)}",
            "知识点薄弱区域": "无法提供",
            "针对性学习策略": "无法提供",
            "习题推荐": "无法提供",
            "时间规划建议": "无法提供"
        }