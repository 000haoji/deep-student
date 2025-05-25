"""
AI API管理器的FastAPI路由定义
"""
from typing import List, Optional, Union, AsyncGenerator
from fastapi import APIRouter, HTTPException, Depends, BackgroundTasks, Query
from fastapi.responses import StreamingResponse
from sqlalchemy.ext.asyncio import AsyncSession

from shared.database import get_db
from shared.utils.logger import get_logger
from .service import AIModelService
from .schemas import (
    AIModelCreateSchema,
    AIModelUpdateSchema,
    AIModelResponseSchema, # For individual model responses
    AIModelListResponseSchema, # For lists of models
    AIRequestSchema,
    AIResponseDataSchema, # For non-streaming responses
    SystemHealthCheckResponseSchema,
    AIModelHealthStatusSchema
)
from .models import AIProvider as AIProviderEnum, TaskType as TaskTypeEnum, AICapability as AICapabilityEnum
from .models import AIModel as AIModelDB # For service layer return types if needed for casting

logger = get_logger(__name__)
router = APIRouter(prefix="/api/v1/ai", tags=["AI Management"])

# 全局AI路由器实例不再需要，AIService 将处理逻辑
# ai_router = AIRouter() 

def get_ai_service(db: AsyncSession = Depends(get_db)) -> AIModelService:
    """获取AI服务实例"""
    return AIModelService(db)

@router.post("/models", response_model=AIModelResponseSchema, status_code=201)
async def create_ai_model_config(
    config: AIModelCreateSchema,
    service: AIModelService = Depends(get_ai_service)
):
    """创建新的AI模型配置"""
    try:
        model_db = await service.create_model(config)
        return AIModelResponseSchema.from_orm(model_db)
    except ValueError as ve:
        logger.warning(f"创建AI模型失败: {ve}")
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"创建AI模型时发生意外错误: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="创建AI模型时发生内部错误")


@router.get("/models", response_model=AIModelListResponseSchema)
async def list_ai_model_configs(
    provider: Optional[AIProviderEnum] = Query(None, description="按提供商筛选"),
    is_active: Optional[bool] = Query(None, description="按激活状态筛选"),
    capability_filter: Optional[AICapabilityEnum] = Query(None, description="按能力筛选"),
    task_type_filter: Optional[TaskTypeEnum] = Query(None, description="按支持的任务类型筛选"),
    page: int = Query(1, ge=1, description="页码"),
    size: int = Query(20, ge=1, le=100, description="每页数量"),
    service: AIModelService = Depends(get_ai_service)
):
    """获取AI模型配置列表 (分页)"""
    models_db, total_count = await service.list_models_db(
        provider=provider, 
        is_active=is_active, 
        capability_filter=capability_filter,
        task_type_filter=task_type_filter,
        page=page, 
        size=size
    )
    models_resp = [AIModelResponseSchema.from_orm(m) for m in models_db]
    return AIModelListResponseSchema(data=models_resp, total=total_count, page=page, size=size)


@router.get("/models/{model_db_id}", response_model=AIModelResponseSchema)
async def get_ai_model_config(
    model_db_id: str, # Changed from model_id to model_db_id for clarity matching service
    service: AIModelService = Depends(get_ai_service)
):
    """获取单个AI模型配置详情"""
    model_db = await service.get_model_db_by_id(model_db_id)
    if not model_db:
        raise HTTPException(status_code=404, detail="模型配置不存在")
    return AIModelResponseSchema.from_orm(model_db)


@router.put("/models/{model_db_id}", response_model=AIModelResponseSchema)
async def update_ai_model_config(
    model_db_id: str,
    config_update: AIModelUpdateSchema,
    service: AIModelService = Depends(get_ai_service)
):
    """更新AI模型配置"""
    try:
        updated_model_db = await service.update_model(model_db_id, config_update)
        if not updated_model_db:
            raise HTTPException(status_code=404, detail="模型配置不存在")
        return AIModelResponseSchema.from_orm(updated_model_db)
    except ValueError as ve:
        logger.warning(f"更新AI模型 (ID: {model_db_id}) 失败: {ve}")
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"更新AI模型 (ID: {model_db_id}) 时发生意外错误: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"更新AI模型时发生内部错误: {str(e)}")


@router.delete("/models/{model_db_id}", status_code=204) # 204 No Content for successful deletion
async def delete_ai_model_config(
    model_db_id: str,
    service: AIModelService = Depends(get_ai_service)
):
    """删除AI模型配置（软删除）"""
    success = await service.delete_model(model_db_id)
    if not success:
        raise HTTPException(status_code=404, detail="模型配置不存在或删除失败")
    return None # Return None for 204


