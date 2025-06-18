import { useState, useEffect } from 'react';

interface ImageViewerProps {
  images: string[];
  currentIndex: number;
  isOpen: boolean;
  onClose: () => void;
  onNext?: () => void;
  onPrev?: () => void;
}

export const ImageViewer: React.FC<ImageViewerProps> = ({
  images,
  currentIndex,
  isOpen,
  onClose,
  onNext,
  onPrev
}) => {
  const [scale, setScale] = useState(1);
  const [rotation, setRotation] = useState(0);
  const [position, setPosition] = useState({ x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [dragStart, setDragStart] = useState({ x: 0, y: 0 });

  // 重置状态当图片改变时
  useEffect(() => {
    setScale(1);
    setRotation(0);
    setPosition({ x: 0, y: 0 });
  }, [currentIndex]);

  // 键盘事件处理
  useEffect(() => {
    if (!isOpen) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'Escape':
          onClose();
          break;
        case 'ArrowLeft':
          onPrev?.();
          break;
        case 'ArrowRight':
          onNext?.();
          break;
        case '+':
        case '=':
          setScale(prev => Math.min(prev * 1.2, 5));
          break;
        case '-':
          setScale(prev => Math.max(prev / 1.2, 0.1));
          break;
        case 'r':
        case 'R':
          setRotation(prev => (prev + 90) % 360);
          break;
        case '0':
          setScale(1);
          setRotation(0);
          setPosition({ x: 0, y: 0 });
          break;
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose, onNext, onPrev]);

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDragging(true);
    setDragStart({
      x: e.clientX - position.x,
      y: e.clientY - position.y
    });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDragging) return;
    setPosition({
      x: e.clientX - dragStart.x,
      y: e.clientY - dragStart.y
    });
  };

  const handleMouseUp = () => {
    setIsDragging(false);
  };

  const handleWheel = (e: React.WheelEvent) => {
    e.preventDefault();
    const delta = e.deltaY > 0 ? 0.9 : 1.1;
    setScale(prev => Math.max(0.1, Math.min(5, prev * delta)));
  };

  if (!isOpen || images.length === 0) return null;

  const currentImage = images[currentIndex];

  return (
    <div className="image-viewer-overlay" onClick={onClose}>
      <div className="image-viewer-container" onClick={(e) => e.stopPropagation()}>
        {/* 工具栏 */}
        <div className="image-viewer-toolbar">
          <div className="toolbar-left">
            <span className="image-counter">
              {currentIndex + 1} / {images.length}
            </span>
          </div>
          
          <div className="toolbar-center">
            <button
              onClick={() => setScale(prev => Math.max(prev / 1.2, 0.1))}
              className="toolbar-button"
              title="缩小 (-)"
            >
              🔍-
            </button>
            <span className="scale-indicator">{Math.round(scale * 100)}%</span>
            <button
              onClick={() => setScale(prev => Math.min(prev * 1.2, 5))}
              className="toolbar-button"
              title="放大 (+)"
            >
              🔍+
            </button>
            <button
              onClick={() => setRotation(prev => (prev + 90) % 360)}
              className="toolbar-button"
              title="旋转 (R)"
            >
              🔄
            </button>
            <button
              onClick={() => {
                setScale(1);
                setRotation(0);
                setPosition({ x: 0, y: 0 });
              }}
              className="toolbar-button"
              title="重置 (0)"
            >
              🏠
            </button>
          </div>
          
          <div className="toolbar-right">
            <button onClick={onClose} className="close-button" title="关闭 (Esc)">
              ✕
            </button>
          </div>
        </div>

        {/* 图片容器 */}
        <div 
          className="image-viewer-content"
          onMouseDown={handleMouseDown}
          onMouseMove={handleMouseMove}
          onMouseUp={handleMouseUp}
          onMouseLeave={handleMouseUp}
          onWheel={handleWheel}
        >
          <img
            src={currentImage}
            alt={`图片 ${currentIndex + 1}`}
            className="viewer-image"
            style={{
              transform: `translate(${position.x}px, ${position.y}px) scale(${scale}) rotate(${rotation}deg)`,
              cursor: isDragging ? 'grabbing' : 'grab'
            }}
            draggable={false}
          />
        </div>

        {/* 导航按钮 */}
        {images.length > 1 && (
          <>
            <button
              onClick={onPrev}
              className="nav-button nav-prev"
              disabled={currentIndex === 0}
              title="上一张 (←)"
            >
              ‹
            </button>
            <button
              onClick={onNext}
              className="nav-button nav-next"
              disabled={currentIndex === images.length - 1}
              title="下一张 (→)"
            >
              ›
            </button>
          </>
        )}

        {/* 缩略图栏 */}
        {images.length > 1 && (
          <div className="image-viewer-thumbnails">
            {images.map((image, index) => (
              <div
                key={index}
                className={`thumbnail ${index === currentIndex ? 'active' : ''}`}
                onClick={() => {
                  // 这里需要父组件提供切换图片的方法
                }}
              >
                <img src={image} alt={`缩略图 ${index + 1}`} />
              </div>
            ))}
          </div>
        )}

        {/* 快捷键提示 */}
        <div className="keyboard-hints">
          <div className="hint">ESC: 关闭</div>
          <div className="hint">←→: 切换</div>
          <div className="hint">+/-: 缩放</div>
          <div className="hint">R: 旋转</div>
          <div className="hint">0: 重置</div>
        </div>
      </div>
    </div>
  );
}; 