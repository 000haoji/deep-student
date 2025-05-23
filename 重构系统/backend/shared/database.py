"""
数据库配置和会话管理
"""
from typing import AsyncGenerator
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.ext.declarative import declarative_base
from sqlalchemy.orm import sessionmaker

from .config import settings
from .utils.logger import get_logger

logger = get_logger(__name__)

# 创建异步引擎
if settings.database_url.startswith("sqlite"):
    engine = create_async_engine(
        settings.database_url,
        echo=settings.debug,
        pool_pre_ping=True
    )
else:
    engine = create_async_engine(
        settings.database_url,
        echo=settings.debug,
        pool_pre_ping=True,
        pool_size=5,
        max_overflow=10
    )

# 创建异步会话工厂
AsyncSessionLocal = sessionmaker(
    engine,
    class_=AsyncSession,
    expire_on_commit=False
)

# 声明基类
Base = declarative_base()


async def get_db() -> AsyncGenerator[AsyncSession, None]:
    """获取数据库会话"""
    async with AsyncSessionLocal() as session:
        try:
            yield session
        finally:
            await session.close()


async def init_db():
    """初始化数据库连接"""
    try:
        # 测试连接
        async with engine.begin() as conn:
            await conn.run_sync(lambda conn: conn.execute("SELECT 1"))
        logger.info("Database connection initialized successfully")
    except Exception as e:
        logger.error(f"Failed to initialize database: {e}")
        raise


async def create_tables():
    """创建所有数据库表"""
    try:
        async with engine.begin() as conn:
            # 导入所有模型以确保它们被注册
            from services.ai_api_manager.models import AIModel, AICallLog
            from services.problem_service.models import Problem, ReviewRecord, ProblemTemplate
            from services.review_service.models import ReviewAnalysis, AnalysisFollowUp, LearningPattern
            from services.file_service.models import FileRecord
            
            # 创建所有表
            await conn.run_sync(Base.metadata.create_all)
            
        logger.info("All database tables created successfully")
    except Exception as e:
        logger.error(f"Failed to create tables: {e}")
        raise


async def drop_tables():
    """删除所有数据库表（仅用于测试）"""
    try:
        async with engine.begin() as conn:
            await conn.run_sync(Base.metadata.drop_all)
        logger.info("All database tables dropped")
    except Exception as e:
        logger.error(f"Failed to drop tables: {e}")
        raise


async def close_db():
    """关闭数据库连接"""
    await engine.dispose()
    logger.info("Database connection closed") 