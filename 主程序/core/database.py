"""
数据库操作模块
"""
import os
import json
import sqlite3
import logging
import traceback
import datetime
from config import DATABASE_PATH
import uuid

logger = logging.getLogger(__name__)

def get_db():
    """获取数据库连接"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        return conn
    except Exception as e:
        logger.error(f"获取数据库连接失败: {str(e)}")
        raise

def init_db():
    """初始化数据库"""
    try:
        # 创建data目录
        if not os.path.exists(os.path.dirname(DATABASE_PATH)):
            os.makedirs(os.path.dirname(DATABASE_PATH))
        
        # 连接数据库
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 创建表
        _ensure_tables(cursor)
        
        # 检查是否需要添加默认API配置
        cursor.execute("SELECT COUNT(*) FROM api_config")
        count = cursor.fetchone()[0]
        
        if count == 0:
            # 添加默认API配置
            default_configs = {
                'openai': {
                    'api_key': '测试OpenAI密钥',
                    'api_url': 'https://api.openai.com/v1',
                    'model': 'gpt-4'
                },
                'deepseek': {
                    'api_key': '测试DeepSeek密钥',
                    'api_url': 'https://api.deepseek.com/v1',
                    'model': 'deepseek-chat'
                },
                'qwen-vl': {
                    'api_key': '测试千问VL密钥',
                    'api_url': 'https://dashscope.aliyuncs.com/v1',
                    'model': 'qwen-vl-plus'
                },
                'gemini': {
                    'api_key': '测试Gemini密钥',
                    'api_url': 'https://generativelanguage.googleapis.com',
                    'model': 'gemini-pro'
                },
                'claude': {
                    'api_key': '测试Claude密钥',
                    'api_url': 'https://api.anthropic.com/v1',
                    'model': 'claude-3-opus-20240229'
                },
                'aliyun_ocr': {
                    'access_key_id': '测试阿里云AccessKeyID',
                    'access_key_secret': '测试阿里云AccessKeySecret',
                    'region_id': 'cn-shanghai'
                },
                'default_models': {
                    'extraction': 'multimodal_qwen-vl',
                    'analysis': 'deepseek'
                },
                'api_alternatives': {
                    'deepseek': {
                        'api_1': {
                            'name': '官方API',
                            'api_key': 'sk-d3377feb708b4d4fac4cb9298119cb48',
                            'api_url': 'https://api.deepseek.com/v1/chat/completions',
                            'model_name': 'deepseek-reasoner'
                        },
                        'api_2': {
                            'name': '硅基流动',
                            'api_key': 'sk-bcwnxcygrdoxdshzkycfoeuvilgilrxslgwoukumenjwpxwu',
                            'api_url': 'https://api.siliconflow.cn/v1/chat/completions',
                            'model_name': 'Pro/deepseek-ai/DeepSeek-R1'
                        },
                        'api_3': {
                            'name': '火山',
                            'api_key': '9811bf5a-8dae-4fc9-ac7e-e1b9bff2f66c',
                            'api_url': 'https://ark.cn-beijing.volces.com/api/v3/chat/completions',
                            'model_name': 'deepseek-r1-250120'
                        }
                    }
                }
            }
            
            # 添加默认配置
            for key, value in default_configs.items():
                cursor.execute(
                    "INSERT INTO api_config (key, value) VALUES (?, ?)",
                    (key, json.dumps(value))
                )
            
            logger.info("添加了默认API配置")
        
        # 提交更改
        conn.commit()
        conn.close()
        
        logger.info("数据库初始化完成")
        return True
    except Exception as e:
        logger.error(f"初始化数据库失败: {str(e)}\n{traceback.format_exc()}")
        return False

def _ensure_tables(cursor):
    """确保所有必要的数据库表都存在"""
    # 创建错题表
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS mistakes (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        question_id TEXT,
        user_id TEXT,
        subject TEXT,
        question_text TEXT,
        question_image_path TEXT,
        answer TEXT,
        user_answer TEXT,
        explanation TEXT,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        tags TEXT,
        correct INTEGER DEFAULT 0,
        difficulty INTEGER DEFAULT 3,
        review_count INTEGER DEFAULT 0,
        last_reviewed TIMESTAMP,
        next_review TIMESTAMP,
        sm2_interval INTEGER DEFAULT 1,
        sm2_repetition INTEGER DEFAULT 0,
        sm2_efactor REAL DEFAULT 2.5,
        extracted_by TEXT DEFAULT 'manual',
        analyzed_by TEXT DEFAULT 'manual',
        source TEXT,
        chapter TEXT,
        page_number INTEGER,
        question_type TEXT,
        knowledge_points TEXT
    )
    ''')
    
    # 创建资源表
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS resources (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        resource_id TEXT,
        title TEXT,
        description TEXT,
        file_path TEXT,
        file_type TEXT,
        tags TEXT,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        owner_id TEXT,
        is_public INTEGER DEFAULT 0,
        download_count INTEGER DEFAULT 0,
        file_size INTEGER,
        thumbnail_path TEXT,
        custom_fields TEXT
    )
    ''')
    
    # 创建设置表
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS settings (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        category TEXT,
        key TEXT,
        value TEXT,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        UNIQUE(category, key)
    )
    ''')
    
    # 创建API配置表
    cursor.execute('''
    CREATE TABLE IF NOT EXISTS api_config (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        key TEXT UNIQUE,
        value TEXT,
        created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
    )
    ''')
    
    logger.debug("已确保所有必要的数据库表存在")

def save_error_problem(problem):
    """保存单个错题到数据库"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 确保表已创建
        _ensure_tables(cursor)
        
        # 处理additional_images字段，确保是JSON字符串
        additional_images_json = None
        if hasattr(problem, 'additional_images') and problem.additional_images:
            additional_images_json = json.dumps(problem.additional_images, ensure_ascii=False)
        
        # 转换tags为JSON字符串
        tags_json = json.dumps(problem.tags, ensure_ascii=False) if problem.tags else json.dumps([])
        
        # 当前时间
        current_time = datetime.datetime.now().isoformat()
        
        # 确保所有字段都是基本类型
        if problem.error_analysis and not isinstance(problem.error_analysis, str):
            logger.warning(f"错题ID {problem.id} 的error_analysis字段不是字符串类型: {type(problem.error_analysis)}")
            problem.error_analysis = str(problem.error_analysis)
            
        if problem.correct_solution and not isinstance(problem.correct_solution, str):
            logger.warning(f"错题ID {problem.id} 的correct_solution字段不是字符串类型: {type(problem.correct_solution)}")
            problem.correct_solution = str(problem.correct_solution)
            
        if problem.problem_content and not isinstance(problem.problem_content, str):
            logger.warning(f"错题ID {problem.id} 的problem_content字段不是字符串类型: {type(problem.problem_content)}")
            problem.problem_content = str(problem.problem_content)
            
        # 确保difficulty是整数
        if hasattr(problem, 'difficulty') and problem.difficulty is not None:
            try:
                problem.difficulty = int(problem.difficulty)
            except (ValueError, TypeError):
                logger.warning(f"错题ID {problem.id} 的difficulty字段无法转换为整数: {problem.difficulty}")
                problem.difficulty = 3  # 默认中等难度
        
        # 确保subject字段存在
        subject = "math"
        if hasattr(problem, 'subject') and problem.subject:
            subject = problem.subject
        
        # 插入错题
        cursor.execute(
            '''
            INSERT INTO error_problems (
                id, image_path, additional_images, problem_content, error_analysis, problem_category,
                problem_subcategory, error_type, difficulty, correct_solution, tags, 
                created_at, updated_at, notes, subject
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ''',
            (
                str(problem.id), 
                str(problem.image_path) if problem.image_path else "", 
                additional_images_json,
                str(problem.problem_content) if problem.problem_content else "",
                str(problem.error_analysis) if problem.error_analysis else "",
                str(problem.problem_category) if problem.problem_category else "",
                str(problem.problem_subcategory) if problem.problem_subcategory else "",
                str(problem.error_type) if problem.error_type else "",
                int(problem.difficulty) if hasattr(problem, 'difficulty') and problem.difficulty is not None else 3,
                str(problem.correct_solution) if problem.correct_solution else "",
                tags_json,
                str(problem.created_at) if problem.created_at else current_time,
                current_time,
                str(problem.notes) if hasattr(problem, 'notes') and problem.notes else "",
                subject  # 使用获取的subject值
            )
        )
        
        # 处理标签
        if problem.tags:
            process_problem_tags(cursor, problem.id, problem.tags)
        
        conn.commit()
        conn.close()
        
        logger.info(f"成功保存错题 ID: {problem.id}")
        return True
    except Exception as e:
        logger.error(f"错题保存失败: {str(e)}\n{traceback.format_exc()}")
        return False

