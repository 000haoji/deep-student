"""
回顾相关API
"""
import uuid
import datetime
import traceback
import logging
import json
from flask import Blueprint, request, jsonify
from core import ai_analysis, database
from models import ReviewSession
import os
from config import UPLOAD_FOLDER
from core.database import get_db
from datetime import datetime, timedelta

logger = logging.getLogger(__name__)

review_api = Blueprint('review_api', __name__)

@review_api.route('/api/review', methods=['POST'])
def create_review():
    """创建回顾分析"""
    try:
        # 获取请求数据
        data = request.json
        problem_ids = data.get('problem_ids', [])
        
        if not problem_ids:
            return jsonify({"error": "未提供错题ID列表"}), 400
        
        # 生成唯一回顾ID
        review_id = str(uuid.uuid4())
        
        # 获取所有错题详情
        problems_data = []
        for pid in problem_ids:
            problem = database.get_error_problem_by_id(pid)
            if problem:
                problems_data.append(problem)
        
        if not problems_data:
            return jsonify({"error": "未找到任何有效的错题"}), 400
        
        # 调用文本API进行回顾分析
        review_analysis = ai_analysis.analyze_with_text_api(problems_data)
        
        # 构建ReviewSession对象
        review = ReviewSession(
            id=review_id,
            problems_included=problem_ids,
            review_analysis=review_analysis,
            improvement_strategy=None,
            created_at=datetime.now().isoformat()
        )
        
        # 保存回顾记录到数据库
        if database.save_review_session(review):
            return jsonify({
                "success": True,
                "review_id": review_id
            })
        else:
            return jsonify({
                "success": False,
                "error": "保存回顾记录失败"
            }), 500
    except Exception as e:
        logger.error(f"创建回顾分析失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@review_api.route('/api/reviews', methods=['GET'])
def get_reviews():
    """获取所有回顾记录"""
    try:
        reviews = database.get_review_sessions()
        return jsonify(reviews)
    except Exception as e:
        logger.error(f"获取回顾列表失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"error": str(e)}), 500

@review_api.route('/api/review/<review_id>', methods=['GET'])
def get_review_detail(review_id):
    """获取回顾详情"""
    try:
        review = database.get_review_session_by_id(review_id)
        if not review:
            return jsonify({"success": False, "error": "未找到指定的回顾记录"}), 404
        
        # 获取所有相关错题
        problem_ids = []
        
        # 处理problems_included字段
        if isinstance(review['problems_included'], str):
            try:
                problem_ids = json.loads(review['problems_included'])
            except:
                problem_ids = []
        elif isinstance(review['problems_included'], list):
            problem_ids = review['problems_included']
        
        problems = []
        
        for pid in problem_ids:
            problem = database.get_error_problem_by_id(pid)
            if problem:
                # 构建图片URL
                image_url = f"/uploads/{os.path.basename(problem['image_path'])}" if 'image_path' in problem and problem['image_path'] else ""
                problem['image_url'] = image_url  # 添加 image_url 字段
                problems.append(problem)
        
        # 返回成功状态和结果数据
        return jsonify({
            "success": True,
            "review": review,
            "problems": problems
        })
    except Exception as e:
        logger.error(f"获取回顾详情失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

@review_api.route('/api/review/<review_id>', methods=['DELETE'])
def delete_review(review_id):
    """删除回顾记录"""
    try:
        logger.info(f"删除回顾记录请求: id={review_id}")
        
        # 使用database模块中的删除功能
        success = database.delete_review_session(review_id)
        
        if success:
            logger.info(f"成功删除回顾记录: {review_id}")
            return jsonify({
                'success': True,
                'message': '删除成功'
            })
        else:
            logger.warning(f"删除回顾记录失败，记录不存在: {review_id}")
            return jsonify({
                'success': False,
                'message': '删除失败，找不到对应的记录'
            }), 404
            
    except Exception as e:
        logger.error(f"删除回顾记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({'success': False, 'error': str(e)}), 500

@review_api.route('/api/review/stream-analysis/<review_id>', methods=['POST'])
def stream_review_analysis(review_id):
    """
    回顾详情流式分析API
    复用主页的流式分析逻辑，确保与主页分析保持一致
    """
    try:
        # 获取回顾记录
        review = database.get_review_session_by_id(review_id)
        if not review:
            return jsonify({"success": False, "error": "未找到指定的回顾记录"}), 404
        
        # 获取所有相关错题
        problem_ids = []
        
        # 处理problems_included字段
        if isinstance(review['problems_included'], str):
            try:
                problem_ids = json.loads(review['problems_included'])
            except:
                problem_ids = []
        elif isinstance(review['problems_included'], list):
            problem_ids = review['problems_included']
        
        problems = []
        
        for pid in problem_ids:
            problem = database.get_error_problem_by_id(pid)
            if problem:
                # 构建图片URL
                image_url = f"/uploads/{os.path.basename(problem['image_path'])}" if 'image_path' in problem and problem['image_path'] else ""
                problem['image_url'] = image_url  # 添加 image_url 字段
                problems.append(problem)
        
        if not problems:
            return jsonify({"success": False, "error": "未找到相关错题"}), 404
        
        # 准备问题数据，确保格式与stream_analysis期望的一致
        formatted_problems = []
        for problem in problems:
            formatted_problem = {
                'id': problem.get('id', ''),
                'problem_content': problem.get('problem_content', ''),
                'content': problem.get('problem_content', ''),  # 兼容性字段
                'title': problem.get('title', f'错题{problem.get("id", "")}'),
                'user_comments': problem.get('notes', ''),
                'error_cause': problem.get('error_type', ''),
                'knowledge_tags': problem.get('knowledge_tags', []) or problem.get('tags', []),
                'problem_category': problem.get('problem_category', ''),
                'problem_subcategory': problem.get('problem_subcategory', '')
            }
            formatted_problems.append(formatted_problem)
        
        # 导入流式分析API，但不直接修改request.json
        from flask import current_app
        from api.stream_analysis_api import stream_analysis_with_data
        
        # 准备分析数据
        analysis_data = {
            "problems": formatted_problems,
            "subject": "math",  # 默认学科
            "review_id": review_id,  # 添加回顾ID
            "is_review_analysis": True  # 标记这是回顾分析请求
        }
        
        # 调用流式分析API的内部函数，传入准备好的数据
        return stream_analysis_with_data(analysis_data)
    except Exception as e:
        logger.error(f"回顾流式分析失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

@review_api.route('/api/tags/tree', methods=['GET'])
def get_tags_tree():
    """获取所有错题标签树"""
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 首先检查problems表是否存在knowledge_tags列
        try:
            cursor.execute("PRAGMA table_info(problems)")
            columns = [info[1] for info in cursor.fetchall()]
            
            if 'knowledge_tags' not in columns:
                # 如果不存在knowledge_tags列，则使用tags列
                tag_column = 'tags'
            else:
                tag_column = 'knowledge_tags'
                
            # 检查problems表是否存在数据
            cursor.execute(f"SELECT COUNT(*) FROM problems")
            if cursor.fetchone()[0] == 0:
                # 如果没有数据，则尝试从error_problems表获取
                cursor.execute("""
                    SELECT id, tags FROM error_problems
                    WHERE tags IS NOT NULL AND tags != ''
                """)
            else:
                # 查询所有错题的知识点标签
                cursor.execute(f"""
                    SELECT id, {tag_column} FROM problems
                    WHERE {tag_column} IS NOT NULL AND {tag_column} != ''
                """)
        except Exception as e:
            # 如果problems表不存在，则尝试从error_problems表获取
            logger.warning(f"尝试查询problems表失败: {str(e)}，将从error_problems表获取数据")
            cursor.execute("""
                SELECT id, tags FROM error_problems
                WHERE tags IS NOT NULL AND tags != ''
            """)
        
        results = cursor.fetchall()
        
        # 标签统计和层级分析
        tag_counter = {}  # 记录每个标签出现的次数
        categories = {}   # 一级分类
        
        for problem_id, tags_json in results:
            try:
                if not tags_json:
                    continue
                    
                tags = json.loads(tags_json)
                if not isinstance(tags, list):
                    continue
                    
                for tag in tags:
                    # 统计标签出现次数
                    if tag in tag_counter:
                        tag_counter[tag] += 1
                    else:
                        tag_counter[tag] = 1
                    
                    # 处理层级关系
                    parts = tag.split('-') if '-' in tag else tag.split('：') if '：' in tag else [tag]
                    
                    if len(parts) > 1:
                        # 有层级关系
                        category = parts[0].strip()
                        subcategory = parts[1].strip()
                        
                        if category not in categories:
                            categories[category] = {'count': 0, 'subcategories': {}}
                        
                        categories[category]['count'] += 1
                        
                        if subcategory not in categories[category]['subcategories']:
                            categories[category]['subcategories'][subcategory] = {'count': 0, 'tags': {}}
                        
                        categories[category]['subcategories'][subcategory]['count'] += 1
                        
                        if len(parts) > 2:
                            # 有三级分类
                            tag_name = parts[2].strip()
                            if tag_name not in categories[category]['subcategories'][subcategory]['tags']:
                                categories[category]['subcategories'][subcategory]['tags'][tag_name] = 0
                            categories[category]['subcategories'][subcategory]['tags'][tag_name] += 1
                    else:
                        # 没有明确层级，作为一级类别
                        category = parts[0].strip()
                        if category not in categories:
                            categories[category] = {'count': 0, 'subcategories': {}}
                        categories[category]['count'] += 1
            except Exception as e:
                logger.error(f"处理问题 {problem_id} 的标签时出错: {str(e)}")
                continue
        
        # 构建标签树
        tag_tree = []
        for category, category_data in categories.items():
            category_node = {
                'id': f"cat_{category}",
                'text': f"{category} ({category_data['count']})",
                'count': category_data['count'],
                'children': []
            }
            
            # 添加子类别
            for subcategory, subcategory_data in category_data['subcategories'].items():
                subcategory_node = {
                    'id': f"subcat_{category}_{subcategory}",
                    'text': f"{subcategory} ({subcategory_data['count']})",
                    'count': subcategory_data['count'],
                    'children': []
                }
                
                # 添加三级标签
                for tag, count in subcategory_data.get('tags', {}).items():
                    subcategory_node['children'].append({
                        'id': f"tag_{category}_{subcategory}_{tag}",
                        'text': f"{tag} ({count})",
                        'count': count
                    })
                
                # 如果没有三级标签，子类别作为叶子节点
                if not subcategory_node['children']:
                    subcategory_node.pop('children')
                
                category_node['children'].append(subcategory_node)
            
            # 如果没有子类别，一级类别作为叶子节点
            if not category_node['children']:
                category_node.pop('children')
            
            tag_tree.append(category_node)
        
        # 按照一级类别中的问题数量排序
        tag_tree.sort(key=lambda x: x['count'], reverse=True)
        
        # 统计总数据
        total_tags = len(tag_counter)
        total_problems = len(results)
        
        return jsonify({
            'success': True,
            'tag_tree': tag_tree,
            'stats': {
                'total_tags': total_tags,
                'total_tagged_problems': total_problems,
                'top_tags': sorted(tag_counter.items(), key=lambda x: x[1], reverse=True)[:10]
            }
        })
    except Exception as e:
        logger.error(f"获取标签树失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            'success': False,
            'error': f"获取标签树失败: {str(e)}"
        }), 500

@review_api.route('/api/review-plans', methods=['POST'])
def create_review_plan():
    """创建复习计划"""
    try:
        data = request.get_json()
        
        # 必须参数
        title = data.get('title')
        if not title:
            return jsonify({
                'success': False,
                'error': '请提供复习计划标题'
            }), 400
            
        # 可选参数
        description = data.get('description', '')
        tags = data.get('tags', [])
        categories = data.get('categories', [])
        start_date = data.get('start_date')
        end_date = data.get('end_date')
        
        # 确保标签和分类是列表
        if isinstance(tags, str):
            tags = [tags]
        if isinstance(categories, str):
            categories = [categories]
            
        # 将标签和分类转换为JSON
        tags_json = json.dumps(tags)
        categories_json = json.dumps(categories)
        
        # 插入数据库
        conn = get_db()
        cursor = conn.cursor()
        
        # 检查reviews表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
        if not cursor.fetchone():
            # 如果reviews表不存在，创建它
            logger.warning("reviews表不存在，正在创建...")
            cursor.execute('''
            CREATE TABLE IF NOT EXISTS reviews (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                description TEXT,
                tags TEXT,
                categories TEXT,
                start_date TEXT,
                end_date TEXT,
                created_at TEXT,
                updated_at TEXT
            )
            ''')
            conn.commit()
        
        cursor.execute("""
            INSERT INTO reviews (title, description, tags, categories, start_date, end_date, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
        """, (
            title,
            description,
            tags_json,
            categories_json,
            start_date,
            end_date,
            datetime.now().strftime('%Y-%m-%d %H:%M:%S')
        ))
        
        review_id = cursor.lastrowid
        conn.commit()
        
        return jsonify({
            'success': True,
            'message': '复习计划创建成功',
            'review_id': review_id
        })
    except Exception as e:
        logger.error(f"创建复习计划失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({
            'success': False,
            'error': f"创建复习计划失败: {str(e)}"
        }), 500
        
@review_api.route('/api/review-plans', methods=['GET'])
def get_review_plans():
    """获取复习计划列表"""
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 检查表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
        table_exists = cursor.fetchone()
        
        if not table_exists:
            logger.warning("reviews表不存在，正在创建...")
            cursor.execute('''
            CREATE TABLE IF NOT EXISTS reviews (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                title TEXT NOT NULL,
                description TEXT,
                tags TEXT,
                categories TEXT,
                start_date TEXT,
                end_date TEXT,
                created_at TEXT,
                updated_at TEXT
            )
            ''')
            conn.commit()
            
            # 返回空列表，因为新表没有数据
            return jsonify({
                'success': True,
                'reviews': []
            })
        
        # 检查表结构是否包含title列
        cursor.execute("PRAGMA table_info(reviews)")
        columns = cursor.fetchall()
        column_names = [column[1] for column in columns]
        
        # 如果是旧表结构（没有title列）
        if 'title' not in column_names:
            logger.info("检测到旧版reviews表结构，使用兼容模式查询")
            cursor.execute("""
                SELECT id, problem_id, status, review_date, notes, created_at
                FROM reviews
                ORDER BY created_at DESC
            """)
            
            results = cursor.fetchall()
            reviews = []
            
            for row in results:
                try:
                    review = {
                        'id': row[0],
                        'problem_id': row[1],
                        'status': row[2],
                        'review_date': row[3],
                        'notes': row[4],
                        'created_at': row[5]
                    }
                    reviews.append(review)
                except Exception as row_error:
                    logger.error(f"处理复习计划数据时出错: {str(row_error)}")
                    continue
        else:
            # 使用新表结构查询
            cursor.execute("""
                SELECT id, title, description, tags, categories, start_date, end_date, created_at
                FROM reviews
                ORDER BY created_at DESC
            """)
            
            results = cursor.fetchall()
            reviews = []
            
            for row in results:
                try:
                    review = {
                        'id': row[0],
                        'title': row[1],
                        'description': row[2],
                        'tags': json.loads(row[3]) if row[3] else [],
                        'categories': json.loads(row[4]) if row[4] else [],
                        'start_date': row[5],
                        'end_date': row[6],
                        'created_at': row[7]
                    }
                    reviews.append(review)
                except Exception as row_error:
                    logger.error(f"处理复习计划数据时出错: {str(row_error)}")
                    continue
        
        return jsonify({
            'success': True,
            'reviews': reviews
        })
    except Exception as e:
        logger.error(f"获取复习计划列表失败: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({
            'success': False,
            'error': f"获取复习计划列表失败: {str(e)}"
        }), 500

@review_api.route('/api/review-analysis', methods=['POST'])
def save_review_analysis_record():
    """保存回顾分析记录"""
    import logging
    logging.info("收到保存回顾分析记录请求")
    
    try:
        # 获取请求数据
        data = request.get_json()
        if not data:
            logging.error("请求体为空或非JSON格式")
            return jsonify({
                "success": False,
                "error": "请求数据格式错误，需要JSON格式的数据"
            }), 400
        
        # 记录接收到的数据（仅在调试模式）
        logging.debug(f"接收到的分析数据: {data}")
        
        # 检查必需字段
        required_fields = ['analysis_id', 'title', 'problems', 'analysis_result']
        missing_fields = [field for field in required_fields if field not in data]
        
        if missing_fields:
            logging.error(f"缺少必需字段: {', '.join(missing_fields)}")
            return jsonify({
                "success": False,
                "error": f"缺少必需字段: {', '.join(missing_fields)}"
            }), 400
        
        # 确保problems字段是数组
        problems = data['problems']
        if not isinstance(problems, list):
            logging.error("problems字段必须是列表")
            return jsonify({
                "success": False,
                "error": "problems字段必须是列表"
            }), 400
        
        # 检查problems是否为空
        if len(problems) == 0:
            logging.error("problems列表为空，至少需要一个问题")
            return jsonify({
                "success": False,
                "error": "请至少选择一个错题进行分析"
            }), 400
        
        # 检查analysis_result字段
        analysis_result = data['analysis_result']
        if not isinstance(analysis_result, dict):
            logging.error("analysis_result字段必须是对象/字典")
            return jsonify({
                "success": False,
                "error": "分析结果格式不正确"
            }), 400
            
        # 提取数据
        analysis_id = data['analysis_id']
        title = data['title']
        description = data.get('description', f"包含{len(problems)}道错题的分析")
            
        # 计算问题数量
        problem_count = len(problems)
        if problem_count == 0:
            logging.warning("分析没有包含任何问题")
            
        # 保存到数据库
        from core.database import save_review_analysis
        
        # 提取问题ID列表
        problem_ids = [p.get('id') for p in problems if p.get('id')]
        
        saved_id = save_review_analysis(
            analysis_id=analysis_id,
            title=title,
            description=description,
            problem_ids=problem_ids,
            analysis_data={
                'problems': problems,
                'analysis_result': analysis_result,
                'problem_count': problem_count,
                'created_at': data.get('created_at', datetime.now().isoformat())
            }
        )
        
        logging.info(f"分析记录已保存，ID: {saved_id}")
        
        return jsonify({
            "success": True,
            "analysis_id": saved_id,
            "message": "分析记录已保存"
        })
        
    except Exception as e:
        logging.error(f"保存分析记录时出错: {str(e)}")
        logging.exception(e)
        return jsonify({
            "success": False,
            "error": f"保存分析记录时出错: {str(e)}"
        }), 500

@review_api.route('/api/review-analysis/<analysis_id>', methods=['GET'])
def get_review_analysis_record(analysis_id):
    """获取特定回顾分析记录"""
    try:
        from core.database import get_review_analysis
        analysis = get_review_analysis(analysis_id)
        
        if not analysis:
            return jsonify({'success': False, 'error': '未找到指定的回顾分析记录'}), 404
        
        # 提取分析数据
        analysis_data = analysis['analysis_data']
        
        # 返回分析结果
        return jsonify({
            'success': True,
            'analysis': {
                'id': analysis['id'],
                'title': analysis['title'],
                'description': analysis['description'],
                'created_at': analysis['created_at'],
                'updated_at': analysis['updated_at'],
                'problems': analysis_data['problems'],
                'analysis_result': analysis_data['analysis_result']
            }
        })
            
    except Exception as e:
        logger.error(f"获取回顾分析记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({'success': False, 'error': str(e)}), 500

@review_api.route('/api/review-analyses', methods=['GET'])
def get_all_review_analyses():
    """获取所有回顾分析记录 - 兼容旧API"""
    try:
        from core.database import get_all_review_analyses
        analyses = get_all_review_analyses()
        
        return jsonify({
            'success': True,
            'analyses': analyses
        })
            
    except Exception as e:
        logger.error(f"获取所有回顾分析记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({'success': False, 'error': str(e)}), 500

@review_api.route('/api/review-analysis/<analysis_id>', methods=['DELETE'])
def delete_review_analysis(analysis_id):
    """删除回顾分析记录 - 兼容旧API，重定向到新系统的删除方法"""
    try:
        # 获取请求中的源信息
        source = request.args.get('source', '')
        logger.info(f"删除分析记录请求: id={analysis_id}, source={source}")
        
        # 统一使用review_sessions或reviews的删除方法
        if source == 'review_sessions':
            from core.database import delete_review_session as delete_func
            success = delete_func(analysis_id)
        elif source == 'reviews':
            from core.database import delete_review as delete_func
            success = delete_func(analysis_id)
        else:
            # 旧API调用，尝试判断ID来源
            from core.database import delete_review, delete_review_session
            
            # 尝试删除review_sessions表中的记录
            success = delete_review_session(analysis_id)
            
            # 如果上面失败，尝试删除reviews表中的记录
            if not success:
                success = delete_review(analysis_id)
            
            if not success:
                logger.warning(f"无法删除分析记录，未找到匹配的记录: {analysis_id}")
        
        return jsonify({
            'success': success,
            'message': '删除成功' if success else '删除失败，找不到对应的记录'
        })
    except Exception as e:
        logger.error(f"删除回顾分析记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({'success': False, 'error': str(e)}), 500

@review_api.route('/api/review/create-from-problems', methods=['POST'])
def create_review_from_problems():
    """
    从选定的问题创建新的回顾会话
    
    接收一个问题ID数组，创建新的回顾会话，然后返回回顾会话ID
    """
    try:
        data = request.json
        if not data:
            return jsonify({"success": False, "error": "未提供任何数据"}), 400
            
        # 获取问题ID列表
        problem_ids = data.get('problem_ids', [])
        if not problem_ids:
            return jsonify({"success": False, "error": "未提供任何问题ID"}), 400
            
        # 创建新的回顾会话
        import uuid
        review_id = str(uuid.uuid4())
        
        # 检查数据库中存在的表结构
        conn = database.get_db()
        cursor = conn.cursor()
        
        # 首先检查review_sessions表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions'")
        if cursor.fetchone():
            # 使用review_sessions表
            cursor.execute('''
            INSERT INTO review_sessions (id, problems_included, created_at)
            VALUES (?, ?, ?)
            ''', (review_id, json.dumps(problem_ids), datetime.now().isoformat()))
            conn.commit()
        else:
            # 然后检查reviews表的结构
            cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
            if cursor.fetchone():
                cursor.execute("PRAGMA table_info(reviews)")
                columns = cursor.fetchall()
                column_names = [column[1] for column in columns]
                
                if 'title' in column_names and 'description' in column_names:
                    # 新版表结构
                    # 使用第一个问题ID作为标题
                    title = problem_ids[0] if problem_ids else "新建回顾"
                    cursor.execute('''
                    INSERT INTO reviews (id, title, description, created_at, updated_at)
                    VALUES (?, ?, ?, ?, ?)
                    ''', (review_id, title, json.dumps(problem_ids), datetime.now().isoformat(), datetime.now().isoformat()))
                else:
                    # 旧版表结构
                    # 使用第一个问题ID作为problem_id
                    problem_id = problem_ids[0] if problem_ids else None
                    cursor.execute('''
                    INSERT INTO reviews (id, problem_id, status, created_at)
                    VALUES (?, ?, ?, ?)
                    ''', (review_id, problem_id, "待回顾", datetime.now().isoformat()))
                
                conn.commit()
            else:
                return jsonify({"success": False, "error": "未找到有效的回顾表结构"}), 500
        
        return jsonify({
            "success": True,
            "review_id": review_id
        })
    
    except Exception as e:
        logger.error(f"创建回顾会话失败: {str(e)}")
        logger.error(traceback.format_exc())
        return jsonify({"success": False, "error": str(e)}), 500

@review_api.route('/api/review/update-analysis', methods=['POST'])
def update_review_analysis():
    """更新回顾分析结果"""
    try:
        # 获取请求数据
        data = request.get_json()
        
        if not data:
            return jsonify({"success": False, "error": "无效的请求数据"}), 400
        
        review_id = data.get('review_id')
        analysis_result = data.get('analysis_result')
        title = data.get('title', f'回顾分析 ({datetime.now().strftime("%Y-%m-%d %H:%M:%S")}')
        save_to_history = data.get('save_to_history', False)
        
        if not review_id:
            return jsonify({"success": False, "error": "未提供review_id"}), 400
        
        if not analysis_result:
            return jsonify({"success": False, "error": "未提供分析结果"}), 400
        
        # 确保analysis_result是JSON可序列化的
        if isinstance(analysis_result, dict) or isinstance(analysis_result, list):
            # 已经是字典或列表，直接使用
            analysis_json = json.dumps(analysis_result, ensure_ascii=False)
        else:
            # 字符串格式，直接使用
            analysis_json = analysis_result
            
            # 尝试验证是否为有效的JSON字符串
            try:
                json.loads(analysis_json if isinstance(analysis_json, str) else analysis_json.decode('utf-8'))
            except:
                # 不是有效的JSON字符串，转换为简单对象
                analysis_json = json.dumps({"分析结果": analysis_result}, ensure_ascii=False)
        
        # 记录日志
        logger.debug(f"更新回顾分析: review_id={review_id}, title={title}, save_to_history={save_to_history}")
        logger.debug(f"分析结果类型: {type(analysis_result)}")
        
        # 保存分析结果
        success = database.update_review_analysis(review_id, analysis_json)
        
        # 同时保存到回顾分析历史表
        if save_to_history and success:
            database.create_review_analysis({
                'review_id': review_id,
                'title': title,
                'content': analysis_json,
                'created_at': datetime.now().strftime('%Y-%m-%d %H:%M:%S')
            })
            logger.info(f"已保存回顾分析到历史记录: {review_id}")
        
        return jsonify({
            "success": success,
            "review_id": review_id,
            "save_to_history": save_to_history
        })
        
    except Exception as e:
        logger.error(f"更新回顾分析失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

@review_api.route('/api/review/<review_id>/analysis', methods=['GET'])
def get_review_analysis(review_id):
    """获取回顾分析结果"""
    try:
        # 获取回顾记录
        review = database.get_review_session_by_id(review_id)
        if not review:
            return jsonify({"success": False, "error": "未找到指定的回顾记录"}), 404
        
        analysis_data = None
        
        # 尝试从review_analysis字段获取分析结果
        if 'review_analysis' in review and review['review_analysis']:
            try:
                if isinstance(review['review_analysis'], str):
                    # 尝试解析JSON
                    try:
                        analysis_data = json.loads(review['review_analysis'])
                        logger.info(f"成功从review_analysis字段解析JSON数据")
                    except json.JSONDecodeError:
                        # 如果不是有效的JSON，则直接使用字符串
                        analysis_data = review['review_analysis']
                        logger.info(f"review_analysis字段不是有效的JSON，直接使用字符串")
                elif isinstance(review['review_analysis'], dict):
                    analysis_data = review['review_analysis']
                    logger.info(f"review_analysis字段是字典类型")
            except Exception as e:
                logger.error(f"解析review_analysis失败: {str(e)}\n{traceback.format_exc()}")
        
        # 尝试从description字段获取分析结果（兼容新版表结构）
        if not analysis_data and 'description' in review and review['description']:
            try:
                if isinstance(review['description'], str):
                    try:
                        analysis_data = json.loads(review['description'])
                        logger.info(f"成功从description字段解析JSON数据")
                    except json.JSONDecodeError:
                        analysis_data = review['description']
                        logger.info(f"description字段不是有效的JSON，直接使用字符串")
                elif isinstance(review['description'], dict):
                    analysis_data = review['description']
                    logger.info(f"description字段是字典类型")
            except Exception as e:
                logger.error(f"解析description失败: {str(e)}\n{traceback.format_exc()}")
        
        # 尝试从notes字段获取分析结果（兼容旧版表结构）
        if not analysis_data and 'notes' in review and review['notes']:
            try:
                if isinstance(review['notes'], str):
                    try:
                        analysis_data = json.loads(review['notes'])
                        logger.info(f"成功从notes字段解析JSON数据")
                    except json.JSONDecodeError:
                        analysis_data = review['notes']
                        logger.info(f"notes字段不是有效的JSON，直接使用字符串")
                elif isinstance(review['notes'], dict):
                    analysis_data = review['notes']
                    logger.info(f"notes字段是字典类型")
            except Exception as e:
                logger.error(f"解析notes失败: {str(e)}\n{traceback.format_exc()}")
        
        if analysis_data:
            # 如果analysis_data是字符串，直接返回
            if isinstance(analysis_data, str):
                logger.info(f"返回字符串类型的分析结果")
                return jsonify({
                    "success": True,
                    "analysis": analysis_data
                })
            
            # 如果是字典类型，转换为Markdown格式
            elif isinstance(analysis_data, dict):
                logger.info(f"将字典类型的分析结果转换为Markdown格式")
                markdown_content = ""
                
                # 添加标题
                markdown_content += "# 回顾分析结果\n\n"
                
                # 添加各个分析部分
                for key, value in analysis_data.items():
                    if value and isinstance(value, str) and key != 'success':
                        markdown_content += f"## {key}\n\n{value}\n\n"
                    elif isinstance(value, list) and key == '知识点标签':
                        tags_list = '\n- '.join(value)
                        markdown_content += f"## {key}\n\n- {tags_list}\n\n"
                
                logger.info(f"生成的Markdown内容长度: {len(markdown_content)}")
                return jsonify({
                    "success": True,
                    "analysis": markdown_content,
                    "raw_data": analysis_data
                })
            
            # 其他类型，尝试转换为字符串
            else:
                logger.warning(f"分析结果类型未知: {type(analysis_data)}")
                return jsonify({
                    "success": True,
                    "analysis": str(analysis_data)
                })
        else:
            logger.warning(f"未找到分析结果")
            return jsonify({"success": False, "error": "未找到分析结果"}), 404
    except Exception as e:
        logger.error(f"获取回顾分析结果失败: {str(e)}\n{traceback.format_exc()}")
        return jsonify({"success": False, "error": str(e)}), 500

def generate_review_analysis(problems_data):
    """生成回顾分析结果
    
    分析多个错题之间的关系，找出共同的错因和知识点
    """
    # 初始化分析结果
    analysis = {
        'total_problems': len(problems_data),
        'knowledge_stats': {},  # 知识点统计
        'error_cause_stats': {},  # 错因统计
        'knowledge_connections': [],  # 知识点之间的联系
        'weak_points': [],  # 薄弱点分析
        'review_strategy': '',  # 复习策略建议
    }
    
    # 收集所有知识点和错因
    all_knowledge_tags = []
    all_error_causes = []
    
    for problem in problems_data:
        # 统计知识点
        knowledge_tags = problem.get('knowledge_tags', [])
        if not knowledge_tags:
            knowledge_tags = problem.get('tags', [])
        
        for tag in knowledge_tags:
            all_knowledge_tags.append(tag)
            if tag in analysis['knowledge_stats']:
                analysis['knowledge_stats'][tag]['count'] += 1
                analysis['knowledge_stats'][tag]['problems'].append(problem['id'])
            else:
                analysis['knowledge_stats'][tag] = {
                    'count': 1,
                    'problems': [problem['id']]
                }
        
        # 统计错因
        error_cause = problem.get('error_cause', '')
        if error_cause:
            all_error_causes.append(error_cause)
            
            # 简单错因提取（可以在未来使用NLP改进）
            key_phrases = extract_key_phrases(error_cause)
            
            for phrase in key_phrases:
                if phrase in analysis['error_cause_stats']:
                    analysis['error_cause_stats'][phrase]['count'] += 1
                    analysis['error_cause_stats'][phrase]['problems'].append(problem['id'])
                else:
                    analysis['error_cause_stats'][phrase] = {
                        'count': 1,
                        'problems': [problem['id']]
                    }
    
    # 分析知识点之间的联系
    knowledge_pairs = {}
    for i in range(len(all_knowledge_tags)):
        for j in range(i+1, len(all_knowledge_tags)):
            tag1 = all_knowledge_tags[i]
            tag2 = all_knowledge_tags[j]
            
            # 避免自我配对
            if tag1 == tag2:
                continue
                
            pair = tuple(sorted([tag1, tag2]))
            if pair in knowledge_pairs:
                knowledge_pairs[pair] += 1
            else:
                knowledge_pairs[pair] = 1
    
    # 筛选出频率较高的知识点对
    for pair, count in sorted(knowledge_pairs.items(), key=lambda x: x[1], reverse=True):
        if count >= 2:  # 至少出现两次的知识点对
            analysis['knowledge_connections'].append({
                'tags': list(pair),
                'count': count
            })
    
    # 找出薄弱点（高频知识点和高频错因）
    for tag, stats in sorted(analysis['knowledge_stats'].items(), key=lambda x: x[1]['count'], reverse=True):
        if stats['count'] >= 2:  # 出现两次以上的知识点被视为潜在薄弱点
            analysis['weak_points'].append({
                'type': 'knowledge',
                'content': tag,
                'frequency': stats['count'],
                'problems': stats['problems']
            })
    
    for cause, stats in sorted(analysis['error_cause_stats'].items(), key=lambda x: x[1]['count'], reverse=True):
        if stats['count'] >= 2:  # 出现两次以上的错因被视为潜在薄弱点
            analysis['weak_points'].append({
                'type': 'error_cause',
                'content': cause,
                'frequency': stats['count'],
                'problems': stats['problems']
            })
    
    # 生成复习策略
    analysis['review_strategy'] = generate_review_strategy(analysis)
    
    return analysis

def extract_key_phrases(text):
    """从错因文本中提取关键短语"""
    # 简单实现：按句分割，提取短语
    # 未来可用NLP改进
    if not text:
        return []
        
    # 分割句子
    sentences = text.replace('。', '.').replace('；', '.').replace(';', '.').split('.')
    
    key_phrases = []
    for sentence in sentences:
        sentence = sentence.strip()
        if not sentence:
            continue
            
        # 简单启发式规则：
        # 1. 短句可能是关键点
        if 5 <= len(sentence) <= 30:
            key_phrases.append(sentence)
        # 2. 包含关键词的句子
        elif any(kw in sentence for kw in ['没有', '缺乏', '不足', '错误', '混淆', '误解', '弄错']):
            key_phrases.append(sentence)
    
    # 如果没有提取到任何短语，取原文前100个字符
    if not key_phrases and text:
        return [text[:min(100, len(text))]]
        
    return key_phrases

def generate_review_strategy(analysis):
    """基于分析结果生成复习策略"""
    total_problems = analysis['total_problems']
    weak_points = analysis['weak_points']
    
    if not weak_points:
        return "未检测到明显的薄弱点，建议全面复习所有知识点。"
    
    # 按频率排序薄弱点
    sorted_weak_points = sorted(weak_points, key=lambda x: x['frequency'], reverse=True)
    
    # 组织复习策略文本
    strategy = f"基于对{total_problems}道错题的分析，建议采取以下复习策略：\n\n"
    
    # 添加重点知识点
    knowledge_points = [p for p in sorted_weak_points if p['type'] == 'knowledge']
    if knowledge_points:
        strategy += "【重点知识点】\n"
        for i, point in enumerate(knowledge_points[:5], 1):  # 最多显示5个
            percentage = round(point['frequency'] / total_problems * 100)
            strategy += f"{i}. {point['content']}（出现频率：{percentage}%）\n"
        strategy += "\n"
    
    # 添加常见错因
    error_causes = [p for p in sorted_weak_points if p['type'] == 'error_cause']
    if error_causes:
        strategy += "【常见错误原因】\n"
        for i, cause in enumerate(error_causes[:5], 1):  # 最多显示5个
            percentage = round(cause['frequency'] / total_problems * 100)
            strategy += f"{i}. {cause['content']}（出现频率：{percentage}%）\n"
        strategy += "\n"
    
    # 添加知识点关联
    if analysis['knowledge_connections']:
        strategy += "【知识点关联】\n"
        for i, conn in enumerate(analysis['knowledge_connections'][:3], 1):  # 最多显示3个
            strategy += f"{i}. {' 与 '.join(conn['tags'])} - 这些知识点经常一起出现，建议联系复习\n"
        strategy += "\n"
    
    # 添加复习建议
    strategy += "【复习建议】\n"
    if len(knowledge_points) >= 3:
        strategy += "1. 优先复习高频知识点，特别是多次出错的内容\n"
    if len(error_causes) >= 2:
        strategy += "2. 注意常见错误原因，避免犯同样的错误\n"
    if analysis['knowledge_connections']:
        strategy += "3. 注重知识点间的关联，建立知识网络\n"
    
    strategy += "4. 针对每个薄弱点制定专项练习\n"
    strategy += "5. 定期回顾这些错题，强化记忆和理解\n"
    
    return strategy
