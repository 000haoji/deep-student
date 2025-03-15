from flask import Blueprint, request, jsonify
import traceback
import json
import logging
import requests
from config import API_CONFIG, SUBJECT_ANALYSIS_PROMPTS
from core.ai_analysis.combined_model import get_subject_system_prompt  # 导入集中式的系统提示词获取函数

ai_analysis_api = Blueprint('ai_analysis_api', __name__)
logger = logging.getLogger(__name__)

@ai_analysis_api.route('/api/ai/deepseek-analysis', methods=['POST'])
def analyze_with_deepseek():
    """使用DeepseekR1进行错题批量分析
    
    请求体格式:
    {
        "problems": [
            {
                "id": "问题ID",
                "title": "题目标题",
                "content": "题目内容",
                "user_comments": "用户补充说明",
                "error_cause": "错因分析",
                "knowledge_tags": ["标签1", "标签2"]
            },
            ...
        ]
    }
    
    返回格式:
    {
        "success": true,
        "result": {
            "overview": "综合分析",
            "patterns": "错题模式识别",
            "recommendations": "针对性学习建议"
        }
    }
    """
    try:
        # 解析请求数据
        data = request.get_json()
        if not data or 'problems' not in data:
            return jsonify({"success": False, "error": "请求数据格式错误，缺少problems字段"}), 400
        
        problems = data['problems']
        if not problems or len(problems) == 0:
            return jsonify({"success": False, "error": "未提供任何错题数据"}), 400
        
        # 从配置中获取DeepseekR1 API参数
        try:
            r1_api_key = API_CONFIG["text_api"]["api_key"]
            r1_api_url = API_CONFIG["text_api"]["api_url"]
            r1_model_name = API_CONFIG["text_api"]["model_name"]
        except KeyError as e:
            logger.error(f"DeepseekR1 API配置缺失: {str(e)}")
            return jsonify({"success": False, "error": f"DeepseekR1 API配置缺失: {str(e)}"}), 500
        
        # 准备分析内容
        problems_for_analysis = []
        for problem in problems:
            problem_id = problem.get('id', '')
            title = problem.get('title', '')
            content = problem.get('content', '')
            user_comments = problem.get('user_comments', '')
            error_cause = problem.get('error_cause', '')
            knowledge_tags = problem.get('knowledge_tags', [])
            
            # 构建该题的分析内容
            problem_analysis = f"""
题目{len(problems_for_analysis)+1}:
标题: {title}
内容: {content}
知识点: {', '.join(knowledge_tags)}
用户补充说明: {user_comments if user_comments else '无'}
错因分析: {error_cause if error_cause else '无'}
---
"""
            problems_for_analysis.append(problem_analysis)
        
        # 获取学科提示词模板
        subject = data.get('subject', 'math')
        subject = subject.lower() if subject else 'math'
        logger.info(f"处理学科: {subject}")
        
        subject_prompt_template = ""
        if subject in SUBJECT_ANALYSIS_PROMPTS:
            subject_prompt_template = SUBJECT_ANALYSIS_PROMPTS[subject].get('full_prompt', '')
            logger.info(f"[学科跟踪] 从配置获取到 {subject} 的提示词模板")
        
        # 如果没有找到特定学科的提示词，使用通用模板
        if not subject_prompt_template:
            logger.warning(f"[学科跟踪] 未找到 {subject} 的提示词模板，使用通用模板")
            subject_prompt_template = """
我需要你帮我分析一组错题，找出错误模式和学习问题，并提供针对性建议。
这些错题信息包括题目内容、用户补充说明和错因分析。请作为一位经验丰富的数学教师，进行深入分析。

以下是错题集合:
{problems}

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

请确保分析深入、专业且具有建设性，直接指出问题并提供明确的改进方向。
"""
        
        # 组合分析提示词
        deepseek_prompt = subject_prompt_template.format(
            subject=subject,
            problems="".join(problems_for_analysis)
        )
        
        # 调用DeepseekR1 API
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {r1_api_key}"
        }
        
        request_data = {
            "model": r1_model_name,
            "messages": [
                {"role": "system", "content": get_subject_system_prompt(subject)},
                {"role": "user", "content": deepseek_prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 3000,
            "stream": False
        }
        
        # 发送请求
        logger.info(f"向DeepseekR1 API发送分析请求: {r1_api_url}")
        try:
            response = requests.post(
                r1_api_url,
                headers=headers,
                json=request_data,
                timeout=120  # 较长的超时时间，因为是批量分析
            )
            
            if response.status_code == 200:
                response_data = response.json()
                if "choices" in response_data and len(response_data["choices"]) > 0:
                    analysis_text = response_data["choices"][0]["message"]["content"]
                    logger.info(f"DeepseekR1分析成功，内容长度: {len(analysis_text)}")
                    
                    # 提取三个部分的分析结果
                    overview = extract_section(analysis_text, "综合分析", "错题模式识别")
                    patterns = extract_section(analysis_text, "错题模式识别", "针对性学习建议")
                    recommendations = extract_section(analysis_text, "针对性学习建议", None)
                    
                    if not overview:
                        overview = "API未返回综合分析部分，请稍后重试"
                    if not patterns:
                        patterns = "API未返回错题模式识别部分，请稍后重试"
                    if not recommendations:
                        recommendations = "API未返回针对性学习建议部分，请稍后重试"
                    
                    # 格式化显示，将Markdown格式的内容转为HTML
                    overview = format_markdown_to_html(overview)
                    patterns = format_markdown_to_html(patterns)
                    recommendations = format_markdown_to_html(recommendations)
                    
                    # 返回分析结果
                    return jsonify({
                        "success": True,
                        "result": {
                            "overview": overview,
                            "patterns": patterns,
                            "recommendations": recommendations
                        }
                    })
                else:
                    logger.error("DeepseekR1响应格式不符合预期")
                    return jsonify({"success": False, "error": "DeepseekR1响应格式不符合预期"}), 500
            else:
                logger.error(f"DeepseekR1 API请求失败，状态码: {response.status_code}")
                return jsonify({"success": False, "error": f"DeepseekR1 API请求失败: {response.text}"}), 500
                
        except requests.exceptions.Timeout:
            logger.error("DeepseekR1请求超时")
            return jsonify({"success": False, "error": "分析请求超时，请稍后重试或减少错题数量"}), 504
        except Exception as e:
            logger.error(f"DeepseekR1请求异常: {str(e)}")
            logger.error(traceback.format_exc())
            return jsonify({"success": False, "error": f"分析请求发生错误: {str(e)}"}), 500
            
    except Exception as e:
        error_message = f"处理分析请求失败: {str(e)}"
        logger.error(f"{error_message}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": error_message}), 500

# 辅助函数：从文本中提取特定部分的内容
def extract_section(text, start_marker, end_marker=None):
    """从文本中提取特定部分的内容"""
    # 寻找开始标记
    start_index = text.find(start_marker)
    if start_index == -1:
        # 尝试不同的格式，如"1. 综合分析"
        alt_start_marker = f"1. {start_marker}" if start_marker == "综合分析" else \
                         f"2. {start_marker}" if start_marker == "错题模式识别" else \
                         f"3. {start_marker}" if start_marker == "针对性学习建议" else \
                         None
        if alt_start_marker:
            start_index = text.find(alt_start_marker)
            if start_index != -1:
                start_marker = alt_start_marker
        
        # 如果仍然找不到
        if start_index == -1:
            return ""
    
    # 获取标题后的内容起始位置
    start_index += len(start_marker)
    
    # 跳过可能的冒号和空白
    while start_index < len(text) and (text[start_index] == ':' or text[start_index].isspace()):
        start_index += 1
    
    # 寻找结束标记
    if end_marker:
        end_index = text.find(end_marker, start_index)
        if end_index == -1:
            # 尝试不同的格式
            alt_end_marker = f"2. {end_marker}" if end_marker == "错题模式识别" else \
                          f"3. {end_marker}" if end_marker == "针对性学习建议" else \
                          None
            if alt_end_marker:
                end_index = text.find(alt_end_marker, start_index)
        
        # 如果找不到结束标记，则取到文本末尾
        if end_index == -1:
            return text[start_index:].strip()
        else:
            return text[start_index:end_index].strip()
    else:
        # 如果没有提供结束标记，则取到文本末尾
        return text[start_index:].strip()

# 辅助函数：将Markdown格式的文本转换为HTML显示
def format_markdown_to_html(text):
    """简单的Markdown到HTML转换"""
    # 处理LaTeX数学公式
    # 由于在HTML中直接处理LaTeX比较复杂，这里只做简单的替换
    # 真实实现可能需要使用MathJax等库
    
    # 处理列表项
    lines = text.split('\n')
    formatted_lines = []
    
    for line in lines:
        # 处理列表项
        if line.strip().startswith('- '):
            formatted_lines.append(f"• {line.strip()[2:]}")
        elif line.strip().startswith('* '):
            formatted_lines.append(f"• {line.strip()[2:]}")
        # 处理数字列表
        elif line.strip() and line.strip()[0].isdigit() and line.strip()[1:].startswith('. '):
            formatted_lines.append(f"{line}")
        else:
            formatted_lines.append(line)
    
    # 重新组合文本
    formatted_text = '<br>'.join(formatted_lines)
    
    # 处理加粗
    while '**' in formatted_text:
        formatted_text = formatted_text.replace('**', '<strong>', 1)
        if '**' in formatted_text:
            formatted_text = formatted_text.replace('**', '</strong>', 1)
    
    # 处理斜体
    while '*' in formatted_text:
        formatted_text = formatted_text.replace('*', '<em>', 1)
        if '*' in formatted_text:
            formatted_text = formatted_text.replace('*', '</em>', 1)
    
    return formatted_text
