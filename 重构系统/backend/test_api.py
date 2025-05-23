"""
简单的API测试脚本 - 不依赖数据库
"""
import sys
import os
from pathlib import Path

# 将当前目录添加到Python路径
current_dir = Path(__file__).parent
sys.path.insert(0, str(current_dir))

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
import uvicorn

# 创建简单的FastAPI应用
app = FastAPI(
    title="错题管理系统 API (测试版)",
    description="简化版API用于测试",
    version="1.0.0"
)

# 配置CORS
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 测试路由
@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API (测试版)",
        "status": "running",
        "endpoints": {
            "health": "/health",
            "test_problem": "/api/v1/test/problem",
            "test_analysis": "/api/v1/test/analysis"
        }
    }

@app.get("/health")
async def health_check():
    """健康检查"""
    return {"status": "healthy", "service": "test"}

@app.post("/api/v1/test/problem")
async def test_create_problem(data: dict):
    """测试创建错题"""
    return {
        "success": True,
        "message": "测试接口 - 创建错题",
        "data": {
            "id": "test-123",
            "title": data.get("title", "测试题目"),
            "content": data.get("content", "测试内容"),
            "created_at": "2025-05-23T17:30:00"
        }
    }

@app.get("/api/v1/test/analysis")
async def test_analysis():
    """测试分析功能"""
    return {
        "success": True,
        "message": "测试接口 - 分析功能",
        "analysis": {
            "total_problems": 10,
            "mastery_rate": 0.75,
            "weak_points": ["数学", "英语语法"],
            "suggestions": ["多练习数学计算", "复习英语语法规则"]
        }
    }

if __name__ == "__main__":
    print("启动测试API服务...")
    print("访问地址: http://localhost:8000")
    print("API文档: http://localhost:8000/docs")
    print("\n这是一个简化的测试版本，不包含完整功能")
    
    uvicorn.run(app, host="0.0.0.0", port=8000, log_level="info") 