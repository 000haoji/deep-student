"""
回顾分析服务的Pydantic模型
"""
from typing import List, Optional, Dict, Any
from datetime import date, datetime
from pydantic import BaseModel, Field, validator
from uuid import UUID

from .models import AnalysisType


# 请求模型
class BatchAnalysisRequest(BaseModel):
    """批量分析请求"""
    problem_ids: List[str] = Field(..., min_items=1, description="要分析的题目ID列表")
    title: Optional[str] = Field(None, description="分析标题")
    description: Optional[str] = Field(None, description="分析描述")
    focus_areas: Optional[List[str]] = Field(None, description="重点关注的领域")
    
    class Config:
        json_schema_extra = {
            "example": {
                "problem_ids": ["id1", "id2", "id3"],
                "title": "微积分错题专项分析",
                "focus_areas": ["计算错误", "概念理解"]
            }
        }


class KnowledgePointAnalysisRequest(BaseModel):
    """知识点分析请求"""
    knowledge_points: List[str] = Field(..., min_items=1, description="要分析的知识点")
    date_range: Optional[Dict[str, date]] = Field(None, description="时间范围")
    min_problem_count: int = Field(5, ge=1, description="最少题目数量")
    
    class Config:
        json_schema_extra = {
            "example": {
                "knowledge_points": ["极限", "导数", "积分"],
                "date_range": {
                    "start": "2024-01-01",
                    "end": "2024-12-31"
                },
                "min_problem_count": 10
            }
        }


class TimePeriodAnalysisRequest(BaseModel):
    """时间段分析请求"""
    start_date: date = Field(..., description="开始日期")
    end_date: date = Field(..., description="结束日期")
    subjects: Optional[List[str]] = Field(None, description="限定学科")
    
    @validator("end_date")
    def validate_date_range(cls, v, values):
        if "start_date" in values and v < values["start_date"]:
            raise ValueError("结束日期不能早于开始日期")
        return v
    
    class Config:
        json_schema_extra = {
            "example": {
                "start_date": "2024-10-01",
                "end_date": "2024-12-31",
                "subjects": ["math", "english"]
            }
        }


class ComprehensiveAnalysisRequest(BaseModel):
    """综合分析请求"""
    include_all_problems: bool = Field(True, description="是否包含所有题目")
    subjects: Optional[List[str]] = Field(None, description="限定学科")
    min_mastery: Optional[float] = Field(None, ge=0, le=1, description="最低掌握度")
    max_mastery: Optional[float] = Field(None, ge=0, le=1, description="最高掌握度")
    focus_on_weaknesses: bool = Field(True, description="是否重点关注薄弱环节")
    
    class Config:
        json_schema_extra = {
            "example": {
                "include_all_problems": False,
                "subjects": ["math"],
                "max_mastery": 0.6,
                "focus_on_weaknesses": True
            }
        }


class FollowUpRequest(BaseModel):
    """跟进请求"""
    analysis_id: Optional[str] = Field(None, description="分析ID")
    action_taken: str = Field(..., description="采取的行动")
    result: str = Field(..., description="结果描述")
    effectiveness_score: int = Field(..., ge=1, le=10, description="效果评分")
    reviewed_problem_ids: Optional[List[str]] = Field(None, description="复习的题目ID")
    notes: Optional[str] = Field(None, description="备注")
    
    class Config:
        json_schema_extra = {
            "example": {
                "action_taken": "重点复习了极限计算方法",
                "result": "掌握了泰勒展开的正确使用",
                "effectiveness_score": 8,
                "reviewed_problem_ids": ["id1", "id2"],
                "notes": "需要继续练习复杂函数的展开"
            }
        }


# 响应模型
class ReviewAnalysisResponse(BaseModel):
    """分析响应"""
    success: bool
    message: Optional[str] = None
    data: Dict[str, Any]
    
    class Config:
        json_schema_extra = {
            "example": {
                "success": True,
                "message": "分析完成",
                "data": {
                    "id": "analysis-id",
                    "title": "分析标题",
                    "analysis_type": "batch_problems",
                    "importance_score": 8.5
                }
            }
        }