def process_problem_tags(cursor, problem_id, tags):
    """处理错题标签，智能查找或创建标签"""
    for tag_name in tags:
        # 规范化标签名称
        tag_name = tag_name.strip()
        if not tag_name:
            continue
        
        # 检查标签是否存在
        cursor.execute("SELECT id, usage_count FROM tags WHERE name = ?", (tag_name,))
        tag_result = cursor.fetchone()
        
        if tag_result:
            # 标签已存在，使用现有标签
            tag_id, usage_count = tag_result
            # 更新标签使用计数
            cursor.execute("UPDATE tags SET usage_count = ? WHERE id = ?", 
                           (usage_count + 1, tag_id))
        else:
            # 标签不存在，创建新标签
            # 确定可能的类别 - 基于常见类别系统
            category = categorize_tag(tag_name)
            
            cursor.execute(
                '''
                INSERT INTO tags (name, category, created_at, usage_count)
                VALUES (?, ?, ?, 1)
                ''',
                (tag_name, category, datetime.datetime.now().isoformat())
            )
            tag_id = cursor.lastrowid
        
        # 关联错题与标签
        cursor.execute(
            '''
            INSERT OR IGNORE INTO problem_tags (problem_id, tag_id)
            VALUES (?, ?)
            ''',
            (problem_id, tag_id)
        )

def categorize_tag(tag_name):
    """根据标签名称自动推断类别"""
    # 常见错误类型
    error_types = ["计算错误", "概念理解错误", "公式应用错误", "推理逻辑错误",
                   "审题错误", "解题方法选择错误", "粗心大意", "时间不足"]
    
    # 数学主要类别
    math_categories = {
        "高等数学": ["极限", "导数", "微分", "积分", "级数", "微分方程", "多元函数"],
        "线性代数": ["矩阵", "行列式", "向量", "特征值", "线性方程", "空间"],
        "概率论": ["概率", "统计", "分布", "随机", "期望", "方差"]
    }
    
    # 判断是否是错误类型
    for error_type in error_types:
        if error_type in tag_name:
            return "错误类型"
    
    # 判断是否是数学子类
    for category, keywords in math_categories.items():
        for keyword in keywords:
            if keyword in tag_name:
                return category
    
    # 默认返回通用类别
    return "通用知识点"

def save_error_problem_with_multiple_images(problem):
    """保存带有多个图片的错题"""
    return save_error_problem(problem)

def get_error_problems(subject="math"):
    """获取所有错题列表
    
    Args:
        subject (str): 学科名称，默认为"math"（数学）
        
    Returns:
        list: 错题列表
    """
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        cursor.execute("""
            SELECT id, problem_category, problem_subcategory, difficulty, 
                   typicality, created_at, image_path, problem_content, tags, subject
            FROM error_problems
            WHERE subject = ?
            ORDER BY created_at DESC
        """, (subject,))
        
        problems = []
        for row in cursor.fetchall():
            try:
                problem = dict(row)
                
                # 确保tags字段是列表
                if problem['tags'] and isinstance(problem['tags'], str):
                    try:
                        problem['tags'] = json.loads(problem['tags'])
                    except json.JSONDecodeError:
                        logger.warning(f"无法解析错题ID {problem['id']} 的tags字段: {problem['tags']}")
                        problem['tags'] = []
                elif problem['tags'] is None:
                    problem['tags'] = []
                    
                # 确保problem_category和problem_subcategory不为空
                if not problem['problem_category']:
                    problem['problem_category'] = "高等数学"
                if not problem['problem_subcategory']:
                    problem['problem_subcategory'] = "微积分"
                
                # 确保problem_content不为空
                if not problem['problem_content']:
                    problem['problem_content'] = "题目内容未提取，请查看图片"
                    
                problems.append(problem)
            except Exception as row_error:
                logger.error(f"处理错题行数据时出错: {str(row_error)}")
                continue
        
        conn.close()
        return problems
        
    except Exception as e:
        logger.error(f"获取错题列表失败: {str(e)}\n{traceback.format_exc()}")
        return []

