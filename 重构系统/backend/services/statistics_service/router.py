"""
统计服务API路由
"""
from fastapi import APIRouter, Depends, HTTPException, status # Added status import
from sqlalchemy.ext.asyncio import AsyncSession
from typing import Dict, Any

from shared.database import get_db
from shared.utils.logger import get_logger
# User and auth imports removed
from .service import StatisticsService
from .schemas import StatisticsRequest, StatisticsResponse

router = APIRouter(prefix="/statistics", tags=["statistics"])
logger = get_logger(__name__)

@router.get("/problems", response_model=StatisticsResponse)
async def get_problem_statistics(
    request: StatisticsRequest,
    # current_user: User = Depends(get_current_user), # Removed current_user dependency
    db: AsyncSession = Depends(get_db)
) -> StatisticsResponse: # Changed to StatisticsResponse to match response_model
    """获取错题统计数据"""
    try:
        service = StatisticsService(db)
        # Assuming service.get_problem_statistics no longer needs user_id
        result = await service.get_problem_statistics(request) 
        
        if not result.get("success", False):
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR, # Use status codes from fastapi
                detail=result.get("error", "Failed to get statistics.")
            )
        
        # Assuming result["data"] matches the StatisticsResponse schema
        return result.get("data")
        
    except HTTPException as http_exc:
        # Re-raise HTTPException to let FastAPI handle it
        raise http_exc
    except Exception as e:
        logger.error(f"Failed to get problem statistics: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"An unexpected error occurred while fetching statistics: {str(e)}"
        )
