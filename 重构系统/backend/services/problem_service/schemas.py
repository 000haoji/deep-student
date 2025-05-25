"""
错题管理服务的Pydantic模型
"""
from typing import List, Optional, Dict, Any
from datetime import datetime
from pydantic import BaseModel, Field, validator, HttpUrl, model_validator # HttpUrl and model_validator
from uuid import UUID

from .models import Subject


# 请求模型
class ProblemCreate(BaseModel):
    """创建错题的请求模型"""
    title: str = Field(..., description="题目标题", max_length=200)
    content: str = Field(..., description="题目内容")
    subject: Subject = Field(..., description="学科")
    category: Optional[str] = Field(None, description="题目分类", max_length=100)
    source: Optional[str] = Field(None, description="题目来源", max_length=200)
    year: Optional[int] = Field(None, description="年份", ge=1900, le=2100)
    user_answer: Optional[str] = Field(None, description="用户答案")
    correct_answer: Optional[str] = Field(None, description="正确答案")
    error_analysis: Optional[str] = Field(None, description="错误分析")
    solution: Optional[str] = Field(None, description="解题思路")
    tags: List[str] = Field(default_factory=list, description="标签列表")
    image_urls: List[str] = Field(default_factory=list, description="题目图片URL列表")
    image_base64: Optional[List[str]] = Field(default_factory=list, description="Base64编码的图片列表，用于上传") # Added for service input
    notes: Optional[str] = Field(None, description="用户备注")

    class Config:
        json_schema_extra = {
            "example": {
                "title": "高等数学-极限计算",
                "content": "计算极限 lim(x->0) sin(x)/x",
                "subject": "math",
                "category": "微积分",
                "user_answer": "0",
                "correct_answer": "1",
                "error_analysis": "没有掌握重要极限公式"
            }
        }


# ProblemTag Schemas
class ProblemTagCreate(BaseModel):
    """创建题目标签的请求模型"""
    name: str = Field(..., description="标签名称", max_length=50)
    description: Optional[str] = Field(None, description="描述")
    color: Optional[str] = Field(None, description="标签颜色 (例如 #FFFFFF)", max_length=20)

    class Config:
        json_schema_extra = {
            "example": {
                "name": "重要考点",
                "description": "历年真题中的重要考点",
                "color": "#FF0000"
            }
        }

class ProblemTagUpdate(BaseModel):
    """更新题目标签的请求模型"""
    name: Optional[str] = Field(None, description="标签名称", max_length=50)
    description: Optional[str] = Field(None, description="描述")
    color: Optional[str] = Field(None, description="标签颜色", max_length=20)

class ProblemTagData(BaseModel):
    """题目标签数据模型"""
    id: str = Field(..., description="标签ID")
    name: str = Field(..., description="标签名称")
    description: Optional[str] = Field(None, description="描述")
    color: Optional[str] = Field(None, description="标签颜色")
    usage_count: int = Field(..., description="使用次数")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")

    class Config:
        from_attributes = True

class ProblemTagResponse(BaseModel):
    """单个题目标签的API响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: Optional[ProblemTagData] = None

class ProblemTagListResponse(BaseModel):
    """题目标签列表的API响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: List[ProblemTagData] = Field(default_factory=list)


# ProblemCategory Schemas
class ProblemCategoryCreate(BaseModel):
    """创建题目分类的请求模型"""
    name: str = Field(..., description="分类名称", max_length=100)
    subject: Subject = Field(..., description="所属学科")
    parent_id: Optional[str] = Field(None, description="父分类ID (用于层级结构)")
    description: Optional[str] = Field(None, description="描述")
    order: int = Field(0, description="排序字段")

    class Config:
        json_schema_extra = {
            "example": {
                "name": "函数与极限",
                "subject": "math",
                "parent_id": None, # Or an existing category ID
                "description": "高等数学第一章内容"
            }
        }

class ProblemCategoryUpdate(BaseModel):
    """更新题目分类的请求模型"""
    name: Optional[str] = Field(None, description="分类名称", max_length=100)
    parent_id: Optional[str] = Field(None, description="父分类ID") # Allow changing parent
    description: Optional[str] = Field(None, description="描述")
    order: Optional[int] = Field(None, description="排序字段")
    # Subject is typically not changed for a category, but could be allowed if needed.

