"""
数据库基础模块
"""
from sqlalchemy.ext.asyncio import create_async_engine, AsyncSession
from sqlalchemy.orm import sessionmaker
from sqlalchemy.pool import NullPool

from .config import settings
from .models.base import Base  # 从新的位置导入Base

# 创建异步引擎
engine = create_async_engine(
    settings.DATABASE_URL,
    echo=settings.DATABASE_ECHO,
    poolclass=NullPool
)

# 创建异步会话工厂
async_session_maker = sessionmaker(
    engine,
    class_=AsyncSession,
    expire_on_commit=False
)

async def init_db():
    """初始化数据库，创建所有表"""
    async with engine.begin() as conn:
        # await conn.run_sync(Base.metadata.drop_all) # Optional: for dropping tables during dev
        await conn.run_sync(Base.metadata.create_all)
    # init_db doesn't need to return the maker if it's globally available in this module
    # and main.py imports it directly.
    # However, if main.py expects it as a return value:
    # return async_session_maker


async def get_db() -> AsyncSession:
    """获取数据库会话 (FastAPI dependency)"""
    async with async_session_maker() as session: # Use the renamed maker
        try:
            yield session
        finally:
            await session.close()
