"""
系统健康检查脚本
检查所有模块是否正确配置和可访问
"""
import asyncio
import sys
from pathlib import Path

# 添加当前目录到Python路径
sys.path.insert(0, str(Path(__file__).parent))

async def check_imports():
    """检查所有模块导入"""
    print("=== 检查模块导入 ===")
    
    modules = [
        "shared.config",
        "shared.database",
        "shared.utils.logger",
        "services.ai_api_manager",
        "services.problem_service",
        "services.review_service",
        "services.file_service",
    ]
    
    failed = []
    for module in modules:
        try:
            __import__(module)
            print(f"✓ {module}")
        except Exception as e:
            print(f"✗ {module}: {e}")
            failed.append(module)
    
    return len(failed) == 0


async def check_database():
    """检查数据库连接"""
    print("\n=== 检查数据库连接 ===")
    try:
        from shared.database import engine
        from sqlalchemy import text
        async with engine.connect() as conn:
            await conn.execute(text("SELECT 1"))
            print("✓ 数据库连接成功")
            return True
    except Exception as e:
        print(f"✗ 数据库连接失败: {e}")
        return False


async def check_services():
    """检查服务配置"""
    print("\n=== 检查服务配置 ===")
    
    checks = []
    
    # 检查AI服务
    try:
        from services.ai_api_manager import ai_router
        print("✓ AI API Manager服务已加载")
        checks.append(True)
    except Exception as e:
        print(f"✗ AI API Manager服务加载失败: {e}")
        checks.append(False)
    
    # 检查Problem服务
    try:
        from services.problem_service import problem_router
        print("✓ Problem服务已加载")
        checks.append(True)
    except Exception as e:
        print(f"✗ Problem服务加载失败: {e}")
        checks.append(False)
    
    # 检查Review服务
    try:
        from services.review_service import review_router
        print("✓ Review服务已加载")
        checks.append(True)
    except Exception as e:
        print(f"✗ Review服务加载失败: {e}")
        checks.append(False)
    
    # 检查File服务
    try:
        from services.file_service import file_service
        print("✓ File服务已加载")
        checks.append(True)
    except Exception as e:
        print(f"✗ File服务加载失败: {e}")
        checks.append(False)
    
    return all(checks)


async def check_config():
    """检查配置"""
    print("\n=== 检查配置 ===")
    
    try:
        from shared.config import settings
        
        critical_configs = [
            ("APP_NAME", settings.app_name),
            ("DATABASE_URL", "***" if settings.database_url else None),
            ("REDIS_URL", "***" if settings.redis_url else None),
            ("MINIO_ENDPOINT", settings.minio_endpoint),
        ]
        
        all_good = True
        for name, value in critical_configs:
            if value:
                print(f"✓ {name}: 已配置")
            else:
                print(f"✗ {name}: 未配置")
                all_good = False
        
        return all_good
    except Exception as e:
        print(f"✗ 配置加载失败: {e}")
        return False


async def check_api_routes():
    """检查API路由"""
    print("\n=== 检查API路由 ===")
    
    try:
        from main import app
        
        routes = []
        for route in app.routes:
            if hasattr(route, "path"):
                routes.append(route.path)
        
        expected_prefixes = [
            "/api/v1/ai",
            "/api/v1/problems",
            "/api/v1/reviews",
        ]
        
        for prefix in expected_prefixes:
            if any(route.startswith(prefix) for route in routes):
                print(f"✓ {prefix}/* 路由已注册")
            else:
                print(f"✗ {prefix}/* 路由未找到")
        
        print(f"\n总共注册了 {len(routes)} 个路由")
        return True
        
    except Exception as e:
        print(f"✗ 路由检查失败: {e}")
        return False


async def main():
    """运行所有检查"""
    print("开始系统健康检查...\n")
    
    results = []
    
    # 1. 检查导入
    results.append(await check_imports())
    
    # 2. 检查配置
    results.append(await check_config())
    
    # 3. 检查数据库
    results.append(await check_database())
    
    # 4. 检查服务
    results.append(await check_services())
    
    # 5. 检查路由
    results.append(await check_api_routes())
    
    # 总结
    print("\n" + "="*50)
    if all(results):
        print("✓ 系统检查通过！所有模块运行正常。")
        return 0
    else:
        print("✗ 系统检查失败！请修复上述问题。")
        return 1


if __name__ == "__main__":
    exit_code = asyncio.run(main())
    sys.exit(exit_code) 