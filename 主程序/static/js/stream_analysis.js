/**
 * 错题分析流式显示功能
 * 复用RAG2025中的流式显示逻辑，实现分析结果的实时显示与思维链展示
 */

// 处理流式响应
function handleStreamResponse(response, resultElement) {
    console.log("[INFO] 开始处理流式响应");
    
    // 判断当前页面类型
    const isReviewAnalysisPage = window.location.pathname.includes('/review') || 
                                window.location.pathname.includes('/review_analysis') ||
                                window.location.search.includes('review_id=');
    
    // 存储最终结果对象
    let finalResultObject = {
        success: true,
        题目类型: '未知类型',  // 提供默认值
        具体分支: '未知分支',  // 提供默认值
        错误类型: '未知错误类型',  // 提供默认值
        题目原文: '',
        错误分析: '',
        正确解法: '',
        知识点标签: []
    };
    
    // 累积的正文内容和推理内容
    let accumulatedContent = '';
    let accumulatedReasoning = '';
    
    // 用于存储不完整的数据
    let incompleteData = '';
    
    // 获取响应流
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    
    console.log("[DEBUG] 初始化结果对象:", JSON.stringify(finalResultObject));
    
    // 更新最终结果对象
    function updateFinalResultObject(newData) {
        if (!newData || typeof newData !== 'object') {
            console.warn("[WARN] 无效的结果对象数据:", newData);
            return;
        }
        
        console.log("[DEBUG] 更新结果对象，新数据:", JSON.stringify(newData));
        
        // 合并新数据到最终结果对象
        Object.assign(finalResultObject, newData);
        
        console.log("[DEBUG] 更新后的最终结果对象:", JSON.stringify(finalResultObject));
    }

    // 显示思维链和结果区域
    const reasoningChainElement = document.createElement('div');
    reasoningChainElement.className = 'reasoning-chain mt-3 mb-3 p-3 border rounded';
    reasoningChainElement.style.display = 'none';  // 默认隐藏
    reasoningChainElement.innerHTML = '<h5>推理过程</h5><div class="reasoning-content"></div>';
    resultElement.appendChild(reasoningChainElement);
    
    const resultContentElement = document.createElement('div');
    resultContentElement.className = 'result-content mt-3';
    resultElement.appendChild(resultContentElement);
    
    // 添加保存按钮容器(初始隐藏)
    const saveButtonContainer = document.createElement('div');
    saveButtonContainer.className = 'save-actions mt-3 text-center';
    saveButtonContainer.style.display = 'none';
    
    // 创建保存按钮
    const saveButton = document.createElement('button');
    saveButton.className = 'btn btn-success btn-lg';
    saveButton.innerHTML = '<i class="fas fa-save"></i> 保存分析结果';
    saveButton.onclick = function() {
        // 禁用按钮防止重复点击
        saveButton.disabled = true;
        saveButton.innerHTML = '<span class="spinner-border spinner-border-sm" role="status" aria-hidden="true"></span> 保存中...';
        
        // 保存分析结果
        saveAnalysisResult(finalResultObject, saveButton);
    };
    
    saveButtonContainer.appendChild(saveButton);
    
    // 只在回顾分析页面显示保存按钮容器
    // 通过URL判断当前页面类型
    if (!isReviewAnalysisPage) {
        saveButtonContainer.style.display = 'none';
    }
    
    resultElement.appendChild(saveButtonContainer);
    
    // 添加切换按钮
    const toggleButton = document.createElement('button');
    toggleButton.className = 'btn btn-sm btn-outline-secondary mt-2';
    toggleButton.textContent = '显示推理过程';
    toggleButton.onclick = function() {
        const isHidden = reasoningChainElement.style.display === 'none';
        reasoningChainElement.style.display = isHidden ? 'block' : 'none';
        toggleButton.textContent = isHidden ? '隐藏推理过程' : '显示推理过程';
    };
    resultElement.insertBefore(toggleButton, reasoningChainElement);
    
    // 处理流式响应
    function processStream() {
        reader.read().then(({ done, value }) => {
            if (done) {
                console.log("[DEBUG] 流式响应完成");
                
                // 只在回顾分析页面显示保存按钮
                if (isReviewAnalysisPage) {
                    saveButtonContainer.style.display = 'block';
                }
                
                // 确保finalResultObject包含完整的推理过程
                if (reasoningChainElement) {
                    const reasoningContent = reasoningChainElement.querySelector('.reasoning-content');
                    if (reasoningContent && reasoningContent.textContent) {
                        finalResultObject.推理过程 = reasoningContent.textContent;
                        console.log("[DEBUG] 已将推理过程添加到最终结果对象");
                    }
                }
                
                // 确保finalResultObject包含完整的综合分析
                if (resultContentElement && resultContentElement.innerHTML) {
                    // 提取综合分析部分
                    const analysisMatch = resultContentElement.innerHTML.match(/<h3>综合分析<\/h3>([\s\S]*?)(?:<h3>|$)/i);
                    if (analysisMatch && analysisMatch[1]) {
                        finalResultObject.综合分析 = analysisMatch[1].trim();
                        console.log("[DEBUG] 已将综合分析添加到最终结果对象");
                    }
                }
                
                console.log("[DEBUG] 完整的最终结果对象:", finalResultObject);
                return;
            }
            
            // 解码接收到的数据
            const chunk = decoder.decode(value, { stream: true });
            console.log("[DEBUG] 收到数据块，长度:" + chunk.length);
            
            // 处理数据块
            processChunk(chunk);
            
            // 继续读取
            processStream();
        }).catch(error => {
            console.error("[ERROR] 读取流时出错:", error);
            appendToResult(`\n\n**读取响应时出错:** ${error.message}`, resultContentElement);
        });
    }
    
    // 处理数据块
    function processChunk(chunk) {
        // 合并之前不完整的数据
        const data = incompleteData + chunk;
        incompleteData = '';
        
        // 按行分割数据
        const lines = data.split('\n');
        
        // 处理每一行，除了最后一行（可能不完整）
        for (let i = 0; i < lines.length - 1; i++) {
            const line = lines[i].trim();
            if (!line) continue;
            
            try {
                // 检查并处理SSE格式数据（以"data:"开头的行）
                let jsonString = line;
                if (line.startsWith('data:')) {
                    jsonString = line.substring(5).trim();
                }
                
                // 解析JSON数据
                const jsonData = JSON.parse(jsonString);
                
                // 处理不同类型的消息
                if (jsonData.choices && jsonData.choices.length > 0) {
                    // 处理deepseek格式的响应
                    const delta = jsonData.choices[0].delta;
                    
                    // 处理内容
                    if (delta.content && delta.content.trim()) {
                        // 累积内容
                        accumulatedContent += delta.content;
                        
                        // 更新界面 - 使用完整的Markdown和LaTeX渲染
                        renderMarkdownAndLatex(accumulatedContent, resultContentElement);
                        
                        // 保存最新内容到数据属性(用于后续可能的重新渲染)
                        resultContentElement.dataset.markdownContent = accumulatedContent;
                        
                        // 保存到finalResultObject
                        finalResultObject.综合分析 = accumulatedContent;
                        
                        // 确保LaTeX公式渲染
                        if (window.MathJax && typeof MathJax.typesetPromise === 'function') {
                            MathJax.typesetPromise([resultContentElement]).catch(function(err) {
                                console.error("[ERROR] 实时MathJax渲染失败:", err);
                            });
                        }
                    }
                    
                    // 处理推理内容
                    if (delta.reasoning_content && delta.reasoning_content.trim()) {
                        // 累积推理内容
                        accumulatedReasoning += delta.reasoning_content;
                        
                        const reasoningContent = reasoningChainElement.querySelector('.reasoning-content');
                        if (reasoningContent) {
                            // 使用完整的Markdown和LaTeX渲染
                            renderMarkdownAndLatex(accumulatedReasoning, reasoningContent);
                            
                            // 保存最新内容到数据属性
                            reasoningContent.dataset.markdownContent = accumulatedReasoning;
                            
                            reasoningChainElement.style.display = 'block';
                            toggleButton.textContent = '隐藏推理过程';
                            
                            // 保存到finalResultObject
                            finalResultObject.推理过程 = accumulatedReasoning;
                            
                            // 确保LaTeX公式渲染
                            if (window.MathJax && typeof MathJax.typesetPromise === 'function') {
                                MathJax.typesetPromise([reasoningContent]).catch(function(err) {
                                    console.error("[ERROR] 实时MathJax渲染失败:", err);
                                });
                            }
                        }
                    }
                    
                    // 如果是完成消息
                    if (jsonData.object === "chat.completion.chunk" && delta.content === "" && delta.reasoning_content === "") {
                        console.log("[INFO] 分析完成");
                        if (isReviewAnalysisPage) {
                            saveButtonContainer.style.display = 'block';
                        }
                    }
                }
                else if (jsonData.type === 'result') {
                    // 显示分析结果
                    if (jsonData.content) {
                        // 使用增强的Markdown渲染函数
                        renderMarkdownAndLatex(jsonData.content, resultContentElement);
                        
                        // 保存到finalResultObject - 原始内容更有价值
                        finalResultObject.综合分析 = jsonData.content;
                    }
                    
                    // 更新最终结果对象
                    if (jsonData.result_object) {
                        updateFinalResultObject(jsonData.result_object);
                    }
                    
                    // 只在回顾分析页面显示保存按钮
                    if (isReviewAnalysisPage) {
                        saveButtonContainer.style.display = 'block';
                    }
                } else if (jsonData.type === 'status') {
                    // 显示状态信息
                    console.log("[INFO] 状态更新:", jsonData.message);
                    
                    // 如果是完成状态，显示保存按钮
                    if (jsonData.status === 'completed' && isReviewAnalysisPage) {
                        saveButtonContainer.style.display = 'block';
                    }
                }
            } catch (error) {
                // 减少日志输出，避免向控制台大量打印错误信息
                if (error.message && error.message.includes("Unexpected token")) {
                    // 常见的流式解析错误，安静处理
                    console.debug("[DEBUG] 流式处理中的正常解析差异");
                } else {
                    // 其他错误仍然记录，但不输出原始数据
                    console.error("[ERROR] 解析JSON失败:", error.name, error.message);
                }
            }
        }
        
        // 保存最后一行（可能不完整）
        incompleteData = lines[lines.length - 1];
        
        // 继续处理流
        processStream();
    }
    
    // 开始处理流
    processStream();
}

