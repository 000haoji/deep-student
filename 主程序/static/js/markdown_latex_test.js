/**
 * Markdown与LaTeX流式输出测试
 * 此脚本用于模拟LLM生成的流式输出数据，测试Markdown和LaTeX在流式显示中的渲染效果
 */

// 预设示例
const examples = {
    markdown: `# Markdown基础示例

这是一个**粗体**文本，这是*斜体*文本。

## 列表示例
* 无序列表项1
* 无序列表项2
  * 嵌套列表项

1. 有序列表项1
2. 有序列表项2

## 代码示例
\`\`\`python
def hello_world():
    print("Hello, World!")
\`\`\`

> 这是一段引用文本
> 第二行引用

---

[这是一个链接](https://example.com)

![这是一个图片描述](https://via.placeholder.com/150)

| 表头1 | 表头2 | 表头3 |
|-------|-------|-------|
| 单元格1 | 单元格2 | 单元格3 |
| 单元格4 | 单元格5 | 单元格6 |`,

    latex: `# LaTeX公式示例

## 行内公式

质能方程: $E=mc^2$

欧拉公式: $e^{i\\pi} + 1 = 0$

二次方程: $ax^2 + bx + c = 0$

圆的面积: $A = \\pi r^2$

## 块级公式

质能方程:

$$E=mc^2$$

欧拉公式:

$$e^{i\\pi} + 1 = 0$$

二次方程的求根公式:

$$x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}$$

高斯分布:

$$f(x) = \\frac{1}{\\sigma\\sqrt{2\\pi}}e^{-\\frac{1}{2}(\\frac{x-\\mu}{\\sigma})^2}$$

矩阵示例:

$$
\\begin{pmatrix}
a & b & c \\\\
d & e & f \\\\
g & h & i
\\end{pmatrix}
$$`,

    mixed: `# Markdown与LaTeX混合示例

## 数学问题分析

在解决二次方程 $ax^2 + bx + c = 0$ 时，我们可以使用判别式 $\\Delta = b^2 - 4ac$ 来确定方程的解的性质：

* 若 $\\Delta > 0$，方程有两个不同的实数解
* 若 $\\Delta = 0$，方程有两个相等的实数解
* 若 $\\Delta < 0$，方程有两个共轭复数解

二次方程的求根公式为：

$$x = \\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}$$

## 物理问题分析

在分析物体运动时，位置 $s$ 与时间 $t$ 的关系可以表示为：

$$s = s_0 + v_0t + \\frac{1}{2}at^2$$

其中：
* $s_0$ 是初始位置
* $v_0$ 是初速度
* $a$ 是加速度

## 代码实现

\`\`\`python
import math

def solve_quadratic(a, b, c):
    # 计算判别式
    delta = b**2 - 4*a*c
    
    if delta > 0:
        # 两个不同的实数解
        x1 = (-b + math.sqrt(delta)) / (2*a)
        x2 = (-b - math.sqrt(delta)) / (2*a)
        return (x1, x2)
    elif delta == 0:
        # 两个相等的实数解
        x = -b / (2*a)
        return (x, x)
    else:
        # 两个共轭复数解
        real = -b / (2*a)
        imag = math.sqrt(-delta) / (2*a)
        return (complex(real, imag), complex(real, -imag))
\`\`\``,

    complex: `# 复杂排版与公式测试

## 多重嵌套列表与公式

* 数学分析
  * 微积分
    * 定积分定义: $\\int_a^b f(x) dx = \\lim_{n \\to \\infty} \\sum_{i=1}^{n} f(x_i) \\Delta x$
    * 微分方程: $\\frac{dy}{dx} + P(x)y = Q(x)$
  * 线性代数
    * 矩阵乘法: $C = A \\times B$ 其中 $c_{ij} = \\sum_{k=1}^{n} a_{ik}b_{kj}$
* 物理学
  * 经典力学
    * 牛顿第二定律: $\\vec{F} = m\\vec{a}$
  * 电磁学
    * 麦克斯韦方程组:
      $$\\nabla \\cdot \\vec{E} = \\frac{\\rho}{\\varepsilon_0}$$
      $$\\nabla \\cdot \\vec{B} = 0$$
      $$\\nabla \\times \\vec{E} = -\\frac{\\partial\\vec{B}}{\\partial t}$$
      $$\\nabla \\times \\vec{B} = \\mu_0\\vec{J} + \\mu_0\\varepsilon_0\\frac{\\partial\\vec{E}}{\\partial t}$$

## 复杂表格与公式

| 函数名 | 定义 | 导数 | 图像特点 |
|--------|------|------|----------|
| 正弦函数 | $\\sin(x)$ | $\\cos(x)$ | 周期函数，振幅为1 |
| 指数函数 | $e^x$ | $e^x$ | 恒正，增长迅速 |
| 对数函数 | $\\ln(x)$ | $\\frac{1}{x}$ | $x>0$时有定义，缓慢增长 |
| 高斯函数 | $e^{-x^2}$ | $-2xe^{-x^2}$ | 钟形曲线，对称于y轴 |

## 算法分析与复杂度

时间复杂度通常用大O符号表示：$O(n)$、$O(n\\log n)$、$O(n^2)$ 等。

排序算法的复杂度比较：

| 算法 | 最佳时间复杂度 | 平均时间复杂度 | 最差时间复杂度 | 空间复杂度 |
|------|----------------|----------------|----------------|------------|
| 冒泡排序 | $O(n)$ | $O(n^2)$ | $O(n^2)$ | $O(1)$ |
| 快速排序 | $O(n\\log n)$ | $O(n\\log n)$ | $O(n^2)$ | $O(\\log n)$ |
| 归并排序 | $O(n\\log n)$ | $O(n\\log n)$ | $O(n\\log n)$ | $O(n)$ |

归并排序的递归关系可以表示为：$T(n) = 2T(\\frac{n}{2}) + O(n)$

## 复杂的数学证明

**定理**：对于任意正整数 $n$，有 $\\sum_{i=1}^{n} i = \\frac{n(n+1)}{2}$

**证明**：令 $S = \\sum_{i=1}^{n} i = 1 + 2 + ... + n$

同时，$S = n + (n-1) + ... + 1$

两式相加得：$2S = (n+1) + (n+1) + ... + (n+1) = n(n+1)$

因此，$S = \\frac{n(n+1)}{2}$

这就证明了我们的定理。$\\square$`
};

