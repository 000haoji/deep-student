"""
简化的数据库初始化脚本
只初始化基本的数据库表结构
"""
import asyncio
import sys
from pathlib import Path

# 添加项目根目录到系统路径
sys.path.insert(0, str(Path(__file__).parent))

from shared.database import engine, Base
from shared.config import settings
from shared.utils.logger import get_logger
from sqlalchemy import text

logger = get_logger(__name__)


async def create_tables():
    """创建所有表"""
    try:
        # 直接导入models模块，避免__init__.py的导入
        import services.user_service.models
        import services.problem_service.models
        import services.ai_api_manager.models
        import services.review_service.models
        import services.file_service.models
        
        # 创建所有表
        async with engine.begin() as conn:
            await conn.run_sync(Base.metadata.create_all)
        
        logger.info("Tables created successfully")
        
        # 检查创建的表
        async with engine.connect() as conn:
            if settings.database_url.startswith("sqlite"):
                result = await conn.execute(text(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
                ))
                tables = [row[0] for row in result]
            else:
                result = await conn.execute(text(
                    "SELECT tablename FROM pg_tables WHERE schemaname = 'public'"
                ))
                tables = [row[0] for row in result]
            
            logger.info(f"Created {len(tables)} tables:")
            for table in tables:
                logger.info(f"  - {table}")
                
    except Exception as e:
        logger.error(f"Failed to create tables: {e}")
        raise


async def create_demo_user():
    """创建演示用户"""
    from sqlalchemy.ext.asyncio import AsyncSession
    from services.user_service.models import User
    
    async with AsyncSession(engine) as session:
        try:
            # 检查是否已存在
            result = await session.execute(
                text("SELECT COUNT(*) FROM users WHERE username = 'demo_user'")
            )
            if result.scalar() > 0:
                logger.info("Demo user already exists")
                return
            
            # 创建演示用户
            user = User(
                username="demo_user",
                email="demo@example.com",
                full_name="演示用户",
                is_active=True
            )
            user.set_password("demo123")
            
            session.add(user)
            await session.commit()
            logger.info("Demo user created successfully")
            
        except Exception as e:
            logger.error(f"Failed to create demo user: {e}")
            await session.rollback()


async def main():
    """主函数"""
    logger.info("=" * 60)
    logger.info(" 数据库初始化 - 简化版本")
    logger.info("=" * 60)
    
    try:
        # 创建表
        await create_tables()
        
        # 创建演示用户
        await create_demo_user()
        
        logger.info("\n✅ 数据库初始化完成！")
        logger.info("\n演示账号:")
        logger.info("  用户名: demo_user")
        logger.info("  密码: demo123")
        
    except Exception as e:
        logger.error(f"\n❌ 初始化失败: {e}")
        raise


if __name__ == "__main__":
    asyncio.run(main()) 