def get_error_problem_by_id(problem_id, subject=None):
    """获取单个错题详情，可以按学科过滤"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 如果提供了学科参数，则按ID和学科筛选
        if subject:
            cursor.execute("SELECT * FROM error_problems WHERE id = ? AND subject = ?", (problem_id, subject))
        else:
            cursor.execute("SELECT * FROM error_problems WHERE id = ?", (problem_id,))
            
        result = cursor.fetchone()
        
        if not result:
            conn.close()
            return None
        
        problem = dict(result)
        
        # 解析JSON字段
        for field in ['tags', 'additional_images']:
            if problem[field] and isinstance(problem[field], str):
                try:
                    problem[field] = json.loads(problem[field])
                except json.JSONDecodeError:
                    logger.warning(f"无法解析错题ID {problem_id} 的{field}字段: {problem[field]}")
                    problem[field] = []
            elif problem[field] is None:
                problem[field] = []
            
        # 确保所有必要字段都有值
        if not problem['problem_category']:
            problem['problem_category'] = "高等数学"
        if not problem['problem_subcategory']:
            problem['problem_subcategory'] = "微积分"
        if not problem['problem_content']:
            problem['problem_content'] = "题目内容未提取，请查看图片"
        if not problem['error_analysis']:
            problem['error_analysis'] = "无错误分析"
        if not problem['correct_solution']:
            problem['correct_solution'] = "无正确解法"
        if not problem['error_type']:
            problem['error_type'] = "未知错误类型"
        
        # 确保subject字段存在并有默认值
        if 'subject' not in problem or not problem['subject']:
            problem['subject'] = "math"
        
        conn.close()
        return problem
    except Exception as e:
        logger.error(f"获取错题详情失败: {str(e)}\n{traceback.format_exc()}")
        return None

def update_problem_typicality(problem_id, typicality):
    """更新错题典型度评分"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        cursor.execute(
            "UPDATE error_problems SET typicality = ?, updated_at = ? WHERE id = ?",
            (typicality, datetime.datetime.now().isoformat(), problem_id)
        )
        
        if cursor.rowcount == 0:
            conn.close()
            return False
            
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        logger.error(f"更新典型度评分失败: {str(e)}\n{traceback.format_exc()}")
        return False

def update_problem_details(problem_id, updates):
    """更新错题详细信息"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 构建更新字段
        update_fields = []
        update_values = []
        
        # 可更新的字段列表
        updatable_fields = [
            'problem_content', 'error_analysis', 'problem_category', 'problem_subcategory',
            'error_type', 'difficulty', 'correct_solution', 'notes', 'typicality', 'subject'
        ]
        
        for field in updatable_fields:
            if field in updates:
                update_fields.append(f"{field} = ?")
                update_values.append(updates[field])
        
        # 特殊处理标签字段
        if 'tags' in updates and isinstance(updates['tags'], list):
            update_fields.append("tags = ?")
            update_values.append(json.dumps(updates['tags'], ensure_ascii=False))
            
            # 更新标签关联
            process_problem_tags(cursor, problem_id, updates['tags'])
            
            # 清除旧关联
            cursor.execute("DELETE FROM problem_tags WHERE problem_id = ?", (problem_id,))
            
            # 重新建立关联
            for tag_name in updates['tags']:
                cursor.execute("SELECT id FROM tags WHERE name = ?", (tag_name,))
                tag_result = cursor.fetchone()
                if tag_result:
                    tag_id = tag_result[0]
                    cursor.execute(
                        "INSERT OR IGNORE INTO problem_tags (problem_id, tag_id) VALUES (?, ?)",
                        (problem_id, tag_id)
                    )
        
        # 添加更新时间
        update_fields.append("updated_at = ?")
        update_values.append(datetime.datetime.now().isoformat())
        
        # 检查是否有更新内容
        if not update_fields:
            conn.close()
            return False
            
        # 构建和执行更新查询
        query = f"UPDATE error_problems SET {', '.join(update_fields)} WHERE id = ?"
        update_values.append(problem_id)
        
        cursor.execute(query, update_values)
        
        if cursor.rowcount == 0:
            conn.close()
            return False
            
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        logger.error(f"更新错题详情失败: {str(e)}\n{traceback.format_exc()}")
        return False

def get_all_tags():
    """获取所有标签及其使用情况"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        cursor.execute("""
            SELECT t.id, t.name, t.category, t.usage_count, t.parent_id, COUNT(pt.problem_id) as actual_count
            FROM tags t
            LEFT JOIN problem_tags pt ON t.id = pt.tag_id
            GROUP BY t.id
            ORDER BY t.category, t.name
        """)
        
        tags = []
        for row in cursor.fetchall():
            tags.append(dict(row))
        
        conn.close()
        return tags
    except Exception as e:
        logger.error(f"获取标签列表失败: {str(e)}\n{traceback.format_exc()}")
        return []

def delete_problems(problem_ids):
    """批量删除错题"""
    try:
        if not problem_ids:
            return False
            
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 删除关联的标签关系
        placeholders = ', '.join(['?' for _ in problem_ids])
        cursor.execute(f"DELETE FROM problem_tags WHERE problem_id IN ({placeholders})", problem_ids)
        
        # 删除错题
        cursor.execute(f"DELETE FROM error_problems WHERE id IN ({placeholders})", problem_ids)
                
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        logger.error(f"批量删除错题失败: {str(e)}\n{traceback.format_exc()}")
        return False

def merge_tags(source_ids, target_id):
    """合并多个标签到一个目标标签"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 获取目标标签信息
        cursor.execute("SELECT name FROM tags WHERE id = ?", (target_id,))
        target = cursor.fetchone()
        if not target:
            conn.close()
            return False
        
        # 更新关联关系
        for source_id in source_ids:
            if str(source_id) == str(target_id):  # 防止自己合并到自己
                continue
                
            # 查找使用源标签的错题
            cursor.execute("SELECT problem_id FROM problem_tags WHERE tag_id = ?", (source_id,))
            problems = cursor.fetchall()
            
            # 为这些错题添加目标标签
            for problem in problems:
                cursor.execute(
                    "INSERT OR IGNORE INTO problem_tags (problem_id, tag_id) VALUES (?, ?)",
                    (problem[0], target_id)
                )
            
            # 删除源标签关联
            cursor.execute("DELETE FROM problem_tags WHERE tag_id = ?", (source_id,))
            
            # 删除源标签
            cursor.execute("DELETE FROM tags WHERE id = ?", (source_id,))
        
        # 更新目标标签使用计数
        cursor.execute("""
            UPDATE tags
            SET usage_count = (
                SELECT COUNT(*) FROM problem_tags WHERE tag_id = ?
            )
            WHERE id = ?
        """, (target_id, target_id))
        
        conn.commit()
        conn.close()
        return True
    except Exception as e:
        logger.error(f"合并标签失败: {str(e)}\n{traceback.format_exc()}")
        return False

