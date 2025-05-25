"""
基础模型定义
"""
import uuid
from sqlalchemy.orm import declarative_base, Mapped, mapped_column
from sqlalchemy import Column, DateTime, String, sql

class BaseModel:
    """基础模型，包含id、创建时间和更新时间"""
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()))
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())

class SoftDeleteMixin:
    """软删除混入类"""
    deleted_at = Column(DateTime, nullable=True)

# All models inheriting from Base will now automatically get id, created_at, and updated_at
Base = declarative_base(cls=BaseModel)
