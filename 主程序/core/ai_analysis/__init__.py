"""
AI分析模块 - 将大文件按照功能拆分为多个子模块
"""

# 导入适配器类 - 提前导入
from .deepseek_adapter import DeepSeekAdapter

# 导入工具函数
from .utils import (
    extract_field_from_text,
    extract_number_from_text,
    extract_tags_from_text
)

# 导入单一模型分析模块
from .single_model import (
    analyze_with_openai_compat_api,
    analyze_with_gemini_api
)

# 导入组合模型分析模块
from .combined_model import (
    analyze_with_multimodal_and_deepseek,
    analyze_with_ocr_and_deepseek,
    safe_text_logging # 导入安全日志函数
)

# 导入回顾分析模块
from .review_analysis import analyze_with_text_api

# 导出所有函数，保持API兼容性
__all__ = [
    'DeepSeekAdapter',  # 导出适配器类
    'analyze_with_openai_compat_api',
    'analyze_with_gemini_api',
    'analyze_with_multimodal_and_deepseek',
    'analyze_with_ocr_and_deepseek',
    'analyze_with_text_api',
    'extract_field_from_text',
    'extract_number_from_text',
    'extract_tags_from_text',
    'safe_text_logging' # 导出安全日志函数
]
