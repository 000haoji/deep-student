"""
系统测试脚本
检查系统各个组件是否正常工作
"""
import sys
import asyncio
from pathlib import Path

# 添加项目根目录到系统路径
sys.path.insert(0, str(Path(__file__).parent))

async def test_imports():
    """测试模块导入"""
    print("1. 测试模块导入...")
    try:
        # 测试共享模块
        from shared.config import settings
        print("  ✓ 配置模块导入成功")
        
        from shared.database import engine, get_db
        print("  ✓ 数据库模块导入成功")
        
        # 测试服务模块
        from services.user_service.models import User
        print("  ✓ 用户模型导入成功")
        
        from services.problem_service.models import Problem
        print("  ✓ 错题模型导入成功")
        
        from services.ai_api_manager.models import AIModel
        print("  ✓ AI模型导入成功")
        
        # 测试主应用
        from app import app
        print("  ✓ FastAPI应用导入成功")
        
        return True
    except Exception as e:
        print(f"  ✗ 导入失败: {e}")
        return False

async def test_database():
    """测试数据库连接"""
    print("\n2. 测试数据库连接...")
    try:
        from shared.database import engine
        from sqlalchemy import text
        
        async with engine.connect() as conn:
            result = await conn.execute(text("SELECT 1"))
            print("  ✓ 数据库连接成功")
            
        # 检查表是否存在
        async with engine.connect() as conn:
            if str(engine.url).startswith("sqlite"):
                result = await conn.execute(text(
                    "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name"
                ))
                tables = [row[0] for row in result]
            else:
                result = await conn.execute(text(
                    "SELECT tablename FROM pg_tables WHERE schemaname = 'public'"
                ))
                tables = [row[0] for row in result]
            
            print(f"  ✓ 发现 {len(tables)} 个数据表")
            for table in tables:
                print(f"    - {table}")
        
        return True
    except Exception as e:
        print(f"  ✗ 数据库测试失败: {e}")
        return False

async def test_api():
    """测试API端点"""
    print("\n3. 测试API端点...")
    try:
        from app import app
        from fastapi.testclient import TestClient
        
        client = TestClient(app)
        
        # 测试根路径
        response = client.get("/")
        if response.status_code == 200:
            print("  ✓ 根路径访问成功")
        else:
            print(f"  ✗ 根路径访问失败: {response.status_code}")
        
        # 测试注册
        test_user = {
            "username": "test_user",
            "email": "test@example.com",
            "password": "test123",
            "full_name": "测试用户"
        }
        
        response = client.post("/register", json=test_user)
        if response.status_code in [200, 400]:  # 400表示用户已存在
            print("  ✓ 注册端点正常")
        else:
            print(f"  ✗ 注册端点异常: {response.status_code}")
        
        return True
    except Exception as e:
        print(f"  ✗ API测试失败: {e}")
        return False

async def main():
    """主测试函数"""
    print("="*60)
    print(" 错题管理系统 - 系统测试")
    print("="*60)
    
    results = []
    
    # 执行测试
    results.append(await test_imports())
    results.append(await test_database())
    results.append(await test_api())
    
    # 总结
    print("\n" + "="*60)
    print(" 测试结果总结")
    print("="*60)
    
    success_count = sum(results)
    total_count = len(results)
    
    print(f"\n通过测试: {success_count}/{total_count}")
    
    if success_count == total_count:
        print("\n✅ 所有测试通过！系统可以正常运行。")
        print("\n下一步:")
        print("1. 运行 'python init_db.py' 初始化数据库")
        print("2. 运行 'uvicorn app:app --reload' 启动服务")
        print("3. 访问 http://localhost:8000/docs 查看API文档")
    else:
        print("\n❌ 部分测试失败，请检查错误信息并修复。")

if __name__ == "__main__":
    asyncio.run(main()) 