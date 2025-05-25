"""
AI API管理器 - 数据库模型
"""
import enum
import uuid # Re-adding uuid for explicit id columns
from datetime import datetime

from sqlalchemy import Column, String, Integer, Float, Boolean, JSON, DateTime, Text, ForeignKey, Enum as SQLEnum, sql # Added sql, replaced Enum with SQLEnum if that was intended
from sqlalchemy.orm import relationship
# Removed: from sqlalchemy.dialects.postgresql import ARRAY

from shared.models.base import Base as SQLBaseModel # Use the correct Base


class AIProvider(enum.Enum):
    """AI提供商枚举"""
    OPENAI = "openai"
    GEMINI = "gemini"
    DEEPSEEK = "deepseek"
    ANTHROPIC = "anthropic" # Example for Claude
    MISTRAL = "mistral"   # Example
    LOCAL = "local"       # For locally hosted models via Ollama, LMStudio etc.
    AZURE_OPENAI = "azure_openai" # Specific variant for Azure OpenAI


class AICapability(enum.Enum):
    """AI模型能力枚举"""
    TEXT = "text"  # Basic text generation, understanding
    ADVANCED_TEXT = "advanced_text" # More complex reasoning, instruction following
    VISION = "vision" # Image understanding
    AUDIO = "audio" # Audio processing (input/output)
    CODE_GENERATION = "code_generation"
    TOOL_USE = "tool_use" # Function calling / tool integration


class TaskType(enum.Enum):
    """AI任务类型枚举"""
    # Problem related tasks
    OCR = "ocr"
    PROBLEM_IMAGE_TO_STRUCTURED_JSON = "problem_image_to_structured_json"
    PROBLEM_INTERACTIVE_DEEP_ANALYSIS = "problem_interactive_deep_analysis" # For chat-like deep dives
    PROBLEM_ANALYSIS = "problem_analysis" # General analysis of a problem (text or structured)
    
    # General AI tasks
    TEXT_GENERATION = "text_generation"
    SUMMARIZATION = "summarization"
    TRANSLATION = "translation"
    QUESTION_ANSWERING = "question_answering"
    CHAT = "chat" # General conversational chat

    # More specific tasks if needed
    CODE_COMPLETION = "code_completion"
    EMBEDDING_GENERATION = "embedding_generation"
    CONTENT_CLASSIFICATION = "content_classification"


class AIModel(SQLBaseModel):
    """AI模型数据库模型"""
    __tablename__ = "ai_models"
    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicitly re-adding id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())
    
    provider = Column(SQLEnum(AIProvider), nullable=False, index=True) # Changed to SQLEnum for consistency
    model_name = Column(String, nullable=False, index=True) # e.g., "gpt-4-turbo", "gemini-1.5-pro-latest"
    
    # Credentials and Access (store encrypted if direct, or name of env var)
    api_key_encrypted = Column(String, nullable=True) # Encrypted API key if stored directly
    api_key_env_var = Column(String, nullable=True)   # Environment variable name for API key
    
    api_url = Column(String, nullable=True) # Base URL for API if not default (e.g. for local models or custom Azure endpoints)
    base_url_env_var = Column(String, nullable=True) # Environment variable name for base URL

    # Operational Parameters
    priority = Column(Integer, default=10, nullable=False, index=True) # Lower is higher priority
    is_active = Column(Boolean, default=True, nullable=False, index=True)
    
    # Capabilities and Supported Tasks (using ARRAY type for PostgreSQL, or JSON for others)
    # For PostgreSQL:
    # capabilities = Column(ARRAY(String), default=list, nullable=True) # List of AICapability enum values
    # supported_task_types = Column(ARRAY(String), default=list, nullable=True) # List of TaskType enum values
    # For other DBs like SQLite, use JSON:
    capabilities = Column(JSON, default=list, nullable=True) # Store as list of strings: [cap.value for cap in capabilities]
    supported_task_types = Column(JSON, default=list, nullable=True)

    # Cost and Limits
    cost_per_1k_input_tokens = Column(Float, nullable=True)
    cost_per_1k_output_tokens = Column(Float, nullable=True)
    cost_per_1k_tokens = Column(Float, nullable=True) # Generic cost if input/output not differentiated
    
    rpm_limit = Column(Integer, nullable=True) # Requests Per Minute
    tpm_limit = Column(Integer, nullable=True) # Tokens Per Minute
    max_tokens_limit = Column(Integer, nullable=True) # Max context + generation tokens model supports

    # Advanced Settings
    timeout_seconds = Column(Integer, default=120, nullable=False)
    max_retries = Column(Integer, default=2, nullable=False)
    custom_headers = Column(JSON, nullable=True) # e.g., {"X-Custom-Header": "value"}
    
    # Statistics / Usage (updated by the system)
    total_requests = Column(Integer, default=0, nullable=False)
    successful_requests = Column(Integer, default=0, nullable=False)
    failed_requests = Column(Integer, default=0, nullable=False) # Added for better stats
    total_tokens_used = Column(Integer, default=0, nullable=False)
    total_cost = Column(Float, default=0.0, nullable=False)
    last_used_at = Column(DateTime(timezone=True), nullable=True)
    last_error = Column(Text, nullable=True) # Store last error message for this model
    last_error_at = Column(DateTime(timezone=True), nullable=True)

    # Timestamps (from SQLBaseModel or explicitly defined if needed)
    # created_at, updated_at are inherited from SQLBaseModel

    # Relationships
    call_logs = relationship("AICallLog", back_populates="model")

    def __repr__(self):
        return f"<AIModel(id='{self.id}', provider='{self.provider.value}', name='{self.model_name}', active={self.is_active})>"


class AICallLog(SQLBaseModel):
    """AI调用日志"""
    __tablename__ = "ai_call_logs"
    id = Column(String(36), primary_key=True, default=lambda: str(uuid.uuid4())) # Explicitly re-adding id
    created_at = Column(DateTime, server_default=sql.func.now())
    updated_at = Column(DateTime, onupdate=sql.func.now())

    model_id = Column(String(36), ForeignKey("ai_models.id"), nullable=False, index=True)
    
    task_type = Column(SQLEnum(TaskType), nullable=False) # Changed to SQLEnum for consistency
    
    request_data = Column(JSON, nullable=True) # Store the input request payload
    response_data = Column(JSON, nullable=True) # Store the output/response from AI (or error structure)
    
    prompt_tokens = Column(Integer, nullable=True)
    completion_tokens = Column(Integer, nullable=True)
    total_tokens = Column(Integer, nullable=True)
    
    duration_ms = Column(Float, nullable=True) # Duration of the AI call in milliseconds
    
    is_success = Column(Boolean, default=True, nullable=False) # More direct status flag
    status = Column(String, default="success", nullable=False) # 'success', 'failed', 'error_provider', 'error_internal'
    error_message = Column(Text, nullable=True)
    
    cost = Column(Float, nullable=True) # Estimated cost of this specific call
    
    # user_id = Column(String(36), ForeignKey("users.id"), nullable=True, index=True) # If tracking calls by user
    # user = relationship("User") # If User model exists and is linked

    called_at = Column(DateTime(timezone=True), default=lambda: datetime.now().astimezone(), nullable=False)

    model = relationship("AIModel", back_populates="call_logs")

    def __repr__(self):
        return f"<AICallLog(id='{self.id}', model_id='{self.model_id}', task='{self.task_type.value}', success={self.is_success})>"

# Diagnostic print to confirm module loading
print(f"Successfully loaded {__name__} with AIModel, AICallLog and Enums.")
