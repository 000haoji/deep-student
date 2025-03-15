from flask import Blueprint, request, jsonify, Response, stream_with_context
import traceback
import json
import logging
import asyncio
import aiohttp
import requests
from config import API_CONFIG
from datetime import datetime
from core import database  # 添加导入数据库模块
from api.problem_api import update_problem_analysis_internal  # 直接导入problem_api的更新函数
import time  # 添加time模块导入
from core.ai_analysis.combined_model import get_subject_system_prompt, get_subject_review_prompt  # 导入集中式的系统提示词获取函数
import config  # 添加导入config模块

stream_analysis_api = Blueprint('stream_analysis_api', __name__)
logger = logging.getLogger(__name__)

@stream_analysis_api.route('/api/ai/stream-analysis', methods=['POST'])
def stream_analysis():
    """
    流式分析错题API，用于交互式错题分析
    """
    # 获取请求数据
    data = request.json
    return stream_analysis_with_data(data)

def stream_analysis_with_data(data):
    """
    流式分析错题API内部函数，接受数据参数而不是从request获取
    用于复用流式分析逻辑，确保与主页分析保持一致
    """
    # 记录开始时间
    start_time = time.time()
    logger.info(f"开始处理流式分析请求，时间戳: {start_time}")
    
    try:
        if not data:
            logger.error("未提供任何数据")
            return jsonify({"success": False, "error": "未提供任何数据"}), 400
        
        # 获取问题列表
        problems = data.get('problems', [])
        if not problems or len(problems) == 0:
            logger.error("未提供任何问题数据")
            return jsonify({"success": False, "error": "未提供任何问题数据"}), 400
        
        # 获取学科，默认为math
        subject = data.get('subject', 'math')
        logger.info(f"[学科跟踪] 从请求体获取学科: '{subject}'")
        
        # 标准化学科名称（转小写）以与配置匹配
        subject = subject.lower() if subject else 'math'
        logger.info(f"[学科跟踪] 标准化后的学科名称: '{subject}'")
        
        # 从请求参数中获取problem_id (优先使用)
        problem_id = request.args.get('problem_id', '')
        
        # 如果URL参数中没有problem_id，尝试从请求体中获取
        if not problem_id:
            # 尝试从problems的第一个问题中获取id
            if problems and len(problems) > 0 and 'id' in problems[0]:
                problem_id = problems[0]['id']
                logger.info(f"从请求数据中获取到问题ID: {problem_id}")
            else:
                # 仍然保留默认ID作为后备方案
                problem_id = "problem-1"
                logger.warning(f"未从请求中找到有效的问题ID，使用默认ID: {problem_id}")
        else:
            logger.info(f"从URL参数中获取到问题ID: {problem_id}")
        
        # 检查每个问题的内容完整性
        check_start_time = time.time()
        logger.info(f"开始检查问题数据: {check_start_time}, 耗时: {check_start_time - start_time:.2f}秒")
        
        for i, problem in enumerate(problems):
            logger.info(f"问题 {i+1} 数据检查:")
            if not problem:
                logger.error(f"问题 {i+1} 为空")
                continue
                
            # 检查关键字段
            for key in ['content', 'user_comments', 'error_cause', 'knowledge_tags']:
                has_key = key in problem
                has_value = has_key and problem[key] and (isinstance(problem[key], str) and len(problem[key]) > 0 or isinstance(problem[key], list) and len(problem[key]) > 0)
                logger.info(f"问题 {i+1} 字段 '{key}': 存在={has_key}, 有值={has_value}, 类型={type(problem[key]) if has_key else 'N/A'}")
                
                if has_key and has_value and isinstance(problem[key], str):
                    logger.info(f"问题 {i+1} 字段 '{key}' 内容预览: {problem[key][:50]}...")
                elif has_key and has_value and isinstance(problem[key], list):
                    logger.info(f"问题 {i+1} 字段 '{key}' 列表内容: {problem[key]}")

        # 从配置中获取DeepSeek API参数
        try:
            api_key = API_CONFIG["text_api"]["api_key"]
            api_url = API_CONFIG["text_api"]["api_url"]
            model_name = API_CONFIG["text_api"]["model_name"]
        except KeyError as e:
            logger.error(f"DeepSeek API配置缺失: {str(e)}")
            return jsonify({"success": False, "error": f"DeepSeek API配置缺失: {str(e)}"}), 500
        
        # 检查是否为回顾分析
        is_review_analysis = data.get('is_review_analysis', False)
        logger.info(f"[分析类型] 是否为回顾分析: {is_review_analysis}")
        
        # 准备分析内容
        prepare_start_time = time.time()
        logger.info(f"开始准备分析内容: {prepare_start_time}, 耗时: {prepare_start_time - check_start_time:.2f}秒")
        
        problems_for_analysis = []
        for problem in problems:
            problem_id = problem.get('id', '')
            # 优先从problem_content获取内容，这是OCR识别的结果
            content = problem.get('problem_content', '') or problem.get('content', '')
            # 记录实际获取到的内容
            logger.info(f"题目{problem_id} - OCR/Content内容: {content[:100]}")
            logger.info(f"题目{problem_id} - OCR/Content内容长度: {len(content)}")
            
            title = problem.get('title', f'错题{problem_id}')
            # 兼容不同的字段名称
            user_comments = problem.get('user_comments', '') or problem.get('notes', '')
            error_cause = problem.get('error_cause', '') or problem.get('error_type', '')  
            knowledge_tags = problem.get('knowledge_tags', []) or problem.get('tags', [])
            
            # 记录实际使用的内容
            logger.info(f"分析题目 {problem_id} 的OCR内容: {content[:100]}...")
            
            # 构建该题的分析内容
            problem_analysis = f"""
题目{len(problems_for_analysis)+1}:
标题: {title}
内容: {content}
知识点: {', '.join(knowledge_tags) if knowledge_tags else '未标记'}
用户补充说明: {user_comments if user_comments else '无'}
错因分析: {error_cause if error_cause else '无'}
---
"""
            problems_for_analysis.append(problem_analysis)
            logger.info(f"题目{problem_id} - 添加到问题分析集合中，分析文本长度: {len(problem_analysis)}")
        
        # 记录问题数据以便调试
        problem = problems[0]  # 取第一个问题进行分析
        logger.info(f"准备分析问题: {problem.get('id', 'unknown')}")
        
        # 优先使用problem_content字段（OCR识别结果）
        problem_content = problem.get('problem_content', '') or problem.get('content', '')
        logger.info(f"问题内容长度: {len(problem_content)}")
        logger.info(f"问题内容预览: {problem_content[:100]}")
        logger.info(f"问题补充信息: {problem.get('user_comments', '')[:100]}")
        logger.info(f"问题错因分析: {problem.get('error_cause', '')[:100]}")
        
        # 获取学科分析模板
        # 由于导入问题，暂时内联定义分析模板
        analysis_template = """"""
        logger.info(f"[学科跟踪] 使用内联定义的分析模板")
        
        # 组合分析提示词
        prompt_start_time = time.time()
        logger.info(f"开始构建提示词: {prompt_start_time}, 耗时: {prompt_start_time - prepare_start_time:.2f}秒")
        
        # 从配置中获取学科提示词模板，而不是使用硬编码
        subject_prompt_template = ""
        if subject in config.SUBJECT_ANALYSIS_PROMPTS:
            subject_prompt_template = config.SUBJECT_ANALYSIS_PROMPTS[subject].get('full_prompt', '')
            analysis_template=subject_prompt_template
            logger.info(f"[学科跟踪] 从配置获取到 {subject} 的提示词模板")
        
        # 如果没有找到特定学科的提示词，使用通用模板
        if not subject_prompt_template:
            logger.warning(f"[学科跟踪] 未找到 {subject} 的提示词模板，使用通用模板")
            subject_prompt_template = """
我需要你帮我分析一组{subject}错题，找出错误模式和学习问题，并提供针对性建议。
这些错题信息包括题目内容、用户补充说明和错因分析。

以下是错题集合：
{problems}

{analysis_template}

请确保分析深入、专业且具有建设性，直接指出问题并提供明确的改进方向。
"""
        else:
            # 针对已经OCR的内容，修改subject_prompt_template
            # 确保提示词中包含实际的OCR内容
            logger.info(f"[学科跟踪] 修改 {subject} 的提示词模板，确保包含OCR内容")
            subject_prompt_template = """
分析这个{subject}错题，请提供以下结构化信息：

以下是错题的OCR内容：
{problems}

{analysis_template}

请确保分析深入、专业且具有建设性，直接指出问题并提供明确的改进方向。
"""
        
        # 构建最终的分析提示词
        deepseek_prompt = subject_prompt_template.format(
            subject=subject,
            problems="".join(problems_for_analysis),
            analysis_template=analysis_template
        )
        
        # 检查提示词中是否包含OCR内容 - 对所有学科都进行检查
        # 获取第一个问题的OCR内容
        first_problem = problems[0]
        problem_content = first_problem.get('problem_content', '') or first_problem.get('content', '')
        
        # 记录OCR内容和最终提示词中的对比
        logger.info(f"[学科跟踪] 问题OCR内容: {problem_content[:100]}...")
        
        # 如果OCR内容未包含在提示词中，强制添加
        if problem_content and problem_content not in deepseek_prompt:
            logger.warning(f"[学科跟踪] OCR内容未包含在提示词中，进行强制添加")
            # 在提示词开头添加OCR内容
            deepseek_prompt = f"""
分析下面的{subject}题目：

题目内容: {problem_content}

{deepseek_prompt}
"""
            logger.info(f"[学科跟踪] 添加OCR内容后的提示词长度: {len(deepseek_prompt)}")
        
        # 记录完整的prompt用于调试
        prompt_end_time = time.time()
        logger.info(f"提示词构建完成，耗时: {prompt_end_time - prompt_start_time:.2f}秒，提示词长度: {len(deepseek_prompt)}")
        # 记录提示词的前200个字符和后200个字符，以便验证问题内容是否被正确包含
        logger.info(f"提示词前200字符: {deepseek_prompt[:200]}")
        logger.info(f"提示词中间部分: {deepseek_prompt[200:400] if len(deepseek_prompt) > 400 else ''}")
        logger.info(f"提示词后200字符: {deepseek_prompt[-200:] if len(deepseek_prompt) > 200 else deepseek_prompt}")
        
        # 构建API请求头
        request_prepare_start_time = time.time()
        logger.info(f"开始准备API请求参数: {request_prepare_start_time}, 耗时: {request_prepare_start_time - prompt_end_time:.2f}秒")
        headers = {
            "Content-Type": "application/json",
            "Authorization": f"Bearer {api_key}"
        }
        
        # 获取系统提示词
        if is_review_analysis:
            system_prompt = get_subject_review_prompt(subject)
            logger.info(f"[学科跟踪] 获取到的回顾分析系统提示词: '{system_prompt[:50]}...'")
        else:
            system_prompt = get_subject_system_prompt(subject)
            logger.info(f"[学科跟踪] 获取到的常规分析系统提示词: '{system_prompt[:50]}...'")
        
        request_data = {
            "model": model_name,
            "messages": [
                {"role": "system", "content": system_prompt},  # 使用获取的系统提示词
                {"role": "user", "content": deepseek_prompt}
            ],
            "temperature": 0.3,
            "max_tokens": 3000,
            "stream": True,
            "enable_reasoning": True,  # 启用思维链
            "enable_reasoning_content": True,  # 启用新版API的reasoning_content字段
            "stream_reasoning_content": True  # 确保reasoning_content也以流式方式传输
        }
        
        request_prepare_end_time = time.time()
        logger.info(f"API请求参数准备完成，耗时: {request_prepare_end_time - request_prepare_start_time:.2f}秒")
        # 记录完整的请求数据用于调试
        logger.info(f"发送给DeepSeek的请求数据: {json.dumps(request_data)}")
        
        # 创建流式响应函数
        def generate(problems, problem_id=None, subject='math'):
            # 调用DeepSeek API
            api_request_start_time = time.time()
            logger.info(f"开始发送API请求: {api_request_start_time}, 耗时: {api_request_start_time - request_prepare_end_time:.2f}秒")
            logger.info(f"向DeepSeek API发送流式分析请求: {api_url}")
            
            # 添加更详细的请求日志记录
            try:
                # 使用Session对象来优化连接复用
                session = requests.Session()
                # 配置适当的超时时间（连接超时和读取超时）
                request_start_time = time.time()
                response = session.post(
                    api_url,
                    headers=headers,
                    json=request_data,
                    timeout=(5, 90),  # 连接超时5秒，读取超时90秒
                    stream=True
                )
                request_end_time = time.time()
                logger.info(f"API请求已发送，等待响应，耗时: {request_end_time - request_start_time:.2f}秒，总耗时: {request_end_time - start_time:.2f}秒")
                
                if response.status_code != 200:
                    error_message = f"DeepSeek API请求失败: {response.status_code}"
                    logger.error(error_message)
                    logger.error(f"响应内容: {response.text}")
                    return jsonify({"success": False, "error": error_message}), 500
                
                # 处理流式响应
                first_response_received = False
                first_data_logged = False
                response_iter = response.iter_lines(chunk_size=1, decode_unicode=False)  # 修改：不在此处解码
                
                # 强制立即发起请求并获取首个响应
                accumulated_content = ""
                for line in response_iter:
                    if line:
                        # 修改：统一处理字节解码，使用utf-8 errors='replace'避免编码错误
                        if isinstance(line, bytes):
                            try:
                                line = line.decode('utf-8', errors='replace')
                            except UnicodeDecodeError:
                                logger.warning(f"UTF-8解码失败，尝试其他编码方式")
                                try:
                                    line = line.decode('latin-1')  # 尝试Latin-1，这是一种不会失败的编码
                                except:
                                    logger.error("所有解码方式都失败")
                                    continue
                        
                        if not first_response_received:
                            first_token_time = time.time()
                            logger.info(f"收到首个响应数据，耗时: {first_token_time - request_end_time:.2f}秒，总耗时: {first_token_time - start_time:.2f}秒")
                            first_response_received = True
                        
                        # 处理可能的编码问题
                        try:
                            # 1. 首先检查行是否以'data: '开头
                            if not line.startswith('data: '):
                                logger.warning(f"收到非标准SSE格式数据: {line[:50]}...")
                                # 修改：尝试修复非标准SSE数据格式
                                if ':' in line:
                                    # 可能只有部分前缀，尝试提取数据部分
                                    parts = line.split(':', 1)
                                    if len(parts) > 1:
                                        data = parts[1].strip()
                                    else:
                                        continue
                                else:
                                    continue
                            else:
                                # 2. 提取并处理数据部分
                                data = line[6:]  # 去掉 'data: ' 前缀
                                data = data.strip()
                            
                            # 3. 处理特殊数据格式
                            if data == '[DONE]':
                                # 转发完成标记
                                logger.info("从DeepSeek接收到完成标记 [DONE]")
                                # 在[DONE]之前发送一个特殊标记，确保前端任务永远被标记为成功
                                try:
                                    # 发送一个始终有效的JSON结果标记
                                    success_marker = {
                                        "success_marker": True,
                                        "timestamp": datetime.now().strftime("%Y-%m-%d %H:%M:%S")
                                    }
                                    yield f"data: {json.dumps(success_marker)}\n\n"
                                except:
                                    pass  # 即使失败也继续
                                
                                yield "data: [DONE]\n\n"
                                break
                                
                            # 4. 检查和清理JSON字符串
                            # 修改：增强JSON字符串清理和验证
                            if not data:
                                continue
                                
                            # 修复可能的损坏JSON
                            if not (data.startswith('{') and data.endswith('}')):
                                logger.warning(f"收到不完整的JSON数据: {data[:50]}...")
                                # 尝试寻找完整的JSON对象
                                if '{' in data and '}' in data:
                                    start_idx = data.find('{')
                                    end_idx = data.rfind('}') + 1
                                    if start_idx < end_idx:
                                        data = data[start_idx:end_idx]
                                    else:
                                        continue
                                else:
                                    continue
                            
                            # 5. 处理转义字符和Unicode编码问题
                            try:
                                # 使用增强的JSON修复函数
                                data = fix_json_escapes(data)
                                
                                # 在尝试解析前记录处理后的数据（仅用于调试）
                                logger.debug(f"处理后的JSON数据: {data[:100]}...")
                            except Exception as json_fix_error:
                                logger.error(f"修复JSON数据时出错: {str(json_fix_error)}")
                                # 如果修复失败，使用默认空对象
                                data = json.dumps({
                                    "题目类型": "未知类型",
                                    "具体分支": "未知分支",
                                    "错误类型": "未知错误类型",
                                    "题目原文": "无法提取题目",
                                    "错误分析": "无法分析，API返回的数据格式错误",
                                    "正确解法": "无法提供解法，API返回的数据格式错误",
                                    "难度评估": 3,
                                    "知识点标签": []
                                })
                            
                            # 6. 解析并转发数据
                            try:
                                data_obj = json.loads(data)
                            except json.JSONDecodeError as e:
                                logger.warning(f"JSON解析错误: {str(e)}, 数据: {data[:50]}...")
                                continue
                            
                            # 只在首次收到数据时记录数据块类型，不记录每个数据块
                            if first_response_received and 'choices' in data_obj and len(data_obj['choices']) > 0:
                                choice = data_obj['choices'][0]
                                delta_keys = []
                                if 'delta' in choice:
                                    delta_keys = list(choice['delta'].keys())
                                if not first_data_logged:
                                    logger.info(f"从DeepSeek接收到数据块类型: delta字段={delta_keys}, 是否有reasoning_content={('reasoning_content' in choice)}")
                                    first_data_logged = True
                        
                            # 检查是否包含各种思维链格式
                            if 'choices' in data_obj and len(data_obj['choices']) > 0:
                                choice = data_obj['choices'][0]
                                
                                # 累积内容 - 在每个数据块处理中累积内容
                                if 'content' in choice:
                                    # 记录累积内容
                                    curr_content = choice.get('content', '')
                                    accumulated_content += curr_content
                                    logger.debug(f"累积内容: +{len(curr_content)}字符, 总计={len(accumulated_content)}")
                                elif 'delta' in choice and 'content' in choice['delta']:
                                    # 记录增量内容
                                    curr_delta = choice['delta'].get('content', '')
                                    accumulated_content += curr_delta
                                    logger.debug(f"累积增量: +{len(curr_delta)}字符, 总计={len(accumulated_content)}")
                                    
                                # 处理DeepSeek Reasoner API的reasoning_content字段
                                if 'reasoning_content' in choice:
                                    # 确保delta对象存在
                                    if 'delta' not in choice:
                                        choice['delta'] = {}
                                    # 将reasoning_content复制到delta.reasoning_content，确保不是None
                                    if choice['reasoning_content'] is not None:
                                        choice['delta']['reasoning_content'] = choice['reasoning_content']
                                
                                # 如果有reasoning但不在delta中，移动到delta中
                                if 'reasoning' in choice and 'delta' in choice:
                                    if 'reasoning' not in choice['delta']:
                                        choice['delta']['reasoning'] = choice['reasoning']
                                        # 删除外层的reasoning避免重复
                                        del choice['reasoning']
                                        
                                # 检查是否是最后一个消息(delta中有内容结束标记或finish_reason)
                                if 'delta' in choice and (
                                    ('content' in choice['delta'] and choice['delta']['content'] and choice['delta']['content'].strip().endswith('```')) or
                                    ('finish_reason' in choice and choice['finish_reason'] is not None)):
                                    # 在最后一个消息中添加完整的分析结果对象
                                    
                                    # 从流式文本中提取有用信息
                                    full_content = accumulated_content
                                    logger.info(f"[内容检查] 流式分析累积内容长度: {len(accumulated_content)}字符")
                                    logger.info(f"[内容预览] 前200字符: {accumulated_content[:200]}")
                                    
                                    # 尝试从内容中提取有用信息，如错误类型和分析结论
                                    error_type = "概念理解错误"  # 默认值
                                    solution = "请参考上述分析"
                                    
                                    # 使用完整的内容作为错误分析
                                    error_analysis = full_content  # 直接使用完整的累积输出作为错误分析内容
                                    
                                    # 简单提取逻辑 - 根据常见的标题或分段提取错误类型
                                    if "错误类型" in full_content and ":" in full_content:
                                        try:
                                            error_type_section = full_content.split("错误类型")[1].split("\n")[0]
                                            if ":" in error_type_section:
                                                error_type = error_type_section.split(":", 1)[1].strip()
                                        except:
                                            logger.warning("无法提取错误类型")
                                    
                                    # 提取正确解法（如果有）
                                    if "正确解法" in full_content or "解题思路" in full_content:
                                        try:
                                            solution_marker = "正确解法" if "正确解法" in full_content else "解题思路"
                                            sections = full_content.split(solution_marker)
                                            if len(sections) > 1:
                                                solution_section = sections[1]
                                                # 尝试查找下一个段落标记
                                                next_section = None
                                                for marker in ["总结", "知识点", "分析", "建议"]:
                                                    if marker in solution_section:
                                                        next_section = solution_section.split(marker)[0]
                                                        break
                                                
                                                if next_section:
                                                    solution = next_section.strip()
                                                else:
                                                    # 如果没有找到下一个段落，取合理的长度
                                                    solution = solution_section[:500].strip()
                                        except:
                                            logger.warning("无法提取正确解法")
                                    
                                    # 从原始请求中提取题目信息
                                    problem_content = ""
                                    knowledge_tags = []
                                    if problems and len(problems) > 0:
                                        problem_content = problems[0].get('content', '') or problems[0].get('problem_content', '')
                                        knowledge_tags = problems[0].get('knowledge_tags', []) or problems[0].get('tags', [])
                                    
                                    # 创建完整的结果对象
                                    choice['result_object'] = {
                                        "problem_id": problem_id,  # 添加problem_id到结果对象
                                        "subject": subject,        # 添加subject到结果对象
                                        "题目类型": problems[0].get('problem_category', '默认题型'),
                                        "具体分支": problems[0].get('problem_subcategory', '默认分支'),
                                        "错误类型": error_type,
                                        "题目原文": problem_content,
                                        "错误分析": error_analysis,  # 使用完整的流式输出内容
                                        "正确解法": solution,
                                        "难度评估": problems[0].get('difficulty', 3),
                                        "知识点标签": knowledge_tags
                                    }
                                    
                                    # 记录完整的分析内容用于调试
                                    logger.info(f"流式分析完成，错误分析内容长度: {len(error_analysis)}")
                                    logger.debug(f"错误分析内容预览: {error_analysis[:200]}...")
                                    
                                    # 添加完整的分析结果到响应中
                                    logger.info("生成了分析结果对象，包含实际提取的内容")
                                    logger.debug(f"结果对象: {json.dumps(choice['result_object'], ensure_ascii=False)}")
                                    
                                    # 直接保存分析结果到数据库
                                    try:
                                        # 直接调用problem_api的update_problem_analysis接口
                                        from api.problem_api import update_problem_analysis_internal
                                        
                                        # 准备保存数据
                                        save_data = {
                                            'problem_id': problem_id,
                                            'analysis_result': choice['result_object']
                                        }
                                        
                                        # 记录保存尝试
                                        logger.info(f"尝试保存分析结果到数据库，问题ID: {problem_id}")
                                        
                                        # 调用内部函数保存数据
                                        save_result = update_problem_analysis_internal(save_data, subject)
                                        
                                        if save_result.get('success', False):
                                            logger.info(f"成功保存分析结果到数据库，问题ID: {problem_id}")
                                            # 添加保存成功信息到流式输出
                                            data_obj['save_result'] = {
                                                'success': True,
                                                'message': '分析结果已自动保存到错题库'
                                            }
                                        else:
                                            logger.error(f"保存分析结果失败: {save_result.get('error', '未知错误')}")
                                            # 添加保存失败信息到流式输出
                                            data_obj['save_result'] = {
                                                'success': False,
                                                'message': f"保存分析结果失败: {save_result.get('error', '未知错误')}"
                                            }
                                    except Exception as save_error:
                                        logger.error(f"保存分析结果时发生异常: {str(save_error)}")
                                        # 添加保存异常信息到流式输出
                                        data_obj['save_result'] = {
                                            'success': False,
                                            'message': f"保存分析结果时发生异常: {str(save_error)}"
                                        }
                                
                                yield f"data: {json.dumps(data_obj, ensure_ascii=False)}\n\n"
                        except Exception as e:
                            logger.error(f"解析流式响应时出错: {str(e)}")
                            logger.error(f"出错的数据: {data[:200]}..." if len(data) > 200 else data)
                            
                            # 构建更友好的错误消息
                            error_msg = f"解析响应出错: {str(e)}"
                            error_data = {
                                'error': error_msg,
                                'choices': [
                                    {
                                        'delta': {
                                            'content': f'**错误:** {error_msg}'
                                        }
                                    }
                                ]
                            }
                            error_output = f"data: {json.dumps(error_data, ensure_ascii=False)}\n\n"
                            logger.info(f"发送错误信息到前端: {error_output[:100]}")
                            yield error_output
                
            except requests.exceptions.Timeout:
                logger.error("DeepSeek API请求超时")
                yield f"data: {json.dumps({'error': '分析请求超时，请稍后重试或减少错题数量'})}\n\n"
                yield "data: [DONE]\n\n"
            except Exception as e:
                logger.error(f"DeepSeek API请求异常: {str(e)}")
                logger.error(traceback.format_exc())
                yield f"data: {json.dumps({'error': f'分析请求发生错误: {str(e)}'})}\n\n"
                yield "data: [DONE]\n\n"
        
        # 返回SSE流式响应
        return Response(
            stream_with_context(generate(problems, problem_id, subject)),
            mimetype='text/event-stream',
            headers={
                'Cache-Control': 'no-cache, no-transform',
                'X-Accel-Buffering': 'no',  # 禁用Nginx缓冲（如果使用）
                'Content-Encoding': 'identity',  # 确保内容不被压缩
                'X-Content-Type-Options': 'nosniff',  # 防止内容嗅探
                'Transfer-Encoding': 'chunked'  # 使用分块传输编码确保立即发送
            }
        )
            
    except Exception as e:
        error_message = f"处理分析请求失败: {str(e)}"
        logger.error(f"{error_message}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": error_message}), 500

