# 开发指南

## 目录
1. [环境准备](#环境准备)
2. [项目结构](#项目结构)
3. [开发流程](#开发流程)
4. [代码规范](#代码规范)
5. [API开发](#api开发)
6. [数据库操作](#数据库操作)
7. [测试指南](#测试指南)
8. [部署说明](#部署说明)

## 环境准备

### 必需软件
- Python 3.11+
- Node.js 18+
- Docker & Docker Compose
- PostgreSQL 14+ (可通过Docker运行)
- Redis 7+ (可通过Docker运行)

### 开发环境设置

1. **克隆项目**
```bash
git clone <repository-url>
cd 重构系统
```

2. **创建Python虚拟环境**
```bash
cd backend
python -m venv venv
source venv/bin/activate  # Windows: venv\Scripts\activate
```

3. **安装依赖**
```bash
make install
# 或手动：
cd backend && pip install -r requirements.txt
cd ../frontend && npm install
```

4. **配置环境变量**
```bash
cp env.template .env
# 编辑.env文件，填入必要的配置
```

5. **启动依赖服务**
```bash
make docker-up
# 或：docker-compose up -d
```

6. **运行数据库迁移**
```bash
make migrate
```

## 项目结构

### 后端结构
```
backend/
├── main.py                  # 应用入口
├── services/               # 微服务
│   ├── ai-api-manager/    # AI API管理服务
│   ├── problem-service/   # 错题管理服务
│   ├── review-service/    # 回顾分析服务
│   ├── user-service/      # 用户服务
│   └── file-service/      # 文件服务
├── shared/                 # 共享组件
│   ├── config.py          # 配置管理
│   ├── database.py        # 数据库连接
│   ├── models/            # 基础模型
│   ├── utils/             # 工具函数
│   └── middleware/        # 中间件
└── infrastructure/         # 基础设施
    ├── database/          # 数据库脚本
    └── docker/            # Docker配置
```

### 服务架构

每个服务都遵循以下结构：
```
service-name/
├── __init__.py      # 包初始化
├── models.py        # 数据模型
├── schemas.py       # Pydantic模式
├── repository.py    # 数据访问层
├── service.py       # 业务逻辑层
├── api.py          # API路由层
└── dependencies.py  # 依赖注入
```

## 开发流程

### 1. 创建新功能分支
```bash
git checkout -b feature/your-feature-name
```

### 2. 开发新功能

#### 添加新的API端点
1. 在对应服务的`models.py`中定义数据模型
2. 在`schemas.py`中定义请求/响应模式
3. 在`repository.py`中实现数据访问
4. 在`service.py`中实现业务逻辑
5. 在`api.py`中定义路由

示例：
```python
# schemas.py
from pydantic import BaseModel

class ProblemCreate(BaseModel):
    title: str
    content: str
    subject: str

# service.py
class ProblemService:
    async def create_problem(self, data: ProblemCreate) -> Problem:
        # 业务逻辑
        pass

# api.py
@router.post("/problems", response_model=ProblemResponse)
async def create_problem(
    data: ProblemCreate,
    service: ProblemService = Depends(get_problem_service)
):
    return await service.create_problem(data)
```

### 3. 编写测试
```python
# tests/test_problem_service.py
import pytest
from httpx import AsyncClient

@pytest.mark.asyncio
async def test_create_problem(client: AsyncClient):
    response = await client.post("/api/v1/problems", json={
        "title": "测试题目",
        "content": "题目内容",
        "subject": "math"
    })
    assert response.status_code == 200
    assert response.json()["title"] == "测试题目"
```

### 4. 代码检查和格式化
```bash
make lint    # 运行代码检查
make format  # 自动格式化代码
```

### 5. 提交代码
```bash
git add .
git commit -m "feat: 添加新功能描述"
git push origin feature/your-feature-name
```

## 代码规范

### Python代码规范
- 遵循PEP 8规范
- 使用类型注解
- 编写docstring文档
- 使用async/await进行异步编程

### 命名规范
- 类名：PascalCase
- 函数名：snake_case
- 常量：UPPER_SNAKE_CASE
- 私有方法：_leading_underscore

### 提交信息规范
使用语义化提交信息：
- feat: 新功能
- fix: 修复bug
- docs: 文档更新
- style: 代码格式调整
- refactor: 代码重构
- test: 测试相关
- chore: 构建/工具相关

## API开发

### RESTful规范
- GET /resources - 获取资源列表
- GET /resources/{id} - 获取单个资源
- POST /resources - 创建资源
- PUT /resources/{id} - 更新资源
- DELETE /resources/{id} - 删除资源

### 响应格式
成功响应：
```json
{
    "data": {},
    "message": "Success",
    "code": 200
}
```

错误响应：
```json
{
    "error": "Error message",
    "code": 400,
    "details": {}
}
```

### 分页
```json
{
    "items": [],
    "total": 100,
    "page": 1,
    "size": 20,
    "pages": 5
}
```

## 数据库操作

### 使用SQLAlchemy ORM
```python
from sqlalchemy import select
from backend.shared.database import get_db

async def get_problems(db: AsyncSession):
    result = await db.execute(
        select(Problem).where(Problem.user_id == user_id)
    )
    return result.scalars().all()
```

### 事务处理
```python
async def create_problem_with_tags(data, db: AsyncSession):
    async with db.begin():
        problem = Problem(**data)
        db.add(problem)
        # 其他操作...
    # 自动提交或回滚
```

## 测试指南

### 运行测试
```bash
make test
# 或指定测试文件
pytest tests/test_specific.py
```

### 测试覆盖率
```bash
pytest --cov=backend --cov-report=html
```

### 测试数据库
测试使用独立的数据库，在`conftest.py`中配置

## 部署说明

### 开发环境部署
```bash
make dev-all  # 启动所有服务
```

### 生产环境部署

1. **构建Docker镜像**
```bash
docker build -t problem-analysis-backend:latest -f backend/Dockerfile .
docker build -t problem-analysis-frontend:latest -f frontend/Dockerfile .
```

2. **使用Docker Compose部署**
```bash
docker-compose -f docker-compose.prod.yml up -d
```

3. **使用Kubernetes部署**
```bash
kubectl apply -f k8s/
```

### 环境变量配置
生产环境必须设置的环境变量：
- `APP_ENV=production`
- `DATABASE_URL`
- `REDIS_URL`
- `JWT_SECRET_KEY`
- `ENCRYPTION_KEY`
- 各AI服务的API密钥

### 监控和日志
- Prometheus metrics: `/metrics`
- 健康检查: `/health`
- 日志输出: JSON格式，便于日志收集

## 常见问题

### 1. 数据库连接失败
检查Docker服务是否启动：
```bash
docker-compose ps
```

### 2. 依赖安装失败
使用国内镜像源：
```bash
pip install -r requirements.txt -i https://pypi.tuna.tsinghua.edu.cn/simple
```

### 3. 端口被占用
修改`.env`文件中的端口配置

## 更多资源

- [FastAPI文档](https://fastapi.tiangolo.com/)
- [SQLAlchemy文档](https://docs.sqlalchemy.org/)
- [Vue 3文档](https://vuejs.org/)
- [项目API文档](http://localhost:8000/docs) 