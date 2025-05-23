"""
AI API管理器的FastAPI路由定义
"""
from typing import List, Optional
from fastapi import APIRouter, HTTPException, Depends, BackgroundTasks
from sqlalchemy.ext.asyncio import AsyncSession

from shared.database import get_db
from shared.utils.logger import get_logger
from .models import AIModelConfig, AIRequest, AIResponse, TaskType
from .router import AIRouter
from .service import AIModelService

logger = get_logger(__name__)
router = APIRouter(prefix="/api/v1/ai", tags=["AI Management"])

# 创建全局AI路由器实例
ai_router = AIRouter()


@router.post("/models", response_model=dict)
async def create_ai_model(
    config: AIModelConfig,
    db: AsyncSession = Depends(get_db)
):
    """创建新的AI模型配置"""
    service = AIModelService(db)
    model = await service.create_model(config)
    return {
        "success": True,
        "message": "AI模型创建成功",
        "data": {
            "id": str(model.id),
            "provider": model.provider,
            "model_name": model.model_name
        }
    }


@router.get("/models", response_model=dict)
async def list_ai_models(
    provider: Optional[str] = None,
    is_active: Optional[bool] = True,
    db: AsyncSession = Depends(get_db)
):
    """获取AI模型列表"""
    service = AIModelService(db)
    models = await service.list_models(provider=provider, is_active=is_active)
    
    return {
        "success": True,
        "data": [
            {
                "id": str(m.id),
                "provider": m.provider,
                "model_name": m.model_name,
                "is_active": m.is_active,
                "capabilities": m.capabilities,
                "priority": m.priority,
                "success_rate": m.successful_requests / m.total_requests if m.total_requests > 0 else 0
            }
            for m in models
        ],
        "total": len(models)
    }


@router.get("/models/{model_id}", response_model=dict)
async def get_ai_model(
    model_id: str,
    db: AsyncSession = Depends(get_db)
):
    """获取单个AI模型详情"""
    service = AIModelService(db)
    model = await service.get_model(model_id)
    
    if not model:
        raise HTTPException(status_code=404, detail="模型不存在")
    
    return {
        "success": True,
        "data": model.to_dict()
    }


@router.put("/models/{model_id}", response_model=dict)
async def update_ai_model(
    model_id: str,
    config: AIModelConfig,
    db: AsyncSession = Depends(get_db)
):
    """更新AI模型配置"""
    service = AIModelService(db)
    model = await service.update_model(model_id, config)
    
    if not model:
        raise HTTPException(status_code=404, detail="模型不存在")
    
    return {
        "success": True,
        "message": "AI模型更新成功",
        "data": {
            "id": str(model.id),
            "provider": model.provider,
            "model_name": model.model_name
        }
    }


@router.delete("/models/{model_id}", response_model=dict)
async def delete_ai_model(
    model_id: str,
    db: AsyncSession = Depends(get_db)
):
    """删除AI模型（软删除）"""
    service = AIModelService(db)
    success = await service.delete_model(model_id)
    
    if not success:
        raise HTTPException(status_code=404, detail="模型不存在")
    
    return {
        "success": True,
        "message": "AI模型删除成功"
    }


@router.post("/call", response_model=AIResponse)
async def call_ai_api(
    request: AIRequest,
    background_tasks: BackgroundTasks,
    db: AsyncSession = Depends(get_db)
):
    """调用AI API"""
    try:
        # 使用全局AI路由器处理请求
        response = await ai_router.route_request(request)
        
        # 后台记录调用日志
        background_tasks.add_task(
            log_ai_call,
            db,
            request,
            response
        )
        
        return response
    except Exception as e:
        logger.error(f"AI API调用失败: {e}")
        raise HTTPException(status_code=500, detail=str(e))


@router.get("/stats", response_model=dict)
async def get_ai_stats(
    db: AsyncSession = Depends(get_db)
):
    """获取AI使用统计"""
    service = AIModelService(db)
    stats = await service.get_statistics()
    
    return {
        "success": True,
        "data": stats
    }


@router.get("/health", response_model=dict)
async def check_ai_health(
    db: AsyncSession = Depends(get_db)
):
    """检查所有AI模型健康状态"""
    service = AIModelService(db)
    health_status = await service.check_all_health()
    
    return {
        "success": True,
        "data": health_status
    }


@router.post("/models/{model_id}/test", response_model=dict)
async def test_ai_model(
    model_id: str,
    db: AsyncSession = Depends(get_db)
):
    """测试AI模型连接"""
    service = AIModelService(db)
    result = await service.test_model(model_id)
    
    return {
        "success": result["success"],
        "message": result.get("message", "测试完成"),
        "data": {
            "response_time": result.get("response_time"),
            "model_info": result.get("model_info")
        }
    }


async def log_ai_call(
    db: AsyncSession,
    request: AIRequest,
    response: AIResponse
):
    """记录AI调用日志（后台任务）"""
    try:
        service = AIModelService(db)
        await service.log_call(request, response)
    except Exception as e:
        logger.error(f"记录AI调用日志失败: {e}") 