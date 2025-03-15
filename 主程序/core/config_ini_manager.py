"""
INI配置文件管理模块，用于读取和修改config.ini文件
同时会自动与数据库配置保持同步
"""
import os
import configparser
import logging
from core.database import update_api_config as update_db_api_config, get_api_config as get_db_api_config

logger = logging.getLogger(__name__)

# 配置文件路径
CONFIG_PATH = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'config.ini')

def get_config():
    """读取配置文件，返回ConfigParser对象"""
    config = configparser.ConfigParser()
    
    try:
        if os.path.exists(CONFIG_PATH):
            config.read(CONFIG_PATH, encoding='utf-8')
            logger.info(f"成功读取配置文件: {CONFIG_PATH}")
        else:
            logger.warning(f"配置文件不存在: {CONFIG_PATH}，将使用默认值")
            create_default_config(config)
    except Exception as e:
        logger.error(f"读取配置文件失败: {str(e)}")
        create_default_config(config)
    
    return config

def create_default_config(config):
    """创建默认配置，并同步到数据库"""
    # 尝试首先从数据库获取配置
    db_config = get_db_api_config()
    
    # API部分
    config['API'] = {
        'openai_api_url': db_config.get('openai', {}).get('api_url', 'https://dashscope.aliyuncs.com/compatible-mode/v1'),
        'openai_api_key': db_config.get('openai', {}).get('api_key', 'sk-92eb4ac9124042118fa9dfa1014347c6'),
        'deepseek_api_url': db_config.get('deepseek', {}).get('api_url', 'https://api.deepseek.com/v1'),
        'deepseek_api_key': db_config.get('deepseek', {}).get('api_key', 'sk-d3377feb708b4d4fac4cb9298119cb48'),
        'default_extraction_model': db_config.get('default_models', {}).get('extraction', 'multimodal_qwen-vl'),
        'default_analysis_model': db_config.get('default_models', {}).get('analysis', 'deepseek')
    }
    
    # VISION_API部分
    config['VISION_API'] = {
        'api_key': db_config.get('vision_api', {}).get('api_key', 'sk-92eb4ac9124042118fa9dfa1014347c6'),
        'api_url': db_config.get('vision_api', {}).get('api_url', 'https://dashscope.aliyuncs.com/compatible-mode/v1/chat/completions'),
        'model_name': db_config.get('vision_api', {}).get('model_name', 'qwen2.5-vl-72b-instruct')
    }
    
    # TEXT_API部分
    config['TEXT_API'] = {
        'api_key': db_config.get('text_api', {}).get('api_key', 'sk-d3377feb708b4d4fac4cb9298119cb48'),
        'api_url': db_config.get('text_api', {}).get('api_url', 'https://api.deepseek.com/v1/chat/completions'),
        'model_name': db_config.get('text_api', {}).get('model_name', 'deepseek-chat')
    }
    
    # PATHS部分
    config['PATHS'] = {
        'upload_folder': 'uploads',
        'database_path': 'math_errors.db'
    }
    
    # 保存默认配置
    try:
        with open(CONFIG_PATH, 'w', encoding='utf-8') as f:
            config.write(f)
        logger.info(f"已创建默认配置文件: {CONFIG_PATH}")
    except Exception as e:
        logger.error(f"创建默认配置文件失败: {str(e)}")

def save_config(config):
    """保存配置到文件"""
    try:
        with open(CONFIG_PATH, 'w', encoding='utf-8') as f:
            config.write(f)
        logger.info(f"配置已保存到: {CONFIG_PATH}")
        return True
    except Exception as e:
        logger.error(f"保存配置文件失败: {str(e)}")
        return False

def update_api_config(updates):
    """更新API部分的配置，并同步到数据库
    
    Args:
        updates (dict): 要更新的配置，键值对
    
    Returns:
        bool: 更新是否成功
    """
    try:
        config = get_config()
        
        # 确保API部分存在
        if 'API' not in config:
            config['API'] = {}
        
        # 更新API配置
        for key, value in updates.items():
            # 确保值不是None或空字符串时才保存
            if value is not None and value != '':
                config['API'][key] = value
        
        # 同步到数据库
        sync_config_to_db(config)
        
        # 保存并返回结果
        return save_config(config)
    except Exception as e:
        logger.error(f"更新API配置失败: {str(e)}")
        return False

def sync_config_to_db(config):
    """将INI配置同步到数据库"""
    try:
        # 同步API配置
        if 'API' in config:
            api_config = dict(config['API'])
            # 构建OpenAI配置
            openai_config = {
                'api_url': api_config.get('openai_api_url', ''),
                'api_key': api_config.get('openai_api_key', '')
            }
            # 构建DeepSeek配置
            deepseek_config = {
                'api_url': api_config.get('deepseek_api_url', ''),
                'api_key': api_config.get('deepseek_api_key', '')
            }
            # 构建默认模型配置
            default_models = {
                'extraction': api_config.get('default_extraction_model', ''),
                'analysis': api_config.get('default_analysis_model', '')
            }
            # 更新到数据库
            if openai_config['api_key'] and openai_config['api_url']:
                update_db_api_config('openai', openai_config)
            if deepseek_config['api_key'] and deepseek_config['api_url']:
                update_db_api_config('deepseek', deepseek_config)
            if default_models['extraction'] or default_models['analysis']:
                update_db_api_config('default_models', default_models)
        
        # 同步VISION_API配置
        if 'VISION_API' in config:
            vision_config = dict(config['VISION_API'])
            if any(vision_config.values()):
                update_db_api_config('vision_api', vision_config)
        
        # 同步TEXT_API配置
        if 'TEXT_API' in config:
            text_config = dict(config['TEXT_API'])
            if any(text_config.values()):
                update_db_api_config('text_api', text_config)
        
        logger.info("已同步INI配置到数据库")
        return True
    except Exception as e:
        logger.error(f"同步配置到数据库失败: {str(e)}")
        return False

def get_api_config():
    """获取API配置
    
    Returns:
        dict: API配置的字典
    """
    try:
        config = get_config()
        if 'API' in config:
            return dict(config['API'])
        else:
            return {}
    except Exception as e:
        logger.error(f"获取API配置失败: {str(e)}")
        return {}

def get_vision_api_config():
    """获取图像分析API配置"""
    try:
        config = get_config()
        if 'VISION_API' in config:
            return dict(config['VISION_API'])
        else:
            return {}
    except Exception as e:
        logger.error(f"获取VISION_API配置失败: {str(e)}")
        return {}

def get_text_api_config():
    """获取文本API配置"""
    try:
        config = get_config()
        if 'TEXT_API' in config:
            return dict(config['TEXT_API'])
        else:
            return {}
    except Exception as e:
        logger.error(f"获取TEXT_API配置失败: {str(e)}")
        return {}

def get_paths_config():
    """获取路径配置"""
    try:
        config = get_config()
        if 'PATHS' in config:
            return dict(config['PATHS'])
        else:
            return {
                'upload_folder': 'uploads',
                'database_path': 'math_errors.db'
            }
    except Exception as e:
        logger.error(f"获取PATHS配置失败: {str(e)}")
        return {
            'upload_folder': 'uploads',
            'database_path': 'math_errors.db'
        }
