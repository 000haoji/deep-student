
// 页面加载完成后执行
document.addEventListener('DOMContentLoaded', function() {
    // 确定当前页面
    const currentPath = window.location.pathname;

    // 根据路径选择初始化函数
    if (currentPath === '/') {
        initializeHomePage();
    } else if (currentPath === '/problems') {
        initializeProblemsPage();
    } else if (currentPath.startsWith('/problem/')) {
        initializeProblemDetailPage(currentPath.split('/').pop());
    } else if (currentPath === '/reviews') {
        initializeReviewsPage();
    } else if (currentPath.startsWith('/review/')) {
        initializeReviewDetailPage(currentPath.split('/').pop());
    }
});

// 首页初始化
function initializeHomePage() {
    const uploadForm = document.getElementById('upload-form');
    const loadingSection = document.getElementById('loading');
    const resultSection = document.getElementById('result-section');

    if (uploadForm) {
        uploadForm.addEventListener('submit', function(event) {
            event.preventDefault();
            
            const fileInput = document.getElementById('file');
            const modelType = document.getElementById('model-type').value;
            
            if (!fileInput.files.length) {
                alert('请选择一个图片文件');
                return;
            }
            
            // 显示加载动画
            uploadForm.classList.add('hidden');
            loadingSection.classList.remove('hidden');
            
            // 准备表单数据
            const formData = new FormData();
            formData.append('file', fileInput.files[0]);
            formData.append('model_type', modelType);
            
            // 发送请求
            fetch('/api/upload', {
                method: 'POST',
                body: formData
            })
            .then(response => response.json())
            .then(data => {
                // 隐藏加载动画
                loadingSection.classList.add('hidden');
                
                if (data.success) {
                    // 保存问题ID，用于后续分析
                    const problemId = data.problem_id;
                    console.log("[INFO] 从上传响应中获取问题ID:", problemId);
                    
                    // 将problem_id保存到DOM元素的data属性中
                    document.getElementById('result-section').dataset.problemId = problemId;
                    
                    // 填充结果
                    displayAnalysisResult(data.analysis);
                    resultSection.classList.remove('hidden');
                    
                    // 显示图片预览
                    const file = fileInput.files[0];
                    const imageUrl = URL.createObjectURL(file);
                    document.getElementById('result-img').src = imageUrl;
                } else {
                    alert('分析失败: ' + (data.error || '未知错误'));
                    uploadForm.classList.remove('hidden');
                }
            })
            .catch(error => {
                console.error('Error:', error);
                alert('请求失败，请检查网络连接');
                loadingSection.classList.add('hidden');
                uploadForm.classList.remove('hidden');
            });
        });
    }
}

