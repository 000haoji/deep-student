"""
AI API管理服务的数据模型
"""
from enum import Enum
from typing import List, Optional, Dict, Any
from datetime import datetime
from pydantic import BaseModel, Field, validator
from sqlalchemy import String, Integer, Float, Boolean, JSON, Enum as SQLEnum, DateTime, Text
from sqlalchemy.dialects.postgresql import UUID
from sqlalchemy.sql import func
from sqlalchemy.orm import Mapped, mapped_column
import uuid

from shared.models.base import BaseModel as SQLBaseModel


class AIProvider(str, Enum):
    """AI提供商枚举"""
    OPENAI = "openai"
    GEMINI = "gemini" 
    DEEPSEEK = "deepseek"
    CLAUDE = "claude"
    QWEN = "qwen"
    
    @classmethod
    def has_value(cls, value: str) -> bool:
        return value in cls._value2member_map_


class AICapability(str, Enum):
    """AI能力枚举"""
    TEXT = "text"
    VISION = "vision"
    EMBEDDING = "embedding"
    AUDIO = "audio"
    
    
class TaskType(str, Enum):
    """任务类型枚举"""
    OCR = "ocr"
    PROBLEM_ANALYSIS = "problem_analysis"
    REVIEW_ANALYSIS = "review_analysis"
    SUMMARIZATION = "summarization"
    TRANSLATION = "translation"


# Pydantic模型（API请求/响应）
class AIModelConfig(BaseModel):
    """AI模型配置"""
    provider: AIProvider
    model_name: str = Field(..., alias='modelName')  # 避免pydantic警告
    api_key: str
    api_url: str
    priority: int = Field(default=1, ge=1, le=10)
    is_active: bool = True
    capabilities: List[AICapability]
    cost_per_1k_tokens: float = Field(default=0.0, ge=0)
    max_tokens: int = Field(default=4096, gt=0)
    timeout: int = Field(default=30, gt=0)
    max_retries: int = Field(default=3, ge=0)
    custom_headers: Optional[Dict[str, str]] = None
    
    @validator("capabilities")
    def validate_capabilities(cls, v):
        if not v:
            raise ValueError("At least one capability must be specified")
        return v
    
    class Config:
        use_enum_values = True
        populate_by_name = True  # 允许使用别名


class AIRequest(BaseModel):
    """AI请求模型"""
    task_type: TaskType
    content: Dict[str, Any]
    preferred_providers: Optional[List[AIProvider]] = None
    preferred_models: Optional[List[str]] = None
    max_tokens: Optional[int] = None
    temperature: float = Field(default=0.7, ge=0, le=2)
    timeout: Optional[int] = None
    
    class Config:
        use_enum_values = True


class AIResponse(BaseModel):
    """AI响应模型"""
    request_id: str
    provider: AIProvider
    model: str
    content: Any
    usage: Dict[str, int]
    duration_ms: float
    cost: float
    success: bool = True
    error: Optional[str] = None
    
    class Config:
        use_enum_values = True


# SQLAlchemy模型（数据库）
class AIModel(SQLBaseModel):
    """AI模型数据库模型"""
    __tablename__ = "ai_models"
    
    provider: Mapped[str] = mapped_column(SQLEnum(AIProvider), nullable=False)
    model_name: Mapped[str] = mapped_column(String(100), nullable=False)
    api_key_encrypted: Mapped[str] = mapped_column(String(500), nullable=False)  # 加密存储
    api_url: Mapped[str] = mapped_column(String(500), nullable=False)
    priority: Mapped[int] = mapped_column(Integer, default=1)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True)
    capabilities: Mapped[List[str]] = mapped_column(JSON, nullable=False)  # 存储为JSON数组
    cost_per_1k_tokens: Mapped[float] = mapped_column(Float, default=0.0)
    max_tokens: Mapped[int] = mapped_column(Integer, default=4096)
    timeout: Mapped[int] = mapped_column(Integer, default=30)
    max_retries: Mapped[int] = mapped_column(Integer, default=3)
    custom_headers: Mapped[Optional[Dict[str, str]]] = mapped_column(JSON, nullable=True)
    
    # 统计字段
    total_requests: Mapped[int] = mapped_column(Integer, default=0)
    successful_requests: Mapped[int] = mapped_column(Integer, default=0)
    failed_requests: Mapped[int] = mapped_column(Integer, default=0)
    total_tokens_used: Mapped[int] = mapped_column(Integer, default=0)
    total_cost: Mapped[float] = mapped_column(Float, default=0.0)
    average_response_time: Mapped[float] = mapped_column(Float, default=0.0)
    last_used_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)
    last_error: Mapped[Optional[str]] = mapped_column(String(500), nullable=True)
    last_error_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)


class AICallLog(SQLBaseModel):
    """AI调用日志"""
    __tablename__ = "ai_call_logs"
    
    model_id: Mapped[str] = mapped_column(String(36), nullable=False)
    task_type: Mapped[str] = mapped_column(SQLEnum(TaskType), nullable=False)
    request_data: Mapped[Dict[str, Any]] = mapped_column(JSON, nullable=False)
    response_data: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)
    tokens_used: Mapped[int] = mapped_column(Integer, default=0)
    cost: Mapped[float] = mapped_column(Float, default=0.0)
    duration_ms: Mapped[float] = mapped_column(Float, nullable=False)
    status: Mapped[str] = mapped_column(String(20), nullable=False)  # success, failed, timeout
    error_message: Mapped[Optional[str]] = mapped_column(String(1000), nullable=True)
    
    class Config:
        use_enum_values = True 