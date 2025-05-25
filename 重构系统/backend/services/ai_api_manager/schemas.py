from typing import List, Optional, Dict, Any, Union
from pydantic import BaseModel, Field, HttpUrl, AwareDatetime
from datetime import datetime

# Import Enums from models.py to ensure single source of truth
# Assuming models.py is in the same directory or python path is configured correctly.
# If services.ai_api_manager.models is the path:
from .models import TaskType as ManagerTaskType, AIProvider as ManagerAIProvider, AICapability as ManagerAICapability

# Schemas for AI Model Configuration
class AIModelBaseSchema(BaseModel):
    """基础AI模型配置 Schema，用于创建和读取。"""
    model_name: str = Field(..., description="模型在提供商处的唯一名称 (e.g., gpt-4, gemini-pro).")
    provider: ManagerAIProvider = Field(..., description="AI提供商")
    
    # Fields for how the system accesses the model
    # These might be stored directly or point to environment variables
    api_key: Optional[str] = Field(None, description="API密钥 (如果直接提供而不是通过环境变量)")
    api_key_env_var: Optional[str] = Field(None, description="存储API密钥的环境变量名 (优先于直接的api_key)")
    base_url: Optional[HttpUrl] = Field(None, description="API基础URL (如果直接提供)")
    base_url_env_var: Optional[str] = Field(None, description="存储API基础URL的环境变量名 (优先于直接的base_url)")

    # Operational parameters
    priority: int = Field(default=10, ge=1, description="模型优先级，数字越小优先级越高")
    is_active: bool = Field(True, description="模型是否激活可用")
    
    capabilities: List[ManagerAICapability] = Field(default_factory=list, description="模型支持的能力列表 (e.g., vision, text)")
    supported_task_types: List[ManagerTaskType] = Field(default_factory=list, description="模型明确支持的任务类型列表")

    # Cost and limits
    cost_per_1k_input_tokens: Optional[float] = Field(None, ge=0, description="每1k输入Token的成本")
    cost_per_1k_output_tokens: Optional[float] = Field(None, ge=0, description="每1k输出Token的成本")
    rpm_limit: Optional[int] = Field(None, ge=0, description="每分钟请求数限制 (RPM)")
    tpm_limit: Optional[int] = Field(None, ge=0, description="每分钟Token数限制 (TPM)")
    max_tokens_limit: Optional[int] = Field(None, ge=0, description="模型支持的最大总Token数 (上下文+生成)") # Renamed from max_tokens to avoid confusion

    # Advanced settings
    timeout_seconds: int = Field(default=120, ge=10, description="API请求超时时间（秒）")
    max_retries: int = Field(default=2, ge=0, description="API请求最大重试次数")
    custom_headers: Optional[Dict[str, str]] = Field(None, description="自定义请求头")

    class Config:
        use_enum_values = True # Important for enums from models.py
        from_attributes = True # Replaced orm_mode
        protected_namespaces = () # For model_name conflict


class AIModelCreateSchema(AIModelBaseSchema):
    """用于创建新AI模型配置的Schema。"""
    pass


class AIModelUpdateSchema(BaseModel):
    """用于更新AI模型配置的Schema。所有字段可选。"""
    model_name: Optional[str] = None
    provider: Optional[ManagerAIProvider] = None
    
    api_key: Optional[str] = None
    api_key_env_var: Optional[str] = None
    base_url: Optional[HttpUrl] = None
    base_url_env_var: Optional[str] = None

    priority: Optional[int] = None
    is_active: Optional[bool] = None
    
    capabilities: Optional[List[ManagerAICapability]] = None
    supported_task_types: Optional[List[ManagerTaskType]] = None

    cost_per_1k_input_tokens: Optional[float] = None
    cost_per_1k_output_tokens: Optional[float] = None
    rpm_limit: Optional[int] = None
    tpm_limit: Optional[int] = None
    max_tokens_limit: Optional[int] = None
    
    timeout_seconds: Optional[int] = None
    max_retries: Optional[int] = None
    custom_headers: Optional[Dict[str, str]] = None

    class Config:
        use_enum_values = True
        from_attributes = True # Replaced orm_mode
        # protected_namespaces is inherited from AIModelBaseSchema


class AIModelResponseSchema(AIModelBaseSchema):
    """用于API响应的AI模型配置Schema，包含数据库ID和时间戳。"""
    id: str # Changed from int to str assuming UUIDs are used for DB IDs
    created_at: AwareDatetime
    updated_at: AwareDatetime

    # Statistics (read-only, populated by the system)
    total_requests: int = Field(default=0)
    successful_requests: int = Field(default=0)
    total_tokens_used: int = Field(default=0)
    total_cost: float = Field(default=0.0)
    last_used_at: Optional[AwareDatetime] = None

    class Config:
        from_attributes = True # Replaced orm_mode
        use_enum_values = True
        # protected_namespaces is inherited from AIModelBaseSchema


class AIModelListResponseSchema(BaseModel):
    success: bool = True
    data: List[AIModelResponseSchema]
    total: int


class SingleAIModelResponseSchema(BaseModel):
    success: bool = True
    data: Optional[AIModelResponseSchema] = None
    message: Optional[str] = None


