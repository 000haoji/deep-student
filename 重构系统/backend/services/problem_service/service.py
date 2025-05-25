"""
错题管理服务业务逻辑
"""
import base64
import io
import time
from typing import List, Optional, Dict, Any
from datetime import datetime, timedelta
import httpx # For downloading image from URL
from sqlalchemy import select, func, and_, or_, desc
from sqlalchemy.ext.asyncio import AsyncSession
from PIL import Image

from shared.utils.logger import LoggerMixin
from ..file_service import file_service
# from ..ai_api_manager.schemas import AIRequest as AIRequestSchema, TaskType # AIRequestSchema if schemas.py version is different
from ..ai_api_manager.models import AIRequest, TaskType # Using AIRequest from models.py as AIRouter does
from ..ai_api_manager.router import ai_router # Import global ai_router
from .models import Problem, ReviewRecord, Subject, ProblemTag, ProblemCategory
from .schemas import (
    ProblemCreate, ProblemUpdate, # ProblemAnalyzeRequest (schema for API, not direct AIRequest)
    ProblemOCRRequest, ReviewRecordCreate, ProblemQuery,
    ProblemTagCreate, ProblemTagUpdate, # Added Tag schemas
    ProblemCategoryCreate, ProblemCategoryUpdate, # Added Category schemas
    ProblemData, # For export data structure
    ProblemBatchRequest # For batch operations
)