# 增强处理JSON转义的函数
def fix_json_escapes(json_str):
    if not json_str:
        return json_str
    
    try:
        # 第一步：修复特定的无效转义序列
        known_invalid_escapes = [
            ('\\(', '('),
            ('\\)', ')'),
            ('\\"', '"'),
            ('\\/', '/'),
            ('\\\\', '\\'),  # 处理双反斜杠
            # 添加更多已知的错误转义
            ('\\，', '，'),
            ('\\。', '。'),
            ('\\、', '、'),
            ('\\；', '；'),
            ('\\：', '：'),
            ('\\？', '？'),
            ('\\！', '！'),
            ('\\（', '（'),
            ('\\）', '）'),
            ('\\《', '《'),
            ('\\》', '》'),
            ('\\【', '【'),
            ('\\】', '】'),
            ('\\—', '—'),
            ('\\…', '…')
        ]
        
        for invalid, valid in known_invalid_escapes:
            json_str = json_str.replace(invalid, valid)
        
        # 第二步：使用正则表达式查找和修复一般的无效转义
        import re
        # 查找所有形如 \x 的转义序列，其中x不是有效转义字符 ('"\/bfnrtu）
        pattern = r'\\([^"\\\\/bfnrtu])'
        json_str = re.sub(pattern, r'\1', json_str)
        
        # 尝试解析JSON来验证是否有效
        json.loads(json_str)
        
        return json_str
        
    except Exception as e:
        print(f"JSON修复失败，将返回空对象: {str(e)}")
        # 如果修复失败，返回一个有效的默认JSON对象
        return json.dumps({
            "题目类型": "未知类型",
            "具体分支": "未知分支",
            "错误类型": "未知错误类型",
            "题目原文": "无法提取题目",
            "错误分析": "无法分析",
            "正确解法": "无法提供解法",
            "难度评估": 3,
            "知识点标签": []
        })

