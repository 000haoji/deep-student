"""
错题管理系统 FastAPI 主应用 - 本地版本（无认证）
"""
from fastapi import FastAPI, HTTPException, Depends
from fastapi.middleware.cors import CORSMiddleware
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from typing import List, Optional
from datetime import datetime
from pydantic import BaseModel
import uuid

from shared.database import get_db, engine, Base
from shared.config import settings
from shared.utils.logger import get_logger

# 导入模型
from services.problem_service.models import Problem, Subject
from services.review_service.models import ReviewAnalysis, AnalysisType
from services.ai_api_manager.api import router as ai_router, ai_router as ai_router_instance
from services.ai_api_manager.models import AIProvider, AICapability, TaskType, AIModel

logger = get_logger(__name__)

# 创建FastAPI应用
app = FastAPI(
    title="错题管理系统 API - 本地版",
    description="考研错题管理与分析系统（无需登录）",
    version="2.0.0"
)

# CORS配置
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# 注册AI API路由
app.include_router(ai_router)

# 固定的用户ID（本地版本使用）
LOCAL_USER_ID = "00000000-0000-0000-0000-000000000001"


# Pydantic模型
class ProblemCreate(BaseModel):
    title: str
    content: str
    subject: str
    category: Optional[str] = None
    user_answer: Optional[str] = None
    correct_answer: Optional[str] = None
    error_analysis: Optional[str] = None
    tags: List[str] = []


class ProblemUpdate(BaseModel):
    title: Optional[str] = None
    content: Optional[str] = None
    category: Optional[str] = None
    user_answer: Optional[str] = None
    correct_answer: Optional[str] = None
    error_analysis: Optional[str] = None
    solution: Optional[str] = None
    tags: Optional[List[str]] = None
    notes: Optional[str] = None


class ProblemResponse(BaseModel):
    id: str
    title: str
    content: str
    subject: str
    category: Optional[str]
    user_answer: Optional[str]
    correct_answer: Optional[str]
    error_analysis: Optional[str]
    solution: Optional[str]
    knowledge_points: List[str]
    difficulty_level: int
    review_count: int
    mastery_level: float
    tags: List[str]
    created_at: datetime
    updated_at: datetime


class AnalysisCreate(BaseModel):
    title: str
    problem_ids: List[str]
    analysis_type: str = "comprehensive"


class AnalysisResponse(BaseModel):
    id: str
    title: str
    analysis_type: str
    problem_ids: List[str]
    total_problems: int
    error_patterns: Optional[List[dict]]
    weakness_areas: Optional[List[dict]]
    improvement_suggestions: Optional[List[str]]
    study_plan: Optional[List[dict]]
    created_at: datetime


# API端点
@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API v2.0 - 本地版",
        "docs": "/docs",
        "status": "running",
        "note": "本地版本无需登录"
    }


@app.post("/problems", response_model=ProblemResponse)
async def create_problem(
    problem_data: ProblemCreate,
    db: AsyncSession = Depends(get_db)
):
    """创建错题"""
    try:
        subject = Subject(problem_data.subject)
    except ValueError:
        raise HTTPException(status_code=400, detail="无效的学科类型")
    
    problem = Problem(
        title=problem_data.title,
        content=problem_data.content,
        subject=subject,
        category=problem_data.category,
        user_answer=problem_data.user_answer,
        correct_answer=problem_data.correct_answer,
        error_analysis=problem_data.error_analysis,
        tags=problem_data.tags
    )
    
    # 默认值
    problem.knowledge_points = []
    problem.difficulty_level = 3
    problem.ai_analysis = {}
    
    db.add(problem)
    await db.commit()
    await db.refresh(problem)
    
    return ProblemResponse(
        id=str(problem.id),
        title=problem.title,
        content=problem.content,
        subject=problem.subject,
        category=problem.category,
        user_answer=problem.user_answer,
        correct_answer=problem.correct_answer,
        error_analysis=problem.error_analysis,
        solution=problem.solution,
        knowledge_points=problem.knowledge_points,
        difficulty_level=problem.difficulty_level,
        review_count=problem.review_count,
        mastery_level=problem.mastery_level,
        tags=problem.tags,
        created_at=problem.created_at,
        updated_at=problem.updated_at
    )


@app.get("/problems", response_model=List[ProblemResponse])
async def get_problems(
    subject: Optional[str] = None,
    category: Optional[str] = None,
    skip: int = 0,
    limit: int = 20,
    db: AsyncSession = Depends(get_db)
):
    """获取错题列表"""
    query = select(Problem)
    
    if subject:
        query = query.where(Problem.subject == subject)
    if category:
        query = query.where(Problem.category == category)
    
    query = query.offset(skip).limit(limit).order_by(Problem.created_at.desc())
    
    result = await db.execute(query)
    problems = result.scalars().all()
    
    return [
        ProblemResponse(
            id=str(problem.id),
            title=problem.title,
            content=problem.content,
            subject=problem.subject,
            category=problem.category,
            user_answer=problem.user_answer,
            correct_answer=problem.correct_answer,
            error_analysis=problem.error_analysis,
            solution=problem.solution,
            knowledge_points=problem.knowledge_points or [],
            difficulty_level=problem.difficulty_level,
            review_count=problem.review_count,
            mastery_level=problem.mastery_level,
            tags=problem.tags or [],
            created_at=problem.created_at,
            updated_at=problem.updated_at
        )
        for problem in problems
    ]


