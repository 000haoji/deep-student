"""
错题管理服务的数据模型
"""
from enum import Enum
from typing import List, Optional, Dict, Any
from datetime import datetime
from sqlalchemy import String, Integer, Float, JSON, Enum as SQLEnum, ForeignKey, Text, DateTime, Boolean, UniqueConstraint, Column, sql
from sqlalchemy.dialects.postgresql import UUID, ARRAY # Keep for now, might be used by other parts not shown
from sqlalchemy.orm import relationship, Mapped, mapped_column
import uuid

# Import for explicit id, created_at, updated_at columns
# uuid is already imported above if needed by default value functions.
# Mapped, mapped_column are already imported.
# String is already imported.
# Column, DateTime, sql from sqlalchemy are needed for explicit timestamps.
from shared.models.base import Base, SoftDeleteMixin


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


class Problem(Base, SoftDeleteMixin):
    """错题模型"""
    __tablename__ = "problems"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    # deleted_at is inherited from SoftDeleteMixin
    
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
    ai_chat_logs: Mapped[List["ProblemAIChatLog"]] = relationship(
        "ProblemAIChatLog",
        back_populates="problem",
        cascade="all, delete-orphan",
        order_by="ProblemAIChatLog.order_in_conversation"  # Ensure logs are ordered
    )


class ReviewRecord(Base):
    """复习记录"""
    __tablename__ = "review_records"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    
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

class ProblemTemplate(Base):
    """题目模板（用于批量导入）"""
    __tablename__ = "problem_templates"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    
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


class ProblemTag(Base):
    """题目标签"""
    __tablename__ = "problem_tags"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    
    name: Mapped[str] = mapped_column(String(50), nullable=False) # Name alone might not be unique across users
    description: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    color: Mapped[Optional[str]] = mapped_column(String(20), nullable=True)  # 标签颜色
    usage_count: Mapped[int] = mapped_column(Integer, default=0)  # 使用次数

    __table_args__ = (
        UniqueConstraint('name', name='_tag_name_uc'),
    )
    
    def __repr__(self):
        return f"<ProblemTag {self.name}>"


class ProblemCategory(Base):
    """题目分类"""
    __tablename__ = "problem_categories"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    
    name: Mapped[str] = mapped_column(String(100), nullable=False)
    subject: Mapped[str] = mapped_column(SQLEnum(Subject), nullable=False)
    parent_id: Mapped[Optional[str]] = mapped_column(String(36), ForeignKey("problem_categories.id"), nullable=True)
    description: Mapped[Optional[str]] = mapped_column(Text, nullable=True)
    order: Mapped[int] = mapped_column(Integer, default=0)  # 排序
    usage_count: Mapped[int] = mapped_column(Integer, default=0)  # 使用次数
    
    # 关联
    parent: Mapped[Optional["ProblemCategory"]] = relationship(
        "ProblemCategory",
        remote_side="ProblemCategory.id",
        backref="children"
    )
    
    __table_args__ = (
        UniqueConstraint('subject', 'name', name='_subject_category_name_uc'),
    )
    
    def __repr__(self):
        return f"<ProblemCategory {self.subject}:{self.name}>"


class ProblemAIChatLog(Base):
    """AI交互聊天记录模型 (用于AI驱动的错题创建流程)"""
    __tablename__ = "problem_ai_chat_logs"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())

    # Link to the problem, nullable because logs can be created during a session before problem is finalized.
    problem_id: Mapped[Optional[str]] = mapped_column(String(36), ForeignKey("problems.id", ondelete="CASCADE"), nullable=True, index=True)
    problem: Mapped[Optional["Problem"]] = relationship("Problem", back_populates="ai_chat_logs") # Optional problem

    # Session ID for AI creation workflow, to group logs before problem_id is known.
    # This links to ProblemAISession.id
    problem_creation_session_id: Mapped[Optional[str]] = mapped_column(String(36), ForeignKey("problem_ai_sessions.id", ondelete="SET NULL"), index=True, nullable=True)
    ai_session: Mapped[Optional["ProblemAISession"]] = relationship("ProblemAISession", back_populates="ai_chat_logs")

    timestamp: Mapped[datetime] = mapped_column(DateTime(timezone=True), default=datetime.utcnow, nullable=False)
    role: Mapped[str] = mapped_column(String(10), nullable=False)  # 'user' or 'ai'
    content: Mapped[str] = mapped_column(Text, nullable=False)
    content_type: Mapped[str] = mapped_column(String(30), default='text', nullable=False) # e.g., 'text', 'json_suggestion', 'user_input', 'ai_structured_output'
    order_in_conversation: Mapped[int] = mapped_column(Integer, nullable=False) # To maintain sequence

    def __repr__(self):
        return f"<ProblemAIChatLog problem_id={self.problem_id} role={self.role} order={self.order_in_conversation}>"