class ProblemCategoryData(BaseModel):
    """题目分类数据模型"""
    id: str = Field(..., description="分类ID")
    name: str = Field(..., description="分类名称")
    subject: Subject = Field(..., description="所属学科")
    parent_id: Optional[str] = Field(None, description="父分类ID")
    description: Optional[str] = Field(None, description="描述")
    order: int = Field(..., description="排序字段")
    usage_count: int = Field(..., description="使用次数")
    children: List['ProblemCategoryData'] = Field(default_factory=list, description="子分类列表") # For hierarchical display
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")

    class Config:
        from_attributes = True

# This is needed to handle the self-referencing 'children' field
# ProblemCategoryData.model_rebuild() # For Pydantic v2
# For Pydantic v1, update_forward_refs() was used. Pydantic v2 handles this better with model_rebuild or automatically.

class ProblemCategoryResponse(BaseModel):
    """单个题目分类的API响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: Optional[ProblemCategoryData] = None

class ProblemCategoryListResponse(BaseModel):
    """题目分类列表的API响应模型 (通常返回扁平列表或树状结构)"""
    success: bool = True
    message: Optional[str] = None
    data: List[ProblemCategoryData] = Field(default_factory=list) # Could be a tree structure


class ProblemUpdate(BaseModel):
    """更新错题的请求模型"""
    title: Optional[str] = Field(None, description="题目标题", max_length=200)
    content: Optional[str] = Field(None, description="题目内容")
    category: Optional[str] = Field(None, description="题目分类", max_length=100)
    source: Optional[str] = Field(None, description="题目来源", max_length=200)
    year: Optional[int] = Field(None, description="年份", ge=1900, le=2100)
    user_answer: Optional[str] = Field(None, description="用户答案")
    correct_answer: Optional[str] = Field(None, description="正确答案")
    error_analysis: Optional[str] = Field(None, description="错误分析")
    solution: Optional[str] = Field(None, description="解题思路")
    tags: Optional[List[str]] = Field(None, description="标签列表")
    image_urls: Optional[List[str]] = Field(None, description="题目图片URL列表")
    notes: Optional[str] = Field(None, description="用户备注")
    mastery_level: Optional[float] = Field(None, description="掌握程度", ge=0, le=1)
    
    class Config:
        json_schema_extra = {
            "example": {
                "error_analysis": "更新的错误分析",
                "mastery_level": 0.8,
                "tags": ["重点", "易错"]
            }
        }


class ProblemAnalyzeRequest(BaseModel):
    """分析错题请求"""
    regenerate: bool = Field(False, description="是否重新生成分析")
    focus_areas: Optional[List[str]] = Field(None, description="重点分析领域")


class ProblemOCRRequest(BaseModel):
    """OCR识别请求"""
    image_base64: Optional[str] = Field(None, description="Base64编码的图片")
    image_url: Optional[HttpUrl] = Field(None, description="图片的URL")
    enhance_math: bool = Field(True, description="是否增强数学公式识别")
    auto_create: bool = Field(False, description="是否自动创建题目")

    @model_validator(mode='before')
    @classmethod
    def check_image_source(cls, values):
        image_base64, image_url = values.get('image_base64'), values.get('image_url')
        if not image_base64 and not image_url:
            raise ValueError('Either image_base64 or image_url must be provided')
        if image_base64 and image_url:
            raise ValueError('Provide either image_base64 or image_url, not both')
        return values
    
    class Config:
        json_schema_extra = {
            "example_base64": {
                "image_base64": "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAUA...",
                "enhance_math": True,
                "auto_create": False
            },
            "example_url": {
                "image_url": "http://example.com/image.png",
                "enhance_math": True,
                "auto_create": True
            }
        }


class ReviewRecordCreate(BaseModel):
    """创建复习记录的请求模型"""
    problem_id: str = Field(..., description="错题ID")
    review_result: Optional[str] = Field(None, description="复习结果", pattern="^(correct|incorrect|partial)$")
    confidence_level: Optional[int] = Field(None, description="信心等级", ge=1, le=5)
    time_spent: Optional[int] = Field(None, description="花费时间(秒)", ge=0)
    notes: Optional[str] = Field(None, description="复习笔记")
    
    class Config:
        json_schema_extra = {
            "example": {
                "problem_id": "123e4567-e89b-12d3-a456-426614174000",
                "review_result": "correct",
                "confidence_level": 4,
                "time_spent": 300,
                "notes": "这次终于理解了"
            }
        }


# 响应模型
class ProblemData(BaseModel): # Renamed from ProblemResponse to represent pure data
    """错题数据模型"""
    id: str = Field(..., description="错题ID")
    title: str = Field(..., description="题目标题")
    content: str = Field(..., description="题目内容")
    subject: Subject = Field(..., description="学科")
    category: Optional[str] = Field(None, description="题目分类")
    source: Optional[str] = Field(None, description="题目来源")
    year: Optional[int] = Field(None, description="年份")
    user_answer: Optional[str] = Field(None, description="用户答案")
    correct_answer: Optional[str] = Field(None, description="正确答案")
    error_analysis: Optional[str] = Field(None, description="错误分析")
    solution: Optional[str] = Field(None, description="解题思路")
    ai_analysis: Optional[Dict[str, Any]] = Field(None, description="AI分析结果")
    knowledge_points: List[str] = Field(default_factory=list, description="知识点列表")
    difficulty_level: int = Field(..., description="难度等级", ge=1, le=5)
    image_urls: List[str] = Field(default_factory=list, description="题目图片URL列表")
    ocr_result: Optional[str] = Field(None, description="OCR识别结果")
    review_count: int = Field(..., description="复习次数")
    last_review_at: Optional[datetime] = Field(None, description="最后复习时间")
    mastery_level: float = Field(..., description="掌握程度", ge=0, le=1)
    tags: List[str] = Field(default_factory=list, description="标签列表")
    notes: Optional[str] = Field(None, description="用户备注")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")

    class Config:
        from_attributes = True # For Pydantic V2
        # orm_mode = True # Enable ORM mode for conversion from model instance
        json_schema_extra = {
            "example": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "title": "高等数学-极限计算",
                "content": "计算极限 lim(x->0) sin(x)/x",
                "subject": "math",
                "category": "微积分",
                "difficulty_level": 3,
                "review_count": 2,
                "mastery_level": 0.6,
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-02T00:00:00Z"
            }
        }

class SingleProblemResponse(BaseModel):
    """单个错题的标准API响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: Optional[ProblemData] = None


