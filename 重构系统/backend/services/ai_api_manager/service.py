"""
AI模型服务层
处理AI模型的CRUD操作和业务逻辑
"""
import time
from typing import List, Optional, Dict, Any
from datetime import datetime
from sqlalchemy import select, and_
from sqlalchemy.ext.asyncio import AsyncSession

from shared.utils.logger import LoggerMixin
from .models import AIModel, AIModelConfig, AIRequest, AIResponse, AICallLog
from .crypto import encrypt_api_key, decrypt_api_key
from .providers import create_provider


class AIModelService(LoggerMixin):
    """AI模型服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
    
    async def create_model(self, config: AIModelConfig) -> AIModel:
        """创建新的AI模型"""
        # 检查是否已存在
        existing = await self.db.execute(
            select(AIModel).where(
                and_(
                    AIModel.provider == config.provider,
                    AIModel.model_name == config.model_name
                )
            )
        )
        
        if existing.scalar_one_or_none():
            raise ValueError(f"模型 {config.provider}:{config.model_name} 已存在")
        
        # 加密API密钥
        encrypted_key = encrypt_api_key(config.api_key)
        
        # 创建模型
        model = AIModel(
            provider=config.provider,
            model_name=config.model_name,
            api_key_encrypted=encrypted_key,
            api_url=config.api_url,
            priority=config.priority,
            is_active=config.is_active,
            capabilities=config.capabilities,
            cost_per_1k_tokens=config.cost_per_1k_tokens,
            max_tokens=config.max_tokens,
            timeout=config.timeout,
            max_retries=config.max_retries,
            custom_headers=config.custom_headers
        )
        
        self.db.add(model)
        await self.db.commit()
        await self.db.refresh(model)
        
        self.log_info(f"创建AI模型: {config.provider}:{config.model_name}")
        return model
    
    async def list_models(
        self,
        provider: Optional[str] = None,
        is_active: Optional[bool] = None
    ) -> List[AIModel]:
        """获取AI模型列表"""
        query = select(AIModel)
        
        if provider:
            query = query.where(AIModel.provider == provider)
        if is_active is not None:
            query = query.where(AIModel.is_active == is_active)
        
        result = await self.db.execute(query.order_by(AIModel.priority.desc()))
        return result.scalars().all()
    
    async def get_model(self, model_id: str) -> Optional[AIModel]:
        """获取单个AI模型"""
        result = await self.db.execute(
            select(AIModel).where(AIModel.id == model_id)
        )
        return result.scalar_one_or_none()
    
    async def update_model(
        self,
        model_id: str,
        config: AIModelConfig
    ) -> Optional[AIModel]:
        """更新AI模型"""
        model = await self.get_model(model_id)
        if not model:
            return None
        
        # 更新字段
        if config.api_key:
            model.api_key_encrypted = encrypt_api_key(config.api_key)
        
        model.api_url = config.api_url
        model.priority = config.priority
        model.is_active = config.is_active
        model.capabilities = config.capabilities
        model.cost_per_1k_tokens = config.cost_per_1k_tokens
        model.max_tokens = config.max_tokens
        model.timeout = config.timeout
        model.max_retries = config.max_retries
        model.custom_headers = config.custom_headers
        
        await self.db.commit()
        await self.db.refresh(model)
        
        self.log_info(f"更新AI模型: {model_id}")
        return model
    
    async def delete_model(self, model_id: str) -> bool:
        """删除AI模型（软删除）"""
        model = await self.get_model(model_id)
        if not model:
            return False
        
        model.is_active = False
        await self.db.commit()
        
        self.log_info(f"禁用AI模型: {model_id}")
        return True
    
    async def test_model(self, model_id: str) -> Dict[str, Any]:
        """测试AI模型连接"""
        model = await self.get_model(model_id)
        if not model:
            raise ValueError("模型不存在")
        
        try:
            # 解密API密钥
            api_key = decrypt_api_key(model.api_key_encrypted)
            
            # 创建临时配置
            config = AIModelConfig(
                provider=model.provider,
                model_name=model.model_name,
                api_key=api_key,
                api_url=model.api_url,
                capabilities=model.capabilities
            )
            
            # 创建提供者并测试
            provider = create_provider(config)
            
            start_time = time.time()
            is_healthy = await provider.check_health()
            response_time = (time.time() - start_time) * 1000
            
            return {
                "success": is_healthy,
                "message": "模型连接正常" if is_healthy else "模型连接失败",
                "response_time": response_time,
                "model_info": {
                    "provider": model.provider,
                    "model_name": model.model_name
                }
            }
        except Exception as e:
            self.log_error(f"测试模型失败: {e}")
            return {
                "success": False,
                "message": str(e),
                "response_time": None,
                "model_info": None
            }
    
    async def log_call(
        self,
        request: AIRequest,
        response: AIResponse
    ) -> None:
        """记录AI调用日志"""
        try:
            # 查找使用的模型
            models = await self.list_models(is_active=True)
            model = None
            for m in models:
                if m.provider == response.provider and m.model_name == response.model:
                    model = m
                    break
            
            if not model:
                self.log_warning(f"找不到模型记录: {response.provider}:{response.model}")
                return
            
            # 创建日志记录
            log = AICallLog(
                model_id=model.id,
                task_type=request.task_type,
                request_data={
                    "content": request.content,
                    "max_tokens": request.max_tokens,
                    "temperature": request.temperature
                },
                response_data={
                    "content": response.content,
                    "usage": response.usage
                },
                tokens_used=response.usage.get("total_tokens", 0),
                cost=response.cost,
                duration_ms=response.duration_ms,
                status="success" if response.success else "failed",
                error_message=response.error
            )
            
            self.db.add(log)
            
            # 更新模型统计
            model.total_requests += 1
            if response.success:
                model.successful_requests += 1
            else:
                model.failed_requests += 1
                model.last_error = response.error
                model.last_error_at = datetime.utcnow()
            
            model.total_tokens_used += response.usage.get("total_tokens", 0)
            model.total_cost += response.cost
            model.last_used_at = datetime.utcnow()
            
            # 更新平均响应时间
            if model.average_response_time == 0:
                model.average_response_time = response.duration_ms
            else:
                # 移动平均
                model.average_response_time = (
                    model.average_response_time * 0.9 + response.duration_ms * 0.1
                )
            
            await self.db.commit()
            
        except Exception as e:
            self.log_error(f"记录AI调用失败: {e}")
            # 不抛出异常，避免影响主流程
    
    async def get_statistics(self) -> Dict[str, Any]:
        """获取AI使用统计"""
        models = await self.list_models()
        
        total_requests = sum(m.total_requests for m in models)
        total_cost = sum(m.total_cost for m in models)
        total_tokens = sum(m.total_tokens_used for m in models)
        
        # 按提供商统计
        provider_stats = {}
        for model in models:
            if model.provider not in provider_stats:
                provider_stats[model.provider] = {
                    "requests": 0,
                    "cost": 0,
                    "tokens": 0,
                    "models": []
                }
            
            provider_stats[model.provider]["requests"] += model.total_requests
            provider_stats[model.provider]["cost"] += model.total_cost
            provider_stats[model.provider]["tokens"] += model.total_tokens_used
            provider_stats[model.provider]["models"].append(model.model_name)
        
        # 最近24小时统计
        recent_logs = await self.db.execute(
            select(AICallLog).where(
                AICallLog.created_at >= datetime.utcnow().replace(
                    hour=0, minute=0, second=0, microsecond=0
                )
            )
        )
        recent_logs = recent_logs.scalars().all()
        
        return {
            "total": {
                "requests": total_requests,
                "cost": total_cost,
                "tokens": total_tokens,
                "models": len(models)
            },
            "by_provider": provider_stats,
            "today": {
                "requests": len(recent_logs),
                "cost": sum(log.cost for log in recent_logs),
                "tokens": sum(log.tokens_used for log in recent_logs)
            },
            "performance": {
                "avg_response_time": sum(m.average_response_time for m in models) / len(models) if models else 0,
                "success_rate": sum(m.successful_requests for m in models) / total_requests if total_requests > 0 else 0
            }
        }
    
    async def check_all_health(self) -> Dict[str, Any]:
        """检查所有AI模型健康状态"""
        models = await self.list_models(is_active=True)
        health_status = {}
        
        for model in models:
            try:
                result = await self.test_model(str(model.id))
                health_status[f"{model.provider}:{model.model_name}"] = {
                    "healthy": result["success"],
                    "response_time": result.get("response_time"),
                    "last_check": datetime.utcnow().isoformat()
                }
            except Exception as e:
                health_status[f"{model.provider}:{model.model_name}"] = {
                    "healthy": False,
                    "error": str(e),
                    "last_check": datetime.utcnow().isoformat()
                }
        
        # 总体健康状态
        healthy_count = sum(1 for status in health_status.values() if status["healthy"])
        total_count = len(health_status)
        
        return {
            "overall_health": healthy_count == total_count,
            "healthy_models": healthy_count,
            "total_models": total_count,
            "models": health_status
        } 