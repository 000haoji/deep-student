"""
错题管理服务
提供错题的CRUD、AI分析、复习管理等功能
"""
from .models import Problem, ReviewRecord, Subject, ProblemTag, ProblemCategory
from .schemas import (
    ProblemCreate,
    ProblemUpdate,
    ProblemData,             # Changed from ProblemResponse
    SingleProblemResponse,   # Added
    ProblemListResponse,
    ReviewRecordCreate,
    # ReviewRecordResponse,  # This was also removed/refactored in schemas.py, need to check its usage.
                           # Assuming ReviewRecordResponse is still valid or was not the cause of the immediate error.
                           # Let's re-check schemas.py for ReviewRecordResponse.
                           # ReviewRecordResponse is still in schemas.py.
    ReviewRecordResponse,
    ProblemQuery
)
from .service import ProblemService
from .api import router as problem_router

__all__ = [
    # 模型
    "Problem",
    "ReviewRecord",
    "Subject",
    "ProblemTag",
    "ProblemCategory",
    
    # Schema
    "ProblemCreate",
    "ProblemUpdate",
    "ProblemData",             # Changed from ProblemResponse
    "SingleProblemResponse",   # Added
    "ProblemListResponse",
    "ReviewRecordCreate",
    "ReviewRecordResponse",
    "ProblemQuery",
    
    # 服务
    "ProblemService",
    
    # 路由
    "problem_router"
]
