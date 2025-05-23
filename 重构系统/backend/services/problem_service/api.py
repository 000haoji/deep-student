"""
错题管理服务的API端点
"""
from typing import List, Optional
from uuid import UUID
from fastapi import APIRouter, Depends, HTTPException, Query, File, UploadFile
from sqlalchemy.ext.asyncio import AsyncSession

from shared.database import get_db
from shared.utils.logger import get_logger
from .models import Subject, Problem
from .schemas import (
    ProblemCreate,
    ProblemUpdate,
    ProblemResponse,
    ProblemListResponse,
    ProblemAnalyzeRequest,
    ProblemOCRRequest,
    ReviewRecordCreate,
    ReviewRecordResponse
)
from .service import ProblemService

logger = get_logger(__name__)
router = APIRouter(prefix="/api/v1/problems", tags=["Problems"])


@router.post("", response_model=ProblemResponse)
async def create_problem(
    data: ProblemCreate,
    auto_analyze: bool = True,
    db: AsyncSession = Depends(get_db)
):
    """创建新的错题"""
    service = ProblemService(db)
    problem = await service.create_problem(data, auto_analyze)
    
    return ProblemResponse(
        success=True,
        message="错题创建成功",
        data=problem.to_dict()
    )


@router.get("", response_model=ProblemListResponse)
async def list_problems(
    subject: Optional[Subject] = None,
    category: Optional[str] = None,
    tags: Optional[List[str]] = Query(None),
    keyword: Optional[str] = None,
    min_difficulty: Optional[int] = Query(None, ge=1, le=5),
    max_difficulty: Optional[int] = Query(None, ge=1, le=5),
    min_mastery: Optional[float] = Query(None, ge=0, le=1),
    max_mastery: Optional[float] = Query(None, ge=0, le=1),
    page: int = Query(1, ge=1),
    size: int = Query(20, ge=1, le=100),
    sort_by: str = Query("created_at", pattern="^(created_at|updated_at|difficulty_level|mastery_level|review_count)$"),
    sort_desc: bool = True,
    db: AsyncSession = Depends(get_db)
):
    """获取错题列表"""
    service = ProblemService(db)
    
    # 构建难度和掌握度范围
    difficulty_range = None
    if min_difficulty or max_difficulty:
        difficulty_range = (min_difficulty or 1, max_difficulty or 5)
    
    mastery_range = None
    if min_mastery is not None or max_mastery is not None:
        mastery_range = (min_mastery or 0.0, max_mastery or 1.0)
    
    problems, total = await service.list_problems(
        subject=subject,
        category=category,
        tags=tags,
        keyword=keyword,
        difficulty_range=difficulty_range,
        mastery_range=mastery_range,
        page=page,
        size=size,
        sort_by=sort_by,
        sort_desc=sort_desc
    )
    
    return ProblemListResponse(
        success=True,
        data=[p.to_dict() for p in problems],
        total=total,
        page=page,
        size=size
    )


@router.get("/{problem_id}", response_model=ProblemResponse)
async def get_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """获取单个错题详情"""
    service = ProblemService(db)
    problem = await service.get_problem(problem_id)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return ProblemResponse(
        success=True,
        data=problem.to_dict()
    )


@router.put("/{problem_id}", response_model=ProblemResponse)
async def update_problem(
    problem_id: str,
    data: ProblemUpdate,
    db: AsyncSession = Depends(get_db)
):
    """更新错题信息"""
    service = ProblemService(db)
    problem = await service.update_problem(problem_id, data)
    
    if not problem:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return ProblemResponse(
        success=True,
        message="错题更新成功",
        data=problem.to_dict()
    )


@router.delete("/{problem_id}", response_model=dict)
async def delete_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """删除错题（软删除）"""
    service = ProblemService(db)
    success = await service.delete_problem(problem_id)
    
    if not success:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return {
        "success": True,
        "message": "错题删除成功"
    }


@router.post("/{problem_id}/analyze", response_model=dict)
async def analyze_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """AI分析错题"""
    service = ProblemService(db)
    
    try:
        analysis = await service.analyze_problem(problem_id)
        return {
            "success": True,
            "message": "分析完成",
            "data": analysis
        }
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        logger.error(f"分析错题失败: {e}")
        raise HTTPException(status_code=500, detail="分析失败")


@router.post("/ocr", response_model=dict)
async def ocr_image(
    data: ProblemOCRRequest,
    db: AsyncSession = Depends(get_db)
):
    """OCR识别图片内容"""
    service = ProblemService(db)
    
    try:
        result = await service.ocr_image(data)
        return {
            "success": True,
            "message": "OCR识别完成",
            "data": result
        }
    except Exception as e:
        logger.error(f"OCR识别失败: {e}")
        raise HTTPException(status_code=500, detail="OCR识别失败")


@router.post("/{problem_id}/reviews", response_model=ReviewRecordResponse)
async def add_review_record(
    problem_id: str,
    data: ReviewRecordCreate,
    db: AsyncSession = Depends(get_db)
):
    """添加复习记录"""
    service = ProblemService(db)
    
    # 确保problem_id一致
    data.problem_id = problem_id
    
    try:
        record = await service.add_review_record(data)
        return ReviewRecordResponse(
            success=True,
            message="复习记录添加成功",
            data={
                "id": str(record.id),
                "problem_id": str(record.problem_id),
                "review_result": record.review_result,
                "confidence_level": record.confidence_level,
                "time_spent": record.time_spent,
                "notes": record.notes,
                "created_at": record.created_at.isoformat()
            }
        )
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))


@router.get("/stats/overview", response_model=dict)
async def get_statistics(
    db: AsyncSession = Depends(get_db)
):
    """获取错题统计信息"""
    service = ProblemService(db)
    stats = await service.get_statistics()
    
    return {
        "success": True,
        "data": stats
    }


@router.get("/stats/knowledge-points", response_model=dict)
async def get_knowledge_point_stats(
    db: AsyncSession = Depends(get_db)
):
    """获取知识点统计"""
    service = ProblemService(db)
    stats = await service.get_knowledge_point_stats()
    
    return {
        "success": True,
        "data": stats
    }


@router.post("/batch", response_model=dict)
async def batch_import(
    problems: List[ProblemCreate],
    auto_analyze: bool = False,
    db: AsyncSession = Depends(get_db)
):
    """批量导入错题"""
    service = ProblemService(db)
    result = await service.batch_import(problems, auto_analyze)
    
    return {
        "success": True,
        "message": f"批量导入完成，成功{result['success']}个，失败{result['failed']}个",
        "data": result
    }


@router.post("/upload/image", response_model=dict)
async def upload_problem_image(
    file: UploadFile = File(...),
    auto_ocr: bool = True,
    db: AsyncSession = Depends(get_db)
):
    """上传错题图片"""
    # 验证文件类型
    allowed_types = ["image/jpeg", "image/png", "image/webp"]
    if file.content_type not in allowed_types:
        raise HTTPException(
            status_code=400,
            detail=f"不支持的文件类型: {file.content_type}"
        )
    
    # 读取文件内容
    content = await file.read()
    
    # TODO: 保存文件到存储服务
    # TODO: 如果auto_ocr=True，执行OCR识别
    
    return {
        "success": True,
        "message": "图片上传成功",
        "data": {
            "filename": file.filename,
            "size": len(content),
            "content_type": file.content_type
        }
    } 