// 显示分析结果
function displayAnalysisResult(analysis) {
    // 输出分析结果到控制台以便调试
    console.log('收到分析结果:', analysis);
    
    try {
        // 进行更严格的错误检查
        if (!analysis) {
            console.error('分析结果为空:', analysis);
            throw new Error('分析结果为空或未定义');
        }
        
        if (typeof analysis !== 'object') {
            console.error('分析结果不是对象类型:', typeof analysis, analysis);
            if (typeof analysis === 'string') {
                try {
                    // 尝试将字符串解析为JSON对象
                    analysis = JSON.parse(analysis);
                } catch (parseError) {
                    console.error('无法将字符串解析为JSON:', parseError);
                    throw new Error('分析结果格式错误');
                }
            } else {
                throw new Error('分析结果格式错误');
            }
        }
        
        // 即使到这一步，也要确保所有必需字段都存在，使用默认值代替缺失的字段
        const safeAnalysis = {
            题目类型: analysis.题目类型 || '未知类型',
            具体分支: analysis.具体分支 || '未知分支',
            错误类型: analysis.错误类型 || '未知错误类型',
            题目原文: analysis.题目原文 || '无法提取题目',
            错误分析: analysis.错误分析 || '无法分析',
            正确解法: analysis.正确解法 || '无法提供解法',
            难度评估: analysis.难度评估 || 3,
            知识点标签: Array.isArray(analysis.知识点标签) ? analysis.知识点标签 : []
        };
        
        // 使用安全对象替换原始对象
        analysis = safeAnalysis;
    } catch (error) {
        // 如果出现任何错误，使用完全默认的分析结果对象
        console.error('处理分析结果时出错:', error);
        analysis = {
            题目类型: '未知类型',
            具体分支: '未知分支',
            错误类型: '未知错误类型',
            题目原文: '无法提取题目',
            错误分析: '无法分析',
            正确解法: '无法提供解法',
            难度评估: 3,
            知识点标签: []
        };
    }
    
    try {
        // 安全地设置内容，确保所有字段都有默认值
        document.getElementById('problem-content').textContent = analysis.题目原文 || '无法提取题目';
        document.getElementById('problem-category').textContent = analysis.题目类型 || '未知类型';
        document.getElementById('problem-subcategory').textContent = analysis.具体分支 || '未知分支';
        document.getElementById('error-type').textContent = analysis.错误类型 || '未知错误类型';
        document.getElementById('error-analysis').textContent = analysis.错误分析 || '无分析';
        document.getElementById('correct-solution').textContent = analysis.正确解法 || '无解法';
        
        // 设置难度星级
        const difficultyElement = document.getElementById('difficulty');
        const difficulty = parseInt(analysis.难度评估) || 3;
        difficultyElement.setAttribute('data-rating', difficulty);
        
        // 显示知识点标签
        const tagsContainer = document.getElementById('knowledge-tags');
        tagsContainer.innerHTML = '';
        let tags = analysis.知识点标签 || [];
        
        // 如果标签是字符串而非数组，尝试解析或转换为数组
        if (typeof tags === 'string') {
            try {
                tags = JSON.parse(tags);
            } catch (e) {
                tags = tags.split(/[,，]/g).map(tag => tag.trim()).filter(tag => tag);
            }
        }
        
        if (!Array.isArray(tags)) {
            console.error('标签格式不是数组:', tags);
            tags = [];
        }
        
        // 如果没有标签，显示"无标签"
        if (tags.length === 0) {
            const noTagSpan = document.createElement('span');
            noTagSpan.className = 'badge bg-secondary me-1';
            noTagSpan.textContent = '无标签';
            tagsContainer.appendChild(noTagSpan);
        } else {
            // 显示每个标签
            tags.forEach(tag => {
                if (tag && typeof tag === 'string') {
                    const tagSpan = document.createElement('span');
                    tagSpan.className = 'badge bg-primary me-1';
                    tagSpan.textContent = tag;
                    tagsContainer.appendChild(tagSpan);
                }
            });
        }
    } catch (error) {
        console.error('显示分析结果时出错:', error);
        // 在页面上显示错误信息
        const errorDiv = document.createElement('div');
        errorDiv.className = 'alert alert-danger mt-3';
        errorDiv.textContent = `显示结果时出错: ${error.message}. 请刷新页面重试。`;
        document.getElementById('analysis-result').appendChild(errorDiv);
    }
}

