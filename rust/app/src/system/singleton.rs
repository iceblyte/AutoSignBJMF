//! 单实例：命名互斥体。
//!
//! 名字与 Python 版一致（`Local\AutoSignBJMF_SingleInstance`），
//! 这样双跑期两个版本也能互相发现，不会同时签到两次。
//!
//! 拿到锁的对象必须活到进程结束；`SingleInstance` 被 drop 时自动释放。

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
use windows_sys::Win32::System::Threading::CreateMutexW;

pub const MUTEX_NAME: &str = r"Local\AutoSignBJMF_SingleInstance";

/// 单实例守卫。
pub struct SingleInstance {
    handle: *mut core::ffi::c_void,
}

impl SingleInstance {
    /// 尝试拿锁。`None` 表示已有实例在跑。
    pub fn acquire() -> Option<Self> {
        Self::acquire_named(MUTEX_NAME)
    }

    /// 用指定名字拿锁。测试要用独立名字，否则会和正在运行的真实实例抢同一个锁。
    pub fn acquire_named(name: &str) -> Option<Self> {
        let name: Vec<u16> = OsStr::new(name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            let handle = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
            if handle.is_null() {
                return None;
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return None;
            }
            Some(Self { handle })
        }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试用独立名字：生产锁会被正在运行的实例占着，
    /// 直接用 MUTEX_NAME 会让测试随机失败（取决于本机是否开着程序）。
    fn test_name(tag: &str) -> String {
        format!("Local\\bjmf-test-{}-{tag}", std::process::id())
    }

    #[test]
    fn 同一进程内第二次拿不到锁() {
        let name = test_name("second");
        let first = SingleInstance::acquire_named(&name);
        assert!(first.is_some(), "第一次应该拿到锁");
        let second = SingleInstance::acquire_named(&name);
        assert!(second.is_none(), "已有实例时不应该拿到锁");
        drop(first);
    }

    #[test]
    fn 释放后可以再次拿到() {
        let name = test_name("reacquire");
        {
            let first = SingleInstance::acquire_named(&name);
            assert!(first.is_some());
        }
        let again = SingleInstance::acquire_named(&name);
        assert!(again.is_some(), "释放后应该能重新拿到");
        drop(again);
    }

    #[test]
    fn 名字与_python_版一致() {
        assert_eq!(MUTEX_NAME, r"Local\AutoSignBJMF_SingleInstance");
    }
}
