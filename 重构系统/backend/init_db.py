"""
数据库初始化脚本
初始化数据库，创建表结构，插入示例数据
"""
import asyncio
import sys
from pathlib import Path
from sqlalchemy import text
from sqlalchemy.ext.asyncio import create_async_engine

# 添加项目根目录到系统路径
sys.path.insert(0, str(Path(__file__).parent))

from shared.config import settings
from shared.database import engine, Base
from shared.utils.logger import get_logger
from infrastructure.database.migrate import run_migrations, check_database

# 只导入模型，不导入service
from services.ai_api_manager.models import AIModel, AIProvider, AICapability
from services.problem_service.models import Problem, Subject
from services.user_service.models import User

logger = get_logger(__name__)


async def create_database():
    """创建数据库（仅用于PostgreSQL）"""
    if settings.database_url.startswith("sqlite"):
        logger.info("Using SQLite, database will be created automatically")
        return
    
    # 从数据库URL中提取数据库名称
    parts = settings.database_url.split("/")
    db_name = parts[-1].split("?")[0]
    base_url = "/".join(parts[:-1])
    
    # 连接到默认数据库
    default_url = f"{base_url}/postgres"
    engine_default = create_async_engine(default_url, echo=False)
    
    try:
        async with engine_default.connect() as conn:
            # 检查数据库是否存在
            result = await conn.execute(
                text(f"SELECT 1 FROM pg_database WHERE datname = '{db_name}'")
            )
            exists = result.scalar() is not None
            
            if not exists:
                # 创建数据库
                await conn.execute(text("COMMIT"))  # 退出事务
                await conn.execute(text(f"CREATE DATABASE {db_name}"))
                logger.info(f"Created database: {db_name}")
            else:
                logger.info(f"Database already exists: {db_name}")
                
    except Exception as e:
        logger.error(f"Failed to create database: {e}")
        raise
    finally:
        await engine_default.dispose()


async def create_tables():
    """创建所有表"""
    try:
        # 创建所有表
        async with engine.begin() as conn:
            await conn.run_sync(Base.metadata.create_all)
        logger.info("Tables created successfully")
    except Exception as e:
        logger.error(f"Failed to create tables: {e}")
        raise


async def create_sample_data():
    """创建示例数据"""
    from sqlalchemy.ext.asyncio import AsyncSession
    from shared.database import get_db
    
    async for session in get_db():
        try:
            # 检查是否已有数据
            result = await session.execute(text("SELECT COUNT(*) FROM ai_models"))
            count = result.scalar()
            
            if count > 0:
                logger.info("Sample data already exists, skipping...")
                return
            
            # 创建AI模型配置
            ai_models = [
                AIModel(
                    provider=AIProvider.OPENAI,
                    model_name="gpt-4",
                    api_key_encrypted="sk-fake-key",  # 实际使用时需要加密
                    api_url="https://api.openai.com/v1",
                    priority=10,
                    is_active=True,
                    capabilities=[AICapability.TEXT.value, AICapability.VISION.value],
                    cost_per_1k_tokens=0.03,
                    max_tokens=4096
                ),
                AIModel(
                    provider=AIProvider.DEEPSEEK,
                    model_name="deepseek-chat",
                    api_key_encrypted="sk-fake-key",
                    api_url="https://api.deepseek.com/v1",
                    priority=8,
                    is_active=True,
                    capabilities=[AICapability.TEXT.value],
                    cost_per_1k_tokens=0.001,
                    max_tokens=4096
                ),
                AIModel(
                    provider=AIProvider.GEMINI,
                    model_name="gemini-pro",
                    api_key_encrypted="fake-key",
                    api_url="https://generativelanguage.googleapis.com/v1",
                    priority=9,
                    is_active=True,
                    capabilities=[AICapability.TEXT.value, AICapability.VISION.value],
                    cost_per_1k_tokens=0.01,
                    max_tokens=8192
                )
            ]
            
            for model in ai_models:
                session.add(model)
            
            # 创建示例用户
            user = User(
                username="demo_user",
                email="demo@example.com",
                full_name="演示用户",
                is_active=True
            )
            user.set_password("demo123")  # 设置密码
            session.add(user)
            
            # 创建示例错题
            problems = [
                Problem(
                    title="高等数学 - 极限计算",
                    content="计算极限 lim(x->0) sin(x)/x",
                    subject=Subject.MATH,
                    category="微积分",
                    user_answer="0",
                    correct_answer="1",
                    error_analysis="没有掌握重要极限公式",
                    knowledge_points=["极限", "洛必达法则", "重要极限"],
                    difficulty_level=3,
                    user_id=user.id
                ),
                Problem(
                    title="英语语法 - 时态",
                    content="Choose the correct tense: I ___ (study) English for 5 years.",
                    subject=Subject.ENGLISH,
                    category="语法",
                    user_answer="studied",
                    correct_answer="have been studying",
                    error_analysis="混淆了过去时和现在完成进行时",
                    knowledge_points=["现在完成进行时", "时态"],
                    difficulty_level=2,
                    user_id=user.id
                )
            ]
            
            for problem in problems:
                session.add(problem)
            
            await session.commit()
            logger.info("Sample data created successfully")
            
        except Exception as e:
            logger.error(f"Failed to create sample data: {e}")
            await session.rollback()
            raise
        break


async def init_database():
    """初始化数据库的主函数"""
    logger.info("Starting database initialization...")
    
    try:
        # 1. 创建数据库（仅PostgreSQL）
        await create_database()
        
        # 2. 创建表结构
        await create_tables()
        
        # 3. 运行迁移
        await run_migrations()
        
        # 4. 检查数据库
        await check_database()
        
        # 5. 创建示例数据
        await create_sample_data()
        
        logger.info("Database initialization completed successfully!")
        
    except Exception as e:
        logger.error(f"Database initialization failed: {e}")
        raise


if __name__ == "__main__":
    asyncio.run(init_database()) 