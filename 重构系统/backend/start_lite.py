"""
轻量级启动脚本 - 快速启动系统进行测试
"""
import sys
import os
from pathlib import Path

# 将当前目录添加到Python路径
current_dir = Path(__file__).parent
sys.path.insert(0, str(current_dir))

import asyncio
from sqlalchemy import text
from sqlalchemy.ext.asyncio import create_async_engine

async def create_minimal_tables():
    """创建最小必需的表结构"""
    # 使用SQLite
    engine = create_async_engine("sqlite+aiosqlite:///./test_error_management.db")
    
    async with engine.begin() as conn:
        # 创建AI模型表
        await conn.execute(text("""
            CREATE TABLE IF NOT EXISTS ai_models (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL,
                model_name TEXT NOT NULL,
                api_key_encrypted TEXT NOT NULL,
                api_url TEXT NOT NULL,
                priority INTEGER DEFAULT 1,
                is_active INTEGER DEFAULT 1,
                capabilities TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """))
        
        # 创建问题表
        await conn.execute(text("""
            CREATE TABLE IF NOT EXISTS problems (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT,
                subject TEXT NOT NULL,
                difficulty_level INTEGER DEFAULT 3,
                mastery_level REAL DEFAULT 0.0,
                knowledge_points TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                deleted_at TIMESTAMP
            )
        """))
        
        # 创建审查分析表
        await conn.execute(text("""
            CREATE TABLE IF NOT EXISTS review_analyses (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                analysis_type TEXT NOT NULL,
                ai_analysis TEXT,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """))
        
        # 创建文件记录表
        await conn.execute(text("""
            CREATE TABLE IF NOT EXISTS file_records (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                original_filename TEXT NOT NULL,
                file_type TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                storage_path TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )
        """))
        
        print("✓ 最小数据库表创建成功")
    
    await engine.dispose()


async def start_app():
    """启动应用"""
    # 创建表
    await create_minimal_tables()
    
    # 设置环境变量
    os.environ["DATABASE_URL"] = "sqlite+aiosqlite:///./test_error_management.db"
    
    # 导入并启动应用
    import uvicorn
    from main import app
    from shared.config import settings
    
    print(f"\n启动应用: {settings.app_name}")
    print(f"访问地址: http://localhost:{settings.port}")
    print(f"API文档: http://localhost:{settings.port}/docs")
    print("\n注意：这是轻量级模式，某些功能可能受限")
    
    config = uvicorn.Config(
        app,
        host=settings.host,
        port=settings.port,
        reload=False,  # 关闭热重载以避免问题
        log_level="info"
    )
    server = uvicorn.Server(config)
    await server.serve()


if __name__ == "__main__":
    asyncio.run(start_app()) 