def backup_database(backup_path=None):
    """备份数据库到文件"""
    try:
        # 默认备份文件路径
        if not backup_path:
            timestamp = datetime.datetime.now().strftime('%Y%m%d_%H%M%S')
            backup_dir = os.path.join(os.path.dirname(DATABASE_PATH), 'backups')
            
            # 确保备份目录存在
            if not os.path.exists(backup_dir):
                os.makedirs(backup_dir)
                
            backup_path = os.path.join(backup_dir, f'backup_{timestamp}.db')
        
        # 备份数据库
        source_conn = sqlite3.connect(DATABASE_PATH)
        dest_conn = sqlite3.connect(backup_path)
        
        # 导出源数据库
        with dest_conn:
            source_conn.backup(dest_conn)
            
        source_conn.close()
        dest_conn.close()
        
        logger.info(f"数据库已备份至: {backup_path}")
        return backup_path
    except Exception as e:
        logger.error(f"数据库备份失败: {str(e)}\n{traceback.format_exc()}")
        return None

def restore_database(backup_path):
    """从备份文件恢复数据库"""
    try:
        # 检查备份文件是否存在
        if not os.path.exists(backup_path):
            logger.error(f"备份文件不存在: {backup_path}")
            return False
        
        # 恢复前先创建当前数据库的备份
        current_backup = backup_database()
        
        # 恢复数据库
        source_conn = sqlite3.connect(backup_path)
        dest_conn = sqlite3.connect(DATABASE_PATH)
        
        # 导入备份数据
        with dest_conn:
            source_conn.backup(dest_conn)
            
        source_conn.close()
        dest_conn.close()
        
        logger.info(f"数据库已从 {backup_path} 恢复")
        return True
    except Exception as e:
        logger.error(f"数据库恢复失败: {str(e)}\n{traceback.format_exc()}")
        return False

# API配置相关函数
def get_api_config(key=None):
    """获取API配置
    
    Args:
        key: 如果提供，则获取特定配置项；否则获取所有配置
        
    Returns:
        如果提供key，返回对应的值（可能是JSON字符串）
        如果不提供key，返回包含所有配置的字典
    """
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 确保api_config表存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='api_config'")
        if not cursor.fetchone():
            logger.warning("api_config表不存在")
            conn.close()
            return None if key else {}
        
        if key:
            cursor.execute("SELECT value FROM api_config WHERE key = ?", (key,))
            result = cursor.fetchone()
            conn.close()
            
            if not result:
                return None
                
            # 返回原始值，由调用者决定是否解析JSON
            return result['value']
        else:
            cursor.execute("SELECT key, value FROM api_config")
            results = cursor.fetchall()
            
            configs = {}
            for row in results:
                configs[row['key']] = row['value']
                
            conn.close()
            return configs
    except Exception as e:
        logger.error(f"获取API配置失败: {str(e)}\n{traceback.format_exc()}")
        return {} if key is None else None

def update_api_config(key, value):
    """更新单个API配置项
    
    Args:
        key (str): 配置键名
        value: 配置值，可以是字符串或需要转为JSON的对象
        
    Returns:
        bool: 更新是否成功
    """
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 确保api_config表存在
        cursor.execute('''
        CREATE TABLE IF NOT EXISTS api_config (
            key TEXT PRIMARY KEY,
            value TEXT,
            updated_at TEXT
        )
        ''')
        
        # 特殊处理API替代品配置，确保它存在
        if key == 'api_alternatives' and isinstance(value, dict) and 'deepseek' in value:
            # 检查deepseek值是否为字符串
            if isinstance(value['deepseek'], str):
                logger.warning(f"发现api_alternatives.deepseek是字符串，尝试转换为字典")
                try:
                    # 尝试将字符串解析为JSON对象
                    value['deepseek'] = json.loads(value['deepseek'])
                except:
                    logger.error(f"无法将api_alternatives.deepseek从字符串转换为字典: {value['deepseek']}")
        
        # 序列化值（如果是字典或列表）
        if isinstance(value, (dict, list)):
            value = json.dumps(value)
        
        timestamp = datetime.datetime.now().isoformat()
        
        # 使用REPLACE策略，自动处理插入或更新
        cursor.execute(
            "REPLACE INTO api_config (key, value, updated_at) VALUES (?, ?, ?)",
            (key, value, timestamp)
        )
            
        conn.commit()
        conn.close()
        
        logger.info(f"API配置已更新: {key}")
        
        # 更新config模块中的全局变量
        try:
            import config
            config.API_CONFIG = load_api_config_from_db()
        except Exception as e:
            logger.warning(f"无法更新config.API_CONFIG全局变量: {str(e)}")
        
        return True
    except Exception as e:
        logger.error(f"更新API配置失败: {str(e)}\n{traceback.format_exc()}")
        return False

