"""
错题管理服务的Pydantic模型
"""
from typing import List, Optional, Dict, Any
from datetime import datetime
from pydantic import BaseModel, Field, validator
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
    image_base64: str = Field(..., description="Base64编码的图片")
    enhance_math: bool = Field(True, description="是否增强数学公式识别")
    auto_create: bool = Field(False, description="是否自动创建题目")


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
class ProblemResponse(BaseModel):
    """错题响应模型"""
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


class ProblemListResponse(BaseModel):
    """错题列表响应"""
    success: bool
    data: List[Dict[str, Any]]
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


class ReviewRecordResponse(BaseModel):
    """复习记录响应"""
    success: bool
    message: Optional[str] = None
    data: Dict[str, Any]


# 批量操作
class BatchProblemOperation(BaseModel):
    """批量操作请求"""
    problem_ids: List[str] = Field(..., min_items=1, description="题目ID列表")
    operation: str = Field(..., pattern="^(delete|tag|analyze)$", description="操作类型")
    params: Optional[Dict[str, Any]] = Field(None, description="操作参数")


# 统计相关
class ProblemStatistics(BaseModel):
    """错题统计"""
    total_problems: int
    by_subject: Dict[str, int]
    by_difficulty: Dict[int, int]
    avg_mastery_level: float
    total_review_count: int
    recent_problems: int
    need_review: int


class KnowledgePointStats(BaseModel):
    """知识点统计"""
    knowledge_point: str
    problem_count: int
    avg_mastery: float
    avg_difficulty: float


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