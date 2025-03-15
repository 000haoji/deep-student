"""
错题相关API
"""
import os
import uuid
import datetime
import traceback
import logging
import json
from flask import Blueprint, request, jsonify, send_from_directory, current_app
from werkzeug.utils import secure_filename
from core import ai_analysis, database
from models import ErrorProblem
from config import UPLOAD_FOLDER, ALLOWED_EXTENSIONS

logger = logging.getLogger(__name__)

problem_api = Blueprint('problem_api', __name__)

def allowed_file(filename):
    return '.' in filename and \
           filename.rsplit('.', 1)[1].lower() in ALLOWED_EXTENSIONS

@problem_api.route('/api/upload', methods=['POST'])
def upload_error():
    """上传错题图片并分析"""
    if 'file' not in request.files:
        return jsonify({"error": "未找到文件部分"}), 400
    
    file = request.files['file']
    if file.filename == '':
        return jsonify({"error": "未选择文件"}), 400
    
    # 获取模型类型参数，默认使用OpenAI兼容API
    model_type = request.form.get('model_type', 'openai')
    logger.info(f"使用模型类型: {model_type}")
    
    # 获取学科参数，默认为"math"
    subject = request.form.get('subject', 'math')
    logger.info(f"错题学科: {subject}")
    
    # 如果是多模态+R1组合模式，则获取多模态模型类型
    multimodal_model = None
    if model_type.startswith('multimodal_'):
        parts = model_type.split('_')
        if len(parts) > 1:
            multimodal_model = parts[1]  # 例如从"multimodal_gpt4v"中提取"gpt4v"
            model_type = "multimodal"  # 将model_type设置为通用的"multimodal"
    
    if file and allowed_file(file.filename):
        try:
            # 生成唯一ID
            problem_id = str(uuid.uuid4())
            
            # 保存图片
            filename = secure_filename(f"{problem_id}_{file.filename}")
            filepath = os.path.join(UPLOAD_FOLDER, filename)
            file.save(filepath)
            
            logger.info(f"已保存图片至 {filepath}")
            
            # 获取用户补充说明
            notes = request.form.get('notes', '')
            
            # 根据模型类型选择分析函数
            logger.info(f"开始使用 {model_type} 分析图片...")
            analysis_result = analyze_problem_image(filepath, model_type, notes, subject)
            
            # 检查是否有错误
            if "error" in analysis_result:
                logger.error(f"分析出错: {analysis_result['error']}")
                return jsonify({
                    "success": False,
                    "error": analysis_result.get('error', '分析失败'),
                    "details": analysis_result
                })
            
            # 构建ErrorProblem对象
            problem = ErrorProblem(
                id=problem_id,
                image_path=filepath,
                problem_content=analysis_result.get('题目原文', ''),
                error_analysis=analysis_result.get('错误分析', ''),
                problem_category=analysis_result.get('题目类型', '未知'),
                problem_subcategory=analysis_result.get('具体分支', '未知'),
                error_type=analysis_result.get('错误类型', '未知'),
                difficulty=analysis_result.get('难度评估', 3),
                correct_solution=analysis_result.get('正确解法', ''),
                tags=analysis_result.get('知识点标签', []),
                created_at=datetime.datetime.now().isoformat(),
                notes=notes,  # 添加 notes 字段
                subject=subject  # 添加 subject 字段
            )
            
            # 保存到数据库
            if database.save_error_problem(problem):
                return jsonify({
                    "success": True,
                    "problem_id": problem_id,
                    "analysis": analysis_result
                })
            else:
                return jsonify({
                    "success": False,
                    "error": "保存到数据库失败"
                }), 500
        except Exception as e:
            error_msg = f"上传处理过程中出错: {str(e)}"
            stack_trace = traceback.format_exc()
            logger.error(f"{error_msg}\n{stack_trace}")
            
            return jsonify({
                "success": False,
                "error": error_msg,
                "stack_trace": stack_trace
            }), 500
    
    return jsonify({"error": "不支持的文件类型"}), 400

