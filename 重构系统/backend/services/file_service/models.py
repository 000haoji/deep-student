"""
文件服务的数据模型
"""
from enum import Enum
from typing import Optional, Dict, Any
from sqlalchemy import String, Integer, Boolean, JSON
from sqlalchemy.dialects.postgresql import UUID
from sqlalchemy.orm import Mapped, mapped_column

from shared.models.base import BaseModel


class FileType(str, Enum):
    """文件类型枚举"""
    IMAGE = "image"
    DOCUMENT = "document"
    AUDIO = "audio"
    VIDEO = "video"
    OTHER = "other"


class StorageProvider(str, Enum):
    """存储提供商"""
    LOCAL = "local"
    MINIO = "minio"
    S3 = "s3"
    OSS = "oss"


class FileRecord(BaseModel):
    """文件记录"""
    __tablename__ = "file_records"
    
    # 文件基本信息
    filename: Mapped[str] = mapped_column(String(255), nullable=False, unique=True)
    original_filename: Mapped[str] = mapped_column(String(255), nullable=False)
    file_type: Mapped[str] = mapped_column(String(20), nullable=False)
    mime_type: Mapped[str] = mapped_column(String(100), nullable=False)
    file_size: Mapped[int] = mapped_column(Integer, nullable=False)  # 字节
    
    # 存储信息
    storage_provider: Mapped[str] = mapped_column(String(20), nullable=False)
    storage_path: Mapped[str] = mapped_column(String(500), nullable=False)
    storage_url: Mapped[Optional[str]] = mapped_column(String(1000), nullable=True)
    
    # 元数据
    file_metadata: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)
    
    # 访问控制
    is_public: Mapped[bool] = mapped_column(Boolean, default=False)
    access_count: Mapped[int] = mapped_column(Integer, default=0)
    
    # 关联信息
    related_type: Mapped[Optional[str]] = mapped_column(String(50), nullable=True)  # problem, analysis, etc.
    related_id: Mapped[Optional[str]] = mapped_column(String(100), nullable=True)
