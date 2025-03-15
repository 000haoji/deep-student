// 问题详情页面的JavaScript

// 全局变量
let problemData = null;
let problemId = null;

// 页面加载完成后执行
document.addEventListener('DOMContentLoaded', function() {
    // 从URL获取问题ID
    const pathParts = window.location.pathname.split('/');
    problemId = pathParts[pathParts.length - 1];
    
    // 加载错题详情
    loadProblemDetail();
    
    // 绑定典型度评分事件
    setupTypicalityRating();
    
    // 绑定编辑切换事件
    setupEditModeToggle();
    
    // 绑定删除按钮事件
    document.getElementById('deleteBtn').addEventListener('click', confirmDelete);
});

// 加载错题详情
async function loadProblemDetail() {
    try {
        // 显示加载状态
        document.getElementById('loading').style.display = 'block';
        document.getElementById('content').style.display = 'none';
        
        const response = await fetch(`/api/problem/${problemId}`);
        if (!response.ok) throw new Error('获取错题详情失败');
        
        problemData = await response.json();
        
        // 填充页面内容
        populateProblemDetail(problemData);
        
        // 隐藏加载状态，显示内容
        document.getElementById('loading').style.display = 'none';
        document.getElementById('content').style.display = 'block';
    } catch (error) {
        console.error('加载错题详情失败:', error);
        document.getElementById('loading').style.display = 'none';
        alert('加载错题详情失败: ' + error.message);
    }
}

// 填充错题详情
function populateProblemDetail(data) {
    // 设置页面标题
    document.title = `错题详情 - ${data.problem_category || '未分类'}`;
    
    // 填充基本信息
    document.getElementById('problem-content').textContent = data.problem_content || '无题目内容';
    document.getElementById('problem-category').textContent = data.problem_category || '未分类';
    document.getElementById('problem-subcategory').textContent = data.problem_subcategory || '未知';
    document.getElementById('error-type').textContent = data.error_type || '未知';
    document.getElementById('difficulty').textContent = `${data.difficulty || 3}/5`;
    
    // 填充错误分析
    document.getElementById('error-analysis').textContent = data.error_analysis || '无错误分析';
    
    // 填充正确解法
    document.getElementById('correct-solution').textContent = data.correct_solution || '无解题思路';
    
    // 填充标签
    const tagsContainer = document.getElementById('tags-container');
    tagsContainer.innerHTML = '';
    
    if (data.tags && data.tags.length > 0) {
        data.tags.forEach(tag => {
            const tagBadge = document.createElement('span');
            tagBadge.className = 'tag-badge';
            tagBadge.textContent = tag;
            tagsContainer.appendChild(tagBadge);
        });
    } else {
        const noTags = document.createElement('em');
        noTags.className = 'text-muted';
        noTags.textContent = '无标签';
        tagsContainer.appendChild(noTags);
    }
    
    // 填充用户补充说明
    const notesElement = document.getElementById('problem-notes');
    if (data.notes && data.notes.trim()) {
        notesElement.textContent = data.notes;
        notesElement.className = '';
    } else {
        notesElement.textContent = '无补充说明';
        notesElement.className = 'fst-italic text-muted';
    }
    
    // 设置图片
    document.getElementById('problem-image').src = `/uploads/${data.image_path.split('/').pop()}`;
    
    // 处理附加图片
    const additionalImagesContainer = document.getElementById('additional-images');
    additionalImagesContainer.innerHTML = '';
    
    if (data.additional_images && data.additional_images.length > 0) {
        const additionalImagesTitle = document.createElement('h6');
        additionalImagesTitle.className = 'mt-4 mb-2';
        additionalImagesTitle.textContent = '附加图片:';
        additionalImagesContainer.appendChild(additionalImagesTitle);
        
        const imagesRow = document.createElement('div');
        imagesRow.className = 'd-flex flex-wrap';
        
        data.additional_images.forEach((imagePath, index) => {
            const imgContainer = document.createElement('div');
            imgContainer.className = 'mb-2 me-2';
            
            const img = document.createElement('img');
            img.src = `/uploads/${imagePath.split('/').pop()}`;
            img.alt = `附加图片 ${index + 1}`;
            img.className = 'additional-image';
            img.addEventListener('click', function() {
                // 点击附加图片时在新窗口中打开
                window.open(this.src, '_blank');
            });
            
            imgContainer.appendChild(img);
            imagesRow.appendChild(imgContainer);
        });
        
        additionalImagesContainer.appendChild(imagesRow);
    }
    
    // 设置典型度评分
    setTypicalityRating(data.typicality || 3);
    
    // 填充编辑表单
    fillEditForm(data);
}