@problem_api.route('/api/upload-multi', methods=['POST'])
def upload_multiple_files():
    """处理批量上传错题图片并分析"""
    if 'files' not in request.files:
        return jsonify({"success": False, "error": "未找到文件部分"}), 400
    
    files = request.files.getlist('files')
    if not files or all(f.filename == '' for f in files):
        return jsonify({"success": False, "error": "未选择任何文件"}), 400
    
    # 获取模型类型参数
    model_type = request.form.get('model_type', 'openai')
    logger.info(f"批量分析使用模型类型: {model_type}")
    
    # 获取学科参数，默认为"math"
    subject = request.form.get('subject', 'math')
    logger.info(f"错题学科: {subject}")
    
    # 获取用户补充说明
    notes = request.form.get('notes', '')
    
    try:
        # 生成唯一问题ID
        problem_id = str(uuid.uuid4())
        
        # 保存所有图片
        saved_files = []
        for file in files:
            if file and allowed_file(file.filename):
                # 对每个文件使用相同的问题ID，但保留原始文件名以区分
                filename = secure_filename(f"{problem_id}_{file.filename}")
                filepath = os.path.join(UPLOAD_FOLDER, filename)
                file.save(filepath)
                saved_files.append({
                    "original_name": file.filename,
                    "saved_path": filepath
                })
                logger.info(f"已保存图片: {filepath}")
        
        if not saved_files:
            return jsonify({"success": False, "error": "未能保存任何有效的图片文件"}), 400
        
        # 使用第一张图片作为主分析对象
        primary_image_path = saved_files[0]["saved_path"]
        
        # 如果有多张图片，可以考虑这些情况：
        # 1. 第一张图片是题目，后续图片是解答或补充
        # 2. 所有图片是同一题目的不同部分
        # 这里我们采用策略1，并在notes中添加指示
        if len(saved_files) > 1:
            extra_notes = f"\n\n注意：用户提供了{len(saved_files)}张图片。第一张是主要题目，其余可能包含答案或解析。"
            if notes:
                notes += extra_notes
            else:
                notes = extra_notes
        
        # 分析图片
        logger.info(f"开始使用 {model_type} 分析多图片问题...")
        analysis_result = analyze_problem_image(primary_image_path, model_type, notes, subject)
        
        # 检查是否有错误
        if "error" in analysis_result:
            logger.error(f"分析出错: {analysis_result['error']}")
            return jsonify({
                "success": False,
                "error": analysis_result.get('error', '分析失败'),
                "details": analysis_result
            })
        
        # 构建ErrorProblem对象，包括所有图片路径
        problem = ErrorProblem(
            id=problem_id,
            image_path=primary_image_path,  # 主图片路径
            additional_images=[f["saved_path"] for f in saved_files[1:]],  # 附加图片路径
            problem_content=analysis_result.get('题目原文', f"[包含{len(saved_files)}张图片的错题]"),  # 使用OCR提取的题目原文
            error_analysis=analysis_result.get('错误分析', ''),
            problem_category=analysis_result.get('题目类型', '未知'),
            problem_subcategory=analysis_result.get('具体分支', '未知'),
            error_type=analysis_result.get('错误类型', '未知'),
            difficulty=analysis_result.get('难度评估', 3),
            correct_solution=analysis_result.get('正确解法', ''),
            tags=analysis_result.get('知识点标签', []),
            created_at=datetime.datetime.now().isoformat(),
            notes=notes,  # 保存用户补充说明
            subject=subject  # 添加 subject 字段
        )
        
        # 保存到数据库
        if database.save_error_problem_with_multiple_images(problem):
            return jsonify({
                "success": True,
                "problem_id": problem_id,
                "analysis": analysis_result
            })
        else:
            return jsonify({
                "success": False,
                "error": "保存到数据库失败"
            }), 500
            
    except Exception as e:
        logger.error(f"批量上传处理过程中出错: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

@problem_api.route('/api/problems', methods=['GET'])
def get_problems():
    """获取所有错题，可按学科过滤"""
    try:
        # 从请求参数中获取学科，默认为"math"
        subject = request.args.get('subject', 'math')
        
        # 获取错题列表，传入学科参数
        problems = database.get_error_problems(subject)
        return jsonify(problems)
    except Exception as e:
        logger.error(f"获取错题列表失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@problem_api.route('/api/problem/<problem_id>', methods=['GET'])
def get_problem_detail(problem_id):
    """获取错题详情"""
    try:
        logger.info(f"请求错题详情: {problem_id}")
        
        # 获取学科参数
        subject = request.args.get('subject', 'math')
        logger.info(f"请求学科: {subject}")
        
        # 调用数据库函数，传入学科参数
        problem = database.get_error_problem_by_id(problem_id, subject=subject)
        
        if not problem:
            logger.error(f"未找到指定的错题ID: {problem_id}")
            return jsonify({"error": "未找到指定的错题"}), 404
        
        # 构建图片URL
        image_url = f"/uploads/{os.path.basename(problem['image_path'])}"
        problem['image_url'] = image_url
        
        # 确保所有必要字段存在且有有效值
        required_fields = {
            'problem_content': '',
            'error_analysis': '',
            'problem_category': '未知',
            'problem_subcategory': '未知分支',
            'error_type': '未知',
            'difficulty': 3,
            'correct_solution': '',
            'notes': '',
            'typicality': 3
        }
        
        for field, default_value in required_fields.items():
            if field not in problem or problem[field] is None:
                problem[field] = default_value
                
        # 确保tags和additional_images字段是列表
        if isinstance(problem['tags'], str):
            try:
                problem['tags'] = json.loads(problem['tags'])
            except:
                problem['tags'] = []
        elif problem['tags'] is None:
            problem['tags'] = []
                
        if isinstance(problem['additional_images'], str):
            try:
                problem['additional_images'] = json.loads(problem['additional_images'])
            except:
                problem['additional_images'] = []
        elif problem['additional_images'] is None:
            problem['additional_images'] = []
        
        logger.info(f"成功获取错题详情: {problem_id}")
        return jsonify(problem)
    except Exception as e:
        logger.error(f"获取错题详情失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": f"获取错题详情失败: {str(e)}"}), 500

@problem_api.route('/api/problem/<problem_id>', methods=['DELETE'])  
def delete_problem(problem_id):
    """删除错题"""
    try:
        # 获取学科参数
        subject = request.args.get('subject', 'math')
        
        # 查询错题是否存在
        problem = database.get_error_problem_by_id(problem_id, subject=subject)
        if not problem:
            return jsonify({"error": "未找到指定的错题"}), 404
        
        # 从数据库中删除
        conn = database.sqlite3.connect(database.DATABASE_PATH)
        cursor = conn.cursor()
        cursor.execute("DELETE FROM error_problems WHERE id = ?", (problem_id,))
        conn.commit()
        conn.close()
        
        # 尝试删除图片文件
        try:
            if os.path.exists(problem['image_path']):
                os.remove(problem['image_path'])
        except Exception as file_e:
            logger.warning(f"删除错题图片失败: {str(file_e)}")
        
        return jsonify({"success": True, "message": "错题已删除"})
    except Exception as e:
        logger.error(f"删除错题失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@problem_api.route('/api/problem/<problem_id>', methods=['PUT'])
def update_problem(problem_id):
    """更新错题信息"""
    try:
        # 获取学科参数
        subject = request.args.get('subject', 'math')
        
        # 查询错题是否存在
        problem = database.get_error_problem_by_id(problem_id, subject=subject)
        if not problem:
            return jsonify({"error": "未找到指定的错题"}), 404
        
        # 获取更新数据
        data = request.json
        if not data:
            return jsonify({"error": "未提供更新数据"}), 400
        
        # 可更新的字段
        updatable_fields = [
            'problem_content', 'error_analysis', 'problem_category',
            'problem_subcategory', 'error_type', 'difficulty',
            'correct_solution', 'tags'
        ]
        
        # 构建更新语句
        update_fields = []
        update_values = []
        
        for field in updatable_fields:
            if field in data:
                if field == 'tags' and isinstance(data[field], list):
                    # 处理标签列表，转为JSON字符串
                    update_fields.append(f"{field} = ?")
                    update_values.append(database.json.dumps(data[field], ensure_ascii=False))
                else:
                    update_fields.append(f"{field} = ?")
                    update_values.append(data[field])
        
        if not update_fields:
            return jsonify({"error": "没有提供有效的更新字段"}), 400
        
        # 执行更新
        conn = database.sqlite3.connect(database.DATABASE_PATH)
        cursor = conn.cursor()
        
        query = f"UPDATE error_problems SET {', '.join(update_fields)} WHERE id = ?"
        update_values.append(problem_id)
        
        cursor.execute(query, update_values)
        conn.commit()
        conn.close()
        
        # 获取更新后的数据
        updated_problem = database.get_error_problem_by_id(problem_id)
        
        return jsonify({
            "success": True,
            "message": "错题信息已更新",
            "problem": updated_problem
        })
    except Exception as e:
        logger.error(f"更新错题信息失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@problem_api.route('/api/problem/<problem_id>/typicality', methods=['POST'])
def update_typicality(problem_id):
    """更新错题典型度评分"""
    try:
        data = request.json
        if not data or 'typicality' not in data:
            return jsonify({"error": "请提供典型度评分"}), 400
            
        typicality = data['typicality']
        # 验证评分范围
        if not isinstance(typicality, int) or typicality < 1 or typicality > 5:
            return jsonify({"error": "典型度评分必须是1-5之间的整数"}), 400
            
        if database.update_problem_typicality(problem_id, typicality):
            return jsonify({
                "success": True,
                "message": "典型度评分已更新",
                "typicality": typicality
            })
        else:
            return jsonify({"error": "更新典型度评分失败"}), 500
    except Exception as e:
        logger.error(f"更新典型度评分失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@problem_api.route('/api/problems/batch-delete', methods=['POST'])
def batch_delete_problems():
    """批量删除错题"""
    try:
        data = request.json
        if not data or 'problem_ids' not in data or not data['problem_ids']:
            return jsonify({"error": "请提供要删除的错题ID列表"}), 400
            
        problem_ids = data['problem_ids']
        
        if database.delete_problems(problem_ids):
            return jsonify({
                "success": True,
                "message": f"已删除 {len(problem_ids)} 道错题",
                "deleted_count": len(problem_ids)
            })
        else:
            return jsonify({"error": "批量删除错题失败"}), 500
    except Exception as e:
        logger.error(f"批量删除错题失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@problem_api.route('/api/problem/<problem_id>/update_image', methods=['POST'])
def update_problem_image(problem_id):
    """更新错题图片"""
    try:
        # 检查是否有图片文件
        if 'image' not in request.files:
            return jsonify({
                'success': False,
                'error': '没有提供图片文件'
            }), 400
            
        image_file = request.files['image']
        
        # 检查文件是否有效
        if image_file.filename == '':
            return jsonify({
                'success': False,
                'error': '未选择图片文件'
            }), 400
            
        # 检查文件类型
        if not allowed_file(image_file.filename):
            return jsonify({
                'success': False,
                'error': '不支持的文件类型，请上传jpg、jpeg、png或gif格式的图片'
            }), 400
            
        # 生成安全的文件名
        filename = secure_filename(image_file.filename)
        # 添加UUID前缀确保唯一性
        unique_filename = f"{uuid.uuid4().hex}-{filename}"
        
        # 确保上传目录存在
        os.makedirs(UPLOAD_FOLDER, exist_ok=True)
        
        # 保存文件
        file_path = os.path.join(UPLOAD_FOLDER, unique_filename)
        image_file.save(file_path)
        
        # 更新数据库中的图片路径
        conn = database.get_db()
        cursor = conn.cursor()
        
        # 检查问题是否存在
        cursor.execute(
            "SELECT id FROM error_problems WHERE id = ?",
            (problem_id,)
        )
        
        if cursor.fetchone() is None:
            conn.close()
            # 如果问题不存在，删除已上传的文件
            if os.path.exists(file_path):
                os.remove(file_path)
            return jsonify({
                'success': False,
                'error': '更新图片路径失败，问题ID不存在'
            }), 404
        
        # 更新图片路径
        cursor.execute(
            "UPDATE error_problems SET image_path = ? WHERE id = ?",
            (file_path, problem_id)
        )
        
        conn.commit()
        conn.close()
        
        return jsonify({
            'success': True,
            'message': '图片更新成功',
            'image_path': file_path
        })
            
    except Exception as e:
        # 记录错误
        print(f"更新错题图片时出错: {str(e)}")
        import traceback
        traceback.print_exc()
        
        return jsonify({
            'success': False,
            'error': f'服务器错误: {str(e)}'
        }), 500

@problem_api.route('/api/problem/update-analysis', methods=['POST'])
def update_problem_analysis():
    """
    更新错题分析结果
    
    请求体格式:
    ```json
    {
        "problem_id": "问题ID",
        "analysis_result": {
            "题目类型": "...",
            "具体分支": "...",
            "错误类型": "...",
            "题目原文": "...",
            "错误分析": "...",
            "正确解法": "...",
            "难度评估": 3,
            "知识点标签": ["标签1", "标签2"]
        }
    }
    """
    # 获取请求数据
    data = request.json
    
    if not data:
        logger.error("未提供更新数据")
        return jsonify({"success": False, "error": "未提供更新数据"}), 400
    
    # 检查必要字段
    if 'problem_id' not in data or 'analysis_result' not in data:
        logger.error("缺少必要字段：需要problem_id和analysis_result")
        return jsonify({
            "success": False, 
            "error": "缺少必要字段：需要problem_id和analysis_result"
        }), 400
    
    # 获取学科参数，默认为math
    subject = request.args.get('subject', 'math')
    
    # 调用内部函数处理数据
    result = update_problem_analysis_internal(data, subject)
    
    # 处理返回结果
    if result.get('success', False):
        return jsonify(result), 200
    else:
        return jsonify(result), 400

def update_problem_analysis_internal(data, subject='math'):
    """
    内部函数：更新错题分析结果
    
    参数:
        data (dict): 包含problem_id和analysis_result的字典
        subject (str): 学科，默认为math
        
    返回值:
        dict: 包含操作结果的字典，格式为 {'success': bool, 'error': str, 'data': any}
    """
    try:
        # 验证数据
        if not data:
            logger.error("未提供更新数据")
            return {"success": False, "error": "未提供更新数据"}
        
        # 检查必要字段
        if 'problem_id' not in data or 'analysis_result' not in data:
            logger.error("缺少必要字段：需要problem_id和analysis_result")
            return {"success": False, "error": "缺少必要字段：需要problem_id和analysis_result"}
        
        problem_id = data['problem_id']
        analysis_result = data['analysis_result']
        
        # 查询错题是否存在
        problem = database.get_error_problem_by_id(problem_id, subject=subject)
        if not problem:
            logger.error(f"未找到错题ID：{problem_id}")
            return {"success": False, "error": f"未找到错题ID：{problem_id}"}
        
        # 构建更新数据
        update_data = {}
        
        # 映射分析结果字段到数据库字段
        field_mapping = {
            '题目类型': 'problem_category',
            '具体分支': 'problem_subcategory',
            '错误类型': 'error_type',
            '题目原文': 'problem_content',
            '错误分析': 'error_analysis',
            '正确解法': 'correct_solution',
            '难度评估': 'difficulty',
            '知识点标签': 'tags'
        }
        
        # 构建更新字段
        for json_field, db_field in field_mapping.items():
            if json_field in analysis_result and analysis_result[json_field] is not None:
                # 特殊处理标签字段
                if json_field == '知识点标签':
                    if isinstance(analysis_result[json_field], list):
                        update_data[db_field] = analysis_result[json_field]
                    elif isinstance(analysis_result[json_field], str):
                        # 尝试解析字符串形式的标签
                        try:
                            parsed_tags = json.loads(analysis_result[json_field])
                            if isinstance(parsed_tags, list):
                                update_data[db_field] = parsed_tags
                        except:
                            # 如果无法解析，尝试用逗号拆分
                            tags = []
                            for sep in [',', '，']:
                                if sep in analysis_result[json_field]:
                                    tags = analysis_result[json_field].split(sep)
                                    break
                            if not tags:
                                tags = [analysis_result[json_field]]  # 如果没有分隔符，当作单个标签
                            update_data[db_field] = [tag.strip() for tag in tags if tag.strip()]
                else:
                    update_data[db_field] = analysis_result[json_field]
        
        # 如果没有有效的更新字段，返回错误
        if not update_data:
            logger.warning("没有提供有效的分析结果字段")
            return {"success": False, "error": "没有提供有效的分析结果字段"}
        
        # 执行更新
        conn = database.sqlite3.connect(database.DATABASE_PATH)
        cursor = conn.cursor()
        
        update_fields = []
        update_values = []
        
        for field, value in update_data.items():
            if field == 'tags' and isinstance(value, list):
                # 处理标签列表，转为JSON字符串
                update_fields.append(f"{field} = ?")
                update_values.append(json.dumps(value, ensure_ascii=False))
            else:
                update_fields.append(f"{field} = ?")
                update_values.append(value)
        
        # 添加更新时间
        update_fields.append("updated_at = ?")
        update_values.append(datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S"))
        
        query = f"UPDATE error_problems SET {', '.join(update_fields)} WHERE id = ?"
        update_values.append(problem_id)
        
        cursor.execute(query, update_values)
        conn.commit()
        conn.close()
        
        # 记录成功信息
        logger.info(f"成功更新错题分析结果，ID: {problem_id}")
        
        # 获取更新后的数据
        updated_problem = database.get_error_problem_by_id(problem_id, subject=subject)
        
        return {
            "success": True, 
            "message": "成功更新错题分析结果",
            "data": {
                "problem_id": problem_id,
                "updated_fields": list(update_data.keys())
            }
        }
        
    except Exception as e:
        error_message = f"更新错题分析结果时出错: {str(e)}"
        logger.error(error_message)
        return {"success": False, "error": error_message}

def analyze_problem_image(image_path, model_type, notes='', subject='math'):
    """根据图片和模型类型进行分析
    
    Args:
        image_path (str): 图片路径
        model_type (str): 模型类型
        notes (str): 用户补充说明，新增参数
        subject (str): 学科参数，影响分析prompt模板，默认为math
    
    Returns:
        dict: 分析结果
    """
    logger.info(f"使用模型类型: {model_type}")
    logger.info(f"分析学科: {subject}")
    logger.info(f"已保存图片到 {image_path}")
    
    try:
        # 根据选择的模型类型进行分析
        if model_type == 'openai':
            logger.info("开始使用 openai 分析图片...")
            logger.info(f"用户补充说明: {notes if notes else '无'}")  # 添加日志记录用户补充
            analysis_result = ai_analysis.analyze_with_openai_compat_api(image_path, notes, subject)
        elif model_type == 'gemini':
            logger.info("开始使用 gemini 分析图片...")
            logger.info(f"用户补充说明: {notes if notes else '无'}")
            analysis_result = ai_analysis.analyze_with_gemini_api(image_path, notes, subject)
        elif model_type == 'ocr_r1':
            logger.info("开始使用 OCR+R1 组合模式分析图片...")
            logger.info(f"用户补充说明: {notes if notes else '无'}")
            analysis_result = ai_analysis.analyze_with_ocr_and_deepseek(image_path, notes, subject)
        elif model_type.startswith('multimodal_'):
            # 提取真正的模型类型
            mm_type = model_type.replace('multimodal_', '')
            logger.info(f"开始使用 multimodal_{mm_type} 分析图片...")
            logger.info(f"用户补充说明: {notes if notes else '无'}")
            analysis_result = ai_analysis.analyze_with_multimodal_and_deepseek(image_path, mm_type, notes, subject)
        else:
            logger.warning(f"未知模型类型: {model_type}，使用默认模型 openai")
            analysis_result = ai_analysis.analyze_with_openai_compat_api(image_path, notes, subject)
            
        # 添加用户补充说明
        if notes and isinstance(analysis_result, dict):
            analysis_result['用户补充'] = notes
            
        return analysis_result
        
    except Exception as e:
        logger.error(f"分析错误: {str(e)}")
        return {
            "error": str(e),
            "题目原文": "分析出错",
            "题目类型": "未知",
            "具体分支": "未知",
            "错误类型": "API错误",
            "错误分析": f"图像分析过程中发生错误: {str(e)}。请尝试其他模型或稍后重试。",
            "正确解法": "无法提供",
            "难度评估": 1,
            "知识点标签": ["未知"]
        }
