"""
配置模块
"""
from typing import List
from pydantic_settings import BaseSettings

class Settings(BaseSettings):
    """应用配置"""
    # 应用基础配置
    APP_NAME: str = "Problem Analysis System"
    APP_ENV: str = "development"
    APP_DEBUG: bool = True
    APP_PORT: int = 8000
    
    # 数据库配置
    DATABASE_URL: str = "sqlite+aiosqlite:///./backend/dev.db"
    DATABASE_ECHO: bool = False
    
    # Redis配置
    REDIS_URL: str = "redis://localhost:6379/0"
    
    # MinIO配置
    MINIO_ENDPOINT: str = "localhost:9000"
    MINIO_ACCESS_KEY: str = "minioadmin"
    MINIO_SECRET_KEY: str = "minioadmin123"
    MINIO_SECURE: bool = False
    MINIO_BUCKET_NAME: str = "problem-images"
    
    # Elasticsearch配置
    ELASTICSEARCH_URL: str = "http://localhost:9200"
    
    # Celery配置
    CELERY_BROKER_URL: str = "amqp://rabbit:rabbit123@localhost:5672//"
    CELERY_RESULT_BACKEND: str = "redis://localhost:6379/1"
    
    # AI模型配置
    OPENAI_API_KEY: str = "your-openai-api-key"
    OPENAI_API_URL: str = "https://api.openai.com/v1"
    GEMINI_API_KEY: str = "your-gemini-api-key"
    GEMINI_API_URL: str = "https://generativelanguage.googleapis.com/v1beta"
    DEEPSEEK_API_KEY: str = "your-deepseek-api-key"
    DEEPSEEK_API_URL: str = "https://api.deepseek.com/v1"
    CLAUDE_API_KEY: str = "your-claude-api-key"
    CLAUDE_API_URL: str = "https://api.anthropic.com/v1"
    ALIYUN_ACCESS_KEY_ID: str = "your-aliyun-access-key-id"
    ALIYUN_ACCESS_KEY_SECRET: str = "your-aliyun-access-key-secret"
    QWEN_API_KEY: str = "your-qwen-api-key"
    QWEN_API_URL: str = "https://dashscope.aliyun.com/compatible-mode/v1"
    
    # CORS配置
    CORS_ORIGINS: List[str] = [
        "http://localhost:3000",
        "http://localhost:5173"
    ]
    
    # 日志配置
    LOG_LEVEL: str = "INFO"
    LOG_FORMAT: str = "json"
    
    # 文件上传配置
    ALLOWED_EXTENSIONS: List[str] = ["png", "jpg", "jpeg", "gif", "pdf"]
    UPLOAD_DIR: str = "uploads"
    MAX_UPLOAD_SIZE: int = 10 * 1024 * 1024  # 10MB
    
    # AI服务配置
    DEFAULT_AI_MODEL: str = "gpt-4-vision-preview"
    AI_REQUEST_TIMEOUT: int = 30
    AI_MAX_RETRIES: int = 3
    AI_ENCRYPTION_KEY: str = "1EJB4R_ETGwNKYyH98K9_9y1qBITz3ZnOrvfWzVffbw="
    
    # 限流配置
    RATE_LIMIT_ENABLED: bool = True
    RATE_LIMIT_DEFAULT: str = "100 per hour"
    
    class Config:
        env_file = ".env"
        case_sensitive = True

settings = Settings()
