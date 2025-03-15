"""
配置文件管理模块，用于读取和修改config.py文件
"""
import os
import re
import ast
import logging
import traceback

logger = logging.getLogger(__name__)

def read_config_file():
    """读取config.py文件内容"""
    try:
        config_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'config.py')
        logger.info(f"读取配置文件: {config_path}")
        
        with open(config_path, 'r', encoding='utf-8') as f:
            content = f.read()
            
        return content
    except Exception as e:
        logger.error(f"读取配置文件失败: {str(e)}\n{traceback.format_exc()}")
        return None

def extract_api_config(content):
    """从config.py内容中提取API_CONFIG字典"""
    try:
        # 找到API_CONFIG的定义
        pattern = r"API_CONFIG\s*=\s*\{(.*?)\}"
        match = re.search(pattern, content, re.DOTALL)
        if not match:
            logger.error("未找到API_CONFIG定义")
            return None
        
        api_config_str = "{" + match.group(1) + "}"
        
        # 使用ast模块安全地解析Python字典
        try:
            # 尝试直接解析完整字典
            api_config = ast.literal_eval(api_config_str)
            return api_config
        except:
            logger.warning("无法直接解析API_CONFIG，尝试手动提取关键配置")
            
            # 提取各个部分配置
            configs = {}
            
            # 提取openai配置
            openai_pattern = r"'openai':\s*\{(.*?)\}"
            openai_match = re.search(openai_pattern, api_config_str, re.DOTALL)
            if openai_match:
                try:
                    openai_str = "{" + openai_match.group(1) + "}"
                    configs['openai'] = ast.literal_eval(openai_str)
                except:
                    logger.warning("无法解析openai配置")
            
            # 提取deepseek配置
            deepseek_pattern = r"'deepseek':\s*\{(.*?)\}"
            deepseek_match = re.search(deepseek_pattern, api_config_str, re.DOTALL)
            if deepseek_match:
                try:
                    deepseek_str = "{" + deepseek_match.group(1) + "}"
                    configs['deepseek'] = ast.literal_eval(deepseek_str)
                except:
                    logger.warning("无法解析deepseek配置")
            
            # 提取default_models配置
            default_models_pattern = r"'default_models':\s*\{(.*?)\}"
            default_models_match = re.search(default_models_pattern, api_config_str, re.DOTALL)
            if default_models_match:
                try:
                    default_models_str = "{" + default_models_match.group(1) + "}"
                    configs['default_models'] = ast.literal_eval(default_models_str)
                except:
                    logger.warning("无法解析default_models配置")
            
            return configs
    except Exception as e:
        logger.error(f"提取API配置失败: {str(e)}\n{traceback.format_exc()}")
        return None

def update_config_file(updates):
    """更新config.py文件中的API配置
    
    Args:
        updates (dict): 要更新的配置，格式为{'openai': {...}, 'deepseek': {...}, 'default_models': {...}}
    """
    try:
        # 读取原始文件内容
        config_path = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), 'config.py')
        with open(config_path, 'r', encoding='utf-8') as f:
            content = f.read()
        
        # 备份原文件
        backup_path = config_path + '.bak'
        with open(backup_path, 'w', encoding='utf-8') as f:
            f.write(content)
        logger.info(f"已备份原配置文件到: {backup_path}")
        
        # 更新各个部分
        if 'openai' in updates:
            # 更新openai配置
            content = update_section(content, 'openai', updates['openai'])
        
        if 'deepseek' in updates:
            # 更新deepseek配置
            content = update_section(content, 'deepseek', updates['deepseek'])
        
        if 'default_models' in updates:
            # 更新default_models配置
            content = update_section(content, 'default_models', updates['default_models'])
        
        # 写入更新后的内容
        with open(config_path, 'w', encoding='utf-8') as f:
            f.write(content)
        
        logger.info("配置文件更新成功")
        return True
    except Exception as e:
        logger.error(f"更新配置文件失败: {str(e)}\n{traceback.format_exc()}")
        return False

def update_section(content, section_name, section_data):
    """更新特定部分的配置
    
    Args:
        content (str): 文件内容
        section_name (str): 配置部分名称
        section_data (dict): 新的配置数据
    
    Returns:
        str: 更新后的文件内容
    """
    try:
        pattern = f"'{section_name}':\\s*\\{{(.*?)\\}}"
        section_match = re.search(pattern, content, re.DOTALL)
        
        if not section_match:
            logger.warning(f"未找到{section_name}配置部分")
            return content
        
        # 格式化新配置为Python字典格式的字符串
        new_section = format_dict(section_data, indent=8)  # 缩进设置为8个空格
        
        # 替换配置部分
        content = content[:section_match.start(1)] + new_section + content[section_match.end(1):]
        
        return content
    except Exception as e:
        logger.error(f"更新{section_name}部分失败: {str(e)}")
        return content

def format_dict(d, indent=4):
    """格式化字典为Python代码字符串
    
    Args:
        d (dict): 要格式化的字典
        indent (int): 缩进空格数
    
    Returns:
        str: 格式化后的字符串
    """
    lines = []
    for key, value in d.items():
        # 处理不同类型的值
        if isinstance(value, str):
            # 字符串值添加引号
            formatted_value = f"'{value}'"
        elif isinstance(value, dict):
            # 递归处理嵌套字典
            nested_dict = format_dict(value, indent + 4)
            formatted_value = f"{{\n{' ' * (indent + 4)}{nested_dict}\n{' ' * indent}}}"
        else:
            # 其他类型直接转字符串
            formatted_value = str(value)
        
        # 添加键值对
        lines.append(f"{' ' * indent}'{key}': {formatted_value}")
    
    return ',\n'.join(lines)
