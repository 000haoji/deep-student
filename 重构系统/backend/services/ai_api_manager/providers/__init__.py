"""
AI提供者模块
"""
from typing import Optional
from .base import BaseAIProvider
from .openai_provider import OpenAIProvider
from .gemini_provider import GeminiProvider
from .deepseek_provider import DeepSeekProvider
from ..models import AIModelConfig, AIProvider


def create_provider(config: AIModelConfig) -> BaseAIProvider:
    """根据配置创建相应的AI提供者实例"""
    provider_map = {
        AIProvider.OPENAI: OpenAIProvider,
        AIProvider.GEMINI: GeminiProvider,
        AIProvider.DEEPSEEK: DeepSeekProvider,
        AIProvider.CLAUDE: OpenAIProvider,
        AIProvider.QWEN: OpenAIProvider,
    }
    
    provider_class = provider_map.get(config.provider)
    if not provider_class:
        raise ValueError(f"不支持的AI提供者: {config.provider}")
    
    return provider_class(
        api_key=config.api_key,
        api_url=config.api_url,
        model_name=config.model_name,
        timeout=getattr(config, 'timeout', 30),
        max_retries=getattr(config, 'max_retries', 3)
    )


__all__ = ["BaseAIProvider", "OpenAIProvider", "create_provider"] 