"""
错题管理服务
提供错题的CRUD、AI分析、复习管理等功能
"""
from .models import Problem, ReviewRecord, Subject
from .schemas import (
    ProblemCreate,
    ProblemUpdate,
    ProblemResponse,
    ProblemListResponse,
    ReviewRecordCreate,
    ReviewRecordResponse
)
# 临时注释掉，避免导入依赖问题
# from .service import ProblemService
# from .api import router as problem_router

__all__ = [
    # 模型
    "Problem",
    "ReviewRecord",
    "Subject",
    
    # Schema
    "ProblemCreate",
    "ProblemUpdate",
    "ProblemResponse",
    "ProblemListResponse",
    "ReviewRecordCreate",
    "ReviewRecordResponse",
    
    # 服务
    # "ProblemService",
    
    # 路由
    # "problem_router"
] 