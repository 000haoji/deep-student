"""
重新创建数据库脚本
删除旧数据库并创建新的包含所有必要列的数据库
"""
import asyncio
import os
from shared.database import drop_tables, create_tables, init_db
from shared.utils.logger import get_logger

logger = get_logger(__name__)


async def recreate_database():
    """重新创建数据库"""
    try:
        logger.info("开始重新创建数据库...")
        
        # 删除旧的数据库文件
        db_file = "error_management.db"
        if os.path.exists(db_file):
            os.remove(db_file)
            logger.info(f"已删除旧数据库文件: {db_file}")
        
        # 初始化连接
        await init_db()
        
        # 创建所有表
        await create_tables()
        
        logger.info("数据库重新创建完成！")
        
    except Exception as e:
        logger.error(f"数据库重新创建失败: {e}")
        raise


if __name__ == "__main__":
    asyncio.run(recreate_database()) 