"""
错题管理服务业务逻辑
"""
import base64
import io
from typing import List, Optional, Dict, Any
from datetime import datetime, timedelta
from sqlalchemy import select, func, and_, or_, desc
from sqlalchemy.ext.asyncio import AsyncSession
from PIL import Image

from shared.utils.logger import LoggerMixin
from services.ai_api_manager import ai_router, AIRequest, TaskType
from services.file_service import file_service
from .models import Problem, ReviewRecord, Subject
from .schemas import (
    ProblemCreate, ProblemUpdate, ProblemAnalyzeRequest,
    ProblemOCRRequest, ReviewRecordCreate
)


class ProblemService(LoggerMixin):
    """错题管理服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
    
    async def create_problem(
        self,
        data: ProblemCreate,
        auto_analyze: bool = True
    ) -> Problem:
        """创建错题"""
        try:
            # 处理图片上传
            image_urls = []
            if data.image_base64:
                for idx, image_b64 in enumerate(data.image_base64):
                    # 上传到文件服务
                    image_data = base64.b64decode(image_b64)
                    filename = f"problem_{datetime.now().timestamp()}_{idx}.png"
                    url = await file_service.upload_image(
                        image_data, filename, "problems"
                    )
                    image_urls.append(url)
            
            # 创建问题实例
            problem_dict = data.dict(exclude={"image_base64"})
            problem_dict["image_urls"] = image_urls
            problem_dict["knowledge_points"] = []
            problem = Problem(**problem_dict)
            
            self.db.add(problem)
            await self.db.commit()
            await self.db.refresh(problem)
            
            # 自动AI分析
            if auto_analyze and (problem.content or problem.image_urls):
                await self.analyze_problem(problem.id)
                await self.db.refresh(problem)
            
            self.log_info(f"Created problem: {problem.id}")
            return problem
            
        except Exception as e:
            self.log_error(f"Failed to create problem: {e}")
            await self.db.rollback()
            raise
    
    async def get_problem(self, problem_id: str) -> Optional[Problem]:
        """获取单个错题"""
        result = await self.db.execute(
            select(Problem).where(
                and_(
                    Problem.id == problem_id,
                    Problem.deleted_at.is_(None)
                )
            )
        )
        return result.scalar_one_or_none()
    
    async def list_problems(
        self,
        subject: Optional[Subject] = None,
        category: Optional[str] = None,
        tags: Optional[List[str]] = None,
        keyword: Optional[str] = None,
        difficulty_range: Optional[tuple[int, int]] = None,
        mastery_range: Optional[tuple[float, float]] = None,
        page: int = 1,
        size: int = 20,
        sort_by: str = "created_at",
        sort_desc: bool = True
    ) -> tuple[List[Problem], int]:
        """获取错题列表"""
        # 构建查询
        query = select(Problem).where(Problem.deleted_at.is_(None))
        
        # 应用过滤条件
        if subject:
            query = query.where(Problem.subject == subject)
        
        if category:
            query = query.where(Problem.category == category)
        
        if tags:
            # 检查是否包含任意标签
            tag_conditions = []
            for tag in tags:
                tag_conditions.append(
                    func.json_contains(Problem.tags, f'"{tag}"')
                )
            query = query.where(or_(*tag_conditions))
        
        if keyword:
            # 在标题、内容、备注中搜索
            query = query.where(
                or_(
                    Problem.title.contains(keyword),
                    Problem.content.contains(keyword),
                    Problem.notes.contains(keyword)
                )
            )
        
        if difficulty_range:
            min_diff, max_diff = difficulty_range
            query = query.where(
                and_(
                    Problem.difficulty_level >= min_diff,
                    Problem.difficulty_level <= max_diff
                )
            )
        
        if mastery_range:
            min_mastery, max_mastery = mastery_range
            query = query.where(
                and_(
                    Problem.mastery_level >= min_mastery,
                    Problem.mastery_level <= max_mastery
                )
            )
        
        # 计算总数
        count_query = select(func.count()).select_from(query.subquery())
        total = await self.db.scalar(count_query)
        
        # 排序
        sort_column = getattr(Problem, sort_by, Problem.created_at)
        if sort_desc:
            query = query.order_by(desc(sort_column))
        else:
            query = query.order_by(sort_column)
        
        # 分页
        query = query.offset((page - 1) * size).limit(size)
        
        result = await self.db.execute(query)
        problems = result.scalars().all()
        
        return problems, total
    
    async def update_problem(
        self,
        problem_id: str,
        data: ProblemUpdate
    ) -> Optional[Problem]:
        """更新错题"""
        problem = await self.get_problem(problem_id)
        if not problem:
            return None
        
        # 更新字段
        update_data = data.dict(exclude_unset=True)
        for field, value in update_data.items():
            setattr(problem, field, value)
        
        await self.db.commit()
        await self.db.refresh(problem)
        
        self.log_info(f"Updated problem: {problem_id}")
        return problem
    
    async def delete_problem(self, problem_id: str) -> bool:
        """软删除错题"""
        problem = await self.get_problem(problem_id)
        if not problem:
            return False
        
        problem.soft_delete()
        await self.db.commit()
        
        self.log_info(f"Soft deleted problem: {problem_id}")
        return True
    
    async def analyze_problem(self, problem_id: str) -> Dict[str, Any]:
        """使用AI分析错题"""
        problem = await self.get_problem(problem_id)
        if not problem:
            raise ValueError("Problem not found")
        
        # 构建AI请求
        content = {
            "problem_content": problem.content,
            "user_answer": problem.user_answer,
            "subject": problem.subject,
            "notes": problem.notes
        }
        
        # 如果有图片，添加到请求中
        if problem.image_urls:
            # TODO: 从文件服务获取图片的base64编码
            pass
        
        # 调用AI API
        ai_request = AIRequest(
            task_type=TaskType.PROBLEM_ANALYSIS,
            content=content
        )
        
        response = await ai_router.route_request(ai_request)
        
        if response.success:
            # 解析AI响应
            ai_analysis = response.content
            
            # 更新问题
            problem.ai_analysis = ai_analysis
            
            # 提取结构化信息
            if isinstance(ai_analysis, dict):
                problem.error_analysis = ai_analysis.get("error_analysis", "")
                problem.correct_answer = ai_analysis.get("correct_answer", "")
                problem.solution = ai_analysis.get("solution", "")
                problem.knowledge_points = ai_analysis.get("knowledge_points", [])
                problem.difficulty_level = ai_analysis.get("difficulty_level", 3)
            
            await self.db.commit()
            
            self.log_info(f"Analyzed problem: {problem_id}")
            return ai_analysis
        else:
            raise Exception(f"AI analysis failed: {response.error}")
    
    async def ocr_image(self, data: ProblemOCRRequest) -> Dict[str, Any]:
        """OCR识别图片"""
        # 调用AI API进行OCR
        ai_request = AIRequest(
            task_type=TaskType.OCR,
            content={
                "image_base64": data.image_base64,
                "enhance_math": data.enhance_math
            }
        )
        
        response = await ai_router.route_request(ai_request)
        
        if response.success:
            ocr_result = response.content
            
            # 如果需要自动创建题目
            if data.auto_create and ocr_result:
                problem_data = ProblemCreate(
                    title="OCR识别题目",
                    content=ocr_result,
                    subject=Subject.MATH,  # 默认数学，可以通过AI判断
                    image_base64=[data.image_base64]
                )
                problem = await self.create_problem(problem_data)
                
                return {
                    "ocr_result": ocr_result,
                    "problem_id": str(problem.id),
                    "created": True
                }
            
            return {
                "ocr_result": ocr_result,
                "created": False
            }
        else:
            raise Exception(f"OCR failed: {response.error}")
    
    async def add_review_record(
        self,
        data: ReviewRecordCreate
    ) -> ReviewRecord:
        """添加复习记录"""
        problem = await self.get_problem(data.problem_id)
        if not problem:
            raise ValueError("Problem not found")
        
        # 创建复习记录
        record = ReviewRecord(
            problem_id=problem.id,
            review_result=data.review_result,
            confidence_level=data.confidence_level,
            time_spent=data.time_spent,
            notes=data.notes
        )
        
        # 更新问题统计
        problem.review_count += 1
        problem.last_review_at = datetime.now()
        
        # 更新掌握程度
        if data.review_result == "correct":
            # 答对提升掌握度
            problem.mastery_level = min(
                1.0,
                problem.mastery_level + 0.1 * (data.confidence_level / 5)
            )
        elif data.review_result == "incorrect":
            # 答错降低掌握度
            problem.mastery_level = max(
                0.0,
                problem.mastery_level - 0.2
            )
        else:  # partial
            # 部分正确小幅提升
            problem.mastery_level = min(
                1.0,
                problem.mastery_level + 0.05
            )
        
        self.db.add(record)
        await self.db.commit()
        await self.db.refresh(record)
        
        self.log_info(f"Added review record for problem: {data.problem_id}")
        return record
    
    async def get_statistics(self) -> Dict[str, Any]:
        """获取统计信息"""
        # 总题目数
        total_query = select(func.count(Problem.id)).where(
            Problem.deleted_at.is_(None)
        )
        total_problems = await self.db.scalar(total_query)
        
        # 按学科统计
        subject_stats = {}
        for subject in Subject:
            count = await self.db.scalar(
                select(func.count(Problem.id)).where(
                    and_(
                        Problem.subject == subject.value,
                        Problem.deleted_at.is_(None)
                    )
                )
            )
            subject_stats[subject.value] = count
        
        # 按难度统计
        difficulty_stats = {}
        for level in range(1, 6):
            count = await self.db.scalar(
                select(func.count(Problem.id)).where(
                    and_(
                        Problem.difficulty_level == level,
                        Problem.deleted_at.is_(None)
                    )
                )
            )
            difficulty_stats[level] = count
        
        # 平均掌握程度
        avg_mastery = await self.db.scalar(
            select(func.avg(Problem.mastery_level)).where(
                Problem.deleted_at.is_(None)
            )
        ) or 0.0
        
        # 总复习次数
        total_reviews = await self.db.scalar(
            select(func.sum(Problem.review_count)).where(
                Problem.deleted_at.is_(None)
            )
        ) or 0
        
        # 最近7天添加的题目
        recent_date = datetime.now() - timedelta(days=7)
        recent_problems = await self.db.scalar(
            select(func.count(Problem.id)).where(
                and_(
                    Problem.created_at >= recent_date,
                    Problem.deleted_at.is_(None)
                )
            )
        )
        
        # 需要复习的题目（掌握度低于0.8且超过3天未复习）
        review_date = datetime.now() - timedelta(days=3)
        need_review = await self.db.scalar(
            select(func.count(Problem.id)).where(
                and_(
                    Problem.mastery_level < 0.8,
                    or_(
                        Problem.last_review_at < review_date,
                        Problem.last_review_at.is_(None)
                    ),
                    Problem.deleted_at.is_(None)
                )
            )
        )
        
        return {
            "total_problems": total_problems,
            "by_subject": subject_stats,
            "by_difficulty": difficulty_stats,
            "avg_mastery_level": float(avg_mastery),
            "total_review_count": total_reviews,
            "recent_problems": recent_problems,
            "need_review": need_review
        }
    
    async def get_knowledge_point_stats(self) -> List[Dict[str, Any]]:
        """获取知识点统计"""
        # 查询所有问题的知识点
        problems = await self.db.execute(
            select(
                Problem.knowledge_points,
                Problem.mastery_level,
                Problem.difficulty_level
            ).where(
                and_(
                    Problem.deleted_at.is_(None),
                    Problem.knowledge_points.isnot(None)
                )
            )
        )
        
        # 统计每个知识点
        knowledge_stats = {}
        for knowledge_points, mastery, difficulty in problems:
            if knowledge_points:
                for kp in knowledge_points:
                    if kp not in knowledge_stats:
                        knowledge_stats[kp] = {
                            "count": 0,
                            "total_mastery": 0,
                            "total_difficulty": 0
                        }
                    
                    knowledge_stats[kp]["count"] += 1
                    knowledge_stats[kp]["total_mastery"] += mastery
                    knowledge_stats[kp]["total_difficulty"] += difficulty
        
        # 计算平均值
        result = []
        for kp, stats in knowledge_stats.items():
            result.append({
                "knowledge_point": kp,
                "problem_count": stats["count"],
                "avg_mastery": stats["total_mastery"] / stats["count"],
                "avg_difficulty": stats["total_difficulty"] / stats["count"]
            })
        
        # 按题目数量排序
        result.sort(key=lambda x: x["problem_count"], reverse=True)
        
        return result
    
    async def batch_import(
        self,
        problems: List[ProblemCreate],
        auto_analyze: bool = True
    ) -> Dict[str, Any]:
        """批量导入题目"""
        total = len(problems)
        success = 0
        failed = 0
        errors = []
        created_ids = []
        
        for idx, problem_data in enumerate(problems):
            try:
                problem = await self.create_problem(
                    problem_data,
                    auto_analyze=auto_analyze
                )
                success += 1
                created_ids.append(str(problem.id))
            except Exception as e:
                failed += 1
                errors.append({
                    "index": idx,
                    "error": str(e)
                })
                self.log_error(f"Failed to import problem {idx}: {e}")
        
        return {
            "total": total,
            "success": success,
            "failed": failed,
            "errors": errors,
            "created_ids": created_ids
        } 