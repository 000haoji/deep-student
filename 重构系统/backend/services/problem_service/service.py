"""
错题管理服务业务逻辑
"""
import base64
import io
import time
import uuid # For generating session IDs
from typing import List, Optional, Dict, Any, AsyncGenerator
from datetime import datetime, timedelta
import httpx # For downloading image from URL
from sqlalchemy import select, func, and_, or_, desc
from sqlalchemy.ext.asyncio import AsyncSession
from PIL import Image
import json # For SSE data

from shared.utils.logger import LoggerMixin
from ..file_service import file_service
from ..ai_api_manager.schemas import AIRequestSchema # Use the new schema
from ..ai_api_manager.models import TaskType # TaskType can still be from models or schemas if defined there
from ..ai_api_manager.service import AIModelService # Import AIService to use it directly
from .models import (
    Problem, ReviewRecord, Subject, ProblemTag, ProblemCategory, ProblemAIChatLog,
    ProblemAISession, ProblemAISessionStatus # Import new models
)
from .schemas import (
    ProblemCreate, ProblemUpdate,
    ProblemOCRRequest, ReviewRecordCreate, ProblemQuery,
    ProblemTagCreate, ProblemTagUpdate,
    ProblemCategoryCreate, ProblemCategoryUpdate,
    ProblemData,
    ProblemBatchRequest,
    # AI-Driven Workflow Schemas
    ProblemAICreateInitiateRequest, ProblemAIStructuredData,
    AIChatLogEntryCreateSchema, ProblemAIFinalizeRequest,
    ProblemAIStreamRequest, AIChatMessage,
    ProblemAISessionCreate, ProblemAISessionData, ProblemAISessionUpdate # Import new schemas
)
from sqlalchemy.orm import selectinload # For eager loading


