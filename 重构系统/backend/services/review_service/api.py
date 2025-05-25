"""
回顾分析服务的API端点
"""
from typing import List, Optional
from fastapi import APIRouter, Depends, HTTPException, Query
from sqlalchemy.ext.asyncio import AsyncSession

from shared.database import get_db
from shared.utils.logger import get_logger
from .models import AnalysisType
from .schemas import (
    BatchAnalysisRequest,
    KnowledgePointAnalysisRequest,
    TimePeriodAnalysisRequest,
    ComprehensiveAnalysisRequest,
    FollowUpRequest,
    ReviewAnalysisResponse, 
    AnalysisListResponse,   
    FollowUpResponse,       
    AnalysisResponse as ReviewAnalysisData # Use AnalysisResponse as ReviewAnalysisData
)
from .service import ReviewService
from uuid import UUID # For validating analysis_id if needed

logger = get_logger(__name__)
router = APIRouter(prefix="/api/v1/reviews", tags=["Reviews"])


@router.post("/batch-analysis", response_model=ReviewAnalysisResponse)
async def create_batch_analysis(
    request: BatchAnalysisRequest,
    # user_id: str = Query(..., description="执行操作的用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """批量分析错题"""
    service = ReviewService(db)
    # user_id REMOVED from service call
    try:
        analysis_orm = await service.analyze_batch_problems(request)
        return ReviewAnalysisResponse(
            success=True,
            message="批量分析完成",
            data=ReviewAnalysisData.from_orm(analysis_orm).dict() # Convert ORM to Pydantic, then to dict
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"批量分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/knowledge-point-analysis", response_model=ReviewAnalysisResponse)
async def create_knowledge_point_analysis(
    request: KnowledgePointAnalysisRequest,
    # user_id: str = Query(..., description="执行操作的用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """分析特定知识点"""
    service = ReviewService(db)
    
    try:
        analysis_orm = await service.analyze_knowledge_points(request) # user_id REMOVED
        return ReviewAnalysisResponse(
            success=True,
            message="知识点分析完成",
            data=ReviewAnalysisData.from_orm(analysis_orm).dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"知识点分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/time-period-analysis", response_model=ReviewAnalysisResponse)
async def create_time_period_analysis(
    request: TimePeriodAnalysisRequest,
    # user_id: str = Query(..., description="执行操作的用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """分析时间段内的学习情况"""
    service = ReviewService(db)
    
    try:
        analysis_orm = await service.analyze_time_period(request) # user_id REMOVED
        return ReviewAnalysisResponse(
            success=True,
            message="时间段分析完成",
            data=ReviewAnalysisData.from_orm(analysis_orm).dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"时间段分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/comprehensive-analysis", response_model=ReviewAnalysisResponse)
async def create_comprehensive_analysis(
    request: ComprehensiveAnalysisRequest,
    # user_id: str = Query(..., description="执行操作的用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """综合分析学习情况"""
    service = ReviewService(db)
    
    try:
        analysis_orm = await service.comprehensive_analysis(request) # user_id REMOVED
        return ReviewAnalysisResponse(
            success=True,
            message="综合分析完成",
            data=ReviewAnalysisData.from_orm(analysis_orm).dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"综合分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.get("/analyses", response_model=AnalysisListResponse)
async def list_analyses(
    # user_id: str = Query(..., description="所属用户ID"), # user_id REMOVED
    analysis_type: Optional[AnalysisType] = Query(None, description="分析类型过滤"),
    importance_min: Optional[float] = Query(None, ge=0, le=10, description="最小重要性评分"),
    urgency_min: Optional[float] = Query(None, ge=0, le=10, description="最小紧急性评分"),
    page: int = Query(1, ge=1, description="页码"),
    size: int = Query(20, ge=1, le=100, description="每页数量"),
    sort_by: str = Query("created_at", description="排序字段"),
    sort_desc: bool = Query(True, description="是否降序排序"),
    db: AsyncSession = Depends(get_db)
):
    """获取分析列表"""
    service = ReviewService(db)
    analyses_orm, total = await service.list_analyses_service(
        # user_id=user_id, # user_id REMOVED
        analysis_type=analysis_type,
        page=page,
        size=size,
        importance_min=importance_min,
        urgency_min=urgency_min,
        sort_by=sort_by,
        sort_desc=sort_desc
    )
    
    return AnalysisListResponse(
        success=True,
        data=[ReviewAnalysisData.from_orm(a).dict() for a in analyses_orm], # Convert each ORM instance
        total=total,
        page=page,
        size=size
    )


@router.get("/analyses/{analysis_id}", response_model=ReviewAnalysisResponse)
async def get_analysis(
    analysis_id: str,
    # user_id: str = Query(..., description="所属用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """获取分析详情"""
    service = ReviewService(db)
    analysis_orm = await service.get_analysis_service(analysis_id) # user_id REMOVED
    
    if not analysis_orm:
        raise HTTPException(status_code=404, detail="分析不存在") # "或无权访问" removed
    
    return ReviewAnalysisResponse(
        success=True,
        data=ReviewAnalysisData.from_orm(analysis_orm).dict()
    )


@router.post("/analyses/{analysis_id}/follow-up", response_model=FollowUpResponse)
async def add_follow_up(
    analysis_id: str, # analysis_id from path will be used in request if needed, or service uses request.analysis_id
    request: FollowUpRequest,
    # user_id: str = Query(..., description="执行操作的用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """添加分析跟进记录"""
    service = ReviewService(db)
    
    # Ensure request.analysis_id is set if service relies on it,
    # though typically it should be part of the FollowUpRequest schema.
    # If FollowUpRequest doesn't have analysis_id, it should be added or service logic adjusted.
    # For now, assuming request.analysis_id is correctly populated by the client or schema.
    # If analysis_id from path is the source of truth, then:
    if not request.analysis_id: # Or if it should always match path
        request.analysis_id = analysis_id

    try:
        # user_id REMOVED from service call
        follow_up_orm = await service.add_follow_up(request)
        return FollowUpResponse(
            success=True,
            message="跟进记录添加成功",
            # Example: converting ORM to dict; adapt if FollowUpResponse.data has specific schema
            data={
                "id": str(follow_up_orm.id),
                "analysis_id": str(follow_up_orm.analysis_id), # Should be a UUID string
                "action_taken": follow_up_orm.action_taken,
                "result": follow_up_orm.result,
                "effectiveness_score": follow_up_orm.effectiveness_score,
                "reviewed_problem_ids": follow_up_orm.reviewed_problem_ids,
                "notes": follow_up_orm.notes,
                "new_mastery_levels": follow_up_orm.new_mastery_levels,
                "created_at": follow_up_orm.created_at.isoformat()
            }
        )
    except ValueError as e: # Raised by service if analysis not found or no permission
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        logger.error(f"添加跟进记录失败: {e}")
        raise HTTPException(status_code=500, detail="添加失败")


@router.get("/insights", response_model=dict)
async def get_learning_insights(
    # user_id: str = Query(..., description="所属用户ID"), # user_id REMOVED
    db: AsyncSession = Depends(get_db)
):
    """获取学习洞察"""
    service = ReviewService(db)
    
    try:
        # user_id REMOVED from service call
        insights = await service.get_learning_insights()
        return {
            "success": True,
            "data": insights
        }
    except Exception as e:
        logger.error(f"获取学习洞察失败: {e}", exc_info=True) # user_id removed from log
        raise HTTPException(status_code=500, detail="获取学习洞察失败")


@router.get("/health", response_model=dict)
async def health_check():
    """健康检查"""
    return {
        "status": "healthy",
        "service": "review-service"
    }