class AnalysisListResponse(BaseModel):
    """分析列表响应"""
    success: bool
    data: List[Dict[str, Any]]
    total: int
    page: int
    size: int
    
    @property
    def pages(self) -> int:
        """总页数"""
        return (self.total + self.size - 1) // self.size


class FollowUpResponse(BaseModel):
    """跟进响应"""
    success: bool
    message: Optional[str] = None
    data: Dict[str, Any]


# 洞察相关
class InsightResponse(BaseModel):
    """学习洞察响应"""
    insight_type: str
    title: str
    description: str
    supporting_data: Dict[str, Any]
    importance: float
    actionable: bool
    recommended_actions: Optional[List[str]] = None


# 统计相关
class AnalysisStatistics(BaseModel):
    """分析统计"""
    total_analyses: int
    by_type: Dict[str, int]
    avg_importance: float
    avg_urgency: float
    total_follow_ups: int
    effectiveness_rate: float


class AnalysisCreate(BaseModel):
    """创建分析请求模型"""
    title: str = Field(..., description="分析标题", max_length=200)
    analysis_type: AnalysisType = Field(..., description="分析类型")
    description: Optional[str] = Field(None, description="分析描述")
    problem_ids: List[str] = Field(..., description="要分析的题目ID列表", min_items=1)
    date_range: Optional[Dict[str, str]] = Field(None, description="分析时间范围")
    
    class Config:
        json_schema_extra = {
            "example": {
                "title": "高等数学期末复习分析",
                "analysis_type": "comprehensive",
                "description": "分析本学期高数错题，准备期末考试",
                "problem_ids": ["id1", "id2", "id3"],
                "date_range": {"start": "2024-01-01", "end": "2024-06-30"}
            }
        }


class AnalysisUpdate(BaseModel):
    """更新分析请求模型"""
    title: Optional[str] = Field(None, description="分析标题", max_length=200)
    description: Optional[str] = Field(None, description="分析描述")
    importance_score: Optional[float] = Field(None, description="重要性评分", ge=0, le=10)
    urgency_score: Optional[float] = Field(None, description="紧急程度评分", ge=0, le=10)
    
    class Config:
        json_schema_extra = {
            "example": {
                "title": "更新后的分析标题",
                "importance_score": 8.5,
                "urgency_score": 7.0
            }
        }


class AnalysisResponse(BaseModel):
    """分析响应模型"""
    id: str = Field(..., description="分析ID")
    title: str = Field(..., description="分析标题")
    analysis_type: AnalysisType = Field(..., description="分析类型")
    description: Optional[str] = Field(None, description="分析描述")
    problem_ids: List[str] = Field(..., description="分析的题目ID列表")
    subjects: List[str] = Field(..., description="涉及的学科")
    knowledge_points: List[str] = Field(..., description="涉及的知识点")
    date_range: Optional[Dict[str, str]] = Field(None, description="时间范围")
    total_problems: int = Field(..., description="题目总数")
    error_patterns: Optional[List[Dict[str, Any]]] = Field(None, description="错误模式")
    weakness_areas: Optional[List[Dict[str, Any]]] = Field(None, description="薄弱环节")
    improvement_suggestions: Optional[List[str]] = Field(None, description="改进建议")
    study_plan: Optional[List[Dict[str, Any]]] = Field(None, description="学习计划")
    avg_difficulty: float = Field(..., description="平均难度")
    avg_mastery: float = Field(..., description="平均掌握度")
    common_mistakes: Optional[Dict[str, int]] = Field(None, description="常见错误统计")
    ai_analysis: Optional[Dict[str, Any]] = Field(None, description="AI分析原始数据")
    importance_score: float = Field(..., description="重要性评分")
    urgency_score: float = Field(..., description="紧急程度评分")
    created_at: datetime = Field(..., description="创建时间")
    updated_at: datetime = Field(..., description="更新时间")
    
    class Config:
        json_schema_extra = {
            "example": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "title": "高等数学期末复习分析",
                "analysis_type": "comprehensive",
                "total_problems": 50,
                "avg_difficulty": 3.5,
                "avg_mastery": 0.65,
                "importance_score": 8.5,
                "urgency_score": 9.0,
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-02T00:00:00Z"
            }
        }