@router.post("/execute", response_model=AIResponseDataSchema) # response_model will be overridden for stream
async def execute_ai_task(
    request: AIRequestSchema,
    service: AIModelService = Depends(get_ai_service)
    # BackgroundTasks no longer needed here as AIService handles logging
):
    """
    执行AI任务。如果 request.stream 为 True，则返回流式响应。
    否则，返回 AIResponseDataSchema。
    """
    try:
        result = await service.process_ai_request(request)
        
        if isinstance(result, AsyncGenerator):
            # AIService.process_ai_request returned a stream generator
            async def stream_wrapper():
                try:
                    async for chunk in result:
                        if isinstance(chunk, str): # Ensure only strings are yielded for SSE text/event-stream
                            yield f"data: {json.dumps({'type': 'content', 'value': chunk})}\n\n"
                        elif isinstance(chunk, dict) and chunk.get("event") == "error": # For explicit error events
                             yield f"data: {json.dumps({'type': 'error', 'message': chunk.get('data', {}).get('message')})}\n\n"
                        # Other dict events (like usage) are handled internally by AIService for logging,
                        # but not typically streamed back to client unless specifically designed for.
                except Exception as e_stream_api:
                    logger.error(f"Error during API stream generation: {e_stream_api}", exc_info=True)
                    # Yield a final error message if something goes wrong in the wrapper itself
                    yield f"data: {json.dumps({'type': 'error', 'message': f'Stream processing error: {str(e_stream_api)}'})}\n\n"

            return StreamingResponse(stream_wrapper(), media_type="text/event-stream")
        
        # Non-streaming response
        if not isinstance(result, AIResponseDataSchema):
            # This case should ideally not happen if AIService.process_ai_request is correct
            logger.error(f"AI任务执行返回了意外的类型: {type(result)}. Request: {request.dict()}")
            raise HTTPException(status_code=500, detail="AI任务执行返回了意外的响应类型")
            
        if not result.success:
            # You might want to map result.error_type to specific HTTP status codes
            status_code = 500
            if result.error_type == "model_selection_error": status_code = 404
            if result.error_type == "config_error": status_code = 503 # Service unavailable due to config
            # Add more mappings as needed
            raise HTTPException(status_code=status_code, detail=result.error_message or "AI任务执行失败")
            
        return result # This is AIResponseDataSchema

    except HTTPException: # Re-raise HTTPExceptions
        raise
    except Exception as e:
        logger.error(f"执行AI任务失败: {e}. Request: {request.dict(exclude_none=True)}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"执行AI任务时发生内部错误: {str(e)}")


@router.get("/health", response_model=SystemHealthCheckResponseSchema)
async def check_system_health(
    service: AIModelService = Depends(get_ai_service)
):
    """检查所有AI模型和提供商的健康状态"""
    health_status = await service.get_system_health()
    return health_status


@router.post("/models/{model_db_id}/test", response_model=AIModelHealthStatusSchema)
async def test_specific_ai_model_connectivity(
    model_db_id: str,
    service: AIModelService = Depends(get_ai_service)
):
    """测试特定AI模型连接性"""
    try:
        health_status = await service.test_model_connectivity(model_db_id)
        # If health_status.healthy is False, client might interpret the 200 OK with healthy:false payload
        # Or, you could raise HTTPException here if not healthy.
        # For now, returning the schema as is.
        return health_status
    except ValueError as ve: # e.g. model not found
        raise HTTPException(status_code=404, detail=str(ve))
    except Exception as e:
        logger.error(f"测试AI模型 (ID: {model_db_id}) 失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"测试AI模型连接时发生内部错误: {str(e)}")


# Deprecated /stats endpoint. If needed, implement properly or remove.
@router.get("/stats", deprecated=True, summary="[Not Implemented] 获取AI使用统计")
async def get_ai_stats_deprecated(
    service: AIModelService = Depends(get_ai_service)
):
    """获取AI使用统计 (当前未完全实现，可查询日志代替)"""
    # Example: could fetch recent call logs
    # logs, total = await service.get_ai_call_logs(page=1, size=10)
    # return {"message": "Stats endpoint not fully implemented. See logs.", "recent_logs_sample": logs}
    raise HTTPException(status_code=501, detail="统计端点未实现。请查询AI调用日志。")


# Removed /analyze_problem endpoint as it's deprecated and non-functional
# Its functionality should be part of ProblemService or use the generic /execute endpoint.

# Utility for SSE streaming if needed directly (example, not used above as wrapper is inline)
# async def sse_format_generator(async_gen: AsyncGenerator[str, None]):
#     async for item in async_gen:
#         yield f"data: {item}\n\n"
