"""
测试配置文件
"""
import pytest
import asyncio
from typing import AsyncGenerator
from sqlalchemy.ext.asyncio import AsyncSession, create_async_engine
from sqlalchemy.orm import sessionmaker
from sqlalchemy.pool import NullPool

from shared.database import Base
from shared.config import settings

# 测试数据库URL
TEST_DATABASE_URL = settings.DATABASE_URL + "_test"

# 创建测试引擎
engine = create_async_engine(
    TEST_DATABASE_URL,
    echo=False,
    poolclass=NullPool
)

# 创建测试会话工厂
async_session = sessionmaker(
    engine,
    class_=AsyncSession,
    expire_on_commit=False
)

@pytest.fixture(scope="session")
def event_loop():
    """创建事件循环"""
    loop = asyncio.get_event_loop_policy().new_event_loop()
    yield loop
    loop.close()

@pytest.fixture(scope="session")
async def init_db():
    """初始化测试数据库"""
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)
        await conn.run_sync(Base.metadata.create_all)
    
    yield
    
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.drop_all)

@pytest.fixture
async def db(init_db) -> AsyncGenerator[AsyncSession, None]:
    """获取数据库会话"""
    async with async_session() as session:
        yield session
        await session.rollback()
        await session.close()

@pytest.fixture
async def test_user(db: AsyncSession):
    """创建测试用户"""
    from services.user_service.models import User
    from services.user_service.schemas import UserCreate
    
    user_data = UserCreate(
        username="testuser",
        email="test@example.com",
        password="testpass123"
    )
    
    user = User(
        username=user_data.username,
        email=user_data.email,
        hashed_password=user_data.get_hashed_password()
    )
    
    db.add(user)
    await db.commit()
    await db.refresh(user)
    
    return user 