"""
回顾分析服务数据模型
"""
from enum import Enum
from typing import List, Optional, Dict, Any
from datetime import datetime
from sqlalchemy import Column, String, Integer, Float, JSON, Text, ForeignKey, Enum as SQLEnum, DateTime, UniqueConstraint, Boolean # Added Boolean
from sqlalchemy.dialects.postgresql import UUID, ARRAY
from sqlalchemy.orm import relationship, Mapped, mapped_column
import uuid
from sqlalchemy.sql import func
from sqlalchemy.ext.declarative import declarative_base

from shared.models.base import BaseModel

Base = declarative_base()

class AnalysisType(str, Enum):
    """分析类型"""
    SINGLE_PROBLEM = "single_problem"  # 单题分析
    BATCH_PROBLEMS = "batch_problems"  # 批量分析
    KNOWLEDGE_POINT = "knowledge_point"  # 知识点分析
    TIME_PERIOD = "time_period"  # 时间段分析
    COMPREHENSIVE = "comprehensive"  # 综合分析


class ReviewAnalysis(BaseModel):
    """回顾分析记录"""
    __tablename__ = "review_analyses"
    
    # 基本信息
    title: Mapped[str] = mapped_column(String(200), nullable=False)
    analysis_type: Mapped[str] = mapped_column(SQLEnum(AnalysisType), nullable=False)
    description: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    
    # 分析范围
    problem_ids: Mapped[List[str]] = mapped_column(JSON, default=list)  # 分析的题目ID列表
    subjects: Mapped[List[str]] = mapped_column(JSON, default=list)  # 涉及的学科
    knowledge_points: Mapped[List[str]] = mapped_column(JSON, default=list)  # 涉及的知识点
    date_range: Mapped[Optional[Dict[str, str]]] = mapped_column(JSON, nullable=True)  # 时间范围
    
    # 分析结果
    total_problems: Mapped[int] = mapped_column(Integer, default=0)
    error_patterns: Mapped[Optional[List[Dict[str, Any]]]] = mapped_column(JSON, nullable=True)  # 错误模式
    weakness_areas: Mapped[Optional[List[Dict[str, Any]]]] = mapped_column(JSON, nullable=True)  # 薄弱环节
    improvement_suggestions: Mapped[Optional[List[str]]] = mapped_column(JSON, nullable=True)  # 改进建议
    study_plan: Mapped[Optional[List[Dict[str, Any]]]] = mapped_column(JSON, nullable=True)  # 学习计划
    
    # 统计数据
    avg_difficulty: Mapped[float] = mapped_column(Float, default=0.0)
    avg_mastery: Mapped[float] = mapped_column(Float, default=0.0)
    common_mistakes: Mapped[Optional[Dict[str, int]]] = mapped_column(JSON, nullable=True)  # 常见错误统计
    
    # AI分析
    ai_analysis: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)  # AI分析原始数据
    
    # 评分
    importance_score: Mapped[float] = mapped_column(Float, default=5.0)  # 重要性评分(0-10)
    urgency_score: Mapped[float] = mapped_column(Float, default=5.0)  # 紧急程度评分(0-10)
    
    # 关联
    follow_ups: Mapped[List["AnalysisFollowUp"]] = relationship(
        "AnalysisFollowUp",
        back_populates="analysis",
        cascade="all, delete-orphan"
    )


class AnalysisFollowUp(BaseModel):
    """分析后续跟进记录"""
    __tablename__ = "analysis_follow_ups"
    
    # 关联
    analysis_id: Mapped[str] = mapped_column(String(36), ForeignKey("review_analyses.id", ondelete="CASCADE"), nullable=False)
    analysis: Mapped["ReviewAnalysis"] = relationship("ReviewAnalysis", back_populates="follow_ups")
    
    # 跟进信息
    action_taken: Mapped[str] = mapped_column(Text, nullable=False)  # 采取的行动
    result: Mapped[Optional[str]] = mapped_column(Text, nullable=True)  # 结果
    effectiveness_score: Mapped[float] = mapped_column(Float, default=5.0)  # 效果评分(0-10)
    
    # 相关题目
    reviewed_problem_ids: Mapped[Optional[List[str]]] = mapped_column(JSON, nullable=True)  # 复习的题目ID
    new_mastery_levels: Mapped[Optional[Dict[str, float]]] = mapped_column(JSON, nullable=True)  # 新的掌握程度
    
    # 备注
    notes: Mapped[Optional[str]] = mapped_column(Text, nullable=True)