class ProblemAISessionStatus(str, Enum):
    """AI错题创建会话状态"""
    ACTIVE = "active"          # 会话正在进行中
    FINALIZED = "finalized"    # 会话已完成，题目已创建
    ABORTED = "aborted"        # 会话被用户或系统中止


class ProblemAISession(Base):
    """AI驱动错题创建会话模型"""
    __tablename__ = "problem_ai_sessions"
    id: Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicit id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())

    # User ID, optional for now as user system is not fully integrated in this context
    # user_id: Mapped[Optional[str]] = mapped_column(String(36), ForeignKey("users.id"), nullable=True, index=True) # Assuming a users table

    status: Mapped[str] = mapped_column(SQLEnum(ProblemAISessionStatus), default=ProblemAISessionStatus.ACTIVE, nullable=False)
    
    # Store the evolving structured data from AI interaction
    current_structured_data: Mapped[Optional[Dict[str, Any]]] = mapped_column(JSON, nullable=True)
    
    # Link to the problem that is eventually created from this session
    final_problem_id: Mapped[Optional[str]] = mapped_column(String(36), ForeignKey("problems.id"), nullable=True, index=True)
    final_problem: Mapped[Optional["Problem"]] = relationship("Problem") # One-to-one or one-to-many if session could spawn multiple (unlikely for this flow)

    initial_image_ref: Mapped[Optional[str]] = mapped_column(String(1024), nullable=True) # URL or reference to the initial image
    initial_subject_hint: Mapped[Optional[str]] = mapped_column(SQLEnum(Subject), nullable=True)


    # Relationship to chat logs for this specific session
    # This assumes ProblemAIChatLog.problem_creation_session_id will be used to link.
    # The foreign key setup would be on ProblemAIChatLog referencing ProblemAISession.id.
    # So, ProblemAISession.id needs to be what ProblemAIChatLog.problem_creation_session_id refers to.
    # Let's adjust ProblemAIChatLog.problem_creation_session_id to be a ForeignKey.
    # And ProblemAISession.id can be a UUID or a string. Given service.py uses f"aipc_sess_{uuid.uuid4()}", string is fine.

    ai_chat_logs: Mapped[List["ProblemAIChatLog"]] = relationship(
        "ProblemAIChatLog",
        # primaryjoin="ProblemAISession.id == ProblemAIChatLog.problem_creation_session_id", # Explicit join if needed
        foreign_keys="[ProblemAIChatLog.problem_creation_session_id]", # Specify FK if SQLAlchemy can't infer
        back_populates="ai_session",
        cascade="all, delete-orphan",
        order_by="ProblemAIChatLog.order_in_conversation"
    )

    def __repr__(self):
        return f"<ProblemAISession id={self.id} status={self.status} problem_id={self.final_problem_id}>"

# Need to adjust ProblemAIChatLog to correctly link to ProblemAISession
# The `problem_creation_session_id` in `ProblemAIChatLog` should now be a ForeignKey
# to `ProblemAISession.id`.
# However, `BaseModel` defines `id` as `Mapped[str] = mapped_column(String(36), primary_key=True, default=lambda: str(uuid.uuid4()))`
# So, `ProblemAISession.id` will be a UUID string.

# Let's re-check ProblemAIChatLog `problem_creation_session_id`
# It's `Mapped[Optional[str]] = mapped_column(String(255), index=True, nullable=True)`
# This needs to become a ForeignKey to `problem_ai_sessions.id`.
# And ProblemAIChatLog needs an `ai_session` relationship.

# Let's modify ProblemAIChatLog definition after ProblemAISession is defined.
# It's better to define ProblemAISession first, then adjust ProblemAIChatLog.
# The current diff is adding ProblemAISession. I will do another replace_in_file for ProblemAIChatLog.