// 错题库页面初始化
function initializeProblemsPage() {
    const problemsTable = document.getElementById('problems-body');
    const noProblemsDiv = document.getElementById('no-problems');
    const selectAllCheckbox = document.getElementById('select-all');
    const createReviewBtn = document.getElementById('create-review-btn');
    const searchInput = document.getElementById('search-input');
    const categoryFilter = document.getElementById('category-filter');
    
    // 加载错题列表
    fetch('/api/problems')
        .then(response => response.json())
        .then(problems => {
            if (problems.length === 0) {
                noProblemsDiv.classList.remove('hidden');
                return;
            }
            
            renderProblemsList(problems, problemsTable);
            
            // 设置全选功能
            if (selectAllCheckbox) {
                selectAllCheckbox.addEventListener('change', function() {
                    const checkboxes = document.querySelectorAll('.problem-checkbox');
                    checkboxes.forEach(cb => {
                        cb.checked = this.checked;
                    });
                    updateCreateReviewButton();
                });
            }
            
            // 检查选中状态并更新按钮
            function updateCreateReviewButton() {
                const checkboxes = document.querySelectorAll('.problem-checkbox:checked');
                createReviewBtn.disabled = checkboxes.length === 0;
            }
            
            // 创建回顾分析
            if (createReviewBtn) {
                createReviewBtn.addEventListener('click', function() {
                    const selectedProblems = [];
                    document.querySelectorAll('.problem-checkbox:checked').forEach(cb => {
                        selectedProblems.push(cb.value);
                    });
                    
                    if (selectedProblems.length > 0) {
                        createReviewAnalysis(selectedProblems);
                    }
                });
            }
            
            // 搜索功能
            if (searchInput) {
                searchInput.addEventListener('input', function() {
                    filterProblems();
                });
            }
            
            // 分类筛选功能
            if (categoryFilter) {
                categoryFilter.addEventListener('change', function() {
                    filterProblems();
                });
            }
            
            // 筛选错题
            function filterProblems() {
                const searchTerm = searchInput.value.toLowerCase();
                const categoryTerm = categoryFilter.value;
                
                fetch('/api/problems')
                    .then(response => response.json())
                    .then(allProblems => {
                        const filteredProblems = allProblems.filter(problem => {
                            const contentMatch = problem.content && problem.content.toLowerCase().includes(searchTerm);
                            const categoryMatch = !categoryTerm || problem.category === categoryTerm;
                            return contentMatch && categoryMatch;
                        });
                        
                        renderProblemsList(filteredProblems, problemsTable);
                        
                        // 重新绑定删除和查看事件
                        document.querySelectorAll('.problem-checkbox').forEach(cb => {
                            cb.addEventListener('change', updateCreateReviewButton);
                        });
                    });
            }
        })
        .catch(error => {
            console.error('Error:', error);
            alert('获取错题列表失败');
        });
}

// 渲染错题列表
function renderProblemsList(problems, tableBody) {
    tableBody.innerHTML = '';
    
    problems.forEach(problem => {
        const row = document.createElement('tr');
        
        // 创建复选框单元格
        const checkboxCell = document.createElement('td');
        const checkbox = document.createElement('input');
        checkbox.type = 'checkbox';
        checkbox.className = 'problem-checkbox';
        checkbox.value = problem.id;
        checkboxCell.appendChild(checkbox);
        
        // 创建其他单元格
        const contentCell = document.createElement('td');
        contentCell.textContent = truncateText(problem.content, 50);
        
        const categoryCell = document.createElement('td');
        categoryCell.textContent = problem.category || '未知';
        
        const subcategoryCell = document.createElement('td');
        subcategoryCell.textContent = problem.subcategory || '未知';
        
        const difficultyCell = document.createElement('td');
        const difficultyStars = document.createElement('div');
        difficultyStars.className = 'difficulty-stars';
        difficultyStars.setAttribute('data-rating', problem.difficulty || 3);
        difficultyCell.appendChild(difficultyStars);
        
        const dateCell = document.createElement('td');
        dateCell.textContent = formatDate(problem.created_at);
        
        const actionsCell = document.createElement('td');
        const viewBtn = document.createElement('a');
        viewBtn.href = `/problem/${problem.id}`;
        viewBtn.className = 'btn secondary';
        viewBtn.style.padding = '5px 10px';
        viewBtn.style.fontSize = '14px';
        viewBtn.textContent = '查看';
        actionsCell.appendChild(viewBtn);
        
        // 添加所有单元格到行
        row.appendChild(checkboxCell);
        row.appendChild(contentCell);
        row.appendChild(categoryCell);
        row.appendChild(subcategoryCell);
        row.appendChild(difficultyCell);
        row.appendChild(dateCell);
        row.appendChild(actionsCell);
        
        // 将行添加到表格
        tableBody.appendChild(row);
        
        // 添加事件监听器
        checkbox.addEventListener('change', function() {
            const createReviewBtn = document.getElementById('create-review-btn');
            const checkboxes = document.querySelectorAll('.problem-checkbox:checked');
            createReviewBtn.disabled = checkboxes.length === 0;
        });
    });
}