class LearningPattern(BaseModel):
    """学习模式记录"""
    __tablename__ = "learning_patterns"
    
    # 基本信息
    pattern_name: Mapped[str] = mapped_column(String(100), nullable=False)
    pattern_type: Mapped[str] = mapped_column(String(50), nullable=False)  # error_pattern, strength, weakness
    description: Mapped[str] = mapped_column(Text, nullable=False)
    
    # 模式数据
    pattern_data: Mapped[Dict[str, Any]] = mapped_column(JSON, nullable=False)  # 模式的详细数据
    confidence_score: Mapped[float] = mapped_column(Float, default=0.0)  # 置信度(0-1)
    
    # 适用范围
    applicable_subjects: Mapped[Optional[List[str]]] = mapped_column(JSON, nullable=True)  # 适用学科
    applicable_knowledge_points: Mapped[Optional[List[str]]] = mapped_column(JSON, nullable=True)  # 适用知识点
    
    # 建议
    recommendations: Mapped[Optional[List[str]]] = mapped_column(JSON, nullable=True)  # 相关建议
    
    # 统计
    occurrence_count: Mapped[int] = mapped_column(Integer, default=1)  # 出现次数
    last_seen_at: Mapped[datetime] = mapped_column(DateTime(timezone=True), server_default=func.now())
    
    __table_args__ = (
        UniqueConstraint('pattern_type', 'pattern_name', name='_type_name_uc'),
    )


class ReviewPlan(Base):
    """复习计划"""
    __tablename__ = "review_plans"
    
    id = Column(String(36), primary_key=True)
    title = Column(String(100), nullable=False)
    subject = Column(String(50), nullable=False)
    category = Column(String(50))
    tags = Column(JSON, default=list)
    start_date = Column(DateTime, nullable=False)
    end_date = Column(DateTime, nullable=False)
    total_problems = Column(Integer, default=0)
    completed_problems = Column(Integer, default=0)
    status = Column(String(20), default="active")  # active, completed, paused
    created_at = Column(DateTime, default=datetime.now)
    updated_at = Column(DateTime, default=datetime.now, onupdate=datetime.now)
    last_adjusted = Column(DateTime)
    adjustment_history = Column(JSON, default=list)
    
    # 关联
    stages = relationship("ReviewStage", back_populates="plan", cascade="all, delete-orphan")
    progress_records = relationship("ReviewProgress", back_populates="plan", cascade="all, delete-orphan")


class ReviewStage(Base):
    """复习阶段"""
    __tablename__ = "review_stages"
    
    id = Column(String(36), primary_key=True)
    plan_id = Column(String(36), ForeignKey("review_plans.id"), nullable=False)
    scheduled_time = Column(DateTime, nullable=False)
    interval_hours = Column(Integer, nullable=False)
    status = Column(String(20), default="pending")  # pending, in_progress, completed, skipped
    completed_at = Column(DateTime)
    problems_completed = Column(Integer, default=0)
    total_problems = Column(Integer, default=0)
    mastery_level = Column(Float, default=0.0)
    adjustment_history = Column(JSON, default=list)
    
    # 关联
    plan = relationship("ReviewPlan", back_populates="stages")
    progress = relationship("ReviewProgress", back_populates="stage", cascade="all, delete-orphan")


class ReviewProgress(Base):
    """复习进度"""
    __tablename__ = "review_progress"
    
    id = Column(String(36), primary_key=True)
    plan_id = Column(String(36), ForeignKey("review_plans.id"), nullable=False)
    stage_id = Column(String(36), ForeignKey("review_stages.id"), nullable=False)
    problem_id = Column(String(36), ForeignKey("problems.id"), nullable=False)
    
    # 进度信息
    status = Column(String(20), default="pending")  # pending, in_progress, completed, skipped
    started_at = Column(DateTime)
    completed_at = Column(DateTime)
    time_spent = Column(Integer, default=0)  # 秒
    attempts = Column(Integer, default=0)
    is_correct = Column(Boolean)
    mastery_level = Column(Float, default=0.0)
    notes = Column(String(500))
    
    # 详细记录
    answer_history = Column(JSON, default=list)  # 答题历史
    error_analysis = Column(JSON)  # 错误分析
    learning_patterns = Column(JSON)  # 学习模式
    
    created_at = Column(DateTime, default=datetime.now)
    updated_at = Column(DateTime, default=datetime.now, onupdate=datetime.now)
    
    # 关联
    plan = relationship("ReviewPlan", back_populates="progress_records")
    stage = relationship("ReviewStage", back_populates="progress")
    problem = relationship("Problem")


class ReviewStatistics(Base):
    """复习统计"""
    __tablename__ = "review_statistics"
    
    id = Column(String(36), primary_key=True)
    plan_id = Column(String(36), ForeignKey("review_plans.id"), nullable=False)
    
    # 统计信息
    total_time_spent = Column(Integer, default=0)  # 总复习时间（秒）
    total_problems = Column(Integer, default=0)
    completed_problems = Column(Integer, default=0)
    correct_problems = Column(Integer, default=0)
    average_mastery = Column(Float, default=0.0)
    
    # 详细统计
    subject_stats = Column(JSON, default=dict)  # 各科目统计
    category_stats = Column(JSON, default=dict)  # 各分类统计
    time_distribution = Column(JSON, default=dict)  # 时间分布
    error_patterns = Column(JSON, default=dict)  # 错误模式
    
    # 时间范围
    start_date = Column(DateTime, nullable=False)
    end_date = Column(DateTime, nullable=False)
    last_updated = Column(DateTime, default=datetime.now, onupdate=datetime.now)
    
    # 关联
    plan = relationship("ReviewPlan")
