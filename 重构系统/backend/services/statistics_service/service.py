"""
统计服务业务逻辑
"""
from datetime import datetime, timedelta
from typing import Dict, Any, List, Optional
from sqlalchemy import select, func, and_, or_, desc
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy.sql import text

from shared.utils.logger import LoggerMixin
from backend.services.problem_service.models import Problem, ReviewRecord
from backend.services.review_service.models import ReviewPlan, ReviewStage, ReviewProgress
from .schemas import StatisticsRequest

class StatisticsService(LoggerMixin):
    """统计服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
    
    async def get_problem_statistics(
        self,
        request: StatisticsRequest
    ) -> Dict[str, Any]:
        """获取错题统计数据"""
        try:
            # 获取基础统计
            overview = await self._get_overview_statistics()
            
            # 获取知识点分布
            knowledge_points = await self._get_knowledge_point_distribution(
                request.timeRange
            )
            
            # 获取难度分布
            difficulty = await self._get_difficulty_distribution(
                request.subject
            )
            
            # 获取掌握度趋势
            mastery_trend = await self._get_mastery_trend(
                request.subject,
                request.dateRange
            )
            
            # 获取错误模式分析
            error_patterns = await self._get_error_patterns(
                request.errorPatternSubject
            )
            
            # 获取学习时间分布
            time_distribution = await self._get_time_distribution(
                request.timeDistributionType
            )
            
            return {
                "success": True,
                "data": {
                    "totalProblems": overview["total_problems"],
                    "pendingReview": overview["pending_review"],
                    "averageMastery": overview["average_mastery"],
                    "newThisWeek": overview["new_this_week"],
                    "knowledgePointDistribution": knowledge_points,
                    "difficultyDistribution": difficulty,
                    "masteryTrend": mastery_trend,
                    "errorPatterns": error_patterns,
                    "timeDistribution": time_distribution
                }
            }
            
        except Exception as e:
            self.log_error(f"Failed to get statistics: {e}")
            return {
                "success": False,
                "error": str(e)
            }
    
    async def _get_overview_statistics(self) -> Dict[str, Any]:
        """获取概览统计"""
        # 总题数
        total_problems = await self.db.scalar(
            select(func.count(Problem.id))
            .where(Problem.deleted_at.is_(None))
        )
        
        # 待复习题目数
        pending_review = await self.db.scalar(
            select(func.count(Problem.id))
            .where(
                and_(
                    Problem.deleted_at.is_(None),
                    Problem.mastery_level < 0.8
                )
            )
        )
        
        # 平均掌握度
        average_mastery = await self.db.scalar(
            select(func.avg(Problem.mastery_level))
            .where(Problem.deleted_at.is_(None))
        ) or 0
        
        # 本周新增
        week_start = datetime.now() - timedelta(days=7)
        new_this_week = await self.db.scalar(
            select(func.count(Problem.id))
            .where(
                and_(
                    Problem.deleted_at.is_(None),
                    Problem.created_at >= week_start
                )
            )
        )
        
        return {
            "total_problems": total_problems,
            "pending_review": pending_review,
            "average_mastery": round(average_mastery * 100, 1),
            "new_this_week": new_this_week
        }
    
    async def _get_knowledge_point_distribution(
        self,
        time_range: str
    ) -> List[Dict[str, Any]]:
        """获取知识点分布"""
        # 根据时间范围确定起始时间
        if time_range == "week":
            start_time = datetime.now() - timedelta(days=7)
        elif time_range == "month":
            start_time = datetime.now() - timedelta(days=30)
        else:  # year
            start_time = datetime.now() - timedelta(days=365)
        
        # 查询知识点分布
        result = await self.db.execute(
            select(
                func.json_array_elements(Problem.knowledge_points).label("point"),
                func.count().label("count")
            )
            .where(
                and_(
                    Problem.deleted_at.is_(None),
                    Problem.created_at >= start_time
                )
            )
            .group_by("point")
            .order_by(desc("count"))
            .limit(10)
        )
        
        return [
            {"name": row.point, "value": row.count}
            for row in result
        ]
    
    async def _get_difficulty_distribution(
        self,
        subject: Optional[str] = None
    ) -> List[int]:
        """获取难度分布"""
        query = select(Problem.difficulty_level)
        
        if subject and subject != "all":
            query = query.where(Problem.subject == subject)
        
        query = query.where(Problem.deleted_at.is_(None))
        
        result = await self.db.execute(query)
        difficulties = [row[0] for row in result]
        
        # 统计各难度级别的数量
        distribution = [0] * 5  # 5个难度级别
        for diff in difficulties:
            if 1 <= diff <= 5:
                distribution[diff-1] += 1
        
        return distribution
    
    async def _get_mastery_trend(
        self,
        subject: Optional[str] = None,
        date_range: Optional[List[datetime]] = None
    ) -> Dict[str, Any]:
        """获取掌握度趋势"""
        # 确定时间范围
        if not date_range:
            end_date = datetime.now()
            start_date = end_date - timedelta(days=30)
        else:
            start_date, end_date = date_range
        
        # 构建查询
        query = select(
            func.date_trunc('day', ReviewProgress.completed_at).label("date"),
            func.avg(ReviewProgress.mastery_level).label("mastery")
        )
        
        if subject and subject != "all":
            query = query.join(Problem).where(Problem.subject == subject)
        
        query = (
            query.where(
                and_(
                    ReviewProgress.completed_at.between(start_date, end_date),
                    ReviewProgress.status == "completed"
                )
            )
            .group_by("date")
            .order_by("date")
        )
        
        result = await self.db.execute(query)
        rows = result.all()
        
        return {
            "dates": [row.date.strftime("%Y-%m-%d") for row in rows],
            "values": [round(row.mastery * 100, 1) for row in rows]
        }
    
    async def _get_error_patterns(
        self,
        subject: Optional[str] = None
    ) -> List[Dict[str, Any]]:
        """获取错误模式分析"""
        query = select(
            func.jsonb_object_keys(ReviewProgress.error_analysis).label("pattern"),
            func.count().label("count")
        )
        
        if subject and subject != "all":
            query = query.join(Problem).where(Problem.subject == subject)
        
        query = (
            query.where(ReviewProgress.error_analysis.isnot(None))
            .group_by("pattern")
            .order_by(desc("count"))
            .limit(10)
        )
        
        result = await self.db.execute(query)
        
        return [
            {"name": row.pattern, "value": row.count}
            for row in result
        ]
    
    async def _get_time_distribution(
        self,
        distribution_type: str
    ) -> Dict[str, Any]:
        """获取学习时间分布"""
        if distribution_type == "day":
            # 按小时统计
            query = select(
                func.extract('hour', ReviewProgress.completed_at).label("hour"),
                func.sum(ReviewProgress.time_spent).label("total_time")
            )
            group_by = "hour"
            order_by = "hour"
            labels = [f"{i:02d}:00" for i in range(24)]
            
        elif distribution_type == "week":
            # 按星期统计
            query = select(
                func.extract('dow', ReviewProgress.completed_at).label("day"),
                func.sum(ReviewProgress.time_spent).label("total_time")
            )
            group_by = "day"
            order_by = "day"
            labels = ["周日", "周一", "周二", "周三", "周四", "周五", "周六"]
            
        else:  # month
            # 按日期统计
            query = select(
                func.date_trunc('day', ReviewProgress.completed_at).label("date"),
                func.sum(ReviewProgress.time_spent).label("total_time")
            )
            group_by = "date"
            order_by = "date"
            labels = []  # 动态生成
            
        query = (
            query.where(
                and_(
                    ReviewProgress.status == "completed",
                    ReviewProgress.completed_at >= datetime.now() - timedelta(days=30)
                )
            )
            .group_by(group_by)
            .order_by(order_by)
        )
        
        result = await self.db.execute(query)
        rows = result.all()
        
        if distribution_type == "month":
            labels = [row.date.strftime("%m-%d") for row in rows]
        
        values = [round(row.total_time / 60, 1) for row in rows]  # 转换为分钟
        
        return {
            "labels": labels,
            "values": values
        } 