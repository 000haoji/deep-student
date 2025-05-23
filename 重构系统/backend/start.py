"""
启动脚本 - 设置正确的Python路径并启动应用
"""
import sys
import os
from pathlib import Path

# 将当前目录添加到Python路径
current_dir = Path(__file__).parent
sys.path.insert(0, str(current_dir))

# 设置环境变量
os.environ.setdefault("PYTHONPATH", str(current_dir))

if __name__ == "__main__":
    # 根据命令行参数决定执行什么
    if len(sys.argv) > 1 and sys.argv[1] == "migrate":
        # 运行数据库迁移
        from infrastructure.database.migrate import run_migrations, check_database
        import asyncio
        
        print("运行数据库迁移...")
        asyncio.run(run_migrations())
        print("\n检查数据库状态...")
        asyncio.run(check_database())
    else:
        # 启动应用
        import uvicorn
        from main import app
        from shared.config import settings
        
        print(f"启动应用: {settings.app_name}")
        print(f"访问地址: http://localhost:{settings.port}")
        print(f"API文档: http://localhost:{settings.port}/docs")
        
        uvicorn.run(
            app,
            host=settings.host,
            port=settings.port,
            reload=settings.debug,
            log_level="info"
        ) 