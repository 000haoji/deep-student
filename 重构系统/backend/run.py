"""
系统启动脚本
"""
import sys
import os
from pathlib import Path

# 将当前目录添加到Python路径
current_dir = Path(__file__).parent
sys.path.insert(0, str(current_dir))

import asyncio
import uvicorn
from shared.config import settings
from shared.utils.logger import get_logger
from shared.database import init_db, create_tables

logger = get_logger(__name__)


async def init_system():
    """初始化系统"""
    logger.info("初始化系统...")
    
    try:
        # 创建数据库表
        await create_tables()
        logger.info("数据库表创建成功")
        
        # 初始化其他资源
        # TODO: 添加初始数据、配置等
        
    except Exception as e:
        logger.error(f"系统初始化失败: {e}")
        raise


def main():
    """主函数"""
    # 检查命令行参数
    if len(sys.argv) > 1:
        command = sys.argv[1]
        
        if command == "init":
            # 初始化系统
            logger.info("执行系统初始化...")
            asyncio.run(init_system())
            logger.info("系统初始化完成")
            return
        
        elif command == "migrate":
            # 运行数据库迁移
            logger.info("运行数据库迁移...")
            from infrastructure.database.migrate import run_migrations, check_database
            asyncio.run(run_migrations())
            asyncio.run(check_database())
            return
        
        elif command == "test":
            # 运行测试
            logger.info("运行测试...")
            import pytest
            pytest.main(["-v", "tests/"])
            return
        
        else:
            logger.error(f"未知命令: {command}")
            print("用法:")
            print("  python run.py         # 启动服务")
            print("  python run.py init    # 初始化系统")
            print("  python run.py migrate # 运行数据库迁移")
            print("  python run.py test    # 运行测试")
            return
    
    # 默认启动服务
    logger.info(f"启动 {settings.app_name}...")
    logger.info(f"环境: {settings.environment}")
    logger.info(f"监听地址: http://{settings.host}:{settings.port}")
    logger.info(f"API文档: http://{settings.host}:{settings.port}/docs")
    
    uvicorn.run(
        "main:app",
        host=settings.host,
        port=settings.port,
        reload=settings.debug,
        log_level="info",
        access_log=True
    )


if __name__ == "__main__":
    main() 