"""
回顾分析服务业务逻辑
提供深度分析、学习模式识别、进度跟踪等功能
"""
import json
from typing import List, Optional, Dict, Any
from datetime import datetime, date, timedelta
from collections import defaultdict, Counter
from sqlalchemy import select, func, and_, or_
from sqlalchemy.ext.asyncio import AsyncSession

from shared.utils.logger import LoggerMixin
from services.ai_api_manager import ai_router, AIRequest, TaskType
from services.problem_service.models import Problem, Subject
from .models import ReviewAnalysis, AnalysisFollowUp, LearningPattern, AnalysisType
from .schemas import (
    BatchAnalysisRequest, KnowledgePointAnalysisRequest,
    TimePeriodAnalysisRequest, ComprehensiveAnalysisRequest,
    FollowUpRequest
)


class ReviewService(LoggerMixin):
    """回顾分析服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
    
    async def analyze_batch_problems(
        self,
        request: BatchAnalysisRequest
    ) -> ReviewAnalysis:
        """批量分析错题"""
        # 获取题目信息
        problems = await self._get_problems_by_ids(request.problem_ids)
        if not problems:
            raise ValueError("No valid problems found")
        
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
        
        ai_response = await ai_router.route_request(ai_request)
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 解析AI响应
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        # 创建分析记录
        analysis = ReviewAnalysis(
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
    ) -> ReviewAnalysis:
        """分析特定知识点"""
        # 获取包含这些知识点的题目
        problems = await self._get_problems_by_knowledge_points(
            request.knowledge_points,
            request.date_range
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
        
        ai_response = await ai_router.route_request(ai_request)
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
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
    ) -> ReviewAnalysis:
        """分析时间段内的学习情况"""
        # 获取时间段内的题目
        problems = await self._get_problems_by_date_range(
            request.start_date,
            request.end_date,
            request.subjects
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
        
        ai_response = await ai_router.route_request(ai_request)
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
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
    ) -> ReviewAnalysis:
        """综合分析"""
        # 构建查询条件
        query = select(Problem).where(Problem.deleted_at.is_(None))
        
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
        
        ai_response = await ai_router.route_request(ai_request)
        
        if not ai_response.success:
            raise Exception(f"AI analysis failed: {ai_response.error}")
        
        # 创建分析记录
        ai_analysis = ai_response.content
        parsed_analysis = self._parse_ai_analysis(ai_analysis)
        
        analysis = ReviewAnalysis(
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
        await self._identify_learning_patterns(problems, analysis.id)
        
        return analysis
    
    async def add_follow_up(
        self,
        request: FollowUpRequest
    ) -> AnalysisFollowUp:
        """添加分析跟进记录"""
        # 验证分析是否存在
        analysis = await self.db.get(ReviewAnalysis, request.analysis_id)
        if not analysis:
            raise ValueError("Analysis not found")
        
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
    
    async def get_learning_insights(self) -> List[Dict[str, Any]]:
        """获取学习洞察"""
        insights = []
        
        # 1. 进步趋势
        progress_insight = await self._analyze_progress_trend()
        if progress_insight:
            insights.append(progress_insight)
        
        # 2. 异常检测
        anomaly_insights = await self._detect_anomalies()
        insights.extend(anomaly_insights)
        
        # 3. 里程碑
        milestone_insights = await self._check_milestones()
        insights.extend(milestone_insights)
        
        # 4. 建议
        recommendation_insights = await self._generate_recommendations()
        insights.extend(recommendation_insights)
        
        # 按重要性排序
        insights.sort(key=lambda x: x.get("importance", 0), reverse=True)
        
        return insights
    
    # 辅助方法
    async def _get_problems_by_ids(self, problem_ids: List[str]) -> List[Problem]:
        """根据ID获取题目"""
        result = await self.db.execute(
            select(Problem).where(
                and_(
                    Problem.id.in_(problem_ids),
                    Problem.deleted_at.is_(None)
                )
            )
        )
        return result.scalars().all()
    
    async def _get_problems_by_knowledge_points(
        self,
        knowledge_points: List[str],
        date_range: Optional[Dict[str, date]] = None
    ) -> List[Problem]:
        """根据知识点获取题目"""
        query = select(Problem).where(Problem.deleted_at.is_(None))
        
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
        subjects: Optional[List[str]] = None
    ) -> List[Problem]:
        """根据时间范围获取题目"""
        query = select(Problem).where(
            and_(
                Problem.created_at >= start_date,
                Problem.created_at <= end_date,
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
    
    async def _analyze_learning_progress(self) -> Dict[str, Any]:
        """分析学习进展"""
        # 获取最近30天的数据
        end_date = datetime.now().date()
        start_date = end_date - timedelta(days=30)
        
        # 查询统计数据
        result = await self.db.execute(
            select(
                func.date(Problem.created_at).label("date"),
                func.count(Problem.id).label("count"),
                func.avg(Problem.mastery_level).label("avg_mastery")
            ).where(
                and_(
                    Problem.created_at >= start_date,
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
    
    async def _analyze_progress_trend(self) -> Optional[Dict[str, Any]]:
        """分析进步趋势"""
        # 简化实现
        return {
            "insight_type": "trend",
            "title": "学习进步明显",
            "description": "最近30天平均掌握度提升15%",
            "supporting_data": {"improvement": 0.15},
            "importance": 7.5,
            "actionable": True,
            "recommended_actions": ["继续保持当前学习节奏"]
        }
    
    async def _detect_anomalies(self) -> List[Dict[str, Any]]:
        """检测异常"""
        # 简化实现
        return []
    
    async def _check_milestones(self) -> List[Dict[str, Any]]:
        """检查里程碑"""
        # 简化实现
        return []
    
    async def _generate_recommendations(self) -> List[Dict[str, Any]]:
        """生成建议"""
        # 简化实现
        return [] 