use crate::{RunReason, APP_NAME};
use std::mem::size_of_val;
use std::path::PathBuf;
use std::sync::mpsc;
use win32_utils::error::{check_error, CheckError};
use win32_utils::macros::LOWORD;
use win32_utils::str::ToWin32Str;
use win32_utils::window::WindowDataExtension;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResource, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    RegisterClassW, SendMessageW, SetWindowLongPtrW, TranslateMessage, CW_USEDEFAULT,
    GWLP_USERDATA, HMENU, MSG, WINDOW_EX_STYLE, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

const CALLBACK_MSG: u32 = WM_APP + 1;
const NOTIFY_DNS_UPDATED: u32 = WM_APP + 2;

struct TrayProperties {
    _log_file_path: PathBuf,
    sender: mpsc::Sender<RunReason>,
    window: HWND,
    icon: NOTIFYICONDATAW,
}

pub struct Tray(Box<TrayProperties>);

impl Tray {
    pub fn new(log_file_path: PathBuf, sender: mpsc::Sender<RunReason>) -> Self {
        unsafe {
            // Create Window Class
            let hinstance = GetModuleHandleW(PCWSTR::default()).unwrap();
            let mut name = "TrayHolder".to_wchar();
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(tray_window_proc),
                hInstance: hinstance,
                lpszClassName: PCWSTR(name.as_mut_ptr()),
                ..Default::default()
            };
            let atom = RegisterClassW(&window_class).check_error().unwrap();

            // Create Window
            let tray_name = "tray".to_wchar();
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(atom as *mut u16),
                PCWSTR(tray_name.as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                HWND::default(),
                HMENU::default(),
                hinstance,
                std::ptr::null_mut(),
            )
            .check_error()
            .unwrap();

            // Register Window data
            let mut window_data = Box::new(TrayProperties {
                _log_file_path: log_file_path,
                sender,
                window: hwnd,
                icon: NOTIFYICONDATAW::default(),
            });
            check_error(|| {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, window_data.as_mut() as *mut _ as isize)
            })
            .unwrap();

            // Create hicon
            let hicon = CreateIconFromResource(
                ICON_BYTES.as_ptr(),
                ICON_BYTES.len() as u32,
                true,
                0x00030000,
            )
            .unwrap();

            // Create tray icon
            window_data.icon.cbSize = size_of_val(&window_data.icon) as u32;
            window_data.icon.hWnd = hwnd;
            window_data.icon.hIcon = hicon;
            window_data.icon.uCallbackMessage = CALLBACK_MSG;
            APP_NAME
                .copy_to_wchar_buffer(&mut window_data.icon.szTip)
                .unwrap();
            window_data.icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
            window_data.icon.Anonymous.uVersion = NOTIFYICON_VERSION_4;

            Shell_NotifyIconW(NIM_ADD, &window_data.icon).ok().unwrap();
            Shell_NotifyIconW(NIM_SETVERSION, &window_data.icon)
                .ok()
                .unwrap();

            Tray(window_data)
        }
    }

    pub fn get_handle(&self) -> TrayHandle {
        TrayHandle(self.0.window)
    }

    pub fn run(self) {
        unsafe {
            let mut msg = MSG::default();
            loop {
                let ret = GetMessageW(&mut msg, HWND::default(), 0, 0).0;
                match ret {
                    -1 => {
                        panic!("GetMessage failed");
                    }
                    0 => break,
                    _ => {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }

            // Cleanup
            Shell_NotifyIconW(NIM_DELETE, &self.0.icon).ok().unwrap();
        }
    }
}

#[derive(Clone)]
pub struct TrayHandle(HWND);

impl TrayHandle {
    pub fn notify_dns_updated(&self) {
        unsafe {
            SendMessageW(self.0, NOTIFY_DNS_UPDATED, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if let Some(properties) = hwnd.get_user_data::<TrayProperties>() {
        match msg {
            CALLBACK_MSG => match LOWORD(l_param.0 as u32) {
                WM_LBUTTONUP | WM_RBUTTONUP => {
                    properties.sender.send(RunReason::TrayButton).ok();
                }
                _ => {}
            },
            NOTIFY_DNS_UPDATED => {
                properties.show_notification("Updated WSL2 DNS configuration");
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, w_param, l_param)
}

impl TrayProperties {
    fn show_notification(&mut self, message: &str) {
        unsafe {
            // NIF_INFO = Display a balloon notification
            self.icon.uFlags = NIF_INFO;
            self.icon.dwInfoFlags = 0;
            APP_NAME
                .copy_to_wchar_buffer(&mut self.icon.szInfoTitle)
                .unwrap();
            message.copy_to_wchar_buffer(&mut self.icon.szInfo).unwrap();
            Shell_NotifyIconW(NIM_MODIFY, &self.icon).ok().unwrap();
        }
    }
}
