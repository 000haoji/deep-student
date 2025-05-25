"""
统计服务数据模型
"""
from datetime import datetime
from typing import List, Optional, Dict, Any
from pydantic import BaseModel

class StatisticsRequest(BaseModel):
    """统计请求参数"""
    timeRange: str = "month"  # week, month, year
    subject: Optional[str] = None
    dateRange: Optional[List[datetime]] = None
    errorPatternSubject: Optional[str] = None
    timeDistributionType: str = "week"  # day, week, month

class StatisticsResponse(BaseModel):
    """统计响应数据"""
    totalProblems: int
    pendingReview: int
    averageMastery: float
    newThisWeek: int
    knowledgePointDistribution: List[Dict[str, Any]]
    difficultyDistribution: List[int]
    masteryTrend: Dict[str, Any]
    errorPatterns: List[Dict[str, Any]]
    timeDistribution: Dict[str, Any] 