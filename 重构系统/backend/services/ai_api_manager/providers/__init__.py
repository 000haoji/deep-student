"""
AI提供者模块
"""
from typing import Dict, Any
from .base import BaseAIProvider
from .openai_provider import OpenAIProvider
from .gemini_provider import GeminiProvider
from .deepseek_provider import DeepSeekProvider
# Import AIProvider Enum from models to use as key and for type hint
from ..models import AIProvider as AIProviderEnum 


def create_provider(provider_config_dict: Dict[str, Any]) -> BaseAIProvider:
    """
    根据AI模型数据库配置字典创建相应的AI提供者实例。
    provider_config_dict is expected to be the output of 
    AIModelService._get_provider_config_dict()
    """
    provider_map = {
        AIProviderEnum.OPENAI: OpenAIProvider,
        AIProviderEnum.GEMINI: GeminiProvider,
        AIProviderEnum.DEEPSEEK: DeepSeekProvider,
        # Assuming Claude and Qwen might use an OpenAI-compatible interface or need their own providers
        AIProviderEnum.CLAUDE: OpenAIProvider, # Placeholder, ideally a ClaudeProvider
        AIProviderEnum.QWEN: OpenAIProvider,   # Placeholder, ideally a QwenProvider
    }
    
    provider_enum_instance = provider_config_dict.get("provider_name")
    if not isinstance(provider_enum_instance, AIProviderEnum):
        raise ValueError(f"无效的提供商名称: {provider_enum_instance}. 必须是 AIProvider 枚举实例。")

    provider_class = provider_map.get(provider_enum_instance)
    
    if not provider_class:
        raise ValueError(f"不支持的AI提供者: {provider_enum_instance.value}")
    
    # Prepare arguments for the provider's __init__ method
    # It matches BaseAIProvider.__init__ signature
    init_args = {
        "model_name": provider_config_dict.get("model_name"),
        "provider_name": provider_enum_instance,
        "api_key": provider_config_dict.get("api_key"),
        "base_url": provider_config_dict.get("base_url"),
        "timeout_seconds": provider_config_dict.get("timeout_seconds", 120),
        "max_retries": provider_config_dict.get("max_retries", 2),
        "rpm_limit": provider_config_dict.get("rpm_limit"),
        "tpm_limit": provider_config_dict.get("tpm_limit"),
        "custom_headers": provider_config_dict.get("custom_headers"),
        "max_tokens_limit": provider_config_dict.get("max_tokens_limit")
    }
    
    # Filter out None values for optional parameters if provider __init__ doesn't expect them
    # or handles them appropriately. Current BaseAIProvider.__init__ uses defaults for many.
    # final_init_args = {k: v for k, v in init_args.items() if v is not None}
    # However, explicit None is fine if __init__ type hints are Optional.

    return provider_class(**init_args)


__all__ = [
    "BaseAIProvider", 
    "OpenAIProvider", 
    "GeminiProvider", # Ensure GeminiProvider is exported if used directly
    "DeepSeekProvider", # Ensure DeepSeekProvider is exported
    "create_provider"
]