// 错题详情页面初始化
function initializeProblemDetailPage(problemId) {
    fetch(`/api/problem/${problemId}`)
        .then(response => response.json())
        .then(problem => {
            // 填充错题详情
            document.getElementById('problem-img').src = problem.image_url;
            document.getElementById('p-category').textContent = problem.problem_category;
            document.getElementById('p-subcategory').textContent = problem.problem_subcategory;
            document.getElementById('p-error-type').textContent = problem.error_type;
            
            const difficultyElement = document.getElementById('p-difficulty');
            difficultyElement.setAttribute('data-rating', problem.difficulty);
            
            document.getElementById('p-date').textContent = formatDate(problem.created_at);
            document.getElementById('p-content').textContent = problem.problem_content;
            document.getElementById('p-analysis').textContent = problem.error_analysis;
            document.getElementById('p-solution').textContent = problem.correct_solution;
            
            // 填充标签
            const tagsContainer = document.getElementById('p-tags');
            tagsContainer.innerHTML = '';
            let tags = problem.tags || [];
            
            // 如果标签是字符串，尝试解析
            if (typeof tags === 'string') {
                try {
                    tags = JSON.parse(tags);
                } catch (e) {
                    tags = [tags];
                }
            }
            
            tags.forEach(tag => {
                const tagElement = document.createElement('span');
                tagElement.className = 'tag';
                tagElement.textContent = tag;
                tagsContainer.appendChild(tagElement);
            });
        })
        .catch(error => {
            console.error('Error:', error);
            alert('获取错题详情失败');
        });
}

// 回顾记录页面初始化
function initializeReviewsPage() {
    const reviewsTable = document.getElementById('reviews-body');
    const noReviewsDiv = document.getElementById('no-reviews');
    
    // 加载回顾记录列表
    fetch('/api/reviews')
        .then(response => response.json())
        .then(reviews => {
            if (reviews.length === 0) {
                noReviewsDiv.classList.remove('hidden');
                return;
            }
            
            reviewsTable.innerHTML = '';
            
            reviews.forEach(review => {
                const row = document.createElement('tr');
                
                // ID列
                const idCell = document.createElement('td');
                idCell.textContent = review.id.substring(0, 8) + '...';
                
                // 错题数量列
                const countCell = document.createElement('td');
                countCell.textContent = review.problem_count;
                
                // 日期列
                const dateCell = document.createElement('td');
                dateCell.textContent = formatDate(review.created_at);
                
                // 操作列
                const actionCell = document.createElement('td');
                const viewBtn = document.createElement('a');
                viewBtn.href = `/review/${review.id}`;
                viewBtn.textContent = '查看详情';
                viewBtn.className = 'btn secondary';
                viewBtn.style.padding = '5px 10px';
                viewBtn.style.fontSize = '14px';
                actionCell.appendChild(viewBtn);
                
                // 添加单元格到行
                row.appendChild(idCell);
                row.appendChild(countCell);
                row.appendChild(dateCell);
                row.appendChild(actionCell);
                
                // 添加行到表格
                reviewsTable.appendChild(row);
            });
        })
        .catch(error => {
            console.error('Error:', error);
            alert('获取回顾记录失败');
        });
}

