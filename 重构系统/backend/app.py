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

# Import for startup event (old AIRouter, if still used for initialization tasks like health checks)
from services.ai_api_manager.router import ai_router as global_ai_event_router 
# Service instances are usually instantiated within API route handlers.
# Service-specific schemas are handled by service API modules.

from services.statistics_service.router import router as statistics_router
from services.problem_service.api import router as problem_router # Main problem router
from services.problem_service.api import tags_router as problem_tags_router
from services.problem_service.api import categories_router as problem_categories_router
from services.problem_service.api import ai_problem_router # Router for AI-driven problem creation
from services.review_service.api import router as review_router
from services.file_service.api import router as file_router
from services.ai_api_manager.api import router as ai_manager_api_router # API for AI model management (CRUD, execute)


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

# 包含各个服务的路由
app.include_router(problem_router)
app.include_router(problem_tags_router)
app.include_router(problem_categories_router)
app.include_router(ai_problem_router) # Include the AI-driven problem creation router
app.include_router(review_router)
app.include_router(file_router)
app.include_router(ai_manager_api_router) 
app.include_router(statistics_router)

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
    # from shared.database import SessionLocal # Example for direct session creation
    # async with SessionLocal() as db_session_for_startup:
        # Perform startup tasks requiring a db session here
    
    # Initialize the old global_ai_event_router if its background tasks (like health checks) are still desired.
    # Its request routing capabilities are superseded by AIService.
    if global_ai_event_router: # Check if it's still relevant/imported
        from shared.database import async_session_maker
        logger.info("Setting session maker for global AI event router...")
        global_ai_event_router.set_session_maker(async_session_maker)
        logger.info("Initializing global AI event router...")
        await global_ai_event_router.initialize()
        logger.info("Global AI event router initialized (for background tasks/health checks).")
    else:
        logger.info("Global AI event router not found or not configured for startup initialization.")

    logger.info("Application started successfully!")


@app.on_event("shutdown")
async def shutdown_event():
    """应用关闭事件"""
    logger.info("Shutting down the application...")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
