import React, { useEffect, useState, useRef } from 'react';
import { useSubject } from '../../contexts/SubjectContext';
import './UnifiedSubjectSelector.css';

interface UnifiedSubjectSelectorProps {
  mode?: 'enabled' | 'all' | 'existing';
  includeAllOption?: boolean;
  placeholder?: string;
  disabled?: boolean;
  className?: string;
  value?: string;
  onChange?: (subject: string) => void;
  existingSubjects?: string[]; // 用于'existing'模式
}

const UnifiedSubjectSelector: React.FC<UnifiedSubjectSelectorProps> = ({
  mode = 'enabled',
  includeAllOption = false,
  placeholder = '选择科目',
  disabled = false,
  className = '',
  value,
  onChange,
  existingSubjects = []
}) => {
  const { 
    currentSubject, 
    setCurrentSubject, 
    getEnabledSubjects, 
    getAllSubjects, 
    loading 
  } = useSubject();

  // 自定义下拉框的状态
  const [isOpen, setIsOpen] = useState(false);
  const selectRef = useRef<HTMLDivElement>(null);
  const itemsRef = useRef<HTMLDivElement>(null);

  // 确定使用的值和onChange处理器
  const selectedValue = value !== undefined ? value : currentSubject;
  const handleChange = onChange !== undefined ? onChange : setCurrentSubject;

  // 根据模式获取科目选项
  const getSubjectOptions = (): string[] => {
    switch (mode) {
      case 'enabled':
        return getEnabledSubjects();
      case 'all':
        return getAllSubjects();
      case 'existing':
        return existingSubjects;
      default:
        return getEnabledSubjects();
    }
  };

  const subjectOptions = getSubjectOptions();

  // 构建完整的选项列表
  const allOptions = [
    ...(includeAllOption ? ['全部科目'] : []),
    ...subjectOptions
  ];

  // 检查是否在content-header环境中
  const isInContentHeader = className.includes('header-subject-selector');

  // 计算下拉框的最佳位置
  const calculateDropdownPosition = () => {
    if (!selectRef.current || !itemsRef.current) return;

    const selectRect = selectRef.current.getBoundingClientRect();
    const itemsHeight = Math.min(500, allOptions.length * (isInContentHeader ? 36 : 44) + 20);
    const viewportHeight = window.innerHeight;
    const spaceBelow = viewportHeight - selectRect.bottom;
    const spaceAbove = selectRect.top;

    console.log('🎯 计算下拉框位置:', {
      selectRect,
      itemsHeight,
      spaceBelow,
      spaceAbove,
      isInContentHeader
    });

    // 设置CSS变量用于动态计算
    selectRef.current.style.setProperty('--select-top', `${selectRect.bottom}px`);

    // 先尝试使用相对定位，避免fixed定位的复杂问题
    itemsRef.current.style.position = 'absolute';
    itemsRef.current.style.left = '0';
    itemsRef.current.style.right = '0';
    itemsRef.current.style.width = 'auto';
    itemsRef.current.style.zIndex = '9999';

    if (isInContentHeader) {
      // 临时添加类名到content-header以修改overflow
      const contentHeader = selectRef.current.closest('.content-header');
      if (contentHeader) {
        contentHeader.classList.add('dropdown-open');
      }
    }
    
    // 如果下方空间不够且上方空间更多，则向上展开
    if (spaceBelow < itemsHeight && spaceAbove > spaceBelow) {
      itemsRef.current.style.top = 'auto';
      itemsRef.current.style.bottom = '100%';
      itemsRef.current.style.marginTop = '0';
      itemsRef.current.style.marginBottom = '8px';
    } else {
      itemsRef.current.style.top = '100%';
      itemsRef.current.style.bottom = 'auto';
      itemsRef.current.style.marginTop = '8px';
      itemsRef.current.style.marginBottom = '0';
    }
    
    // 重置可能影响定位的样式
    itemsRef.current.style.transform = 'none';
  };

  // 清理header的overflow样式
  const cleanupHeaderOverflow = () => {
    if (isInContentHeader && selectRef.current) {
      const contentHeader = selectRef.current.closest('.content-header');
      if (contentHeader) {
        contentHeader.classList.remove('dropdown-open');
      }
    }
  };

  // 监听科目状态变化
  useEffect(() => {
    console.log('🎯 科目选择器状态监听:', {
      currentSubject,
      selectedValue,
      subjectOptions,
      loading,
      mode
    });
  }, [currentSubject, selectedValue, subjectOptions.length, loading, mode]);

  // 处理选项点击
  const handleOptionClick = (event: React.MouseEvent, optionValue: string) => {
    event.preventDefault();
    event.stopPropagation();
    
    console.log('🎯 科目选择器变更:', {
      oldValue: selectedValue,
      newValue: optionValue,
      mode,
      handleChangeFunction: handleChange.name || 'anonymous'
    });
    
    handleChange(optionValue);
    setIsOpen(false);
    cleanupHeaderOverflow();
  };

  // 处理选择器主体点击
  const handleSelectClick = (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    
    if (!disabled) {
      const newOpenState = !isOpen;
      setIsOpen(newOpenState);
      
      if (newOpenState) {
        // 在下一帧计算位置，确保DOM已更新
        requestAnimationFrame(() => {
          calculateDropdownPosition();
        });
      } else {
        cleanupHeaderOverflow();
      }
    }
  };

  // 处理点击外部关闭
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (selectRef.current && !selectRef.current.contains(event.target as Node)) {
        setIsOpen(false);
        cleanupHeaderOverflow();
      }
    };

    const handleResize = () => {
      if (isOpen) {
        calculateDropdownPosition();
      }
    };

    const handleScroll = () => {
      if (isOpen && isInContentHeader) {
        // 对于header中的下拉框，在滚动时重新计算位置
        calculateDropdownPosition();
      }
    };

    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      window.addEventListener('resize', handleResize);
      window.addEventListener('scroll', handleScroll, true);
    }
    
    return () => {
      document.removeEventListener('mousedown', handleClickOutside);
      window.removeEventListener('resize', handleResize);
      window.removeEventListener('scroll', handleScroll, true);
    };
  }, [isOpen, isInContentHeader]);

  // 组件卸载时清理
  useEffect(() => {
    return () => {
      cleanupHeaderOverflow();
    };
  }, []);

  // 获取显示的文本
  const getDisplayText = () => {
    if (loading) return '加载中...';
    if (!selectedValue) return placeholder;
    return selectedValue;
  };

  if (loading) {
    return (
      <div className={`unified-subject-selector loading ${className}`}>
        <div className="custom-select">
          <div className="select-selected disabled">
            <span className="selected-value">加载中...</span>
            <span className="select-arrow">⌵</span>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className={`unified-subject-selector ${className}`} ref={selectRef}>
      <div className="custom-select">
        <div 
          className={`select-selected ${disabled ? 'disabled' : ''}`}
          onClick={handleSelectClick}
          onMouseDown={(e) => e.stopPropagation()}
        >
          <span className="selected-value">{getDisplayText()}</span>
          <span 
            className="select-arrow"
            style={{
              transform: isOpen ? 'rotate(180deg)' : 'rotate(0deg)'
            }}
          >
            ⌵
          </span>
        </div>
        {isOpen && !disabled && (
          <div className="select-items show" ref={itemsRef}>
            {!selectedValue && (
              <div 
                className="select-item disabled-option"
                style={{ color: '#a0aec0', cursor: 'not-allowed' }}
              >
                {placeholder}
              </div>
            )}
            {allOptions.map(option => (
              <div
                key={option}
                className={`select-item ${option === selectedValue ? 'selected' : ''}`}
                onClick={(e) => handleOptionClick(e, option)}
                onMouseDown={(e) => e.stopPropagation()}
              >
                {option}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
};

export { UnifiedSubjectSelector };
export default UnifiedSubjectSelector;