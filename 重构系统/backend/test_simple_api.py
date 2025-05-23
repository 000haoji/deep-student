"""
简化的API测试
直接运行，不依赖数据库
"""
from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import List, Dict, Any
import uvicorn
from datetime import datetime

app = FastAPI(title="错题管理系统测试API", version="2.0.0-test")

# 内存存储（模拟数据库）
problems_db = {}
analyses_db = {}
problem_counter = 0
analysis_counter = 0


class ProblemCreate(BaseModel):
    title: str
    content: str
    subject: str
    category: str = None
    user_answer: str = None
    notes: str = None


class ProblemResponse(BaseModel):
    id: int
    title: str
    content: str
    subject: str
    category: str = None
    user_answer: str = None
    notes: str = None
    created_at: str
    mastery_level: float = 0.5


@app.get("/")
async def root():
    return {
        "message": "错题管理系统测试API",
        "status": "running",
        "endpoints": {
            "创建错题": "POST /problems",
            "获取错题列表": "GET /problems",
            "获取单个错题": "GET /problems/{id}",
            "创建分析": "POST /analyses",
            "获取分析列表": "GET /analyses"
        }
    }


@app.post("/problems", response_model=ProblemResponse)
async def create_problem(problem: ProblemCreate):
    global problem_counter
    problem_counter += 1
    
    problem_data = {
        "id": problem_counter,
        **problem.dict(),
        "created_at": datetime.now().isoformat(),
        "mastery_level": 0.5
    }
    
    problems_db[problem_counter] = problem_data
    
    return ProblemResponse(**problem_data)


@app.get("/problems", response_model=List[ProblemResponse])
async def list_problems(subject: str = None):
    result = []
    for problem in problems_db.values():
        if subject is None or problem["subject"] == subject:
            result.append(ProblemResponse(**problem))
    return result


@app.get("/problems/{problem_id}", response_model=ProblemResponse)
async def get_problem(problem_id: int):
    if problem_id not in problems_db:
        raise HTTPException(status_code=404, detail="错题不存在")
    return ProblemResponse(**problems_db[problem_id])


@app.post("/analyses")
async def create_analysis(problem_ids: List[int], title: str = "批量分析"):
    global analysis_counter
    analysis_counter += 1
    
    # 检查题目是否存在
    valid_problems = []
    for pid in problem_ids:
        if pid in problems_db:
            valid_problems.append(problems_db[pid])
    
    if not valid_problems:
        raise HTTPException(status_code=400, detail="没有找到有效的题目")
    
    # 创建分析
    analysis = {
        "id": analysis_counter,
        "title": title,
        "problem_count": len(valid_problems),
        "subjects": list(set(p["subject"] for p in valid_problems)),
        "created_at": datetime.now().isoformat(),
        "summary": f"分析了{len(valid_problems)}道题目",
        "suggestions": [
            "多做练习题巩固知识",
            "复习相关概念",
            "注意细节问题"
        ]
    }
    
    analyses_db[analysis_counter] = analysis
    
    return analysis


@app.get("/analyses")
async def list_analyses():
    return list(analyses_db.values())


@app.get("/health")
async def health_check():
    return {
        "status": "healthy",
        "timestamp": datetime.now().isoformat()
    }


if __name__ == "__main__":
    print("启动错题管理系统测试API...")
    print("访问地址: http://localhost:8001")
    print("API文档: http://localhost:8001/docs")
    uvicorn.run(app, host="127.0.0.1", port=8001) 