// 将内容追加到结果元素
function appendToResult(content, targetElement) {
    if (!content || !targetElement) {
        console.warn("[DEBUG] 内容或目标元素为空");
        return;
    }
    
    // 初始化Markdown内容属性
    if (!targetElement.dataset.markdownContent) {
        targetElement.dataset.markdownContent = '';
    }
    
    // 累积Markdown内容
    targetElement.dataset.markdownContent += content;
    
    // 使用增强的Markdown渲染函数
    renderMarkdownAndLatex(targetElement.dataset.markdownContent, targetElement);
    
    // 确保LaTeX公式渲染
    if (window.MathJax && typeof MathJax.typesetPromise === 'function') {
        MathJax.typesetPromise([targetElement]).catch(function(err) {
            console.error("[ERROR] MathJax渲染失败:", err);
        });
    }
}

// 启用流式分析
async function enableStreamAnalysis() {
    // 修改分析问题函数
    window.analyzeProblemWithStream = async function(problem, modelType, subject) {
        // 准备请求数据
        const problemData = {
            id: problem.id,
            title: `错题${problem.id}`,
            problem_content: '',  // 修改为与ErrorProblem类一致的字段名
            notes: problem.notes || '',  // 修改为与ErrorProblem类一致的字段名
            error_type: '',  // 修改为与ErrorProblem类一致的字段名
            tags: []  // 修改为与ErrorProblem类一致的字段名
        };
        
        // 增加调试日志
        console.log("[DEBUG] 初始化问题数据:", JSON.stringify(problemData));
        
        // 如果有图片，处理图片内容
        if (problem.images && problem.images.length > 0) {
            problemData.problem_content = `[包含${problem.images.length}张图片的错题]`;
            
            // 使用原始的方法处理图片上传
            updateQueueItemStatus(problem.id, 'processing', '准备上传图片...');
            
            // 创建FormData对象
            const formData = new FormData();
            
            // 添加图片文件
            for (let i = 0; i < problem.images.length; i++) {
                formData.append('files', problem.images[i]);
            }
            
            // 添加模型类型和学科
            formData.append('model_type', modelType);
            formData.append('subject', subject);
            
            // 添加文字补充
            if (problem.notes) {
                formData.append('notes', problem.notes);
            }
            
            // 发送请求获取图片分析
            updateQueueItemStatus(problem.id, 'processing', '正在提取图片内容...');
            const response = await fetch('/api/upload-multi', {
                method: 'POST',
                body: formData
            });
            
            // 检查响应
            if (!response.ok) {
                const errorText = await response.text();
                throw new Error(`服务器错误 (${response.status}): ${errorText}`);
            }
            
            // 解析响应
            const result = await response.json();
            if (!result.success) {
                throw new Error(result.error || '未知错误');
            }
            
            // 更新问题内容，确保字段存在并有默认值
            if (result.analysis && typeof result.analysis === 'object') {
                console.log("[DEBUG] 从OCR分析结果获取内容:", JSON.stringify(result.analysis));
                
                // 如果后端返回了problem_id，使用该ID替代临时ID
                if (result.problem_id) {
                    console.log("[INFO] 从API响应中获取到有效的问题ID:", result.problem_id);
                    // 更新问题的ID以及UI元素的数据属性
                    problemData.id = result.problem_id;
                    problem.id = result.problem_id; // 同时更新原始问题对象
                    
                    // 将UUID存储到DOM元素属性中
                    const resultContainer = document.getElementById('analysisResults');
                    if (resultContainer) {
                        resultContainer.dataset.lastProblemId = result.problem_id;
                    }
                }
                
                // 优先使用题目原文字段（中文字段名）
                problemData.problem_content = result.analysis.题目原文 || 
                                             result.analysis.extracted_text || 
                                             `[包含${problem.images.length}张图片的错题]`;
                
                console.log("[DEBUG] 提取的题目内容:", problemData.problem_content);
                
                // 直接使用中文字段名结果
                if (result.analysis.错误类型) {
                    problemData.error_type = result.analysis.错误类型;
                }
                
                // 如果有题目类型，则使用它
                if (result.analysis.题目类型) {
                    problemData.problem_category = result.analysis.题目类型;
                }
                
                // 如果有具体分支，则使用它
                if (result.analysis.具体分支) {
                    problemData.problem_subcategory = result.analysis.具体分支;
                }
                
                // 提取知识点标签
                if (result.analysis.知识点标签) {
                    if (Array.isArray(result.analysis.知识点标签)) {
                        problemData.tags = result.analysis.知识点标签;
                    } else {
                        problemData.tags = String(result.analysis.知识点标签).split(/[,，]/).map(tag => tag.trim()).filter(tag => tag);
                    }
                }
            }

            console.log("[DEBUG] 最终发送到流式分析API的数据:", JSON.stringify(problemData));
        }
        
        // 准备流式分析请求
        updateQueueItemStatus(problem.id, 'processing', '正在进行流式分析...');
        
        // 创建结果元素
        const uniqueId = Date.now(); // 使用时间戳作为唯一标识符
        const resultId = `analysis-result-${uniqueId}`;
        const resultElement = document.createElement('div');
        resultElement.id = resultId;
        resultElement.className = 'problem-result mb-4 p-3 border rounded';
        // 仍然保存真实ID为数据属性，以便于后续处理
        resultElement.dataset.problemId = problem.id;
        
        // 添加标题
        const titleElement = document.createElement('h4');
        // 不再显示问题ID，只显示分析结果标题
        titleElement.textContent = '错题分析结果';
        // 为了调试目的，将ID存储为自定义数据属性而不是显示出来
        titleElement.dataset.problemId = problem.id;
        resultElement.appendChild(titleElement);
        
        // 添加加载指示器
        const loadingElement = document.createElement('div');
        loadingElement.className = 'loading-indicator text-center my-3';
        loadingElement.innerHTML = '<div class="spinner-border text-primary" role="status"><span class="visually-hidden">加载中...</span></div><div class="mt-2">正在生成分析结果...</div>';
        resultElement.appendChild(loadingElement);
        
        // 将结果添加到页面
        document.getElementById('analysisResults').appendChild(resultElement);
        
        try {
            // 发送流式分析请求
            console.log("[DEBUG] 准备发送流式分析请求...");
            
            // 构建URL，添加problem_id参数
            const url = new URL('/api/ai/stream-analysis', window.location.origin);
            
            // 检查problem.id是否为有效的UUID格式
            function isValidUUID(id) {
                const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
                return uuidRegex.test(id);
            }
            
            // 如果是测试或示例ID (如problem-1)，则尝试从当前URL获取实际问题ID
            let actualProblemId = problem.id;
            
            // 如果ID不是UUID格式，尝试从其他来源获取
            if (!isValidUUID(actualProblemId)) {
                // 首先尝试从DOM元素的data-problem-id属性获取
                const resultSection = document.getElementById('result-section');
                if (resultSection && resultSection.dataset.problemId) {
                    const domProblemId = resultSection.dataset.problemId;
                    if (isValidUUID(domProblemId)) {
                        console.log("[INFO] 从DOM元素获取到有效的问题ID:", domProblemId);
                        actualProblemId = domProblemId;
                    }
                }
                
                // 其次，尝试从analysisResults元素的data-last-problem-id属性获取
                if (!isValidUUID(actualProblemId)) {
                    const analysisResults = document.getElementById('analysisResults');
                    if (analysisResults && analysisResults.dataset.lastProblemId) {
                        const lastProblemId = analysisResults.dataset.lastProblemId;
                        if (isValidUUID(lastProblemId)) {
                            console.log("[INFO] 从analysisResults元素获取到最近的问题ID:", lastProblemId);
                            actualProblemId = lastProblemId;
                        }
                    }
                }
                
                // 如果仍然没有获取到有效ID，尝试从URL路径获取 (例如 /problem/{uuid})
                if (!isValidUUID(actualProblemId)) {
                    const pathParts = window.location.pathname.split('/');
                    if (pathParts.length > 2 && pathParts[1] === 'problem') {
                        const urlProblemId = pathParts[2];
                        if (isValidUUID(urlProblemId)) {
                            console.log("[INFO] 从URL获取到有效的问题ID:", urlProblemId);
                            actualProblemId = urlProblemId;
                        }
                    }
                }
                
                // 若还没有UUID，记录警告
                if (!isValidUUID(actualProblemId)) {
                    console.warn("[WARN] 无法获取有效的UUID格式问题ID，使用原始ID:", actualProblemId);
                }
            }
            
            // 添加problem_id作为查询参数
            url.searchParams.append('problem_id', actualProblemId);
            // 添加学科作为查询参数
            url.searchParams.append('subject', subject || 'math');
            // 添加回顾分析标记
            url.searchParams.append('is_review_analysis', window.location.pathname.includes('/review') || window.location.pathname.includes('/review_analysis') || window.location.search.includes('review_id='));
            
            console.log("[DEBUG] 发送流式分析请求:", url.toString());
            
            const streamResponse = await fetch(url, {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    problems: [problemData],
                    subject: subject || 'math',  // 确保传递学科信息
                    is_review_analysis: window.location.pathname.includes('/review') || window.location.pathname.includes('/review_analysis') || window.location.search.includes('review_id=')
                })
            });
            
            console.log("[DEBUG] 流式分析请求已发送，状态码:", streamResponse.status);
            
            // 检查响应
            if (!streamResponse.ok) {
                const errorText = await streamResponse.text();
                console.error("[ERROR] 流式分析请求失败:", streamResponse.status, errorText);
                throw new Error(`流式分析请求失败 (${streamResponse.status}): ${errorText}`);
            }
            
            // 移除加载指示器
            loadingElement.remove();
            
            // 处理流式响应
            handleStreamResponse(streamResponse, resultElement);
            
            // 更新队列状态
            updateQueueItemStatus(problem.id, 'completed');
            
            // 返回成功
            return {
                success: true,
                problemId: problem.id
            };
            
        } catch (error) {
            // 处理错误
            console.error("[ERROR] 分析错题${problem.id}时出错:", error);
            
            // 添加错误信息
            const errorElement = document.createElement('div');
            errorElement.className = 'alert alert-danger';
            errorElement.textContent = `分析失败: ${error.message}`;
            resultElement.appendChild(errorElement);
            
            // 更新队列状态
            updateQueueItemStatus(problem.id, 'error', error.message);
            
            // 移除加载指示器
            loadingElement.remove();
            
            // 返回失败
            return {
                success: false,
                problemId: problem.id,
                error: error.message
            };
        }
    };
    
    console.log("[DEBUG] 流式分析功能已启用");
}

