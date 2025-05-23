.PHONY: help install dev test lint format clean docker-up docker-down migrate

# 默认目标
help:
	@echo "可用的命令："
	@echo "  make install      - 安装依赖"
	@echo "  make dev          - 启动开发环境"
	@echo "  make test         - 运行测试"
	@echo "  make lint         - 代码检查"
	@echo "  make format       - 代码格式化"
	@echo "  make clean        - 清理临时文件"
	@echo "  make docker-up    - 启动Docker服务"
	@echo "  make docker-down  - 停止Docker服务"
	@echo "  make migrate      - 运行数据库迁移"

# 安装依赖
install:
	cd backend && pip install -r requirements.txt
	cd frontend && npm install

# 启动开发环境
dev: docker-up
	cd backend && python main.py

# 运行测试
test:
	cd backend && pytest -v

# 代码检查
lint:
	cd backend && flake8 .
	cd backend && mypy .

# 代码格式化
format:
	cd backend && black .

# 清理临时文件
clean:
	find . -type d -name "__pycache__" -exec rm -rf {} +
	find . -type f -name "*.pyc" -delete
	find . -type f -name "*.pyo" -delete
	find . -type f -name ".coverage" -delete
	find . -type d -name ".pytest_cache" -exec rm -rf {} +
	find . -type d -name ".mypy_cache" -exec rm -rf {} +

# Docker相关
docker-up:
	docker-compose up -d

docker-down:
	docker-compose down

docker-logs:
	docker-compose logs -f

# 数据库迁移
migrate:
	cd backend && alembic upgrade head

# 创建新的迁移
migration:
	cd backend && alembic revision --autogenerate -m "$(message)"

# 生成加密密钥
generate-key:
	cd backend && python -m services.ai_api_manager.crypto generate

# 前端开发
frontend-dev:
	cd frontend && npm run dev

frontend-build:
	cd frontend && npm run build

# 完整的开发环境启动
dev-all: docker-up
	make -j2 dev frontend-dev 