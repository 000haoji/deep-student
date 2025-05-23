"""
错题管理系统API演示
展示主要功能的使用示例
"""
import asyncio
import aiohttp
import json
from datetime import datetime


BASE_URL = "http://localhost:8000"


async def demo_problem_api():
    """演示错题管理功能"""
    async with aiohttp.ClientSession() as session:
        print("=== 错题管理系统API演示 ===\n")
        
        # 1. 创建错题
        print("1. 创建新错题...")
        problem_data = {
            "title": "高等数学 - 极限计算",
            "content": "求极限 lim(x→0) (sin x - x) / x³",
            "subject": "math",
            "category": "极限",
            "user_answer": "-1/3",
            "notes": "泰勒展开没有展开到足够的阶数"
        }
        
        async with session.post(
            f"{BASE_URL}/api/v1/test/problem",
            json=problem_data
        ) as resp:
            result = await resp.json()
            print(f"创建结果: {json.dumps(result, indent=2, ensure_ascii=False)}")
            problem_id = result.get("data", {}).get("id")
        
        # 2. 获取分析
        print("\n2. 获取学习分析...")
        async with session.get(f"{BASE_URL}/api/v1/test/analysis") as resp:
            analysis = await resp.json()
            print(f"分析结果: {json.dumps(analysis, indent=2, ensure_ascii=False)}")
        
        # 3. 批量创建示例
        print("\n3. 批量创建错题示例...")
        problems = [
            {
                "title": "英语语法 - 时态",
                "content": "I ___ (go) to school yesterday.",
                "subject": "english",
                "category": "语法",
                "user_answer": "go",
                "correct_answer": "went"
            },
            {
                "title": "政治 - 马克思主义基本原理",
                "content": "什么是生产力？",
                "subject": "politics",
                "category": "基本原理",
                "user_answer": "劳动工具",
                "notes": "答案不完整"
            }
        ]
        
        for p in problems:
            async with session.post(
                f"{BASE_URL}/api/v1/test/problem",
                json=p
            ) as resp:
                result = await resp.json()
                print(f"创建 '{p['title']}': {result.get('success', False)}")
        
        print("\n演示完成！")
        print("\n提示：")
        print("- 访问 http://localhost:8000/docs 查看完整API文档")
        print("- 这是简化的测试版本，完整版本包含更多功能")
        print("- 完整版支持：AI分析、图片OCR、复习计划、知识图谱等")


async def show_system_architecture():
    """展示系统架构"""
    print("\n=== 新系统架构 ===\n")
    
    architecture = """
    ┌─────────────────────────────────────────────────┐
    │                   前端应用                       │
    │         (Vue.js / React / Mobile App)           │
    └─────────────────────┬───────────────────────────┘
                          │ RESTful API
    ┌─────────────────────┴───────────────────────────┐
    │                  API网关                         │
    │              (FastAPI + Auth)                    │
    └─────────────────────┬───────────────────────────┘
                          │
    ┌──────────┬──────────┼──────────┬───────────────┐
    │          │          │          │               │
    ▼          ▼          ▼          ▼               ▼
┌─────────┐┌─────────┐┌─────────┐┌─────────┐ ┌─────────┐
│ AI API  ││ 错题    ││ 复习    ││ 文件    │ │ 用户    │
│ 管理器  ││ 服务    ││ 分析    ││ 服务    │ │ 服务    │
└─────────┘└─────────┘└─────────┘└─────────┘ └─────────┘
     │          │          │          │           │
     └──────────┴──────────┴──────────┴───────────┘
                          │
                    ┌─────┴─────┐
                    │ PostgreSQL│
                    │   Redis   │
                    │   MinIO   │
                    └───────────┘
    """
    
    print(architecture)
    
    print("\n核心特性：")
    print("✓ 微服务架构，各服务独立部署")
    print("✓ 智能AI路由，自动选择最优模型")
    print("✓ 异步处理，高并发支持")
    print("✓ 完整的错误处理和日志记录")
    print("✓ Docker容器化部署")
    print("✓ 自动API文档生成")
    print("✓ 类型安全（Pydantic）")
    print("✓ 数据库迁移管理")


def compare_with_old_system():
    """与旧系统对比"""
    print("\n=== 新旧系统对比 ===\n")
    
    comparison = """
    | 特性         | 旧系统              | 新系统                    |
    |--------------|---------------------|---------------------------|
    | 架构         | 单体应用            | 微服务架构                |
    | 代码组织     | 混乱，循环依赖      | 清晰分层，依赖注入        |
    | API设计      | 不规范              | RESTful + OpenAPI规范     |
    | 数据库       | SQLite，无迁移      | PostgreSQL + 迁移管理     |
    | AI集成       | 硬编码，单一模型    | 智能路由，多模型支持      |
    | 错误处理     | 基本异常捕获        | 结构化错误，详细日志      |
    | 性能         | 同步阻塞            | 异步非阻塞，高并发        |
    | 测试         | 无                  | 单元测试 + 集成测试       |
    | 部署         | 手动                | Docker + CI/CD            |
    | 文档         | README.md           | 自动生成 + 交互式文档     |
    | 扩展性       | 困难                | 插件式架构，易扩展        |
    | 监控         | 无                  | Prometheus + Grafana      |
    """
    
    print(comparison)


if __name__ == "__main__":
    print("错题管理系统 2.0 - 演示程序\n")
    
    # 显示系统架构
    show_system_architecture()
    
    # 显示对比
    compare_with_old_system()
    
    # 运行API演示
    print("\n开始API演示...")
    asyncio.run(demo_problem_api()) 