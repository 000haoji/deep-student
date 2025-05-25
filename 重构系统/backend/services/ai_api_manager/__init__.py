"""
AI API管理器服务
智能路由和管理多个AI模型
"""
# Avoid importing from submodules in __init__ to prevent circular dependencies.
# Let modules that need these definitions import them directly.

# __all__ can still list the intended public interface of the package,
# but these names won't be directly available via `from . import ...` anymore.
# Users would need to use `from .models import AIProvider` etc.

__all__ = [
    # Enums
    "AIProvider",
    "AICapability",
    "TaskType",

    # Schemas (Pydantic Models)
    "AIModelConfig",
    "AIRequest",
    "AIResponse",

    # Routers & Service (Classes/Instances)
    "router", # The FastAPI router instance from .api
    "ai_router", # The AIRouter instance from .router
    "AIRouter", # The AIRouter class
    "AIModelService", # The AIModelService class
]