// 设置典型度评分
function setTypicalityRating(rating) {
    const stars = document.getElementById('typicality-rating').querySelectorAll('i');
    stars.forEach((star, index) => {
        if (index < rating) {
            star.className = 'bi bi-star-fill';
        } else {
            star.className = 'bi bi-star';
        }
    });
}

// 设置典型度评分事件
function setupTypicalityRating() {
    const stars = document.getElementById('typicality-rating').querySelectorAll('i');
    stars.forEach(star => {
        // 鼠标悬停效果
        star.addEventListener('mouseenter', function() {
            const rating = parseInt(this.dataset.rating);
            stars.forEach((s, idx) => {
                s.className = idx < rating ? 'bi bi-star-fill' : 'bi bi-star';
            });
        });
        
        // 点击事件
        star.addEventListener('click', async function() {
            const rating = parseInt(this.dataset.rating);
            try {
                const response = await fetch(`/api/problem/${problemId}/typicality`, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ typicality: rating })
                });
                
                if (!response.ok) throw new Error('更新典型度评分失败');
                
                const result = await response.json();
                
                if (result.success) {
                    // 更新UI
                    setTypicalityRating(rating);
                    showToast(`典型度已更新为 ${rating}/5`, 'success');
                    
                    // 更新本地数据
                    if (problemData) {
                        problemData.typicality = rating;
                    }
                } else {
                    throw new Error(result.error || '更新失败');
                }
            } catch (error) {
                console.error('更新典型度失败:', error);
                showToast('更新典型度失败: ' + error.message, 'danger');
                
                // 恢复原有评分
                if (problemData) {
                    setTypicalityRating(problemData.typicality || 3);
                }
            }
        });
    });
    
    // 鼠标移出评分区恢复显示
    document.getElementById('typicality-rating').addEventListener('mouseleave', function() {
        if (problemData) {
            setTypicalityRating(problemData.typicality || 3);
        }
    });
}

// 填充编辑表单
function fillEditForm(data) {
    document.getElementById('edit-problem-content').value = data.problem_content || '';
    document.getElementById('edit-problem-category').value = data.problem_category || '';
    document.getElementById('edit-problem-subcategory').value = data.problem_subcategory || '';
    document.getElementById('edit-error-type').value = data.error_type || '';
    document.getElementById('edit-difficulty').value = data.difficulty || 3;
    document.getElementById('edit-error-analysis').value = data.error_analysis || '';
    document.getElementById('edit-correct-solution').value = data.correct_solution || '';
    document.getElementById('edit-tags').value = data.tags ? data.tags.join(',') : '';
    document.getElementById('edit-notes').value = data.notes || '';
}

// 设置编辑模式切换
function setupEditModeToggle() {
    document.querySelectorAll('.edit-mode-toggle').forEach(toggle => {
        toggle.addEventListener('click', function() {
            const targetId = this.dataset.target;
            const contentDiv = document.getElementById(targetId);
            const editDiv = document.getElementById(targetId + '-edit');
            
            if (contentDiv.style.display !== 'none') {
                // 切换到编辑模式
                contentDiv.style.display = 'none';
                editDiv.style.display = 'block';
                this.innerHTML = '<i class="bi bi-x-square"></i> 取消';
            } else {
                // 切换回查看模式
                contentDiv.style.display = 'block';
                editDiv.style.display = 'none';
                this.innerHTML = '<i class="bi bi-pencil-square"></i> 编辑';
            }
        });
    });
    
    // 绑定编辑表单提交事件
    document.getElementById('basic-info-form').addEventListener('submit', function(e) {
        e.preventDefault();
        saveBasicInfo();
    });
    
    document.getElementById('analysis-form').addEventListener('submit', function(e) {
        e.preventDefault();
        saveAnalysis();
    });
}

