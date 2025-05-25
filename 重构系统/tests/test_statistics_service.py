"""
统计服务自动化测试
"""
import pytest
from datetime import datetime, timedelta
from sqlalchemy.ext.asyncio import AsyncSession
from backend.services.statistics_service.service import StatisticsService
from backend.services.statistics_service.schemas import StatisticsRequest
from backend.services.problem_service.models import Problem, Subject
from backend.services.review_service.models import ReviewProgress, ReviewPlan
@pytest.fixture
async def test_data(db: AsyncSession):
    """准备测试数据"""
    # 创建测试题目
    problems = []
    for i in range(10):
        problem = Problem(
            title=f"测试题目{i}",
            subject=Subject.MATH,
            difficulty_level=i % 5 + 1,
            mastery_level=i * 0.1,
            knowledge_points=["知识点1", "知识点2"],
            created_at=datetime.now() - timedelta(days=i)
        )
        db.add(problem)
        problems.append(problem)
    
    # 创建复习计划
    plan = ReviewPlan(
        title="测试计划",
        subject=Subject.MATH,
        start_date=datetime.now() - timedelta(days=30),
        end_date=datetime.now() + timedelta(days=30),
        total_problems=10
    )
    db.add(plan)
    
    # 创建复习进度
    for i, problem in enumerate(problems):
        progress = ReviewProgress(
            plan_id=plan.id,
            problem_id=problem.id,
            status="completed",
            completed_at=datetime.now() - timedelta(days=i),
            time_spent=300,  # 5分钟
            mastery_level=i * 0.1,
            error_analysis={"计算错误": 1} if i % 2 == 0 else {"概念错误": 1}
        )
        db.add(progress)
    
    await db.commit()
    return problems, plan

@pytest.mark.asyncio
async def test_get_problem_statistics(db: AsyncSession, test_data):
    """测试获取错题统计"""
    service = StatisticsService(db)
    request = StatisticsRequest(
        timeRange="month",
        subject="math",
        timeDistributionType="week"
    )
    
    result = await service.get_problem_statistics(request)
    assert result["success"]
    data = result["data"]
    
    # 验证基础统计
    assert data["totalProblems"] == 10
    assert data["pendingReview"] > 0
    assert 0 <= data["averageMastery"] <= 100
    assert data["newThisWeek"] >= 0
    
    # 验证知识点分布
    assert len(data["knowledgePointDistribution"]) > 0
    assert all("name" in item and "value" in item 
              for item in data["knowledgePointDistribution"])
    
    # 验证难度分布
    assert len(data["difficultyDistribution"]) == 5
    assert sum(data["difficultyDistribution"]) == 10
    
    # 验证掌握度趋势
    assert "dates" in data["masteryTrend"]
    assert "values" in data["masteryTrend"]
    assert len(data["masteryTrend"]["dates"]) == len(data["masteryTrend"]["values"])
    
    # 验证错误模式
    assert len(data["errorPatterns"]) > 0
    assert all("name" in item and "value" in item 
              for item in data["errorPatterns"])
    
    # 验证时间分布
    assert "labels" in data["timeDistribution"]
    assert "values" in data["timeDistribution"]
    assert len(data["timeDistribution"]["labels"]) == len(data["timeDistribution"]["values"])

@pytest.mark.asyncio
async def test_get_overview_statistics(db: AsyncSession, test_data):
    """测试获取概览统计"""
    service = StatisticsService(db)
    overview = await service._get_overview_statistics()
    
    assert overview["total_problems"] == 10
    assert overview["pending_review"] > 0
    assert 0 <= overview["average_mastery"] <= 100
    assert overview["new_this_week"] >= 0

@pytest.mark.asyncio
async def test_get_knowledge_point_distribution(db: AsyncSession, test_data):
    """测试获取知识点分布"""
    service = StatisticsService(db)
    
    # 测试不同时间范围
    for time_range in ["week", "month", "year"]:
        distribution = await service._get_knowledge_point_distribution(time_range)
        assert len(distribution) > 0
        assert all("name" in item and "value" in item for item in distribution)

@pytest.mark.asyncio
async def test_get_difficulty_distribution(db: AsyncSession, test_data):
    """测试获取难度分布"""
    service = StatisticsService(db)
    
    # 测试所有科目
    distribution = await service._get_difficulty_distribution()
    assert len(distribution) == 5
    assert sum(distribution) == 10
    
    # 测试特定科目
    math_distribution = await service._get_difficulty_distribution("math")
    assert len(math_distribution) == 5
    assert sum(math_distribution) == 10

@pytest.mark.asyncio
async def test_get_mastery_trend(db: AsyncSession, test_data):
    """测试获取掌握度趋势"""
    service = StatisticsService(db)
    
    # 测试默认时间范围
    trend = await service._get_mastery_trend()
    assert "dates" in trend
    assert "values" in trend
    assert len(trend["dates"]) == len(trend["values"])
    
    # 测试特定时间范围
    date_range = [
        datetime.now() - timedelta(days=7),
        datetime.now()
    ]
    trend = await service._get_mastery_trend(date_range=date_range)
    assert len(trend["dates"]) <= 7

@pytest.mark.asyncio
async def test_get_error_patterns(db: AsyncSession, test_data):
    """测试获取错误模式"""
    service = StatisticsService(db)
    
    # 测试所有科目
    patterns = await service._get_error_patterns()
    assert len(patterns) > 0
    assert all("name" in item and "value" in item for item in patterns)
    
    # 测试特定科目
    math_patterns = await service._get_error_patterns("math")
    assert len(math_patterns) > 0

@pytest.mark.asyncio
async def test_get_time_distribution(db: AsyncSession, test_data):
    """测试获取时间分布"""
    service = StatisticsService(db)
    
    # 测试不同类型的时间分布
    for dist_type in ["day", "week", "month"]:
        distribution = await service._get_time_distribution(dist_type)
        assert "labels" in distribution
        assert "values" in distribution
        assert len(distribution["labels"]) == len(distribution["values"])
        
        if dist_type == "day":
            assert len(distribution["labels"]) <= 24
        elif dist_type == "week":
            assert len(distribution["labels"]) == 7
        else:
            assert len(distribution["labels"]) <= 30 