# 使用集中式函数替代此函数
# def get_subject_teacher_prompt(subject='math'):
#     """
#     根据学科获取对应的教师身份提示词
#     
#     Args:
#         subject: 学科名称，默认为'math'
#         
#     Returns:
#         str: 该学科的教师身份提示词
#     """
#     import config
#     
#     # 确保学科名称为小写，避免大小写匹配问题
#     subject = subject.lower() if subject else 'math'
#     
#     # 输出调试日志，用于确认传入的学科参数
#     logger.info(f"获取教师提示词，传入学科: {subject}")
#     logger.info(f"可用学科配置: {list(config.SUBJECT_ANALYSIS_PROMPTS.keys())}")
#     
#     # 默认数学教师提示词（兜底方案）
#     default_prompt = "你是一位专业的数学教师，擅长分析学生的错题模式并提供针对性的学习建议。"
#     
#     # 如果没有提供学科或者学科不在配置中，返回默认提示词
#     if not subject or subject not in config.SUBJECT_ANALYSIS_PROMPTS:
#         logger.warning(f"未找到学科 {subject} 的系统提示词，使用默认数学教师提示词")
#         return default_prompt
#     
#     # 提取该学科的教师身份提示词
#     subject_config = config.SUBJECT_ANALYSIS_PROMPTS.get(subject, {})
#     teacher_prompt = subject_config.get('teacher_prompt', '')
#     
#     # 如果没有找到教师身份提示词，回退到默认值
#     if not teacher_prompt:
#         logger.warning(f"学科 {subject} 没有配置教师身份提示词，使用默认数学教师提示词")
#         return default_prompt
#     
#     logger.info(f"使用学科 {subject} 的教师身份提示词: {teacher_prompt}")
#     return teacher_prompt
