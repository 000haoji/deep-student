"""
配置文件
"""
import os
import json
import sqlite3
import logging
import traceback
from datetime import datetime
import shutil
import time

# 配置日志
logging.basicConfig(level=logging.INFO)
logger = logging.getLogger(__name__)

# 基础路径
BASE_DIR = os.path.dirname(os.path.abspath(__file__))

# 数据库配置
DATABASE_PATH = os.path.join(BASE_DIR, 'math_errors.db')

# 上传文件夹配置
UPLOAD_FOLDER = os.path.join(BASE_DIR, 'uploads')
ALLOWED_EXTENSIONS = {'png', 'jpg', 'jpeg', 'gif', 'pdf'}

# 备份配置
BACKUP_FOLDER = os.path.join(BASE_DIR, 'backups')

# 确保上传目录存在
if not os.path.exists(UPLOAD_FOLDER):
    os.makedirs(UPLOAD_FOLDER)

# 确保备份目录存在
if not os.path.exists(BACKUP_FOLDER):
    os.makedirs(BACKUP_FOLDER)

# 默认API配置 - 用于初始化数据库或备用
DEFAULT_API_CONFIG = {
    # 图像分析 API - OpenAI兼容 (阿里云通义、Azure等)
    "vision_api": {
        "api_key": "", 
        "api_url": "",
        "model_name": "qwen2.5-vl-72b-instruct"
    },
    # Google Gemini API - 用于图像分析的替代选项
    "gemini_api": {
        "api_key": "",
        "api_url": "",
        "model_name": "gemini-2.0-flash"
    },
    # 文本分析 API (回顾分析)
    "text_api": {
        "api_key": "",  
        "api_url": "",  
        "model_name": "deepseek-chat"
    },
    # 阿里云OCR + Deepseek R1组合模式
    "aliyun_ocr": {
        "access_key_id": "",
        "access_key_secret": "",
        "region_id": "cn-shanghai"
    },
    # 多模态模型配置
    "multimodal_models": {
        "gpt4v": {
            "api_key": "",
            "api_url": "",
            "model_name": "gpt-4-vision-preview"
        },
        "claude3": {
            "api_key": "",
            "api_url": "",
            "model_name": "claude-3-opus-20240229"
        },
        "qwen-vl": {
            "api_key": "", 
            "api_url": "",
            "model_name": "qwen2.5-vl-72b-instruct"
        },
        "gemini": {
            "api_key": "",
            "api_url": "",
            "model_name": "gemini-2.0-flash"
        }
    },
    # 备用API端点配置
    "api_alternatives": {
        "deepseek": []
    },
    'openai': {
        'api_url': '',
        'api_key': '',
        'model': 'gpt-4'
    },
    'deepseek': {
        'api_url': '',
        'api_key': '',
        'model': 'deepseek-chat'
    },
    'default_models': {
        'extraction': 'multimodal_qwen-vl',
        'analysis': 'deepseek'
    }
}

# 多学科支持配置
DEFAULT_SUBJECT = 'math'  # 默认学科

# 学科配置
SUBJECTS = {
    "math": {
        "name": "考研数学",
        "description": "考研数学错题管理",
        "icon": "fa-square-root-alt",
        "enabled": True,
        "default_categories": ["高等数学", "线性代数", "概率论"],
        "default_tags": ["极限", "微分", "积分", "级数", "矩阵", "行列式", "概率分布", "数理统计"],
        "analysis_prompt": "这是一道考研数学题，请分析错误原因并给出正确解法。"
    },
    "english": {
        "name": "考研英语",
        "description": "考研英语错题管理",
        "icon": "fa-language",
        "enabled": True,
        "default_categories": ["阅读理解", "翻译", "写作", "完形填空"],
        "default_tags": ["词汇", "语法", "长难句", "逻辑关系", "主旨大意", "细节题", "推断题"],
        "analysis_prompt": "这是一道考研英语题，请分析错误原因并给出正确答案与解析。"
    },
    "politics": {
        "name": "考研政治",
        "description": "考研政治错题管理",
        "icon": "fa-landmark",
        "enabled": True,
        "default_categories": ["马原", "毛中特", "史纲", "思修法基"],
        "default_tags": ["原理题", "分析题", "辨析题", "多选题", "材料分析题"],
        "analysis_prompt": "这是一道考研政治题，请分析错误原因并给出正确答案与解析。"
    },
    "professional": {
        "name": "专业课",
        "description": "考研专业课错题管理",
        "icon": "fa-book",
        "enabled": True,
        "default_categories": ["专业基础课", "专业核心课"],
        "default_tags": ["概念", "原理", "方法", "计算", "应用"],
        "analysis_prompt": "这是一道考研专业课题，请分析错误原因并给出正确答案与解析。"
    }
}

