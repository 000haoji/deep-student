"""
完全重置数据库
"""
import os
import asyncio
from sqlalchemy import create_engine, text
from shared.config import settings
from shared.database import Base, engine, create_tables, init_db
from shared.utils.logger import get_logger

# 导入所有模型以确保它们被注册
from services.ai_api_manager.models import AIModel, AICallLog
from services.problem_service.models import Problem, ReviewRecord, ProblemTemplate
from services.review_service.models import ReviewAnalysis, AnalysisFollowUp, LearningPattern
from services.file_service.models import FileRecord

logger = get_logger(__name__)


async def reset_database():
    """重置数据库"""
    try:
        # 1. 删除旧数据库文件
        db_file = "error_management.db"
        if os.path.exists(db_file):
            os.remove(db_file)
            logger.info(f"已删除旧数据库文件: {db_file}")
        
        # 2. 创建新的数据库文件和所有表
        logger.info("创建新数据库...")
        
        # 使用同步引擎创建表（更可靠）
        sync_engine = create_engine(settings.database_url.replace("+aiosqlite", ""))
        Base.metadata.create_all(sync_engine)
        sync_engine.dispose()
        
        logger.info("数据库重置完成！所有表已创建。")
        
        # 3. 测试数据库
        await init_db()
        logger.info("数据库连接测试成功！")
        
    except Exception as e:
        logger.error(f"数据库重置失败: {e}")
        raise


if __name__ == "__main__":
    asyncio.run(reset_database()) 