/**
 * 渲染助手函数 - 帮助解决MathJax和Markdown渲染问题
 */

/**
 * 尝试重新渲染页面上的LaTeX公式
 */
function reRenderMathJax() {
    if (window.MathJax) {
        console.log("手动触发MathJax重新渲染...");
        
        try {
            // 清理旧的渲染
            if (typeof MathJax.typesetClear === 'function') {
                MathJax.typesetClear();
            }
            
            // 重新渲染
            if (typeof MathJax.typeset === 'function') {
                MathJax.typeset();
                return true;
            } else if (typeof MathJax.typesetPromise === 'function') {
                MathJax.typesetPromise().then(() => {
                    console.log("重新渲染完成");
                });
                return true;
            }
        } catch (e) {
            console.error("重新渲染失败:", e);
        }
    }
    console.warn("MathJax未加载，无法重新渲染");
    return false;
}

/**
 * 检查页面上是否存在未渲染的LaTeX公式
 * @returns {Array} 未渲染的公式列表
 */
function checkUnrenderedFormulas() {
    // 检查内联公式
    const inlinePattern = /\$((?!\$).)+\$/g;
    const blockPattern = /\$\$((?!\$\$).)+\$\$/g;
    
    const htmlContent = document.body.innerHTML;
    const inlineMatches = htmlContent.match(inlinePattern) || [];
    const blockMatches = htmlContent.match(blockPattern) || [];
    
    console.log(`发现 ${inlineMatches.length} 个可能未渲染的内联公式，${blockMatches.length} 个块级公式`);
    
    return {
        inline: inlineMatches,
        block: blockMatches
    };
}

/**
 * 显示页面上LaTeX公式的渲染状态
 */
function showLatexStatus() {
    // 寻找已渲染的MathJax元素
    const renderedElements = document.querySelectorAll('.MathJax');
    console.log(`已渲染的MathJax元素数量: ${renderedElements.length}`);
    
    // 检查未渲染的公式
    const unrendered = checkUnrenderedFormulas();
    console.log("未渲染公式样本:", unrendered.inline.slice(0, 3), unrendered.block.slice(0, 3));
    
    // 检查MathJax状态
    if (window.MathJax) {
        console.log("MathJax版本信息:", MathJax.version);
        console.log("MathJax配置:", MathJax);
    } else {
        console.warn("MathJax未加载");
    }
    
    return {
        renderedCount: renderedElements.length,
        unrenderedInline: unrendered.inline.length,
        unrenderedBlock: unrendered.block.length,
        mathjaxLoaded: !!window.MathJax
    };
}

// 添加功能，支持API设置测试相关的调试功能
/**
 * 测试API端点连接，显示在控制台
 * @param {string} apiType - API类型，例如 'openai' 或 'deepseek'
 * @param {string} apiUrl - API端点URL
 * @param {string} apiKey - API密钥
 */
function testApiEndpoint(apiType, apiUrl, apiKey) {
    console.log(`正在测试 ${apiType} API 端点: ${apiUrl}`);
    
    fetch('/api/settings/test-connection', {
        method: 'POST',
        headers: {
            'Content-Type': 'application/json'
        },
        body: JSON.stringify({
            api_type: apiType,
            api_url: apiUrl,
            api_key: apiKey
        })
    })
    .then(response => response.json())
    .then(data => {
        if (data.success) {
            console.log(`✓ ${apiType} API 连接成功，响应时间: ${data.response_time.toFixed(2)}秒`);
        } else {
            console.error(`✗ ${apiType} API 连接失败: ${data.message}`);
        }
    })
    .catch(error => {
        console.error(`测试 ${apiType} API 时发生错误:`, error);
    });
}

// 导出扩展的RenderHelper对象
if (window.RenderHelper) {
    window.RenderHelper.testApiEndpoint = testApiEndpoint;
} else {
    window.RenderHelper = {
        testApiEndpoint
    };
}

// 导出函数
window.RenderHelper = {
    reRenderMathJax,
    checkUnrenderedFormulas,
    showLatexStatus,
    testApiEndpoint
};
