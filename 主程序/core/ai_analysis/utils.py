"""
AI分析辅助工具模块
"""
import re

def extract_field_from_text(text, field_name):
    """从自由文本中提取特定字段的内容"""
    pattern = rf"{field_name}[:：]\s*(.*?)(?:\n|$)"
    match = re.search(pattern, text, re.DOTALL)
    if match:
        return match.group(1).strip()
    return None

def extract_number_from_text(text, field_name):
    """从自由文本中提取数字"""
    pattern = rf"{field_name}[:：]\s*(\d+)"
    match = re.search(pattern, text)
    if match:
        return int(match.group(1))
    return None

def extract_tags_from_text(text):
    """从自由文本中提取标签列表"""
    pattern = r"知识点标签[:：]\s*(.*?)(?:\n|$)"
    match = re.search(pattern, text, re.DOTALL)
    if match:
        tags_text = match.group(1).strip()
        # 尝试分割标签
        tags = re.split(r'[,，、\s]+', tags_text)
        return [tag.strip() for tag in tags if tag.strip()]
    return []
