"""
错题管理服务的API端点
"""
from typing import List, Optional, Dict, Any # Added Dict, Any
from uuid import UUID
import json # For SSE streaming
from fastapi import APIRouter, Depends, HTTPException, Query, File, UploadFile
from sqlalchemy.ext.asyncio import AsyncSession
from pydantic import BaseModel, Field # For new response model & User model placeholder, Added Field
from shared.database import get_db
from shared.utils.logger import get_logger
from .models import Subject, Problem
from .schemas import (
    ProblemCreate,
    ProblemUpdate,
    ProblemData,             # Changed from ProblemResponse
    SingleProblemResponse,   # New wrapper for single problem
    ProblemListResponse,
    ProblemAnalyzeRequest,
    ProblemOCRRequest,
    ReviewRecordCreate,
    ReviewRecordResponse,
    ProblemStatisticsResponse,
    KnowledgePointStatsListResponse,
    # Problem Tag Schemas
    ProblemTagCreate,
    ProblemTagUpdate,
    ProblemTagData,
    ProblemTagResponse,
    ProblemTagListResponse,
    # Problem Category Schemas
    ProblemCategoryCreate,
    ProblemCategoryUpdate,
    ProblemCategoryData,
    ProblemCategoryResponse,
    ProblemCategoryListResponse,
    # Batch Request Schema
    ProblemBatchRequest,
    # AI-Driven Workflow Schemas
    ProblemAICreateInitiateRequest,
    ProblemAICreateInitiateResponse,
    ProblemAIStreamRequest,
    ProblemAIFinalizeRequest,
    ProblemAIStructuredData, # Though this is part of InitiateResponse, good to have for type hints if needed
    # Schemas for AI Session and Chat History
    ProblemAISessionData,
    SingleProblemAISessionResponse,
    ProblemAISessionListResponse, # Added for listing sessions
    AIChatMessage # For chat history response
)
from .models import ProblemAISessionStatus # Added for status filter

# Custom response model for chat history
class AIChatHistoryResponse(BaseModel):
    success: bool = True
    message: Optional[str] = None
    data: List[AIChatMessage] = Field(default_factory=list)

from .service import ProblemService
from ..file_service.schemas import FileUploadResponse as StorageFileUploadResponse # For file storage info
from ..file_service.service import file_service as get_file_service_instance # Import the actual service instance, aliased to avoid conflict with potential local var
from ..ai_api_manager.service import AIModelService # For AI capabilities
from ..ai_api_manager.schemas import AIRequestSchema # For AI requests
from ..ai_api_manager.models import TaskType as AI_TaskType # Import TaskType directly from models

# pydantic.BaseModel is already imported
from fastapi.responses import JSONResponse, StreamingResponse # For export
import io # For CSV Streaming
import csv # For CSV generation
from enum import Enum as PyEnum # To avoid conflict with models.Subject
import uuid # For session_id generation

logger = get_logger(__name__)

# Router for traditional problem management
router = APIRouter(prefix="/api/v1/problems", tags=["Problems"])

# Router for AI-Driven Problem Creation
ai_problem_router = APIRouter(prefix="/api/v1/problems/ai-create", tags=["Problems - AI Driven Creation"])


# Dependency for AIModelService (assuming it might need DB for model configs)
async def get_ai_model_service(db: AsyncSession = Depends(get_db)) -> AIModelService:
    return AIModelService(db)

# Dependency for FileService (assuming it might need DB for file records)
async def get_file_service(db: AsyncSession = Depends(get_db)) -> get_file_service_instance:
    # FileService instance is directly imported and might not need db passed here if it's configured globally
    # However, if its methods that we call expect a db session, we should provide it.
    # For now, returning the imported instance. Revisit if its methods need db explicitly.
    # return get_file_service_instance # This would return the global instance
    return get_file_service_instance(db) # If FileService needs a session per request like other services


class ExportFormat(str, PyEnum):
    JSON = "json"
    CSV = "csv"

# New response model for image upload
class ProblemImageUploadResponse(BaseModel):
    success: bool
    message: str
    file_info: Optional[StorageFileUploadResponse] = None
    ocr_result: Optional[Dict[str, Any]] = None # To store result from problem_service.ocr_image


