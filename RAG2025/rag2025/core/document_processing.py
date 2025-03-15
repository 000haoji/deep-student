#!/usr/bin/env python
# -*- coding: utf-8 -*-

"""
文档处理模块 - 处理文本清理和分割
"""

import re
from typing import List, Dict, Any, Optional, Union
from haystack import Document

from .config import logger
from .document_processor import TextSplitter

def clean_and_split_text(text: str, chunk_size: int = 512, chunk_overlap: int = 128) -> List[str]:
    """
    清理文本并分割成块
    
    Args:
        text: 要处理的原始文本
        chunk_size: 每个块的最大大小（字符数）
        chunk_overlap: 块之间的重叠大小（字符数）
        
    Returns:
        分割后的文本块列表
    """
    # 清理文本
    cleaned_text = clean_text(text)
    
    # 分割文本
    splitter = TextSplitter(chunk_size=chunk_size, chunk_overlap=chunk_overlap)
    chunks = splitter.split_text(cleaned_text)
    
    logger.info(f"文本被分割为 {len(chunks)} 个块")
    
    return chunks

def clean_text(text: str) -> str:
    """
    清理文本，去除不必要的空白字符和特殊格式
    
    Args:
        text: 要清理的原始文本
        
    Returns:
        清理后的文本
    """
    if not text:
        return ""
    
    # 替换多个空白字符为单个空格
    text = re.sub(r'\s+', ' ', text)
    
    # 去除特殊控制字符
    text = re.sub(r'[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]', '', text)
    
    # 去除多余的换行符
    text = re.sub(r'\n\s*\n+', '\n\n', text)
    
    # 去除首尾空白
    text = text.strip()
    
    return text 