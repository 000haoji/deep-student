"""
回顾分析服务业务逻辑
提供深度分析、学习模式识别、进度跟踪等功能
"""
import json
import uuid # 导入 uuid 模块
from typing import List, Optional, Dict, Any
from datetime import datetime, date, timedelta
from collections import defaultdict, Counter
from sqlalchemy import select, func, and_, or_
from sqlalchemy.ext.asyncio import AsyncSession

from shared.utils.logger import LoggerMixin
# from services.ai_api_manager.service import AIModelService # No longer needed here
from services.ai_api_manager.models import TaskType # Use models.AIRequest
from services.ai_api_manager.schemas import AIRequestSchema as AIRequest # Import Schema
from services.ai_api_manager.router import ai_router # Import global ai_router
from services.problem_service.models import Problem, Subject, ReviewRecord
from services.problem_service.service import ProblemService
from .models import ReviewAnalysis, AnalysisFollowUp, LearningPattern, AnalysisType, ReviewPlan, ReviewStage
from .schemas import (
    BatchAnalysisRequest, KnowledgePointAnalysisRequest,
    TimePeriodAnalysisRequest, ComprehensiveAnalysisRequest,
    FollowUpRequest # ReviewPlanCreate, ReviewPlanUpdate # Temporarily commented out
)