// 在文档加载完成后初始化
document.addEventListener('DOMContentLoaded', function() {
    // 检查是否有必要的脚本和元素
    if (typeof marked !== 'undefined') {
        console.log("[DEBUG] 初始化流式分析功能");
        enableStreamAnalysis();
        
        // 尝试替换原始的分析函数
        if (window.analyzeProblem && typeof window.analyzeProblemWithStream === 'function') {
            // 备份原始函数，以便在需要时回退
            window.originalAnalyzeProblem = window.analyzeProblem;
            
            // 用流式版本替换
            window.analyzeProblem = window.analyzeProblemWithStream;
            
            console.log("[DEBUG] 已将分析函数替换为流式版本");
        }
    } else {
        console.warn("[DEBUG] 缺少必要的脚本库(marked)，流式分析功能无法初始化");
    }
});

// 强化版的Markdown和LaTeX渲染函数，结合了之前的保护LaTeX的方法
function renderMarkdownAndLatex(content, targetElement) {
    if (!content || !targetElement) {
        console.warn("[WARNING] 渲染内容或目标元素为空");
        return;
    }
    
    try {
        // 步骤1: 暂时"保护"LaTeX公式，避免被Markdown解析器修改
        const latexPlaceholders = [];
        
        // 保护行内公式: $...$
        let processedContent = content.replace(/\$([^\$]+)\$/g, function(match) {
            latexPlaceholders.push(match);
            return `LATEXPLACEHOLDER${latexPlaceholders.length - 1}`;
        });
        
        // 保护块级公式: $$...$$
        processedContent = processedContent.replace(/\$\$([\s\S]+?)\$\$/g, function(match) {
            latexPlaceholders.push(match);
            return `LATEXPLACEHOLDER${latexPlaceholders.length - 1}`;
        });
        
        // 保护 \( ... \) 格式的公式
        processedContent = processedContent.replace(/\\\(([\s\S]+?)\\\)/g, function(match) {
            latexPlaceholders.push(match);
            return `LATEXPLACEHOLDER${latexPlaceholders.length - 1}`;
        });
        
        // 保护 \[ ... \] 格式的公式
        processedContent = processedContent.replace(/\\\[([\s\S]+?)\\\]/g, function(match) {
            latexPlaceholders.push(match);
            return `LATEXPLACEHOLDER${latexPlaceholders.length - 1}`;
        });
        
        // 步骤2: 优先尝试使用markdown-it渲染
        let htmlContent = '';
        
        // 使用markdown-it (如果可用)
        if (typeof window.markdownit !== 'undefined') {
            try {
                // 创建markdown-it实例
                if (!window.markdownItInstance) {
                    window.markdownItInstance = window.markdownit({
                        html: true,
                        linkify: true,
                        typographer: true,
                        breaks: true
                    });
                }
                
                // 渲染Markdown
                htmlContent = window.markdownItInstance.render(processedContent);
            } catch (mdItError) {
                console.warn("[WARNING] markdown-it渲染失败，将尝试使用marked:", mdItError.message);
                // 失败后尝试使用marked
            }
        }
        
        // 备用: 使用marked.js (如果markdown-it不可用或失败)
        if (!htmlContent && typeof window.marked !== 'undefined') {
            try {
                // 配置marked选项
                marked.setOptions({
                    breaks: true,      // 将回车转换为<br>
                    gfm: true,         // 启用GitHub风格的Markdown
                    headerIds: true,   // 为标题添加ID
                    mangle: false      // 不转义autolink和标题
                });
                
                // 渲染Markdown
                htmlContent = marked.parse(processedContent);
            } catch (markedError) {
                console.warn("[WARNING] marked渲染失败:", markedError.message);
                // 如果marked也失败，将使用简单HTML转换
            }
        }
        
        // 最终备用: 简单HTML转换
        if (!htmlContent) {
            console.warn("[WARNING] 所有Markdown渲染器都不可用，使用简单HTML转换");
            htmlContent = processedContent
                .replace(/\n\n/g, '</p><p>')
                .replace(/\n/g, '<br>')
                .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
                .replace(/\*(.*?)\*/g, '<em>$1</em>')
                .replace(/```([\s\S]*?)```/g, '<pre><code>$1</code></pre>');
            htmlContent = `<p>${htmlContent}</p>`;
        }
        
        // 步骤3: 恢复LaTeX公式
        const finalContent = htmlContent.replace(/LATEXPLACEHOLDER(\d+)/g, function(match, index) {
            return latexPlaceholders[parseInt(index)];
        });
        
        // 更新目标元素
        targetElement.innerHTML = finalContent;
        
        // 确保LaTeX公式渲染 (使用MathJax)
        if (window.MathJax && typeof MathJax.typesetPromise === 'function') {
            MathJax.typesetPromise([targetElement]).catch(function(err) {
                console.error("[ERROR] MathJax渲染失败:", err);
            });
        }
        
        // 增强样式
        enhanceMarkdownRendering(targetElement);
        return true;
    } catch (error) {
        console.error("[ERROR] 渲染失败，降级为纯文本显示:", error);
        targetElement.textContent = content; // 最终降级：直接显示原始文本
        return false;
    }
}