class ProblemListResponse(BaseModel):
    """错题列表响应"""
    success: bool = True
    data: List[ProblemData] # Changed from List[Dict[str, Any]]
    total: int
    page: int
    size: int
    
    @property
    def pages(self) -> int:
        """总页数"""
        return (self.total + self.size - 1) // self.size
    
    class Config:
        json_schema_extra = {
            "example": {
                "success": True,
                "data": [...],
                "total": 100,
                "page": 1,
                "size": 20
            }
        }


class ReviewRecordData(BaseModel):
    """复习记录数据模型"""
    id: str = Field(..., description="复习记录ID")
    problem_id: str = Field(..., description="错题ID")
    review_result: Optional[str] = Field(None, description="复习结果")
    confidence_level: Optional[int] = Field(None, description="信心等级")
    time_spent: Optional[int] = Field(None, description="花费时间(秒)")
    notes: Optional[str] = Field(None, description="复习笔记")
    ai_feedback: Optional[Dict[str, Any]] = Field(None, description="AI给出的反馈")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")

    class Config:
        from_attributes = True
        json_schema_extra = {
            "example": {
                "id": "rev_123e4567-e89b-12d3-a456-426614174000",
                "problem_id": "prob_123e4567-e89b-12d3-a456-426614174000",
                "review_result": "correct",
                "confidence_level": 4,
                "time_spent": 180,
                "notes": "掌握得更好了",
                "created_at": "2024-01-03T10:00:00Z",
                "updated_at": "2024-01-03T10:05:00Z"
            }
        }


class ReviewRecordResponse(BaseModel):
    """复习记录响应"""
    success: bool
    message: Optional[str] = None
    data: Optional[ReviewRecordData] = None


# 批量操作
class BatchProblemOperation(BaseModel):
    """批量操作请求"""
    problem_ids: List[str] = Field(..., min_items=1, description="题目ID列表")
    operation: str = Field(..., pattern="^(delete|tag|analyze)$", description="操作类型")
    params: Optional[Dict[str, Any]] = Field(None, description="操作参数")