// 保存基本信息
async function saveBasicInfo() {
    try {
        const updateData = {
            problem_content: document.getElementById('edit-problem-content').value,
            problem_category: document.getElementById('edit-problem-category').value,
            problem_subcategory: document.getElementById('edit-problem-subcategory').value,
            error_type: document.getElementById('edit-error-type').value,
            difficulty: parseInt(document.getElementById('edit-difficulty').value),
            tags: document.getElementById('edit-tags').value.split(',').map(tag => tag.trim()).filter(tag => tag),
            notes: document.getElementById('edit-notes').value
        };
        
        const response = await fetch(`/api/problem/${problemId}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(updateData)
        });
        
        if (!response.ok) throw new Error('更新错题信息失败');
        
        const result = await response.json();
        
        if (result.success) {
            // 更新本地数据
            problemData = result.problem;
            
            // 更新页面显示
            document.getElementById('problem-content').textContent = problemData.problem_content || '无题目内容';
            document.getElementById('problem-category').textContent = problemData.problem_category || '未分类';
            document.getElementById('problem-subcategory').textContent = problemData.problem_subcategory || '未知';
            document.getElementById('error-type').textContent = problemData.error_type || '未知';
            document.getElementById('difficulty').textContent = `${problemData.difficulty || 3}/5`;
            
            // 更新标签
            const tagsContainer = document.getElementById('tags-container');
            tagsContainer.innerHTML = '';
            
            if (problemData.tags && problemData.tags.length > 0) {
                problemData.tags.forEach(tag => {
                    const tagBadge = document.createElement('span');
                    tagBadge.className = 'tag-badge';
                    tagBadge.textContent = tag;
                    tagsContainer.appendChild(tagBadge);
                });
            } else {
                const noTags = document.createElement('em');
                noTags.className = 'text-muted';
                noTags.textContent = '无标签';
                tagsContainer.appendChild(noTags);
            }
            
            // 更新用户补充说明
            const notesElement = document.getElementById('problem-notes');
            if (problemData.notes && problemData.notes.trim()) {
                notesElement.textContent = problemData.notes;
                notesElement.className = '';
            } else {
                notesElement.textContent = '无补充说明';
                notesElement.className = 'fst-italic text-muted';
            }
            
            // 切换回查看模式
            document.getElementById('basic-info-content').style.display = 'block';
            document.getElementById('basic-info-edit').style.display = 'none';
            document.querySelector('[data-target="basic-info-content"]').innerHTML = '<i class="bi bi-pencil-square"></i> 编辑';
            
            showToast('基本信息已更新', 'success');
        } else {
            throw new Error(result.error || '保存失败');
        }
    } catch (error) {
        console.error('更新错题基本信息失败:', error);
        showToast('更新基本信息失败: ' + error.message, 'danger');
    }
}

// 保存分析和解法
async function saveAnalysis() {
    try {
        const updateData = {
            error_analysis: document.getElementById('edit-error-analysis').value,
            correct_solution: document.getElementById('edit-correct-solution').value
        };
        
        const response = await fetch(`/api/problem/${problemId}`, {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(updateData)
        });
        
        if (!response.ok) throw new Error('更新错题分析失败');
        
        const result = await response.json();
        
        if (result.success) {
            // 更新本地数据
            problemData = result.problem;
            
            // 更新页面显示
            document.getElementById('error-analysis').textContent = problemData.error_analysis || '无错误分析';
            document.getElementById('correct-solution').textContent = problemData.correct_solution || '无解题思路';
            
            // 切换回查看模式
            document.getElementById('analysis-content').style.display = 'block';
            document.getElementById('analysis-edit').style.display = 'none';
            document.querySelector('[data-target="analysis-content"]').innerHTML = '<i class="bi bi-pencil-square"></i> 编辑';
            
            showToast('错误分析和解题思路已更新', 'success');
        } else {
            throw new Error(result.error || '保存失败');
        }
    } catch (error) {
        console.error('更新错题分析和解法失败:', error);
        showToast('更新分析和解法失败: ' + error.message, 'danger');
    }
}

// 确认删除
function confirmDelete() {
    if (confirm('确定要删除这道错题吗？此操作不可撤销！')) {
        deleteProblem();
    }
}

// 删除错题
async function deleteProblem() {
    try {
        const response = await fetch(`/api/problem/${problemId}`, {
            method: 'DELETE'
        });
        
        if (!response.ok) throw new Error('删除错题失败');
        
        const result = await response.json();
        
        if (result.success) {
            showToast('错题已删除', 'success');
            setTimeout(() => {
                window.location.href = '/problems';
            }, 1500);
        } else {
            throw new Error(result.error || '删除失败');
        }
    } catch (error) {
        console.error('删除错题失败:', error);
        showToast('删除错题失败: ' + error.message, 'danger');
    }
}

// 显示Toast通知
function showToast(message, type) {
    const toastContainer = document.querySelector('.toast-container');
    
    const toastHTML = `
        <div class="toast align-items-center text-white bg-${type} border-0 mb-2" role="alert" aria-live="assertive" aria-atomic="true">
            <div class="d-flex">
                <div class="toast-body">
                    ${message}
                </div>
                <button type="button" class="btn-close btn-close-white me-2 m-auto" data-bs-dismiss="toast" aria-label="Close"></button>
            </div>
        </div>
    `;
    
    toastContainer.insertAdjacentHTML('beforeend', toastHTML);
    
    const toastElement = toastContainer.lastElementChild;
    const toast = new bootstrap.Toast(toastElement, { autohide: true, delay: 3000 });
    toast.show();
    
    // 设置自动移除
    toastElement.addEventListener('hidden.bs.toast', function () {
        toastElement.remove();
    });
}