"""
共享数据模型
"""
from .base import (
    BaseModel,
    TimestampMixin,
    UUIDMixin,
    SoftDeleteMixin
)

__all__ = [
    "BaseModel",
    "TimestampMixin",
    "UUIDMixin",
    "SoftDeleteMixin"
] 