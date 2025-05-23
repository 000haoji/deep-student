"""
主应用入口
"""
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager

from shared.config import settings
from shared.utils.logger import get_logger
from shared.database import init_db

# 导入路由
from services.ai_api_manager import router as ai_router
from services.problem_service import problem_router
from services.review_service import review_router

logger = get_logger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    """应用生命周期管理"""
    # 启动时
    logger.info("Starting application...")
    
    # 初始化数据库
    await init_db()
    
    yield
    
    # 关闭时
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
app.include_router(ai_router)
app.include_router(problem_router)
app.include_router(review_router)


@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API",
        "version": "2.0.0",
        "docs": "/docs",
        "endpoints": {
            "ai": "/api/v1/ai",
            "problems": "/api/v1/problems", 
            "reviews": "/api/v1/reviews"
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
        host=settings.host,
        port=settings.port,
        reload=settings.debug,
        log_level="info"
    ) 