// 增强Markdown样式渲染函数 - 改进版本
function enhanceMarkdownRendering(element) {
    if (!element) return;
    
    try {
        // 添加markdown-body类以应用GitHub样式
        element.classList.add('markdown-body');
        
        // 设置基本字体大小
        element.style.fontSize = '16px';
        
        // 设置表格样式
        const tables = element.querySelectorAll('table');
        tables.forEach(table => {
            table.style.borderCollapse = 'collapse';
            table.style.width = '100%';
            table.style.margin = '16px 0';
            table.style.fontSize = '0.95em';
            
            // 设置表格边框
            const cells = table.querySelectorAll('th, td');
            cells.forEach(cell => {
                cell.style.border = '1px solid #ddd';
                cell.style.padding = '8px 12px';
                cell.style.textAlign = 'left';
            });
            
            // 设置表头样式
            const headers = table.querySelectorAll('th');
            headers.forEach(header => {
                header.style.backgroundColor = '#f5f5f5';
                header.style.fontWeight = 'bold';
            });
            
            // 设置奇偶行样式
            const rows = table.querySelectorAll('tr');
            rows.forEach((row, index) => {
                if (index % 2 === 1) {
                    row.style.backgroundColor = '#f9f9f9';
                }
            });
        });
        
        // 设置代码块样式
        const codeBlocks = element.querySelectorAll('pre code');
        codeBlocks.forEach(block => {
            block.style.display = 'block';
            block.style.padding = '12px 16px';
            block.style.borderRadius = '4px';
            block.style.backgroundColor = '#f6f8fa';
            block.style.fontFamily = 'Consolas, Monaco, "Andale Mono", monospace';
            block.style.fontSize = '0.95em';
            block.style.overflowX = 'auto';
            block.style.whiteSpace = 'pre';
            block.style.lineHeight = '1.5';
        });
        
        // 设置标题样式
        for (let i = 1; i <= 6; i++) {
            const headings = element.querySelectorAll(`h${i}`);
            headings.forEach(heading => {
                heading.style.marginTop = '24px';
                heading.style.marginBottom = '16px';
                heading.style.fontWeight = 'bold';
                heading.style.lineHeight = '1.25';
                
                // 不同级别标题使用不同的字体大小
                switch (i) {
                    case 1: heading.style.fontSize = '2em'; heading.style.borderBottom = '1px solid #eaecef'; break;
                    case 2: heading.style.fontSize = '1.5em'; heading.style.borderBottom = '1px solid #eaecef'; break;
                    case 3: heading.style.fontSize = '1.25em'; break;
                    case 4: heading.style.fontSize = '1em'; break;
                    case 5: heading.style.fontSize = '0.875em'; break;
                    case 6: heading.style.fontSize = '0.85em'; break;
                }
            });
        }
        
        // 设置列表样式
        const lists = element.querySelectorAll('ul, ol');
        lists.forEach(list => {
            list.style.paddingLeft = '2em';
            list.style.marginBottom = '16px';
        });
        
        // 设置普通段落样式
        const paragraphs = element.querySelectorAll('p');
        paragraphs.forEach(p => {
            p.style.marginTop = '0';
            p.style.marginBottom = '16px';
            p.style.lineHeight = '1.6';
        });
        
        // 设置引用块样式
        const blockquotes = element.querySelectorAll('blockquote');
        blockquotes.forEach(blockquote => {
            blockquote.style.paddingLeft = '1em';
            blockquote.style.borderLeft = '4px solid #ddd';
            blockquote.style.color = '#6a737d';
            blockquote.style.marginLeft = '0';
            blockquote.style.marginRight = '0';
        });
        
        // 确保公式正确对齐和显示
        const mathElements = element.querySelectorAll('.katex, .katex-display');
        mathElements.forEach(math => {
            math.style.fontSize = '1.1em';
            math.style.fontFamily = 'KaTeX_Math, Times New Roman, serif';
        });
        
    } catch (error) {
        console.error("[ERROR] 增强渲染样式失败:", error);
    }
}