class ProblemService(LoggerMixin):
    """错题管理服务"""
    
    def __init__(self, db: AsyncSession):
        self.db = db
        # self._ai_service removed, will use global ai_router
    
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
            
            # 调用AI服务进行分析
            # analysis_response = await self.ai_service.analyze(analysis_request) # Old call
            analysis_response = await ai_router.route_request(analysis_request) # Use global ai_router
            
            if not analysis_response.success or not analysis_response.content:
                error_message = analysis_response.error or "Unknown AI analysis error from router"
                self.log_error(f"AI analysis failed for problem {problem_id}: {error_message}")
                # Consider not raising an exception here to allow partial updates,
                # or make it configurable. For now, let's assume failure means no update.
            # For a robust system, you might want to log the error and proceed,
            # or retry, or flag the problem for manual review.
            # For now, let's assume failure means no update.
            # Let's update the problem even if AI analysis fails partially or completely.
            # The AI analysis fields in the DB might remain null or unchanged.
            # For now, let's assume if AI fails, we don't update AI-derived fields.
            # To make it simple, if AI analysis fails, we just log and don't update AI fields.
            
            # Attempt to store the error message if the model supports it (e.g., an ai_analysis_error field)
            if hasattr(problem, 'ai_analysis_error'):
                 problem.ai_analysis_error = error_message 
                 await self.db.commit() # Commit this error state

            self.log_warning(f"AI analysis failed for problem {problem_id}, AI fields not updated. Error: {error_message}")
            return {
                "status": "ai_failed",
                "problem_id": str(problem_id),
                "message": f"AI analysis failed: {error_message}",
                "analysis": None
            }

            # AI analysis was successful, content should be a JSON string
            raw_ai_content = analysis_response.content
            analysis_data_dict = {}
            if isinstance(raw_ai_content, str):
                try:
                    analysis_data_dict = json.loads(raw_ai_content)
                except json.JSONDecodeError:
                    self.log_error(f"Failed to parse AI analysis JSON for problem {problem_id}: {raw_ai_content}")
                    # Handle as AI failure for updating fields
                    if hasattr(problem, 'ai_analysis_error'):
                        problem.ai_analysis_error = "Failed to parse AI JSON response"
                        await self.db.commit()
                    return {
                        "status": "ai_failed",
                        "problem_id": str(problem_id),
                        "message": "AI analysis successful, but failed to parse JSON response.",
                        "analysis": None
                    }
            elif isinstance(raw_ai_content, dict): # If AI provider already returns a dict
                analysis_data_dict = raw_ai_content
            else:
                self.log_error(f"Unexpected AI analysis content type for problem {problem_id}: {type(raw_ai_content)}")
                if hasattr(problem, 'ai_analysis_error'):
                    problem.ai_analysis_error = "Unexpected AI response content type"
                    await self.db.commit()
                return {
                    "status": "ai_failed",
                    "problem_id": str(problem_id),
                    "message": "AI analysis successful, but unexpected response content type.",
                    "analysis": None
                }

            # 更新问题信息
            # Update tags first, as they might also be suggested by AI
            new_tags = analysis_data_dict.get("tags", problem.tags if problem.tags else [])
            if isinstance(new_tags, list): # Ensure it's a list
                original_tags = list(problem.tags) if problem.tags else []
                tags_to_add = [tag for tag in new_tags if tag not in original_tags]
                tags_to_remove = [tag for tag in original_tags if tag not in new_tags]
                if tags_to_add: await self.increment_tag_usage(tags_to_add)
                if tags_to_remove: await self.decrement_tag_usage(tags_to_remove)
                problem.tags = new_tags
            
            problem.knowledge_points = analysis_data_dict.get("knowledge_points", problem.knowledge_points)
            problem.difficulty_level = analysis_data_dict.get("difficulty_level", problem.difficulty_level)
            problem.error_analysis = analysis_data_dict.get("error_analysis", problem.error_analysis)
            problem.solution = analysis_data_dict.get("solution", problem.solution)
            problem.ai_analysis = analysis_data_dict # Store the full parsed JSON

            # Handle suggested category
            suggested_category_name = analysis_data_dict.get("suggested_category")
            if suggested_category_name and isinstance(suggested_category_name, str):
                old_category_name = problem.category
                if old_category_name != suggested_category_name:
                    problem.category = suggested_category_name # Update problem's category name
                    if old_category_name:
                        await self.decrement_category_usage(problem.subject, old_category_name)
                    await self.increment_category_usage(problem.subject, suggested_category_name) # Creates if not exists

            if hasattr(problem, 'ai_analysis_error'): # Clear any previous error
                problem.ai_analysis_error = None
            
            await self.db.commit()
            await self.db.refresh(problem)
            
            self.log_info(f"Successfully analyzed problem: {problem_id}")
            return {
                "status": "success",
                "problem_id": problem_id,
                "analysis": analysis_result
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

        ai_ocr_request_content = {
            "image_base64": image_b64_for_ai,
            "enhance_math": data.enhance_math,
        }
        
        # 调用AI API进行OCR
        ai_ocr_direct_request = AIRequest( # Construct AIRequest directly
            task_type=TaskType.OCR,
            content=ai_ocr_request_content
            # metadata, preferred_providers, etc. can be added if needed by ai_router
        )
        
        # response = await self.ai_service.route_request(ai_request) # Old call
        ocr_response = await ai_router.route_request(ai_ocr_direct_request) # Use global ai_router
        
        if ocr_response.success and ocr_response.content:
            ocr_text_result = ocr_response.content.get("text")
            
            if ocr_text_result is None: # Check if 'text' key exists and has a value
                 self.log_warning(f"OCR successful but no text content returned for image. AI Response: {ocr_response.content}")
                 ocr_text_result = ""


            # 如果需要自动创建题目
            if data.auto_create and ocr_text_result:
                problem_create_payload = ProblemCreate(
                    title=f"OCR 题目 - {datetime.now().strftime('%Y-%m-%d %H:%M')}", # Generic title
                    content=ocr_text_result,
                    subject=data.subject or Subject.MATH,  # Use provided subject or default
                    image_base64=[original_image_for_problem_creation] if original_image_for_problem_creation else [],
                    category=data.category, # Pass category if provided in OCRRequest
                )
                created_problem = await self.create_problem(
                    problem_create_payload,
                    auto_analyze=False
                )
                
                return {
                    "text": ocr_text_result, # Return the OCR'd text
                    "problem_id": str(created_problem.id),
                    "created": True,
                    "message": "OCR successful and problem created."
                }
            
            return {
                "text": ocr_text_result, # Return the OCR'd text
                "created": False,
                "message": "OCR successful."
            }
        else:
            error_msg = ocr_response.error or "OCR failed with unknown error from AI service via router"
            self.log_error(f"OCR failed: {error_msg}")
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
