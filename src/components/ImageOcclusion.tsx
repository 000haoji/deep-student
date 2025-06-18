import React, { useState, useRef, useCallback, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import './ImageOcclusion.css';

interface TextRegion {
  text: string;
  bbox: [number, number, number, number];
  confidence: number;
  region_id: string;
}

interface ImageOcrResponse {
  success: boolean;
  text_regions: TextRegion[];
  full_text: string;
  image_width: number;
  image_height: number;
  error_message?: string;
}

interface OcclusionMask {
  mask_id: string;
  bbox: [number, number, number, number];
  original_text: string;
  hint?: string;
  mask_style: MaskStyle;
}

interface MaskStyle {
  SolidColor?: { color: string };
  BlurEffect?: { intensity: number };
  Rectangle?: { color: string; opacity: number };
}

interface ImageOcclusionCard {
  id: string;
  task_id: string;
  image_path: string;
  image_base64?: string;
  image_width: number;
  image_height: number;
  masks: OcclusionMask[];
  title: string;
  description?: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  subject: string;
}

interface ImageOcclusionResponse {
  success: boolean;
  card?: ImageOcclusionCard;
  error_message?: string;
}

const ImageOcclusion: React.FC = () => {
  const [selectedImage, setSelectedImage] = useState<string | null>(null);
  const [isProcessing, setIsProcessing] = useState(false);
  const [textRegions, setTextRegions] = useState<TextRegion[]>([]);
  const [selectedRegions, setSelectedRegions] = useState<Set<string>>(new Set());
  const [cardTitle, setCardTitle] = useState('');
  const [cardDescription, setCardDescription] = useState('');
  const [cardSubject, setCardSubject] = useState('数学');
  const [cardTags, setCardTags] = useState('');
  const [maskStyle, setMaskStyle] = useState<MaskStyle>({ Rectangle: { color: '#FF0000', opacity: 0.7 } });
  const [imageScale, setImageScale] = useState(1);
  const [createdCard, setCreatedCard] = useState<ImageOcclusionCard | null>(null);
  const [showPreview, setShowPreview] = useState(false);
  const [imageLoaded, setImageLoaded] = useState(false);
  const [useHighResolution, setUseHighResolution] = useState(true);

  const fileInputRef = useRef<HTMLInputElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [originalImageDimensions, setOriginalImageDimensions] = useState<{ width: number; height: number } | null>(null);

  const handleImageUpload = useCallback((event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      const base64 = e.target?.result as string;
      setSelectedImage(base64);
      setTextRegions([]);
      setSelectedRegions(new Set());
      setCreatedCard(null);
      setImageLoaded(false);
    };
    reader.readAsDataURL(file);
  }, []);

  const extractTextCoordinates = useCallback(async () => {
    if (!selectedImage) return;

    setIsProcessing(true);
    try {
      const base64Data = selectedImage.split(',')[1];
      const response = await invoke<ImageOcrResponse>('extract_image_text_coordinates', {
        request: {
          image_base64: base64Data,
          extract_coordinates: true,
          target_text: null,
          vl_high_resolution_images: useHighResolution,
        },
      });

      if (response.success) {
        console.log('✅ OCR识别成功:', {
          区域数量: response.text_regions.length,
          图片尺寸: `${response.image_width}x${response.image_height}`,
          全文: response.full_text.substring(0, 100) + ('...'),
          示例区域: response.text_regions.slice(0, 3).map((r, i) => ({
            索引: i,
            文字: r.text,
            坐标: `[${r.bbox.join(', ')}]`,
            置信度: `${(r.confidence * 100).toFixed(1)}%`
          }))
        });
        
        setTextRegions(response.text_regions);
        const dimensions = { width: response.image_width, height: response.image_height };
        setOriginalImageDimensions(dimensions);
        
        // 如果图片已加载，立即计算缩放比例；否则等待图片加载完成
        if (imageLoaded) {
          console.log('🔄 OCR完成，图片已加载，立即计算缩放比例');
          updateImageScale(response.image_width, response.image_height, true);
        } else {
          console.log('⏳ OCR完成，等待图片加载完成后计算缩放比例');
        }
      } else {
        const errorMsg = response.error_message || '未知错误';
        console.error('❌ OCR识别失败:', errorMsg);
        alert(`OCR识别失败: ${errorMsg}\n\n请检查:\n1. 网络连接是否正常\n2. API配置是否正确\n3. 图片格式是否支持`);
      }
    } catch (error) {
      console.error('❌ OCR识别异常:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      alert(`OCR识别异常: ${errorMessage}\n\n可能的原因:\n1. 网络连接中断\n2. 服务器响应超时\n3. 图片文件损坏`);
    } finally {
      setIsProcessing(false);
    }
  }, [selectedImage, useHighResolution]);

  const updateImageScale = useCallback((originalWidth: number, originalHeight: number, forceUpdate = false) => {
    if (!imageRef.current) {
      console.warn('⚠️ 图片引用不存在，无法计算缩放比例');
      return;
    }

    // 等待图片完全渲染后再计算尺寸
    const calculateScale = () => {
      const img = imageRef.current!;
      const displayWidth = img.clientWidth;
      const displayHeight = img.clientHeight;
      const naturalWidth = img.naturalWidth;
      const naturalHeight = img.naturalHeight;
      
      console.log('🔍 尺寸信息收集:', {
        原始OCR尺寸: `${originalWidth}x${originalHeight}`,
        图片自然尺寸: `${naturalWidth}x${naturalHeight}`,
        显示尺寸: `${displayWidth}x${displayHeight}`,
        图片加载状态: imageLoaded
      });
      
      // 验证原始尺寸
      if (originalWidth <= 0 || originalHeight <= 0) {
        console.error('❌ 原始图片尺寸无效:', { originalWidth, originalHeight });
        setImageScale(1);
        return;
      }

      // 验证显示尺寸
      if (displayWidth <= 0 || displayHeight <= 0) {
        if (!forceUpdate) {
          console.warn('⚠️ 显示尺寸无效，延迟重试:', { displayWidth, displayHeight });
          // 延迟重试，给图片更多时间渲染
          setTimeout(() => updateImageScale(originalWidth, originalHeight, true), 100);
          return;
        } else {
          console.error('❌ 显示尺寸持续无效，使用默认缩放比例');
          setImageScale(1);
          return;
        }
      }

      // 计算缩放比例 - 确保图片按比例缩放
      const scaleX = displayWidth / originalWidth;
      const scaleY = displayHeight / originalHeight;
      const scale = Math.min(scaleX, scaleY); // 使用较小的缩放比例保持图片比例
      
      console.log('✅ 缩放计算完成:', {
        X轴缩放: scaleX.toFixed(4),
        Y轴缩放: scaleY.toFixed(4),
        最终缩放: scale.toFixed(4)
      });
      
      setImageScale(scale);
    };

    // 如果图片已加载，立即计算；否则等待加载完成
    if (imageLoaded) {
      calculateScale();
    } else {
      console.log('⏳ 等待图片加载完成后计算缩放比例');
      // 等待下一个渲染周期
      requestAnimationFrame(calculateScale);
    }
  }, [imageLoaded]);

  const handleRegionClick = useCallback((regionId: string) => {
    setSelectedRegions(prev => {
      const newSet = new Set(prev);
      if (newSet.has(regionId)) {
        newSet.delete(regionId);
      } else {
        newSet.add(regionId);
      }
      return newSet;
    });
  }, []);

  const createImageOcclusionCard = useCallback(async () => {
    if (!selectedImage || selectedRegions.size === 0 || !cardTitle.trim()) {
      alert('请上传图片、选择要遮罩的区域，并填写卡片标题');
      return;
    }

    setIsProcessing(true);
    try {
      const base64Data = selectedImage.split(',')[1];
      const response = await invoke<ImageOcclusionResponse>('create_image_occlusion_card', {
        request: {
          image_base64: base64Data,
          title: cardTitle.trim(),
          description: cardDescription.trim() || null,
          subject: cardSubject,
          tags: cardTags.split(',').map(tag => tag.trim()).filter(tag => tag.length > 0),
          selected_regions: Array.from(selectedRegions),
          mask_style: maskStyle,
          use_high_resolution: useHighResolution,
        },
      });

      if (response.success && response.card) {
        console.log('✅ 遮罩卡创建成功:', {
          卡片ID: response.card.id,
          标题: response.card.title,
          遮罩数量: response.card.masks.length,
          图片尺寸: `${response.card.image_width}x${response.card.image_height}`
        });
        setCreatedCard(response.card);
        setShowPreview(true);
        alert('✅ 图片遮罩卡创建成功！');
      } else {
        const errorMsg = response.error_message || '未知错误';
        console.error('❌ 遮罩卡创建失败:', errorMsg);
        alert(`创建失败: ${errorMsg}\n\n请检查:\n1. 是否选择了遮罩区域\n2. 卡片信息是否完整\n3. 网络连接是否正常`);
      }
    } catch (error) {
      console.error('❌ 创建遮罩卡异常:', error);
      const errorMessage = error instanceof Error ? error.message : String(error);
      alert(`创建失败: ${errorMessage}\n\n可能的原因:\n1. 网络连接问题\n2. 服务器处理错误\n3. 数据格式异常`);
    } finally {
      setIsProcessing(false);
    }
  }, [selectedImage, selectedRegions, cardTitle, cardDescription, cardSubject, cardTags, maskStyle, useHighResolution]);

  const renderTextRegions = useCallback(() => {
    if (!textRegions.length || !imageRef.current || !imageLoaded || imageScale <= 0) {
      console.log('🚫 无法渲染文字区域:', {
        文字区域数量: textRegions.length,
        图片引用: !!imageRef.current,
        图片加载: imageLoaded,
        缩放比例: imageScale
      });
      return null;
    }

    if (!imageRef.current.parentElement) {
      console.error('❌ 图片父元素不存在');
      return null;
    }

    // 获取精确的容器和图片位置信息
    const imgRect = imageRef.current.getBoundingClientRect();
    const containerRect = imageRef.current.parentElement.getBoundingClientRect();
    
    // 计算图片相对于容器的精确偏移（不重复计算padding）
    const imgOffsetX = imgRect.left - containerRect.left;
    const imgOffsetY = imgRect.top - containerRect.top;

    console.log('🎯 渲染文字区域 - 精确坐标计算:', {
      容器位置: `${containerRect.left}, ${containerRect.top}`,
      图片位置: `${imgRect.left}, ${imgRect.top}`,
      图片偏移: `${imgOffsetX}, ${imgOffsetY}`,
      图片尺寸: `${imgRect.width}x${imgRect.height}`,
      缩放比例: imageScale.toFixed(4),
      区域数量: textRegions.length
    });

    return textRegions.map((region) => {
      const [x1, y1, x2, y2] = region.bbox;
      const isSelected = selectedRegions.has(region.region_id);
      
      // 精确的坐标转换：原始坐标 * 缩放比例 + 图片偏移
      const finalLeft = x1 * imageScale + imgOffsetX;
      const finalTop = y1 * imageScale + imgOffsetY;
      const finalWidth = (x2 - x1) * imageScale;
      const finalHeight = (y2 - y1) * imageScale;
      
      // 调试信息（只为第一个区域输出，避免日志混乱）
      if (region === textRegions[0]) {
        console.log('📍 第一个区域坐标转换:', {
          原始坐标: `[${x1}, ${y1}, ${x2}, ${y2}]`,
          缩放后: `[${(x1 * imageScale).toFixed(1)}, ${(y1 * imageScale).toFixed(1)}, ${(x2 * imageScale).toFixed(1)}, ${(y2 * imageScale).toFixed(1)}]`,
          最终位置: `${finalLeft.toFixed(1)}, ${finalTop.toFixed(1)}`,
          区域尺寸: `${finalWidth.toFixed(1)}x${finalHeight.toFixed(1)}`,
          文字内容: region.text
        });
      }
      
      return (
        <div
          key={region.region_id}
          className={`text-region ${isSelected ? 'selected' : ''}`}
          style={{
            position: 'absolute',
            left: `${Math.round(finalLeft)}px`,
            top: `${Math.round(finalTop)}px`,
            width: `${Math.round(finalWidth)}px`,
            height: `${Math.round(finalHeight)}px`,
            border: `2px solid ${isSelected ? '#FF0000' : '#00FF00'}`,
            backgroundColor: isSelected ? 'rgba(255, 0, 0, 0.2)' : 'rgba(0, 255, 0, 0.1)',
            cursor: 'pointer',
            borderRadius: '2px',
            boxSizing: 'border-box',
            pointerEvents: 'auto'
          }}
          onClick={() => handleRegionClick(region.region_id)}
          title={`${region.text} (置信度: ${(region.confidence * 100).toFixed(1)}%)`}
        />
      );
    });
  }, [textRegions, selectedRegions, imageScale, imageLoaded, handleRegionClick]);

  const renderMaskPreview = useCallback(() => {
    if (!createdCard || !showPreview || !imageRef.current || imageScale <= 0) {
      console.log('🚫 无法渲染遮罩预览:', {
        有卡片: !!createdCard,
        显示预览: showPreview,
        图片引用: !!imageRef.current,
        缩放比例: imageScale
      });
      return null;
    }

    if (!imageRef.current.parentElement) {
      console.error('❌ 遮罩预览: 图片父元素不存在');
      return null;
    }

    // 使用与文字区域相同的坐标计算方法
    const imgRect = imageRef.current.getBoundingClientRect();
    const containerRect = imageRef.current.parentElement.getBoundingClientRect();
    
    const imgOffsetX = imgRect.left - containerRect.left;
    const imgOffsetY = imgRect.top - containerRect.top;

    console.log('🎭 渲染遮罩预览:', {
      遮罩数量: createdCard.masks.length,
      图片偏移: `${imgOffsetX}, ${imgOffsetY}`,
      缩放比例: imageScale.toFixed(4)
    });

    return createdCard.masks.map((mask) => {
      const [x1, y1, x2, y2] = mask.bbox;
      let maskStyleCSS: React.CSSProperties = {};

      if ('SolidColor' in mask.mask_style && mask.mask_style.SolidColor) {
        maskStyleCSS.backgroundColor = mask.mask_style.SolidColor.color;
      } else if ('BlurEffect' in mask.mask_style && mask.mask_style.BlurEffect) {
        maskStyleCSS.backgroundColor = 'rgba(128, 128, 128, 0.8)';
        maskStyleCSS.backdropFilter = `blur(${mask.mask_style.BlurEffect.intensity}px)`;
      } else if ('Rectangle' in mask.mask_style && mask.mask_style.Rectangle) {
        const rectStyle = mask.mask_style.Rectangle;
        maskStyleCSS.backgroundColor = rectStyle.color;
        maskStyleCSS.opacity = rectStyle.opacity;
      }

      // 使用与文字区域相同的坐标转换方法
      const finalLeft = x1 * imageScale + imgOffsetX;
      const finalTop = y1 * imageScale + imgOffsetY;
      const finalWidth = (x2 - x1) * imageScale;
      const finalHeight = (y2 - y1) * imageScale;

      return (
        <div
          key={mask.mask_id}
          className="mask-overlay"
          style={{
            position: 'absolute',
            left: `${Math.round(finalLeft)}px`,
            top: `${Math.round(finalTop)}px`,
            width: `${Math.round(finalWidth)}px`,
            height: `${Math.round(finalHeight)}px`,
            cursor: 'pointer',
            borderRadius: '2px',
            border: '1px solid rgba(0, 0, 0, 0.3)',
            boxSizing: 'border-box',
            pointerEvents: 'auto',
            ...maskStyleCSS,
          }}
          onClick={() => {
            const isHidden = (document.querySelector(`[data-mask-id="${mask.mask_id}"]`) as HTMLElement)?.style.display === 'none';
            const element = document.querySelector(`[data-mask-id="${mask.mask_id}"]`) as HTMLElement;
            if (element) {
              element.style.display = isHidden ? 'block' : 'none';
            }
          }}
          data-mask-id={mask.mask_id}
          title={`点击切换显示: ${mask.original_text}`}
        />
      );
    });
  }, [createdCard, showPreview, imageScale]);

  useEffect(() => {
    // 只有在有OCR数据和图片已加载时才监听窗口大小变化
    if (!originalImageDimensions || !imageLoaded) {
      return;
    }

    const debouncedUpdateScale = () => {
      console.log('🔄 窗口大小变化，重新计算缩放比例');
      updateImageScale(originalImageDimensions.width, originalImageDimensions.height, true);
    };

    let timeoutId: number | null = null;
    const handleResize = () => {
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
      timeoutId = setTimeout(debouncedUpdateScale, 300); // 增加防抖时间
    };

    window.addEventListener('resize', handleResize);

    return () => {
      window.removeEventListener('resize', handleResize);
      if (timeoutId) {
        clearTimeout(timeoutId);
      }
    };
  }, [originalImageDimensions, imageLoaded, updateImageScale]);


  return (
    <div style={{
      width: '100%',
      height: '100%',
      overflow: 'auto',
      background: '#f8fafc'
    }}>
      {/* 头部区域 - 统一白色样式 */}
      <div style={{
        background: 'white',
        borderBottom: '1px solid #e5e7eb',
        padding: '24px 32px',
        position: 'relative'
      }}>
        <div style={{
          position: 'absolute',
          top: 0,
          left: 0,
          right: 0,
          height: '4px',
          background: 'linear-gradient(90deg, #667eea, #764ba2)'
        }}></div>
        
        <div style={{ position: 'relative', zIndex: 1 }}>
          <div style={{ display: 'flex', alignItems: 'center', marginBottom: '8px' }}>
            <svg style={{ width: '32px', height: '32px', marginRight: '12px', color: '#667eea' }} fill="none" stroke="currentColor" viewBox="0 0 24 24" strokeWidth={2}>
              <rect width="18" height="18" x="3" y="3" rx="2" ry="2" />
              <circle cx="9" cy="9" r="2" />
              <path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21" />
            </svg>
            <h1 style={{ fontSize: '28px', fontWeight: '700', margin: 0, color: '#1f2937' }}>图片遮罩卡制作</h1>
          </div>
          <p style={{ fontSize: '16px', color: '#6b7280', margin: 0, lineHeight: '1.5' }}>
            上传图片，选择要遮罩的文字区域，创建互动学习卡片
          </p>
        </div>
      </div>

      <div className="image-occlusion-container" style={{padding: '24px'}}>
        <div className="upload-section">
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*"
            onChange={handleImageUpload}
            style={{ display: 'none' }}
          />
          <button
            onClick={() => fileInputRef.current?.click()}
            className="upload-button"
            disabled={isProcessing}
          >
            选择图片
          </button>

          {selectedImage && (
            <button
              onClick={extractTextCoordinates}
              className="extract-button"
              disabled={isProcessing}
            >
            {isProcessing ? '识别中...' : '识别文字区域'}
            </button>
          )}
          {selectedImage && (
            <div className="form-group-inline">
              <input
                type="checkbox"
                id="highResToggle"
                checked={useHighResolution}
                onChange={(e) => setUseHighResolution(e.target.checked)}
                disabled={isProcessing}
              />
              <label htmlFor="highResToggle">使用高分辨率模式</label>
            </div>
          )}
        </div>

        {selectedImage && (
          <div className="image-workspace">
            <div className="image-container">
              <img
                ref={imageRef}
                src={selectedImage}
                alt="待处理图片"
                className="main-image"
                onLoad={(e) => {
                  const img = e.target as HTMLImageElement;
                  console.log('🖼️ 图片加载完成:', {
                    自然尺寸: `${img.naturalWidth}x${img.naturalHeight}`,
                    显示尺寸: `${img.clientWidth}x${img.clientHeight}`,
                    是否有OCR数据: originalImageDimensions !== null
                  });
                  setImageLoaded(true);
                  
                  // 如果已有OCR数据，立即重新计算缩放比例
                  if (originalImageDimensions) {
                    console.log('🔄 图片加载完成，重新计算缩放比例');
                    // 使用setTimeout确保图片尺寸已经更新
                    setTimeout(() => {
                      updateImageScale(originalImageDimensions.width, originalImageDimensions.height, true);
                    }, 50);
                  }
                }}
              />
              {!showPreview && renderTextRegions()}
              {showPreview && renderMaskPreview()}
            </div>

            {textRegions.length > 0 && !showPreview && (
              <div className="controls-panel">
                <div className="card-info">
                  <h3>卡片信息</h3>
                  <div className="form-group">
                    <label htmlFor="cardTitle">标题 *</label>
                    <input
                      id="cardTitle"
                      type="text"
                      value={cardTitle}
                      onChange={(e) => setCardTitle(e.target.value)}
                      placeholder="输入卡片标题"
                    />
                  </div>

                  <div className="form-group">
                    <label htmlFor="cardDescription">描述</label>
                    <textarea
                      id="cardDescription"
                      value={cardDescription}
                      onChange={(e) => setCardDescription(e.target.value)}
                      placeholder="输入卡片描述（可选）"
                      rows={3}
                    />
                  </div>

                  <div className="form-row">
                    <div className="form-group">
                      <label htmlFor="cardSubject">学科</label>
                      <select
                        id="cardSubject"
                        value={cardSubject}
                        onChange={(e) => setCardSubject(e.target.value)}
                      >
                        <option value="数学">数学</option>
                        <option value="物理">物理</option>
                        <option value="化学">化学</option>
                        <option value="英语">英语</option>
                        <option value="语文">语文</option>
                        <option value="历史">历史</option>
                        <option value="地理">地理</option>
                        <option value="生物">生物</option>
                        <option value="其他">其他</option>
                      </select>
                    </div>

                    <div className="form-group">
                      <label htmlFor="cardTags">标签</label>
                      <input
                        id="cardTags"
                        type="text"
                        value={cardTags}
                        onChange={(e) => setCardTags(e.target.value)}
                        placeholder="用逗号分隔多个标签"
                      />
                    </div>
                  </div>
                </div>

                <div className="mask-settings">
                  <h3>遮罩样式</h3>
                  <div className="mask-style-options">
                    <label>
                      <input
                        type="radio"
                        name="maskStyle"
                        checked={'Rectangle' in maskStyle}
                        onChange={() => setMaskStyle({ Rectangle: { color: '#FF0000', opacity: 0.7 } })}
                      />
                      半透明矩形
                    </label>
                    <label>
                      <input
                        type="radio"
                        name="maskStyle"
                        checked={'SolidColor' in maskStyle}
                        onChange={() => setMaskStyle({ SolidColor: { color: '#000000' } })}
                      />
                      纯色遮罩
                    </label>
                    <label>
                      <input
                        type="radio"
                        name="maskStyle"
                        checked={'BlurEffect' in maskStyle}
                        onChange={() => setMaskStyle({ BlurEffect: { intensity: 5 } })}
                      />
                      模糊效果
                    </label>
                  </div>
                </div>

                <div className="region-stats">
                  <p>已识别 {textRegions.length} 个文字区域</p>
                  <p>已选择 {selectedRegions.size} 个区域进行遮罩</p>
                </div>

                <button
                  onClick={createImageOcclusionCard}
                  className="create-button"
                  disabled={isProcessing || selectedRegions.size === 0 || !cardTitle.trim()}
                >
                  {isProcessing ? '创建中...' : '创建遮罩卡'}
                </button>
              </div>
            )}

            {showPreview && createdCard && (
              <div className="preview-panel">
                <h3>预览模式</h3>
                <p>点击红色遮罩区域可以切换显示/隐藏文字</p>
                <div className="preview-controls">
                  <button onClick={() => setShowPreview(false)}>返回编辑</button>
                  <button onClick={() => {
                    console.log('📄 尝试导出ANKI卡片:', {
                      卡片ID: createdCard?.id,
                      标题: createdCard?.title,
                      遮罩数量: createdCard?.masks.length
                    });
                    alert('🔧 导出功能即将推出\n\n将支持:\n1. 导出为Anki apkg文件\n2. 自定义遮罩样式\n3. 批量导出功能');
                  }}>
                    导出到ANKI
                  </button>
                </div>
                <div className="card-details">
                  <h4>{createdCard.title}</h4>
                  {createdCard.description && <p>{createdCard.description}</p>}
                  <div className="tags">
                    {createdCard.tags.map((tag, index) => (
                      <span key={index} className="tag">{tag}</span>
                    ))}
                  </div>
                </div>
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
};

export default ImageOcclusion;
