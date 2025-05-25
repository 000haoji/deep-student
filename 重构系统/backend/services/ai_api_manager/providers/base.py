"""
AI提供商基类
定义所有AI提供商的通用接口
"""
from abc import ABC, abstractmethod
from typing import Dict, Any, Optional, AsyncGenerator, Tuple, Union

# Use AIRequestSchema for a well-defined input structure
from ..schemas import AIRequestSchema 
# TaskType might also be needed if not fully encapsulated in AIRequestSchema for provider logic
from ..models import AIProvider as AIProviderEnum # For typing provider_name


class BaseAIProvider(ABC):
    """AI提供商基类"""
    
    def __init__(
        self,
        model_name: str, # e.g. "gpt-4", "gemini-pro"
        provider_name: AIProviderEnum, # e.g. AIProviderEnum.OPENAI
        api_key: Optional[str],
        base_url: Optional[str], # Changed from api_url for clarity
        timeout_seconds: int = 120,
        max_retries: int = 2,
        rpm_limit: Optional[int] = None,
        tpm_limit: Optional[int] = None,
        custom_headers: Optional[Dict[str, str]] = None,
        max_tokens_limit: Optional[int] = None, # Model's own context/output limit
        # **kwargs: To absorb any other config params from the dict
    ):
        self.model_name = model_name
        self.provider_name = provider_name
        self.api_key = api_key
        self.base_url = base_url
        self.timeout_seconds = timeout_seconds
        self.max_retries = max_retries
        self.rpm_limit = rpm_limit
        self.tpm_limit = tpm_limit
        self.custom_headers = custom_headers or {}
        self.max_tokens_limit = max_tokens_limit
        # self.kwargs = kwargs # Store other params if needed

    @abstractmethod
    async def execute_task(
        self,
        request: AIRequestSchema
    ) -> Dict[str, Any]:
        """
        执行非流式AI任务。
        期望返回一个字典，包含:
        - "content_text": Optional[str]
        - "content_json": Optional[Dict]
        - "usage": {"prompt_tokens": int, "completion_tokens": int, "total_tokens": int}
        - "error": Optional[str] (错误信息)
        - "error_type": Optional[str] (错误类型, e.g., 'api_error', 'config_error', 'rate_limit')
        """
        pass

    @abstractmethod
    async def execute_task_stream(
        self,
        request: AIRequestSchema
    ) -> AsyncGenerator[Union[str, Dict[str, Any]], None]:
        """
        执行流式AI任务。
        应该异步生成 (yield):
        - str: 文本内容块。
        - Dict: 特殊事件，例如 {"event": "usage", "data": {"prompt_tokens": ..., "completion_tokens": ...}}
                  或 {"event": "error", "data": {"message": "...", "type": "..."}}
        """
        pass
    
    @abstractmethod
    async def check_health(self) -> Tuple[bool, Optional[str]]:
        """
        检查服务健康状态。
        
        Returns:
            Tuple (is_healthy: bool, message: Optional[str])
            message 可以是成功信息或错误细节。
        """
        pass
    
    # calculate_cost is now primarily handled by AIModelService.
    # This can be kept if providers have very specific ways to report cost beyond token counts,
    # or removed to simplify the provider interface.
    # For now, let's assume it's not strictly needed here if service calculates from DB model costs.
    # @abstractmethod
    # def calculate_cost(self, usage: Dict[str, int]) -> float:
    #     """
    #     计算使用成本 (如果提供商有特殊计算方式或返回特定成本单位)
    #     """
    #     pass
    
    async def close(self) -> None:
        """
        关闭任何活动的客户端会话或连接（如果需要）。
        例如, aiohttp.ClientSession。
        """
        pass