// 保存分析结果到数据库
function saveAnalysisResult(analysis, saveButton) {
    console.log("[DEBUG] 尝试保存分析结果");
    
    // 获取问题ID - 优先从analysis对象中获取
    let problemId = null;
    
    // 首先从analysis对象中查找problem_id
    if (analysis && analysis.problem_id) {
        problemId = analysis.problem_id;
    }
    
    // 其次从URL参数中获取
    if (!problemId || !isValidUUID(problemId)) {
        const urlParams = new URLSearchParams(window.location.search);
        const urlProblemId = urlParams.get('problem_id');
        if (urlProblemId) {
            problemId = urlProblemId;
        }
    }
    
    // 最后从URL路径获取 (例如 /problem/{uuid})
    if (!problemId || !isValidUUID(problemId)) {
        const pathParts = window.location.pathname.split('/');
        if (pathParts.length > 2 && pathParts[1] === 'problem') {
            const urlProblemId = pathParts[2];
            if (isValidUUID(urlProblemId)) {
                problemId = urlProblemId;
            }
        }
    }
    
    if (!problemId) {
        console.warn("[WARN] 无法获取有效的problem_id，无法保存分析结果");
        
        if (saveButton) {
            // 显示错误消息
            saveButton.className = 'btn btn-danger btn-lg';
            saveButton.innerHTML = '<i class="fas fa-times"></i> 保存失败: 无法获取有效的问题ID';
            
            // 3秒后恢复按钮状态
            setTimeout(() => {
                saveButton.disabled = false;
                saveButton.className = 'btn btn-success btn-lg';
                saveButton.innerHTML = '<i class="fas fa-save"></i> 重试保存';
            }, 3000);
        }
        
        return;
    }
    
    const subject = analysis.subject || 'math';
    
    // 获取结果元素中显示的完整内容
    const resultContentElement = document.querySelector('.result-content');
    if (resultContentElement && resultContentElement.textContent) {
        // 查看分析对象中是否已有错误分析字段
        if (!analysis['错误分析'] || analysis['错误分析'] === '') {
            console.log("[INFO] 设置错误分析内容，当前内容为空");
            analysis['错误分析'] = resultContentElement.textContent;
            console.log(`[DEBUG] 设置错误分析内容长度: ${analysis['错误分析'].length}`);
        }
    }
    
    // 构建保存请求数据
    const saveData = {
        problem_id: problemId,
        analysis_result: analysis
    };
    
    console.log("[DEBUG] 发送保存请求，数据:", JSON.stringify(saveData));
    
    // 发送保存请求到标准problem_api接口
    fetch(`/api/problem/update-analysis?subject=${subject}`, {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json',
        },
        body: JSON.stringify(saveData)
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            console.log("[INFO] 分析结果保存成功");
            
            // 显示成功消息在状态区域
            const statusElement = document.getElementById('status-message');
            if (statusElement) {
                statusElement.textContent = '分析已完成并成功保存';
                statusElement.style.color = '#28a745'; // 绿色表示成功
            }
            
            // 如果提供了保存按钮，更新按钮状态
            if (saveButton) {
                // 显示成功消息
                saveButton.className = 'btn btn-success btn-lg';
                saveButton.innerHTML = '<i class="fas fa-check"></i> 保存成功';
                
                // 添加查看链接
                if (!document.querySelector('.view-problem-btn')) {
                    const viewButton = document.createElement('a');
                    viewButton.href = `/problem/${problemId}`;
                    viewButton.className = 'btn btn-primary btn-lg ms-2 view-problem-btn';
                    viewButton.innerHTML = '<i class="fas fa-eye"></i> 查看错题';
                    viewButton.target = '_blank';
                    saveButton.parentNode.appendChild(viewButton);
                }
                
                // 3秒后恢复按钮状态
                setTimeout(() => {
                    saveButton.disabled = false;
                    saveButton.innerHTML = '<i class="fas fa-save"></i> 保存分析结果';
                }, 3000);
            }
        } else {
            console.error("[ERROR] 保存分析结果失败:", data.error);
            
            // 如果提供了保存按钮，更新按钮状态
            if (saveButton) {
                // 显示错误消息
                saveButton.className = 'btn btn-danger btn-lg';
                saveButton.innerHTML = '<i class="fas fa-times"></i> 保存失败: ' + (data.error || '未知错误');
                
                // 3秒后恢复按钮状态
                setTimeout(() => {
                    saveButton.disabled = false;
                    saveButton.className = 'btn btn-success btn-lg';
                    saveButton.innerHTML = '<i class="fas fa-save"></i> 重试保存';
                }, 3000);
            }
        }
    })
    .catch(error => {
        console.error("[ERROR] 保存分析请求出错:", error);
        
        // 如果提供了保存按钮，更新按钮状态
        if (saveButton) {
            // 显示错误消息
            saveButton.className = 'btn btn-danger btn-lg';
            saveButton.innerHTML = '<i class="fas fa-times"></i> 网络错误，请重试';
            
            // 3秒后恢复按钮状态
            setTimeout(() => {
                saveButton.disabled = false;
                saveButton.className = 'btn btn-success btn-lg';
                saveButton.innerHTML = '<i class="fas fa-save"></i> 重试保存';
            }, 3000);
        }
    });
}

// 检查是否有效的UUID
function isValidUUID(id) {
    if (!id || typeof id !== 'string') return false;
    const uuidRegex = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
    return uuidRegex.test(id);
}
