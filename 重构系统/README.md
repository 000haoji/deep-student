# 错题管理系统 2.0

基于微服务架构的考研错题管理与智能分析系统。

## 🌟 主要特性

- **错题管理**: 支持创建、编辑、删除、复习错题
- **智能分析**: AI驱动的错误模式识别和学习建议
- **多学科支持**: 数学、英语、政治、专业课
- **学习追踪**: 记录复习历史，跟踪掌握程度
- **数据可视化**: 图表展示学习进度和统计信息
- **用户认证**: 安全的用户注册和登录系统

## 🚀 快速开始

### 系统要求

- Python 3.8+
- Node.js 14+
- 现代浏览器（Chrome、Firefox、Safari、Edge）

### 一键启动

```bash
# 克隆项目
git clone <repository-url>
cd 重构系统

# 运行启动脚本
python start_all.py
```

启动脚本会自动：
1. 检查系统环境
2. 安装所有依赖
3. 初始化数据库
4. 启动后端服务（端口 8000）
5. 启动前端服务（端口 3000）

### 手动启动

#### 后端启动

```bash
cd backend

# 创建虚拟环境
python -m venv venv

# 激活虚拟环境
# Windows:
venv\Scripts\activate
# Linux/Mac:
source venv/bin/activate

# 安装依赖
pip install -r requirements.txt
pip install werkzeug pyjwt

# 初始化数据库
python init_db.py

# 启动服务
uvicorn app:app --reload --host 0.0.0.0 --port 8000
```

#### 前端启动

```bash
cd frontend

# 安装依赖
npm install

# 启动开发服务器
npm run dev
```

## 📱 使用说明

### 访问地址

- 前端界面: http://localhost:3000
- API文档: http://localhost:8000/docs

### 演示账号

- 用户名: `demo_user`
- 密码: `demo123`

### 主要功能

1. **错题管理**
   - 添加新错题
   - 编辑错题信息
   - 删除错题
   - 标记复习进度

2. **智能分析**
   - 批量分析错题
   - 识别错误模式
   - 生成学习计划
   - 个性化建议

3. **数据统计**
   - 查看学习进度
   - 学科分布图表
   - 掌握度趋势
   - 复习记录

## 🏗️ 系统架构

```
重构系统/
├── backend/                # 后端服务
│   ├── services/          # 微服务模块
│   │   ├── ai_api_manager/    # AI管理
│   │   ├── problem_service/   # 错题服务
│   │   ├── review_service/    # 分析服务
│   │   ├── file_service/      # 文件服务
│   │   └── user_service/      # 用户服务
│   ├── shared/            # 共享组件
│   ├── infrastructure/    # 基础设施
│   └── app.py            # 主应用
├── frontend/              # 前端应用
│   ├── src/
│   │   ├── views/        # 页面组件
│   │   ├── components/   # 通用组件
│   │   ├── stores/       # 状态管理
│   │   ├── router/       # 路由配置
│   │   └── utils/        # 工具函数
│   └── package.json
└── docs/                  # 项目文档
```

## 🛠️ 技术栈

### 后端
- **FastAPI**: 高性能异步Web框架
- **SQLAlchemy**: ORM数据库操作
- **Pydantic**: 数据验证
- **JWT**: 用户认证
- **PostgreSQL/SQLite**: 数据存储

### 前端
- **Vue 3**: 渐进式JavaScript框架
- **Element Plus**: UI组件库
- **Pinia**: 状态管理
- **Vue Router**: 路由管理
- **Chart.js**: 数据可视化
- **Axios**: HTTP客户端

## 🔧 配置说明

### 环境变量

创建 `backend/.env` 文件：

```env
DATABASE_URL=sqlite+aiosqlite:///./error_management.db
SECRET_KEY=your-secret-key-here
ENVIRONMENT=development
```

### 数据库配置

默认使用SQLite，可切换到PostgreSQL：

```env
DATABASE_URL=postgresql+asyncpg://user:password@localhost/dbname
```

## 📝 API文档

启动后端服务后，访问 http://localhost:8000/docs 查看交互式API文档。

主要API端点：

- `POST /register` - 用户注册
- `POST /token` - 用户登录
- `GET /problems` - 获取错题列表
- `POST /problems` - 创建错题
- `PUT /problems/{id}` - 更新错题
- `DELETE /problems/{id}` - 删除错题
- `POST /analyses` - 创建分析
- `GET /statistics` - 获取统计信息

## 🤝 贡献指南

1. Fork 项目
2. 创建特性分支 (`git checkout -b feature/AmazingFeature`)
3. 提交更改 (`git commit -m 'Add some AmazingFeature'`)
4. 推送到分支 (`git push origin feature/AmazingFeature`)
5. 开启 Pull Request

## 📄 许可证

本项目采用 MIT 许可证 - 查看 [LICENSE](LICENSE) 文件了解详情

## 👥 联系方式

如有问题或建议，请提交 Issue 或联系项目维护者。

---

**注意**: 这是一个演示项目，生产环境使用前请：
- 更改默认密钥
- 配置真实的AI API密钥
- 使用PostgreSQL等生产级数据库
- 添加适当的安全措施 