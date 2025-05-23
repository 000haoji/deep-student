# 错题管理系统 2.0 - 快速开始指南

## 🚀 10分钟快速上手

### 1. 最简单的启动方式（测试环境）

```bash
# 进入后端目录
cd 重构系统/backend

# 创建虚拟环境（如果还没有）
python -m venv venv

# 激活虚拟环境
# Windows:
.\venv\Scripts\activate
# Linux/Mac:
source venv/bin/activate

# 安装基础依赖
pip install fastapi uvicorn aiofiles

# 运行测试API
python test_api.py
```

访问 http://localhost:8000/docs 查看API文档！

### 2. 完整系统启动（开发环境）

```bash
# 安装所有依赖
pip install -r requirements.txt

# 运行数据库迁移
python start.py migrate

# 启动应用
python start.py
```

### 3. 生产环境部署（Docker）

```bash
# 使用Docker Compose一键启动
docker-compose up -d

# 查看运行状态
docker-compose ps
```

## 📚 核心功能展示

### 创建错题
```python
import requests

# 创建一道数学错题
response = requests.post("http://localhost:8000/api/v1/problems", json={
    "title": "求导数",
    "content": "求 f(x) = x³ + 2x² - 5x + 1 的导数",
    "subject": "math",
    "user_answer": "3x² + 2x - 5",
    "correct_answer": "3x² + 4x - 5",
    "notes": "忘记了2x²的导数是4x"
})
```

### AI分析
```python
# 分析错题
response = requests.post(f"http://localhost:8000/api/v1/problems/{problem_id}/analyze")
analysis = response.json()

print(f"错误原因: {analysis['error_analysis']}")
print(f"知识点: {', '.join(analysis['knowledge_points'])}")
print(f"改进建议: {analysis['suggestions']}")
```

### 批量分析
```python
# 批量分析多道题目
response = requests.post("http://localhost:8000/api/v1/reviews/batch-analysis", json={
    "problem_ids": ["id1", "id2", "id3"],
    "title": "微积分专题错误分析"
})
```

## 🎯 主要改进点

### 1. **智能AI路由**
- 自动选择最合适的AI模型
- 支持负载均衡和故障转移
- 成本优化（优先使用便宜的模型）

### 2. **结构化数据**
- 所有数据都有明确的类型定义
- 自动验证输入数据
- 统一的错误响应格式

### 3. **高性能**
- 异步处理，支持高并发
- 数据库连接池
- Redis缓存热点数据

### 4. **易于扩展**
- 微服务架构，服务间解耦
- 插件式设计，轻松添加新功能
- 标准化的API接口

## 📊 系统架构对比

| 功能模块 | 旧系统问题 | 新系统解决方案 |
|---------|-----------|---------------|
| 配置管理 | 976行的config.py，混合各种配置 | 环境变量 + pydantic-settings，类型安全 |
| 数据库操作 | 1757行的database.py，SQL拼接 | SQLAlchemy ORM + 异步支持 |
| AI调用 | 硬编码API密钥，单一模型 | 加密存储 + 智能路由 + 多模型 |
| 错误处理 | try-except简单捕获 | 结构化错误 + 详细日志 + 链路追踪 |
| API设计 | 不规范的路由定义 | OpenAPI规范 + 自动文档 |

## 🛠️ 开发工具

### API文档
- Swagger UI: http://localhost:8000/docs
- ReDoc: http://localhost:8000/redoc

### 数据库管理
```bash
# 创建新的迁移
alembic revision --autogenerate -m "描述"

# 应用迁移
alembic upgrade head

# 回滚
alembic downgrade -1
```

### 测试
```bash
# 运行所有测试
pytest

# 运行特定测试
pytest tests/test_problem_service.py

# 查看测试覆盖率
pytest --cov=services --cov-report=html
```

## 💡 最佳实践

1. **使用类型提示**
   ```python
   async def create_problem(data: ProblemCreate) -> Problem:
       # 自动验证输入数据类型
   ```

2. **异步编程**
   ```python
   async def analyze_problems(problem_ids: List[str]):
       # 并发处理多个分析任务
       tasks = [analyze_single(id) for id in problem_ids]
       results = await asyncio.gather(*tasks)
   ```

3. **错误处理**
   ```python
   try:
       result = await problem_service.create(data)
   except ValidationError as e:
       raise HTTPException(status_code=400, detail=e.errors())
   ```

## 🔗 相关资源

- [完整API文档](./docs/api.md)
- [部署指南](./docs/deployment.md)
- [开发指南](./docs/development.md)
- [架构设计文档](./docs/architecture.md)

## ❓ 常见问题

**Q: 如何添加新的AI模型？**
A: 在AI配置中添加模型信息，系统会自动识别并纳入路由系统。

**Q: 如何自定义错误分析规则？**
A: 可以通过扩展分析服务，添加自定义的分析逻辑。

**Q: 支持哪些数据导入格式？**
A: 目前支持JSON、CSV、Excel，可以通过插件扩展其他格式。

---

🎉 恭喜！您已经了解了新系统的核心功能。开始使用它来提升您的学习效率吧！ 