@router.post("", response_model=SingleProblemResponse)
async def create_problem(
    data: ProblemCreate, 
    auto_analyze: bool = Query(True, description="是否在创建后自动执行AI分析"),
    db: AsyncSession = Depends(get_db)
):
    """创建新的错题"""
    service = ProblemService(db)
    problem_orm_instance = await service.create_problem(data, auto_analyze=auto_analyze)
    
    return SingleProblemResponse(
        success=True,
        message="错题创建成功",
        data=ProblemData.from_orm(problem_orm_instance) # Convert ORM instance to Pydantic model
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
    
    problems_orm, total = await service.list_problems(
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
        data=[ProblemData.from_orm(p) for p in problems_orm], # Convert each ORM instance
        total=total,
        page=page,
        size=size
    )


@router.get("/{problem_id}", response_model=SingleProblemResponse)
async def get_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """获取单个错题详情"""
    service = ProblemService(db)
    problem_orm_instance = await service.get_problem_by_id(problem_id)
    
    if not problem_orm_instance:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return SingleProblemResponse(
        success=True,
        data=ProblemData.from_orm(problem_orm_instance) # Convert ORM instance
    )


@router.put("/{problem_id}", response_model=SingleProblemResponse)
async def update_problem(
    problem_id: str,
    data: ProblemUpdate,
    db: AsyncSession = Depends(get_db)
):
    """更新错题信息"""
    service = ProblemService(db)
    problem_orm_instance = await service.update_problem(problem_id, data=data)
    
    if not problem_orm_instance:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return SingleProblemResponse(
        success=True,
        message="错题更新成功",
        data=ProblemData.from_orm(problem_orm_instance) # Convert ORM instance
    )


@router.delete("/{problem_id}", response_model=SingleProblemResponse)
async def delete_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """删除错题（软删除）"""
    service = ProblemService(db)
    success = await service.delete_problem(problem_id)
    
    if not success:
        raise HTTPException(status_code=404, detail="错题不存在")
    
    return SingleProblemResponse(
        success=True,
        message="错题删除成功"
    )


@router.post("/{problem_id}/analyze", response_model=SingleProblemResponse)
async def analyze_problem(
    problem_id: str,
    db: AsyncSession = Depends(get_db)
):
    """AI分析错题"""
    service = ProblemService(db)
    
    # First, check if the problem exists
    problem_to_analyze = await service.get_problem_by_id(problem_id)
    if not problem_to_analyze:
        raise HTTPException(status_code=404, detail="错题不存在")

    try:
        analysis_result = await service.analyze_problem(problem_id)
        
        if analysis_result.get("status") == "success":
            # Re-fetch to get updated data including the new analysis
            updated_problem_orm = await service.get_problem_by_id(problem_id)
            return SingleProblemResponse(
                success=True,
                message="错题AI分析完成",
                data=ProblemData.from_orm(updated_problem_orm)
            )
        else: # AI analysis failed or had issues
            return SingleProblemResponse(
                success=False,
                message=analysis_result.get("message", "AI分析过程中发生错误"),
                data=ProblemData.from_orm(problem_to_analyze) # Return current problem state before analysis attempt
            )

    except ValueError as e: # Raised by service.get_problem if not found
        raise HTTPException(status_code=404, detail=str(e))
    except Exception as e:
        logger.error(f"AI分析错题 {problem_id} 失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"AI分析失败: {str(e)}")


@router.post("/ocr", response_model=SingleProblemResponse) 
async def ocr_image(
    data: ProblemOCRRequest, 
    db: AsyncSession = Depends(get_db)
):
    """
    OCR识别图片内容。
    如果auto_create=True且识别成功，会创建新题目。
    """
    service = ProblemService(db)
    
    try:
        ocr_service_result = await service.ocr_image(data)
        
        if ocr_service_result.get("created"):
            created_problem_id = ocr_service_result.get("problem_id")
            # Fetch the created problem to return its data
            created_problem_orm = await service.get_problem_by_id(created_problem_id)
            if created_problem_orm:
                return SingleProblemResponse(
                    success=True,
                    message=ocr_service_result.get("message", "OCR识别并创建题目成功"),
                    data=ProblemData.from_orm(created_problem_orm)
                )
            else: # Should not happen if creation was successful
                logger.error(f"OCR created problem {created_problem_id} but failed to retrieve it.")
                # Fallback to just OCR text
                return SingleProblemResponse(success=True, message="OCR识别成功，但获取创建的题目信息失败。", data={"ocr_text": ocr_service_result.get("text")})

        else: # OCR successful, but no problem created
            # We need a way to return just the OCR text, SingleProblemResponse.data expects ProblemData.
            # For now, returning it in message, or consider a different response model for OCR-only.
            return SingleProblemResponse(
                success=True,
                message=f"OCR识别完成: {ocr_service_result.get('text')}",
                # data field is for ProblemData, so cannot directly put OCR text here unless we change the model
                # A dedicated OCRResponse model would be better.
                # For now, using a generic dict in data for simplicity if no problem created.
                data = {"ocr_text": ocr_service_result.get("text")} if not ocr_service_result.get("created") else None
            )

    except Exception as e:
        logger.error(f"OCR识别失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"OCR识别失败: {str(e)}")


@router.post("/{problem_id}/reviews", response_model=ReviewRecordResponse)
async def add_review_record(
    problem_id: str,
    data: ReviewRecordCreate,
    db: AsyncSession = Depends(get_db)
):
    """添加复习记录"""
    service = ProblemService(db)

    # First, verify the problem exists
    problem_to_review = await service.get_problem_by_id(problem_id)
    if not problem_to_review:
        raise HTTPException(status_code=404, detail="错题不存在")
    
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


@router.get("/stats/overview", response_model=ProblemStatisticsResponse)
async def get_statistics(
    db: AsyncSession = Depends(get_db)
):
    """获取错题统计信息"""
    service = ProblemService(db)
    stats_data = await service.get_statistics()
    
    return ProblemStatisticsResponse(
        success=True,
        data=stats_data
    )


@router.get("/stats/knowledge-points", response_model=KnowledgePointStatsListResponse)
async def get_knowledge_point_stats(
    db: AsyncSession = Depends(get_db)
):
    """获取知识点统计"""
    service = ProblemService(db)
    stats_data = await service.get_knowledge_point_stats()
    
    return KnowledgePointStatsListResponse(
        success=True,
        data=stats_data
    )


@router.post("/batch", response_model=dict)
async def batch_import(
    problems: List[ProblemCreate],
    auto_analyze: bool = False,
    db: AsyncSession = Depends(get_db)
):
    """批量导入错题"""
    service = ProblemService(db)
    result = await service.batch_import(problems, auto_analyze=auto_analyze)
    
    return {
        "success": True,
        "message": f"批量导入完成，成功{result['success']}个，失败{result['failed']}个",
        "data": result
    }


@router.post("/upload/image", response_model=ProblemImageUploadResponse)
async def upload_problem_image(
    file: UploadFile = File(...),
    auto_ocr: bool = Query(True, description="是否在上传后自动执行OCR"),
    db: AsyncSession = Depends(get_db)
):
    """
    上传错题图片。
    图片将保存到文件服务，如果auto_ocr为True，则会尝试进行OCR识别。
    如果OCR后设置了自动创建题目，且成功，则会创建新题目。
    """
    service = ProblemService(db)
    file_info_response: Optional[StorageFileUploadResponse] = None
    ocr_data: Optional[Dict[str, Any]] = None

    # 1. 验证文件类型 (可选，file_service可能也会处理)
    allowed_types = ["image/jpeg", "image/png", "image/webp", "image/gif", "application/pdf"] # Added PDF for OCR
    if file.content_type not in allowed_types:
        raise HTTPException(
            status_code=400,
            detail=f"Unsupported file type: {file.content_type}. Allowed: {', '.join(allowed_types)}"
        )

    try:
        # 2. 读取文件内容
        content = await file.read()

        # 3. 保存文件到存储服务 (file_service)
        #    We need the FileRecord object or at least the access_url / object_name from file_service
        #    to pass to OCR if needed.
        #    The file_service.upload_image should ideally return a structure similar to its FileUploadResponse schema,
        #    or the FileRecord ORM object.
        #    Let's assume file_service.upload_image returns the access_url and we can query the record.
        
        # Using file_service.api.upload_image_endpoint logic as a reference for return type
        # This is a bit indirect. Ideally, we'd call a service method that returns structured data.
        
        uploaded_file_access_url = await file_service.upload_image(
            image_data=content,
            filename=file.filename,
            category="problem_images", # Specific category for problem images
            db=db, # Pass db session for FileRecord creation
            is_public=False # Or True, depending on policy
        )

        # Retrieve the full FileRecord from the database based on the access_url or object_name
        # This logic mirrors what's in file_service.api for now.
        # A more direct service method returning FileRecord would be cleaner.
        parts = uploaded_file_access_url.split('/')
        object_name_from_url = "/".join(parts[4:])
        
        from ..file_service.models import FileRecord as StorageFileRecord # Ensure correct import
        # stmt = select(Problem.FileRecord).where(Problem.FileRecord.object_name == object_name_from_url).order_by(Problem.FileRecord.created_at.desc()) # Incorrect reference
        # ^^^ Problem.FileRecord is likely wrong. It should be just FileRecord from file_service.models
        # Let's assume direct import of FileRecord model
        # from ..file_service.models import FileRecord as StorageFileRecord # Already imported above
        stmt_fs = select(StorageFileRecord).where(StorageFileRecord.object_name == object_name_from_url).order_by(StorageFileRecord.created_at.desc())

        result = await db.execute(stmt_fs)
        storage_file_record = result.scalars().first()

        if not storage_file_record:
            logger.error(f"Image uploaded to {uploaded_file_access_url} but failed to retrieve FileRecord from DB.")
            # Even if DB record fails, image might be in MinIO. Decide on error handling.
            # For now, let's proceed but file_info might be incomplete.
            # Or raise an error:
            raise HTTPException(status_code=500, detail="Image uploaded but DB record failed.")

        file_info_response = StorageFileUploadResponse.from_orm(storage_file_record)


        # 4. 如果auto_ocr=True，执行OCR识别
        if auto_ocr:
            ocr_request = ProblemOCRRequest(
                # image_base64=base64.b64encode(content).decode('utf-8'), # Option 1: send base64 directly
                image_url=uploaded_file_access_url, # Option 2: send URL, service.ocr_image will handle download
                enhance_math=True, # Default or make configurable
                auto_create=True  # Default or make configurable
                # user_id is not part of ProblemOCRRequest anymore
            )
            try:
                ocr_data = await service.ocr_image(ocr_request)
            except Exception as ocr_exc:
                logger.error(f"OCR failed for uploaded image {file.filename}: {ocr_exc}", exc_info=True)
                # Don't let OCR failure fail the entire upload if file was saved.
                # Return success for upload, but with OCR error message.
                return ProblemImageUploadResponse(
                    success=True, # File upload itself might be successful
                    message=f"Image uploaded successfully to {uploaded_file_access_url}, but OCR failed: {str(ocr_exc)}",
                    file_info=file_info_response,
                    ocr_result={"error": str(ocr_exc)}
                )
        
        message = f"Image uploaded successfully to {uploaded_file_access_url}."
        if auto_ocr and ocr_data:
            message += f" OCR Status: {ocr_data.get('message', 'Completed.')}"
            if ocr_data.get('created'):
                message += f" New problem created with ID: {ocr_data.get('problem_id')}"


        return ProblemImageUploadResponse(
            success=True,
            message=message,
            file_info=file_info_response,
            ocr_result=ocr_data
        )

    except HTTPException as http_exc:
        logger.error(f"HTTPException during problem image upload: {http_exc.detail}", exc_info=True)
        raise http_exc
    except Exception as e:
        logger.error(f"Error uploading problem image {file.filename}: {e}", exc_info=True)
        raise HTTPException(
            status_code=500,
            detail=f"Could not upload problem image: {file.filename}. Error: {str(e)}"
        )

# --- Problem Tag Endpoints ---

tags_router = APIRouter(prefix="/api/v1/problem-tags", tags=["Problem Tags"])

@tags_router.post("", response_model=ProblemTagResponse, summary="创建题目标签")
async def create_problem_tag_endpoint( # Renamed to avoid conflict if router was part of ProblemService
    data: ProblemTagCreate,
    db: AsyncSession = Depends(get_db)
):
    """创建新的题目标签。"""
    service = ProblemService(db)
    try:
        tag = await service.create_problem_tag(data)
        return ProblemTagResponse(
            success=True,
            message="题目标签创建成功",
            data=ProblemTagData.from_orm(tag)
        )
    except ValueError as ve:
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"创建题目标签失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="创建题目标签失败")

@tags_router.get("", response_model=ProblemTagListResponse, summary="获取题目标签列表")
async def list_problem_tags_endpoint(
    page: int = Query(1, ge=1, description="页码"),
    size: int = Query(100, ge=1, le=200, description="每页数量"), # Default size 100 for tags
    db: AsyncSession = Depends(get_db)
):
    """获取所有题目标签，支持分页。"""
    service = ProblemService(db)
    tags_orm, total = await service.list_problem_tags(page=page, size=size)
    
    # FastAPI/Pydantic will handle total for pagination if ProblemTagListResponse includes it.
    # For now, returning a simple list. If pagination info is needed in response, adjust schema.
    return ProblemTagListResponse(
        success=True,
        data=[ProblemTagData.from_orm(t) for t in tags_orm]
        # Add total, page, size to ProblemTagListResponse if needed for frontend pagination
    )

@tags_router.get("/{tag_id}", response_model=ProblemTagResponse, summary="获取单个题目标签")
async def get_problem_tag_endpoint(
    tag_id: str,
    db: AsyncSession = Depends(get_db)
):
    """通过ID获取单个题目标签详情。"""
    service = ProblemService(db)
    tag = await service.get_problem_tag_by_id(tag_id)
    if not tag:
        raise HTTPException(status_code=404, detail="题目标签不存在")
    return ProblemTagResponse(data=ProblemTagData.from_orm(tag))

@tags_router.put("/{tag_id}", response_model=ProblemTagResponse, summary="更新题目标签")
async def update_problem_tag_endpoint(
    tag_id: str,
    data: ProblemTagUpdate,
    db: AsyncSession = Depends(get_db)
):
    """更新指定的题目标签。"""
    service = ProblemService(db)
    try:
        tag = await service.update_problem_tag(tag_id, data)
        if not tag:
            raise HTTPException(status_code=404, detail="题目标签不存在")
        return ProblemTagResponse(
            success=True,
            message="题目标签更新成功",
            data=ProblemTagData.from_orm(tag)
        )
    except ValueError as ve: # For uniqueness constraint on name update
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"更新题目标签 {tag_id} 失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="更新题目标签失败")


@tags_router.delete("/{tag_id}", response_model=ProblemTagResponse, summary="删除题目标签")
async def delete_problem_tag_endpoint(
    tag_id: str,
    db: AsyncSession = Depends(get_db)
):
    """
    删除指定的题目标签。
    注意：此操作目前不会自动从使用该标签的题目中移除标签引用，
    也不会校验 `usage_count`。请谨慎操作。
    """
    service = ProblemService(db)
    # Consider checking tag usage_count before deletion if strict policy is needed
    # tag_to_delete = await service.get_problem_tag_by_id(tag_id)
    # if tag_to_delete and tag_to_delete.usage_count > 0:
    #     raise HTTPException(status_code=400, detail=f"Tag '{tag_to_delete.name}' is currently in use and cannot be deleted.")

    success = await service.delete_problem_tag(tag_id)
    if not success:
        raise HTTPException(status_code=404, detail="题目标签不存在或删除失败")
    return ProblemTagResponse(success=True, message="题目标签删除成功")

# It's important to include this new router in the main application
# This would typically be done in backend/app.py or backend/main.py
# For example: app.include_router(problem_tags_router)
    # This part is outside the scope of modifying this specific file.


# --- Problem Category Endpoints ---

categories_router = APIRouter(prefix="/api/v1/problem-categories", tags=["Problem Categories"])

@categories_router.post("", response_model=ProblemCategoryResponse, summary="创建题目分类")
async def create_problem_category_endpoint(
    data: ProblemCategoryCreate,
    db: AsyncSession = Depends(get_db)
):
    """创建新的题目分类。"""
    service = ProblemService(db)
    try:
        category = await service.create_problem_category(data)
        # Convert ORM model to Pydantic model for response
        # The ProblemCategoryData schema has `children: List['ProblemCategoryData']`
        # Pydantic's from_orm should handle this if relationships are loaded.
        return ProblemCategoryResponse(
            success=True,
            message="题目分类创建成功",
            data=ProblemCategoryData.from_orm(category)
        )
    except ValueError as ve:
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"创建题目分类失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="创建题目分类失败")

@categories_router.get("", response_model=ProblemCategoryListResponse, summary="获取题目分类列表")
async def list_problem_categories_endpoint(
    subject: Optional[Subject] = Query(None, description="按学科过滤"),
    parent_id: Optional[str] = Query("root", description="父分类ID ('root'为顶级, None为所有扁平, 或指定ID)"),
    hierarchical: bool = Query(True, description="是否返回层级结构 (仅当parent_id='root'时有效)"),
    db: AsyncSession = Depends(get_db)
):
    """
    获取题目分类列表。
    支持按学科过滤，获取特定父分类下的子分类，或获取层级结构的分类树。
    """
    service = ProblemService(db)
    categories_orm = await service.list_problem_categories(
        subject=subject,
        parent_id=parent_id,
        hierarchical=hierarchical
    )
    
    # Convert ORM models to Pydantic models.
    # If hierarchical=True and service returns a tree of ORM models (where children are populated via relationship),
    # ProblemCategoryData.from_orm should recursively convert them.
    data_list = [ProblemCategoryData.from_orm(cat) for cat in categories_orm]
    
    return ProblemCategoryListResponse(
        success=True,
        data=data_list
    )

@categories_router.get("/{category_id}", response_model=ProblemCategoryResponse, summary="获取单个题目分类")
async def get_problem_category_endpoint(
    category_id: str,
    db: AsyncSession = Depends(get_db)
):
    """通过ID获取单个题目分类详情。"""
    service = ProblemService(db)
    category = await service.get_problem_category_by_id(category_id)
    if not category:
        raise HTTPException(status_code=404, detail="题目分类不存在")
    return ProblemCategoryResponse(data=ProblemCategoryData.from_orm(category))

@categories_router.put("/{category_id}", response_model=ProblemCategoryResponse, summary="更新题目分类")
async def update_problem_category_endpoint(
    category_id: str,
    data: ProblemCategoryUpdate,
    db: AsyncSession = Depends(get_db)
):
    """更新指定的题目分类。"""
    service = ProblemService(db)
    try:
        category = await service.update_problem_category(category_id, data)
        if not category:
            raise HTTPException(status_code=404, detail="题目分类不存在")
        return ProblemCategoryResponse(
            success=True,
            message="题目分类更新成功",
            data=ProblemCategoryData.from_orm(category)
        )
    except ValueError as ve:
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"更新题目分类 {category_id} 失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="更新题目分类失败")

@categories_router.delete("/{category_id}", response_model=ProblemCategoryResponse, summary="删除题目分类")
async def delete_problem_category_endpoint(
    category_id: str,
    db: AsyncSession = Depends(get_db)
):
    """
    删除指定的题目分类。
    注意：如果分类下有子分类，默认会阻止删除。
    """
    service = ProblemService(db)
    try:
        success = await service.delete_problem_category(category_id)
        if not success: # Should be caught by service raising exception or returning False for not found
            raise HTTPException(status_code=404, detail="题目分类不存在或删除失败")
        return ProblemCategoryResponse(success=True, message="题目分类删除成功")
    except ValueError as ve: # Catch errors like "cannot delete category with children"
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"删除题目分类 {category_id} 失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail="删除题目分类失败")

# Remember to include categories_router in backend/app.py
# app.include_router(categories_router)


@router.get("/export", summary="导出错题数据", response_description="导出的文件内容")
async def export_problems_data(
    # Filter parameters (similar to list_problems)
    subject: Optional[Subject] = Query(None, description="按学科过滤"),
    category: Optional[str] = Query(None, description="按题目分类过滤"),
    tags: Optional[List[str]] = Query(None, description="按标签过滤 (任意匹配)"),
    keyword: Optional[str] = Query(None, description="按关键词搜索 (标题、内容、备注)"),
    min_difficulty: Optional[int] = Query(None, ge=1, le=5, description="最小难度"),
    max_difficulty: Optional[int] = Query(None, ge=1, le=5, description="最大难度"),
    min_mastery: Optional[float] = Query(None, ge=0, le=1, description="最小掌握度"),
    max_mastery: Optional[float] = Query(None, ge=0, le=1, description="最大掌握度"),
    sort_by: str = Query("created_at", pattern="^(created_at|updated_at|difficulty_level|mastery_level|review_count)$", description="排序字段"),
    sort_desc: bool = Query(True, description="是否降序排序"),
    export_format: ExportFormat = Query(ExportFormat.JSON, description="导出格式 (json 或 csv)"),
    db: AsyncSession = Depends(get_db)
):
    """
    根据指定的筛选条件导出错题数据。
    支持 JSON 和 CSV 格式。
    """
    service = ProblemService(db)
    
    difficulty_range = None
    if min_difficulty or max_difficulty:
        difficulty_range = (min_difficulty or 1, max_difficulty or 5)
    
    mastery_range = None
    if min_mastery is not None or max_mastery is not None:
        mastery_range = (min_mastery or 0.0, max_mastery or 1.0)

    problems_data_pydantic = await service.export_problems(
        subject=subject,
        category=category,
        tags=tags,
        keyword=keyword,
        difficulty_range=difficulty_range,
        mastery_range=mastery_range,
        sort_by=sort_by,
        sort_desc=sort_desc
    )

    if not problems_data_pydantic:
        return JSONResponse(
            status_code=200, # Or 404 if no data matches, but export of empty set is valid
            content={"message": "没有符合条件的错题数据可供导出。"}
        )

    if export_format == ExportFormat.JSON:
        # Convert Pydantic models to dicts for JSONResponse
        content_to_export = [p.model_dump(mode="json") for p in problems_data_pydantic]
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"problems_export_{timestamp}.json"
        headers = {"Content-Disposition": f'attachment; filename="{filename}"'}
        return JSONResponse(content=content_to_export, headers=headers)

    elif export_format == ExportFormat.CSV:
        # Define CSV headers from ProblemData fields
        # We can take all fields or select specific ones
        
        # Get all fields from ProblemData schema
        # This might be too many for a CSV, consider a subset.
        # For now, let's use all string/int/float/bool fields for simplicity.
        
        # A more robust way would be to define exportable fields.
        # For simplicity, get field names from the first Pydantic model
        if not problems_data_pydantic: # Should have been caught above
             return JSONResponse(status_code=404, content={"message": "No data to export."})

        # Extract field names from the Pydantic model
        field_names = list(ProblemData.model_fields.keys())
        
        # Filter out complex fields like lists or dicts if they don't serialize well to simple CSV
        # For example, 'tags', 'image_urls', 'knowledge_points', 'ai_analysis' might be better as stringified JSON in CSV.
        
        output = io.StringIO()
        writer = csv.DictWriter(output, fieldnames=field_names, extrasaction='ignore', quoting=csv.QUOTE_NONNUMERIC)
        writer.writeheader()
        
        for problem_pydantic_model in problems_data_pydantic:
            problem_dict = problem_pydantic_model.model_dump()
            # Preprocess complex fields for CSV (e.g., join lists into strings)
            for field in ["tags", "image_urls", "knowledge_points"]:
                if field in problem_dict and isinstance(problem_dict[field], list):
                    problem_dict[field] = ", ".join(map(str, problem_dict[field]))
            if "ai_analysis" in problem_dict and isinstance(problem_dict["ai_analysis"], dict):
                 import json # local import
                 problem_dict["ai_analysis"] = json.dumps(problem_dict["ai_analysis"], ensure_ascii=False)
            # Ensure datetime objects are formatted
            for field, value in problem_dict.items():
                if isinstance(value, datetime):
                    problem_dict[field] = value.isoformat()

            writer.writerow(problem_dict)
        
        output.seek(0)
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        filename = f"problems_export_{timestamp}.csv"
        headers = {
            "Content-Disposition": f'attachment; filename="{filename}"',
            "Content-Type": "text/csv; charset=utf-8-sig" # Ensure BOM for Excel compatibility with UTF-8
        }
        # Add BOM for UTF-8 CSV to be opened correctly by Excel
        # BOM = b'\xef\xbb\xbf'
        # content_with_bom = BOM + output.getvalue().encode('utf-8')
        
        # Simpler: Use StreamingResponse which handles bytes
        
        # It's better to stream bytes directly.
        # Create a generator for byte stream
        def iter_csv_bytes():
            yield b'\xef\xbb\xbf' # UTF-8 BOM
            # Write header as bytes
            header_row = ",".join(f'"{name}"' for name in field_names) + "\n"
            yield header_row.encode('utf-8')

            # Write data rows as bytes
            temp_output = io.StringIO()
            data_writer = csv.DictWriter(temp_output, fieldnames=field_names, extrasaction='ignore', quoting=csv.QUOTE_NONNUMERIC)
            # No header for data_writer as we wrote it manually

            for problem_pydantic_model in problems_data_pydantic:
                problem_dict = problem_pydantic_model.model_dump()
                for field in ["tags", "image_urls", "knowledge_points"]:
                    if field in problem_dict and isinstance(problem_dict[field], list):
                        problem_dict[field] = ", ".join(map(str, problem_dict[field]))
                if "ai_analysis" in problem_dict and isinstance(problem_dict["ai_analysis"], dict):
                    import json
                    problem_dict[field] = json.dumps(problem_dict["ai_analysis"], ensure_ascii=False)
                for field, value in problem_dict.items():
                    if isinstance(value, datetime):
                        problem_dict[field] = value.isoformat()
                    elif value is None: # csv.DictWriter writes empty string for None by default.
                        problem_dict[field] = "" 
                
                data_writer.writerow(problem_dict)
                temp_output.seek(0)
                yield temp_output.read().encode('utf-8')
                temp_output.truncate(0) # Clear for next row
                temp_output.seek(0)


        # return StreamingResponse(io.BytesIO(output.getvalue().encode('utf-8-sig')), media_type="text/csv", headers=headers)
        # Using utf-8-sig for Excel compatibility.
        # For StreamingResponse, the iterator must yield bytes.
        
        # Correct way with StreamingResponse and an iterator yielding bytes:
        # The csv writer writes to a StringIO, then we encode and yield.
        
        # Let's try a simpler CSV export first that might not be perfectly streamed for large data
        # but is easier to implement correctly. For very large CSVs, a true streaming generator is better.

        # Re-simplifying CSV for now, focusing on correctness of content.
        # True streaming would involve yielding rows one by one as bytes.
        string_io_buffer = io.StringIO()
        # Add BOM for UTF-8 CSV
        string_io_buffer.write('\ufeff') # This is the BOM for UTF-8

        csv_writer = csv.DictWriter(string_io_buffer, fieldnames=field_names, extrasaction='ignore', quoting=csv.QUOTE_NONNUMERIC)
        csv_writer.writeheader()
        for problem_pydantic_model in problems_data_pydantic:
            problem_dict = problem_pydantic_model.model_dump()
            processed_row = {}
            for field_name in field_names:
                value = problem_dict.get(field_name)
                if isinstance(value, list):
                    processed_row[field_name] = ", ".join(map(str, value))
                elif isinstance(value, dict):
                    import json # local import
                    processed_row[field_name] = json.dumps(value, ensure_ascii=False)
                elif isinstance(value, datetime):
                    processed_row[field_name] = value.isoformat()
                elif isinstance(value, PyEnum): # Handle our Subject enum if it's directly in data
                     processed_row[field_name] = value.value
                else:
                    processed_row[field_name] = value if value is not None else ""
            csv_writer.writerow(processed_row)
        
        # Get CSV content as string
        csv_content = string_io_buffer.getvalue()
        string_io_buffer.close()
        
        return StreamingResponse(
            iter([csv_content.encode("utf-8")]), # iter to make it a valid iterator for StreamingResponse
            media_type="text/csv", 
            headers=headers
        )

    else: # Should not happen due to Enum validation
        raise HTTPException(status_code=400, detail="不支持的导出格式。")


class BatchOperationResultDetail(BaseModel):
    id: str
    error: str

class BatchOperationResponse(BaseModel):
    success: bool
    message: str
    successful_ids: List[str] = Field(default_factory=list)
    failed_count: int
    errors: List[BatchOperationResultDetail] = Field(default_factory=list)


@router.post("/batch-operate", response_model=BatchOperationResponse, summary="批量操作错题")
async def batch_operate_problems_endpoint(
    request_data: ProblemBatchRequest,
    db: AsyncSession = Depends(get_db)
):
    """
    对指定的错题列表执行批量操作。
    支持的操作: "delete", "update", "analyze".
    - "delete": 批量软删除错题。
    - "update": 批量更新错题的指定字段 (在 `update_data` 中提供)。
    - "analyze": 批量对错题执行AI分析。
    """
    service = ProblemService(db)
    try:
        results = await service.batch_operate_problems(request_data)
        
        failed_count = len(results.get("failed_ids", []))
        if "commit_error" in results: # Handle overarching commit error
            failed_count = len(request_data.problem_ids) # Assume all failed if commit failed
            return BatchOperationResponse(
                success=False,
                message=f"批量操作因提交错误失败: {results['commit_error']}",
                failed_count=failed_count,
                errors=[{"id": "N/A", "error": f"Commit error: {results['commit_error']}"}]
            )

        return BatchOperationResponse(
            success=failed_count == 0,
            message=f"批量操作 '{request_data.operation}' 完成。成功: {len(results.get('successful_ids', []))}, 失败: {failed_count}.",
            successful_ids=results.get("successful_ids", []),
            failed_count=failed_count,
            errors=[BatchOperationResultDetail(id=err["id"], error=err["error"]) for err in results.get("errors", [])]
        )
    except Exception as e:
        logger.error(f"执行批量操作失败: {e}", exc_info=True)
        # If a general exception occurs, assume all failed for this request
        return BatchOperationResponse(
            success=False,
            message=f"执行批量操作时发生意外错误: {str(e)}",
            failed_count=len(request_data.problem_ids),
            errors=[{"id": pid, "error": str(e)} for pid in request_data.problem_ids]
        )


# --- AI-Driven Problem Creation Endpoints ---

ai_problem_router = APIRouter(prefix="/api/v1/problems/ai-create", tags=["Problems - AI Driven Creation"])

@ai_problem_router.post("/initiate", response_model=ProblemAICreateInitiateResponse, summary="阶段一：AI驱动错题创建 - 初始化")
async def ai_initiate_problem_creation(
    request_data: ProblemAICreateInitiateRequest,
    db: AsyncSession = Depends(get_db)
    # No need to inject file_service or ai_model_service here, ProblemService handles them
):
    """
    上传图片（Base64或URL），AI进行初步OCR和结构化分析。
    服务层将处理图片上传、调用AI模型，并返回包含 session_id 和初步分析结果。
    """
    problem_service_instance = ProblemService(db)
    try:
        # The service method initiate_ai_problem_creation handles file saving and AI call.
        structured_data_from_service = await problem_service_instance.initiate_ai_problem_creation(request_data)
        
        return ProblemAICreateInitiateResponse(
            success=True,
            message="AI错题创建初始化成功。",
            data=structured_data_from_service
        )
    except HTTPException as http_exc: # Re-raise HTTP exceptions that service might raise
        logger.error(f"HTTPException during AI problem initiation: {http_exc.detail}", exc_info=True)
        raise http_exc
    except ValueError as ve: # Catch specific validation errors from service or Pydantic
        logger.warning(f"AI initiate problem creation Value Error: {ve}", exc_info=True)
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"AI错题创建初始化失败: {e}", exc_info=True)
        # Catch-all for other unexpected errors from the service layer
        raise HTTPException(status_code=500, detail=f"AI错题创建初始化过程发生内部错误: {str(e)}")


@ai_problem_router.get("/interactive-stream/{session_id}", summary="阶段二：AI驱动错题创建 - 交互式分析流 (SSE)")
async def ai_interactive_analysis_stream(
    session_id: str,
    user_message: Optional[str] = Query(None, description="用户发送的消息"),
    chat_history_json: Optional[str] = Query(None, alias="chat_history_json", description="JSON字符串格式的聊天历史"),
    initial_data_ref_json: Optional[str] = Query(None, alias="initial_data_ref_json", description="JSON字符串格式的初始结构化数据参考"),
    db: AsyncSession = Depends(get_db)
):
    """
    与AI进行交互式分析和错题内容细化。
    客户端通过此接口发送用户消息和对话历史 (作为查询参数)，接收AI的流式响应 (Server-Sent Events)。
    - `chat_history_json`: 应为 `List[ChatLogEntry]` 的JSON字符串。
    - `initial_data_ref_json`: 应为 `ProblemAIStructuredData` (或其部分) 的JSON字符串。
    """
    service = ProblemService(db)

    parsed_chat_history: List[Dict[str, Any]] = []
    if chat_history_json:
        try:
            parsed_chat_history = json.loads(chat_history_json)
            if not isinstance(parsed_chat_history, list):
                parsed_chat_history = [] # Default to empty if not a list
        except json.JSONDecodeError:
            logger.warning(f"SSE stream: Invalid JSON for chat_history_json for session {session_id}")
            # Potentially raise HTTPException or proceed with empty history
            parsed_chat_history = []

    parsed_initial_data_ref: Optional[Dict[str, Any]] = None
    if initial_data_ref_json:
        try:
            parsed_initial_data_ref = json.loads(initial_data_ref_json)
            if not isinstance(parsed_initial_data_ref, dict):
                 parsed_initial_data_ref = None
        except json.JSONDecodeError:
            logger.warning(f"SSE stream: Invalid JSON for initial_data_ref_json for session {session_id}")
            parsed_initial_data_ref = None
            
    # Reconstruct the payload for the service layer, matching ProblemAIStreamRequest schema
    parsed_chat_history_models = []
    if parsed_chat_history: # Ensure it's not empty
        for item in parsed_chat_history:
            # Validate item structure before creating AIChatMessage
            if isinstance(item, dict) and "role" in item and "content" in item:
                try:
                    parsed_chat_history_models.append(AIChatMessage(role=item["role"], content=item["content"]))
                except Exception as e_chat_model: # Catch Pydantic validation error for AIChatMessage
                    logger.warning(f"SSE stream: Invalid item in chat_history_json for session {session_id}: {item}. Error: {e_chat_model}")
                    # Skip invalid items or handle error appropriately
            else:
                logger.warning(f"SSE stream: Skipping malformed item in chat_history_json for session {session_id}: {item}")


    initial_data_ref_model: Optional[ProblemAIStructuredData] = None
    if parsed_initial_data_ref:
        try:
            initial_data_ref_model = ProblemAIStructuredData(**parsed_initial_data_ref)
        except Exception as e_initial_data: # Catch Pydantic validation error
            logger.warning(f"SSE stream: Invalid JSON for initial_data_ref_json, cannot parse as ProblemAIStructuredData for session {session_id}. Error: {e_initial_data}")
            # Proceed with None if parsing fails

    stream_request_payload = ProblemAIStreamRequest(
        user_message=user_message,
        chat_history=parsed_chat_history_models,
        initial_data_ref=initial_data_ref_model
    )
    
    async def stream_wrapper():
        """Wraps the service stream to format data as SSE."""
        try:
            # Pass the session_id_str and the reconstructed payload to the service method
            async for chunk_data in service.handle_ai_interactive_analysis_stream(session_id_str=session_id, request_payload=stream_request_payload):
                if isinstance(chunk_data, str): # Assuming service yields dicts as per its original design
                    # This path might not be hit if service.handle_ai_interactive_analysis_stream always yields dicts
                    sse_event = {"type": "content_chunk", "value": chunk_data}
                    yield f"data: {json.dumps(sse_event)}\n\n"
                elif isinstance(chunk_data, dict): # This is the expected path from service's stream_ai_response_for_session
                    # The service should yield dicts structured for SSE (e.g., {"type": "content_chunk", "value": "..."})
                    # Or {"type": "error", "message": "..."}
                    # Or {"type": "stream_end"}
                    yield f"data: {json.dumps(chunk_data)}\n\n"
                else:
                    logger.warning(f"Unknown chunk type received in AI stream for session {session_id}: {type(chunk_data)}. Expected dict.")
            
            # Stream_end event should ideally be yielded by the service layer method itself.
            # If not, we can add it here, but it's cleaner if the service controls its full stream lifecycle.
            # For robustness, if the service stream ends without a specific 'stream_end' event,
            # the client's EventSource onclose or onerror will eventually trigger.
            # However, an explicit end event is good practice.
            # Let's assume service.handle_ai_interactive_analysis_stream yields a stream_end event.

        except HTTPException as http_exc_stream: # Catch HTTP exceptions from the service layer
            logger.error(f"HTTPException during stream processing for session {session_id}: {http_exc_stream.detail}", exc_info=True)
            error_event = {"type": "error", "message": http_exc_stream.detail, "status_code": http_exc_stream.status_code}
            yield f"data: {json.dumps(error_event)}\n\n"
        except Exception as e_stream_api:
            logger.error(f"Error during API stream generation for session {session_id}: {e_stream_api}", exc_info=True)
            error_event = {"type": "error", "message": f"Stream processing error: {str(e_stream_api)}"}
            yield f"data: {json.dumps(error_event)}\n\n"

    return StreamingResponse(stream_wrapper(), media_type="text/event-stream")


@ai_problem_router.post("/finalize", response_model=SingleProblemResponse, summary="阶段三：AI驱动错题创建 - 最终确认")
async def ai_finalize_problem_creation(
    request_data: ProblemAIFinalizeRequest,
    db: AsyncSession = Depends(get_db)
):
    """
    最终确认AI辅助创建的错题信息，并将其保存到数据库。
    请求中应包含所有错题字段、AI的完整分析JSON以及聊天记录。
    """
    service = ProblemService(db)
    try:
        created_problem_orm = await service.finalize_ai_problem_creation(request_data)
        return SingleProblemResponse(
            success=True,
            message="AI辅助创建错题并成功保存。",
            data=ProblemData.from_orm(created_problem_orm)
        )
    except HTTPException as http_exc:
        raise http_exc
    except ValueError as ve:
        logger.warning(f"AI finalize problem creation Value Error for session {request_data.session_id}: {ve}", exc_info=True)
        raise HTTPException(status_code=400, detail=str(ve))
    except Exception as e:
        logger.error(f"AI辅助创建错题最终确认失败 for session {request_data.session_id}: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"AI辅助创建错题最终确认失败: {str(e)}")


@ai_problem_router.get("/session/{session_id}", response_model=SingleProblemAISessionResponse, summary="获取AI错题创建会话详情")
async def get_ai_problem_session_details(
    session_id: str,
    db: AsyncSession = Depends(get_db)
):
    """
    根据会话ID获取AI错题创建会话的详细信息，
    包括当前状态、结构化数据、初始信息、以及最终关联的错题（如果已生成）和聊天记录。
    """
    service = ProblemService(db)
    try:
        # Service method should handle eager loading of related data (chat logs, final problem)
        session_orm = await service.get_problem_ai_session(session_id, load_chat_logs=True, load_final_problem=True)
        if not session_orm:
            raise HTTPException(status_code=404, detail=f"AI创建会话 {session_id} 未找到。")

        # Convert ORM model (ProblemAISession) to Pydantic model (ProblemAISessionData)
        # This conversion should correctly handle nested ProblemData and List[AIChatMessage]
        # if ProblemAISessionData.Config.from_attributes = True (or orm_mode=True for Pydantic v1)
        # and relationships are correctly defined and loaded in the ORM model.
        
        # Explicitly convert chat logs if needed and not handled by from_orm for nested lists of Pydantic models
        chat_logs_pydantic = []
        if session_orm.ai_chat_logs: # ai_chat_logs is the relationship name in ProblemAISession model
            for log_entry_orm in session_orm.ai_chat_logs:
                chat_logs_pydantic.append(AIChatMessage.from_orm(log_entry_orm)) # Assuming AIChatMessage can from_orm ProblemAIChatLog

        # Convert final_problem if it exists
        final_problem_pydantic = None
        if session_orm.final_problem: # final_problem is the relationship name
            final_problem_pydantic = ProblemData.from_orm(session_orm.final_problem)

        session_data_pydantic = ProblemAISessionData(
            id=str(session_orm.id), # Ensure ID is string
            status=session_orm.status,
            initial_image_ref=str(session_orm.initial_image_ref) if session_orm.initial_image_ref else None,
            initial_subject_hint=session_orm.initial_subject_hint,
            current_structured_data=session_orm.current_structured_data, # Assuming this is already a dict/JSONB
            final_problem_id=str(session_orm.final_problem_id) if session_orm.final_problem_id else None,
            created_at=session_orm.created_at,
            updated_at=session_orm.updated_at,
            final_problem=final_problem_pydantic,
            chat_logs=chat_logs_pydantic
        )
        
        return SingleProblemAISessionResponse(
            success=True,
            data=session_data_pydantic
        )
    except HTTPException as http_exc:
        raise http_exc
    except Exception as e:
        logger.error(f"获取AI会话 {session_id} 详情失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"获取AI会话详情时发生内部错误: {str(e)}")


@ai_problem_router.get("/chat-history/{session_id}", response_model=AIChatHistoryResponse, summary="获取AI错题创建会话的聊天记录")
async def get_ai_problem_session_chat_history(
    session_id: str,
    db: AsyncSession = Depends(get_db)
):
    """
    根据会话ID获取AI错题创建会话的完整聊天记录。
    """
    service = ProblemService(db)
    try:
        # Service method returns List[ProblemAIChatLog] (ORM objects)
        chat_logs_orm = await service.get_chat_history_for_session(session_id)
        
        if not chat_logs_orm: # Check if the list is empty
            # To distinguish between "session not found" and "session exists but has no logs",
            # we check if the session itself exists.
            _session = await service.get_problem_ai_session(session_id, load_chat_logs=False, load_final_problem=False)
            if not _session:
                 raise HTTPException(status_code=404, detail=f"AI创建会话 {session_id} 未找到。")
        
        # Convert List[ProblemAIChatLog] (ORM) to List[AIChatMessage] (Pydantic)
        chat_logs_pydantic = [AIChatMessage.from_orm(log) for log in chat_logs_orm]
        
        return AIChatHistoryResponse(
            success=True,
            message="聊天记录获取成功。" if chat_logs_pydantic else "该会话没有聊天记录。",
            data=chat_logs_pydantic
        )
    except HTTPException as http_exc:
        raise http_exc
    except Exception as e:
        logger.error(f"获取AI会话 {session_id} 聊天记录失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"获取聊天记录时发生内部错误: {str(e)}")


@ai_problem_router.get("/sessions", response_model=ProblemAISessionListResponse, summary="获取AI错题创建会话列表")
async def list_ai_problem_sessions_endpoint(
    page: int = Query(1, ge=1, description="页码"),
    size: int = Query(20, ge=1, le=100, description="每页数量"),
    status: Optional[ProblemAISessionStatus] = Query(None, description="按会话状态过滤"),
    sort_by: str = Query("created_at", description="排序字段 (例如: created_at, updated_at, status)"),
    sort_desc: bool = Query(True, description="是否降序排序"),
    db: AsyncSession = Depends(get_db)
):
    """
    获取所有AI错题创建会话的列表，支持分页和按状态过滤。
    """
    service = ProblemService(db)
    try:
        # Validate sort_by field against ProblemAISession model attributes if necessary
        allowed_sort_fields = ["created_at", "updated_at", "status", "initial_subject_hint"] # Add more as needed
        if sort_by not in allowed_sort_fields:
            sort_by = "created_at" # Default if invalid

        sessions_orm, total_sessions = await service.list_ai_problem_sessions(
            page=page,
            size=size,
            status=status,
            sort_by=sort_by,
            sort_desc=sort_desc
        )

        # Convert ORM list to Pydantic list
        sessions_data_pydantic = []
        for session_orm in sessions_orm:
            # For list view, we might not need full chat_logs or final_problem details to keep response light.
            # ProblemAISessionData by default tries to load them if relationships are present.
            # The service method `list_ai_problem_sessions` currently does not eager load these.
            # So, chat_logs and final_problem will be empty lists/None in ProblemAISessionData.from_orm(session_orm)
            # which is generally fine for a list view.
            sessions_data_pydantic.append(ProblemAISessionData.from_orm(session_orm))
            
        # The ProblemAISessionListResponse schema expects `data`, `total`, `page`, `size`
        # It needs to be updated to include these pagination fields, or we return a custom dict.
        # Let's assume ProblemAISessionListResponse is updated or we construct the dict.
        # For now, ProblemAISessionListResponse only has success, message, data.
        # Let's adapt the response to include pagination.
        # Update: The schema `ProblemAISessionListResponse` was defined without pagination fields in the prompt.
        # I will return a dict matching the structure of `ProblemListResponse` for consistency.
        # The ProblemAISessionListResponse schema has been updated to include pagination fields.
        return ProblemAISessionListResponse(
            success=True,
            message="AI会话列表获取成功。",
            data=sessions_data_pydantic,
            total=total_sessions,
            page=page,
            size=size
            # The 'pages' property is calculated automatically by the Pydantic model
        )

    except Exception as e:
        logger.error(f"获取AI会话列表失败: {e}", exc_info=True)
        raise HTTPException(status_code=500, detail=f"获取AI会话列表时发生内部错误: {str(e)}")


# Make sure to include ai_problem_router in the main app.
# e.g., in backend/app.py: app.include_router(ai_problem_router_from_problem_service)
