"""
简单的服务器启动脚本
"""
import uvicorn

if __name__ == "__main__":
    # 启动应用
    uvicorn.run(
        "app:app",
        host="0.0.0.0",
        port=8000,
        log_level="info"
    ) 