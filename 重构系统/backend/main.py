"""
主应用入口
"""
import os
import sys
from pathlib import Path

# 添加项目根目录到 Python 路径
backend_dir = Path(__file__).parent
if str(backend_dir) not in sys.path:
    sys.path.insert(0, str(backend_dir))

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager

from shared.config import settings
from shared.utils.logger import get_logger
from shared.database import init_db

# 导入路由
# from services.ai_api_manager import get_router as get_ai_router # This function might not be standard
from services.ai_api_manager.api import router as ai_models_mgmt_router # For AI model CRUD, stats etc.
from services.ai_api_manager.api import ai_router as global_ai_request_router # The AIRouter instance for making calls
from services.problem_service.api import router as problem_router
from services.review_service.api import router as review_router
from services.file_service.api import router as file_router

logger = get_logger(__name__)

# ai_router, ai_service_router = get_ai_router() # Replaced with direct imports

@asynccontextmanager
async def lifespan(app: FastAPI):
    """应用生命周期管理"""
    # 启动时
    logger.info("Starting application...")
    
    # 初始化数据库
    await init_db() 
    
    # 初始化全局 AI 请求路由器
    # Import the correct session maker from shared.database
    from shared.database import AsyncSessionLocal as global_db_session_maker

    if global_db_session_maker:
        logger.info("Setting session maker for AI Router...")
        global_ai_request_router.set_session_maker(global_db_session_maker)
        logger.info("Initializing AI Router...")
        await global_ai_request_router.initialize()
        logger.info("Global AI Request Router initialized successfully.")
    else:
        logger.error("Failed to initialize Global AI Request Router: AsyncSessionLocal not available from shared.database.")

    yield
    
    # 关闭时
    logger.info("Cleaning up AI Router...")
    await global_ai_request_router.cleanup()
    logger.info("Shutting down application...")


# 创建应用
app = FastAPI(
    title="错题管理系统 API",
    description="考研错题管理与分析系统后端API",
    version="2.0.0",
    lifespan=lifespan
)

# 配置CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=settings.cors_origins,
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 注册路由
app.include_router(ai_models_mgmt_router) # This is for /api/v1/ai/models, /api/v1/ai/stats etc.
# The global_ai_request_router (AIRouter instance) doesn't have FastAPI routes itself,
# it's used by services. The /api/v1/ai/call endpoint is part of ai_models_mgmt_router but uses global_ai_request_router.
app.include_router(problem_router)
app.include_router(review_router)
app.include_router(file_router)


@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API",
        "version": "2.0.0",
        "docs": "/docs",
        "endpoints": {
            "ai_management": "/api/v1/ai/models", # Example, specific routes are in ai_models_mgmt_router
            "problems": "/api/v1/problems",
            "reviews": "/api/v1/reviews",
            "files": "/api/v1/files"
        }
    }


@app.get("/health")
async def health_check():
    """健康检查"""
    return {
        "status": "healthy",
        "service": "main",
        "version": "2.0.0"
    }


if __name__ == "__main__":
    import uvicorn
    
    uvicorn.run(
        "main:app",
        host="0.0.0.0", # Or settings.APP_HOST if defined
        port=settings.APP_PORT, # Use APP_PORT from settings
        reload=settings.APP_DEBUG, # Use APP_DEBUG for reload
        log_level="info"
    )