// 回顾详情页面初始化
function initializeReviewDetailPage(reviewId) {
    fetch(`/api/review/${reviewId}`)
        .then(response => response.json())
        .then(review => {
            // 填充回顾详情
            document.getElementById('r-date').textContent = formatDate(review.created_at);
            document.getElementById('r-problem-count').textContent = review.problems ? review.problems.length : 0;
            
            // 填充分析结果
            const analysis = review.review_analysis || {};
            document.getElementById('r-error-patterns').textContent = analysis.错误模式识别 || '无数据';
            document.getElementById('r-weak-areas').textContent = analysis.知识点薄弱区域 || '无数据';
            document.getElementById('r-strategies').textContent = analysis.针对性学习策略 || '无数据';
            document.getElementById('r-recommendations').textContent = analysis.习题推荐 || '无数据';
            document.getElementById('r-time-plan').textContent = analysis.时间规划建议 || '无数据';
            
            // 显示包含的错题
            const thumbnailsContainer = document.getElementById('problem-thumbnails');
            thumbnailsContainer.innerHTML = '';
            
            if (review.problems && review.problems.length > 0) {
                review.problems.forEach(problem => {
                    const thumbnailDiv = document.createElement('div');
                    thumbnailDiv.className = 'problem-thumbnail';
                    thumbnailDiv.onclick = function() {
                        window.location.href = `/problem/${problem.id}`;
                    };
                    
                    const thumbnailImg = document.createElement('img');
                    thumbnailImg.src = problem.image_url;
                    thumbnailImg.alt = '错题缩略图';
                    
                    const thumbnailInfo = document.createElement('div');
                    thumbnailInfo.className = 'thumbnail-info';
                    
                    const thumbnailTitle = document.createElement('h4');
                    thumbnailTitle.textContent = truncateText(problem.problem_content, 30);
                    
                    const thumbnailCategory = document.createElement('small');
                    thumbnailCategory.textContent = `${problem.problem_category} - ${problem.problem_subcategory}`;
                    
                    thumbnailInfo.appendChild(thumbnailTitle);
                    thumbnailInfo.appendChild(thumbnailCategory);
                    
                    thumbnailDiv.appendChild(thumbnailImg);
                    thumbnailDiv.appendChild(thumbnailInfo);
                    
                    thumbnailsContainer.appendChild(thumbnailDiv);
                });
            } else {
                thumbnailsContainer.textContent = '没有找到相关错题';
            }
        })
        .catch(error => {
            console.error('Error:', error);
            alert('获取回顾详情失败');
        });
}

// 创建回顾分析
function createReviewAnalysis(problemIds) {
    // 显示加载中
    const createReviewBtn = document.getElementById('create-review-btn');
    const originalText = createReviewBtn.textContent;
    createReviewBtn.textContent = '分析中...';
    createReviewBtn.disabled = true;
    
    // 发送请求创建回顾分析
    fetch('/api/review', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify({
            problem_ids: problemIds
        })
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            alert('回顾分析已创建');
            window.location.href = `/review/${data.review_id}`;
        } else {
            alert('创建回顾分析失败: ' + (data.error || '未知错误'));
            createReviewBtn.textContent = originalText;
            createReviewBtn.disabled = false;
        }
    })
    .catch(error => {
        console.error('Error:', error);
        alert('创建回顾分析请求失败');
        createReviewBtn.textContent = originalText;
        createReviewBtn.disabled = false;
    });
}

// 辅助函数：截断文本
function truncateText(text, maxLength) {
    if (!text) return '无内容';
    return text.length > maxLength ? text.substring(0, maxLength) + '...' : text;
}

// 辅助函数：格式化日期
function formatDate(dateString) {
    if (!dateString) return '未知日期';
    
    try {
        const date = new Date(dateString);
        return date.toLocaleString('zh-CN', {
            year: 'numeric',
            month: '2-digit',
            day: '2-digit',
            hour: '2-digit',
            minute: '2-digit'
        });
    } catch (e) {
        return dateString;
    }
}
