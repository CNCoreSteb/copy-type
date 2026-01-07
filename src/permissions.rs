//! 权限检查模块

use log::{info, warn};
use crate::i18n::I18n;

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
    pub fn get_warning_message(&self, i18n: &I18n) -> Option<String> {
        if self.all_granted() {
            None
        } else {
            let mut messages = Vec::new();
            
            if !self.keyboard_simulation {
                messages.push(i18n.t("permissions.warn_keyboard"));
            }
            if !self.clipboard_access {
                messages.push(i18n.t("permissions.warn_clipboard"));
            }
            
            messages.extend(self.issues.iter().cloned());
            
            Some(messages.join("\n"))
        }
    }
}

/// 检查应用程序所需的权限
pub fn check_permissions(i18n: &I18n) -> PermissionStatus {
    #[cfg(target_os = "windows")]
    {
        check_windows_permissions(i18n)
    }
    
    #[cfg(target_os = "macos")]
    {
        check_macos_permissions(i18n)
    }
    
    #[cfg(target_os = "linux")]
    {
        check_linux_permissions(i18n)
    }
}

#[cfg(target_os = "windows")]
fn check_windows_permissions(i18n: &I18n) -> PermissionStatus {
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
        info!("{}", i18n.t("permissions.windows.not_admin"));
        // 这不一定是问题，只是记录一下
    }
    
    // 检查是否有可能被安全软件阻止
    // 这里我们通过尝试创建一个 Enigo 实例来检测
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            info!("{}", i18n.t("permissions.windows.enigo_ok"));
        }
        Err(e) => {
            let err = e.to_string();
            warn!("{}", i18n.tr("permissions.windows.enigo_fail", &[("err", err.as_str())]));
            keyboard_ok = false;
            issues.push(i18n.tr(
                "permissions.windows.enigo_fail_issue",
                &[("err", err.as_str())],
            ));
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
fn check_macos_permissions(i18n: &I18n) -> PermissionStatus {
    let mut issues = Vec::new();
    let keyboard_ok;
    let clipboard_ok = true;
    
    // macOS 需要辅助功能权限才能模拟键盘输入
    // 我们通过尝试创建 Enigo 实例来检测
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            keyboard_ok = true;
            info!("{}", i18n.t("permissions.macos.accessibility_granted"));
        }
        Err(e) => {
            keyboard_ok = false;
            let err = e.to_string();
            warn!(
                "{}",
                i18n.tr("permissions.macos.accessibility_denied", &[("err", err.as_str())])
            );
            issues.push(i18n.t("permissions.macos.accessibility_fix"));
        }
    }
    
    PermissionStatus {
        keyboard_simulation: keyboard_ok,
        clipboard_access: clipboard_ok,
        issues,
    }
}

#[cfg(target_os = "linux")]
fn check_linux_permissions(i18n: &I18n) -> PermissionStatus {
    let mut issues = Vec::new();
    let keyboard_ok;
    let clipboard_ok = true;
    
    // Linux 上检查是否可以访问输入设备
    match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(_) => {
            keyboard_ok = true;
            info!("{}", i18n.t("permissions.linux.keyboard_ok"));
        }
        Err(e) => {
            keyboard_ok = false;
            let err = e.to_string();
            warn!("{}", i18n.tr("permissions.linux.keyboard_denied", &[("err", err.as_str())]));
            issues.push(i18n.t("permissions.linux.add_to_input_group"));
        }
    }
    
    PermissionStatus {
        keyboard_simulation: keyboard_ok,
        clipboard_access: clipboard_ok,
        issues,
    }
}

/// 获取权限修复建议
pub fn get_permission_fix_instructions(i18n: &I18n) -> String {
    #[cfg(target_os = "windows")]
    {
        return i18n.t("permissions.fix.windows");
    }
    
    #[cfg(target_os = "macos")]
    {
        return i18n.t("permissions.fix.macos");
    }
    
    #[cfg(target_os = "linux")]
    {
        return i18n.t("permissions.fix.linux");
    }
    
    #[allow(unreachable_code)]
    String::new()
}
