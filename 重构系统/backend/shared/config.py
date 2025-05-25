"""
应用配置管理
使用pydantic-settings管理环境变量和配置
"""
from typing import List, Optional, Union, Dict, Any
from pydantic import Field, validator, root_validator
from pydantic_settings import BaseSettings, SettingsConfigDict
from functools import lru_cache
import os


class Settings(BaseSettings):
    """应用配置"""
    
    # 基本配置
    app_name: str = Field(default="错题管理系统", env="APP_NAME")
    app_env: str = Field(default="development", env="APP_ENV")
    app_debug: bool = Field(default=True, env="APP_DEBUG")
    app_port: int = Field(default=8000, env="APP_PORT")
    host: str = Field(default="0.0.0.0", env="HOST")
    port: int = Field(default=8000, env="PORT")
    debug: bool = Field(default=True, env="DEBUG")
    
    # 数据库配置
    # Use local SQLite for development; ignore environment override
    database_url: str = "sqlite+aiosqlite:///./dev.db"
    database_echo: bool = False
    
    # Redis配置（可选）
    redis_url: Optional[str] = Field(default=None, env="REDIS_URL")
    
    # MinIO配置
    minio_endpoint: str = Field(default="localhost:9000", env="MINIO_ENDPOINT")
    minio_access_key: str = Field(default="minioadmin", env="MINIO_ACCESS_KEY")
    minio_secret_key: str = Field(default="minioadmin", env="MINIO_SECRET_KEY")
    minio_secure: bool = Field(default=False, env="MINIO_SECURE")
    minio_bucket_name: str = Field(default="error-management", env="MINIO_BUCKET_NAME")
    
    # CORS配置
    cors_origins: List[str] = Field(
        default=["http://localhost:3000", "http://localhost:8000"],
        env="CORS_ORIGINS"
    )
    
    # 日志配置
    log_level: str = Field(default="INFO", env="LOG_LEVEL")
    log_format: str = Field(default="json", env="LOG_FORMAT")
    
    # AI API配置
    openai_api_key: Optional[str] = Field(default=None, env="OPENAI_API_KEY")
    openai_api_base: Optional[str] = Field(default=None, env="OPENAI_API_BASE")
    openai_model: str = Field(default="gpt-3.5-turbo", env="OPENAI_MODEL")
    
    anthropic_api_key: Optional[str] = Field(default=None, env="ANTHROPIC_API_KEY")
    anthropic_model: str = Field(default="claude-3-sonnet-20240229", env="ANTHROPIC_MODEL")
    
    # Google AI配置
    google_api_key: Optional[str] = Field(default=None, env="GOOGLE_API_KEY")
    google_model: str = Field(default="gemini-pro", env="GOOGLE_MODEL")
    
    # 文件上传配置
    max_upload_size: int = Field(default=10485760, env="MAX_UPLOAD_SIZE")  # 10MB
    allowed_extensions: List[str] = Field(
        default=[".jpg", ".jpeg", ".png", ".pdf", ".doc", ".docx"],
        env="ALLOWED_EXTENSIONS"
    )
    
    # Elasticsearch配置（可选）
    elasticsearch_url: Optional[str] = Field(default=None, env="ELASTICSEARCH_URL")

    # JWT 密钥
    jwt_secret_key: str = Field(default="a_very_strong_default_secret_key_for_dev_only", env="JWT_SECRET_KEY")

    @root_validator(skip_on_failure=True)
    @classmethod
    def _override_db_for_dev_and_validate_url(cls, values: Dict[str, Any]) -> Dict[str, Any]:
        """
        Overrides database_url and database_echo for development environment.
        Ensures sqlite URLs use the 'sqlite+aiosqlite://' scheme.
        """
        app_env = values.get("app_env")
        
        if app_env == "development":
            # Force SQLite and echo for development
            values["database_url"] = "sqlite+aiosqlite:///./dev.db"
            values["database_echo"] = True
        
        # Validate/normalize database_url format for SQLite, regardless of env
        # This handles cases where DATABASE_URL might be set to "sqlite://" for other envs
        # or if the default is still "sqlite://"
        db_url = values.get("database_url")
        if db_url and db_url.startswith("sqlite://"):
            values["database_url"] = db_url.replace("sqlite://", "sqlite+aiosqlite://")
        
        return values
    
    @property
    def is_development(self) -> bool:
        """是否为开发环境"""
        return self.app_env == "development"
    
    @property
    def is_production(self) -> bool:
        """是否为生产环境"""
        return self.app_env == "production"
    
    @property
    def is_testing(self) -> bool:
        """是否为测试环境"""
        return self.app_env == "testing"
    
    # 配置Pydantic Settings
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
        # 允许从环境变量读取JSON格式的列表
        json_schema_extra={
            "env_parse_none_str": "null",
        }
    )


@lru_cache()
def get_settings() -> Settings:
    """
    获取配置单例
    使用lru_cache确保全局只有一个配置实例
    """
    return Settings()


# 创建全局配置实例
settings = get_settings()