def suggest_tags(partial_tag, limit=10):
    """根据部分标签名称推荐匹配的标签"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 模糊查询匹配的标签
        cursor.execute(
            "SELECT name FROM tags WHERE name LIKE ? ORDER BY usage_count DESC LIMIT ?", 
            (f"%{partial_tag}%", limit)
        )
        
        results = cursor.fetchall()
        suggestions = [row['name'] for row in results]
        
        conn.close()
        return suggestions
    except Exception as e:
        logger.error(f"标签推荐失败: {str(e)}\n{traceback.format_exc()}")
        return []

def save_review_session(review):
    """保存回顾记录"""
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 准备数据
        problems_included = json.dumps(review.problems_included)
        review_analysis = json.dumps(review.review_analysis) if review.review_analysis else None
        improvement_strategy = json.dumps(review.improvement_strategy) if review.improvement_strategy else None
        
        # 执行插入
        cursor.execute('''
        INSERT INTO review_sessions (id, problems_included, review_analysis, improvement_strategy, created_at)
        VALUES (?, ?, ?, ?, ?)
        ''', (
            review.id,
            problems_included,
            review_analysis,
            improvement_strategy,
            review.created_at
        ))
        
        conn.commit()
        return True
    except Exception as e:
        logger.error(f"保存回顾记录失败: {str(e)}")
        return False

def get_review_sessions():
    """获取所有回顾记录"""
    conn = get_db()
    cursor = conn.cursor()
    
    cursor.execute('''
    SELECT id, problems_included, review_analysis, improvement_strategy, created_at
    FROM review_sessions
    ORDER BY created_at DESC
    ''')
    
    columns = [col[0] for col in cursor.description]
    reviews = []
    
    for row in cursor.fetchall():
        review = dict(zip(columns, row))
        
        # 将JSON字符串转换为Python对象
        if review['problems_included']:
            review['problems_included'] = json.loads(review['problems_included'])
        else:
            review['problems_included'] = []
            
        if review['review_analysis']:
            try:
                review['review_analysis'] = json.loads(review['review_analysis'])
            except:
                review['review_analysis'] = {}
        else:
            review['review_analysis'] = {}
            
        if review['improvement_strategy']:
            try:
                review['improvement_strategy'] = json.loads(review['improvement_strategy'])
            except:
                review['improvement_strategy'] = {}
        else:
            review['improvement_strategy'] = {}
            
        reviews.append(review)
    
    return reviews

def get_review_session_by_id(review_id):
    """
    通过ID获取回顾记录，兼容两种表结构
    
    Args:
        review_id (str): 回顾ID
        
    Returns:
        dict: 回顾记录，如果不存在则返回None
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 第一步：检查review_sessions表
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions'")
        if cursor.fetchone():
            cursor.execute("PRAGMA table_info(review_sessions)")
            columns = [info[1] for info in cursor.fetchall()]
            
            if 'id' in columns:
                # review_sessions表结构
                cursor.execute('''
                SELECT id, problems_included, review_analysis, improvement_strategy, created_at
                FROM review_sessions
                WHERE id = ?
                ''', (review_id,))
                
                columns = [col[0] for col in cursor.description]
                row = cursor.fetchone()
                
                if row:
                    review = dict(zip(columns, row))
                    
                    # 将JSON字符串转换为Python对象
                    if review['problems_included']:
                        review['problems_included'] = json.loads(review['problems_included'])
                    else:
                        review['problems_included'] = []
                        
                    if review['review_analysis']:
                        try:
                            review['review_analysis'] = json.loads(review['review_analysis'])
                        except:
                            review['review_analysis'] = {}
                    else:
                        review['review_analysis'] = {}
                        
                    if review['improvement_strategy']:
                        try:
                            review['improvement_strategy'] = json.loads(review['improvement_strategy'])
                        except:
                            review['improvement_strategy'] = {}
                    else:
                        review['improvement_strategy'] = {}
                        
                    return review
        
        # 第二步：检查reviews表（新旧两种格式）
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
        if cursor.fetchone():
            # 检查表结构
            cursor.execute("PRAGMA table_info(reviews)")
            columns = [info[1] for info in cursor.fetchall()]
            
            if 'title' in columns and 'description' in columns and 'tags' in columns and 'categories' in columns:
                # 新版表结构
                cursor.execute('''
                SELECT id, title, description, tags, categories, start_date, end_date, created_at, updated_at
                FROM reviews
                WHERE id = ?
                ''', (review_id,))
                
                columns = [col[0] for col in cursor.description]
                row = cursor.fetchone()
                
                if row:
                    review = dict(zip(columns, row))
                    
                    # 解析JSON字段
                    if review.get('description'):
                        try:
                            review['review_analysis'] = json.loads(review['description'])
                        except:
                            # 如果description不是有效的JSON，但可能是问题ID列表
                            try:
                                # 尝试解析为问题ID列表
                                problem_ids = json.loads(review['description'])
                                if isinstance(problem_ids, list):
                                    review['problems_included'] = problem_ids
                                    review['review_analysis'] = {}
                                else:
                                    review['review_analysis'] = {}
                            except:
                                review['review_analysis'] = {}
                    else:
                        review['review_analysis'] = {}
                        
                    if review.get('tags'):
                        try:
                            review['tags'] = json.loads(review['tags'])
                        except:
                            pass
                            
                    if review.get('categories'):
                        try:
                            review['categories'] = json.loads(review['categories'])
                        except:
                            pass
                    
                    # 构建兼容的problems_included
                    if review.get('title'):
                        # 假设title可能包含问题ID信息
                        review['problems_included'] = [{'id': review['title']}]
                    else:
                        review['problems_included'] = []
                        
                    return review
            else:
                # 旧版表结构
                cursor.execute('''
                SELECT id, problem_id, status, review_date, notes, created_at
                FROM reviews
                WHERE id = ?
                ''', (review_id,))
                
                columns = [col[0] for col in cursor.description]
                row = cursor.fetchone()
                
                if row:
                    review = dict(zip(columns, row))
                    
                    # 构建与新版格式兼容的结构
                    if review.get('notes'):
                        try:
                            review['review_analysis'] = json.loads(review['notes'])
                        except:
                            # 尝试将notes转换为字符串
                            try:
                                review['review_analysis'] = {"notes": str(review['notes'])}
                            except:
                                review['review_analysis'] = {}
                    else:
                        review['review_analysis'] = {}
                        
                    # 从problem_id构建problems_included
                    if review.get('problem_id'):
                        review['problems_included'] = [{'id': review['problem_id']}]
                    else:
                        review['problems_included'] = []
                        
                    return review
        
        return None
    except Exception as e:
        logger.error(f"获取回顾记录失败: {str(e)}\n{traceback.format_exc()}")
        return None

