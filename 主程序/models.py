"""
数据模型定义
"""
from dataclasses import dataclass
from typing import List, Dict, Any, Optional  

@dataclass
class ErrorProblem:
    """错题模型类"""
    
    id: str = None
    image_path: str = None
    problem_content: str = None
    error_analysis: str = None
    problem_category: str = None
    problem_subcategory: str = None
    error_type: str = None
    difficulty: int = 3
    correct_solution: str = None
    tags: list = None
    created_at: str = None
    notes: str = None
    typicality: int = 3
    additional_images: list = None
    updated_at: str = None
    subject: str = "math"  
    
    def __post_init__(self):
        """初始化后处理，确保列表字段不是None"""
        if self.tags is None:
            self.tags = []
        if self.additional_images is None:
            self.additional_images = []
        
        # 设置默认值
        self.problem_content = self.problem_content or ""
        self.error_analysis = self.error_analysis or ""
        self.problem_category = self.problem_category or "未分类"
        self.problem_subcategory = self.problem_subcategory or ""
        self.error_type = self.error_type or ""
        self.difficulty = self.difficulty or 3
        self.correct_solution = self.correct_solution or ""
        self.notes = self.notes or ""
        self.typicality = self.typicality or 3
        self.subject = self.subject or "math"  
    
    def to_dict(self):
        """转换为字典格式"""
        return {
            'id': self.id,
            'image_path': self.image_path,
            'problem_content': self.problem_content,
            'error_analysis': self.error_analysis,
            'problem_category': self.problem_category,
            'problem_subcategory': self.problem_subcategory,
            'error_type': self.error_type,
            'difficulty': self.difficulty,
            'correct_solution': self.correct_solution,
            'tags': self.tags,
            'created_at': self.created_at,
            'notes': self.notes,
            'typicality': self.typicality,
            'additional_images': self.additional_images,
            'updated_at': self.updated_at,
            'subject': self.subject  
        }
    
    @classmethod
    def from_dict(cls, data):
        """从字典创建实例"""
        return cls(
            id=data.get('id'),
            image_path=data.get('image_path'),
            problem_content=data.get('problem_content'),
            error_analysis=data.get('error_analysis'),
            problem_category=data.get('problem_category'),
            problem_subcategory=data.get('problem_subcategory'),
            error_type=data.get('error_type'),
            difficulty=data.get('difficulty'),
            correct_solution=data.get('correct_solution'),
            tags=data.get('tags'),
            created_at=data.get('created_at'),
            notes=data.get('notes'),
            typicality=data.get('typicality'),
            additional_images=data.get('additional_images'),
            updated_at=data.get('updated_at'),
            subject=data.get('subject', 'math')  
        )

@dataclass
class ReviewSession:
    """回顾分析模型"""
    id: str
    problems_included: List[str]
    review_analysis: Dict[str, Any]
    improvement_strategy: Optional[str]
    created_at: str
