"""
错题管理系统 FastAPI 主应用
"""
from fastapi import FastAPI, HTTPException, Depends, status
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from typing import List, Optional, Dict, Any 
from datetime import datetime 
# import uuid # uuid might not be needed if not used directly here

from shared.database import get_db, engine, Base # get_db might not be needed if no app-level endpoints use it
from shared.config import settings
from shared.utils.logger import get_logger

# Service-specific models and schemas are typically handled within their respective API modules.
# Only import what's truly needed at the app level.
# from services.problem_service.models import Problem, Subject # Example, likely not needed here
# from services.ai_api_manager.models import AIModel, AIProvider # Example, likely not needed here
# from services.review_service.models import ReviewAnalysis, AnalysisType # Example, likely not needed here

from services.ai_api_manager.router import ai_router # 导入ai_router instance for startup
# Service instances (like ProblemService, ReviewService) are usually instantiated within API route handlers, not globally in app.py.
# Service-specific schemas (like ServiceProblemCreate) are also handled by service API modules.

from services.statistics_service.router import router as statistics_router
from services.problem_service.api import tags_router as problem_tags_router # Added import for tags_router
from services.problem_service.api import categories_router as problem_categories_router # Added import for categories_router

logger = get_logger(__name__)

# 创建FastAPI应用
app = FastAPI(
    title="错题管理系统 API",
    description="考研错题管理与分析系统",
    version="2.0.0"
)

# CORS配置
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # 生产环境应该限制具体域名
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 导入并包含各个服务的路由
from services.problem_service.api import router as problem_router
from services.review_service.api import router as review_router
from services.file_service.api import router as file_router
from services.ai_api_manager.api import router as ai_manager_api_router # Renamed to avoid conflict with ai_router instance

app.include_router(problem_router)
app.include_router(review_router)
app.include_router(file_router)
app.include_router(ai_manager_api_router) # Use the imported router for AI model management
app.include_router(statistics_router)
app.include_router(problem_tags_router) # Included the new tags router
app.include_router(problem_categories_router) # Included the new categories router

# API端点

@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API v2.0",
        "docs": "/docs",
        "status": "running"
    }

# 启动事件
@app.on_event("startup")
async def startup_event():
    """应用启动事件"""
    logger.info("Starting up the application...")
    
    # 创建数据库表（如果不存在）
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)

    # 初始化AI路由器
    # 注意：这里需要一种方式在应用启动时获得一个AsyncSession实例。
    # get_db() 通常用作 FastAPI 依赖项，在请求上下文中创建会话。
    # 对于启动任务，我们可能需要直接从 SessionLocal 创建一个会话。
    # 暂时先假设我们可以通过某种方式获取到db session，后续可能需要调整
    # 例如，可以像这样创建一个临时的session：
    # from shared.database import SessionLocal
    # async with SessionLocal() as db:
    #     await ai_router.set_db_session(db) # 确保ai_router有set_db_session方法
    #     await ai_router.initialize(db)
    # 为了简化，这里先尝试直接使用get_db()的上下文，但这可能不是最佳实践
    # 正确的做法是像 manage.py 或 cli.py 那样直接创建和管理会话。
    # 现在 AIRouter 使用 session_maker
    from shared.database import async_session_maker # Corrected import name
    logger.info("Setting session maker for AI router...")
    ai_router.set_session_maker(async_session_maker) # 传递 session_maker
    logger.info("Initializing AI router...")
    await ai_router.initialize() # initialize 现在不需要 session 参数
    logger.info("AI router initialized.")
    
    logger.info("Application started successfully!")


@app.on_event("shutdown")
async def shutdown_event():
    """应用关闭事件"""
    logger.info("Shutting down the application...")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
