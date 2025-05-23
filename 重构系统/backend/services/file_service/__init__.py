"""
文件服务
处理文件上传、存储和管理
"""

from .models import FileRecord, FileType
# 临时注释掉，避免导入依赖问题
from .service import FileService, file_service

__all__ = [
    "FileRecord",
    "FileType",
    "FileService",
    "file_service"
] 