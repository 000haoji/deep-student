"""
日志配置模块
使用structlog实现结构化日志
"""
import logging
import sys
from typing import Any, Dict
import structlog
from structlog.processors import JSONRenderer, TimeStamper, add_log_level
from structlog.stdlib import add_logger_name, BoundLogger

from ..config import settings


def setup_logging() -> None:
    """配置日志系统"""
    # 设置Python标准日志级别
    log_level = getattr(logging, settings.log_level.upper())
    
    # 配置标准logging
    logging.basicConfig(
        format="%(message)s",
        stream=sys.stdout,
        level=log_level,
    )
    
    # 配置structlog处理器
    processors = [
        TimeStamper(fmt="iso"),
        add_log_level,
        add_logger_name,
    ]
    
    # 根据配置选择输出格式
    if settings.log_format == "json":
        processors.append(JSONRenderer())
    else:
        processors.append(structlog.dev.ConsoleRenderer())
    
    # 配置structlog
    structlog.configure(
        processors=processors,
        context_class=dict,
        logger_factory=structlog.stdlib.LoggerFactory(),
        cache_logger_on_first_use=True,
    )


def get_logger(name: str) -> BoundLogger:
    """获取日志器实例"""
    return structlog.get_logger(name)


class LoggerMixin:
    """日志混入类，为类提供logger属性"""
    
    @property
    def logger(self) -> BoundLogger:
        """获取绑定的logger实例"""
        if not hasattr(self, "_logger"):
            self._logger = get_logger(self.__class__.__module__)
        return self._logger
    
    def log_info(self, message: str, **kwargs: Any) -> None:
        """记录信息日志"""
        self.logger.info(message, **kwargs)
    
    def log_error(self, message: str, **kwargs: Any) -> None:
        """记录错误日志"""
        self.logger.error(message, **kwargs)
    
    def log_warning(self, message: str, **kwargs: Any) -> None:
        """记录警告日志"""
        self.logger.warning(message, **kwargs)
    
    def log_debug(self, message: str, **kwargs: Any) -> None:
        """记录调试日志"""
        self.logger.debug(message, **kwargs)


def log_api_request(
    method: str,
    path: str,
    status_code: int,
    duration_ms: float,
    **extra: Any
) -> None:
    """记录API请求日志"""
    logger = get_logger("api.request")
    logger.info(
        "api_request",
        method=method,
        path=path,
        status_code=status_code,
        duration_ms=duration_ms,
        **extra
    )


def log_ai_api_call(
    provider: str,
    model: str,
    success: bool,
    duration_ms: float,
    tokens_used: int = 0,
    error: str = None,
    **extra: Any
) -> None:
    """记录AI API调用日志"""
    logger = get_logger("ai.api_call")
    log_data = {
        "provider": provider,
        "model": model,
        "success": success,
        "duration_ms": duration_ms,
        "tokens_used": tokens_used,
        **extra
    }
    
    if error:
        log_data["error"] = error
        logger.error("ai_api_call_failed", **log_data)
    else:
        logger.info("ai_api_call_success", **log_data)


def log_database_query(
    query: str,
    duration_ms: float,
    rows_affected: int = 0,
    **extra: Any
) -> None:
    """记录数据库查询日志"""
    logger = get_logger("database.query")
    logger.debug(
        "database_query",
        query=query[:200],  # 限制查询长度
        duration_ms=duration_ms,
        rows_affected=rows_affected,
        **extra
    ) 