class FollowUpCreate(BaseModel):
    """创建跟进记录请求模型"""
    analysis_id: str = Field(..., description="分析ID")
    action_taken: str = Field(..., description="采取的行动")
    result: Optional[str] = Field(None, description="结果")
    effectiveness_score: float = Field(default=5.0, description="效果评分", ge=0, le=10)
    reviewed_problem_ids: Optional[List[str]] = Field(None, description="复习的题目ID")
    notes: Optional[str] = Field(None, description="备注")
    
    class Config:
        json_schema_extra = {
            "example": {
                "analysis_id": "123e4567-e89b-12d3-a456-426614174000",
                "action_taken": "完成了微积分专项练习",
                "result": "正确率从60%提升到85%",
                "effectiveness_score": 8.0,
                "reviewed_problem_ids": ["id1", "id2"],
                "notes": "重点复习了泰勒展开和洛必达法则"
            }
        }


class LearningPatternResponse(BaseModel):
    """学习模式响应模型"""
    id: str = Field(..., description="模式ID")
    pattern_name: str = Field(..., description="模式名称")
    pattern_type: str = Field(..., description="模式类型")
    description: str = Field(..., description="模式描述")
    pattern_data: Dict[str, Any] = Field(..., description="模式数据")
    confidence_score: float = Field(..., description="置信度")
    applicable_subjects: Optional[List[str]] = Field(None, description="适用学科")
    applicable_knowledge_points: Optional[List[str]] = Field(None, description="适用知识点")
    recommendations: Optional[List[str]] = Field(None, description="相关建议")
    occurrence_count: int = Field(..., description="出现次数")
    last_seen_at: datetime = Field(..., description="最后出现时间")
    created_at: datetime = Field(..., description="创建时间")
    
    class Config:
        json_schema_extra = {
            "example": {
                "id": "123e4567-e89b-12d3-a456-426614174000",
                "pattern_name": "计算粗心",
                "pattern_type": "error_pattern",
                "description": "在计算过程中经常出现低级错误",
                "confidence_score": 0.85,
                "occurrence_count": 15,
                "recommendations": [
                    "做题时放慢速度",
                    "计算完成后进行检查",
                    "使用草稿纸规范计算过程"
                ],
                "last_seen_at": "2024-01-15T10:30:00Z",
                "created_at": "2024-01-01T00:00:00Z"
            }
        }


class AnalysisSummaryResponse(BaseModel):
    """分析摘要响应模型"""
    total_analyses: int = Field(..., description="分析总数")
    analyses_by_type: Dict[str, int] = Field(..., description="按类型统计")
    recent_analyses: List[Dict[str, Any]] = Field(..., description="最近的分析")
    top_patterns: List[Dict[str, Any]] = Field(..., description="主要模式")
    overall_progress: Dict[str, float] = Field(..., description="整体进展")
    
    class Config:
        json_schema_extra = {
            "example": {
                "total_analyses": 25,
                "analyses_by_type": {
                    "single_problem": 10,
                    "batch_problems": 8,
                    "comprehensive": 7
                },
                "recent_analyses": [
                    {
                        "id": "id1",
                        "title": "高数期末分析",
                        "created_at": "2024-01-15T00:00:00Z"
                    }
                ],
                "top_patterns": [
                    {
                        "pattern": "计算错误",
                        "frequency": 0.35
                    }
                ],
                "overall_progress": {
                    "mastery_improvement": 0.15,
                    "error_reduction": 0.25
                }
            }
        } 