"""
AI模型服务层
处理AI模型的CRUD操作和业务逻辑
"""
import time
import os
from typing import List, Optional, Dict, Any, AsyncGenerator, Union # Added Union
from datetime import datetime
from sqlalchemy import select, and_, update, func # Added func
from sqlalchemy.ext.asyncio import AsyncSession
from shared.utils.logger import LoggerMixin

# Import Enums and ORM Models first
from .models import (
    AIModel as AIModelDB,
    AICallLog,
    AIProvider, # Enum
    TaskType,   # Enum
    AICapability # Enum
)
# Then import Pydantic Schemas
from .schemas import (
    AIModelCreateSchema,
    AIModelUpdateSchema,
    AIModelResponseSchema,
    AIRequestSchema,
    AIResponseDataSchema,
    SystemHealthCheckResponseSchema,
    AIProviderHealthStatusSchema,
    AIModelHealthStatusSchema
)
# Other necessary imports
from .crypto import encrypt_api_key, decrypt_api_key
from .providers import create_provider, BaseAIProvider # Import BaseAIProvider for type hinting


class AIModelService(LoggerMixin):
    """AI模型服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
    
    async def create_model(self, config: AIModelCreateSchema) -> AIModelDB:
        """创建新的AI模型配置"""
        existing_model_stmt = select(AIModelDB).where(
            and_(
                AIModelDB.provider == config.provider,
                AIModelDB.model_name == config.model_name
            )
        )
        existing_model_result = await self.db.execute(existing_model_stmt)
        
        if existing_model_result.scalar_one_or_none():
            self.log_warning(f"Attempt to create duplicate AI model: {config.provider.value}:{config.model_name}")
            raise ValueError(f"模型 {config.provider.value}:{config.model_name} 已存在")
        
        encrypted_api_key_value = None
        if config.api_key and not config.api_key_env_var: # Prioritize env_var if both given, but usually one or other
            encrypted_api_key_value = encrypt_api_key(config.api_key)
        
        db_model = AIModelDB(
            provider=config.provider,
            model_name=config.model_name,
            api_key_encrypted=encrypted_api_key_value, # Store encrypted direct key if provided
            api_key_env_var=config.api_key_env_var,
            api_url=str(config.base_url) if config.base_url else None, # Store direct base_url if provided
            base_url_env_var=config.base_url_env_var,
            priority=config.priority,
            is_active=config.is_active,
            capabilities=[c.value for c in config.capabilities],
            supported_task_types=[t.value for t in config.supported_task_types],
            cost_per_1k_input_tokens=config.cost_per_1k_input_tokens,
            cost_per_1k_output_tokens=config.cost_per_1k_output_tokens,
            # cost_per_1k_tokens remains for legacy or simple models, not directly in create schema now
            max_tokens_limit=config.max_tokens_limit,
            timeout_seconds=config.timeout_seconds,
            max_retries=config.max_retries,
            rpm_limit=config.rpm_limit,
            tpm_limit=config.tpm_limit,
            custom_headers=config.custom_headers
        )
        
        self.db.add(db_model)
        await self.db.commit()
        await self.db.refresh(db_model)
        
        self.log_info(f"创建AI模型: {db_model.provider.value}:{db_model.model_name} (ID: {db_model.id})")
        return db_model

    async def list_models_db(
        self,
        provider: Optional[AIProvider] = None,
        is_active: Optional[bool] = None,
        capability_filter: Optional[AICapability] = None,
        task_type_filter: Optional[TaskType] = None,
        page: int = 1,
        size: int = 20
    ) -> (List[AIModelDB], int): # Returns (models, total_count)
        """获取AI模型DB对象列表 (分页)"""
        offset = (page - 1) * size
        query = select(AIModelDB)
        count_query = select(func.count()).select_from(AIModelDB)

        if provider:
            query = query.where(AIModelDB.provider == provider)
            count_query = count_query.where(AIModelDB.provider == provider)
        if is_active is not None:
            query = query.where(AIModelDB.is_active == is_active)
            count_query = count_query.where(AIModelDB.is_active == is_active)
        
        # For JSON array fields, filtering in SQL can be DB-specific.
        # Example for PostgreSQL using '?' operator for array containment:
        if capability_filter:
            query = query.where(AIModelDB.capabilities.op('?')(capability_filter.value))
            count_query = count_query.where(AIModelDB.capabilities.op('?')(capability_filter.value))
        if task_type_filter:
            query = query.where(AIModelDB.supported_task_types.op('?')(task_type_filter.value))
            count_query = count_query.where(AIModelDB.supported_task_types.op('?')(task_type_filter.value))
        
        total_count_result = await self.db.execute(count_query)
        total_count = total_count_result.scalar_one()

        query = query.order_by(AIModelDB.priority.asc(), AIModelDB.model_name.asc()).offset(offset).limit(size)
        result = await self.db.execute(query)
        models = result.scalars().all()
        return models, total_count

    async def get_model_db_by_id(self, model_db_id: str) -> Optional[AIModelDB]:
        """获取单个AI模型DB对象 by DB ID (UUID string)"""
        result = await self.db.execute(
            select(AIModelDB).where(AIModelDB.id == model_db_id)
        )
        return result.scalar_one_or_none()

    async def get_model_db_by_provider_and_name(self, provider: AIProvider, model_name: str) -> Optional[AIModelDB]:
        """获取单个AI模型DB对象 by provider and name"""
        result = await self.db.execute(
            select(AIModelDB).where(AIModelDB.provider == provider, AIModelDB.model_name == model_name)
        )
        return result.scalar_one_or_none()

    async def update_model(
        self,
        model_db_id: str, 
        config_update: AIModelUpdateSchema
    ) -> Optional[AIModelDB]:
        """更新AI模型配置"""
        model_db = await self.get_model_db_by_id(model_db_id)
        if not model_db:
            self.log_warning(f"Attempt to update non-existent AI model with ID: {model_db_id}")
            return None
        
        update_data = config_update.dict(exclude_unset=True)

        if 'model_name' in update_data and update_data['model_name'] != model_db.model_name:
            new_model_name = update_data['model_name']
            current_provider = AIProvider(update_data['provider']) if 'provider' in update_data else model_db.provider
            
            existing_stmt = select(AIModelDB).where(
                AIModelDB.provider == current_provider,
                AIModelDB.model_name == new_model_name,
                AIModelDB.id != model_db_id
            )
            existing_result = await self.db.execute(existing_stmt)
            if existing_result.scalar_one_or_none():
                raise ValueError(f"模型 {current_provider.value}:{new_model_name} 已存在")
        
        if 'api_key' in update_data and update_data['api_key'] and not update_data.get('api_key_env_var'):
            model_db.api_key_encrypted = encrypt_api_key(update_data['api_key'])
            update_data.pop('api_key') # Handled
        
        if 'base_url' in update_data and update_data['base_url'] and not update_data.get('base_url_env_var'):
            model_db.api_url = str(update_data['base_url'])
            update_data.pop('base_url') # Handled

        if 'capabilities' in update_data and update_data['capabilities'] is not None:
            model_db.capabilities = [c.value for c in update_data['capabilities']]
            update_data.pop('capabilities')

        if 'supported_task_types' in update_data and update_data['supported_task_types'] is not None:
            model_db.supported_task_types = [t.value for t in update_data['supported_task_types']]
            update_data.pop('supported_task_types')


        for key, value in update_data.items():
            if hasattr(model_db, key): # value can be None (to unset optional fields) or False for bools
                setattr(model_db, key, value)
            else:
                self.log_warning(f"Key {key} not found in AIModelDB during update for model ID {model_db_id}")

        try:
            await self.db.commit()
            await self.db.refresh(model_db)
            self.log_info(f"更新AI模型: {model_db.provider.value}:{model_db.model_name} (ID: {model_db_id})")
            return model_db
        except Exception as e:
            await self.db.rollback()
            self.log_error(f"更新AI模型 (ID: {model_db_id}) 失败: {str(e)}")
            raise
    
    async def delete_model(self, model_db_id: str) -> bool:
        """软删除AI模型 (标记为inactive)"""
        model_db = await self.get_model_db_by_id(model_db_id)
        if not model_db:
            return False
        
        model_db.is_active = False
        # If using SoftDeleteMixin, it would handle deleted_at automatically or via a delete method.
        # For now, just setting is_active to False.
        await self.db.commit()
        
        self.log_info(f"已禁用AI模型: {model_db.provider.value}:{model_db.model_name} (ID: {model_db_id})")
        return True
    
    async def _get_provider_config_dict(self, model_db: AIModelDB) -> Dict[str, Any]:
        """Helper to prepare config dict for provider instantiation."""
        api_key_to_use = None
        if model_db.api_key_env_var:
            api_key_to_use = os.getenv(model_db.api_key_env_var)
            if not api_key_to_use:
                self.log_error(f"API key env var {model_db.api_key_env_var} not found for model {model_db.model_name}")
        elif model_db.api_key_encrypted:
             api_key_to_use = decrypt_api_key(model_db.api_key_encrypted)
        
        base_url_to_use = None
        if model_db.base_url_env_var:
            base_url_to_use = os.getenv(model_db.base_url_env_var)
        elif model_db.api_url: # api_url field in DB stores the direct base_url
            base_url_to_use = model_db.api_url

        if not api_key_to_use:
             self.log_warning(f"API key is not configured for model {model_db.model_name} ({model_db.provider.value}). Provider calls may fail.")

        return {
            "model_name": model_db.model_name, # Changed from model_id for clarity, provider expects model's name
            "provider_name": model_db.provider, # Pass the enum itself
            "api_key": api_key_to_use,
            "base_url": base_url_to_use,
            "timeout_seconds": model_db.timeout_seconds,
            "max_retries": model_db.max_retries,
            "rpm_limit": model_db.rpm_limit,
            "tpm_limit": model_db.tpm_limit,
            "custom_headers": model_db.custom_headers,
            "max_tokens_limit": model_db.max_tokens_limit
        }

    async def test_model_connectivity(self, model_db_id: str) -> AIModelHealthStatusSchema:
        """测试AI模型连接性，返回健康状态Schema"""
        model_db = await self.get_model_db_by_id(model_db_id)
        timestamp_now = datetime.now().astimezone()

        if not model_db:
            raise ValueError(f"模型ID {model_db_id} 不存在")
        
        if not model_db.is_active:
            return AIModelHealthStatusSchema(
                model_name=model_db.model_name,
                provider=model_db.provider,
                healthy=False,
                message="模型未激活",
                last_checked_at=timestamp_now
            )
        
        try:
            provider_config_dict = await self._get_provider_config_dict(model_db)
            provider_instance = create_provider(provider_config_dict)
            
            start_time = time.monotonic()
            is_healthy, message = await provider_instance.check_health()
            response_time_ms = (time.monotonic() - start_time) * 1000
            
            return AIModelHealthStatusSchema(
                model_name=model_db.model_name,
                provider=model_db.provider,
                healthy=is_healthy,
                message=message or ("连接成功" if is_healthy else "连接失败"),
                response_time_ms=response_time_ms,
                last_checked_at=timestamp_now
            )
        except Exception as e:
            self.log_error(f"测试模型 {model_db.provider.value}:{model_db.model_name} (ID: {model_db_id}) 失败: {e}", exc_info=True)
            return AIModelHealthStatusSchema(
                model_name=model_db.model_name,
                provider=model_db.provider,
                healthy=False,
                message=str(e),
                last_checked_at=timestamp_now
            )

    def _get_required_capabilities_for_task(self, task_type: TaskType) -> List[AICapability]:
        """Maps a TaskType to required AICapability(ies)."""
        # This mapping needs to be comprehensive.
        mapping = {
            TaskType.OCR: [AICapability.VISION, AICapability.TEXT],
            TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON: [AICapability.VISION, AICapability.ADVANCED_TEXT], # or TEXT
            TaskType.PROBLEM_INTERACTIVE_DEEP_ANALYSIS: [AICapability.ADVANCED_TEXT], # Potentially VISION if history includes images
            TaskType.PROBLEM_ANALYSIS: [AICapability.ADVANCED_TEXT], # Default, may need VISION if images involved
            # ... other mappings ...
            TaskType.SUMMARIZATION: [AICapability.TEXT],
            TaskType.TRANSLATION: [AICapability.TEXT],
        }
        return mapping.get(task_type, [AICapability.TEXT]) # Default to TEXT capability

    async def _select_best_model_for_task(
        self, 
        task_type: TaskType, 
        preferred_model_name: Optional[str] = None,
        preferred_provider: Optional[AIProvider] = None
    ) -> Optional[AIModelDB]:
        """选择最适合任务的激活模型"""
        query = select(AIModelDB).where(AIModelDB.is_active == True)

        if preferred_model_name:
            query = query.where(AIModelDB.model_name == preferred_model_name)
        if preferred_provider:
            query = query.where(AIModelDB.provider == preferred_provider)
        
        # Primary sort: user preference (if specified), then priority, then cost (implicitly by fetching all)
        query = query.order_by(AIModelDB.priority.asc(), AIModelDB.model_name.asc()) # Lower priority number is better

        candidate_models_result = await self.db.execute(query)
        candidate_models = candidate_models_result.scalars().all()

        required_capabilities = self._get_required_capabilities_for_task(task_type)
        
        suitable_models = []
        for model_db in candidate_models:
            # Check 1: Model supports the specific task type (if specified in model's supported_task_types)
            task_type_supported = not model_db.supported_task_types or task_type.value in model_db.supported_task_types
            
            # Check 2: Model has all required generic capabilities
            model_caps_set = set(model_db.capabilities or [])
            capabilities_met = all(rc.value in model_caps_set for rc in required_capabilities)
            
            if task_type_supported and capabilities_met:
                suitable_models.append(model_db)

        if not suitable_models and (preferred_model_name or preferred_provider):
            # Fallback if preferred model didn't meet capability/task_type or no preferred model found
            self.log_warning(f"首选模型不适用或未找到。为任务 {task_type.value} 查找其他适用模型。")
            fallback_query = select(AIModelDB).where(AIModelDB.is_active == True)
            fallback_query = fallback_query.order_by(AIModelDB.priority.asc(), AIModelDB.model_name.asc())
            
            all_active_models_result = await self.db.execute(fallback_query)
            all_active_models = all_active_models_result.scalars().all()
            
            suitable_models = []
            for model_db in all_active_models:
                task_type_supported = not model_db.supported_task_types or task_type.value in model_db.supported_task_types
                model_caps_set = set(model_db.capabilities or [])
                capabilities_met = all(rc.value in model_caps_set for rc in required_capabilities)
                if task_type_supported and capabilities_met:
                    suitable_models.append(model_db)
        
        if not suitable_models:
            self.log_error(f"找不到能够执行任务 {task_type.value} 的激活模型。")
            return None
        
        # TODO: Future: Implement load balancing or more sophisticated selection if multiple models match.
        # For now, returning the first one (highest priority from the filtered list).
        self.log_info(f"为任务 {task_type.value} 选择的模型: {suitable_models[0].provider.value}:{suitable_models[0].model_name}")
        return suitable_models[0]

    async def _calculate_cost(self, model_db: AIModelDB, prompt_tokens: int, completion_tokens: int) -> float:
        cost = 0.0
        if model_db.cost_per_1k_input_tokens is not None and prompt_tokens > 0:
            cost += (prompt_tokens / 1000) * model_db.cost_per_1k_input_tokens
        if model_db.cost_per_1k_output_tokens is not None and completion_tokens > 0:
            cost += (completion_tokens / 1000) * model_db.cost_per_1k_output_tokens
        
        # Fallback to general cost_per_1k_tokens if specific ones are not set but general one is
        if cost == 0.0 and model_db.cost_per_1k_tokens > 0:
            total_tokens = (prompt_tokens or 0) + (completion_tokens or 0)
            cost = (total_tokens / 1000) * model_db.cost_per_1k_tokens
        return round(cost, 6) # Store cost with a reasonable precision

    async def process_ai_request(
        self, request: AIRequestSchema
    ) -> Union[AIResponseDataSchema, AsyncGenerator[str, None]]: # Return type depends on stream
        """
        处理AIRequest：选择模型，实例化提供商，执行任务，记录调用，并返回AIResponseDataSchema。
        如果 request.stream 为 True，则返回一个异步生成器。
        """
        selected_model_db: Optional[AIModelDB] = await self._select_best_model_for_task(
            request.task_type,
            request.preferred_model_name,
            request.preferred_provider
        )

        if not selected_model_db:
            return AIResponseDataSchema(
                success=False,
                task_type=request.task_type,
                error_message=f"找不到适合任务类型 {request.task_type.value} 的AI模型",
                error_type="model_selection_error",
                model_used=request.preferred_model_name,
                provider_used=request.preferred_provider
            )

        try:
            provider_config_dict = await self._get_provider_config_dict(selected_model_db)
            provider_instance: BaseAIProvider = create_provider(provider_config_dict)
            
            start_time = time.monotonic()
            
            if request.stream:
                # Return an async generator that provider_instance.execute_task_stream yields from
                async def stream_response_generator():
                    prompt_tokens_val = 0
                    completion_tokens_val = 0
                    full_response_content = ""
                    error_during_stream = None
                    try:
                        async for chunk_data in provider_instance.execute_task_stream(request):
                            # chunk_data could be a string (content) or a dict with event info
                            if isinstance(chunk_data, str):
                                # Provider yielded a raw content string
                                yield {"type": "content_chunk", "value": chunk_data}
                                full_response_content += chunk_data
                            elif isinstance(chunk_data, dict):
                                # Provider yielded a structured event (e.g., error, usage)
                                event_type = chunk_data.get("event")
                                event_data = chunk_data.get("data", {})

                                if event_type == "usage":
                                    pt_new = event_data.get("prompt_tokens", event_data.get("promptTokenCount"))
                                    ct_new = event_data.get("completion_tokens", event_data.get("candidatesTokenCount", event_data.get("completionTokenCount")))
                                    
                                    if pt_new is not None: prompt_tokens_val = pt_new
                                    if ct_new is not None: completion_tokens_val = ct_new
                                    self.log_debug(f"Stream usage update: p_tokens={prompt_tokens_val}, c_tokens={completion_tokens_val}")
                                    # Usage events are typically not forwarded as SSE data, but processed internally.
                                
                                elif event_type == "error":
                                    error_during_stream = event_data.get("message", str(event_data))
                                    error_type_stream = event_data.get("type", "provider_stream_error")
                                    self.log_error(f"流式传输错误 ({error_type_stream}): {error_during_stream}")
                                    yield {"type": "error", "message": error_during_stream, "error_type": error_type_stream}
                                    break # Stop stream on provider error
                                else:
                                    # Unknown event type from provider, could log or forward if necessary
                                    self.log_warning(f"流式传输收到未知类型的事件: {chunk_data}")
                                    # Optionally yield it if downstream can handle generic dicts
                                    # yield chunk_data 
                            # Final usage data should be captured by the 'usage' event before stream ends or in finally.
                    
                    except Exception as e_stream:
                        self.log_error(f"流式AI请求 (模型 {selected_model_db.model_name}) 异常: {e_stream}", exc_info=True)
                        error_during_stream = str(e_stream)
                        yield {"type": "error", "message": error_during_stream, "error_type": "internal_stream_exception"}
                    
                    finally:
                        # Ensure stream_end is sent if no critical error broke the loop prematurely before logging
                        if not error_during_stream: # Or if error was already yielded and loop broken
                            yield {"type": "stream_end", "session_id": request.context.get("session_id") if request.context else None}

                        duration_ms = (time.monotonic() - start_time) * 1000
                        calculated_cost = await self._calculate_cost(selected_model_db, prompt_tokens_val, completion_tokens_val)
                        
                        log_entry = AICallLog(
                            model_id=str(selected_model_db.id),
                            task_type=request.task_type,
                            request_data=request.dict(),
                            response_data={"aggregated_content": full_response_content, "error": error_during_stream},
                            prompt_tokens=prompt_tokens_val,
                            completion_tokens=completion_tokens_val,
                            total_tokens=(prompt_tokens_val or 0) + (completion_tokens_val or 0),
                            duration_ms=duration_ms,
                            status="success" if not error_during_stream else "failed",
                            is_success = not error_during_stream,
                            error_message=error_during_stream,
                            cost=calculated_cost,
                            called_at=datetime.now().astimezone()
                        )
                        self.db.add(log_entry)
                        
                        model_update_stmt = update(AIModelDB).where(AIModelDB.id == selected_model_db.id).values(
                            total_requests=AIModelDB.total_requests + 1,
                            successful_requests=AIModelDB.successful_requests + (1 if not error_during_stream else 0),
                            failed_requests=AIModelDB.failed_requests + (1 if error_during_stream else 0),
                            total_tokens_used=AIModelDB.total_tokens_used + (log_entry.total_tokens or 0),
                            total_cost=AIModelDB.total_cost + calculated_cost,
                            last_used_at=log_entry.called_at
                        )
                        if error_during_stream:
                            model_update_stmt = model_update_stmt.values(last_error=error_during_stream[:499], last_error_at=log_entry.called_at)
                        
                        await self.db.execute(model_update_stmt)
                        await self.db.commit()
                return stream_response_generator()

            # Non-streaming path
            provider_response_data = await provider_instance.execute_task(request)
            duration_ms = (time.monotonic() - start_time) * 1000
            
            pt = provider_response_data.get("usage", {}).get("prompt_tokens")
            ct = provider_response_data.get("usage", {}).get("completion_tokens")
            calculated_cost = await self._calculate_cost(selected_model_db, pt, ct)

            log_entry = AICallLog(
                model_id=str(selected_model_db.id),
                task_type=request.task_type,
                request_data=request.dict(),
                response_data=provider_response_data, # Store the whole response
                prompt_tokens=pt,
                completion_tokens=ct,
                total_tokens=(pt or 0) + (ct or 0),
                duration_ms=duration_ms,
                status="success" if not provider_response_data.get("error") else "failed",
                is_success=not provider_response_data.get("error"),
                error_message=str(provider_response_data.get("error")) if provider_response_data.get("error") else None,
                cost=calculated_cost,
                called_at=datetime.now().astimezone()
            )
            self.db.add(log_entry)
            
            model_update_stmt = update(AIModelDB).where(AIModelDB.id == selected_model_db.id).values(
                total_requests=AIModelDB.total_requests + 1,
                successful_requests=AIModelDB.successful_requests + (1 if log_entry.is_success else 0),
                failed_requests=AIModelDB.failed_requests + (0 if log_entry.is_success else 1),
                total_tokens_used=AIModelDB.total_tokens_used + (log_entry.total_tokens or 0),
                total_cost=AIModelDB.total_cost + calculated_cost,
                last_used_at=log_entry.called_at
            )
            if log_entry.error_message:
                 model_update_stmt = model_update_stmt.values(last_error=log_entry.error_message[:499], last_error_at=log_entry.called_at)
            await self.db.execute(model_update_stmt)
            await self.db.commit()
            
            if provider_response_data.get("error"):
                return AIResponseDataSchema(
                    success=False,
                    task_type=request.task_type,
                    result_text=provider_response_data.get("content_text"), # Assuming provider returns content_text/content_json
                    result_json=provider_response_data.get("content_json"),
                    model_used=selected_model_db.model_name,
                    provider_used=selected_model_db.provider,
                    error_message=str(provider_response_data.get("error")),
                    error_type=provider_response_data.get("error_type", "provider_error"),
                    prompt_tokens=pt, completion_tokens=ct, total_tokens=(pt or 0) + (ct or 0),
                    cost=calculated_cost, duration_ms=duration_ms
                )

            return AIResponseDataSchema(
                success=True,
                task_type=request.task_type,
                result_text=provider_response_data.get("content_text"),
                result_json=provider_response_data.get("content_json"),
                model_used=selected_model_db.model_name,
                provider_used=selected_model_db.provider,
                prompt_tokens=pt, completion_tokens=ct, total_tokens=(pt or 0) + (ct or 0),
                cost=calculated_cost, duration_ms=duration_ms
            )

        except Exception as e:
            self.log_error(f"处理AI请求 (任务 {request.task_type.value}, 模型 {selected_model_db.model_name if selected_model_db else 'N/A'}) 失败: {str(e)}", exc_info=True)
            return AIResponseDataSchema(
                success=False,
                task_type=request.task_type,
                error_message=str(e),
                error_type="internal_service_error",
                model_used=selected_model_db.model_name if selected_model_db else request.preferred_model_name,
                provider_used=selected_model_db.provider if selected_model_db else request.preferred_provider
            )
    
    async def get_system_health(self) -> SystemHealthCheckResponseSchema:
        """检查所有AI模型和提供商的健康状态"""
        timestamp_now = datetime.now().astimezone()
        active_models_db, _ = await self.list_models_db(is_active=True, size=1000) # Get all active models

        provider_map: Dict[AIProvider, AIProviderHealthStatusSchema] = {}

        for model_db_instance in active_models_db:
            model_health = await self.test_model_connectivity(str(model_db_instance.id))
            
            if model_db_instance.provider not in provider_map:
                provider_map[model_db_instance.provider] = AIProviderHealthStatusSchema(
                    provider=model_db_instance.provider,
                    overall_healthy=True, # Assume true initially
                    models_status=[]
                )
            
            provider_map[model_db_instance.provider].models_status.append(model_health)
            if not model_health.healthy:
                provider_map[model_db_instance.provider].overall_healthy = False # Mark provider unhealthy if any model is

        overall_system_health = all(p_status.overall_healthy for p_status in provider_map.values()) if provider_map else True
        
        return SystemHealthCheckResponseSchema(
            overall_system_healthy=overall_system_health,
            timestamp=timestamp_now,
            providers_health=list(provider_map.values())
        )

    async def get_ai_call_logs(
        self, 
        model_db_id: Optional[str] = None, 
        task_type: Optional[TaskType] = None,
        page: int = 1,
        size: int = 20
    ) -> (List[AICallLog], int):
        """获取AI调用日志 (分页)"""
        offset = (page - 1) * size
        query = select(AICallLog)
        count_query = select(func.count()).select_from(AICallLog)

        if model_db_id:
            query = query.where(AICallLog.model_id == model_db_id)
            count_query = count_query.where(AICallLog.model_id == model_db_id)
        if task_type:
            query = query.where(AICallLog.task_type == task_type)
            count_query = count_query.where(AICallLog.task_type == task_type)
        
        total_count_result = await self.db.execute(count_query)
        total_count = total_count_result.scalar_one()

        query = query.order_by(AICallLog.called_at.desc()).offset(offset).limit(size) # `called_at` is from BaseModel
        result = await self.db.execute(query)
        logs = result.scalars().all()
        return logs, total_count
