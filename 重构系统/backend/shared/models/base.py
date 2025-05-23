"""
数据库模型基类
"""
import uuid
from datetime import datetime
from typing import Any, Dict
from sqlalchemy import Column, DateTime, String, func
from sqlalchemy.ext.declarative import declared_attr
from sqlalchemy.orm import Mapped, mapped_column

from ..database import Base


class TimestampMixin:
    """时间戳混入类"""
    
    created_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now()
    )
    
    updated_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True),
        nullable=False,
        server_default=func.now(),
        onupdate=func.now()
    )


class UUIDMixin:
    """UUID主键混入类"""
    
    # 使用String类型存储UUID，以支持SQLite
    id: Mapped[str] = mapped_column(
        String(36),
        primary_key=True,
        default=lambda: str(uuid.uuid4()),
        nullable=False
    )


class BaseModel(Base, UUIDMixin, TimestampMixin):
    """所有模型的基类"""
    __abstract__ = True
    
    def to_dict(self) -> Dict[str, Any]:
        """转换为字典"""
        result = {}
        for column in self.__table__.columns:
            value = getattr(self, column.name)
            if isinstance(value, uuid.UUID):
                value = str(value)
            elif isinstance(value, datetime):
                value = value.isoformat()
            result[column.name] = value
        return result
    
    def update_from_dict(self, data: Dict[str, Any]) -> None:
        """从字典更新模型"""
        for key, value in data.items():
            if hasattr(self, key) and key not in ["id", "created_at"]:
                setattr(self, key, value)
    
    def __repr__(self) -> str:
        """字符串表示"""
        class_name = self.__class__.__name__
        attrs = []
        for column in self.__table__.columns:
            attrs.append(f"{column.name}={getattr(self, column.name)!r}")
        return f"{class_name}({', '.join(attrs)})"


class SoftDeleteMixin:
    """软删除混入类"""
    
    deleted_at: Mapped[datetime] = mapped_column(
        DateTime(timezone=True), 
        nullable=True
    )
    
    @property
    def is_deleted(self) -> bool:
        """是否已删除"""
        return self.deleted_at is not None
    
    def soft_delete(self) -> None:
        """软删除"""
        self.deleted_at = datetime.utcnow()
    
    def restore(self) -> None:
        """恢复删除"""
        self.deleted_at = None 