@app.get("/problems/{problem_id}", response_model=ProblemResponse)
async def get_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """获取单个错题详情"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return ProblemResponse(
        id=str(problem.id),
        title=problem.title,
        content=problem.content,
        subject=problem.subject,
        category=problem.category,
        user_answer=problem.user_answer,
        correct_answer=problem.correct_answer,
        error_analysis=problem.error_analysis,
        solution=problem.solution,
        knowledge_points=problem.knowledge_points or [],
        difficulty_level=problem.difficulty_level,
        review_count=problem.review_count,
        mastery_level=problem.mastery_level,
        tags=problem.tags or [],
        created_at=problem.created_at,
        updated_at=problem.updated_at
    )


@app.put("/problems/{problem_id}", response_model=ProblemResponse)
async def update_problem(
    problem_id: str,
    problem_update: ProblemUpdate,
    db: AsyncSession = Depends(get_db)
):
    """更新错题"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    update_data = problem_update.dict(exclude_unset=True)
    for field, value in update_data.items():
        setattr(problem, field, value)
    
    await db.commit()
    await db.refresh(problem)
    
    return ProblemResponse(
        id=str(problem.id),
        title=problem.title,
        content=problem.content,
        subject=problem.subject,
        category=problem.category,
        user_answer=problem.user_answer,
        correct_answer=problem.correct_answer,
        error_analysis=problem.error_analysis,
        solution=problem.solution,
        knowledge_points=problem.knowledge_points or [],
        difficulty_level=problem.difficulty_level,
        review_count=problem.review_count,
        mastery_level=problem.mastery_level,
        tags=problem.tags or [],
        created_at=problem.created_at,
        updated_at=problem.updated_at
    )


@app.delete("/problems/{problem_id}")
async def delete_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """删除错题"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    await db.delete(problem)
    await db.commit()
    
    return {"message": "错题已删除"}


@app.get("/statistics")
async def get_statistics(
    db: AsyncSession = Depends(get_db)
):
    """获取统计信息"""
    problem_count = await db.scalar(
        select(func.count(Problem.id))
    )
    
    subject_stats = await db.execute(
        select(
            Problem.subject,
            func.count(Problem.id).label("count"),
            func.avg(Problem.mastery_level).label("avg_mastery")
        ).group_by(Problem.subject)
    )
    
    subject_data = [
        {
            "subject": row.subject,
            "count": row.count,
            "avg_mastery": float(row.avg_mastery or 0)
        }
        for row in subject_stats
    ]
    
    return {
        "total_problems": problem_count or 0,
        "subject_statistics": subject_data,
        "total_analyses": 0,
        "recent_reviews": []
    }


# 简化的分析API
@app.get("/analyses")
async def get_analyses():
    """获取分析列表（暂时返回空）"""
    return []


@app.post("/analyses")
async def create_analysis(data: AnalysisCreate):
    """创建分析（暂时返回模拟数据）"""
    return {
        "id": str(uuid.uuid4()),
        "title": data.title,
        "analysis_type": data.analysis_type,
        "problem_ids": data.problem_ids,
        "total_problems": len(data.problem_ids),
        "error_patterns": [],
        "weakness_areas": [],
        "improvement_suggestions": ["功能开发中"],
        "study_plan": [],
        "created_at": datetime.now()
    }


@app.post("/test/create-default-ai-model")
async def create_default_ai_model(
    db: AsyncSession = Depends(get_db)
):
    """创建默认的AI模型配置（用于测试）"""
    # 检查是否已经存在
    result = await db.execute(
        select(AIModel).where(
            AIModel.provider == AIProvider.OPENAI,
            AIModel.model_name == "gpt-3.5-turbo"
        )
    )
    existing = result.scalar_one_or_none()
    
    if existing:
        return {"message": "默认AI模型已存在", "model_id": str(existing.id)}
    
    # 创建默认的OpenAI模型
    model = AIModel(
        provider=AIProvider.OPENAI,
        model_name="gpt-3.5-turbo",
        api_key_encrypted="sk-your-api-key-here",  # 实际使用时需要替换
        api_url="https://api.openai.com/v1",
        priority=1,
        is_active=True,
        capabilities=[AICapability.TEXT.value],
        cost_per_1k_tokens=0.002,
        max_tokens=4096,
        timeout=30,
        max_retries=3
    )
    
    db.add(model)
    await db.commit()
    await db.refresh(model)
    
    return {
        "message": "默认AI模型创建成功",
        "model_id": str(model.id),
        "note": "请通过API更新真实的API密钥"
    }


# 启动事件
@app.on_event("startup")
async def startup_event():
    """应用启动事件"""
    logger.info("Starting up the application (Local version - No Auth)...")
    
    # 创建数据库表（如果不存在）
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    
    # 初始化AI路由器
    async with AsyncSession(engine) as session:
        await ai_router_instance.initialize(session)
    
    logger.info("Application started successfully!")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000) 