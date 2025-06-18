import React from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import remarkMath from 'remark-math';
import rehypeKatex from 'rehype-katex';

interface MarkdownRendererProps {
  content: string;
  className?: string;
}

// 预处理函数：处理LaTeX和空行
const preprocessContent = (content: string): string => {
  if (!content) return '';
  
  let processedContent = content;

  // 专门处理 bmatrix 环境
  processedContent = processedContent.replace(/\\begin{bmatrix}(.*?)\\end{bmatrix}/gs, (match, matrixContent) => {
    // 移除每行末尾 \ 之前和之后的空格
    let cleanedMatrix = matrixContent.replace(/\s*\\\\\s*/g, ' \\\\ '); // 保留一个空格以便阅读，KaTeX应能处理
    // 移除 & 周围的空格
    cleanedMatrix = cleanedMatrix.replace(/\s*&\s*/g, '&');
    // 移除行首和行尾的空格
    cleanedMatrix = cleanedMatrix.split(' \\\\ ').map((row: string) => row.trim()).join(' \\\\ ');
    return `\\begin{bmatrix}${cleanedMatrix}\\end{bmatrix}`;
  });

  // 处理空行：将多个连续的空行减少为最多一个空行
  processedContent = processedContent
    // 将 Windows 风格的换行符统一为 Unix 风格
    .replace(/\r\n/g, '\n')
    // 移除行尾空格
    .replace(/[ \t]+$/gm, '')
    // 处理列表中的空项：移除只包含数字和点号的行（空列表项）
    .replace(/^\s*\d+\.\s*$/gm, '')
    // 处理列表中连续的空行
    .replace(/(\d+\.\s*[^\n]*\n)\n+(?=\d+\.)/g, '$1\n')
    // 将3个或以上连续换行符替换为最多2个
    .replace(/\n{3,}/g, '\n\n')
    // 处理空格行：包含只有空格的行
    .replace(/\n[ \t]*\n/g, '\n\n')
    // 特别处理列表项之间的空行
    .replace(/(\d+\.\s*[^\n]*)\n\n+(\d+\.\s*[^\n]*)/g, '$1\n$2')
    // 移除开头和结尾的多余空行
    .replace(/^\n+/, '')
    .replace(/\n+$/, '');

  return processedContent;
};

export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({ 
  content, 
  className = '' 
}) => {
  // 预处理内容，处理LaTeX公式和空行
  const processedContent = preprocessContent(content);
  
  return (
    <div className={`markdown-content ${className}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        rehypePlugins={[
          [rehypeKatex, {
            throwOnError: false, // 不抛出错误，以红色显示错误内容
            errorColor: '#cc0000',
            strict: false, // 宽松模式
            trust: false, // 安全模式
            macros: {
              '\\RR': '\\mathbb{R}',
              '\\NN': '\\mathbb{N}',
              '\\ZZ': '\\mathbb{Z}',
              '\\QQ': '\\mathbb{Q}',
              '\\CC': '\\mathbb{C}'
            }
          }]
        ]}
        components={{
          // 自定义代码块渲染
          code: ({ node, inline, className, children, ...props }: any) => {
            return !inline ? (
              <pre className="code-block">
                <code className={className} {...props}>
                  {children}
                </code>
              </pre>
            ) : (
              <code className="inline-code" {...props}>
                {children}
              </code>
            );
          },
          // 自定义表格渲染
          table: ({ children }) => (
            <div className="table-wrapper">
              <table className="markdown-table">{children}</table>
            </div>
          ),
        }}
      >
        {processedContent}
      </ReactMarkdown>
    </div>
  );
};