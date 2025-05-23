"""
错题管理系统 FastAPI 主应用
"""
from fastapi import FastAPI, HTTPException, Depends, status
from fastapi.middleware.cors import CORSMiddleware
from fastapi.security import OAuth2PasswordBearer, OAuth2PasswordRequestForm
from sqlalchemy.ext.asyncio import AsyncSession
from sqlalchemy import select, func
from typing import List, Optional
from datetime import datetime, timedelta
from jose import jwt
from pydantic import BaseModel
import uuid

from shared.database import get_db, engine, Base
from shared.config import settings
from shared.utils.logger import get_logger

# 导入模型
from services.user_service.models import User, UserRole
from services.problem_service.models import Problem, Subject
from services.ai_api_manager.models import AIModel, AIProvider
from services.review_service.models import ReviewAnalysis, AnalysisType

logger = get_logger(__name__)

# 创建FastAPI应用
app = FastAPI(
    title="错题管理系统 API",
    description="考研错题管理与分析系统",
    version="2.0.0"
)

# CORS配置
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # 生产环境应该限制具体域名
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# OAuth2配置
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

# JWT配置
SECRET_KEY = "your-secret-key-here"  # 实际应该从环境变量读取
ALGORITHM = "HS256"
ACCESS_TOKEN_EXPIRE_MINUTES = 30


# Pydantic模型
class UserCreate(BaseModel):
    username: str
    email: str
    password: str
    full_name: str


class UserLogin(BaseModel):
    username: str
    password: str


class UserResponse(BaseModel):
    id: str
    username: str
    email: str
    full_name: str
    role: str
    is_active: bool


class Token(BaseModel):
    access_token: str
    token_type: str
    user: UserResponse


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


# 工具函数
def create_access_token(data: dict):
    to_encode = data.copy()
    expire = datetime.utcnow() + timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    to_encode.update({"exp": expire})
    encoded_jwt = jwt.encode(to_encode, SECRET_KEY, algorithm=ALGORITHM)
    return encoded_jwt


async def get_current_user(token: str = Depends(oauth2_scheme), db: AsyncSession = Depends(get_db)):
    credentials_exception = HTTPException(
        status_code=status.HTTP_401_UNAUTHORIZED,
        detail="Could not validate credentials",
        headers={"WWW-Authenticate": "Bearer"},
    )
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        user_id: str = payload.get("sub")
        if user_id is None:
            raise credentials_exception
    except jwt.PyJWTError:
        raise credentials_exception
    
    user = await db.get(User, user_id)
    if user is None:
        raise credentials_exception
    return user


# API端点

@app.get("/")
async def root():
    """根路径"""
    return {
        "message": "错题管理系统 API v2.0",
        "docs": "/docs",
        "status": "running"
    }


@app.post("/register", response_model=UserResponse)
async def register(user_data: UserCreate, db: AsyncSession = Depends(get_db)):
    """用户注册"""
    # 检查用户名是否存在
    result = await db.execute(
        select(User).where(User.username == user_data.username)
    )
    if result.scalar_one_or_none():
        raise HTTPException(status_code=400, detail="用户名已存在")
    
    # 检查邮箱是否存在
    result = await db.execute(
        select(User).where(User.email == user_data.email)
    )
    if result.scalar_one_or_none():
        raise HTTPException(status_code=400, detail="邮箱已被注册")
    
    # 创建新用户
    user = User(
        username=user_data.username,
        email=user_data.email,
        full_name=user_data.full_name,
        role=UserRole.STUDENT
    )
    user.set_password(user_data.password)
    
    db.add(user)
    await db.commit()
    await db.refresh(user)
    
    return UserResponse(
        id=str(user.id),
        username=user.username,
        email=user.email,
        full_name=user.full_name,
        role=user.role,
        is_active=user.is_active
    )


@app.post("/token", response_model=Token)
async def login(form_data: OAuth2PasswordRequestForm = Depends(), db: AsyncSession = Depends(get_db)):
    """用户登录"""
    result = await db.execute(
        select(User).where(User.username == form_data.username)
    )
    user = result.scalar_one_or_none()
    
    if not user or not user.check_password(form_data.password):
        raise HTTPException(
            status_code=status.HTTP_401_UNAUTHORIZED,
            detail="用户名或密码错误",
            headers={"WWW-Authenticate": "Bearer"},
        )
    
    # 更新登录信息
    user.update_login_info()
    await db.commit()
    
    # 创建访问令牌
    access_token = create_access_token(data={"sub": str(user.id)})
    
    return Token(
        access_token=access_token,
        token_type="bearer",
        user=UserResponse(
            id=str(user.id),
            username=user.username,
            email=user.email,
            full_name=user.full_name,
            role=user.role,
            is_active=user.is_active
        )
    )