class ReviewService(LoggerMixin):
    """复习服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
        self.problem_service = ProblemService(db)
        # self._ai_service and ai_service property removed, will use global ai_router
    
    async def analyze_batch_problems(
        self,
        request: BatchAnalysisRequest
        # user_id: str # user_id REMOVED
    ) -> ReviewAnalysis:
        """批量分析错题"""
        # 获取题目信息
        problems = await self._get_problems_by_ids(request.problem_ids) # user_id removed
        if not problems:
            raise ValueError("No valid problems found or problem IDs are invalid.") # "for the current user" removed
        
        # 准备分析数据
        analysis_data = self._prepare_problems_data(problems)
        
        # 调用AI进行深度分析
        ai_request = AIRequest(
            task_type=TaskType.REVIEW_ANALYSIS,
            content={
                "problems": analysis_data,
                "focus_areas": request.focus_areas,
                "analysis_type": "batch_problems"
            }
        )
        
        ai_response = await ai_router.route_request(ai_request) # Use global ai_router
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 解析AI响应
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        # 创建分析记录
        analysis = ReviewAnalysis(
            # user_id=user_id, # user_id REMOVED from ReviewAnalysis model
            title=request.title or f"批量分析 - {len(problems)}道题目",
            analysis_type=AnalysisType.BATCH_PROBLEMS,
            description=request.description,
            problem_ids=[p.id for p in problems],
            subjects=list(set(p.subject for p in problems)),
            knowledge_points=self._extract_all_knowledge_points(problems),
            ai_analysis=ai_analysis,
            error_patterns=parsed_analysis.get("error_patterns", []),
            weakness_areas=parsed_analysis.get("weakness_areas", []),
            improvement_suggestions=parsed_analysis.get("suggestions", []),
            study_plan=parsed_analysis.get("study_plan", []),
            total_problems=len(problems),
            avg_difficulty=sum(p.difficulty_level for p in problems) / len(problems),
            avg_mastery=sum(p.mastery_level for p in problems) / len(problems),
            common_mistakes=self._analyze_common_mistakes(problems),
            importance_score=parsed_analysis.get("importance_score", 5.0),
            urgency_score=parsed_analysis.get("urgency_score", 5.0)
        )
        
        self.db.add(analysis)
        await self.db.commit()
        await self.db.refresh(analysis)
        
        self.log_info(f"Created batch analysis: {analysis.id}")
        return analysis
    
    async def analyze_knowledge_points(
        self,
        request: KnowledgePointAnalysisRequest
        # user_id: str # user_id REMOVED
    ) -> ReviewAnalysis:
        """分析特定知识点"""
        # 获取包含这些知识点的题目
        problems = await self._get_problems_by_knowledge_points(
            knowledge_points=request.knowledge_points,
            # user_id=user_id, # user_id removed
            date_range=request.date_range
        )
        
        if len(problems) < request.min_problem_count:
            raise ValueError(
                f"Not enough problems found. Required: {request.min_problem_count}, "
                f"Found: {len(problems)}"
            )
        
        # 准备分析数据
        analysis_data = {
            "knowledge_points": request.knowledge_points,
            "problems": self._prepare_problems_data(problems),
            "focus": "knowledge_mastery"
        }
        
        # AI分析
        ai_request = AIRequest(
            task_type=TaskType.REVIEW_ANALYSIS,
            content=analysis_data
        )
        
        ai_response = await ai_router.route_request(ai_request) # Use global ai_router
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
            # user_id=user_id, # user_id REMOVED
            title=f"知识点分析 - {', '.join(request.knowledge_points[:3])}",
            analysis_type=AnalysisType.KNOWLEDGE_POINT,
            description=f"分析知识点: {', '.join(request.knowledge_points)}",
            problem_ids=[p.id for p in problems],
            subjects=list(set(p.subject for p in problems)),
            knowledge_points=request.knowledge_points,
            date_range=request.date_range,
            ai_analysis=ai_analysis,
            error_patterns=parsed_analysis.get("error_patterns", []),
            weakness_areas=parsed_analysis.get("weakness_areas", []),
            improvement_suggestions=parsed_analysis.get("suggestions", []),
            study_plan=parsed_analysis.get("study_plan", []),
            total_problems=len(problems),
            avg_difficulty=sum(p.difficulty_level for p in problems) / len(problems),
            avg_mastery=sum(p.mastery_level for p in problems) / len(problems),
            common_mistakes=self._analyze_common_mistakes(problems),
            importance_score=parsed_analysis.get("importance_score", 5.0),
            urgency_score=parsed_analysis.get("urgency_score", 5.0)
        )
        
        self.db.add(analysis)
        await self.db.commit()
        await self.db.refresh(analysis)
        
        return analysis
    
    async def analyze_time_period(
        self,
        request: TimePeriodAnalysisRequest
        # user_id: str # user_id REMOVED
    ) -> ReviewAnalysis:
        """分析时间段内的学习情况"""
        # 获取时间段内的题目
        problems = await self._get_problems_by_date_range(
            start_date=request.start_date,
            end_date=request.end_date,
            # user_id=user_id, # user_id removed
            subjects=request.subjects
        )
        
        if not problems:
            raise ValueError("No problems found in the specified time period")
        
        # 按时间分组统计
        time_stats = self._analyze_time_trends(problems, request.start_date, request.end_date)
        
        # AI分析
        ai_request = AIRequest(
            task_type=TaskType.REVIEW_ANALYSIS,
            content={
                "time_period": {
                    "start": request.start_date.isoformat(),
                    "end": request.end_date.isoformat()
                },
                "problems": self._prepare_problems_data(problems),
                "time_stats": time_stats,
                "analysis_type": "time_trend"
            }
        )
        
        ai_response = await ai_router.route_request(ai_request) # Use global ai_router
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
            # user_id=user_id, # user_id REMOVED
            title=f"时间段分析 {request.start_date} - {request.end_date}",
            analysis_type=AnalysisType.TIME_PERIOD,
            description=f"分析时间段内的学习进展和趋势",
            problem_ids=[p.id for p in problems],
            subjects=list(set(p.subject for p in problems)),
            knowledge_points=self._extract_all_knowledge_points(problems),
            date_range={
                "start": request.start_date.isoformat(),
                "end": request.end_date.isoformat()
            },
            ai_analysis=ai_analysis,
            error_patterns=parsed_analysis.get("error_patterns", []),
            weakness_areas=parsed_analysis.get("weakness_areas", []),
            improvement_suggestions=parsed_analysis.get("suggestions", []),
            study_plan=parsed_analysis.get("study_plan", []),
            total_problems=len(problems),
            avg_difficulty=sum(p.difficulty_level for p in problems) / len(problems),
            avg_mastery=sum(p.mastery_level for p in problems) / len(problems),
            common_mistakes=self._analyze_common_mistakes(problems),
            importance_score=parsed_analysis.get("importance_score", 5.0),
            urgency_score=parsed_analysis.get("urgency_score", 5.0)
        )
        
        self.db.add(analysis)
        await self.db.commit()
        await self.db.refresh(analysis)
        
        return analysis
    
    async def comprehensive_analysis(
        self,
        request: ComprehensiveAnalysisRequest
        # user_id: str # user_id REMOVED
    ) -> ReviewAnalysis:
        """综合分析"""
        # 构建查询条件
        query = select(Problem).where(
            Problem.deleted_at.is_(None)
            # Problem.user_id == user_id # user_id filter REMOVED
        )
        
        if not request.include_all_problems:
            # 只分析特定范围
            if request.subjects:
                query = query.where(Problem.subject.in_(request.subjects))
            
            if request.min_mastery is not None:
                query = query.where(Problem.mastery_level >= request.min_mastery)
            
            if request.max_mastery is not None:
                query = query.where(Problem.mastery_level <= request.max_mastery)
        
        result = await self.db.execute(query)
        problems = result.scalars().all()
        
        if not problems:
            raise ValueError("No problems found for analysis")
        
        # 全面分析
        comprehensive_data = {
            "total_problems": len(problems),
            "subjects_distribution": self._analyze_subject_distribution(problems),
            "difficulty_distribution": self._analyze_difficulty_distribution(problems),
            "mastery_distribution": self._analyze_mastery_distribution(problems),
            "knowledge_coverage": self._analyze_knowledge_coverage(problems),
            "learning_progress": await self._analyze_learning_progress(),
            "focus_on_weaknesses": request.focus_on_weaknesses
        }
        
        # AI综合分析
        ai_request = AIRequest(
            task_type=TaskType.REVIEW_ANALYSIS,
            content={
                "analysis_type": "comprehensive",
                "data": comprehensive_data,
                "sample_problems": self._prepare_problems_data(problems[:50])  # 样本
            }
        )
        
        ai_response = await ai_router.route_request(ai_request) # Use global ai_router
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
            # user_id=user_id, # user_id REMOVED
            title="综合学习分析报告",
            analysis_type=AnalysisType.COMPREHENSIVE,
            description="全面分析学习情况、进展和改进方向",
            problem_ids=[p.id for p in problems[:100]],  # 只存储前100个ID
            subjects=list(set(p.subject for p in problems)),
            knowledge_points=list(set(
                kp for p in problems 
                for kp in (p.knowledge_points or [])
            ))[:50],  # 前50个知识点
            ai_analysis=ai_analysis,
            error_patterns=parsed_analysis.get("error_patterns", []),
            weakness_areas=parsed_analysis.get("weakness_areas", []),
            improvement_suggestions=parsed_analysis.get("suggestions", []),
            study_plan=parsed_analysis.get("study_plan", []),
            total_problems=len(problems),
            avg_difficulty=sum(p.difficulty_level for p in problems) / len(problems),
            avg_mastery=sum(p.mastery_level for p in problems) / len(problems),
            common_mistakes=self._analyze_common_mistakes(problems),
            importance_score=parsed_analysis.get("importance_score", 8.0),
            urgency_score=parsed_analysis.get("urgency_score", 7.0)
        )
        
        self.db.add(analysis)
        await self.db.commit()
        await self.db.refresh(analysis)
        
        # 识别学习模式
        # await self._identify_learning_patterns(problems, analysis.id) # Temporarily comment out if LearningPattern is not fully defined or used
        
        return analysis

    async def get_analysis_service(self, analysis_id: str) -> Optional[ReviewAnalysis]: # user_id removed
        """获取单个分析详情""" # user_id validation removed
        try:
            analysis_uuid = uuid.UUID(analysis_id)
        except ValueError:
            self.log_warning(f"Invalid UUID format for analysis_id: {analysis_id}")
            return None
            
        result = await self.db.execute(
            select(ReviewAnalysis).where(
                and_(
                    ReviewAnalysis.id == analysis_uuid
                    # ReviewAnalysis.user_id == user_id # user_id filter REMOVED
                )
            )
        )
        return result.scalar_one_or_none()

    async def list_analyses_service(
        self,
        # user_id: str, # user_id REMOVED
        analysis_type: Optional[AnalysisType] = None,
        page: int = 1,
        size: int = 20,
        importance_min: Optional[float] = None,
        urgency_min: Optional[float] = None,
        sort_by: str = "created_at", # Default sort field
        sort_desc: bool = True      # Default sort order
    ) -> tuple[List[ReviewAnalysis], int]:
        """获取分析列表""" # user_id validation removed
        query = select(ReviewAnalysis) # user_id filter REMOVED

        if analysis_type:
            query = query.where(ReviewAnalysis.analysis_type == analysis_type)
        if importance_min is not None:
            query = query.where(ReviewAnalysis.importance_score >= importance_min)
        if urgency_min is not None:
            query = query.where(ReviewAnalysis.urgency_score >= urgency_min)

        # 计算总数
        count_query = select(func.count()).select_from(query.subquery())
        total = await self.db.scalar(count_query)

        # 排序
        sort_column_attr = getattr(ReviewAnalysis, sort_by, ReviewAnalysis.created_at)
        if sort_desc:
            query = query.order_by(desc(sort_column_attr))
        else:
            query = query.order_by(sort_column_attr)
        
        # 分页
        query = query.offset((page - 1) * size).limit(size)
        
        result = await self.db.execute(query)
        analyses = result.scalars().all()
        
        return analyses, total
    
    async def add_follow_up(
        self,
        request: FollowUpRequest
        # user_id: str # user_id REMOVED
    ) -> AnalysisFollowUp:
        """添加分析跟进记录"""
        # 验证分析是否存在
        analysis = await self.get_analysis_service(request.analysis_id) # user_id removed
        if not analysis:
            raise ValueError("Analysis not found.") # "or user does not have permission" removed
        
        # 创建跟进记录
        follow_up = AnalysisFollowUp(
            analysis_id=analysis.id,
            action_taken=request.action_taken,
            result=request.result,
            effectiveness_score=request.effectiveness_score,
            reviewed_problem_ids=request.reviewed_problem_ids,
            notes=request.notes
        )
        
        # 如果有复习的题目，计算新的掌握度
        if request.reviewed_problem_ids:
            new_mastery_levels = {}
            for problem_id in request.reviewed_problem_ids:
                problem = await self.db.get(Problem, problem_id)
                if problem:
                    new_mastery_levels[str(problem_id)] = problem.mastery_level
            
            follow_up.new_mastery_levels = new_mastery_levels
        
        self.db.add(follow_up)
        await self.db.commit()
        await self.db.refresh(follow_up)
        
        return follow_up
    
    async def get_learning_insights(self) -> List[Dict[str, Any]]: # user_id removed
        """获取学习洞察""" # "(用户特定)" removed
        insights = []
        
        # 1. 进步趋势
        progress_insight = await self._analyze_progress_trend() # user_id removed
        if progress_insight:
            insights.append(progress_insight)
        
        # 2. 异常检测
        anomaly_insights = await self._detect_anomalies() # user_id removed
        insights.extend(anomaly_insights)
        
        # 3. 里程碑
        milestone_insights = await self._check_milestones() # user_id removed
        insights.extend(milestone_insights)
        
        # 4. 建议
        recommendation_insights = await self._generate_recommendations() # user_id removed
        insights.extend(recommendation_insights)
        
        # 按重要性排序
        insights.sort(key=lambda x: x.get("importance", 0), reverse=True)
        
        return insights
    
    # 辅助方法
    async def _get_problems_by_ids(self, problem_ids: List[str]) -> List[Problem]: # user_id removed
        """根据ID获取题目""" # user_id validation removed
        # 注意：problem_ids 可能是字符串形式的UUID，需要转换为UUID对象进行查询
        problem_ids_as_uuid = []
        for pid_str in problem_ids:
            try:
                problem_ids_as_uuid.append(uuid.UUID(pid_str))
            except ValueError:
                self.log_warning(f"Invalid UUID format for problem ID: {pid_str}")
                # 根据策略，可以选择忽略无效ID或引发错误
        
        if not problem_ids_as_uuid:
            return [] # 如果没有有效的UUID，则返回空列表

        result = await self.db.execute(
            select(Problem).where(
                and_(
                    Problem.id.in_(problem_ids_as_uuid), # 使用转换后的UUID列表
                    # Problem.user_id == user_id, # user_id filter REMOVED
                    Problem.deleted_at.is_(None)
                )
            )
        )
        return result.scalars().all()
    
    async def _get_problems_by_knowledge_points(
        self,
        knowledge_points: List[str],
        # user_id: str, # user_id REMOVED
        date_range: Optional[Dict[str, date]] = None
    ) -> List[Problem]:
        """根据知识点获取题目"""
        query = select(Problem).where(
            Problem.deleted_at.is_(None)
            # Problem.user_id == user_id # user_id filter REMOVED
        )
        
        # 知识点过滤
        kp_conditions = []
        for kp in knowledge_points:
            kp_conditions.append(
                func.json_contains(Problem.knowledge_points, f'"{kp}"')
            )
        query = query.where(or_(*kp_conditions))
        
        # 时间范围过滤
        if date_range:
            if "start" in date_range:
                query = query.where(Problem.created_at >= date_range["start"])
            if "end" in date_range:
                query = query.where(Problem.created_at <= date_range["end"])
        
        result = await self.db.execute(query)
        return result.scalars().all()
    
    async def _get_problems_by_date_range(
        self,
        start_date: date,
        end_date: date,
        # user_id: str, # user_id REMOVED
        subjects: Optional[List[str]] = None
    ) -> List[Problem]:
        """根据时间范围获取题目"""
        query = select(Problem).where(
            and_(
                Problem.created_at >= start_date,
                Problem.created_at <= end_date,
                # Problem.user_id == user_id, # user_id filter REMOVED
                Problem.deleted_at.is_(None)
            )
        )
        
        if subjects:
            query = query.where(Problem.subject.in_(subjects))
        
        result = await self.db.execute(query)
        return result.scalars().all()
    
    def _prepare_problems_data(self, problems: List[Problem]) -> List[Dict[str, Any]]:
        """准备题目数据供AI分析"""
        return [
            {
                "id": str(p.id),
                "subject": p.subject,
                "category": p.category,
                "difficulty": p.difficulty_level,
                "mastery": p.mastery_level,
                "knowledge_points": p.knowledge_points or [],
                "error_analysis": p.error_analysis,
                "review_count": p.review_count,
                "last_review": p.last_review_at.isoformat() if p.last_review_at else None
            }
            for p in problems
        ]
    
    def _parse_ai_analysis(self, ai_analysis: Any) -> Dict[str, Any]:
        """解析AI分析结果"""
        if isinstance(ai_analysis, str):
            try:
                return json.loads(ai_analysis)
            except:
                return {"raw_analysis": ai_analysis}
        
        return ai_analysis if isinstance(ai_analysis, dict) else {}
    
    def _extract_all_knowledge_points(self, problems: List[Problem]) -> List[str]:
        """提取所有知识点"""
        all_kps = []
        for p in problems:
            if p.knowledge_points:
                all_kps.extend(p.knowledge_points)
        
        # 去重并返回
        return list(set(all_kps))
    
    def _analyze_common_mistakes(self, problems: List[Problem]) -> Dict[str, int]:
        """分析常见错误"""
        mistakes = Counter()
        
        for p in problems:
            if p.error_analysis:
                # 简单的错误类型提取（实际应该更智能）
                if "计算" in p.error_analysis:
                    mistakes["计算错误"] += 1
                if "概念" in p.error_analysis:
                    mistakes["概念错误"] += 1
                if "理解" in p.error_analysis:
                    mistakes["理解错误"] += 1
                if "粗心" in p.error_analysis:
                    mistakes["粗心大意"] += 1
        
        return dict(mistakes)
    
    def _analyze_time_trends(
        self,
        problems: List[Problem],
        start_date: date,
        end_date: date
    ) -> Dict[str, Any]:
        """分析时间趋势"""
        # 按周分组
        weekly_stats = defaultdict(lambda: {
            "count": 0,
            "total_mastery": 0,
            "total_difficulty": 0
        })
        
        for p in problems:
            week_key = p.created_at.date().isocalendar()[:2]  # (year, week)
            weekly_stats[week_key]["count"] += 1
            weekly_stats[week_key]["total_mastery"] += p.mastery_level
            weekly_stats[week_key]["total_difficulty"] += p.difficulty_level
        
        # 计算每周平均值
        trend_data = []
        for week, stats in sorted(weekly_stats.items()):
            trend_data.append({
                "week": f"{week[0]}-W{week[1]}",
                "count": stats["count"],
                "avg_mastery": stats["total_mastery"] / stats["count"],
                "avg_difficulty": stats["total_difficulty"] / stats["count"]
            })
        
        return {
            "weekly_trends": trend_data,
            "total_weeks": len(trend_data)
        }
    
    def _analyze_subject_distribution(self, problems: List[Problem]) -> Dict[str, int]:
        """分析学科分布"""
        distribution = Counter(p.subject for p in problems)
        return dict(distribution)
    
    def _analyze_difficulty_distribution(self, problems: List[Problem]) -> Dict[int, int]:
        """分析难度分布"""
        distribution = Counter(p.difficulty_level for p in problems)
        return dict(distribution)
    
    def _analyze_mastery_distribution(self, problems: List[Problem]) -> Dict[str, int]:
        """分析掌握度分布"""
        ranges = {
            "low": 0,      # 0-0.3
            "medium": 0,   # 0.3-0.7
            "high": 0      # 0.7-1.0
        }
        
        for p in problems:
            if p.mastery_level < 0.3:
                ranges["low"] += 1
            elif p.mastery_level < 0.7:
                ranges["medium"] += 1
            else:
                ranges["high"] += 1
        
        return ranges
    
    def _analyze_knowledge_coverage(self, problems: List[Problem]) -> Dict[str, Any]:
        """分析知识点覆盖"""
        kp_stats = defaultdict(lambda: {"count": 0, "total_mastery": 0})
        
        for p in problems:
            if p.knowledge_points:
                for kp in p.knowledge_points:
                    kp_stats[kp]["count"] += 1
                    kp_stats[kp]["total_mastery"] += p.mastery_level
        
        # 转换为列表并排序
        coverage = []
        for kp, stats in kp_stats.items():
            coverage.append({
                "knowledge_point": kp,
                "count": stats["count"],
                "avg_mastery": stats["total_mastery"] / stats["count"]
            })
        
        coverage.sort(key=lambda x: x["count"], reverse=True)
        
        return {
            "total_knowledge_points": len(coverage),
            "top_knowledge_points": coverage[:10]
        }
    
    async def _analyze_learning_progress(self) -> Dict[str, Any]: # user_id removed
        """分析学习进展""" # "(用户特定)" removed
        # 获取最近30天的数据
        end_date = datetime.now().date()
        start_date = end_date - timedelta(days=30)
        
        # 查询统计数据 (用户特定)
        result = await self.db.execute(
            select(
                func.date(Problem.created_at).label("date"),
                func.count(Problem.id).label("count"),
                func.avg(Problem.mastery_level).label("avg_mastery")
            ).where(
                and_(
                    Problem.created_at >= start_date,
                    # Problem.user_id == user_id, # Filter by user_id REMOVED
                    Problem.deleted_at.is_(None)
                )
            ).group_by(func.date(Problem.created_at))
        )
        
        daily_stats = result.all()
        
        return {
            "days_analyzed": 30,
            "total_problems": sum(stat.count for stat in daily_stats),
            "avg_daily_problems": len(daily_stats) / 30 if daily_stats else 0,
            "mastery_trend": "improving" if daily_stats else "stable"  # 简化判断
        }
    
    async def _identify_learning_patterns(
        self,
        problems: List[Problem],
        analysis_id: str
    ) -> None:
        """识别学习模式"""
        # 这里可以实现更复杂的模式识别算法
        patterns = []
        
        # 示例：识别薄弱时间段
        time_patterns = self._identify_time_patterns(problems)
        patterns.extend(time_patterns)
        
        # 示例：识别知识点关联
        kp_patterns = self._identify_knowledge_patterns(problems)
        patterns.extend(kp_patterns)
        
        # 保存识别的模式
        for pattern_data in patterns:
            pattern = LearningPattern(**pattern_data)
            self.db.add(pattern)
        
        await self.db.commit()
    
    def _identify_time_patterns(self, problems: List[Problem]) -> List[Dict[str, Any]]:
        """识别时间相关的学习模式"""
        # 简化实现
        return []
    
    def _identify_knowledge_patterns(self, problems: List[Problem]) -> List[Dict[str, Any]]:
        """识别知识点相关的模式"""
        # 简化实现
        return []
    
    async def _analyze_progress_trend(self) -> Optional[Dict[str, Any]]: # user_id removed
        """分析进步趋势""" # "(用户特定)" removed
        # 获取最近30天的学习进展数据
        learning_progress_data = await self._analyze_learning_progress() # user_id removed
        
        # 简化实现：如果最近有题目且平均掌握度高于某个阈值，则认为进步明显
        # 在实际应用中，这里会进行更复杂的时间序列分析或比较分析
        avg_daily_problems = learning_progress_data.get("avg_daily_problems", 0)
        # mastery_trend_indicator = learning_progress_data.get("mastery_trend", "stable") # Placeholder

        # This is a very simplified example.
        # A real trend analysis would look at changes over time.
        # For now, let's assume if they did some problems recently, it's a positive sign.
        # The `_analyze_learning_progress` already calculates `mastery_trend` in a simplified way.

        problems_last_30_days_count = await self.db.scalar(
            select(func.count(Problem.id)).where(
                # Problem.user_id == user_id, # user_id filter REMOVED
                Problem.created_at >= (datetime.now().date() - timedelta(days=30)),
                Problem.deleted_at.is_(None)
            )
        )
        
        if problems_last_30_days_count > 5: # Arbitrary threshold
            return {
                "insight_type": "trend",
                "title": "近期学习活跃",
                "description": f"过去30天内记录了 {problems_last_30_days_count} 道题目，请继续保持！", # "您在" removed
                "supporting_data": {"problems_last_30_days": problems_last_30_days_count},
                "importance": 7.0,
                "actionable": True,
                "recommended_actions": ["制定学习计划，保持学习节奏", "针对近期错题进行复盘"]
            }
        return None
    
    async def _detect_anomalies(self) -> List[Dict[str, Any]]: # user_id removed
        """检测异常""" # "(用户特定)" removed
        # 示例：检测在某个知识点上掌握度突然下降
        anomalies = []
        # Query for problems where mastery dropped significantly after a review
        # This is a complex query and requires tracking mastery changes over time or per review.
        # For simplification, this part remains a placeholder.
        # Example: "知识点 '微积分初步' 掌握度近期有明显波动，建议重点复习。"
        return anomalies
    
    async def _check_milestones(self) -> List[Dict[str, Any]]: # user_id removed
        """检查里程碑""" # "(用户特定)" removed
        milestones = []
        # 示例：解决了超过100道题
        total_problems_count = await self.db.scalar(
            select(func.count(Problem.id)).where(Problem.deleted_at.is_(None)) # user_id filter REMOVED
        )
        if total_problems_count >= 100:
            milestones.append({
                "insight_type": "milestone",
                "title": "成就达成：累计解决100题！",
                "description": "已累计解决超过100道题目，继续努力！", # "恭喜您" removed
                "supporting_data": {"total_problems": total_problems_count},
                "importance": 8.0,
                "actionable": False
            })
        # Add more milestones: e.g., "数学学科平均掌握度达到80%"
        return milestones
    
    async def _generate_recommendations(self) -> List[Dict[str, Any]]: # user_id removed
        """生成建议""" # "(用户特定)" removed
        recommendations = []
        # 示例：如果在某些知识点上掌握度较低，则生成复习建议
        low_mastery_problems = await self.db.execute(
            select(Problem.knowledge_points, func.avg(Problem.mastery_level).label("avg_kp_mastery"))
            .where(Problem.mastery_level < 0.5, Problem.deleted_at.is_(None), Problem.knowledge_points.isnot(None)) # user_id filter REMOVED
            .group_by(Problem.knowledge_points) # This group by might be tricky if knowledge_points is JSON array
            .limit(5) # Get top 5 knowledge areas with low mastery
        )
        # The above query is a bit complex due to JSON array.
        # A simpler approach for recommendation: find problems with low mastery and suggest reviewing them.
        
        problems_to_review_query = select(Problem).where(
            # Problem.user_id == user_id, # user_id filter REMOVED
            Problem.mastery_level < 0.6, # Threshold for "needs review"
            Problem.deleted_at.is_(None)
        ).order_by(Problem.last_review_at.asc().nulls_first(), Problem.created_at.desc()).limit(3) # Prioritize older or never reviewed

        problems_to_review_results = await self.db.execute(problems_to_review_query)
        problems_to_review = problems_to_review_results.scalars().all()

        if problems_to_review:
            problem_titles = [p.title for p in problems_to_review]
            recommendations.append({
                "insight_type": "recommendation",
                "title": "复习建议",
                "description": f"在以下题目上掌握度较低，建议优先复习：{', '.join(problem_titles)}", # "您" removed
                "supporting_data": {"problem_ids_to_review": [str(p.id) for p in problems_to_review]},
                "importance": 7.5,
                "actionable": True,
                "recommended_actions": ["将这些题目加入复习计划", "重新学习相关知识点"]
            })
        return recommendations
    
    # async def create_review_plan(
    #     self,
    #     data: ReviewPlanCreate # This schema is not defined yet
    # ) -> ReviewPlan:
    #     """创建复习计划"""
    #     try:
    #         # 获取需要复习的题目
    #         # user_id related logic removed
    #         problems, _ = await self.problem_service.list_problems(
    #             # user_id=user_id_for_problems, # user_id REMOVED
    #             subject=data.subject,
    #             category=data.category,
    #             tags=data.tags,
    #             mastery_range=(0, 0.8)  # 掌握度低于80%的题目
    #         )
            
    #         if not problems:
    #             raise ValueError("No problems found for review for the specified criteria.") # "for the specified user" removed
            
    #         # 创建复习计划
    #         plan = ReviewPlan(
    #             id=str(uuid.uuid4()), # Ensure ID is generated if model expects it
    #             # user_id=user_id_for_problems, # user_id REMOVED from ReviewPlan model
    #             title=data.title,
    #             subject=data.subject,
    #             category=data.category,
    #             tags=data.tags,
    #             start_date=data.start_date,
    #             end_date=data.end_date,
    #             total_problems=len(problems)
    #         )
            
    #         self.db.add(plan)
    #         await self.db.commit()
    #         await self.db.refresh(plan)
            
    #         # 生成复习阶段
    #         await self._generate_review_stages(plan, problems)
            
    #         self.log_info(f"Created review plan: {plan.id}")
    #         return plan
            
    #     except Exception as e:
    #         self.log_error(f"Failed to create review plan: {e}")
    #         await self.db.rollback()
    #         raise
    
    async def _generate_review_stages(
        self,
        plan: ReviewPlan,
        problems: List[Problem]
    ) -> None:
        """生成复习阶段"""
        # 艾宾浩斯遗忘曲线复习间隔（小时）
        review_intervals = [1, 24, 72, 168, 360, 720]
        
        # 计算每个阶段的复习时间
        current_time = plan.start_date
        for interval in review_intervals:
            stage_time = current_time + timedelta(hours=interval)
            if stage_time > plan.end_date:
                break
                
            # 创建复习阶段
            stage = ReviewStage(
                plan_id=plan.id,
                scheduled_time=stage_time,
                interval_hours=interval,
                status="pending"
            )
            
            self.db.add(stage)
        
        await self.db.commit()
    
    async def adjust_review_plan(
        self,
        plan_id: str,
        user_performance: Dict[str, float]
    ) -> ReviewPlan:
        """动态调整复习计划"""
        plan = await self.get_review_plan(plan_id)
        if not plan:
            raise ValueError("Review plan not found")
        
        try:
            # 获取最近的复习记录
            recent_records = await self._get_recent_review_records(plan_id)
            
            # 分析用户表现
            performance_analysis = self._analyze_performance(recent_records, user_performance)
            
            # 调整复习间隔
            await self._adjust_review_intervals(plan, performance_analysis)
            
            # 更新计划状态
            plan.last_adjusted = datetime.now()
            plan.adjustment_history.append({
                "timestamp": datetime.now().isoformat(),
                "analysis": performance_analysis
            })
            
            await self.db.commit()
            await self.db.refresh(plan)
            
            self.log_info(f"Adjusted review plan: {plan_id}")
            return plan
            
        except Exception as e:
            self.log_error(f"Failed to adjust review plan: {e}")
            await self.db.rollback()
            raise
    
    async def _get_recent_review_records(
        self,
        plan_id: str,
        days: int = 7
    ) -> List[ReviewRecord]:
        """获取最近的复习记录"""
        cutoff_time = datetime.now() - timedelta(days=days)
        
        result = await self.db.execute(
            select(ReviewRecord)
            .where(
                and_(
                    ReviewRecord.plan_id == plan_id,
                    ReviewRecord.created_at >= cutoff_time
                )
            )
            .order_by(ReviewRecord.created_at.desc())
        )
        
        return result.scalars().all()
    
    def _analyze_performance(
        self,
        records: List[ReviewRecord],
        user_performance: Dict[str, float]
    ) -> Dict[str, Any]:
        """分析用户表现"""
        if not records:
            return {"status": "insufficient_data"}
        
        # 计算平均掌握度
        avg_mastery = sum(r.mastery_level for r in records) / len(records)
        
        # 分析错误模式
        error_patterns = {}
        for record in records:
            if record.error_analysis:
                pattern = record.error_analysis.get("pattern", "unknown")
                error_patterns[pattern] = error_patterns.get(pattern, 0) + 1
        
        # 分析学习速度
        completion_times = []
        for i in range(1, len(records)):
            time_diff = (records[i-1].created_at - records[i].created_at).total_seconds()
            completion_times.append(time_diff)
        
        avg_completion_time = sum(completion_times) / len(completion_times) if completion_times else 0
        
        return {
            "status": "success",
            "avg_mastery": avg_mastery,
            "error_patterns": error_patterns,
            "avg_completion_time": avg_completion_time,
            "user_performance": user_performance
        }
    
    async def _adjust_review_intervals(
        self,
        plan: ReviewPlan,
        performance: Dict[str, Any]
    ) -> None:
        """调整复习间隔"""
        if performance["status"] != "success":
            return
        
        # 获取所有待处理的复习阶段
        result = await self.db.execute(
            select(ReviewStage)
            .where(
                and_(
                    ReviewStage.plan_id == plan.id,
                    ReviewStage.status == "pending"
                )
            )
            .order_by(ReviewStage.scheduled_time)
        )
        
        stages = result.scalars().all()
        
        # 根据表现调整间隔
        avg_mastery = performance["avg_mastery"]
        current_time = datetime.now()
        
        for stage in stages:
            if stage.scheduled_time <= current_time:
                continue
                
            # 根据掌握度调整间隔
            if avg_mastery > 0.8:  # 掌握度好，增加间隔
                new_interval = stage.interval_hours * 1.5
            elif avg_mastery < 0.5:  # 掌握度差，减少间隔
                new_interval = stage.interval_hours * 0.75
            else:
                new_interval = stage.interval_hours
            
            # 更新阶段时间
            stage.interval_hours = new_interval
            stage.scheduled_time = current_time + timedelta(hours=new_interval)
            
            # 记录调整原因
            stage.adjustment_history.append({
                "timestamp": datetime.now().isoformat(),
                "reason": f"Based on mastery level: {avg_mastery:.2f}",
                "old_interval": stage.interval_hours,
                "new_interval": new_interval
            })
        
        await self.db.commit()
    
    async def get_review_plan(self, plan_id: str) -> Optional[ReviewPlan]:
        """获取复习计划"""
        result = await self.db.execute(
            select(ReviewPlan).where(ReviewPlan.id == plan_id)
        )
        return result.scalar_one_or_none()