# 学科分析提示词配置
SUBJECT_ANALYSIS_PROMPTS = {
    "math": {
        "full_prompt": """
分析这张数学错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的数学题目内容
2. 题目类型：确定这是考研数学中的哪个大类（高等数学/线性代数/概率论与数理统计）
3. 具体分支：在该大类下的具体分支（如极限/积分/矩阵/特征值等）
4. 错误类型：属于哪种常见错误（计算错误/概念理解错误/公式应用错误/推理逻辑错误等）
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张数学错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容（包括数学符号）
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的数学题目
   b. 题目类型：确定这是考研数学中的哪个大类（高等数学/线性代数/概率论与数理统计）
   c. 具体分支：在该大类下的具体分支（如极限/积分/矩阵/特征值等）
   d. 错误类型：属于哪种常见错误（计算错误/概念理解错误/公式应用错误/推理逻辑错误等）
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整数学题目文本",
  "题目类型": "高等数学/线性代数/概率统计中的一种",
  "具体分支": "在大类下的具体数学分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的数学教师，擅长分析学生的数学错题并提供针对性的学习建议。"
    },
    "physics": {
        "full_prompt": """
分析这张物理错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的物理题目内容
2. 题目类型：确定这是物理学中的哪个大类（力学/电磁学/热学/光学/现代物理）
3. 具体分支：在该大类下的具体分支（如牛顿力学/静电场/热力学第一定律等）
4. 错误类型：属于哪种常见错误（物理概念错误/公式使用错误/计算错误/单位错误等）
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张物理错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容（包括物理符号和公式）
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的物理题目
   b. 题目类型：确定这是物理学中的哪个大类（力学/电磁学/热学/光学/现代物理）
   c. 具体分支：在该大类下的具体分支（如牛顿力学/静电场/热力学第一定律等）
   d. 错误类型：属于哪种常见错误（物理概念错误/公式使用错误/计算错误/单位错误等）
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整物理题目文本",
  "题目类型": "力学/电磁学/热学/光学/现代物理中的一种",
  "具体分支": "在大类下的具体物理分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的物理教师，擅长分析学生的物理错题并提供针对性的学习建议。"
    },
    "chemistry": {
        "full_prompt": """
分析这张化学错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的化学题目内容
2. 题目类型：确定这是化学中的哪个大类（无机化学/有机化学/物理化学/分析化学）
3. 具体分支：在该大类下的具体分支（如化学平衡/电化学/有机反应/元素化合物等）
4. 错误类型：属于哪种常见错误（化学概念错误/反应条件错误/计算错误/平衡错误等）
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张化学错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容（包括化学符号和方程式）
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的化学题目
   b. 题目类型：确定这是化学中的哪个大类（无机化学/有机化学/物理化学/分析化学）
   c. 具体分支：在该大类下的具体分支（如化学平衡/电化学/有机反应/元素化合物等）
   d. 错误类型：属于哪种常见错误（化学概念错误/反应条件错误/计算错误/平衡错误等）
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整化学题目文本",
  "题目类型": "无机化学/有机化学/物理化学/分析化学中的一种",
  "具体分支": "在大类下的具体化学分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的化学教师，擅长分析学生的化学错题并提供针对性的学习建议。"
    },
    "english": {
        "full_prompt": """
分析这张英语错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的英语题目内容
2. 题目类型：确定这是英语中的哪个大类（阅读理解/语法/写作/翻译/词汇等）
3. 具体分支：在该大类下的具体分支（如时态/从句/词义辨析/段落理解等）
4. 错误类型：属于哪种常见错误（语法错误/词汇使用错误/理解错误/逻辑错误等）
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张英语错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容（包括所有英文内容和选项）
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的英语题目
   b. 题目类型：确定这是英语中的哪个大类（阅读理解/语法/写作/翻译/词汇等）
   c. 具体分支：在该大类下的具体分支（如时态/从句/词义辨析/段落理解等）
   d. 错误类型：属于哪种常见错误（语法错误/词汇使用错误/理解错误/逻辑错误等）
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整英语题目文本",
  "题目类型": "阅读理解/语法/写作/翻译/词汇中的一种",
  "具体分支": "在大类下的具体英语分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的英语教师，擅长分析学生的英语错题并提供针对性的学习建议。"
    },
    "politics": {
        "full_prompt": """
分析这张政治错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的政治题目内容
2. 题目类型：确定这是政治中的哪个大类（马原/毛中特/史纲/思修法基）
3. 具体分支：在该大类下的具体分支（如唯物辩证法/生产力与生产关系/改革开放等）
4. 错误类型：属于哪种常见错误（概念混淆/因果倒置/材料分析错误/观点偏离等）
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张政治错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的政治题目
   b. 题目类型：确定这是政治中的哪个大类（马原/毛中特/史纲/思修法基）
   c. 具体分支：在该大类下的具体分支（如唯物辩证法/生产力与生产关系/改革开放等）
   d. 错误类型：属于哪种常见错误（概念混淆/因果倒置/材料分析错误/观点偏离等）
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整政治题目文本",
  "题目类型": "马原/毛中特/史纲/思修法基中的一种",
  "具体分支": "在大类下的具体政治分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的政治教师，擅长分析学生的政治错题并提供针对性的学习建议。"
    },
    "professional": {
        "full_prompt": """
分析这张专业课错题图片，请提供以下结构化信息：
1. 题目原文：完整提取图片中的专业课题目内容
2. 题目类型：确定这属于专业课的哪个方向
3. 具体分支：在该方向下的具体分支
4. 错误类型：分析常见错误类型
5. 错误分析：详细分析错误的原因和产生的环节
6. 正确解法：给出完整的正确解题思路和步骤
7. 难度评估：1-5分，1最简单，5最困难
8. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签
""",
        "qwen_prompt": """
分析这张专业课错题图片，完成以下任务：

1. 首先，准确提取图片中所有文本内容（包括各种专业符号）
2. 然后，识别并分析题目内容，提供以下结构化信息：
   a. 题目原文：完整重构提取的专业课题目
   b. 题目类型：确定这属于专业课的哪个方向
   c. 具体分支：在该方向下的具体分支
   d. 错误类型：分析常见错误类型
   e. 难度评估：1-5分，1最简单，5最困难
   f. 知识点标签：提取3-5个与此题相关的核心知识点，作为标签数组

请以严格的JSON格式返回结果，不要添加任何额外解释。JSON结构如下：

{
  "题目原文": "完整专业课题目文本",
  "题目类型": "专业方向类别",
  "具体分支": "在大类下的具体专业分支",
  "错误类型": "确定的错误类型",
  "难度评估": 3,
  "知识点标签": ["标签1", "标签2", "标签3"]
}
""",
        "teacher_prompt": "你是一位专业的专业课教师，擅长分析学生的专业课错题并提供针对性的学习建议。"
    }
}

# 获取默认学科的提示词
DEFAULT_ANALYSIS_PROMPT = SUBJECT_ANALYSIS_PROMPTS.get(DEFAULT_SUBJECT, {}).get("full_prompt", "")

# 从数据库加载API配置
def load_api_config_from_db():
    """从数据库加载API配置，如果数据库中没有配置则返回默认配置"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 检查api_config表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='api_config'")
        if not cursor.fetchone():
            logger.warning("api_config表不存在，使用默认配置")
            conn.close()
            return DEFAULT_API_CONFIG.copy()
        
        # 查询所有API配置
        cursor.execute("SELECT key, value FROM api_config")
        rows = cursor.fetchall()
        conn.close()
        
        if not rows:
            logger.warning("数据库中没有API配置，使用默认配置")
            return DEFAULT_API_CONFIG.copy()
        
        # 将配置转换为字典
        db_config = {}
        for row in rows:
            key = row['key']
            value = row['value']
            try:
                # 尝试将JSON字符串解析为Python对象
                db_config[key] = json.loads(value)
                logger.info(f"加载配置: {key}, 类型: {type(db_config[key])}")
                # 特别记录api_alternatives的内容
                if key == 'api_alternatives':
                    logger.info(f"API替代配置内容: {json.dumps(db_config[key], ensure_ascii=False)}")
                    # 检查deepseek字段
                    if 'deepseek' in db_config[key]:
                        deepseek_config = db_config[key]['deepseek']
                        deepseek_type = type(deepseek_config)
                        logger.info(f"DeepSeek替代API类型: {deepseek_type}, 内容: {json.dumps(deepseek_config, ensure_ascii=False)}")
                        
                        # 统一处理DeepSeek配置格式 - 确保是字典格式
                        if isinstance(deepseek_config, list):
                            # 将列表转换为字典格式
                            new_config = {}
                            for i, api in enumerate(deepseek_config):
                                if not api or not isinstance(api, dict):
                                    continue  # 跳过无效数据
                                api_id = api.get('id') or f"api_{i+1}"
                                # 设置优先级基于列表顺序
                                if 'priority' not in api:
                                    api['priority'] = i + 1
                                new_config[api_id] = api
                            db_config[key]['deepseek'] = new_config
                            logger.info(f"已将deepseek替代API从列表转换为字典格式，共{len(new_config)}个API")
            except (json.JSONDecodeError, TypeError) as e:
                logger.error(f"解析配置 {key} 失败: {str(e)}")
                # 如果解析失败，则保持原值
                db_config[key] = value
        
        # 检查配置是否完整，缺失的部分使用默认配置
        config = DEFAULT_API_CONFIG.copy()
        for key, value in db_config.items():
            if key in config:
                if isinstance(value, dict) and isinstance(config[key], dict):
                    # 对于嵌套的字典，进行更新
                    config[key].update(value)
                else:
                    # 对于非字典值或顶层键，直接替换
                    config[key] = value
            else:
                # 对于新增的键，直接添加
                config[key] = value
        
        # 确保api_alternatives结构完整并保持其格式（统一使用字典格式）
        if 'api_alternatives' not in config:
            logger.info("未找到api_alternatives配置，使用默认配置")
            config['api_alternatives'] = DEFAULT_API_CONFIG['api_alternatives'].copy()
        elif 'deepseek' not in config['api_alternatives']:
            logger.info("未找到api_alternatives.deepseek配置，使用默认配置")
            config['api_alternatives']['deepseek'] = {}
        else:
            # 确保deepseek配置是字典格式
            deepseek_config = config['api_alternatives']['deepseek']
            if isinstance(deepseek_config, list):
                # 将列表转换为字典
                new_config = {}
                for i, api in enumerate(deepseek_config):
                    if not api or not isinstance(api, dict):
                        continue
                    api_id = api.get('id') or f"api_{i+1}"
                    if 'priority' not in api:
                        api['priority'] = i + 1
                    new_config[api_id] = api
                config['api_alternatives']['deepseek'] = new_config
                logger.info(f"已将api_alternatives.deepseek从列表转换为字典格式")
            
            # 记录加载的DeepSeek替代API配置类型和内容
            deepseek_type = type(config['api_alternatives']['deepseek'])
            logger.info(f"加载api_alternatives.deepseek配置成功，类型: {deepseek_type}")
            
            if deepseek_type is dict:
                logger.info(f"字典格式的DeepSeek替代API包含 {len(config['api_alternatives']['deepseek'])} 个键值对")
            else:
                logger.warning(f"未知格式的DeepSeek替代API: {deepseek_type}")
        
        logger.info("成功从数据库加载API配置")
        return config
        
    except Exception as e:
        logger.error(f"从数据库加载API配置失败: {str(e)}\n{traceback.format_exc()}")
        logger.info("使用默认API配置")
        return DEFAULT_API_CONFIG.copy()

# 初始化API配置数据库
def init_api_config_db():
    """初始化数据库中的API配置表，如果表不存在则创建，但不填充默认数据"""
    try:
        conn = sqlite3.connect(DATABASE_PATH)
        cursor = conn.cursor()
        
        # 检查api_config表是否存在
        cursor.execute("SELECT name FROM sqlite_master WHERE type='table' AND name='api_config'")
        if not cursor.fetchone():
            logger.info("创建api_config表")
            cursor.execute('''
            CREATE TABLE IF NOT EXISTS api_config (
                key TEXT PRIMARY KEY,
                value TEXT,
                updated_at TEXT
            )
            ''')
            
            conn.commit()
            logger.info("成功创建API配置表")
        else:
            logger.info("数据库中已有API配置表，检查是否需要更新结构")
            
            # 这里可以添加表结构更新逻辑（如果将来需要）
            
        conn.close()
        
    except Exception as e:
        logger.error(f"初始化API配置数据库失败: {str(e)}\n{traceback.format_exc()}")

# 更新数据库中的API配置并重新加载
def update_api_config_in_db(key, value):
    """更新数据库中的特定API配置并重新加载全局配置
    
    Args:
        key: 配置键
        value: 配置值（将自动转换为JSON）
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
        
        # 记录要更新的值的类型和部分内容
        value_type = type(value)
        logger.info(f"更新配置 {key}，类型: {value_type}")
        
        # 特别处理api_alternatives
        if key == 'api_alternatives' and isinstance(value, dict) and 'deepseek' in value:
            deepseek_type = type(value['deepseek'])
            logger.info(f"保存的DeepSeek替代API类型: {deepseek_type}")
            if deepseek_type is dict:
                logger.info(f"字典格式的DeepSeek替代API包含 {len(value['deepseek'])} 个键值对")
            elif deepseek_type is list:
                logger.info(f"列表格式的DeepSeek替代API包含 {len(value['deepseek'])} 个项目")
                # 记录列表中的每个项目
                for i, item in enumerate(value['deepseek']):
                    logger.info(f"DeepSeek替代API项目 {i+1}: {json.dumps(item, ensure_ascii=False)}")
            else:
                logger.warning(f"未知格式的DeepSeek替代API: {deepseek_type}")
        
        # 将值转换为JSON字符串
        if isinstance(value, (dict, list)):
            json_value = json.dumps(value)
        else:
            json_value = value
            
        timestamp = datetime.now().isoformat()
        
        # 使用REPLACE策略，自动处理插入或更新
        cursor.execute(
            "REPLACE INTO api_config (key, value, updated_at) VALUES (?, ?, ?)",
            (key, json_value, timestamp)
        )
        
        conn.commit()
        conn.close()
        
        # 更新全局API_CONFIG变量
        global API_CONFIG
        API_CONFIG = load_api_config_from_db()
        
        logger.info(f"成功更新API配置: {key}")
        return True
    except Exception as e:
        logger.error(f"更新API配置失败: {str(e)}\n{traceback.format_exc()}")
        return False

# 保存完整的API设置
def save_api_settings(config_dict):
    """保存完整的API设置
    
    Args:
        config_dict: 包含所有API设置的字典
    
    Returns:
        bool: 保存是否成功
    """
    try:
        logger.info(f"准备保存API设置，包含 {len(config_dict)} 个配置项")
        
        # 特别检查并记录api_alternatives配置
        if 'api_alternatives' in config_dict:
            api_alternatives = config_dict['api_alternatives']
            logger.info(f"API替代配置类型: {type(api_alternatives)}")
            
            if isinstance(api_alternatives, dict) and 'deepseek' in api_alternatives:
                deepseek_type = type(api_alternatives['deepseek'])
                logger.info(f"DeepSeek替代API类型: {deepseek_type}")
                
                if deepseek_type is dict:
                    logger.info(f"字典格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个键值对")
                elif deepseek_type is list:
                    logger.info(f"列表格式的DeepSeek替代API包含 {len(api_alternatives['deepseek'])} 个项目")
                    for i, item in enumerate(api_alternatives['deepseek']):
                        logger.info(f"DeepSeek替代API项目 {i+1}: {json.dumps(item, ensure_ascii=False)}")
                else:
                    logger.warning(f"未知格式的DeepSeek替代API: {deepseek_type}")
        
        # 遍历所有配置项并逐个更新
        success = True
        for key, value in config_dict.items():
            logger.info(f"保存配置项: {key}")
            if not update_api_config_in_db(key, value):
                success = False
                logger.error(f"保存API设置失败: {key}")
        
        # 更新全局API_CONFIG变量
        if success:
            global API_CONFIG
            API_CONFIG = load_api_config_from_db()
            logger.info("成功保存所有API设置")
        
        return success
    except Exception as e:
        logger.error(f"保存API设置失败: {str(e)}\n{traceback.format_exc()}")
        return False

# 全局API配置 - 从数据库加载或使用默认值
API_CONFIG = load_api_config_from_db()

# 确保数据库中有API配置
init_api_config_db()

# 尝试从配置文件加载（向后兼容，迁移期间使用）
try:
    if os.path.exists('config.json'):
        logger.info("发现config.json文件，尝试迁移配置到数据库...")
        with open('config.json', 'r', encoding='utf-8') as f:
            file_config = json.load(f)
            # 检查是否包含有效的配置
            has_changes = False
            
            if 'openai' in file_config and file_config['openai'].get('api_key'):
                update_api_config_in_db('openai', file_config['openai'])
                has_changes = True
                logger.info("已迁移OpenAI配置到数据库")
                
            if 'deepseek' in file_config and file_config['deepseek'].get('api_key'):
                update_api_config_in_db('deepseek', file_config['deepseek'])
                has_changes = True
                logger.info("已迁移DeepSeek配置到数据库")
                
            if 'default_models' in file_config:
                update_api_config_in_db('default_models', file_config['default_models'])
                has_changes = True
                logger.info("已迁移默认模型配置到数据库")
                
            if has_changes:
                # 重新加载全局配置
                API_CONFIG = load_api_config_from_db()
                # 备份并重命名配置文件，防止重复加载
                backup_file = 'config.json.backup'
                if not os.path.exists(backup_file):
                    try:
                        shutil.copy('config.json', backup_file)
                        logger.info(f"已备份config.json为{backup_file}")
                    except Exception as e:
                        logger.error(f"备份配置文件失败: {str(e)}")
except Exception as e:
    logger.error(f"加载配置文件失败: {str(e)}\n{traceback.format_exc()}")

# 数学题目大分类
MATH_CATEGORIES = {
    "高等数学": ["极限", "导数", "微分", "积分", "级数", "多元函数微分", "多元函数积分", "常微分方程"],
    "线性代数": ["行列式", "矩阵", "向量", "线性方程组", "特征值", "特征向量", "二次型", "线性空间"],
    "概率论与数理统计": ["随机事件", "随机变量", "数字特征", "大数定律", "中心极限定理", "参数估计", "假设检验", "回归分析"]
}

# 常见错误类型
ERROR_TYPES = [
    "计算错误", "概念理解错误", "公式应用错误", "推理逻辑错误", 
    "审题错误", "解题方法选择错误", "粗心大意", "时间不足"
]

# 获取默认模型配置
def get_default_models():
    """获取默认模型配置，处理解析错误"""
    try:
        # 从数据库获取配置
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 查询default_models配置
        cursor.execute("SELECT value FROM api_config WHERE key='default_models'")
        row = cursor.fetchone()
        
        if row and row['value']:
            try:
                # 尝试解析JSON
                models = json.loads(row['value'])
                logger.info(f"从数据库加载default_models成功: {models}")
                return models
            except json.JSONDecodeError:
                logger.error(f"解析default_models失败: {row['value']}")
        
        # 查询单独的字段
        extraction = None
        analysis = None
        
        cursor.execute("SELECT value FROM api_config WHERE key='default_models.extraction'")
        row = cursor.fetchone()
        if row and row['value']:
            extraction = row['value']
        
        cursor.execute("SELECT value FROM api_config WHERE key='default_models.analysis'")
        row = cursor.fetchone()
        if row and row['value']:
            analysis = row['value']
        
        conn.close()
        
        # 如果至少有一个字段存在，返回组合的字典
        if extraction or analysis:
            result = {
                "extraction": extraction or "multimodal_qwen-vl",
                "analysis": analysis or "deepseek"
            }
            logger.info(f"从单独字段加载default_models: {result}")
            return result
        
        # 使用默认值
        logger.info("使用默认的models配置")
        return {
            "extraction": "multimodal_qwen-vl",
            "analysis": "deepseek"
        }
    
    except Exception as e:
        logger.error(f"获取默认模型配置失败: {str(e)}")
        # 返回默认值
        return {
            "extraction": "multimodal_qwen-vl",
            "analysis": "deepseek"
        }

# 设置默认模型配置
def set_default_models(extraction_model, analysis_model):
    """设置默认模型配置"""
    try:
        # 构建配置字典
        models = {
            "extraction": extraction_model,
            "analysis": analysis_model
        }
        
        # 存储为整体配置
        update_api_config_in_db('default_models', models)
        
        # 同时存储单独的字段（向后兼容）
        update_api_config_in_db('default_models.extraction', extraction_model)
        update_api_config_in_db('default_models.analysis', analysis_model)
        
        return True
    except Exception as e:
        logger.error(f"设置默认模型失败: {str(e)}")
        return False

# 获取DeepSeek API配置
def get_deepseek_api_config():
    """获取DeepSeek API配置"""
    try:
        # 连接数据库
        conn = sqlite3.connect(DATABASE_PATH)
        conn.row_factory = sqlite3.Row
        cursor = conn.cursor()
        
        # 查询API替代配置
        cursor.execute("SELECT value FROM api_config WHERE key=?", ("api_alternatives.deepseek",))
        alt_row = cursor.fetchone()
        
        # 初始化替代API字典
        alt_apis = {}
        
        # 处理替代API配置
        if alt_row and alt_row['value']:
            try:
                alt_apis = json.loads(alt_row['value'])
                logger.info(f"加载了 {len(alt_apis)} 个DeepSeek API")
            except json.JSONDecodeError:
                logger.error(f"解析DeepSeek API配置失败: {alt_row['value']}")
        
        conn.close()
        
        # 返回完整的配置
        return {
            "apis": alt_apis
        }
    
    except Exception as e:
        logger.error(f"获取DeepSeek API配置失败: {str(e)}")
        # 返回默认值
        return {
            "apis": {}
        }

# 保存DeepSeek API配置
def save_deepseek_api_config(apis):
    """保存DeepSeek API配置"""
    try:
        # 确保APIs是字典格式
        if not isinstance(apis, dict):
            if isinstance(apis, list):
                new_apis = {}
                for i, api in enumerate(apis):
                    if isinstance(api, dict) and 'api_key' in api:
                        api_id = api.get('id') or f"api_{i}_{int(time.time())}"
                        # 设置优先级
                        if 'priority' not in api:
                            api['priority'] = i + 1
                        new_apis[api_id] = api
                apis = new_apis
            else:
                apis = {}
        
        # 保存到数据库
        update_api_config_in_db('api_alternatives.deepseek', apis)
        
        logger.info(f"保存DeepSeek API配置: {len(apis)}个API")
        return True
    
    except Exception as e:
        logger.error(f"保存DeepSeek API配置失败: {str(e)}")
        return False

# 获取可用的DeepSeek API
def get_available_deepseek_api():
    """获取可用的DeepSeek API配置"""
    config = get_deepseek_api_config()
    
    # 收集所有API
    all_apis = []
    
    # 添加所有API
    apis = config['apis']
    if apis and isinstance(apis, dict):
        for api_id, api in apis.items():
            if api and api.get('api_key'):
                if 'priority' not in api:
                    api['priority'] = 999  # 默认低优先级
                all_apis.append(api)
    
    # 按优先级排序
    all_apis.sort(key=lambda x: x.get('priority', 999))
    
    # 返回排序后的API列表
    if all_apis:
        logger.info(f"找到{len(all_apis)}个可用的DeepSeek API")
        return all_apis
    else:
        logger.warning("未找到可用的DeepSeek API")
        return []

# 筛选并返回可用的DeepSeek API密钥
def filter_deepseek_keys(api_config):
    """
    筛选并返回可用的DeepSeek API密钥
    """
    if not api_config:
        return []
        
    deepseek_keys = api_config.get('api_alternatives', {}).get('deepseek', [])
    
    if deepseek_keys and isinstance(deepseek_keys, list) and len(deepseek_keys) > 0:
        logger.info(f"找到 {len(deepseek_keys)} 个DeepSeek API配置")
        return deepseek_keys
    else:
        logger.warning("未找到可用的DeepSeek API")
        return []

# 尝试从文件加载学科配置
def load_subjects_from_file():
    try:
        data_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data')
        subjects_file = os.path.join(data_dir, 'subjects.json')
        
        if os.path.exists(subjects_file):
            with open(subjects_file, 'r', encoding='utf-8') as f:
                custom_subjects = json.load(f)
                if custom_subjects and isinstance(custom_subjects, dict):
                    # 更新学科配置，而不是替换
                    SUBJECTS.update(custom_subjects)
                    logger.info(f"成功从 {subjects_file} 加载了自定义学科配置")
    except Exception as e:
        logger.error(f"加载学科配置失败: {str(e)}")

# 尝试从文件加载提示词配置
def load_prompts_from_file():
    try:
        data_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'data')
        prompts_file = os.path.join(data_dir, 'subject_prompts.json')
        
        if os.path.exists(prompts_file):
            with open(prompts_file, 'r', encoding='utf-8') as f:
                custom_prompts = json.load(f)
                if custom_prompts and isinstance(custom_prompts, dict):
                    # 更新提示词配置，而不是替换
                    SUBJECT_ANALYSIS_PROMPTS.update(custom_prompts)
                    logger.info(f"成功从 {prompts_file} 加载了自定义提示词配置")
    except Exception as e:
        logger.error(f"加载提示词配置失败: {str(e)}")

# 启动时加载配置
load_subjects_from_file()
load_prompts_from_file()

# 加载配置文件
def load_config():
    try:
        with open('config.json', 'r', encoding='utf-8') as f:
            config = json.load(f)
            return config
    except Exception as e:
        logger.error(f"加载配置文件失败: {str(e)}")
        return {}

# 保存配置文件
def save_config(config):
    try:
        with open('config.json', 'w', encoding='utf-8') as f:
            json.dump(config, f, ensure_ascii=False, indent=4)
            logger.info("配置文件保存成功")
    except Exception as e:
        logger.error(f"保存配置文件失败: {str(e)}")

# 加载配置
def load_settings():
    try:
        config = load_config()
        if config:
            logger.info("从配置文件加载配置")
            return config
        else:
            logger.info("配置文件为空，使用默认配置")
            return DEFAULT_API_CONFIG.copy()
    except Exception as e:
        logger.error(f"加载配置失败: {str(e)}")
        return DEFAULT_API_CONFIG.copy()

# 保存配置
def save_settings(config):
    try:
        save_config(config)
        logger.info("配置保存成功")
    except Exception as e:
        logger.error(f"保存配置失败: {str(e)}")

# 加载配置
config = load_settings()

# 保存配置
save_settings(config)
