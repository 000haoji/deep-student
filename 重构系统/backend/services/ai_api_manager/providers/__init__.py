"""
AI提供者模块
"""
from typing import Optional
from .base import BaseAIProvider
from .openai_provider import OpenAIProvider
from ..models import AIModelConfig, AIProvider


def create_provider(config: AIModelConfig) -> BaseAIProvider:
    """根据配置创建相应的AI提供者实例"""
    provider_map = {
        AIProvider.OPENAI: OpenAIProvider,
        AIProvider.GEMINI: OpenAIProvider,  # 暂时使用OpenAI的实现
        AIProvider.DEEPSEEK: OpenAIProvider,  # 暂时使用OpenAI的实现
        AIProvider.CLAUDE: OpenAIProvider,  # 暂时使用OpenAI的实现
        AIProvider.QWEN: OpenAIProvider,  # 暂时使用OpenAI的实现
    }
    
    provider_class = provider_map.get(config.provider)
    if not provider_class:
        raise ValueError(f"不支持的AI提供者: {config.provider}")
    
    return provider_class(config)


__all__ = ["BaseAIProvider", "OpenAIProvider", "create_provider"] 