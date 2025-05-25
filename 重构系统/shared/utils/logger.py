"""
日志工具模块
"""
import logging
from functools import wraps
from typing import Optional

def get_logger(name: str) -> logging.Logger:
    """获取日志记录器"""
    logger = logging.getLogger(name)
    if not logger.handlers:
        handler = logging.StreamHandler()
        formatter = logging.Formatter(
            '%(asctime)s - %(name)s - %(levelname)s - %(message)s'
        )
        handler.setFormatter(formatter)
        logger.addHandler(handler)
        logger.setLevel(logging.INFO)
    return logger

class LoggerMixin:
    """日志混入类"""
    
    def __init__(self):
        self.logger = get_logger(self.__class__.__name__)
    
    def log_info(self, message: str):
        """记录信息日志"""
        self.logger.info(message)
    
    def log_error(self, message: str, exc_info: Optional[Exception] = None):
        """记录错误日志"""
        self.logger.error(message, exc_info=exc_info)
    
    def log_warning(self, message: str):
        """记录警告日志"""
        self.logger.warning(message)
    
    def log_debug(self, message: str):
        """记录调试日志"""
        self.logger.debug(message)

def log_execution(logger: Optional[logging.Logger] = None):
    """函数执行日志装饰器"""
    def decorator(func):
        @wraps(func)
        async def wrapper(*args, **kwargs):
            nonlocal logger
            if logger is None:
                logger = get_logger(func.__module__)
            
            logger.info(f"Executing {func.__name__}")
            try:
                result = await func(*args, **kwargs)
                logger.info(f"Successfully executed {func.__name__}")
                return result
            except Exception as e:
                logger.error(f"Failed to execute {func.__name__}: {str(e)}", exc_info=e)
                raise
        return wrapper
    return decorator 