"""
AI提供商基类
定义所有AI提供商的通用接口
"""
from abc import ABC, abstractmethod
from typing import Dict, Any, Optional, AsyncGenerator
from ..models import TaskType


class BaseAIProvider(ABC):
    """AI提供商基类"""
    
    def __init__(
        self,
        api_key: str,
        api_url: str,
        model_name: str,
        timeout: int = 30,
        max_retries: int = 3
    ):
        self.api_key = api_key
        self.api_url = api_url
        self.model_name = model_name
        self.timeout = timeout
        self.max_retries = max_retries
    
    @abstractmethod
    async def call_api(
        self,
        task_type: TaskType,
        content: Dict[str, Any],
        stream: bool = False,
        max_tokens: Optional[int] = None,
        temperature: float = 0.7,
        timeout: Optional[int] = None,
        **kwargs
    ) -> Dict[str, Any] | AsyncGenerator[Dict[str, Any], None]:
        """
        调用AI API
        
        Args:
            task_type: 任务类型
            content: 请求内容
            stream: 是否流式响应
            max_tokens: 最大令牌数
            temperature: 温度参数
            timeout: 超时时间
            **kwargs: 其他参数
            
        Returns:
            响应数据或流式生成器
        """
        pass
    
    @abstractmethod
    async def check_health(self) -> bool:
        """
        检查服务健康状态
        
        Returns:
            是否健康
        """
        pass
    
    @abstractmethod
    def calculate_cost(self, usage: Dict[str, int]) -> float:
        """
        计算使用成本
        
        Args:
            usage: 使用统计（如tokens数量）
            
        Returns:
            成本金额
        """
        pass
    
    async def close(self) -> None:
        """
        关闭连接（如果需要）
        """
        pass 