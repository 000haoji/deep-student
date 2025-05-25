"""
文件服务API的Pydantic模型
"""
from pydantic import BaseModel, HttpUrl
from typing import Optional
from datetime import datetime

from .models import FileType # 确保可以从同级models导入

class FileUploadResponse(BaseModel):
    """文件上传成功响应"""
    filename: str
    original_name: str
    file_type: FileType
    mime_type: str
    size: int
    access_url: HttpUrl
    object_name: str
    category: Optional[str] = None
    created_at: datetime

    class Config:
        from_attributes = True # For Pydantic V2
        # orm_mode = True
        # Pydantic V2 or_mode renamed to from_attributes

class FileDetailResponse(BaseModel):
    """文件详细信息响应"""
    filename: str
    original_name: str
    file_type: FileType
    mime_type: str
    size: int
    bucket_name: str
    object_name: str
    storage_path: str
    access_url: HttpUrl
    is_public: bool
    category: Optional[str] = None
    related_id: Optional[str] = None
    description: Optional[str] = None
    created_at: datetime
    updated_at: Optional[datetime] = None
    deleted_at: Optional[datetime] = None

    class Config:
        from_attributes = True # For Pydantic V2
        # orm_mode = True

class PresignedUrlResponse(BaseModel):
    """预签名URL响应"""
    object_name: str
    url: HttpUrl
    expires_in_seconds: int