// 强化版的Markdown和LaTeX渲染函数
function renderMarkdownAndLatex(content, targetElement) {
    if (!content || !targetElement) {
        console.warn("[WARNING] 渲染内容或目标元素为空");
        return;
    }
    
    try {
        console.log("[DEBUG] 开始渲染Markdown和LaTeX");
        
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
                console.log("[DEBUG] 使用markdown-it渲染成功");
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
                console.log("[DEBUG] 使用marked渲染成功");
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
        console.log("[DEBUG] HTML内容已更新");
        
        // 确保LaTeX公式渲染 (使用MathJax)
        if (window.MathJax) {
            try {
                if (typeof MathJax.typesetPromise === 'function') {
                    console.log("[DEBUG] 尝试使用MathJax.typesetPromise渲染LaTeX");
                    MathJax.typesetPromise([targetElement]).catch(function(err) {
                        console.error("[ERROR] MathJax渲染失败:", err);
                    });
                } else if (typeof MathJax.typeset === 'function') {
                    console.log("[DEBUG] 尝试使用MathJax.typeset渲染LaTeX");
                    MathJax.typeset([targetElement]);
                }
            } catch (mjError) {
                console.error("[ERROR] MathJax处理异常:", mjError);
            }
        } else {
            console.warn("[WARNING] MathJax未定义，LaTeX公式将不会被渲染");
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

// 增强Markdown样式渲染函数
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
            block.style.padding = '12px';
            block.style.borderRadius = '4px';
            block.style.fontSize = '0.9em';
            block.style.fontFamily = 'Consolas, Monaco, "Andale Mono", monospace';
            
            // 添加语法高亮（如果有highlight.js）
            if (typeof hljs !== 'undefined') {
                try {
                    hljs.highlightElement(block);
                } catch (error) {
                    console.warn("[WARNING] 代码高亮失败:", error);
                }
            }
        });
        
        // 设置引用块样式
        const blockquotes = element.querySelectorAll('blockquote');
        blockquotes.forEach(blockquote => {
            blockquote.style.borderLeft = '4px solid #ddd';
            blockquote.style.paddingLeft = '16px';
            blockquote.style.margin = '16px 0';
            blockquote.style.color = '#555';
        });
        
        // 设置列表样式
        const lists = element.querySelectorAll('ul, ol');
        lists.forEach(list => {
            list.style.paddingLeft = '32px';
            list.style.margin = '12px 0';
        });
    } catch (error) {
        console.warn("[WARNING] 增强样式应用失败:", error.message);
    }
}