# 统计相关
class ProblemStatisticsData(BaseModel): # Renamed to indicate it's the data part
    """错题统计数据"""
    total_problems: int = Field(..., description="总题目数")
    by_subject: Dict[str, int] = Field(..., description="按学科统计的题目数")
    by_difficulty: Dict[int, int] = Field(..., description="按难度统计的题目数")
    avg_mastery_level: float = Field(..., description="平均掌握程度")
    total_review_count: int = Field(..., description="总复习次数")
    recent_problems: int = Field(..., description="最近7天新增题目数")
    need_review: int = Field(..., description="需要复习的题目数")

    class Config:
        json_schema_extra = {
            "example": {
                "total_problems": 150,
                "by_subject": {"math": 80, "english": 40, "politics": 30},
                "by_difficulty": {1:10, 2:30, 3:70, 4:30, 5:10},
                "avg_mastery_level": 0.65,
                "total_review_count": 300,
                "recent_problems": 20,
                "need_review": 45
            }
        }


class ProblemStatisticsResponse(BaseModel):
    """错题统计响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: Optional[ProblemStatisticsData] = None


class KnowledgePointStatsData(BaseModel): # Renamed to indicate it's the data part
    """单个知识点统计数据"""
    knowledge_point: str = Field(..., description="知识点名称")
    problem_count: int = Field(..., description="相关题目数量")
    avg_mastery: float = Field(..., description="平均掌握度")
    avg_difficulty: float = Field(..., description="平均难度")

    class Config:
        json_schema_extra = {
            "example": {
                "knowledge_point": "洛必达法则",
                "problem_count": 15,
                "avg_mastery": 0.7,
                "avg_difficulty": 3.5
            }
        }

class KnowledgePointStatsListResponse(BaseModel):
    """知识点统计列表响应模型"""
    success: bool = True
    message: Optional[str] = None
    data: List[KnowledgePointStatsData] = Field(default_factory=list)


class ProblemFilterRequest(BaseModel):
    """错题过滤请求模型"""
    problem_ids: List[str] = Field(..., description="错题ID列表", min_items=1)
    operation: str = Field(..., pattern="^(delete|tag|analyze)$", description="操作类型")
    tags: Optional[List[str]] = Field(None, description="标签列表(tag操作需要)")


class ProblemBatchRequest(BaseModel):
    """批量操作请求模型"""
    problem_ids: List[str] = Field(..., description="错题ID列表", min_items=1)
    operation: str = Field(..., description="操作类型", pattern="^(delete|update|analyze)$")
    update_data: Optional[Dict[str, Any]] = Field(None, description="更新数据(仅update操作需要)")
    
    class Config:
        json_schema_extra = {
            "example": {
                "problem_ids": ["id1", "id2", "id3"],
                "operation": "update",
                "update_data": {
                    "tags": ["已复习"],
                    "mastery_level": 0.8
                }
            }
        }


class ProblemQuery(BaseModel):
    """错题查询参数模型"""
    subject: Optional[Subject] = Field(None, description="学科")
    category: Optional[str] = Field(None, description="题目分类")
    tags: Optional[List[str]] = Field(None, description="标签列表")
    keyword: Optional[str] = Field(None, description="关键词搜索")
    min_difficulty: Optional[int] = Field(None, description="最小难度", ge=1, le=5)
    max_difficulty: Optional[int] = Field(None, description="最大难度", ge=1, le=5)
    min_mastery: Optional[float] = Field(None, description="最小掌握度", ge=0, le=1)
    max_mastery: Optional[float] = Field(None, description="最大掌握度", ge=0, le=1)
    page: int = Field(1, description="页码", ge=1)
    size: int = Field(20, description="每页数量", ge=1, le=100)
    sort_by: str = Field("created_at", description="排序字段", pattern="^(created_at|updated_at|difficulty_level|mastery_level|review_count)$")
    sort_desc: bool = Field(True, description="是否降序排序")
    
    class Config:
        json_schema_extra = {
            "example": {
                "subject": "math",
                "category": "微积分",
                "tags": ["重点", "易错"],
                "keyword": "极限",
                "min_difficulty": 2,
                "max_difficulty": 4,
                "min_mastery": 0.3,
                "max_mastery": 0.8,
                "page": 1,
                "size": 20,
                "sort_by": "created_at",
                "sort_desc": True
            }
        }
