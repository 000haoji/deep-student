"""
文件服务业务逻辑
处理文件上传、下载、删除等操作
"""
import os
import io
import hashlib
import mimetypes
from typing import Optional, BinaryIO, Union
from datetime import datetime, timedelta
from pathlib import Path
from minio import Minio
from minio.error import S3Error
from PIL import Image
import aiofiles

from shared.config import settings
from shared.utils.logger import LoggerMixin
from sqlalchemy.ext.asyncio import AsyncSession
from .models import FileRecord, FileType


class FileService(LoggerMixin):
    """文件服务"""
    
    def __init__(self):
        super().__init__()  # Initialize LoggerMixin
        # 初始化MinIO客户端
        self.client = None
        self.minio_available = False
        self._init_minio_client()
    
    def _init_minio_client(self):
        """初始化MinIO客户端（优雅降级）"""
        try:
            self.client = Minio(
                settings.MINIO_ENDPOINT,
                access_key=settings.MINIO_ACCESS_KEY,
                secret_key=settings.MINIO_SECRET_KEY,
                secure=settings.MINIO_SECURE
            )
            # 测试连接
            self.client.list_buckets()
            self.minio_available = True
            # 确保bucket存在
            self._ensure_bucket()
            self.log_info("MinIO client initialized successfully")
        except Exception as e:
            self.minio_available = False
            # Ensure self.logger is available before calling self.log_warning
            if not hasattr(self, 'logger'): # Should be set by LoggerMixin's __init__
                print(f"Logger not initialized in FileService. MinIO error: {e}") # Fallback print
            else:
                self.log_warning(f"MinIO is not available: {e}. File service will work in degraded mode.")
    
    def _ensure_bucket(self):
        """确保存储桶存在"""
        if not self.minio_available:
            return
            
        try:
            if not self.client.bucket_exists(settings.MINIO_BUCKET_NAME):
                self.client.make_bucket(settings.MINIO_BUCKET_NAME)
                self.log_info(f"Created bucket: {settings.MINIO_BUCKET_NAME}")
        except S3Error as e:
            self.log_error(f"Failed to ensure bucket: {e}")
    
    def _get_file_type(self, mime_type: str) -> FileType:
        """根据MIME类型判断文件类型"""
        if mime_type.startswith("image/"):
            return FileType.IMAGE
        elif mime_type == "application/pdf":
            return FileType.PDF
        elif mime_type.startswith("text/") or mime_type in [
            "application/msword",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        ]:
            return FileType.DOCUMENT
        else:
            return FileType.OTHER
    
    def _generate_object_name(self, filename: str, category: str) -> str:
        """生成对象存储名称"""
        # 使用日期分层存储
        date_path = datetime.now().strftime("%Y/%m/%d")
        # 生成唯一文件名
        timestamp = datetime.now().timestamp()
        file_ext = Path(filename).suffix
        unique_name = f"{timestamp}{file_ext}"
        
        return f"{category}/{date_path}/{unique_name}"
    
    def _calculate_file_hash(self, file_data: bytes) -> str:
        """计算文件哈希值"""
        return hashlib.md5(file_data).hexdigest()
    
    def _check_minio_available(self):
        """检查MinIO是否可用"""
        if not self.minio_available:
            raise Exception("MinIO service is not available. Please ensure MinIO is running.")
    
    async def upload_file(
        self,
        file_data: Union[bytes, BinaryIO],
        filename: str,
        category: str = "general",
        related_id: Optional[str] = None,
        description: Optional[str] = None,
        is_public: bool = False,
        db: Optional[AsyncSession] = None
    ) -> str:
        """
        上传文件
        
        Args:
            file_data: 文件数据
            filename: 文件名
            category: 文件分类
            related_id: 关联ID
            description: 描述
            is_public: 是否公开访问
            db: 数据库会话（如果提供，会记录到数据库）
            
        Returns:
            文件访问URL
        """
        self._check_minio_available()
        
        try:
            # 如果是二进制数据，转换为BytesIO
            if isinstance(file_data, bytes):
                file_stream = io.BytesIO(file_data)
                file_size = len(file_data)
            else:
                file_stream = file_data
                file_stream.seek(0, 2)  # 移到文件末尾
                file_size = file_stream.tell()
                file_stream.seek(0)  # 重置到开头
            
            # 获取MIME类型
            mime_type, _ = mimetypes.guess_type(filename)
            if not mime_type:
                mime_type = "application/octet-stream"
            
            # 生成对象名称
            object_name = self._generate_object_name(filename, category)
            
            # 上传到MinIO
            self.client.put_object(
                bucket_name=settings.MINIO_BUCKET_NAME,
                object_name=object_name,
                data=file_stream,
                length=file_size,
                content_type=mime_type
            )
            
            # 生成访问URL
            if is_public:
                # 公开访问URL
                access_url = f"http://{settings.MINIO_ENDPOINT}/{settings.MINIO_BUCKET_NAME}/{object_name}"
            else:
                # 生成预签名URL（7天有效期）
                access_url = self.client.presigned_get_object(
                    bucket_name=settings.MINIO_BUCKET_NAME,
                    object_name=object_name,
                    expires=timedelta(days=7)
                )
            
            # 如果提供了数据库会话，记录到数据库
            if db:
                file_record = FileRecord(
                    filename=object_name.split("/")[-1],
                    original_name=filename,
                    file_type=self._get_file_type(mime_type).value,
                    mime_type=mime_type,
                    size=file_size,
                    bucket_name=settings.MINIO_BUCKET_NAME,
                    object_name=object_name,
                    storage_path=f"{settings.MINIO_BUCKET_NAME}/{object_name}",
                    access_url=access_url,
                    is_public=is_public,
                    category=category,
                    related_id=related_id,
                    description=description
                )
                db.add(file_record)
                await db.commit()
            
            self.log_info(f"Uploaded file: {object_name}")
            return access_url
            
        except Exception as e:
            self.log_error(f"Failed to upload file: {e}")
            raise
    
    async def upload_image(
        self,
        image_data: bytes,
        filename: str,
        category: str = "images",
        max_width: Optional[int] = 1920,
        max_height: Optional[int] = 1080,
        quality: int = 85,
        **kwargs
    ) -> str:
        """
        上传图片（带压缩）
        
        Args:
            image_data: 图片数据
            filename: 文件名
            category: 分类
            max_width: 最大宽度
            max_height: 最大高度
            quality: 压缩质量(1-100)
            **kwargs: 其他参数传递给upload_file
            
        Returns:
            图片访问URL
        """
        try:
            # 打开图片
            image = Image.open(io.BytesIO(image_data))
            
            # 如果需要，调整大小
            if max_width and max_height:
                image.thumbnail((max_width, max_height), Image.Resampling.LANCZOS)
            
            # 转换为RGB（如果是RGBA）
            if image.mode in ('RGBA', 'LA'):
                background = Image.new('RGB', image.size, (255, 255, 255))
                background.paste(image, mask=image.split()[-1])
                image = background
            
            # 压缩图片
            output = io.BytesIO()
            image.save(output, format='JPEG', quality=quality, optimize=True)
            compressed_data = output.getvalue()
            
            # 确保文件名以.jpg结尾
            if not filename.lower().endswith(('.jpg', '.jpeg')):
                filename = Path(filename).stem + '.jpg'
            
            # 上传压缩后的图片
            return await self.upload_file(
                compressed_data,
                filename,
                category,
                **kwargs
            )
            
        except Exception as e:
            self.log_error(f"Failed to upload image: {e}")
            # 如果图片处理失败，尝试直接上传原始数据
            return await self.upload_file(image_data, filename, category, **kwargs)
    
    async def download_file(self, object_name: str) -> bytes:
        """
        下载文件
        
        Args:
            object_name: 对象名称
            
        Returns:
            文件数据
        """
        self._check_minio_available()
        
        try:
            response = self.client.get_object(
                bucket_name=settings.MINIO_BUCKET_NAME,
                object_name=object_name
            )
            data = response.read()
            response.close()
            response.release_conn()
            
            self.log_info(f"Downloaded file: {object_name}")
            return data
            
        except S3Error as e:
            self.log_error(f"Failed to download file: {e}")
            raise
    
    async def delete_file(
        self,
        object_name: str,
        db: Optional[AsyncSession] = None
    ) -> bool:
        """
        删除文件
        
        Args:
            object_name: 对象名称
            db: 数据库会话
            
        Returns:
            是否成功
        """
        self._check_minio_available()
        
        try:
            # 从MinIO删除
            self.client.remove_object(
                bucket_name=settings.MINIO_BUCKET_NAME,
                object_name=object_name
            )
            
            # 如果提供了数据库会话，软删除记录
            if db:
                from sqlalchemy import select
                result = await db.execute(
                    select(FileRecord).where(FileRecord.object_name == object_name)
                )
                file_record = result.scalar_one_or_none()
                if file_record:
                    file_record.soft_delete()
                    await db.commit()
            
            self.log_info(f"Deleted file: {object_name}")
            return True
            
        except S3Error as e:
            self.log_error(f"Failed to delete file: {e}")
            return False
    
    async def get_file_url(
        self,
        object_name: str,
        expires: int = 3600
    ) -> str:
        """
        获取文件访问URL
        
        Args:
            object_name: 对象名称
            expires: 过期时间（秒）
            
        Returns:
            访问URL
        """
        self._check_minio_available()
        
        try:
            url = self.client.presigned_get_object(
                bucket_name=settings.MINIO_BUCKET_NAME,
                object_name=object_name,
                expires=timedelta(seconds=expires)
            )
            return url
        except S3Error as e:
            self.log_error(f"Failed to get file URL: {e}")
            raise
    
    async def list_files(
        self,
        prefix: Optional[str] = None,
        recursive: bool = True
    ) -> list:
        """
        列出文件
        
        Args:
            prefix: 前缀过滤
            recursive: 是否递归
            
        Returns:
            文件列表
        """
        self._check_minio_available()
        
        try:
            objects = self.client.list_objects(
                bucket_name=settings.MINIO_BUCKET_NAME,
                prefix=prefix,
                recursive=recursive
            )
            
            files = []
            for obj in objects:
                files.append({
                    "name": obj.object_name,
                    "size": obj.size,
                    "last_modified": obj.last_modified,
                    "etag": obj.etag
                })
            
            return files
            
        except S3Error as e:
            self.log_error(f"Failed to list files: {e}")
            return []
    
    async def get_file_info(self, object_name: str) -> dict:
        """
        获取文件信息
        
        Args:
            object_name: 对象名称
            
        Returns:
            文件信息
        """
        self._check_minio_available()
        
        try:
            stat = self.client.stat_object(
                bucket_name=settings.MINIO_BUCKET_NAME,
                object_name=object_name
            )
            
            return {
                "name": object_name,
                "size": stat.size,
                "content_type": stat.content_type,
                "last_modified": stat.last_modified,
                "etag": stat.etag,
                "metadata": stat.metadata
            }
            
        except S3Error as e:
            self.log_error(f"Failed to get file info: {e}")
            raise


# 创建单例实例
file_service = FileService()