@app.get("/me", response_model=UserResponse)
async def get_current_user_info(current_user: User = Depends(get_current_user)):
    """获取当前用户信息"""
    return UserResponse(
        id=str(current_user.id),
        username=current_user.username,
        email=current_user.email,
        full_name=current_user.full_name,
        role=current_user.role,
        is_active=current_user.is_active
    )


@app.post("/problems", response_model=ProblemResponse)
async def create_problem(
    problem_data: ProblemCreate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """创建错题"""
    # 验证学科
    try:
        subject = Subject(problem_data.subject)
    except ValueError:
        raise HTTPException(status_code=400, detail="无效的学科类型")
    
    # 创建错题
    problem = Problem(
        title=problem_data.title,
        content=problem_data.content,
        subject=subject,
        category=problem_data.category,
        user_answer=problem_data.user_answer,
        correct_answer=problem_data.correct_answer,
        error_analysis=problem_data.error_analysis,
        tags=problem_data.tags,
        user_id=current_user.id
    )
    
    # 模拟AI分析
    problem.knowledge_points = ["知识点1", "知识点2", "知识点3"]
    problem.difficulty_level = 3
    problem.ai_analysis = {
        "error_type": "概念理解不足",
        "suggestions": ["建议复习相关概念", "多做类似练习"]
    }
    
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
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """获取错题列表"""
    query = select(Problem).where(Problem.user_id == current_user.id)
    
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
            knowledge_points=problem.knowledge_points,
            difficulty_level=problem.difficulty_level,
            review_count=problem.review_count,
            mastery_level=problem.mastery_level,
            tags=problem.tags,
            created_at=problem.created_at,
            updated_at=problem.updated_at
        )
        for problem in problems
    ]


@app.get("/problems/{problem_id}", response_model=ProblemResponse)
async def get_problem(
    problem_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """获取单个错题详情"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    if problem.user_id != current_user.id:
        raise HTTPException(status_code=403, detail="无权访问此错题")
    
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


@app.put("/problems/{problem_id}", response_model=ProblemResponse)
async def update_problem(
    problem_id: str,
    problem_update: ProblemUpdate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """更新错题"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    if problem.user_id != current_user.id:
        raise HTTPException(status_code=403, detail="无权修改此错题")
    
    # 更新字段
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
        knowledge_points=problem.knowledge_points,
        difficulty_level=problem.difficulty_level,
        review_count=problem.review_count,
        mastery_level=problem.mastery_level,
        tags=problem.tags,
        created_at=problem.created_at,
        updated_at=problem.updated_at
    )


