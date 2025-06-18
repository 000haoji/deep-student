import React, { useRef, useEffect } from 'react';
import { AnkiCardTemplate } from '../types';

export const IframePreview: React.FC<{ htmlContent: string; cssContent: string; }> = ({ htmlContent, cssContent }) => {
  const iframeRef = useRef<HTMLIFrameElement>(null);

  useEffect(() => {
    const iframe = iframeRef.current;
    if (!iframe || !iframe.contentWindow) return;

    const doc = iframe.contentWindow.document;
    doc.open();
    doc.write(`
      <!DOCTYPE html>
      <html>
        <head>
          <meta charset="UTF-8">
          <style>
            body { 
              margin: 0; 
              padding: 0; 
              display: flex; 
              justify-content: center; 
              align-items: center; 
              height: 100%;
              box-sizing: border-box;
              background-color: transparent;
            }
            /* 添加填空题高亮样式 */
            .cloze-revealed {
              background: #FFD700 !important;
              color: #2c3e50 !important;
              padding: 2px 8px !important;
              border-radius: 6px !important;
              font-weight: 600 !important;
              box-shadow: 0 2px 4px rgba(0,0,0,0.2) !important;
            }
            ${cssContent}
          </style>
        </head>
        <body>
          ${htmlContent}
        </body>
      </html>
    `);
    doc.close();

    const updateHeight = () => {
      if (iframe && iframe.contentWindow && iframe.contentWindow.document.body) {
        const body = iframe.contentWindow.document.body;
        const contentHeight = body.scrollHeight;
        iframe.style.height = `${contentHeight}px`;
      }
    };

    const timer = setTimeout(updateHeight, 100);
    const observer = new ResizeObserver(updateHeight);
    observer.observe(doc.body);

    return () => {
      clearTimeout(timer);
      observer.disconnect();
    };
  }, [htmlContent, cssContent]);

  return (
    <iframe
      ref={iframeRef}
      title="Card Preview"
      style={{ width: '100%', border: 'none', minHeight: '80px', verticalAlign: 'top' }}
      scrolling="no"
    />
  );
};

