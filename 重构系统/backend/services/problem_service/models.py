"""
错题管理服务的数据模型
"""
from enum import Enum
from typing import List, Optional, Dict, Any
from datetime import datetime
from sqlalchemy import String, Integer, Float, JSON, Enum as SQLEnum, ForeignKey, Text, DateTime, Boolean, UniqueConstraint
from sqlalchemy.dialects.postgresql import UUID, ARRAY
from sqlalchemy.orm import relationship, Mapped, mapped_column
import uuid

from shared.models.base import BaseModel


class Subject(str, Enum):
    """学科枚举"""
    MATH = "math"
    ENGLISH = "english"
    POLITICS = "politics"
    PROFESSIONAL = "professional"
    
    @classmethod
    def get_display_name(cls, value: str) -> str:
        """获取显示名称"""
        display_map = {
            "math": "数学",
            "english": "英语", 
            "politics": "政治",
            "professional": "专业课"
        }
        return display_map.get(value, value)


class DifficultyLevel(int, Enum):
    """难度等级"""
    VERY_EASY = 1
    EASY = 2
    MEDIUM = 3
    HARD = 4
    VERY_HARD = 5


class Problem(BaseModel):
    """错题模型"""
    __tablename__ = "problems"
    
    # 基本信息
    title: Mapped[str] = mapped_column(String(200), nullable=False)
    content: Mapped[str] = mapped_column(Text, nullable=False)  # 题目内容
    subject: Mapped[str] = mapped_column(SQLEnum(Subject), nullable=False, index=True)  # 学科
    category: Mapped[Optional[str]] = mapped_column(String(100), index=True, nullable=True)  # 题目分类
    source: Mapped[Optional[str]] = mapped_column(String(200), nullable=True)  # 题目来源
    year: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)  # 年份
    
    # 错误相关
    error_analysis: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 错误分析
    user_answer: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 用户答案
    correct_answer: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 正确答案
    solution: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 解题思路
    
    # AI分析结果
    ai_analysis: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)  # AI分析的完整结果
    knowledge_points: Mapped[List[str]] = mapped_column(JSON, default=list)  # 知识点列表
    difficulty_level: Mapped[int] = mapped_column(Integer, default=3)  # 难度等级
    
    # 图片相关
    image_urls: Mapped[List[str]] = mapped_column(JSON, default=list)  # 题目图片URL列表
    ocr_result: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # OCR识别结果
    
    # 统计信息
    review_count: Mapped[int] = mapped_column(Integer, default=0)  # 复习次数
    last_review_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)  # 最后复习时间
    mastery_level: Mapped[float] = mapped_column(Float, default=0.0)  # 掌握程度(0-1)
    
    # 标签
    tags: Mapped[List[str]] = mapped_column(JSON, default=list)  # 标签列表
    
    # 备注
    notes: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 用户备注
    
    # 关联
    review_records: Mapped[List["ReviewRecord"]] = relationship(
        "ReviewRecord",
        back_populates="problem",
        cascade="all, delete-orphan"
    )


class ReviewRecord(BaseModel):
    """复习记录"""
    __tablename__ = "review_records"
    
    # 关联
    problem_id: Mapped[str] = mapped_column(String(36), ForeignKey("problems.id", ondelete="CASCADE"), nullable=False)
    problem: Mapped["Problem"] = relationship("Problem", back_populates="review_records")
    
    # 复习信息
    review_result: Mapped[Optional[str]] = mapped_column(String(20), nullable=True)  # correct, incorrect, partial
    confidence_level: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)  # 1-5 信心等级
    time_spent: Mapped[Optional[int]] = mapped_column(Integer, nullable=True)  # 花费时间(秒)
    notes: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 复习笔记
    
    # AI反馈
    ai_feedback: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)  # AI给出的反馈


class ProblemTemplate(BaseModel):
    """题目模板（用于批量导入）"""
    __tablename__ = "problem_templates"
    
    name: Mapped[str] = mapped_column(String(100), nullable=False)
    subject: Mapped[str] = mapped_column(SQLEnum(Subject), nullable=False)
    category: Mapped[Optional[str]] = mapped_column(String(100), nullable=True)
    template_content: Mapped[Dict[str, Any]] = mapped_column(JSON, nullable=False)
    example_content: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)  # 示例内容
    description: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    usage_count: Mapped[int] = mapped_column(Integer, default=0)
    tags: Mapped[List[str]] = mapped_column(JSON, default=list)
    
    __table_args__ = (
        UniqueConstraint('subject', 'name', name='_subject_name_uc'),
    ) 