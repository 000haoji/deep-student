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
    FollowUpResponse
)
from .service import ReviewService

logger = get_logger(__name__)
router = APIRouter(prefix="/api/v1/reviews", tags=["Reviews"])


@router.post("/batch-analysis", response_model=ReviewAnalysisResponse)
async def create_batch_analysis(
    request: BatchAnalysisRequest,
    db: AsyncSession = Depends(get_db)
):
    """批量分析错题"""
    service = ReviewService(db)
    
    try:
        analysis = await service.analyze_batch_problems(request)
        return ReviewAnalysisResponse(
            success=True,
            message="批量分析完成",
            data=analysis.to_dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"批量分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/knowledge-point-analysis", response_model=ReviewAnalysisResponse)
async def create_knowledge_point_analysis(
    request: KnowledgePointAnalysisRequest,
    db: AsyncSession = Depends(get_db)
):
    """分析特定知识点"""
    service = ReviewService(db)
    
    try:
        analysis = await service.analyze_knowledge_points(request)
        return ReviewAnalysisResponse(
            success=True,
            message="知识点分析完成",
            data=analysis.to_dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"知识点分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/time-period-analysis", response_model=ReviewAnalysisResponse)
async def create_time_period_analysis(
    request: TimePeriodAnalysisRequest,
    db: AsyncSession = Depends(get_db)
):
    """分析时间段内的学习情况"""
    service = ReviewService(db)
    
    try:
        analysis = await service.analyze_time_period(request)
        return ReviewAnalysisResponse(
            success=True,
            message="时间段分析完成",
            data=analysis.to_dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"时间段分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/comprehensive-analysis", response_model=ReviewAnalysisResponse)
async def create_comprehensive_analysis(
    request: ComprehensiveAnalysisRequest,
    db: AsyncSession = Depends(get_db)
):
    """综合分析学习情况"""
    service = ReviewService(db)
    
    try:
        analysis = await service.comprehensive_analysis(request)
        return ReviewAnalysisResponse(
            success=True,
            message="综合分析完成",
            data=analysis.to_dict()
        )
    except ValueError as e:
        raise HTTPException(status_code=400, detail=str(e))
    except Exception as e:
        logger.error(f"综合分析失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.get("/analyses", response_model=AnalysisListResponse)
async def list_analyses(
    analysis_type: Optional[AnalysisType] = None,
    importance_min: Optional[float] = Query(None, ge=0, le=10),
    urgency_min: Optional[float] = Query(None, ge=0, le=10),
    page: int = Query(1, ge=1),
    size: int = Query(20, ge=1, le=100),
    db: AsyncSession = Depends(get_db)
):
    """获取分析列表"""
    service = ReviewService(db)
    
    # TODO: 实现列表查询方法
    analyses = []
    total = 0
    
    return AnalysisListResponse(
        success=True,
        data=[a.to_dict() for a in analyses],
        total=total,
        page=page,
        size=size
    )


@router.get("/analyses/{analysis_id}", response_model=ReviewAnalysisResponse)
async def get_analysis(
    analysis_id: str,
    db: AsyncSession = Depends(get_db)
):
    """获取分析详情"""
    service = ReviewService(db)
    
    # TODO: 实现获取单个分析的方法
    raise HTTPException(status_code=404, detail="分析不存在")


@router.post("/analyses/{analysis_id}/follow-up", response_model=FollowUpResponse)
async def add_follow_up(
    analysis_id: str,
    request: FollowUpRequest,
    db: AsyncSession = Depends(get_db)
):
    """添加分析跟进记录"""
    service = ReviewService(db)
    
    # 确保analysis_id一致
    request.analysis_id = analysis_id
    
    try:
        follow_up = await service.add_follow_up(request)
        return FollowUpResponse(
            success=True,
            message="跟进记录添加成功",
            data={
                "id": str(follow_up.id),
                "analysis_id": str(follow_up.analysis_id),
                "action_taken": follow_up.action_taken,
                "result": follow_up.result,
                "effectiveness_score": follow_up.effectiveness_score,
                "created_at": follow_up.created_at.isoformat()
            }
        )
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        logger.error(f"添加跟进记录失败: {e}")
        raise HTTPException(status_code=500, detail="添加失败")


@router.get("/insights", response_model=dict)
async def get_learning_insights(
    db: AsyncSession = Depends(get_db)
):
    """获取学习洞察"""
    service = ReviewService(db)
    
    try:
        insights = await service.get_learning_insights()
        return {
            "success": True,
            "data": insights
        }
    except Exception as e:
        logger.error(f"获取学习洞察失败: {e}")
        raise HTTPException(status_code=500, detail="获取失败")


@router.get("/health", response_model=dict)
async def health_check():
    """健康检查"""
    return {
        "status": "healthy",
        "service": "review-service"
    } 