# Schemas for AI Task Execution
class AIRequestSchema(BaseModel):
    task_type: ManagerTaskType = Field(..., description="AI任务类型")
    
    # Input data for the task
    prompt: Optional[str] = Field(None, description="主提示文本")
    image_url: Optional[HttpUrl] = Field(None, description="图片URL (用于多模态任务)")
    image_base64: Optional[str] = Field(None, description="Base64编码的图片数据 (用于多模态任务)")
    context: Optional[Dict[str, Any]] = Field(None, description="任务特定上下文 (e.g., problem data for analysis)")
    history: Optional[List[Dict[str, str]]] = Field(None, description="对话历史 for chat-like tasks (e.g., [{'role': 'user', 'content': '...'}])")
    
    # Model selection preferences
    preferred_model_name: Optional[str] = Field(None, description="偏好的模型名称 (e.g., gpt-4o)")
    preferred_provider: Optional[ManagerAIProvider] = Field(None, description="偏好的AI提供商")
    
    # Generation parameters
    max_output_tokens: Optional[int] = Field(None, description="期望的最大输出Token数")
    temperature: Optional[float] = Field(None, ge=0, le=2, description="生成温度，控制随机性")
    top_p: Optional[float] = Field(None, ge=0, le=1, description="Top-p (nucleus) sampling")
    # Add other common generation params like frequency_penalty, presence_penalty if needed

    # Operational parameters for this specific request
    stream: bool = Field(False, description="是否使用流式响应")
    output_format: Optional[str] = Field("text", description="期望的输出格式 (e.g., 'text', 'json_object')") # Changed 'json' to 'json_object' for clarity
    # user_id: Optional[str] = None # User system is deprecated

    class Config:
        use_enum_values = True
        from_attributes = True # Replaced orm_mode


class AIResponseDataSchema(BaseModel): # Changed from AIResponse to avoid Pydantic model name collision if used directly
    """AI任务执行结果的Schema。"""
    success: bool
    task_type: ManagerTaskType
    result_text: Optional[str] = Field(None, description="AI处理的文本结果")
    result_json: Optional[Dict[str, Any]] = Field(None, description="AI处理的JSON结构化结果")
    
    model_used: Optional[str] = Field(None, description="实际使用的模型名称")
    provider_used: Optional[ManagerAIProvider] = Field(None, description="实际使用的AI提供商")
    
    error_message: Optional[str] = Field(None, description="如果发生错误，错误信息")
    error_type: Optional[str] = Field(None, description="错误类型 (e.g., 'api_error', 'config_error')")

    # Usage statistics for the call
    prompt_tokens: Optional[int] = Field(None)
    completion_tokens: Optional[int] = Field(None)
    total_tokens: Optional[int] = Field(None)
    cost: Optional[float] = Field(None, description="本次调用的估算成本")
    duration_ms: Optional[float] = Field(None, description="本次调用的处理时长（毫秒）")
    
    # request_id: Optional[str] = Field(None, description="唯一的请求ID，用于追踪") # Can be added by the service

    class Config:
        use_enum_values = True
        from_attributes = True # Replaced orm_mode
        protected_namespaces = () # For model_used conflict


# Schema for task-specific model preferences (if needed, e.g. for a config file)
class AITaskModelPreferenceSchema(BaseModel):
    task_type: ManagerTaskType
    preferred_model_names: List[str] = Field(default_factory=list) # Ordered list of model names
    preferred_providers: List[ManagerAIProvider] = Field(default_factory=list) # Ordered list of providers

    class Config:
        use_enum_values = True
        from_attributes = True # Replaced orm_mode


# Schemas for internal operations like encrypted config (can remain similar)
class EncryptedConfigField(BaseModel):
    key: str 
    value: str # Encrypted value

class EncryptedConfigRequest(BaseModel):
    configs: List[EncryptedConfigField]

class EncryptedConfigResponse(BaseModel):
    success: bool
    message: str
    updated_keys: List[str] = []


# Schemas for Health Check
class AIModelHealthStatusSchema(BaseModel):
    model_name: str
    provider: ManagerAIProvider
    healthy: bool
    message: Optional[str] = None
    response_time_ms: Optional[float] = None
    last_checked_at: Optional[AwareDatetime] = None
    class Config:
        from_attributes = True # Added for consistency, though may not be strictly needed if not from ORM
        protected_namespaces = () # For model_name conflict

class AIProviderHealthStatusSchema(BaseModel):
    provider: ManagerAIProvider
    overall_healthy: bool
    models_status: List[AIModelHealthStatusSchema] = []

class SystemHealthCheckResponseSchema(BaseModel):
    overall_system_healthy: bool
    timestamp: AwareDatetime
    providers_health: List[AIProviderHealthStatusSchema] = []

    class Config:
        use_enum_values = True
        from_attributes = True # Replaced orm_mode

# Example of a more specific request/response for OCR if needed,
# otherwise AIRequestSchema and AIResponseDataSchema can be generic.
class OCRRequestSchema(BaseModel):
    image_url: Optional[HttpUrl] = None
    image_base64: Optional[str] = None
    language_hint: Optional[str] = None # e.g., "eng", "chi_sim"
    # Ensure at least one image source is provided
    # @validator ...

class OCRResponseDataSchema(BaseModel):
    text: str
    language_detected: Optional[str] = None
    confidence: Optional[float] = None
    # bounding_boxes: Optional[List[Dict]] = None # If detailed OCR info is needed
