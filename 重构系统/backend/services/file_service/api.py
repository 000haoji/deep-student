"""
文件服务 API 路由
"""
from fastapi import APIRouter, Depends, UploadFile, File, HTTPException, status, Form
from fastapi.responses import StreamingResponse
from sqlalchemy.ext.asyncio import AsyncSession
from typing import Optional, List

from shared.database import get_db
from shared.utils.logger import get_logger
from .service import file_service # 从同级service模块导入
from .schemas import FileUploadResponse, PresignedUrlResponse, FileDetailResponse # 从同级schemas模块导入
from .models import FileRecord # 从同级models导入
from sqlalchemy import select


logger = get_logger(__name__)

router = APIRouter(
    prefix="/api/v1/files",
    tags=["File Service"],
)

@router.post("/upload", response_model=FileUploadResponse)
async def upload_file_endpoint(
    file: UploadFile = File(...),
    category: Optional[str] = Form("general"),
    related_id: Optional[str] = Form(None),
    description: Optional[str] = Form(None),
    is_public: bool = Form(False),
    db: AsyncSession = Depends(get_db)
):
    """
    上传单个文件。

    - **file**: 要上传的文件。
    - **category**: 文件分类 (例如: "avatars", "problem_attachments")。
    - **related_id**: 关联的业务ID (例如: user_id, problem_id)。
    - **description**: 文件描述。
    - **is_public**: 文件是否公开访问 (生成永久URL或预签名URL)。
    """
    try:
        contents = await file.read()
        # 使用 service 中的 upload_file 方法，它会处理 MinIO 上传和数据库记录
        # 注意：service.py 中的 upload_file 返回的是 access_url (str)
        # 我们需要从数据库获取 FileRecord 以填充 FileUploadResponse
        
        access_url = await file_service.upload_file(
            file_data=contents,
            filename=file.filename,
            category=category,
            related_id=related_id,
            description=description,
            is_public=is_public,
            db=db  # 传递数据库会话以保存记录
        )
        
        # 从数据库中检索刚刚创建的 FileRecord
        # object_name 是基于 category 和 filename 生成的，需要从 access_url 中反推或者修改 service 返回 FileRecord
        # 简单起见，这里假设 access_url 包含 object_name 且容易提取
        # 更稳妥的做法是让 service.upload_file 返回 FileRecord 对象或其ID

        # 尝试从数据库获取最新的 FileRecord，假设 object_name 是唯一的且刚被创建
        # 这是一个简化的查找，实际应用可能需要更精确的查找方式
        # 例如，如果upload_file返回了object_name，则用object_name查找
        
        # 为了能够正确返回 FileUploadResponse，我们需要 FileRecord 实例
        # 目前 file_service.upload_file 只返回 access_url.
        # 为了获取完整的 FileRecord，我们需要在 upload_file 之后查询数据库
        # 一个简单的策略是按 access_url 查询，但这不够健壮。
        # 更优的做法是让 upload_file 返回创建的 FileRecord 实例或其 object_name。
        # 这里我们先假设可以通过 access_url 反查到（不推荐生产这样做）
        
        # 假设 object_name 可以从 access_url 中解析出来
        # Example: http://minio_endpoint/bucket_name/category/date/filename.ext
        # object_name = category/date/filename.ext
        
        parts = access_url.split('/')
        object_name_from_url = "/".join(parts[4:]) # 根据URL结构调整索引

        stmt = select(FileRecord).where(FileRecord.object_name == object_name_from_url).order_by(FileRecord.created_at.desc())
        result = await db.execute(stmt)
        file_record = result.scalars().first()

        if not file_record:
            logger.error(f"Failed to retrieve FileRecord from DB after upload for URL: {access_url}")
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail="File uploaded but failed to retrieve record details."
            )
            
        return file_record # Pydantic 会自动从 ORM 模型转换

    except HTTPException as http_exc:
        logger.error(f"HTTPException during file upload: {http_exc.detail}")
        raise http_exc
    except Exception as e:
        logger.error(f"Error uploading file: {file.filename}, Error: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Could not upload file: {file.filename}. Error: {str(e)}"
        )

