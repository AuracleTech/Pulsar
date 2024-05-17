use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use std::error::Error;
use std::num::NonZeroIsize;
use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::winuser::*;

pub struct Window {
    pub hwnd: HWND,
    pub hdc: HDC,
    pub raw_window_handle: RawWindowHandle,
    pub raw_display_handle: RawDisplayHandle,
    pub width: u32,
    pub height: u32,
}

impl Window {
    pub fn create_main(
        name: &str,
        title: &str,
        width: u32,
        height: u32,
    ) -> Result<Window, Box<dyn Error>> {
        let name = to_wstring(name);
        let title = to_wstring(title);

        unsafe {
            let hinstance = GetModuleHandleW(std::ptr::null_mut());
            let wnd_class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_OWNDC | CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(window_proc),
                hInstance: hinstance,
                hIcon: LoadIconW(std::ptr::null_mut(), IDI_APPLICATION),
                hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
                hbrBackground: COLOR_WINDOWFRAME as HBRUSH,
                lpszClassName: name.as_ptr(),
                ..std::mem::zeroed()
            };

            if RegisterClassExW(&wnd_class) == 0 {
                return Err("Window Registration Failed".into());
            }

            let hwnd = match CreateWindowExW(
                0,
                name.as_ptr(),
                title.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,
                height as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinstance,
                std::ptr::null_mut(),
            ) {
                hwnd if hwnd.is_null() => return Err("Window Creation Failed!".into()),
                hwnd => hwnd,
            };

            let hdc = GetDC(hwnd);
            if hdc.is_null() {
                panic!("Failed to get device context");
            }

            let hinstance = GetModuleHandleW(std::ptr::null_mut());

            let hwnd_non_zero = NonZeroIsize::new(hwnd as isize).unwrap();
            let mut win32handle = Win32WindowHandle::new(hwnd_non_zero);
            win32handle.hinstance = NonZeroIsize::new(hinstance as isize);

            let raw_window_handle = RawWindowHandle::Win32(win32handle);

            let handle = WindowsDisplayHandle::new();
            let raw_display_handle = RawDisplayHandle::Windows(handle);

            Ok(Window {
                hwnd,
                hdc,
                raw_window_handle,
                raw_display_handle,
                width,
                height,
            })
        }
    }

    pub fn show(&self) {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
            UpdateWindow(self.hwnd);
        }
    }

    pub fn run(&self) {
        unsafe {
            let mut msg: MSG = std::mem::MaybeUninit::zeroed().assume_init();
            while GetMessageW(&mut msg, self.hwnd, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }
}

fn to_wstring(value: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(self.hwnd, self.hdc);
            DestroyWindow(self.hwnd);
        }
    }
}
