"""
AI请求路由器
负责智能路由、负载均衡、故障转移等
"""
import asyncio
import random
import time
import uuid
from collections import defaultdict
from datetime import datetime, timedelta
from typing import Dict, List, Optional, AsyncGenerator, Any

from sqlalchemy import select
from sqlalchemy.ext.asyncio import AsyncSession, async_sessionmaker # Import async_sessionmaker

from shared.utils.logger import LoggerMixin
# from shared.database import get_db # get_db is for FastAPI dependencies
from .models import (
    AIModel, AICallLog, AIRequest, AIResponse,
    AIProvider, TaskType, AICapability
)
from .providers.base import BaseAIProvider
from .providers.openai_provider import OpenAIProvider
from .providers.gemini_provider import GeminiProvider
from .providers.deepseek_provider import DeepSeekProvider
from .crypto import decrypt_api_key
# TODO: 添加其他提供商的导入


class AIRouter(LoggerMixin):
    """AI路由器"""
    
    def __init__(self):
        self._providers: Dict[str, BaseAIProvider] = {}
        self._model_cache: Dict[str, AIModel] = {}
        self._health_status: Dict[str, bool] = {}
        self._last_health_check: Dict[str, datetime] = {}
        self._request_count: Dict[str, int] = defaultdict(int)
        self._error_count: Dict[str, int] = defaultdict(int)
        self._lock = asyncio.Lock()
        self._session_maker: Optional[async_sessionmaker[AsyncSession]] = None # Changed to session_maker
    
    def set_session_maker(self, session_maker: async_sessionmaker[AsyncSession]): # Renamed and changed type
        """设置数据库会话工厂"""
        self._session_maker = session_maker
    
    async def initialize(self) -> None: # Removed session argument
        """初始化路由器，加载所有可用的AI模型"""
        if not self._session_maker:
            self.log_error("Database session maker not set in AIRouter. Cannot initialize.")
            return

        async with self._session_maker() as session: # Create session from maker
            # 查询所有激活的AI模型
            result = await session.execute(
                select(AIModel).where(AIModel.is_active == True)
            )
            models = result.scalars().all()
        
        for model in models:
            await self._register_model(model)
        
        self.log_info(f"Initialized AI router with {len(models)} models")
    
    async def _register_model(self, model: AIModel) -> None:
        """注册AI模型"""
        model_key = f"{model.provider}:{model.model_name}"
        
        # 创建提供商实例
        provider = self._create_provider(model)
        if provider:
            self._providers[model_key] = provider
            self._model_cache[model_key] = model
            self._health_status[model_key] = True  # 初始假设健康
            
            self.log_info(
                f"Registered model: {model_key}",
                capabilities=model.capabilities
            )
    
    def _create_provider(self, model: AIModel) -> Optional[BaseAIProvider]:
        """根据模型配置创建提供商实例"""
        # 解密API密钥
        api_key = decrypt_api_key(model.api_key_encrypted)
        provider_map = {
            AIProvider.OPENAI: OpenAIProvider,
            AIProvider.GEMINI: GeminiProvider,
            AIProvider.DEEPSEEK: DeepSeekProvider,
            # TODO: 添加其他提供商映射
        }
        provider_class = provider_map.get(model.provider)
        if not provider_class:
            self.log_error(f"Unknown provider: {model.provider}")
            return None
        return provider_class(
            api_key=api_key,
            api_url=model.api_url,
            model_name=model.model_name,
            timeout=model.timeout,
            max_retries=model.max_retries
        )
    
    async def route_request(
        self,
        request: AIRequest,
        stream: bool = False
    ) -> AIResponse | AsyncGenerator[Dict[str, Any], None]:
        """
        路由AI请求到合适的模型
        
        Args:
            request: AI请求
            stream: 是否需要流式响应
            
        Returns:
            AI响应或流式生成器
        """
        request_id = str(uuid.uuid4())
        start_time = time.time()
        
        try:
            # 1. 选择合适的模型
            selected_model = await self._select_model(request)
            if not selected_model:
                raise ValueError("No suitable model available")
            
            model_key = f"{selected_model.provider}:{selected_model.model_name}"
            provider = self._providers[model_key]
            
            # 2. 记录请求
            self._request_count[model_key] += 1
            
            # 3. 执行请求
            if stream:
                return self._handle_stream_request(
                    provider, selected_model, request, request_id, start_time
                )
            else:
                return await self._handle_single_request(
                    provider, selected_model, request, request_id, start_time
                )
            
        except Exception as e:
            self.log_error(
                "Failed to route request",
                request_id=request_id,
                error=str(e)
            )
            
            # 返回错误响应
            return AIResponse(
                request_id=request_id,
                provider=AIProvider.OPENAI,  # 默认值
                model="unknown",
                content=None,
                usage={},
                duration_ms=(time.time() - start_time) * 1000,
                cost=0.0,
                success=False,
                error=str(e)
            )
    
    async def _select_model(self, request: AIRequest) -> Optional[AIModel]:
        """选择最合适的模型"""
        # 1. 根据任务类型获取支持的模型
        suitable_models = await self._get_suitable_models(request.task_type)
        
        # 2. 根据用户偏好过滤
        if request.preferred_providers:
            suitable_models = [
                m for m in suitable_models
                if m.provider in request.preferred_providers
            ]
        
        if request.preferred_models:
            suitable_models = [
                m for m in suitable_models
                if m.model_name in request.preferred_models
            ]
        
        if not suitable_models:
            return None
        
        # 3. 健康检查过滤
        healthy_models = await self._filter_healthy_models(suitable_models)
        if not healthy_models:
            # 如果没有健康的模型，尝试重新检查所有模型
            await self._perform_health_checks(suitable_models)
            healthy_models = await self._filter_healthy_models(suitable_models)
        
        if not healthy_models:
            return None
        
        # 4. 根据优先级和负载选择
        return self._select_by_priority_and_load(healthy_models)
    
    async def _get_suitable_models(self, task_type: TaskType) -> List[AIModel]:
        """获取支持特定任务类型的模型"""
        # 任务类型到能力的映射
        capability_map = {
            TaskType.OCR: [AICapability.VISION],
            TaskType.PROBLEM_ANALYSIS: [AICapability.TEXT, AICapability.VISION],
            TaskType.REVIEW_ANALYSIS: [AICapability.TEXT],
            TaskType.BATCH_PROBLEM_ANALYSIS: [AICapability.TEXT],
            TaskType.SUMMARIZATION: [AICapability.TEXT],
            TaskType.TRANSLATION: [AICapability.TEXT],
            TaskType.CODE_GENERATION: [AICapability.TEXT],
            TaskType.CODE_REVIEW: [AICapability.TEXT],
            TaskType.MATH_SOLVING: [AICapability.TEXT],
        }
        
        required_capabilities = capability_map.get(task_type, [AICapability.TEXT])
        
        suitable_models = []
        for model_key, model in self._model_cache.items():
            model_capabilities = set(model.capabilities)
            if any(cap in model_capabilities for cap in required_capabilities):
                suitable_models.append(model)
        
        return suitable_models
    
    async def _filter_healthy_models(self, models: List[AIModel]) -> List[AIModel]:
        """过滤出健康的模型"""
        healthy_models = []
        current_time = datetime.now()
        
        for model in models:
            model_key = f"{model.provider}:{model.model_name}"
            
            # 检查是否需要更新健康状态
            last_check = self._last_health_check.get(model_key)
            if not last_check or (current_time - last_check) > timedelta(minutes=5):
                # 异步健康检查
                asyncio.create_task(self._check_model_health(model_key))
            
            # 使用缓存的健康状态
            if self._health_status.get(model_key, False):
                healthy_models.append(model)
        
        return healthy_models
    
    async def _check_model_health(self, model_key: str) -> None:
        """检查单个模型的健康状态"""
        provider = self._providers.get(model_key)
        if not provider:
            return
        
        try:
            is_healthy = await provider.check_health()
            async with self._lock:
                self._health_status[model_key] = is_healthy
                self._last_health_check[model_key] = datetime.now()
                
            if not is_healthy:
                self.log_warning(f"Model {model_key} is unhealthy")
        except Exception as e:
            self.log_error(f"Health check failed for {model_key}: {e}")
            async with self._lock:
                self._health_status[model_key] = False
                self._last_health_check[model_key] = datetime.now()
    
    async def _perform_health_checks(self, models: List[AIModel]) -> None:
        """批量执行健康检查"""
        tasks = []
        for model in models:
            model_key = f"{model.provider}:{model.model_name}"
            tasks.append(self._check_model_health(model_key))
        
        await asyncio.gather(*tasks, return_exceptions=True)
    
    def _select_by_priority_and_load(self, models: List[AIModel]) -> AIModel:
        """根据优先级和负载选择模型"""
        # 按优先级分组
        priority_groups = defaultdict(list)
        for model in models:
            priority_groups[model.priority].append(model)
        
        # 从高优先级开始选择
        for priority in sorted(priority_groups.keys(), reverse=True):
            group = priority_groups[priority]
            
            # 在同一优先级内，使用加权随机选择（基于错误率）
            weights = []
            for model in group:
                model_key = f"{model.provider}:{model.model_name}"
                error_rate = self._calculate_error_rate(model_key)
                # 错误率越低，权重越高
                weight = max(1.0 - error_rate, 0.1)
                weights.append(weight)
            
            # 加权随机选择
            selected = random.choices(group, weights=weights, k=1)[0]
            return selected
        
        # 默认返回第一个
        return models[0]
    
    def _calculate_error_rate(self, model_key: str) -> float:
        """计算模型的错误率"""
        total = self._request_count.get(model_key, 0)
        errors = self._error_count.get(model_key, 0)
        
        if total == 0:
            return 0.0
        
        return errors / total
    
    async def _handle_single_request(
        self,
        provider: BaseAIProvider,
        model: AIModel,
        request: AIRequest,
        request_id: str,
        start_time: float
    ) -> AIResponse:
        """处理单次请求"""
        model_key = f"{model.provider}:{model.model_name}"
        
        try:
            # 调用AI API
            result = await provider.call_api(
                task_type=request.task_type,
                content=request.content,
                max_tokens=request.max_tokens,
                temperature=request.temperature,
                timeout=request.timeout
            )
            
            # 计算耗时和成本
            duration_ms = (time.time() - start_time) * 1000
            tokens_used = result.get("usage", {})
            cost = provider.calculate_cost(tokens_used)
            
            # 记录成功
            await self._record_success(
                model, request, result, duration_ms, cost
            )
            
            # 记录日志
            log_ai_api_call(
                provider=model.provider,
                model=model.model_name,
                success=True,
                duration_ms=duration_ms,
                tokens_used=tokens_used.get("total_tokens", 0)
            )
            
            return AIResponse(
                request_id=request_id,
                provider=model.provider,
                model=model.model_name,
                content=result.get("content"),
                usage=tokens_used,
                duration_ms=duration_ms,
                cost=cost,
                success=True
            )
            
        except Exception as e:
            # 记录错误
            self._error_count[model_key] += 1
            await self._record_failure(model, request, str(e), start_time)
            
            # 记录日志
            log_ai_api_call(
                provider=model.provider,
                model=model.model_name,
                success=False,
                duration_ms=(time.time() - start_time) * 1000,
                error=str(e)
            )
            
            # 尝试故障转移
            return await self._failover(request, [model], str(e))
    
    async def _handle_stream_request(
        self,
        provider: BaseAIProvider,
        model: AIModel,
        request: AIRequest,
        request_id: str,
        start_time: float
    ) -> AsyncGenerator[Dict[str, Any], None]:
        """处理流式请求"""
        model_key = f"{model.provider}:{model.model_name}"
        
        try:
            # 获取流式响应生成器
            stream_generator = await provider.call_api(
                task_type=request.task_type,
                content=request.content,
                stream=True,
                max_tokens=request.max_tokens,
                temperature=request.temperature,
                timeout=request.timeout
            )
            
            # 包装流式响应
            total_tokens = 0
            async for chunk in stream_generator:
                # 添加元数据
                chunk["request_id"] = request_id
                chunk["provider"] = model.provider
                chunk["model"] = model.model_name
                
                yield chunk
                
                # 统计tokens
                if "usage" in chunk:
                    total_tokens = chunk["usage"].get("total_tokens", total_tokens)
            
            # 记录成功
            duration_ms = (time.time() - start_time) * 1000
            await self._record_success(
                model, request, {"usage": {"total_tokens": total_tokens}},
                duration_ms, 0.0  # TODO: 计算流式响应的成本
            )
            
        except Exception as e:
            # 记录错误
            self._error_count[model_key] += 1
            self.log_error(f"Stream request failed: {e}")
            
            # 返回错误消息
            yield {
                "error": str(e),
                "request_id": request_id,
                "provider": model.provider,
                "model": model.model_name
            }
    
    async def _failover(
        self,
        request: AIRequest,
        failed_models: List[AIModel],
        error: str
    ) -> AIResponse:
        """故障转移到备用模型"""
        # 获取所有可用模型，排除已失败的
        all_models = await self._get_suitable_models(request.task_type)
        failed_model_keys = {
            f"{m.provider}:{m.model_name}" for m in failed_models
        }
        
        backup_models = [
            m for m in all_models
            if f"{m.provider}:{m.model_name}" not in failed_model_keys
        ]
        
        if not backup_models:
            # 没有备用模型
            return AIResponse(
                request_id=str(uuid.uuid4()),
                provider=AIProvider.OPENAI,
                model="unknown",
                content=None,
                usage={},
                duration_ms=0,
                cost=0.0,
                success=False,
                error=f"All models failed. Last error: {error}"
            )
        
        # 选择备用模型
        backup_model = self._select_by_priority_and_load(backup_models)
        model_key = f"{backup_model.provider}:{backup_model.model_name}"
        provider = self._providers[model_key]
        
        self.log_info(
            f"Failing over to backup model: {model_key}",
            original_error=error
        )
        
        # 递归调用，使用备用模型
        try:
            return await self._handle_single_request(
                provider, backup_model, request,
                str(uuid.uuid4()), time.time()
            )
        except Exception as e:
            # 继续故障转移
            failed_models.append(backup_model)
            return await self._failover(request, failed_models, str(e))
    
    async def _record_success(
        self,
        model: AIModel,
        request: AIRequest,
        result: Dict[str, Any],
        duration_ms: float,
        cost: float
    ) -> None:
        """记录成功的调用"""
        if not self._session_maker:
            self.log_error("Database session maker not set. Cannot record success.")
            return
        async with self._session_maker() as session: # Create session from maker
            # 创建调用日志
            log = AICallLog(
                model_id=model.id,
                task_type=request.task_type,
                request_data=request.dict(),
                response_data=result,
                tokens_used=result.get("usage", {}).get("total_tokens", 0),
                cost=cost,
                duration_ms=duration_ms,
                status="success"
            )
            session.add(log)
            
            # 更新模型统计
            model.total_requests += 1
            model.successful_requests += 1
            model.total_tokens_used += result.get("usage", {}).get("total_tokens", 0)
            model.total_cost += cost
            model.last_used_at = datetime.now()
            
            # 更新平均响应时间
            if model.average_response_time == 0:
                model.average_response_time = duration_ms
            else:
                # 指数移动平均
                alpha = 0.1
                model.average_response_time = (
                    alpha * duration_ms +
                    (1 - alpha) * model.average_response_time
                )
            
            session.add(model)
            await session.commit()
    
    async def _record_failure(
        self,
        model: AIModel,
        request: AIRequest,
        error: str,
        start_time: float
    ) -> None:
        """记录失败的调用"""
        if not self._session_maker:
            self.log_error("Database session maker not set. Cannot record failure.")
            return
        async with self._session_maker() as session: # Create session from maker
            duration_ms = (time.time() - start_time) * 1000
            
            # 创建调用日志
            log = AICallLog(
                model_id=model.id,
                task_type=request.task_type,
                request_data=request.dict(),
                response_data=None,
                tokens_used=0,
                cost=0.0,
                duration_ms=duration_ms,
                status="failed",
                error_message=error[:1000]  # 限制错误消息长度
            )
            session.add(log)
            
            # 更新模型统计
            model.total_requests += 1
            model.failed_requests += 1
            model.last_error = error[:500]
            model.last_error_at = datetime.now()
            
            session.add(model)
            await session.commit()
    
    async def get_model_stats(self) -> Dict[str, Any]:
        """获取模型统计信息"""
        stats = {
            "total_models": len(self._model_cache),
            "healthy_models": sum(1 for v in self._health_status.values() if v),
            "models": []
        }
        
        for model_key, model in self._model_cache.items():
            model_stats = {
                "key": model_key,
                "provider": model.provider,
                "model": model.model_name,
                "priority": model.priority,
                "is_healthy": self._health_status.get(model_key, False),
                "total_requests": model.total_requests,
                "success_rate": (
                    model.successful_requests / model.total_requests
                    if model.total_requests > 0 else 0
                ),
                "average_response_time": model.average_response_time,
                "total_cost": model.total_cost,
                "error_rate": self._calculate_error_rate(model_key)
            }
            stats["models"].append(model_stats)
        
        # 按优先级和成功率排序
        stats["models"].sort(
            key=lambda x: (-x["priority"], -x["success_rate"])
        )
        
        return stats
    
    async def cleanup(self) -> None:
        """清理资源"""
        # 关闭所有提供商连接
        tasks = []
        for provider in self._providers.values():
            tasks.append(provider.close())
        
        await asyncio.gather(*tasks, return_exceptions=True)
        
        self._providers.clear()
        self._model_cache.clear()
        self._health_status.clear()
        
        self.log_info("AI router cleaned up")


# 创建全局路由器实例
ai_router = AIRouter()