@router.post("/upload-image", response_model=FileUploadResponse)
async def upload_image_endpoint(
    file: UploadFile = File(...),
    category: Optional[str] = Form("images"),
    related_id: Optional[str] = Form(None),
    description: Optional[str] = Form(None),
    is_public: bool = Form(False),
    max_width: Optional[int] = Form(1920),
    max_height: Optional[int] = Form(1080),
    quality: int = Form(85),
    db: AsyncSession = Depends(get_db)
):
    """
    上传图片文件，并进行压缩处理。

    - **file**: 要上传的图片文件。
    - **category**: 文件分类。
    - **related_id**: 关联的业务ID。
    - **description**: 文件描述。
    - **is_public**: 文件是否公开访问。
    - **max_width**: 图片压缩后的最大宽度。
    - **max_height**: 图片压缩后的最大高度。
    - **quality**: 图片压缩质量 (1-100)。
    """
    try:
        contents = await file.read()
        
        access_url = await file_service.upload_image(
            image_data=contents,
            filename=file.filename,
            category=category,
            max_width=max_width,
            max_height=max_height,
            quality=quality,
            related_id=related_id,
            description=description,
            is_public=is_public,
            db=db
        )

        parts = access_url.split('/')
        object_name_from_url = "/".join(parts[4:])

        stmt = select(FileRecord).where(FileRecord.object_name == object_name_from_url).order_by(FileRecord.created_at.desc())
        result = await db.execute(stmt)
        file_record = result.scalars().first()

        if not file_record:
            logger.error(f"Failed to retrieve FileRecord from DB after image upload for URL: {access_url}")
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail="Image uploaded but failed to retrieve record details."
            )
            
        return file_record

    except HTTPException as http_exc:
        logger.error(f"HTTPException during image upload: {http_exc.detail}")
        raise http_exc
    except Exception as e:
        logger.error(f"Error uploading image: {file.filename}, Error: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Could not upload image: {file.filename}. Error: {str(e)}"
        )


@router.get("/{object_name:path}/url", response_model=PresignedUrlResponse)
async def get_presigned_url_endpoint(object_name: str, expires: int = 3600):
    """
    获取文件的预签名访问URL。

    - **object_name**: MinIO中的对象完整路径 (e.g., category/2023/01/01/file.jpg)。
    - **expires**: URL有效时间（秒），默认为1小时。
    """
    try:
        url = await file_service.get_file_url(object_name, expires=expires)
        return PresignedUrlResponse(object_name=object_name, url=url, expires_in_seconds=expires)
    except Exception as e:
        logger.error(f"Error getting presigned URL for {object_name}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND, # 或500，取决于错误类型
            detail=f"Could not get presigned URL for file: {object_name}. Error: {str(e)}"
        )

@router.get("/{object_name:path}/download")
async def download_file_endpoint(object_name: str):
    """
    下载文件。

    - **object_name**: MinIO中的对象完整路径。
    """
    try:
        file_data = await file_service.download_file(object_name)
        # 尝试从对象名获取原始文件名或 MIME 类型以优化下载体验
        # 这里简单地返回二进制流
        # 注意：filename 和 media_type 在 StreamingResponse 中可以帮助浏览器正确处理文件
        # 可能需要从 FileRecord 中获取原始文件名和MIME类型
        return StreamingResponse(iter([file_data]), media_type="application/octet-stream")
    except Exception as e:
        logger.error(f"Error downloading file {object_name}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"Could not download file: {object_name}. Error: {str(e)}"
        )

@router.get("/{object_name:path}/info", response_model=FileDetailResponse)
async def get_file_info_endpoint(object_name: str, db: AsyncSession = Depends(get_db)):
    """
    获取存储在数据库中的文件记录信息。

    - **object_name**: MinIO中的对象完整路径。
    """
    stmt = select(FileRecord).where(FileRecord.object_name == object_name)
    result = await db.execute(stmt)
    file_record = result.scalars().first()

    if not file_record:
        raise HTTPException(
            status_code=status.HTTP_404_NOT_FOUND,
            detail=f"File record not found for object: {object_name}"
        )
    return file_record
    
@router.delete("/{object_name:path}", status_code=status.HTTP_204_NO_CONTENT)
async def delete_file_endpoint(object_name: str, db: AsyncSession = Depends(get_db)):
    """
    删除文件（从MinIO和数据库记录）。

    - **object_name**: MinIO中的对象完整路径。
    """
    try:
        success = await file_service.delete_file(object_name, db=db)
        if not success:
            raise HTTPException(
                status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
                detail=f"Failed to delete file from storage or database: {object_name}"
            )
        return None # FastAPI 会自动处理 204
    except Exception as e:
        logger.error(f"Error deleting file {object_name}: {e}", exc_info=True)
        raise HTTPException(
            status_code=status.HTTP_500_INTERNAL_SERVER_ERROR,
            detail=f"Could not delete file: {object_name}. Error: {str(e)}"
        )

# 可以在这里添加更多端点，例如列出文件等
# @router.get("/", response_model=List[FileDetailResponse])
# async def list_files_endpoint(
#     category: Optional[str] = None, 
#     related_id: Optional[str] = None,
#     db: AsyncSession = Depends(get_db)
# ):
#     # ... 实现基于数据库记录的文件列表 ...
#     pass