def update_review_analysis(review_id, analysis_data):
    """
    更新回顾分析数据，兼容所有表结构
    
    Args:
        review_id (str): 回顾ID
        analysis_data (dict/str): 分析数据，可以是字典或JSON字符串
    
    Returns:
        bool: 更新成功返回True，否则返回False
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 格式化分析数据确保一致性
        if isinstance(analysis_data, str):
            # 尝试解析JSON字符串，如果失败则直接使用
            try:
                # 尝试解析为JSON对象
                json_obj = json.loads(analysis_data)
                analysis_json = json.dumps(json_obj, ensure_ascii=False)
            except:
                # 不是有效的JSON字符串，将其作为内容保存
                analysis_json = json.dumps({"分析结果": analysis_data}, ensure_ascii=False)
        else:
            # 对象类型，转换为JSON字符串
            analysis_json = json.dumps(analysis_data, ensure_ascii=False)
        
        logger.debug(f"格式化后的分析数据 (update_review_analysis): {analysis_json[:100]}...")
        
        updated = False
        
        # 检查表结构 - 第一步：检查review_sessions表
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions'")
        if cursor.fetchone():
            cursor.execute("PRAGMA table_info(review_sessions)")
            columns = [info[1] for info in cursor.fetchall()]
            
            if 'review_analysis' in columns:
                # review_sessions表结构
                cursor.execute('''
                UPDATE review_sessions
                SET review_analysis = ?
                WHERE id = ?
                ''', (analysis_json, review_id))
                
                updated = updated or cursor.rowcount > 0
                logger.debug(f"更新review_sessions表结果: {updated}, rowcount={cursor.rowcount}")
        
        # 第二步：检查新版reviews表
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
        if cursor.fetchone():
            # 检查表结构
            cursor.execute("PRAGMA table_info(reviews)")
            columns = [info[1] for info in cursor.fetchall()]
            
            if 'description' in columns and 'tags' in columns and 'categories' in columns:
                # 新版表结构
                cursor.execute('''
                UPDATE reviews
                SET description = ?
                WHERE id = ?
                ''', (analysis_json, review_id))
            else:
                # 旧版表结构
                cursor.execute('''
                UPDATE reviews
                SET notes = ?
                WHERE id = ?
                ''', (analysis_json, review_id))
            
            updated = updated or cursor.rowcount > 0
            logger.debug(f"更新reviews表结果: {updated}, rowcount={cursor.rowcount}")
        
        conn.commit()
        
        # 如果没有更新任何记录
        if not updated:
            logger.warning(f"未找到ID为 {review_id} 的回顾记录，或者数据未发生变化")
        
        return updated
    except Exception as e:
        logger.error(f"更新回顾分析数据失败: {str(e)}\n{traceback.format_exc()}")
        return False

def save_review_analysis(analysis_id, title, description, problem_ids, analysis_data):
    """保存回顾分析记录
    
    Args:
        analysis_id (str): 分析ID
        title (str): 分析标题
        description (str): 分析描述
        problem_ids (list): 问题ID列表
        analysis_data (dict): 分析数据
    
    Returns:
        bool: 是否保存成功
    """
    conn = get_db()
    cursor = conn.cursor()
    
    try:
        # 先检查是否已存在
        cursor.execute("SELECT id FROM review_analysis WHERE id = ?", (analysis_id,))
        existing = cursor.fetchone()
        
        # 将列表和字典转换为JSON字符串
        problem_ids_json = json.dumps(problem_ids)
        analysis_data_json = json.dumps(analysis_data)
        
        if existing:
            # 更新现有记录
            cursor.execute('''
            UPDATE review_analysis 
            SET title = ?, 
                description = ?, 
                problem_ids = ?, 
                analysis_data = ?,
                updated_at = CURRENT_TIMESTAMP
            WHERE id = ?
            ''', (title, description, problem_ids_json, analysis_data_json, analysis_id))
        else:
            # 插入新记录
            cursor.execute('''
            INSERT INTO review_analysis 
            (id, title, description, problem_ids, analysis_data)
            VALUES (?, ?, ?, ?, ?)
            ''', (analysis_id, title, description, problem_ids_json, analysis_data_json))
        
        conn.commit()
        return True
    except Exception as e:
        print(f"保存回顾分析时出错: {e}")
        return False
    finally:
        conn.close()

def get_review_analysis(analysis_id):
    """获取特定回顾分析记录
    
    Args:
        analysis_id (str): 分析ID
    
    Returns:
        dict: 分析记录，如果不存在则返回None
    """
    conn = get_db()
    cursor = conn.cursor()
    
    try:
        cursor.execute('''
        SELECT id, title, description, problem_ids, analysis_data, created_at, updated_at
        FROM review_analysis
        WHERE id = ?
        ''', (analysis_id,))
        
        row = cursor.fetchone()
        if not row:
            # 如果在review_analysis表中找不到，尝试从reviews表中获取
            try:
                # 检查reviews表是否存在
                cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
                if cursor.fetchone():
                    cursor.execute('''
                    SELECT id, notes, created_at, updated_at
                    FROM reviews
                    WHERE id = ?
                    ''', (analysis_id,))
                    
                    review_row = cursor.fetchone()
                    if review_row:
                        # 从旧表结构中构造分析对象
                        analysis_data = None
                        try:
                            # 尝试解析notes字段为JSON
                            if review_row[1]:
                                analysis_data = json.loads(review_row[1])
                        except:
                            analysis_data = {"原始内容": review_row[1]}
                        
                        return {
                            'id': review_row[0],
                            'title': f'回顾分析 #{review_row[0]}',
                            'description': review_row[1],
                            'problem_ids': [],
                            'analysis_data': analysis_data or {},
                            'created_at': review_row[2],
                            'updated_at': review_row[3]
                        }
            except Exception as e:
                logger.error(f"从reviews表获取分析数据时出错: {e}")
                # 继续返回None
                
            return None
        
        # 安全解析JSON字段
        def safe_json_loads(json_str, default=None):
            if not json_str:
                return default
            try:
                return json.loads(json_str)
            except:
                logger.warning(f"JSON解析失败: {json_str[:100]}")
                return default
        
        # 正常从review_analysis表获取数据
        problem_ids = safe_json_loads(row[3], [])
        
        # 尝试解析analysis_data
        analysis_data = None
        try:
            if row[4]:
                analysis_data = json.loads(row[4])
        except:
            logger.warning(f"解析analysis_data失败: {row[4][:100]}")
            analysis_data = {"原始内容": row[4]}
        
        # 构造结果对象
        analysis = {
            'id': row[0],
            'title': row[1],
            'description': row[2],
            'problem_ids': problem_ids,
            'analysis_data': analysis_data or {},
            'created_at': row[5],
            'updated_at': row[6]
        }
        
        return analysis
    except Exception as e:
        logger.error(f"获取回顾分析时出错: {e}\n{traceback.format_exc()}")
        return None
    finally:
        conn.close()

def get_all_review_analyses():
    """获取所有回顾分析记录
    
    Returns:
        list: 分析记录列表
    """
    conn = get_db()
    cursor = conn.cursor()
    
    try:
        analyses = []
        
        # 只获取新系统的回顾会话中包含分析结果的记录 (review_sessions表)
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions'")
        if cursor.fetchone():
            # 检查review_sessions表的列结构
            cursor.execute("PRAGMA table_info(review_sessions)")
            columns = [column[1] for column in cursor.fetchall()]
            
            if 'title' in columns and 'description' in columns:
                # 新表结构
                try:
                    cursor.execute('''
                    SELECT id, title, description, problems_included, created_at, updated_at
                    FROM review_sessions
                    WHERE (review_analysis IS NOT NULL AND review_analysis != '') 
                       OR (description IS NOT NULL AND description != '')
                    ORDER BY created_at DESC
                    ''')
                    
                    for row in cursor.fetchall():
                        # 检查是否包含有效的分析结果
                        analysis_data = None
                        problems_included = []
                        
                        # 尝试从description字段解析分析结果
                        try:
                            if row[2] and isinstance(row[2], str):
                                analysis_data = json.loads(row[2])
                        except:
                            pass
                        
                        # 尝试解析problems_included
                        try:
                            if row[3] and isinstance(row[3], str):
                                problems_included = json.loads(row[3])
                            elif isinstance(row[3], list):
                                problems_included = row[3]
                        except:
                            problems_included = []
                        
                        # 只添加包含有效分析结果的记录
                        if analysis_data and isinstance(analysis_data, dict):
                            analysis = {
                                'id': row[0],
                                'title': row[1] or f"回顾分析 #{row[0][:8]}",
                                'description': "从回顾会话中保存的分析结果",
                                'problem_count': len(problems_included),
                                'created_at': row[4],
                                'updated_at': row[5] or row[4],
                                'source': 'review_sessions',
                                'system_type': '新系统',
                                'url': f"/review/{row[0]}"
                            }
                            analyses.append(analysis)
                except Exception as e:
                    logger.error(f"获取新版review_sessions表数据时出错: {str(e)}")
                    logger.error(traceback.format_exc())
            else:
                # 旧表结构
                try:
                    cursor.execute('''
                    SELECT id, problem_id, notes, problems_included, created_at, review_date
                    FROM review_sessions
                    WHERE notes IS NOT NULL AND notes != ''
                    ORDER BY created_at DESC
                    ''')
                    
                    for row in cursor.fetchall():
                        # 检查是否包含有效的分析结果
                        analysis_data = None
                        problems_included = []
                        
                        # 尝试从notes字段解析分析结果
                        try:
                            if row[2] and isinstance(row[2], str):
                                analysis_data = json.loads(row[2])
                        except:
                            pass
                        
                        # 尝试解析problems_included
                        try:
                            if row[3] and isinstance(row[3], str):
                                problems_included = json.loads(row[3])
                            elif isinstance(row[3], list):
                                problems_included = row[3]
                        except:
                            problems_included = []
                        
                        # 只添加包含有效分析结果的记录
                        if analysis_data and isinstance(analysis_data, dict):
                            analysis = {
                                'id': row[0],
                                'title': f"回顾分析 #{row[0][:8]}",
                                'description': "从旧版回顾会话中保存的分析结果",
                                'problem_count': len(problems_included),
                                'created_at': row[4],
                                'updated_at': row[5] or row[4],
                                'source': 'review_sessions',
                                'system_type': '新系统',
                                'url': f"/review/{row[0]}"
                            }
                            analyses.append(analysis)
                except Exception as e:
                    logger.error(f"获取旧版review_sessions表数据时出错: {str(e)}")
                    logger.error(traceback.format_exc())
        
        # 获取reviews表中的记录
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
        if cursor.fetchone():
            try:
                cursor.execute("PRAGMA table_info(reviews)")
                columns = [column[1] for column in cursor.fetchall()]
                
                if 'title' in columns and 'description' in columns:
                    # 新版表结构
                    cursor.execute('''
                    SELECT id, title, description, created_at, updated_at 
                    FROM reviews
                    ORDER BY created_at DESC
                    ''')
                    
                    for row in cursor.fetchall():
                        try:
                            # 解析description字段
                            description = row[2]
                            problems = []
                            if description and isinstance(description, str):
                                try:
                                    data = json.loads(description)
                                    if isinstance(data, list):
                                        problems = data
                                    elif isinstance(data, dict) and 'problems' in data:
                                        problems = data['problems']
                                except:
                                    pass
                            
                            analysis = {
                                'id': row[0],
                                'title': row[1] or f"回顾 #{row[0][:8]}",
                                'description': description if not isinstance(description, list) and not isinstance(description, dict) else "包含错题分析结果",
                                'problem_count': len(problems) if problems else 0,
                                'created_at': row[3],
                                'updated_at': row[4] or row[3],
                                'source': 'reviews',
                                'system_type': '新系统',
                                'url': f"/review/{row[0]}"
                            }
                            analyses.append(analysis)
                        except Exception as e:
                            logger.error(f"处理reviews记录时出错: {str(e)}")
                else:
                    # 旧版表结构
                    cursor.execute('''
                    SELECT id, problem_id, notes, created_at, review_date
                    FROM reviews
                    ORDER BY created_at DESC
                    ''')
                    
                    for row in cursor.fetchall():
                        # 提取notes中的摘要信息
                        summary = row[2] or '无分析'
                        if isinstance(summary, str) and len(summary) > 100:
                            summary = summary[:100] + '...'
                        
                        analysis = {
                            'id': row[0],
                            'title': f"旧版回顾 #{row[0][:8]}",
                            'description': summary,
                            'problem_count': 1,  # 旧版每条记录只关联一个问题
                            'created_at': row[3],
                            'updated_at': row[4] or row[3],
                            'source': 'reviews',
                            'system_type': '新系统',
                            'url': f"/review/{row[0]}"
                        }
                        analyses.append(analysis)
            except Exception as e:
                logger.error(f"获取reviews表数据时出错: {str(e)}")
                logger.error(traceback.format_exc())
        
        return analyses
    except Exception as e:
        logger.error(f"获取回顾分析记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        return []

def delete_review_analysis(analysis_id):
    """
    该功能已弃用，系统已移除旧版回顾分析功能
    
    Returns:
        bool: 始终返回False表示操作失败
    """
    logger.warning(f"尝试删除已弃用的旧系统回顾分析: {analysis_id}")
    return False

def update_problem_image(problem_id, image_path):
    """
    更新错题的图片路径
    
    参数:
        problem_id (str): 错题ID
        image_path (str): 图片路径
        
    返回:
        bool: 更新成功返回True，否则返回False
    """
    try:
        conn = get_db()
        cursor = conn.cursor()
        
        # 检查问题是否存在
        cursor.execute(
            "SELECT id FROM error_problems WHERE id = ?",
            (problem_id,)
        )
        
        if cursor.fetchone() is None:
            conn.close()
            return False
        
        # 更新图片路径
        cursor.execute(
            "UPDATE error_problems SET image_path = ? WHERE id = ?",
            (image_path, problem_id)
        )
        
        conn.commit()
        conn.close()
        
        return True
    except Exception as e:
        print(f"更新错题图片路径时出错: {str(e)}")
        import traceback
        traceback.print_exc()
        return False

def load_api_config_from_db():
    """从数据库加载API配置"""
    try:
        # 连接到数据库
        conn = get_db()
        cursor = conn.cursor()
        
        # 查询所有API配置
        cursor.execute("SELECT key, value FROM api_config")
        rows = cursor.fetchall()
        
        # 关闭数据库连接
        conn.close()
        
        if not rows:
            logger.warning("数据库中没有API配置数据")
            return None
        
        # 构建配置字典
        api_config = {}
        for key, value in rows:
            try:
                # 解析JSON值
                parsed_value = json.loads(value)
                api_config[key] = parsed_value
            except json.JSONDecodeError:
                logger.warning(f"无法解析API配置值: {key}={value}")
                
        # 处理特殊字段
        # 处理api_alternatives结构，确保它存在
        if 'api_alternatives' not in api_config:
            api_config['api_alternatives'] = {}
        if 'deepseek' not in api_config['api_alternatives']:
            api_config['api_alternatives']['deepseek'] = {}
        
        # 返回完整配置
        return api_config
    except Exception as e:
        logger.error(f"从数据库加载API配置失败: {str(e)}")
        return None

def create_review_analysis(review_data=None, title=None, description=None, problem_ids=None):
    """创建新的回顾分析记录
    
    Args:
        review_data (dict): 包含分析数据的字典，新版API传递方式
        title (str): 分析标题，旧版API传递方式
        description (str): 分析描述，旧版API传递方式
        problem_ids (str): JSON格式的问题ID列表，旧版API传递方式
    
    Returns:
        str: 新创建的分析ID，如果失败则返回None
    """
    conn = get_db()
    cursor = conn.cursor()
    
    try:
        analysis_id = str(uuid.uuid4())
        now = datetime.datetime.now().isoformat()
        
        # 支持两种不同的调用方式
        if isinstance(review_data, dict):
            # 新版API调用方式
            title = review_data.get('title', f'回顾分析 ({now})')
            description = review_data.get('content', '')
            problem_ids_list = review_data.get('problem_ids', [])
            review_id = review_data.get('review_id', '')
            created_at = review_data.get('created_at', now)
            
            # 将问题ID列表转换为JSON字符串
            if isinstance(problem_ids_list, list):
                problem_ids = json.dumps(problem_ids_list)
            else:
                problem_ids = '[]'
                
            logger.debug(f"使用新版API创建回顾分析: title={title}, review_id={review_id}")
        else:
            # 旧版API调用方式
            created_at = now
            review_id = None
            logger.debug(f"使用旧版API创建回顾分析: title={title}")
        
        # 检查review_analysis表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_analysis'")
        if not cursor.fetchone():
            # 创建review_analysis表，新增review_id和analysis_data字段
            cursor.execute('''
            CREATE TABLE IF NOT EXISTS review_analysis (
                id TEXT PRIMARY KEY,
                title TEXT,
                description TEXT,
                problem_ids TEXT,
                review_id TEXT,
                analysis_data TEXT,
                created_at TEXT,
                updated_at TEXT
            )
            ''')
            conn.commit()
            
        # 检查表结构是否包含review_id和analysis_data字段
        cursor.execute("PRAGMA table_info(review_analysis)")
        columns = [info[1] for info in cursor.fetchall()]
        
        # 如果缺少必要的字段，添加这些字段
        if 'review_id' not in columns:
            cursor.execute("ALTER TABLE review_analysis ADD COLUMN review_id TEXT")
        
        if 'analysis_data' not in columns:
            cursor.execute("ALTER TABLE review_analysis ADD COLUMN analysis_data TEXT")
        
        # 插入新记录
        cursor.execute('''
        INSERT INTO review_analysis (id, title, description, problem_ids, review_id, analysis_data, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ''', (analysis_id, title, description, problem_ids, review_id, description, created_at, now))
        
        conn.commit()
        logger.info(f"成功创建回顾分析记录: id={analysis_id}, title={title}, review_id={review_id}")
        return analysis_id
    except Exception as e:
        logger.error(f"创建回顾分析记录失败: {str(e)}\n{traceback.format_exc()}")
        return None
    finally:
        conn.close()

def delete_review_session(review_id):
    """
    删除回顾会话记录
    
    Args:
        review_id (str): 回顾会话ID
        
    Returns:
        bool: 删除成功返回True，否则返回False
    """
    if not review_id:
        logger.error("删除回顾会话记录失败: 未提供有效ID")
        return False
    
    conn = get_db()
    cursor = conn.cursor()
    success = False
    
    try:
        # 首先尝试从review_sessions表删除
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='review_sessions'")
        if cursor.fetchone():
            cursor.execute("DELETE FROM review_sessions WHERE id = ?", (review_id,))
            if cursor.rowcount > 0:
                logger.info(f"成功从review_sessions表删除记录: {review_id}")
                success = True
        
        # 如果从review_sessions表删除失败，尝试从reviews表删除
        if not success:
            cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='reviews'")
            if cursor.fetchone():
                cursor.execute("DELETE FROM reviews WHERE id = ?", (review_id,))
                if cursor.rowcount > 0:
                    logger.info(f"成功从reviews表删除记录: {review_id}")
                    success = True
        
        conn.commit()
        return success
    except Exception as e:
        logger.error(f"删除回顾会话记录时出错: {str(e)}")
        logger.error(traceback.format_exc())
        conn.rollback()
        return False
    finally:
        conn.close()

def delete_review(review_id):
    """
    删除回顾记录的别名函数，调用delete_review_session
    
    Args:
        review_id (str): 回顾ID
        
    Returns:
        bool: 删除成功返回True，否则返回False
    """
    return delete_review_session(review_id)