export const renderCardPreview = (template: string, templateData: AnkiCardTemplate, actualCardData?: any) => {
  let rendered = template;
  
  rendered = rendered.replace(/\{\{Front\}\}/g, templateData.preview_front);
  rendered = rendered.replace(/\{\{Back\}\}/g, templateData.preview_back);
  
  // 检测是否为背面模板
  const isBackTemplate = rendered.includes('完整内容') || rendered.includes('{{text:') || 
                         (templateData.preview_back && rendered.includes(templateData.preview_back));
  
  
  // 根据模板类型提供合适的示例数据
  const getTemplateSpecificData = () => {
    const templateName = templateData.name || '';
    const templateId = templateData.id || '';
    
    if (templateName.includes('学术') || templateId === 'academic-card') {
      return {
        'Deck': 'AI学习卡片',
        'Notes': '这是一个关于生物学概念的补充说明，帮助更好地理解DNA复制过程。',
        'Example': '例如：在细胞分裂过程中，DNA双螺旋结构会解开，每条链作为模板合成新的互补链，从而保证遗传信息的准确传递。',
        'Source': '分子生物学教材 - 第五章',
        'Tags': '生物学,分子生物学,DNA'
      };
    } else if (templateName.includes('编程') || templateId === 'code-card') {
      return {
        'Deck': 'AI学习卡片',
        'Notes': '这是一个关于Python编程的补充说明，帮助更好地理解列表操作。',
        'Example': '例如：使用append()方法可以向列表末尾添加元素，使用extend()可以添加多个元素。',
        'Source': 'Python官方文档',
        'Tags': 'Python,编程,数据结构'
      };
    } else if (templateName.includes('选择题') || templateId === 'choice-card') {
      return {
        'Deck': 'AI学习卡片',
        'Notes': '这是一个关于物理概念的补充说明，帮助更好地理解牛顿定律。',
        'Example': '例如：当汽车突然刹车时，车内的人会向前倾，这就是惯性的体现。',
        'Source': '大学物理教材 - 第二章',
        'Tags': '物理,力学,基础概念'
      };
    } else if (templateName.includes('填空') || templateId === 'cloze-card') {
      return {
        'Deck': 'AI学习卡片',
        'Notes': '这是一个关于社会工作的补充说明，帮助更好地理解基本要素。',
        'Example': '例如：在社区服务中，社会工作者通过专业技能帮助有需要的个人和家庭解决问题。',
        'Source': '社会工作基础教材 - 第一章',
        'Tags': '社会学,社会工作,基础概念'
      };
    } else {
      return {
        'Deck': 'AI学习卡片',
        'Notes': '这是一个学习概念的补充说明，帮助更好地理解相关知识点。',
        'Example': '例如：通过实践和反复练习，可以更好地掌握和运用所学知识。',
        'Source': '学习资料',
        'Tags': '学习,知识,基础'
      };
    }
  };

  const specificData = getTemplateSpecificData();
  const sampleData: Record<string, string> = {
    ...specificData,
    'Code': 'my_list = [1, 2, 3, 4, 5]\nprint(my_list)\n# 输出: [1, 2, 3, 4, 5]',
    'Text': '社会工作的要素包括：1. {{c1::社会工作者}}：服务和帮助的提供者；2. {{c2::受助者}}：遇到困难且需要帮助的人。',
    'Hint': '记住社会工作的两个基本要素。',
    'Question': '下列哪个是牛顿第一定律的内容？',
    'OptionA': 'F=ma',
    'OptionB': '作用力与反作用力',
    'OptionC': '惯性定律',
    'OptionD': '万有引力定律',
    'optiona': 'F=ma',
    'optionb': '作用力与反作用力',
    'optionc': '惯性定律',
    'optiond': '万有引力定律',
    'Correct': 'C',
    'correct': 'C',
    'Explanation': '牛顿第一定律又称惯性定律，表述物体在没有外力作用时保持静止或匀速直线运动状态。',
    'explanation': '牛顿第一定律又称惯性定律，表述物体在没有外力作用时保持静止或匀速直线运动状态。'
  };
  
  if (actualCardData) {
    Object.keys(sampleData).forEach(key => {
      if (actualCardData[key.toLowerCase()] !== undefined) {
        sampleData[key] = actualCardData[key.toLowerCase()];
      } else if (actualCardData[key] !== undefined) {
        sampleData[key] = actualCardData[key];
      }
    });
  }
  
  rendered = rendered.replace(/\{\{cloze:(\w+)\}\}/g, (match, fieldName) => {
    if (sampleData[fieldName]) {
      if (isBackTemplate) {
        // 背面显示完整内容（高亮答案）
        return sampleData[fieldName].replace(/\{\{c(\d+)::([^}]+?)\}\}/g, 
          '<span class="cloze-revealed">$2</span>'
        );
      } else {
        // 正面显示挖空
        return sampleData[fieldName].replace(/\{\{c(\d+)::([^}]+?)\}\}/g, 
          '<span class="cloze">[...]</span>'
        );
      }
    }
    return match;
  });
  
  rendered = rendered.replace(/\{\{text:(\w+)\}\}/g, (match, fieldName) => {
    if (sampleData[fieldName]) {
      return sampleData[fieldName].replace(/\{\{c(\d+)::([^}]+?)\}\}/g, '$2');
    }
    return match;
  });
  
  rendered = rendered.replace(/\{\{#(\w+)\}\}([\s\S]*?)\{\{\/\1\}\}/g, (match, fieldName, content) => {
    if (sampleData[fieldName]) {
      return content.replace(new RegExp(`\\{\\{${fieldName}\\}\\}`, 'g'), sampleData[fieldName]);
    }
    return '';
  });
  
  // 根据模板类型设置标签
  const getTemplateTags = () => {
    const templateName = templateData.name || '';
    const templateId = templateData.id || '';
    
    if (templateName.includes('学术') || templateId === 'academic-card') {
      return { array: ['生物学', '分子生物学', 'DNA'], string: '生物学, 分子生物学, DNA' };
    } else if (templateName.includes('编程') || templateId === 'code-card') {
      return { array: ['Python', '编程', '数据结构'], string: 'Python, 编程, 数据结构' };
    } else if (templateName.includes('选择题') || templateId === 'choice-card') {
      return { array: ['物理', '力学', '基础'], string: '物理, 力学, 基础' };
    } else if (templateName.includes('填空') || templateId === 'cloze-card') {
      return { array: ['社会学', '社会工作', '基础'], string: '社会学, 社会工作, 基础' };
    } else {
      return { array: ['学习', '知识', '基础'], string: '学习, 知识, 基础' };
    }
  };

  const templateTags = getTemplateTags();
  
  rendered = rendered.replace(/\{\{#Tags\}\}([\s\S]*?)\{\{\/Tags\}\}/g, (match, tagTemplate) => {
    return templateTags.array.map(tag => tagTemplate.replace(/\{\{\.\}\}/g, tag)).join('');
  });
  
  rendered = rendered.replace(/\{\{Tags\}\}/g, templateTags.string);
  
  Object.entries(sampleData).forEach(([key, value]) => {
    if (!rendered.includes(`{{cloze:${key}}}`) && !rendered.includes(`{{text:${key}}}`)) {
      rendered = rendered.replace(new RegExp(`\\{\\{${key}\\}\\}`, 'g'), value);
    }
  });
  
  rendered = rendered.replace(/\{\{\.\}\}/g, '');
  rendered = rendered.replace(/\{\{[^}]*\}\}/g, '');
  
  return rendered;
};
