//! 权限检查模块

use log::{info, warn};

/// 权限检查结果
#[derive(Debug, Clone)]
pub struct PermissionStatus {
    /// 是否有键盘模拟权限
    pub keyboard_simulation: bool,
    /// 是否有剪贴板访问权限
    pub clipboard_access: bool,
    /// 权限问题描述
    pub issues: Vec<String>,
}

impl PermissionStatus {
    /// 检查是否所有权限都满足
    pub fn all_granted(&self) -> bool {
        self.keyboard_simulation && self.clipboard_access
    }

    /// 获取权限问题的描述信息
    pub fn get_warning_message(&self) -> Option<String> {
        if self.all_granted() {
            None
        } else {
            let mut messages = Vec::new();
            
            if !self.keyboard_simulation {
                messages.push("• 键盘模拟权限不足：程序可能无法正常输入文字");
            }
            if !self.clipboard_access {
                messages.push("• 剪贴板访问权限不足：程序可能无法读取复制的内容");
            }
            
            messages.extend(self.issues.iter().map(|s| s.as_str()));
            
            Some(messages.join("\n"))
        }
    }
}

/// 检查应用程序所需的权限
pub fn check_permissions() -> PermissionStatus {
    #[cfg(target_os = "windows")]
    {
        check_windows_permissions()
    }
    
    #[cfg(target_os = "macos")]
    {
        check_macos_permissions()
    }
    
    #[cfg(target_os = "linux")]
    {
        check_linux_permissions()
    }
}

#[cfg(target_os = "windows")]
fn check_windows_permissions() -> PermissionStatus {
    use windows::Win32::UI::Accessibility::UiaClientsAreListening;
    
    let mut issues = Vec::new();
    let mut keyboard_ok = true;
    let clipboard_ok = true; // Windows 通常不限制剪贴板访问
    
    // 检查 UI Automation 是否可用（这是键盘模拟的基础）
    // 实际上 Windows 上通常不需要特殊权限，但某些安全软件可能会阻止
    unsafe {
        // UiaClientsAreListening 可以用来检测 UI Automation 是否正常工作
        let _ = UiaClientsAreListening();
    }
    
    // 检查是否在管理员权限下运行（某些情况下可能需要）
    if !is_elevated() {
        info!("程序未以管理员权限运行");
        // 这不一定是问题，只是记录一下
    }
    
    // 检查是否有可能被安全软件阻止
    // 这里我们通过尝试创建一个 Enigo 实例来检测
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            info!("键盘模拟初始化成功");
        }
        Err(e) => {
            warn!("键盘模拟初始化失败: {}", e);
            keyboard_ok = false;
            issues.push(format!("键盘模拟器初始化失败: {}", e));
        }
    }
    
    PermissionStatus {
        keyboard_simulation: keyboard_ok,
        clipboard_access: clipboard_ok,
        issues,
    }
}

#[cfg(target_os = "windows")]
fn is_elevated() -> bool {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    
    unsafe {
        let mut token_handle = HANDLE::default();
        let process = GetCurrentProcess();
        
        if OpenProcessToken(process, TOKEN_QUERY, &mut token_handle).is_err() {
            return false;
        }
        
        let mut elevation = TOKEN_ELEVATION::default();
        let mut return_length = 0u32;
        
        let result = GetTokenInformation(
            token_handle,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut return_length,
        );
        
        let _ = CloseHandle(token_handle);
        
        result.is_ok() && elevation.TokenIsElevated != 0
    }
}

#[cfg(target_os = "macos")]
fn check_macos_permissions() -> PermissionStatus {
    let mut issues = Vec::new();
    let keyboard_ok;
    let clipboard_ok = true;
    
    // macOS 需要辅助功能权限才能模拟键盘输入
    // 我们通过尝试创建 Enigo 实例来检测
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            keyboard_ok = true;
            info!("辅助功能权限已授予");
        }
        Err(e) => {
            keyboard_ok = false;
            warn!("辅助功能权限未授予: {}", e);
            issues.push("请在「系统偏好设置」→「安全性与隐私」→「隐私」→「辅助功能」中授权本应用".to_string());
        }
    }
    
    PermissionStatus {
        keyboard_simulation: keyboard_ok,
        clipboard_access: clipboard_ok,
        issues,
    }
}

#[cfg(target_os = "linux")]
fn check_linux_permissions() -> PermissionStatus {
    let mut issues = Vec::new();
    let keyboard_ok;
    let clipboard_ok = true;
    
    // Linux 上检查是否可以访问输入设备
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            keyboard_ok = true;
            info!("键盘模拟权限正常");
        }
        Err(e) => {
            keyboard_ok = false;
            warn!("键盘模拟权限不足: {}", e);
            issues.push("可能需要将用户添加到 input 组：sudo usermod -a -G input $USER".to_string());
        }
    }
    
    PermissionStatus {
        keyboard_simulation: keyboard_ok,
        clipboard_access: clipboard_ok,
        issues,
    }
}

/// 获取权限修复建议
pub fn get_permission_fix_instructions() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        r#"修复建议：

1. 确保没有安全软件阻止本程序
2. 尝试以管理员身份运行程序
3. 检查 Windows 安全中心是否有相关警告
4. 如果问题持续，请尝试重新安装程序"#
    }
    
    #[cfg(target_os = "macos")]
    {
        r#"修复建议：

1. 打开「系统偏好设置」
2. 进入「安全性与隐私」→「隐私」
3. 在左侧选择「辅助功能」
4. 点击左下角的锁图标解锁
5. 勾选 Copy-Type 应用程序
6. 重新启动本程序"#
    }
    
    #[cfg(target_os = "linux")]
    {
        r#"修复建议：

1. 将当前用户添加到 input 组：
   sudo usermod -a -G input $USER

2. 注销并重新登录

3. 如果使用 Wayland，可能需要额外配置：
   - 某些 Wayland 环境可能不支持全局键盘模拟
   - 考虑切换到 X11 会话"#
    }
}