@app.delete("/problems/{problem_id}")
async def delete_problem(
    problem_id: str,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """删除错题"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    if problem.user_id != current_user.id:
        raise HTTPException(status_code=403, detail="无权删除此错题")
    
    await db.delete(problem)
    await db.commit()
    
    return {"message": "错题已删除"}


@app.post("/problems/{problem_id}/review")
async def review_problem(
    problem_id: str,
    mastery_level: float,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """复习错题"""
    problem = await db.get(Problem, problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    if problem.user_id != current_user.id:
        raise HTTPException(status_code=403, detail="无权复习此错题")
    
    # 更新复习信息
    problem.review_count += 1
    problem.last_review_at = datetime.utcnow()
    problem.mastery_level = mastery_level
    
    await db.commit()
    
    return {"message": "复习记录已更新", "review_count": problem.review_count}


@app.post("/analyses", response_model=AnalysisResponse)
async def create_analysis(
    analysis_data: AnalysisCreate,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """创建分析"""
    # 验证题目ID
    problem_ids = [uuid.UUID(pid) for pid in analysis_data.problem_ids]
    
    # 查询题目
    result = await db.execute(
        select(Problem).where(
            Problem.id.in_(problem_ids),
            Problem.user_id == current_user.id
        )
    )
    problems = result.scalars().all()
    
    if len(problems) != len(problem_ids):
        raise HTTPException(status_code=400, detail="部分题目不存在或无权访问")
    
    # 创建分析
    analysis = ReviewAnalysis(
        title=analysis_data.title,
        analysis_type=AnalysisType(analysis_data.analysis_type),
        problem_ids=[str(pid) for pid in problem_ids],
        total_problems=len(problems),
        user_id=current_user.id
    )
    
    # 模拟AI分析结果
    analysis.error_patterns = [
        {"pattern": "概念理解不足", "frequency": 0.6, "severity": 8},
        {"pattern": "计算错误", "frequency": 0.3, "severity": 5}
    ]
    analysis.weakness_areas = [
        {"area": "微积分", "mastery": 0.4, "priority": 9},
        {"area": "线性代数", "mastery": 0.6, "priority": 7}
    ]
    analysis.improvement_suggestions = [
        "加强基础概念理解",
        "多做练习题",
        "建立错题本"
    ]
    analysis.study_plan = [
        {"topic": "微积分复习", "hours": 5, "deadline": "2024-01-15"},
        {"topic": "线性代数练习", "hours": 3, "deadline": "2024-01-20"}
    ]
    
    db.add(analysis)
    await db.commit()
    await db.refresh(analysis)
    
    return AnalysisResponse(
        id=str(analysis.id),
        title=analysis.title,
        analysis_type=analysis.analysis_type,
        problem_ids=analysis.problem_ids,
        total_problems=analysis.total_problems,
        error_patterns=analysis.error_patterns,
        weakness_areas=analysis.weakness_areas,
        improvement_suggestions=analysis.improvement_suggestions,
        study_plan=analysis.study_plan,
        created_at=analysis.created_at
    )


@app.get("/analyses", response_model=List[AnalysisResponse])
async def get_analyses(
    skip: int = 0,
    limit: int = 20,
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """获取分析列表"""
    query = select(ReviewAnalysis).where(
        ReviewAnalysis.user_id == current_user.id
    ).offset(skip).limit(limit).order_by(ReviewAnalysis.created_at.desc())
    
    result = await db.execute(query)
    analyses = result.scalars().all()
    
    return [
        AnalysisResponse(
            id=str(analysis.id),
            title=analysis.title,
            analysis_type=analysis.analysis_type,
            problem_ids=analysis.problem_ids,
            total_problems=analysis.total_problems,
            error_patterns=analysis.error_patterns,
            weakness_areas=analysis.weakness_areas,
            improvement_suggestions=analysis.improvement_suggestions,
            study_plan=analysis.study_plan,
            created_at=analysis.created_at
        )
        for analysis in analyses
    ]


@app.get("/statistics")
async def get_statistics(
    current_user: User = Depends(get_current_user),
    db: AsyncSession = Depends(get_db)
):
    """获取统计信息"""
    # 题目统计
    problem_count = await db.scalar(
        select(func.count(Problem.id)).where(Problem.user_id == current_user.id)
    )
    
    # 按学科统计
    subject_stats = await db.execute(
        select(
            Problem.subject,
            func.count(Problem.id).label("count"),
            func.avg(Problem.mastery_level).label("avg_mastery")
        ).where(
            Problem.user_id == current_user.id
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
    
    # 分析统计
    analysis_count = await db.scalar(
        select(func.count(ReviewAnalysis.id)).where(
            ReviewAnalysis.user_id == current_user.id
        )
    )
    
    # 最近复习
    recent_reviews = await db.execute(
        select(Problem).where(
            Problem.user_id == current_user.id,
            Problem.last_review_at.isnot(None)
        ).order_by(
            Problem.last_review_at.desc()
        ).limit(5)
    )
    
    recent_review_data = [
        {
            "id": str(p.id),
            "title": p.title,
            "subject": p.subject,
            "last_review_at": p.last_review_at
        }
        for p in recent_reviews.scalars().all()
    ]
    
    return {
        "total_problems": problem_count,
        "subject_statistics": subject_data,
        "total_analyses": analysis_count,
        "recent_reviews": recent_review_data
    }


# 启动事件
@app.on_event("startup")
async def startup_event():
    """应用启动事件"""
    logger.info("Starting up the application...")
    
    # 创建数据库表（如果不存在）
    async with engine.begin() as conn:
        await conn.run_sync(Base.metadata.create_all)
    
    logger.info("Application started successfully!")


@app.on_event("shutdown")
async def shutdown_event():
    """应用关闭事件"""
    logger.info("Shutting down the application...")


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000) 