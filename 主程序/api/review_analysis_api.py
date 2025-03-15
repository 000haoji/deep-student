from flask import Blueprint, request, jsonify
import traceback
import json
import logging
import time
from config import API_CONFIG
from core import database
from datetime import datetime
from core.ai_analysis.combined_model import get_subject_system_prompt, get_subject_review_prompt

review_analysis_api = Blueprint('review_analysis_api', __name__)
logger = logging.getLogger(__name__)

@review_analysis_api.route('/api/ai/review-analysis', methods=['POST'])
def review_analysis():
    """
    回顾分析API，用于提供针对性的错题分析
    复用主页的流式分析逻辑，但跳过OCR步骤
    """
    # 记录开始时间
    start_time = time.time()
    logger.info(f"开始处理回顾分析请求，时间戳: {start_time}")
    
    try:
        # 获取请求数据
        data = request.json
        if not data:
            logger.error("未提供任何数据")
            return jsonify({"success": False, "error": "未提供任何数据"}), 400
        
        # 获取回顾ID和问题数据
        review_id = data.get('review_id', '')
        problems = data.get('problems', [])
        
        if not review_id:
            logger.error("未提供回顾ID")
            return jsonify({"success": False, "error": "未提供回顾ID"}), 400
            
        if not problems or len(problems) == 0:
            logger.error("未提供任何问题数据")
            return jsonify({"success": False, "error": "未提供任何问题数据"}), 400
        
        # 获取学科，默认为math
        subject = data.get('subject', 'math')
        logger.info(f"[学科跟踪] 从请求体获取学科: '{subject}'")
        
        # 标准化学科名称（转小写）以与配置匹配
        subject = subject.lower() if subject else 'math'
        logger.info(f"[学科跟踪] 标准化后的学科名称: '{subject}'")
        
        # 获取系统提示词
        system_prompt = get_subject_system_prompt(subject)
        
        # 构建回顾分析的问题内容
        problems_for_analysis = []
        for problem in problems:
            # 确保问题内容和问题类型存在
            if 'content' in problem and 'type' in problem:
                problems_for_analysis.append({
                    'problem_id': problem.get('id', ''),
                    'problem_content': problem.get('content', ''),
                    'problem_type': problem.get('type', ''),
                    'student_answer': problem.get('student_answer', ''),
                    'correct_answer': problem.get('correct_answer', ''),
                    'analysis': problem.get('analysis', '')
                })
        
        if len(problems_for_analysis) == 0:
            logger.error("所有问题数据格式无效")
            return jsonify({"success": False, "error": "所有问题数据格式无效"}), 400
        
        # 构建回顾分析的问题内容
        problems_content = ""
        for i, problem in enumerate(problems_for_analysis):
            problems_content += f"""
【题目{i+1}】：
{problem['problem_content']}

【题目类型】：{problem['problem_type']}
【学生答案】：{problem['student_answer']}
【正确答案】：{problem['correct_answer']}
【题目分析】：{problem['analysis']}

"""

        # 构建回顾分析的用户提示词
        user_prompt = f"""错题数量：{len(problems_for_analysis)}道

错题内容如下：
{problems_content}"""
        
        # 构建回顾分析的完整提示词
        def build_review_prompt(subject_name, problem_data):
            try:
                review_prompt = get_subject_review_prompt(subject_name)
                problem_text = problem_data
                return f"{review_prompt}\n\n{problem_text}"
            except Exception as e:
                logger.error(f"构建回顾分析提示词失败: {str(e)}")
                return None
        
        full_prompt = build_review_prompt(subject, user_prompt)
        
        logger.info(f"构建完成的用户提示词长度: {len(full_prompt)}")
        
        # 调用大语言模型API进行分析
        # 复用首页分析的API调用逻辑
        try:
            # 从配置中获取文本分析API参数
            try:
                api_key = API_CONFIG["text_api"]["api_key"]
                api_url = API_CONFIG["text_api"]["api_url"]
                model_name = API_CONFIG["text_api"]["model_name"]
            except KeyError as e:
                # 回退到旧版配置结构
                if "deepseek" in API_CONFIG and isinstance(API_CONFIG["deepseek"], dict):
                    # 新结构: config.py中的deepseek字典
                    model_name = API_CONFIG["deepseek"].get("model", "deepseek-chat")
                    api_key = API_CONFIG["deepseek"].get("api_key", "")
                    api_url = API_CONFIG["deepseek"].get("api_url", "https://api.deepseek.com/v1")
                else:
                    # 旧结构或备用方式
                    model_name = API_CONFIG.get("DEEPSEEK_MODEL_NAME", "deepseek-chat")
                    api_key = API_CONFIG.get("DEEPSEEK_API_KEY", "")
                    api_url = API_CONFIG.get("DEEPSEEK_API_URL", "https://api.deepseek.com/v1")
            
            if not api_key:
                logger.error("未配置API密钥")
                return jsonify({"success": False, "error": "未配置API密钥，请在设置中配置"}), 500
            
            # 完整endpoint应以/chat/completions结尾
            if not api_url.endswith("/chat/completions"):
                api_url = api_url.rstrip("/") + "/chat/completions"
                
            logger.info(f"使用API端点: {api_url}")
            
            # 准备API请求
            headers = {
                "Authorization": f"Bearer {api_key}",
                "Content-Type": "application/json"
            }
            
            payload = {
                "model": model_name,
                "messages": [
                    {"role": "system", "content": system_prompt},
                    {"role": "user", "content": full_prompt}
                ],
                "temperature": 0.3,
                "max_tokens": 4000
            }
            
            # 调用API获取分析结果
            import requests
            logger.info(f"向API发送请求: {api_url}")
            response = requests.post(
                api_url,
                headers=headers,
                json=payload,
                timeout=(5, 90)  # 连接超时5秒，读取超时90秒
            )
            
            if response.status_code != 200:
                logger.error(f"API调用失败: {response.status_code} {response.text}")
                return jsonify({"success": False, "error": f"API调用失败: {response.status_code}"}), 500
                
            # 解析响应结果
            response_data = response.json()
            ai_output = response_data['choices'][0]['message']['content']
            
            logger.info(f"获取到AI输出内容，长度: {len(ai_output)}")
            
            # 处理分析结果，提取关键信息
            analysis_result = {
                "overview": extract_section(ai_output, "综合分析"),
                "patterns": extract_section(ai_output, "错题模式识别"),
                "recommendations": extract_section(ai_output, "针对性学习建议")
            }
            
            # 将分析结果保存到数据库
            try:
                # 使用通用函数获取回顾记录，以兼容两种表结构
                review_session = database.get_review_session_by_id(review_id)
                
                if review_session:
                    # 更新review_analysis字段
                    current_analysis = {}
                    
                    # 兼容两种表结构的数据格式
                    if 'review_analysis' in review_session:
                        review_analysis = review_session['review_analysis']
                        # 解析现有数据
                        if isinstance(review_analysis, str):
                            try:
                                current_analysis = json.loads(review_analysis)
                            except:
                                current_analysis = {}
                        elif isinstance(review_analysis, dict):
                            current_analysis = review_analysis
                    
                    # 添加新的AI分析结果
                    current_analysis['ai_analysis'] = analysis_result
                    current_analysis['ai_analysis_timestamp'] = datetime.now().isoformat()
                    
                    # 使用通用函数更新回顾分析数据
                    # 兼容两种表结构
                    success = database.update_review_analysis(review_id, current_analysis)
                    
                    if success:
                        logger.info(f"成功将AI分析结果保存到回顾记录 {review_id}")
                    else:
                        logger.error(f"更新回顾记录分析结果失败: {review_id}")
                else:
                    logger.error(f"未找到回顾记录: {review_id}")
            except Exception as e:
                logger.error(f"保存分析结果到数据库时出错: {str(e)}")
                logger.error(traceback.format_exc())
                # 注意：这里我们继续返回分析结果，即使保存失败
            
            # 记录完成时间和持续时间
            end_time = time.time()
            duration = end_time - start_time
            logger.info(f"完成回顾分析，持续时间: {duration:.2f}秒")
            
            # 返回分析结果
            return jsonify({
                "success": True,
                "result": analysis_result,
                "duration": f"{duration:.2f}秒"
            })
            
        except Exception as e:
            logger.error(f"处理回顾分析时出错: {str(e)}")
            logger.error(traceback.format_exc())
            return jsonify({"success": False, "error": str(e)}), 500

    except Exception as e:
        logger.error(f"处理回顾分析时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({"success": False, "error": str(e)}), 500

def extract_section(text, section_name):
    """从AI输出文本中提取特定部分"""
    try:
        # 方法1：寻找明确的标题格式
        patterns = [
            f"{section_name}：", 
            f"{section_name}:", 
            f"## {section_name}", 
            f"# {section_name}",
            f"**{section_name}**",
            f"*{section_name}*"
        ]
        
        for pattern in patterns:
            if pattern in text:
                start_idx = text.index(pattern) + len(pattern)
                # 查找下一个部分的开始
                next_section_idx = float('inf')
                for p in patterns:
                    if p != pattern and p in text[start_idx:]:
                        idx = text.index(p, start_idx)
                        next_section_idx = min(next_section_idx, idx)
                
                if next_section_idx < float('inf'):
                    return text[start_idx:next_section_idx].strip()
                else:
                    return text[start_idx:].strip()
        
        # 方法2：按段落分割，寻找包含关键词的段落
        paragraphs = text.split('\n\n')
        for i, para in enumerate(paragraphs):
            if section_name in para and i < len(paragraphs) - 1:
                return paragraphs[i+1].strip()
        
        # 如果无法找到特定部分，返回整个文本
        return text
    except Exception as e:
        logger.error(f"提取'{section_name}'部分时出错: {str(e)}")
        return text  # 返回原始文本作为备选