class ProblemService(LoggerMixin):
    """错题管理服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
        # AIService will be instantiated on demand or could be passed via DI
    
    async def _get_ai_service(self) -> AIModelService:
        # Helper to get AI service instance, avoids repeated instantiation if not needed
        # Or could be initialized in __init__ if ProblemService is created per request.
        # For simplicity, creating it here when needed.
        return AIModelService(self.db)

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
                        image_data=image_data,
                        filename=filename,
                        category="problems",
                        db=self.db  # 传递数据库会话
                    )
                    image_urls.append(url)
            
            # 创建问题实例
            problem_dict = data.dict(exclude={"image_base64"})
            problem_dict["image_urls"] = image_urls
            problem_dict["knowledge_points"] = []
            # user_id is removed from Problem model

            problem = Problem(**problem_dict)
            
            self.db.add(problem)
            await self.db.commit()
            await self.db.refresh(problem)
            
            # 自动AI分析
            if auto_analyze and (problem.content or problem.image_urls):
                await self.analyze_problem(problem.id)
            await self.db.refresh(problem) # Refresh to get AI analysis results
            
            self.log_info(f"Created problem: {problem.id}")
            return problem
            
        except Exception as e:
            self.log_error(f"Failed to create problem: {e}")
            await self.db.rollback()
            raise

    async def _get_problem_by_id_internal(self, problem_id: str) -> Optional[Problem]:
        """
        内部获取单个错题方法，不进行用户ID校验。
        主要用于服务内部确认题目是否存在等场景。
        """
        result = await self.db.execute(
            select(Problem).where(
                and_(
                    Problem.id == problem_id,
                    Problem.deleted_at.is_(None)
                )
            )
        )
        return result.scalar_one_or_none()

    async def get_problem_by_id(self, problem_id: str) -> Optional[Problem]:
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
        query = select(Problem).where(
            Problem.deleted_at.is_(None)
        )
            
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
        problem = await self.get_problem_by_id(problem_id)
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

    # ProblemCategory Service Methods
    async def create_problem_category(self, data: ProblemCategoryCreate) -> ProblemCategory:
        """创建题目分类"""
        # 检查同一学科下是否存在同名分类
        existing_category = await self.db.execute(
            select(ProblemCategory).where(
                and_(
                    ProblemCategory.name == data.name,
                    ProblemCategory.subject == data.subject
                )
            )
        )
        if existing_category.scalar_one_or_none():
            raise ValueError(f"Category with name '{data.name}' already exists in subject '{data.subject.value}'.")

        # 检查父分类是否存在 (如果提供了 parent_id)
        if data.parent_id:
            parent_category = await self.get_problem_category_by_id(data.parent_id)
            if not parent_category:
                raise ValueError(f"Parent category with ID '{data.parent_id}' not found.")
            if parent_category.subject != data.subject:
                raise ValueError("Parent category must belong to the same subject.")

        category = ProblemCategory(**data.dict())
        self.db.add(category)
        await self.db.commit()
        await self.db.refresh(category)
        self.log_info(f"Created problem category: {category.id} - {category.name}")
        return category

    async def get_problem_category_by_id(self, category_id: str) -> Optional[ProblemCategory]:
        """通过ID获取题目分类"""
        result = await self.db.execute(
            select(ProblemCategory).where(ProblemCategory.id == category_id)
        )
        return result.scalar_one_or_none()

    async def get_problem_category_by_subject_and_name(self, subject: Subject, name: str) -> Optional[ProblemCategory]:
        """通过学科和名称获取题目分类"""
        result = await self.db.execute(
            select(ProblemCategory).where(
                and_(
                    ProblemCategory.subject == subject,
                    ProblemCategory.name == name
                )
            )
        )
        return result.scalar_one_or_none()

    async def _build_category_tree(self, categories: List[ProblemCategory], parent_id: Optional[str] = None) -> List[ProblemCategory]:
        """内部辅助函数，用于构建分类树"""
        tree = []
        for category in categories:
            if category.parent_id == parent_id:
                # 为了在返回的 Pydantic 模型中也包含 children，我们需要确保 ORM 模型的 children 属性被加载
                # 如果 children 是通过 backref 自动填充的，并且在序列化时 Pydantic 从 ORM 实例转换，
                # 那么这里递归构建时，children 应该能被正确访问。
                # 如果需要显式加载，可以使用 selectinload。但对于简单递归，直接访问通常可行。
                
                # 递归构建子树
                children_nodes = await self._build_category_tree(categories, category.id)
                # The ProblemCategory ORM model has a `children` relationship.
                # When converting to Pydantic (ProblemCategoryData), this relationship should be used.
                # Here, we are returning ORM objects. The Pydantic schema `ProblemCategoryData`
                # has `children: List['ProblemCategoryData']`.
                # If the API returns `ProblemCategoryData`, the conversion from ORM to Pydantic
                # should handle populating `children` based on the relationship.
                # For now, this function returns ORM objects, and the tree structure is implicit.
                # If the API needs to return a fully populated tree of Pydantic models,
                # the conversion step in the API endpoint needs to handle this.
                
                # Let's assume for now that the relationship will be handled by Pydantic's from_orm
                # when the API endpoint serializes the response.
                # The `children` attribute on the ORM model should be populated by SQLAlchemy's relationship.
                # We might need to ensure they are loaded if lazy loading is an issue.
                # For simplicity, let's assume they are available.
                
                # If we want to explicitly set the children on the ORM model for this tree construction:
                # category.children = children_nodes # This would modify the ORM instance directly.
                                                # This is generally fine if these are transient for tree building.

                tree.append(category) # Add the category itself
                # The children are part of the category object due to the SQLAlchemy relationship
        return tree


    async def list_problem_categories(
        self,
        subject: Optional[Subject] = None,
        parent_id: Optional[str] = "root", # "root" to fetch top-level, None to fetch all flat, or specific parent_id
        hierarchical: bool = True # If true, attempts to return a tree structure for top-level items
    ) -> List[ProblemCategory]: # Returns ORM models
        """
        获取题目分类列表。
        - subject: 按学科过滤。
        - parent_id: 'root' 表示获取顶级分类, None 表示获取所有分类（扁平化）, 或指定父ID获取其直接子分类。
        - hierarchical: 若为True且parent_id='root', 则尝试构建并返回树状结构的顶级分类及其子分类。
                       若为False, 或parent_id不是'root', 则返回扁平列表。
        """
        query = select(ProblemCategory).order_by(ProblemCategory.subject, ProblemCategory.order, ProblemCategory.name)

        if subject:
            query = query.where(ProblemCategory.subject == subject)

        if parent_id == "root": # Fetch top-level categories
            query = query.where(ProblemCategory.parent_id.is_(None))
            results = await self.db.execute(query)
            top_level_categories = list(results.scalars().all())

            if hierarchical:
                # If hierarchical, fetch all categories for the subject (if specified) and build tree
                all_categories_query = select(ProblemCategory).order_by(ProblemCategory.subject, ProblemCategory.order, ProblemCategory.name)
                if subject:
                    all_categories_query = all_categories_query.where(ProblemCategory.subject == subject)
                
                all_results = await self.db.execute(all_categories_query)
                all_categories_list = list(all_results.scalars().all())
                
                # Build tree from all categories, starting with roots
                # The _build_category_tree helper needs to be adapted or the logic embedded here.
                # For now, let's simplify: if hierarchical, we fetch all and the Pydantic model with from_orm
                # will use the relationships to form the tree.
                # The key is that ProblemCategory.children relationship is correctly defined.
                
                # A more direct way to build the tree:
                # 1. Fetch all relevant categories.
                # 2. Create a dictionary of categories by ID.
                # 3. Iterate and assign children to their parents.
                
                # For now, returning top-level. The Pydantic schema `ProblemCategoryData`
                # with `children: List['ProblemCategoryData']` and `from_attributes=True`
                # should automatically populate children if the SQLAlchemy relationships are loaded.
                # We might need `selectinload(ProblemCategory.children)` for eager loading if issues arise.
                # For simplicity, let's assume lazy loading or Pydantic's ORM mode handles it.
                # The API endpoint will convert these ORM objects to Pydantic schemas.
                
                # To make `hierarchical` truly effective here at the service layer returning ORM objects
                # with children populated, we would need to recursively query or use SQLAlchemy's features
                # like joinedload/selectinload for the children relationship, potentially to a certain depth.

                # Let's refine this: if hierarchical, we fetch all and then filter for roots,
                # relying on SQLAlchemy relationships for children to be available for Pydantic.
                if subject:
                    all_categories_stmt = select(ProblemCategory).filter(ProblemCategory.subject == subject).order_by(ProblemCategory.order, ProblemCategory.name)
                else:
                    all_categories_stmt = select(ProblemCategory).order_by(ProblemCategory.order, ProblemCategory.name)
                
                # Eagerly load children and their children recursively if needed.
                # For one level: options(selectinload(ProblemCategory.children))
                # For multiple levels, it gets more complex or use joinedload.
                # from sqlalchemy.orm import selectinload
                # all_categories_stmt = all_categories_stmt.options(selectinload(ProblemCategory.children))


                all_categories_result = await self.db.execute(all_categories_stmt)
                all_categories = list(all_categories_result.scalars().all())
                
                # Build a map and then construct the tree
                category_map = {cat.id: cat for cat in all_categories}
                # Reset children for all to avoid duplicates if this function is called multiple times with same objects
                for cat_id in category_map:
                    category_map[cat_id].children = [] # Assuming ProblemCategory ORM has a children list attribute

                root_categories = []
                for cat in all_categories:
                    if cat.parent_id is None:
                        root_categories.append(cat)
                    elif cat.parent_id in category_map:
                        parent_cat = category_map[cat.parent_id]
                        # Ensure parent_cat.children is a list that can be appended to
                        if not hasattr(parent_cat, 'children_list_for_tree'): # Use a temporary attribute
                            parent_cat.children_list_for_tree = []
                        parent_cat.children_list_for_tree.append(cat) # This populates a temp list

                # The Pydantic model ProblemCategoryData uses `children`.
                # The ORM model ProblemCategory has `children` via backref.
                # So, if relationships are loaded, Pydantic should pick them up.
                # The manual tree building above into `children_list_for_tree` is one way if direct relationship
                # loading isn't sufficient or if we want to control the structure explicitly.
                # For now, let's rely on the ORM relationship and Pydantic's from_orm.
                # The `top_level_categories` fetched earlier are the ones to return.
                # Their `children` attribute should be populated by SQLAlchemy if accessed.
                return top_level_categories # Return only top-level; Pydantic will handle nesting via relationships

        elif parent_id is None: # Fetch all categories (flat list)
            results = await self.db.execute(query)
            return list(results.scalars().all())
        else: # Fetch direct children of a specific parent
            query = query.where(ProblemCategory.parent_id == parent_id)
            results = await self.db.execute(query)
            return list(results.scalars().all())


    async def update_problem_category(self, category_id: str, data: ProblemCategoryUpdate) -> Optional[ProblemCategory]:
        """更新题目分类"""
        category = await self.get_problem_category_by_id(category_id)
        if not category:
            return None

        update_data = data.dict(exclude_unset=True)

        # Check for name uniqueness if name or subject is changed
        new_name = update_data.get("name", category.name)
        # Subject cannot be changed in this update schema, so we use existing category.subject
        
        if "name" in update_data and new_name != category.name:
            existing_category = await self.db.execute(
                select(ProblemCategory).where(
                    and_(
                        ProblemCategory.name == new_name,
                        ProblemCategory.subject == category.subject, # Use existing subject
                        ProblemCategory.id != category_id
                    )
                )
            )
            if existing_category.scalar_one_or_none():
                raise ValueError(f"Category with name '{new_name}' already exists in subject '{category.subject.value}'.")

        # Prevent creating circular dependencies with parent_id
        if "parent_id" in update_data:
            new_parent_id = update_data["parent_id"]
            if new_parent_id:
                if new_parent_id == category_id:
                    raise ValueError("Category cannot be its own parent.")
                # Check if new_parent_id is one of the current category's children (deep check)
                # This requires traversing the children tree, which can be complex.
                # For now, a simple check:
                current_child_ids = [child.id for child in category.children] # Assuming children are loaded
                if new_parent_id in current_child_ids:
                     raise ValueError("Cannot set parent to one of its own children.")
                
                # Also ensure parent exists and is in the same subject
                parent_category = await self.get_problem_category_by_id(new_parent_id)
                if not parent_category:
                    raise ValueError(f"New parent category with ID '{new_parent_id}' not found.")
                if parent_category.subject != category.subject:
                     raise ValueError("New parent category must belong to the same subject.")
            # If new_parent_id is None, it's becoming a top-level category, which is fine.


        for field, value in update_data.items():
            setattr(category, field, value)

        await self.db.commit()
        await self.db.refresh(category)
        self.log_info(f"Updated problem category: {category_id}")
        return category

    async def delete_problem_category(self, category_id: str) -> bool:
        """删除题目分类"""
        category = await self.get_problem_category_by_id(category_id)
        if not category:
            return False

        # Check if category has children
        children_count = await self.db.scalar(
            select(func.count(ProblemCategory.id)).where(ProblemCategory.parent_id == category_id)
        )
        if children_count > 0:
            # Option 1: Prevent deletion
            raise ValueError("Cannot delete category with child categories. Please reassign or delete children first.")
            # Option 2: Promote children (set parent_id to None) - requires more logic
            # Option 3: Cascade delete (dangerous, not implemented here)

        # Check if category is in use by problems (usage_count)
        # The Problem.category field is a string, not a direct FK.
        # So, we rely on the usage_count if it's accurately maintained.
        # For now, we'll assume usage_count is advisory.
        # A stricter check would query Problems table:
        # problem_usage_count = await self.db.scalar(
        #     select(func.count(Problem.id)).where(Problem.category == category.name, Problem.subject == category.subject)
        # )
        # if problem_usage_count > 0:
        #     raise ValueError(f"Category '{category.name}' is in use by {problem_usage_count} problems and cannot be deleted.")

        await self.db.delete(category)
        await self.db.commit()
        self.log_info(f"Deleted problem category: {category_id}")
        return True

    async def increment_category_usage(self, subject: Subject, category_name: str):
        """增加分类使用计数"""
        if not category_name: return
        category = await self.get_problem_category_by_subject_and_name(subject, category_name)
        if not category:
            # Category does not exist, create it
            try:
                new_category_data = ProblemCategoryCreate(name=category_name, subject=subject, description="Automatically created by AI suggestion or problem assignment.")
                category = await self.create_problem_category(new_category_data) # create_problem_category handles commit and refresh
                category.usage_count = 1 # Initial usage
                self.log_info(f"Auto-created category '{category_name}' for subject '{subject.value}' with usage 1.")
            except ValueError as ve: # Handles if create_problem_category raises due to concurrent creation (e.g. unique constraint)
                self.log_warning(f"Failed to auto-create category '{category_name}' for subject '{subject.value}': {ve}. Trying to fetch again.")
                category = await self.get_problem_category_by_subject_and_name(subject, category_name)
                if category: # If fetched successfully after a potential race condition
                    category.usage_count += 1
                else: # Still not found, log error and skip increment
                    self.log_error(f"Failed to find or create category '{category_name}' for subject '{subject.value}' after creation attempt. Usage not incremented.")
                    return 
        else:
            category.usage_count += 1
        
        await self.db.commit()
        await self.db.refresh(category)


    async def decrement_category_usage(self, subject: Subject, category_name: str):
        """减少分类使用计数"""
        if not category_name: return
        category = await self.get_problem_category_by_subject_and_name(subject, category_name)
        if category:
            category.usage_count = max(0, category.usage_count - 1)
            await self.db.commit()
            await self.db.refresh(category)
        else:
            self.log_warning(f"Attempted to decrement usage for non-existent category '{category_name}' in subject '{subject.value}'.")

    # Need to update create_problem and update_problem to use these category usage methods
    # This requires Problem.category to be linked to ProblemCategory.id or a (subject, name) tuple.
    # Currently Problem.category is just a string.
    # For accurate usage_count, Problem.category should ideally be a foreign key to ProblemCategory.id.
    # If Problem.category remains a string, then increment/decrement logic needs to be called
    # explicitly when a problem's category string changes.

    # Let's adjust create_problem and update_problem assuming Problem.category is a string name
    # and we call these increment/decrement methods.

    async def create_problem(
        self,
        data: ProblemCreate,
        auto_analyze: bool = True
    ) -> Problem:
        """创建错题 (覆盖之前的方法以包含 category usage)"""
        try:
            # 处理图片上传
            image_urls = []
            if data.image_base64:
                for idx, image_b64 in enumerate(data.image_base64):
                    image_data = base64.b64decode(image_b64)
                    filename = f"problem_{datetime.now().timestamp()}_{idx}.png"
                    url = await file_service.upload_image(
                        image_data=image_data, filename=filename, category="problems", db=self.db
                    )
                    image_urls.append(url)
            
            problem_dict = data.dict(exclude={"image_base64", "tags"})
            problem_dict["image_urls"] = image_urls
            problem_dict["knowledge_points"] = []
            
            problem = Problem(**problem_dict)
            
            if data.tags:
                problem.tags = list(set(data.tags))
                await self.increment_tag_usage(problem.tags)

            if data.category: # If a category is specified
                await self.increment_category_usage(data.subject, data.category)

            self.db.add(problem)
            await self.db.commit()
            await self.db.refresh(problem)
            
            if auto_analyze and (problem.content or problem.image_urls):
                await self.analyze_problem(problem.id)
            await self.db.refresh(problem) 
            
            self.log_info(f"Created problem: {problem.id}")
            return problem
            
        except Exception as e:
            self.log_error(f"Failed to create problem: {e}")
            await self.db.rollback()
            raise

    async def update_problem(
        self,
        problem_id: str,
        data: ProblemUpdate
    ) -> Optional[Problem]:
        """更新错题 (覆盖之前的方法以包含 category usage)"""
        problem = await self.get_problem_by_id(problem_id)
        if not problem:
            return None
        
        original_tags = list(problem.tags) if problem.tags else []
        original_category_name = problem.category
        original_subject = problem.subject # Subject of the problem

        update_data = data.dict(exclude_unset=True)
        
        new_tags_from_update = None
        if "tags" in update_data:
            new_tags_from_update = list(set(update_data.pop("tags")))

        new_category_name = update_data.get("category", original_category_name)
        # Note: ProblemUpdate schema does not allow changing subject. If it did, logic would be more complex.

        for field, value in update_data.items():
            setattr(problem, field, value)
        
        # Handle tags update
        if new_tags_from_update is not None:
            tags_to_add = [tag for tag in new_tags_from_update if tag not in original_tags]
            tags_to_remove = [tag for tag in original_tags if tag not in new_tags_from_update]
            if tags_to_add: await self.increment_tag_usage(tags_to_add)
            if tags_to_remove: await self.decrement_tag_usage(tags_to_remove)
            problem.tags = new_tags_from_update

        # Handle category update
        if "category" in update_data: # If category field was in the update payload
            if original_category_name and original_category_name != new_category_name:
                await self.decrement_category_usage(original_subject, original_category_name)
            if new_category_name and new_category_name != original_category_name:
                # Subject for increment is the problem's current subject (which cannot be changed by this update)
                await self.increment_category_usage(problem.subject, new_category_name)
            # If new_category_name is None and original was not, decrement original.
            # If new_category_name is set and original was None, increment new.
            # The logic above covers changes. If new_category_name is None, problem.category becomes None.

        await self.db.commit()
        await self.db.refresh(problem)
        
        self.log_info(f"Updated problem: {problem_id}")
        return problem
    
    async def export_problems(
        self,
        subject: Optional[Subject] = None,
        category: Optional[str] = None,
        tags: Optional[List[str]] = None,
        keyword: Optional[str] = None,
        difficulty_range: Optional[tuple[int, int]] = None,
        mastery_range: Optional[tuple[float, float]] = None,
        sort_by: str = "created_at",
        sort_desc: bool = True
    ) -> List[ProblemData]: # Returns a list of Pydantic models ready for serialization
        """获取用于导出的错题数据列表 (不分页)"""
        # 构建查询，与 list_problems 类似，但不分页
        query = select(Problem).where(
            Problem.deleted_at.is_(None)
        )
            
        if subject:
            query = query.where(Problem.subject == subject)
        if category:
            query = query.where(Problem.category == category)
        if tags:
            tag_conditions = [func.json_contains(Problem.tags, f'"{tag}"') for tag in tags]
            query = query.where(or_(*tag_conditions))
        if keyword:
            query = query.where(
                or_(
                    Problem.title.contains(keyword),
                    Problem.content.contains(keyword),
                    Problem.notes.contains(keyword)
                )
            )
        if difficulty_range:
            min_diff, max_diff = difficulty_range
            query = query.where(and_(Problem.difficulty_level >= min_diff, Problem.difficulty_level <= max_diff))
        if mastery_range:
            min_mastery, max_mastery = mastery_range
            query = query.where(and_(Problem.mastery_level >= min_mastery, Problem.mastery_level <= max_mastery))
        
        # 排序
        sort_column = getattr(Problem, sort_by, Problem.created_at)
        if sort_desc:
            query = query.order_by(desc(sort_column))
        else:
            query = query.order_by(sort_column)
        
        result = await self.db.execute(query)
        problems_orm = result.scalars().all()
        
        # 将ORM模型转换为Pydantic模型
        problems_data = [ProblemData.from_orm(p) for p in problems_orm]
        
        self.log_info(f"Exporting {len(problems_data)} problems.")
        return problems_data

    async def delete_problem(self, problem_id: str) -> bool:
        """软删除错题"""
        problem = await self.get_problem_by_id(problem_id)
        if not problem:
            return False
        
        problem.soft_delete()
        await self.db.commit()
        
        self.log_info(f"Soft deleted problem: {problem_id}")
        return True

    async def batch_operate_problems(self, batch_request: ProblemBatchRequest) -> Dict[str, Any]:
        """批量操作错题"""
        problem_ids = batch_request.problem_ids
        operation = batch_request.operation
        update_data_dict = batch_request.update_data

        results = {"successful_ids": [], "failed_ids": [], "errors": []}

        problems_to_operate = await self.db.execute(
            select(Problem).where(Problem.id.in_(problem_ids), Problem.deleted_at.is_(None))
        )
        problems_map = {str(p.id): p for p in problems_to_operate.scalars().all()}

        for problem_id_str in problem_ids:
            problem = problems_map.get(problem_id_str)
            if not problem:
                results["failed_ids"].append(problem_id_str)
                results["errors"].append({"id": problem_id_str, "error": "Problem not found or already deleted."})
                continue

            try:
                if operation == "delete":
                    problem.soft_delete()
                    # No need to call self.delete_problem as it commits individually
                    results["successful_ids"].append(problem_id_str)
                
                elif operation == "analyze":
                    # Note: analyze_problem itself commits. This might be slow in a batch.
                    # Consider making analyze_problem not commit and commit at the end of batch.
                    # For now, it will commit per problem.
                    analysis_outcome = await self.analyze_problem(str(problem.id))
                    if analysis_outcome.get("status") == "success":
                        results["successful_ids"].append(problem_id_str)
                    else:
                        results["failed_ids"].append(problem_id_str)
                        results["errors"].append({"id": problem_id_str, "error": analysis_outcome.get("message", "AI analysis failed")})
                
                elif operation == "update":
                    if not update_data_dict:
                        results["failed_ids"].append(problem_id_str)
                        results["errors"].append({"id": problem_id_str, "error": "Update data not provided for update operation."})
                        continue
                    
                    # Use ProblemUpdate schema for validation if update_data_dict structure matches it
                    # Or apply changes directly if schema is simple enough
                    
                    original_tags = list(problem.tags) if problem.tags else []
                    original_category_name = problem.category
                    original_subject = problem.subject

                    new_tags_from_update = None
                    if "tags" in update_data_dict:
                        new_tags_from_update = list(set(update_data_dict.get("tags", []))) # Ensure list

                    new_category_name = update_data_dict.get("category", original_category_name)

                    for field, value in update_data_dict.items():
                        if field == "tags": # Tags handled separately below
                            continue
                        if hasattr(problem, field):
                            setattr(problem, field, value)
                    
                    # Handle tags update for batch
                    if new_tags_from_update is not None:
                        tags_to_add_batch = [tag for tag in new_tags_from_update if tag not in original_tags]
                        tags_to_remove_batch = [tag for tag in original_tags if tag not in new_tags_from_update]
                        
                        # These tag usage methods commit. This is problematic for batch.
                        # Ideally, collect all tag changes and update counts once.
                        # For now, this will cause multiple commits if many problems change tags.
                        if tags_to_add_batch: await self.increment_tag_usage(tags_to_add_batch)
                        if tags_to_remove_batch: await self.decrement_tag_usage(tags_to_remove_batch)
                        problem.tags = new_tags_from_update

                    # Handle category update for batch
                    if "category" in update_data_dict:
                        if original_category_name and original_category_name != new_category_name:
                            await self.decrement_category_usage(original_subject, original_category_name)
                        if new_category_name and new_category_name != original_category_name:
                            await self.increment_category_usage(problem.subject, new_category_name)
                    
                    results["successful_ids"].append(problem_id_str)
                else:
                    results["failed_ids"].append(problem_id_str)
                    results["errors"].append({"id": problem_id_str, "error": "Invalid batch operation."})

            except Exception as e:
                self.log_error(f"Error during batch operation '{operation}' for problem {problem.id}: {e}")
                results["failed_ids"].append(problem_id_str)
                results["errors"].append({"id": problem_id_str, "error": str(e)})
                # Rollback for this specific problem's changes if possible,
                # but the structure here commits after each sub-operation or at the end.
                # Better to collect all changes and commit once.
        
        try:
            await self.db.commit() # Commit all changes made (e.g., soft deletes, direct updates)
        except Exception as e:
            self.log_error(f"Error committing batch operations: {e}")
            await self.db.rollback()
            # Mark all as failed if commit fails? Or just report commit error.
            results["commit_error"] = str(e)
            # This is tricky; some operations might have committed individually (like AI analysis).
            # For robustness, operations that modify DB should not commit individually within the loop.

        self.log_info(f"Batch operation '{operation}' completed. Success: {len(results['successful_ids'])}, Failed: {len(results['failed_ids'])}")
        return results

    async def analyze_problem(self, problem_id: str) -> Dict[str, Any]:
        """使用AI分析错题"""
        problem = await self.get_problem_by_id(problem_id)
        if not problem:
            self.log_warning(f"Problem with ID {problem_id} not found.")
            raise ValueError(f"Problem not found.")
        
        try:
            # 准备分析内容
            content = problem.content or ""
            if problem.image_urls:
                # 如果有图片，先进行OCR
                ocr_results = []
                for image_url in problem.image_urls:
                    # Pass user_id to ocr_image if it might auto-create (though here it's for analysis, not creation)
                    # For analysis, auto_create should be False.
                    ocr_request = ProblemOCRRequest(image_url=image_url, auto_create=False)
                    ocr_result = await self.ocr_image(ocr_request)
                    if ocr_result.get("text"):
                        ocr_results.append(ocr_result["text"])
                content += "\n".join(ocr_results)
            
            # 构建AI分析请求
            # Ensure content is passed as a dictionary for AIRequest
            ai_content_payload = {"text": content}
            if problem.subject:
                subject_value = problem.subject.value if hasattr(problem.subject, 'value') else str(problem.subject)
                ai_content_payload["subject"] = subject_value

            analysis_request = AIRequest(
                task_type=TaskType.PROBLEM_ANALYSIS,
                content=ai_content_payload,
                metadata={
                    "problem_id": str(problem.id),
                    "subject": problem.subject,
                    "category": problem.category,
                    "user_answer": problem.user_answer,
                    "correct_answer": problem.correct_answer
                }
            )
            
            ai_content_payload = {"text": content}
            if problem.subject:
                subject_value = problem.subject.value if hasattr(problem.subject, 'value') else str(problem.subject)
                ai_content_payload["subject"] = subject_value

            analysis_request = AIRequestSchema( # Use AIRequestSchema
                task_type=TaskType.PROBLEM_ANALYSIS,
                prompt=json.dumps(ai_content_payload), # Pass structured content as JSON string in prompt or use context
                # Or better: use context field in AIRequestSchema
                # context={"text": content, "subject": subject_value, ... other problem fields ...}
                # Let's adjust to use context properly
                context={
                    "text": content,
                    "subject": subject_value,
                    "user_answer": problem.user_answer, # Assuming these are available on problem model
                    "correct_answer": problem.correct_answer
                }
                # metadata is for AIService logging, not AI provider directly
            )
            
            ai_service = await self._get_ai_service()
            analysis_response_data: AIResponseDataSchema = await ai_service.process_ai_request(analysis_request)
            
            if not analysis_response_data.success or not analysis_response_data.result_json:
                error_message = analysis_response_data.error_message or "Unknown AI analysis error"
                self.log_error(f"AI analysis failed for problem {problem_id}: {error_message}")
                
                if hasattr(problem, 'ai_analysis_error'):
                    problem.ai_analysis_error = error_message 
                    await self.db.commit()

                self.log_warning(f"AI analysis failed for problem {problem_id}, AI fields not updated. Error: {error_message}")
                return {
                    "status": "ai_failed",
                    "problem_id": str(problem_id),
                    "message": f"AI analysis failed: {error_message}",
                    "analysis": None
                }

            analysis_data_dict = analysis_response_data.result_json # result_json should be a dict
            
            if not isinstance(analysis_data_dict, dict):
                self.log_error(f"AI analysis for problem {problem_id} returned non-dict JSON: {analysis_data_dict}")
                if hasattr(problem, 'ai_analysis_error'):
                    problem.ai_analysis_error = "AI returned non-dictionary JSON structure."
                    await self.db.commit()
                return {
                    "status": "ai_failed",
                    "problem_id": str(problem_id),
                    "message": "AI analysis successful, but returned non-dictionary JSON.",
                    "analysis": None
                }
            
            new_tags = analysis_data_dict.get("tags", problem.tags if problem.tags else [])
            if isinstance(new_tags, list): # Ensure it's a list
                original_tags = list(problem.tags) if problem.tags else []
                tags_to_add = [tag for tag in new_tags if tag not in original_tags]
                tags_to_remove = [tag for tag in original_tags if tag not in new_tags]
                if tags_to_add: await self.increment_tag_usage(tags_to_add)
                if tags_to_remove: await self.decrement_tag_usage(tags_to_remove)
                problem.tags = new_tags
            
            problem.knowledge_points = analysis_data_dict.get("knowledge_points", problem.knowledge_points or [])
            problem.difficulty_level = analysis_data_dict.get("difficulty_level", problem.difficulty_level or 3)
            problem.error_analysis = analysis_data_dict.get("error_analysis", problem.error_analysis)
            problem.solution = analysis_data_dict.get("solution", problem.solution)
            problem.ai_analysis = analysis_data_dict

            suggested_category_name = analysis_data_dict.get("suggested_category")
            if suggested_category_name and isinstance(suggested_category_name, str):
                old_category_name = problem.category
                if old_category_name != suggested_category_name:
                    problem.category = suggested_category_name
                    if old_category_name:
                        await self.decrement_category_usage(problem.subject, old_category_name)
                    await self.increment_category_usage(problem.subject, suggested_category_name)

            if hasattr(problem, 'ai_analysis_error'):
                problem.ai_analysis_error = None
            
            await self.db.commit()
            await self.db.refresh(problem)
            
            self.log_info(f"Successfully analyzed problem: {problem_id}")
            # analysis_result was not defined. Use analysis_data_dict from AI.
            return {
                "status": "success",
                "problem_id": str(problem_id),
                "analysis": analysis_data_dict 
            }
            
        except Exception as e:
            self.log_error(f"Failed to analyze problem {problem_id}: {str(e)}")
            await self.db.rollback()
            raise
    
    async def ocr_image(self, data: ProblemOCRRequest) -> Dict[str, Any]:
        """OCR识别图片,可以接收base64或者图片URL"""
        image_b64_for_ai = None
        original_image_for_problem_creation = None # Store the base64 of the original image for auto_create

        if data.image_base64:
            image_b64_for_ai = data.image_base64
            original_image_for_problem_creation = data.image_base64
        elif data.image_url:
            try:
                async with httpx.AsyncClient() as client:
                    response = await client.get(str(data.image_url))
                    response.raise_for_status() # Raise an exception for bad status codes
                    image_bytes = await response.aread()
                    image_b64_for_ai = base64.b64encode(image_bytes).decode('utf-8')
                    original_image_for_problem_creation = image_b64_for_ai # Store for potential problem creation
            except httpx.HTTPStatusError as e:
                self.log_error(f"Failed to download image from URL {data.image_url} for OCR: {e}")
                raise Exception(f"Failed to download image from URL: {data.image_url}")
            except Exception as e:
                self.log_error(f"An error occurred while processing image URL {data.image_url} for OCR: {e}")
                raise Exception(f"Error processing image URL: {data.image_url}")
        
        if not image_b64_for_ai:
            # This case should be prevented by Pydantic model validation, but as a safeguard:
            raise ValueError("No image data provided for OCR (neither base64 nor valid URL).")

        # Corrected block: Call AI API using AIRequestSchema
        ai_ocr_direct_request = AIRequestSchema( 
            task_type=TaskType.OCR,
            image_base64=image_b64_for_ai, # Pass base64 directly
            # image_url could also be used if AI provider supports it and base64 is not preferred
            context={"enhance_math": data.enhance_math}
        )
        
        ai_service = await self._get_ai_service()
        ocr_response_data: AIResponseDataSchema = await ai_service.process_ai_request(ai_ocr_direct_request)
        
        if ocr_response_data.success and ocr_response_data.result_text is not None:
            ocr_text_result = ocr_response_data.result_text
            
            if data.auto_create and ocr_text_result:
                problem_create_payload = ProblemCreate(
                    title=f"OCR 题目 - {datetime.now().strftime('%Y-%m-%d %H:%M')}",
                    content=ocr_text_result,
                    subject=data.subject or Subject.MATH,
                    image_base64=[original_image_for_problem_creation] if original_image_for_problem_creation else [],
                    category=data.category,
                )
                # Pass auto_analyze=False to prevent re-analysis immediately for OCR'd problem
                created_problem = await self.create_problem(problem_create_payload, auto_analyze=False)
                
                return {
                    "text": ocr_text_result,
                    "problem_id": str(created_problem.id),
                    "created": True,
                    "message": "OCR successful and problem created."
                }
            
            return {
                "text": ocr_text_result,
                "created": False,
                "message": "OCR successful."
            }
        else:
            error_msg = ocr_response_data.error_message or "OCR failed with unknown error"
            self.log_error(f"OCR failed: {error_msg}. AI Response: {ocr_response_data.dict()}")
            raise Exception(f"OCR failed: {error_msg}")
    
    async def add_review_record(
        self,
        data: ReviewRecordCreate
    ) -> ReviewRecord:
        """添加复习记录"""
        # 确保题目存在
        problem = await self.get_problem_by_id(data.problem_id)
        if not problem:
            self.log_warning(f"Attempt to add review for non-existent problem {data.problem_id}")
            raise ValueError("Problem not found.")
        
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
            select(func.sum(Problem.review_count)).where( # This sums review_count from Problem table
                Problem.deleted_at.is_(None)
            )
            # If you need to count ReviewRecord entries instead:
            # select(func.count(ReviewRecord.id)).join(Problem).where(
            #     and_(Problem.deleted_at.is_(None))
            # )
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
                    Problem.knowledge_points.isnot(None) # Ensure knowledge_points is not NULL
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

    # ProblemTag Service Methods
    async def create_problem_tag(self, data: ProblemTagCreate) -> ProblemTag:
        """创建题目标签"""
        # Check if tag with the same name already exists
        existing_tag = await self.db.execute(
            select(ProblemTag).where(ProblemTag.name == data.name)
        )
        if existing_tag.scalar_one_or_none():
            raise ValueError(f"Tag with name '{data.name}' already exists.")

        tag = ProblemTag(**data.dict())
        self.db.add(tag)
        await self.db.commit()
        await self.db.refresh(tag)
        self.log_info(f"Created problem tag: {tag.id} - {tag.name}")
        return tag

    async def get_problem_tag_by_id(self, tag_id: str) -> Optional[ProblemTag]:
        """通过ID获取题目标签"""
        result = await self.db.execute(
            select(ProblemTag).where(ProblemTag.id == tag_id)
        )
        return result.scalar_one_or_none()

    async def get_problem_tag_by_name(self, name: str) -> Optional[ProblemTag]:
        """通过名称获取题目标签"""
        result = await self.db.execute(
            select(ProblemTag).where(ProblemTag.name == name)
        )
        return result.scalar_one_or_none()

    async def list_problem_tags(self, page: int = 1, size: int = 100) -> tuple[List[ProblemTag], int]:
        """获取题目标签列表 (分页)"""
        query = select(ProblemTag).order_by(ProblemTag.name) # Order by name by default
        
        count_query = select(func.count()).select_from(query.subquery())
        total = await self.db.scalar(count_query)
        
        query = query.offset((page - 1) * size).limit(size)
        
        result = await self.db.execute(query)
        tags = result.scalars().all()
        return tags, total

    async def update_problem_tag(self, tag_id: str, data: ProblemTagUpdate) -> Optional[ProblemTag]:
        """更新题目标签"""
        tag = await self.get_problem_tag_by_id(tag_id)
        if not tag:
            return None
        
        update_data = data.dict(exclude_unset=True)
        
        # If name is being updated, check for uniqueness
        if "name" in update_data and update_data["name"] != tag.name:
            existing_tag = await self.get_problem_tag_by_name(update_data["name"])
            if existing_tag and existing_tag.id != tag_id:
                raise ValueError(f"Tag with name '{update_data['name']}' already exists.")

        for field, value in update_data.items():
            setattr(tag, field, value)
        
        await self.db.commit()
        await self.db.refresh(tag)
        self.log_info(f"Updated problem tag: {tag_id}")
        return tag

    async def delete_problem_tag(self, tag_id: str) -> bool:
        """删除题目标签"""
        tag = await self.get_problem_tag_by_id(tag_id)
        if not tag:
            return False
        
        # Before deleting a tag, consider implications:
        # 1. How to handle problems that currently use this tag?
        #    - Remove the tag from all problems' 'tags' list.
        #    - Prevent deletion if usage_count > 0.
        # For now, let's implement a simple delete. Updating Problem.tags would be complex here.
        # A more robust solution would involve a background task or careful handling during problem updates.
        # The `ProblemTag.usage_count` is not automatically managed by these basic CRUDs.
        # It should be updated when problems' tags are modified.

        await self.db.delete(tag)
        await self.db.commit()
        self.log_info(f"Deleted problem tag: {tag_id}")
        return True
    
    async def increment_tag_usage(self, tag_names: List[str]):
        """增加标签使用计数"""
        if not tag_names:
            return
        for tag_name in set(tag_names): # Use set to avoid duplicate increments for same tag in one call
            tag = await self.get_problem_tag_by_name(tag_name)
            if tag:
                tag.usage_count += 1
            else:
                # Optionally create the tag if it doesn't exist
                # For now, we assume tags are pre-created or managed separately.
                # Or, this could be a place to auto-create tags.
                # Let's try auto-creating if not found.
                new_tag_data = ProblemTagCreate(name=tag_name)
                tag = await self.create_problem_tag(new_tag_data) # create_problem_tag handles commit
                tag.usage_count = 1 # Set to 1 as it's newly created and used once
        await self.db.commit()

    async def decrement_tag_usage(self, tag_names: List[str]):
        """减少标签使用计数"""
        if not tag_names:
            return
        for tag_name in set(tag_names):
            tag = await self.get_problem_tag_by_name(tag_name)
            if tag:
                tag.usage_count = max(0, tag.usage_count - 1)
        await self.db.commit()

    # Modify create_problem and update_problem to use these helper methods
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
                        image_data=image_data,
                        filename=filename,
                        category="problems",
                        db=self.db  # 传递数据库会话
                    )
                    image_urls.append(url)
            
            # 创建问题实例
            problem_dict = data.dict(exclude={"image_base64", "tags"}) # Exclude tags for now
            problem_dict["image_urls"] = image_urls
            problem_dict["knowledge_points"] = []
            
            problem = Problem(**problem_dict)
            
            # Handle tags separately to update usage_count
            if data.tags:
                problem.tags = list(set(data.tags)) # Ensure unique tags
                await self.increment_tag_usage(problem.tags)

            self.db.add(problem)
            await self.db.commit() # Commit problem first
            await self.db.refresh(problem)
            
            # 自动AI分析
            if auto_analyze and (problem.content or problem.image_urls):
                await self.analyze_problem(problem.id) # This might commit again
            await self.db.refresh(problem) 
            
            self.log_info(f"Created problem: {problem.id}")
            return problem
            
        except Exception as e:
            self.log_error(f"Failed to create problem: {e}")
            await self.db.rollback()
            raise

    async def update_problem(
        self,
        problem_id: str,
        data: ProblemUpdate
    ) -> Optional[Problem]:
        """更新错题"""
        problem = await self.get_problem_by_id(problem_id)
        if not problem:
            return None
        
        original_tags = list(problem.tags) if problem.tags else []
        
        # 更新字段
        update_data = data.dict(exclude_unset=True)
        
        new_tags_from_update = None
        if "tags" in update_data:
            new_tags_from_update = list(set(update_data.pop("tags"))) # Ensure unique, remove from update_data

        for field, value in update_data.items():
            setattr(problem, field, value)
        
        # Handle tags update
        if new_tags_from_update is not None:
            tags_to_add = [tag for tag in new_tags_from_update if tag not in original_tags]
            tags_to_remove = [tag for tag in original_tags if tag not in new_tags_from_update]
            
            if tags_to_add:
                await self.increment_tag_usage(tags_to_add)
            if tags_to_remove:
                await self.decrement_tag_usage(tags_to_remove)
            
            problem.tags = new_tags_from_update # Set the new list of tags

        await self.db.commit()
        await self.db.refresh(problem)
        
        self.log_info(f"Updated problem: {problem_id}")
        return problem

    # --- AI-Driven Problem Creation Workflow Methods ---

    async def initiate_ai_problem_creation(
        self,
        request_data: ProblemAICreateInitiateRequest
    ) -> ProblemAIStructuredData:
        """
        阶段一：AI驱动错题创建 - 初始化。
        接收图片，调用AI进行初步OCR和结构化分析。
        """
        self.log_info(f"Initiating AI problem creation. Subject hint: {request_data.subject_hint}")
        image_b64_content: Optional[str] = None
        uploaded_image_url: Optional[str] = None

        try:
            if request_data.image_base64:
                image_b64_content = request_data.image_base64.split(',')[-1] # Remove data:image/... part if present
                image_data_bytes = base64.b64decode(image_b64_content)
            elif request_data.image_url:
                async with httpx.AsyncClient() as client:
                    response = await client.get(str(request_data.image_url))
                    response.raise_for_status()
                    image_data_bytes = await response.aread()
                image_b64_content = base64.b64encode(image_data_bytes).decode('utf-8') # For AI provider if it needs b64
            else:
                # This case should be caught by Pydantic validation in ProblemAICreateInitiateRequest
                raise ValueError("No image data provided (base64 or URL).")

            # 1. 上传图片到文件服务 (可选，但推荐，以获取稳定的URL或ID)
            # filename = f"ai_problem_init_{datetime.now().timestamp()}.png" # TODO: determine extension from b64 or URL
            # For simplicity, let's assume PNG for now or that file_service handles it.
            try:
                # Attempt to determine file extension
                img = Image.open(io.BytesIO(image_data_bytes))
                extension = img.format.lower() if img.format else "png"
            except Exception:
                extension = "png" # Default if format detection fails
            
            filename = f"ai_init_{uuid.uuid4()}.{extension}"

            uploaded_image_url = await file_service.upload_image(
                image_data=image_data_bytes,
                filename=filename,
                category="ai_problem_creation",
                db=self.db
            )
            self.log_info(f"Image uploaded to file service: {uploaded_image_url}")

            # 2. 准备AI请求 (session_id for AI context will be the new ProblemAISession.id)
            # Placeholder for session ID for now, will be replaced by ProblemAISession.id
            temp_session_id_for_ai_context = str(uuid.uuid4()) # Temporary, will be replaced

            ai_stage1_request = AIRequestSchema(
                task_type=TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON,
                image_base64=image_b64_content,
                context={
                    "subject_hint": request_data.subject_hint.value if request_data.subject_hint else None,
                    "session_id": temp_session_id_for_ai_context, # Pass a session concept to AI if it uses it
                    "uploaded_image_url": uploaded_image_url # Pass image ref to AI context
                }
            )

            # 3. 调用AI API 管理器
            ai_service = await self._get_ai_service()
            self.log_info(f"Calling AI service for task {TaskType.PROBLEM_IMAGE_TO_STRUCTURED_JSON} with temp context session_id {temp_session_id_for_ai_context}")
            ai_response_data: AIResponseDataSchema = await ai_service.process_ai_request(ai_stage1_request)

            if not ai_response_data.success or not isinstance(ai_response_data.result_json, dict):
                error_msg = ai_response_data.error_message or "AI stage 1 analysis failed or returned invalid JSON content."
                self.log_error(f"AI stage 1 (structured JSON) failed. AI Response: {ai_response_data.dict()}")
                raise Exception(f"AI processing (Stage 1) failed: {error_msg}")

            structured_ai_data_dict = ai_response_data.result_json
            self.log_info(f"AI stage 1 (structured JSON) successful. Raw AI Data: {structured_ai_data_dict}")

            # 4. 创建 ProblemAISession 记录
            new_ai_session = ProblemAISession(
                initial_image_ref=str(uploaded_image_url) if uploaded_image_url else None, # Ensure str
                initial_subject_hint=request_data.subject_hint,
                current_structured_data=structured_ai_data_dict, # Store the raw JSON from AI
                status=ProblemAISessionStatus.ACTIVE
                # id will be auto-generated by BaseModel
            )
            self.db.add(new_ai_session)
            await self.db.commit()
            await self.db.refresh(new_ai_session)
            session_id = new_ai_session.id # This is the actual session ID to be used
            self.log_info(f"Created ProblemAISession with ID: {session_id}")


            # 5. 构建并返回 ProblemAIStructuredData (Pydantic model for response)
            # Ensure all fields expected by ProblemAIStructuredData are present or handled.
            # The AI response (structured_ai_data_dict) should ideally match the fields.
            
            # Ensure the session_id in the returned structured data matches the new ProblemAISession.id
            response_structured_data = ProblemAIStructuredData(
                session_id=session_id, # Use the actual session ID from the created DB record
                raw_ocr_text=structured_ai_data_dict.get("raw_ocr_text"),
                extracted_content=structured_ai_data_dict.get("extracted_content"),
                suggested_subject=structured_ai_data_dict.get("suggested_subject"),
                preliminary_category=structured_ai_data_dict.get("preliminary_category"),
                preliminary_tags=structured_ai_data_dict.get("preliminary_tags", []),
                image_regions_of_interest=structured_ai_data_dict.get("image_regions_of_interest"),
                detected_language=structured_ai_data_dict.get("detected_language"),
                original_image_ref=str(uploaded_image_url) if uploaded_image_url else None # Store the URL from file service
            )
            
            # Update the session's current_structured_data with this Pydantic model's dict form
            # to ensure it has the correct session_id field as well.
            new_ai_session.current_structured_data = response_structured_data.model_dump()
            await self.db.commit()
            await self.db.refresh(new_ai_session)

            return response_structured_data

        except Exception as e:
            self.log_error(f"Error in initiate_ai_problem_creation: {e}", exc_info=True)
            # Consider specific error handling or re-raising
            raise

    async def handle_ai_interactive_analysis_stream(
        self,
        session_id_str: str, # Changed from session_id to match API call
        request_payload: ProblemAIStreamRequest # Changed from payload to match API call
    ) -> AsyncGenerator[Dict[str, Any], None]: # Yielding Dicts for SSE
        """
        阶段二：AI驱动错题创建 - 处理与AI的交互式分析流。
        使用Server-Sent Events (SSE) 返回AI的流式响应。
        接收包含初始数据引用（可选）、用户当前消息（可选）和聊天历史的请求体。
        Saves user and AI messages to ProblemAIChatLog.
        """
        self.log_info(f"Handling AI interactive analysis stream for session {session_id_str}. User message: '{request_payload.user_message}'. History items: {len(request_payload.chat_history)}")

        # Validate session_id format (optional, Pydantic in ProblemAIStreamRequest might do this if session_id is part of it)
        try:
            problem_session_uuid = uuid.UUID(session_id_str)
        except ValueError:
            self.log_error(f"Invalid session_id format for interactive stream: {session_id_str}")
            yield {"type": "error", "message": "Invalid session ID format."}
            return

        # 1. Save User's Message to Chat Log
        #    The problem_id is not known yet in this AI-driven creation workflow stage.
        #    Chat logs here are associated with the session_id.
        # Validate session_id exists and is active
        ai_session = await self.get_problem_ai_session(session_id_str, load_chat_logs=False) # Don't need chat logs here
        if not ai_session:
            self.log_error(f"AI Session not found for ID: {session_id_str} in interactive stream.")
            yield {"type": "error", "message": "AI session not found.", "session_id": session_id_str}
            return
        if ai_session.status != ProblemAISessionStatus.ACTIVE:
            self.log_warning(f"AI Session {session_id_str} is not active (status: {ai_session.status}). Stream aborted.")
            yield {"type": "error", "message": f"AI session is not active (status: {ai_session.status}).", "session_id": session_id_str}
            return

        # 1. Save User's Message to Chat Log
        latest_order_stmt = select(func.max(ProblemAIChatLog.order_in_conversation)).where(
            ProblemAIChatLog.problem_creation_session_id == session_id_str
        )
        latest_order_result = await self.db.scalar(latest_order_stmt)
        current_order = (latest_order_result or 0) + 1

        if request_payload.user_message:
            user_log_entry = ProblemAIChatLog(
                problem_creation_session_id=session_id_str, # Link to ProblemAISession.id
                role="user",
                content=request_payload.user_message,
                content_type="text",
                order_in_conversation=current_order,
                timestamp=datetime.now()
            )
            self.db.add(user_log_entry)
            await self.db.commit()
            current_order += 1
            self.log_info(f"User message saved for AI session {session_id_str}, order {user_log_entry.order_in_conversation}")

        # 2. Prepare AI Request
        # Use current_structured_data from the session if initial_data_ref is not in payload or for context
        current_session_structured_data = ai_session.current_structured_data
        
        context_for_ai = {
            "session_id": session_id_str,
             # Prefer initial_data_ref from request if provided (e.g. first stream call by client)
             # otherwise use the one stored in the session.
            "structured_problem_context": request_payload.initial_data_ref.model_dump() if request_payload.initial_data_ref else current_session_structured_data,
        }

        ai_interactive_request = AIRequestSchema(
            task_type=TaskType.PROBLEM_INTERACTIVE_DEEP_ANALYSIS,
            stream=True,
            prompt=request_payload.user_message, # User's current message
            history=[hist.model_dump() for hist in request_payload.chat_history], # Full history from client
            context=context_for_ai
        )

        accumulated_ai_response_content = ""
        ai_response_saved = False

        try:
            ai_service = await self._get_ai_service()
            self.log_info(f"Streaming AI request for session {session_id_str}, task: {ai_interactive_request.task_type}")
            
            result_generator = await ai_service.process_ai_request(ai_interactive_request)
            
            if not isinstance(result_generator, AsyncGenerator):
                self.log_error(f"AIService did not return a generator for streaming request in session {session_id_str}")
                yield {"type": "error", "message": "Internal error: AI service did not stream.", "session_id": session_id_str}
                return

            async for chunk_data in result_generator:
                if isinstance(chunk_data, dict):
                    yield chunk_data # Forward to API layer for SSE formatting
                    
                    event_type = chunk_data.get("type")
                    if event_type == "content_chunk":
                        accumulated_ai_response_content += chunk_data.get("value", "")
                    elif event_type == "structured_data_update": # Hypothetical event from AI service
                        updated_data = chunk_data.get("data")
                        if isinstance(updated_data, dict) and ai_session: # ai_session should be valid here
                            ai_session.current_structured_data = updated_data
                            ai_session.updated_at = datetime.now()
                            self.db.add(ai_session) # Add to session for update
                            await self.db.commit()
                            await self.db.refresh(ai_session)
                            self.log_info(f"AI Session {session_id_str} current_structured_data updated via stream.")
                    elif event_type == "stream_end":
                        if accumulated_ai_response_content and not ai_response_saved:
                            # Save final AI response chunk
                            # ... (save logic as before)
                            pass # Handled below
                        break 
                    elif event_type == "error":
                        # ... (error handling as before)
                        break 
                else: # Fallback for unexpected chunk type
                    self.log_warning(f"AIService yielded unexpected data type: {type(chunk_data)} for session {session_id_str}.")
                    # ... (fallback handling as before)

            # Save accumulated AI response if any, after stream ends or breaks
            if accumulated_ai_response_content and not ai_response_saved:
                ai_log_entry = ProblemAIChatLog(
                    problem_creation_session_id=session_id_str,
                    role="ai",
                    content=accumulated_ai_response_content,
                    content_type="text_markdown",
                    order_in_conversation=current_order,
                    timestamp=datetime.now()
                )
                self.db.add(ai_log_entry)
                await self.db.commit()
                ai_response_saved = True # Mark as saved
                self.log_info(f"Final accumulated AI response saved for session {session_id_str}, order {ai_log_entry.order_in_conversation}")
            
            # Ensure a stream_end event is yielded if not already by AI service
            if chunk_data.get("type") != "stream_end": # Check last chunk from generator
                 yield {"type": "stream_end", "session_id": session_id_str}

            self.log_info(f"AI interactive stream processing completed for session {session_id_str}")

        except Exception as e:
            # ... (Outer exception handling as before, including saving partial AI response)
            self.log_error(f"Unhandled error during AI interactive stream for session {session_id_str}: {e}", exc_info=True)
            yield {"type": "error", "message": f"Internal server error during stream: {str(e)}", "session_id": session_id_str}
            if accumulated_ai_response_content and not ai_response_saved:
                 try:
                    ai_log_entry = ProblemAIChatLog(
                        problem_creation_session_id=session_id_str,
                        role="ai",
                        content=accumulated_ai_response_content + f"\n[Error after this content: {str(e)}]",
                        content_type="text_partial_error",
                        order_in_conversation=current_order, # Ensure current_order is accessible
                        timestamp=datetime.now()
                    )
                    self.db.add(ai_log_entry)
                    await self.db.commit()
                    self.log_info(f"Partially accumulated AI response with error saved for session {session_id_str}.")
                 except Exception as db_save_exc:
                     self.log_error(f"Failed to save partial AI response during error handling for session {session_id_str}: {db_save_exc}")


    async def finalize_ai_problem_creation(
        self,
        request_data: ProblemAIFinalizeRequest
    ) -> Problem:
        """
        阶段三：AI驱动错题创建 - 最终确认并保存。
        接收包含所有最终字段和聊天记录的请求，创建错题，并更新AI会话状态。
        """
        self.log_info(f"Finalizing AI problem creation for session {request_data.session_id}. Title: {request_data.title}")
        
        # 0. Retrieve and validate AI Session
        ai_session = await self.get_problem_ai_session(request_data.session_id)
        if not ai_session:
            self.log_error(f"AI Session not found for ID: {request_data.session_id} during finalization.")
            raise ValueError(f"AI Session with ID {request_data.session_id} not found.")
        if ai_session.status == ProblemAISessionStatus.FINALIZED:
            self.log_warning(f"AI Session {request_data.session_id} is already finalized.")
            # Depending on policy, either raise error or return existing problem if problem_id exists
            if ai_session.final_problem_id:
                existing_problem = await self.get_problem_by_id(ai_session.final_problem_id)
                if existing_problem: return existing_problem # Or raise error: "Session already finalized"
            raise ValueError("AI Session already finalized.")
        if ai_session.status == ProblemAISessionStatus.ABORTED:
            raise ValueError("AI Session was aborted and cannot be finalized.")

        try:
            # 1. Create Problem entity (same as before)
            problem_core_data = request_data.dict(exclude={"chat_logs", "session_id", "ai_full_analysis_json"})
            problem_dict = {}
            for field in Problem.__table__.columns.keys():
                if field in problem_core_data:
                    problem_dict[field] = problem_core_data[field]
            
            problem_dict.setdefault("knowledge_points", [])
            problem_dict.setdefault("image_urls", request_data.image_urls or []) # Use image_urls from request
            problem_dict.setdefault("difficulty_level", 3)
            problem_dict.setdefault("mastery_level", 0.0)
            problem_dict.setdefault("review_count", 0)
            problem_dict.setdefault("tags", [])
            problem_dict["ai_analysis"] = request_data.ai_full_analysis_json

            new_problem = Problem(**problem_dict)

            if new_problem.tags:
                await self.increment_tag_usage(list(set(new_problem.tags)))
            if new_problem.category:
                await self.increment_category_usage(new_problem.subject, new_problem.category)

            self.db.add(new_problem)
            await self.db.flush() 
            await self.db.refresh(new_problem)
            self.log_info(f"Problem entity created with ID {new_problem.id} for session {request_data.session_id}")

            # 2. Link Chat Logs (already linked to session, now link problem_id)
            # This part of the logic remains similar: update problem_id on existing logs.
            stmt_select_logs = select(ProblemAIChatLog).where(
                ProblemAIChatLog.problem_creation_session_id == request_data.session_id
            )
            result_logs = await self.db.execute(stmt_select_logs)
            session_chat_logs = result_logs.scalars().all()
            for chat_log in session_chat_logs:
                chat_log.problem_id = new_problem.id
                self.db.add(chat_log)
            
            self.log_info(f"Updated {len(session_chat_logs)} chat log entries with problem_id {new_problem.id}")

            # 3. Update ProblemAISession
            ai_session.final_problem_id = new_problem.id
            ai_session.status = ProblemAISessionStatus.FINALIZED
            # Optionally, update current_structured_data one last time if request_data is more final
            # For example, if ai_full_analysis_json is the most complete version of structured data:
            if request_data.ai_full_analysis_json:
                 ai_session.current_structured_data = request_data.ai_full_analysis_json
            else: # Or use the problem_dict created for the problem
                 # Construct a dict that matches ProblemAIStructuredData closely from problem_dict
                 final_structured_for_session = {
                     "session_id": ai_session.id,
                     "extracted_content": new_problem.content,
                     "suggested_subject": new_problem.subject,
                     "preliminary_category": new_problem.category,
                     "preliminary_tags": new_problem.tags,
                     "knowledge_points": new_problem.knowledge_points, # Add KPs if available
                     # ... other relevant fields from Problem that match ProblemAIStructuredData
                 }
                 ai_session.current_structured_data = final_structured_for_session


            ai_session.updated_at = datetime.now()
            self.db.add(ai_session)

            await self.db.commit()
            await self.db.refresh(new_problem)
            await self.db.refresh(ai_session)
            # Refresh chat logs if they need to be accessed with updated problem_id immediately
            for log in session_chat_logs:
                await self.db.refresh(log)

            self.log_info(f"Successfully finalized AI session {request_data.session_id} and created problem {new_problem.id}")
            return new_problem

        except Exception as e:
            self.log_error(f"Error in finalize_ai_problem_creation for session {request_data.session_id}: {e}", exc_info=True)
            await self.db.rollback()
            raise

    # --- New methods for ProblemAISession ---

    async def get_problem_ai_session(
        self, 
        session_id: str, 
        load_chat_logs: bool = True, 
        load_final_problem: bool = True
    ) -> Optional[ProblemAISession]:
        """获取指定的AI错题创建会话及其关联数据。"""
        query = select(ProblemAISession).where(ProblemAISession.id == session_id)
        
        options_to_load = []
        if load_chat_logs:
            options_to_load.append(selectinload(ProblemAISession.ai_chat_logs))
        if load_final_problem:
            options_to_load.append(selectinload(ProblemAISession.final_problem)) # Assuming 'final_problem' is the relationship name
            
        if options_to_load:
            query = query.options(*options_to_load)
            
        result = await self.db.execute(query)
        session = result.scalar_one_or_none()
        
        if session:
            self.log_info(f"Retrieved AI session {session_id}. Chat logs loaded: {load_chat_logs}, Final problem loaded: {load_final_problem}")
        else:
            self.log_warning(f"AI session {session_id} not found.")
        return session

    async def get_chat_history_for_session(self, session_id: str) -> List[ProblemAIChatLog]:
        """获取指定AI错题创建会话的所有聊天记录。"""
        # Ensure the session itself exists first (optional, but good practice)
        session_exists = await self.db.scalar(select(func.count(ProblemAISession.id)).where(ProblemAISession.id == session_id))
        if not session_exists:
            self.log_warning(f"Attempted to get chat history for non-existent AI session: {session_id}")
            return [] # Or raise HTTPException(status_code=404, detail="AI session not found")

        stmt = select(ProblemAIChatLog).where(
            ProblemAIChatLog.problem_creation_session_id == session_id
        ).order_by(ProblemAIChatLog.order_in_conversation)
        
        result = await self.db.execute(stmt)
        chat_logs = result.scalars().all()
        self.log_info(f"Retrieved {len(chat_logs)} chat log entries for AI session {session_id}.")
        return chat_logs

    async def list_ai_problem_sessions(
        self,
        page: int = 1,
        size: int = 20,
        status: Optional[ProblemAISessionStatus] = None,
        # user_id: Optional[str] = None, # If user association is added later
        sort_by: str = "created_at",
        sort_desc: bool = True
    ) -> tuple[List[ProblemAISession], int]:
        """获取AI错题创建会话列表，支持分页和状态过滤。"""
        query = select(ProblemAISession)

        # if user_id: # Example for future user filtering
        #     query = query.where(ProblemAISession.user_id == user_id)
        
        if status:
            query = query.where(ProblemAISession.status == status)

        # Eager load related data if it's commonly needed for list display
        # For now, just basic session data. Details can be fetched via get_problem_ai_session.
        # query = query.options(selectinload(ProblemAISession.final_problem).load_only(Problem.id, Problem.title) if needed)


        # Count total matching sessions
        count_query = select(func.count()).select_from(query.subquery())
        total_sessions = await self.db.scalar(count_query)

        # Apply sorting
        sort_column = getattr(ProblemAISession, sort_by, ProblemAISession.created_at)
        if sort_desc:
            query = query.order_by(desc(sort_column))
        else:
            query = query.order_by(sort_column)
        
        # Apply pagination
        query = query.offset((page - 1) * size).limit(size)
        
        result = await self.db.execute(query)
        sessions = result.scalars().all()
        
        self.log_info(f"Listed {len(sessions)} AI problem sessions. Total matching: {total_sessions}. Page: {page}, Size: {size}, Status filter: {status}")
        return sessions, total_sessions