// 直接实现渲染函数，不依赖外部文件
function directRenderMarkdownAndLatex(content, element) {
    console.log('[DEBUG] 开始渲染Markdown和LaTeX，内容长度：' + content.length);
    
    try {
        // 检查marked库是否正确加载
        if (typeof marked === 'undefined') {
            console.error('[ERROR] marked库未加载！');
            element.innerHTML = '<div class="alert alert-danger">错误：marked库未加载</div>';
            element.innerHTML += '<pre>' + content + '</pre>';
            return;
        }
        
        console.log('[DEBUG] marked库已加载，版本：' + (marked.version || 'unknown'));
        
        // 配置marked选项
        marked.setOptions({
            renderer: new marked.Renderer(),
            headerIds: true,
            gfm: true,
            breaks: true,
            pedantic: false,
            sanitize: false,
            smartLists: true,
            smartypants: false
        });
        
        // 使用marked渲染Markdown内容
        try {
            const htmlContent = marked.parse(content);
            element.innerHTML = htmlContent;
            console.log('[DEBUG] Markdown渲染完成');
        } catch (markdownError) {
            console.error('[ERROR] Markdown渲染失败：', markdownError);
            // 使用备用方法尝试渲染
            element.innerHTML = content
                .replace(/^# (.*?)$/gm, '<h1>$1</h1>')
                .replace(/^## (.*?)$/gm, '<h2>$1</h2>')
                .replace(/^### (.*?)$/gm, '<h3>$1</h3>')
                .replace(/\*\*(.*?)\*\*/g, '<strong>$1</strong>')
                .replace(/\*(.*?)\*/g, '<em>$1</em>')
                .replace(/`(.*?)`/g, '<code>$1</code>')
                .replace(/\n/g, '<br>');
            console.log('[DEBUG] 使用备用渲染方法');
        }
        
        // 确保MathJax正确渲染LaTeX
        if (window.MathJax) {
            console.log('[DEBUG] MathJax已加载，尝试渲染LaTeX');
            try {
                if (typeof MathJax.typesetPromise === 'function') {
                    MathJax.typesetPromise([element]).then(() => {
                        console.log('[DEBUG] MathJax渲染完成');
                    }).catch(function(err) {
                        console.error('[ERROR] MathJax渲染失败:', err);
                    });
                } else if (typeof MathJax.typeset === 'function') {
                    MathJax.typeset([element]);
                    console.log('[DEBUG] MathJax渲染完成（使用typeset方法）');
                } else {
                    console.warn('[WARNING] MathJax不支持typesetPromise或typeset方法');
                }
            } catch (mathJaxError) {
                console.error('[ERROR] MathJax处理异常：', mathJaxError);
            }
        } else {
            console.warn('[WARNING] MathJax未加载，LaTeX将无法渲染');
        }
        
        // 添加语法高亮
        if (typeof hljs !== 'undefined') {
            try {
                const codeBlocks = element.querySelectorAll('pre code');
                console.log('[DEBUG] 发现' + codeBlocks.length + '个代码块，应用语法高亮');
                codeBlocks.forEach(block => {
                    hljs.highlightElement(block);
                });
            } catch (hlError) {
                console.error('[ERROR] 语法高亮应用失败：', hlError);
            }
        } else {
            console.warn('[WARNING] highlight.js未加载，代码块将不会应用语法高亮');
        }
    } catch (error) {
        console.error('[ERROR] 渲染总体失败：', error);
        element.innerHTML = '<div class="alert alert-danger">错误: ' + error.message + '</div>';
        element.innerHTML += '<pre>' + content + '</pre>';
    }
}

// 页面初始化函数
function initializeTestPage() {
    console.log('[INFO] 初始化Markdown和LaTeX测试页面');
    
    // 获取DOM元素
    const clientContainer = document.getElementById('client-result');
    const serverContainer = document.getElementById('server-result');
    const contentInput = document.getElementById('content-input');
    const speedInput = document.getElementById('speed-input');
    const speedDisplay = document.getElementById('speed-display');
    
    // 初始化速度滑块事件
    speedInput.addEventListener('input', function() {
        speedDisplay.textContent = this.value + ' ms';
    });
    
    // 确保MathJax已正确初始化
    if (!window.MathJax) {
        console.error('[ERROR] MathJax未加载或未初始化');
        document.getElementById('status-message').textContent = 'MathJax未加载，LaTeX公式可能不会被正确渲染';
        document.getElementById('status-message').classList.add('alert', 'alert-warning');
        document.getElementById('status-message').classList.remove('d-none');
    }
    
    // 确保marked已正确初始化
    if (!window.marked) {
        console.error('[ERROR] marked.js未加载或未初始化');
        document.getElementById('status-message').textContent = 'marked.js未加载，Markdown可能不会被正确渲染';
        document.getElementById('status-message').classList.add('alert', 'alert-warning');
        document.getElementById('status-message').classList.remove('d-none');
    }
    
    // 注册示例按钮事件
    const exampleButtons = document.querySelectorAll('.example-btn');
    exampleButtons.forEach(button => {
        button.addEventListener('click', function() {
            const exampleKey = this.getAttribute('data-example');
            if (exampleKey && examples[exampleKey]) {
                contentInput.value = examples[exampleKey];
            }
        });
    });
    
    // 注册客户端流式渲染按钮事件
    document.getElementById('client-stream-btn').addEventListener('click', function() {
        const content = contentInput.value;
        const speed = parseInt(speedInput.value) || 50;
        
        if (!content) {
            alert('请输入要渲染的内容');
            return;
        }
        
        simulateClientStream(content, clientContainer, speed);
    });
    
    // 注册服务器流式渲染按钮事件
    document.getElementById('server-stream-btn').addEventListener('click', function() {
        const content = contentInput.value;
        const speed = parseInt(speedInput.value) || 50;
        
        if (!content) {
            alert('请输入要渲染的内容');
            return;
        }
        
        simulateServerStream(content, serverContainer, speed);
    });
    
    // 注册直接渲染按钮事件
    document.getElementById('direct-render-btn').addEventListener('click', function() {
        const content = contentInput.value;
        
        if (!content) {
            alert('请输入要渲染的内容');
            return;
        }
        
        // 清空结果容器
        clientContainer.innerHTML = '';
        serverContainer.innerHTML = '';
        
        // 创建结果内容容器
        const clientResultContent = document.createElement('div');
        clientResultContent.className = 'result-content';
        clientContainer.appendChild(clientResultContent);
        
        const serverResultContent = document.createElement('div');
        serverResultContent.className = 'result-content';
        serverContainer.appendChild(serverResultContent);
        
        // 使用系统同款渲染函数直接渲染
        console.log('[INFO] 使用renderMarkdownAndLatex直接渲染');
        
        // 渲染到客户端和服务器容器
        const clientSuccess = renderMarkdownAndLatex(content, clientResultContent);
        const serverSuccess = renderMarkdownAndLatex(content, serverResultContent);
        
        // 显示渲染状态
        if (clientSuccess && serverSuccess) {
            console.log('[INFO] 直接渲染成功');
        } else {
            console.error('[ERROR] 直接渲染失败');
        }
    });
    
    // 初始化示例内容
    if (examples.default) {
        contentInput.value = examples.default;
    }
    
    console.log('[INFO] 测试页面初始化完成');
}

// 在DOM加载完成后初始化页面
document.addEventListener('DOMContentLoaded', function() {
    console.log('[INFO] DOM加载完成，开始初始化测试页面');
    initializeTestPage();
});

// 加载预设示例
function loadExample(type) {
    if (examples[type]) {
        document.getElementById('testContent').value = examples[type];
    }
}

// 清空结果区域
function clearResult() {
    document.getElementById('resultArea').innerHTML = '';
    document.getElementById('thinkingIndicator').style.display = 'none';
}

// 开始测试
function startTest() {
    const content = document.getElementById('testContent').value;
    if (!content.trim()) {
        alert('请输入要测试的内容');
        return;
    }
    
    const speed = parseInt(document.getElementById('streamSpeed').value);
    const useServer = document.getElementById('useServerStream').checked;
    const resultArea = document.getElementById('resultArea');
    
    // 清空结果区域
    clearResult();
    
    // 显示思考指示器
    document.getElementById('thinkingIndicator').style.display = 'block';
    
    console.log('[DEBUG] 开始测试，使用' + (useServer ? '服务器' : '客户端') + '模拟，速度：' + speed + 'ms/字符');
    console.log('[DEBUG] 测试内容长度：' + content.length + '字符');
    
    if (useServer) {
        // 使用服务器模拟流式输出
        simulateServerStream(content, resultArea, speed);
    } else {
        // 使用客户端模拟流式输出
        simulateClientStream(content, resultArea, speed);
    }
}

// 模拟客户端流式输出（不依赖服务器API）
function simulateClientStream(content, targetElement, speed) {
    const chars = content.split('');
    let currentContent = '';
    let index = 0;
    
    // 清空目标元素并添加思考指示器
    targetElement.innerHTML = '';
    const thinkingIndicator = document.getElementById('thinkingIndicator');
    thinkingIndicator.style.display = 'block';
    
    // 创建结果内容容器
    const resultContentElement = document.createElement('div');
    resultContentElement.className = 'result-content';
    targetElement.appendChild(resultContentElement);
    
    function streamNextChar() {
        if (index < chars.length) {
            // 添加下一个字符
            currentContent += chars[index];
            index++;
            
            // 使用系统同款渲染函数进行渲染
            renderMarkdownAndLatex(currentContent, resultContentElement);
            
            // 设置下一个字符的定时器
            setTimeout(streamNextChar, speed);
        } else {
            // 完成流式输出
            thinkingIndicator.style.display = 'none';
            
            // 确保最终渲染
            renderMarkdownAndLatex(currentContent, resultContentElement);
        }
    }
    
    // 开始流式输出第一个字符
    setTimeout(streamNextChar, speed);
}

// 模拟服务器流式输出
function simulateServerStream(content, targetElement, speed) {
    // 显示思考指示器
    const thinkingIndicator = document.getElementById('thinkingIndicator');
    thinkingIndicator.style.display = 'block';
    
    // 清空目标元素，准备接收新内容
    targetElement.innerHTML = '';
    
    // 创建结果内容容器
    const resultContentElement = document.createElement('div');
    resultContentElement.className = 'result-content';
    targetElement.appendChild(resultContentElement);
    
    // 当handleStreamResponse不可用时使用的备用函数
    const fallbackStreamHandler = function(response) {
        const reader = response.body.getReader();
        let contentSoFar = '';
        
        function processNextChunk() {
            return reader.read().then(({done, value}) => {
                if (done) {
                    console.log('[DEBUG] 流式读取完成');
                    thinkingIndicator.style.display = 'none';
                    // 确保最终渲染
                    renderMarkdownAndLatex(contentSoFar, resultContentElement);
                    return;
                }
                
                // 处理获取的数据
                const chunk = new TextDecoder().decode(value);
                const lines = chunk.split('\n\n');
                
                for (const line of lines) {
                    if (line.startsWith('data: ')) {
                        try {
                            const data = JSON.parse(line.substring(6));
                            
                            if (data.choices && data.choices[0] && data.choices[0].delta) {
                                contentSoFar += data.choices[0].delta.content || '';
                            }
                        } catch (e) {
                            console.warn('[WARNING] 解析数据出错:', e);
                        }
                    }
                }
                
                // 使用系统同款渲染函数
                renderMarkdownAndLatex(contentSoFar, resultContentElement);
                
                // 处理下一块数据
                return processNextChunk();
            });
        }
        
        return processNextChunk();
    };
    
    // 发送请求到服务器
    fetch('/api/test/markdown-latex-stream', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify({
            content: content,
            speed: speed
        })
    })
    .then(response => {
        if (!response.ok) {
            throw new Error('服务器响应错误');
        }
        
        // 使用fallbackStreamHandler处理响应
        // 不依赖现有的handleStreamResponse函数，确保测试页面完全独立工作
        fallbackStreamHandler(response);
    })
    .catch(error => {
        console.error('[ERROR] 流处理错误:', error);
        targetElement.innerHTML = '<div class="alert alert-danger">错误: ' + error.message + '</div>';
        thinkingIndicator.style.display = 'none';
    });
}
