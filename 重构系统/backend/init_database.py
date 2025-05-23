"""
初始化数据库脚本
创建所有必要的数据库表
"""
import asyncio
from shared.database import create_tables, init_db
from shared.utils.logger import get_logger

logger = get_logger(__name__)


async def init_database():
    """初始化数据库"""
    try:
        logger.info("开始初始化数据库...")
        
        # 初始化连接
        await init_db()
        
        # 创建所有表
        await create_tables()
        
        logger.info("数据库初始化完成！")
        
    except Exception as e:
        logger.error(f"数据库初始化失败: {e}")
        raise


if __name__ == "__main__":
    asyncio.run(init_database()) 