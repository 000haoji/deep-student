"""
用户管理服务的数据模型
"""
from enum import Enum
from typing import Optional, List
from datetime import datetime
from sqlalchemy import String, Boolean, Enum as SQLEnum, Integer, DateTime
from sqlalchemy.orm import Mapped, mapped_column, relationship
from passlib.context import CryptContext

from shared.models.base import BaseModel

# 创建密码加密上下文
pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")


class UserRole(str, Enum):
    """用户角色枚举"""
    STUDENT = "student"
    TEACHER = "teacher"
    ADMIN = "admin"


class User(BaseModel):
    """用户模型"""
    __tablename__ = "users"
    
    # 基本信息
    username: Mapped[str] = mapped_column(String(50), unique=True, nullable=False)
    email: Mapped[str] = mapped_column(String(100), unique=True, nullable=False)
    full_name: Mapped[str] = mapped_column(String(100), nullable=False)
    password_hash: Mapped[str] = mapped_column(String(255), nullable=False)
    
    # 角色和状态
    role: Mapped[str] = mapped_column(SQLEnum(UserRole), default=UserRole.STUDENT, nullable=False)
    is_active: Mapped[bool] = mapped_column(Boolean, default=True, nullable=False)
    is_verified: Mapped[bool] = mapped_column(Boolean, default=False, nullable=False)
    
    # 个人资料
    avatar_url: Mapped[Optional[str]] = mapped_column(String(500), nullable=True)
    bio: Mapped[Optional[str]] = mapped_column(String(500), nullable=True)
    phone: Mapped[Optional[str]] = mapped_column(String(20), nullable=True)
    
    # 学习信息
    grade: Mapped[Optional[str]] = mapped_column(String(20), nullable=True)  # 年级
    school: Mapped[Optional[str]] = mapped_column(String(100), nullable=True)  # 学校
    major: Mapped[Optional[str]] = mapped_column(String(50), nullable=True)  # 专业
    
    # 登录信息
    last_login_at: Mapped[Optional[datetime]] = mapped_column(DateTime(timezone=True), nullable=True)
    login_count: Mapped[int] = mapped_column(Integer, default=0)
    
    # 关联
    problems: Mapped[List["Problem"]] = relationship(
        "Problem",
        back_populates="user",
        lazy="dynamic"
    )
    
    analyses: Mapped[List["ReviewAnalysis"]] = relationship(
        "ReviewAnalysis",
        back_populates="user",
        lazy="dynamic"
    )
    
    def set_password(self, password: str) -> None:
        """设置密码"""
        self.password_hash = pwd_context.hash(password)
    
    def check_password(self, password: str) -> bool:
        """验证密码"""
        return pwd_context.verify(password, self.password_hash)
    
    def update_login_info(self) -> None:
        """更新登录信息"""
        self.last_login_at = datetime.utcnow()
        self.login_count += 1
    
    @property
    def is_teacher_or_admin(self) -> bool:
        """是否是老师或管理员"""
        return self.role in [UserRole.TEACHER, UserRole.ADMIN]
    
    def __repr__(self) -> str:
        return f"<User {self.username}>" 