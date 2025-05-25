"""
基础模型定义
"""
from sqlalchemy.orm import declarative_base
from sqlalchemy import Column, DateTime, sql

class BaseModel:
    """基础模型，包含创建时间和更新时间"""
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())

class SoftDeleteMixin:
    """软删除混入类"""
    deleted_at = Column(DateTime, nullable=True)

Base = declarative_base(cls=BaseModel) 