// 备份和恢复系统功能
// 创建于 2025-03-07

// 备份相关变量
let currentBackupId = null;

// 加载备份列表
async function loadBackups() {
    try {
        const backupsList = document.getElementById('backupsList');
        
        if (!backupsList) {
            console.error("找不到备份列表元素");
            return;
        }
        
        // 显示加载中提示
        backupsList.innerHTML = '<tr><td colspan="4" class="text-center">加载备份列表中...</td></tr>';
        
        const response = await fetch('/system/backups');
        const data = await response.json();
        
        if (data.success && data.backups && data.backups.length > 0) {
            // 清空列表
            backupsList.innerHTML = '';
            
            // 添加备份项
            data.backups.forEach(backup => {
                const row = document.createElement('tr');
                
                // 格式化大小
                const sizeInKB = backup.size / 1024;
                const sizeInMB = sizeInKB / 1024;
                let sizeStr = '';
                
                if (sizeInMB >= 1) {
                    sizeStr = sizeInMB.toFixed(2) + ' MB';
                } else {
                    sizeStr = sizeInKB.toFixed(2) + ' KB';
                }
                
                row.innerHTML = `
                    <td>${backup.id}</td>
                    <td>${backup.time}</td>
                    <td>${sizeStr}</td>
                    <td>
                        <button class="btn btn-sm btn-primary restore-backup-btn" data-backup-id="${backup.id}">
                            <i class="bi bi-cloud-upload"></i> 恢复
                        </button>
                    </td>
                `;
                
                backupsList.appendChild(row);
            });
            
            // 添加恢复按钮的事件监听器
            document.querySelectorAll('.restore-backup-btn').forEach(btn => {
                btn.addEventListener('click', function() {
                    const backupId = this.getAttribute('data-backup-id');
                    showRestoreBackupDialog(backupId);
                });
            });
        } else {
            backupsList.innerHTML = '<tr><td colspan="4" class="text-center">没有找到备份</td></tr>';
        }
    } catch (error) {
        console.error("加载备份列表失败:", error);
        const backupsList = document.getElementById('backupsList');
        if (backupsList) {
            backupsList.innerHTML = '<tr><td colspan="4" class="text-center text-danger">加载备份列表失败</td></tr>';
        }
        showToast('error', '加载备份列表失败，请刷新重试');
    }
}

// 创建备份
async function createBackup() {
    try {
        // 显示加载指示器
        document.getElementById('backupLoading').style.display = 'inline-block';
        document.getElementById('createBackupBtn').disabled = true;
        
        const response = await fetch('/system/backup', {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            }
        });
        
        const data = await response.json();
        
        if (data.success) {
            showToast('success', data.message);
            // 刷新备份列表
            loadBackups();
        } else {
            showToast('error', `创建备份失败: ${data.error}`);
        }
    } catch (error) {
        console.error("创建备份失败:", error);
        showToast('error', '创建备份失败，请稍后重试');
    } finally {
        // 隐藏加载指示器
        document.getElementById('backupLoading').style.display = 'none';
        document.getElementById('createBackupBtn').disabled = false;
    }
}

// 恢复备份
async function restoreBackup(backupId) {
    try {
        // 显示系统范围的加载状态
        document.body.classList.add('system-loading');
        
        const response = await fetch(`/system/restore/${backupId}`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json'
            }
        });
        
        const data = await response.json();
        
        if (data.success) {
            showToast('success', '恢复成功，系统将重新加载数据');
            
            // 延迟后刷新页面
            setTimeout(() => {
                window.location.reload();
            }, 2000);
        } else {
            showToast('error', `恢复失败: ${data.error}`);
            document.body.classList.remove('system-loading');
        }
    } catch (error) {
        console.error("恢复备份失败:", error);
        showToast('error', '恢复备份失败，请稍后重试');
        document.body.classList.remove('system-loading');
    }
}

// 显示恢复备份确认对话框
function showRestoreBackupDialog(backupId) {
    currentBackupId = backupId;
    const dialog = document.getElementById('restoreBackupDialog');
    if (dialog) {
        dialog.classList.add('active');
        // 设置焦点到取消按钮（更安全的默认选项）
        document.getElementById('cancelRestoreBtn').focus();
    }
}

// 隐藏恢复备份确认对话框
function hideRestoreBackupDialog() {
    const dialog = document.getElementById('restoreBackupDialog');
    if (dialog) {
        dialog.classList.remove('active');
        currentBackupId = null;
    }
}

// 初始化备份和恢复功能
function initBackupAndRestore() {
    console.log("初始化备份和恢复功能");
    
    // 创建备份按钮
    const createBackupBtn = document.getElementById('createBackupBtn');
    if (createBackupBtn) {
        createBackupBtn.addEventListener('click', createBackup);
    }
    
    // 刷新备份列表按钮
    const refreshBackupsBtn = document.getElementById('refreshBackupsBtn');
    if (refreshBackupsBtn) {
        refreshBackupsBtn.addEventListener('click', loadBackups);
    }
    
    // 取消恢复备份按钮
    const cancelRestoreBtn = document.getElementById('cancelRestoreBtn');
    if (cancelRestoreBtn) {
        cancelRestoreBtn.addEventListener('click', hideRestoreBackupDialog);
    }
    
    // 确认恢复备份按钮
    const confirmRestoreBtn = document.getElementById('confirmRestoreBtn');
    if (confirmRestoreBtn) {
        confirmRestoreBtn.addEventListener('click', function() {
            if (currentBackupId) {
                restoreBackup(currentBackupId);
                hideRestoreBackupDialog();
            }
        });
    }
    
    // 添加系统加载样式
    const styleId = 'backup-restore-styles';
    if (!document.getElementById(styleId)) {
        const style = document.createElement('style');
        style.id = styleId;
        style.textContent = `
            .system-loading {
                position: relative;
                pointer-events: none;
            }
            .system-loading::after {
                content: '';
                position: fixed;
                top: 0;
                left: 0;
                width: 100%;
                height: 100%;
                background-color: rgba(255, 255, 255, 0.7);
                display: flex;
                justify-content: center;
                align-items: center;
                z-index: 9999;
            }
            .system-loading::before {
                content: '系统正在恢复...';
                position: fixed;
                top: 50%;
                left: 50%;
                transform: translate(-50%, -50%);
                padding: 20px;
                background: white;
                border-radius: 8px;
                box-shadow: 0 4px 8px rgba(0,0,0,0.1);
                z-index: 10000;
            }
        `;
        document.head.appendChild(style);
    }
    
    // 注册标签页事件
    const systemTab = document.getElementById('system-tab');
    if (systemTab) {
        systemTab.addEventListener('shown.bs.tab', function(event) {
            loadBackups();
        });
    }
}

// 在文档加载完成时初始化
document.addEventListener('DOMContentLoaded', function() {
    initBackupAndRestore();
});
