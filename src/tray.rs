use crate::{RunReason, APP_NAME};
use std::mem::size_of_val;
use std::path::PathBuf;
use std::ptr::null;
use std::sync::mpsc;
use win32_utils::error::{check_error, CheckError};
use win32_utils::macros::LOWORD;
use win32_utils::str::ToWin32Str;
use win32_utils::window::WindowDataExtension;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NIN_SELECT, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResource, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DispatchMessageW,
    GetCursorPos, GetMessageW, InsertMenuW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu, TranslateMessage, CW_USEDEFAULT,
    GWLP_USERDATA, HMENU, MF_BYPOSITION, MF_STRING, MSG, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
    TPM_LEFTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_CONTEXTMENU, WNDCLASSW,
    WS_OVERLAPPEDWINDOW,
};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

const IDM_EXIT: usize = 100;
const IDM_SHOW_LOG: usize = 101;
const IDM_UPDATE_DNS: usize = 102;

const TRAY_ICON_CALLBACK: u32 = WM_APP + 1;
const NOTIFY_DNS_UPDATED: u32 = WM_APP + 2;

struct TrayProperties {
    log_file_path: PathBuf,
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
                log_file_path,
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
            window_data.icon.uCallbackMessage = TRAY_ICON_CALLBACK;
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
            TRAY_ICON_CALLBACK => match LOWORD(l_param.0 as u32) {
                NIN_SELECT | WM_CONTEXTMENU => {
                    properties.show_tray_menu();
                }
                _ => {}
            },
            NOTIFY_DNS_UPDATED => {
                properties.show_notification("Updated WSL2 DNS configuration");
            }
            WM_COMMAND => {
                properties.handle_command(w_param);
            }
            _ => {}
        }
    }
    DefWindowProcW(hwnd, msg, w_param, l_param)
}

impl TrayProperties {
    unsafe fn show_notification(&mut self, message: &str) {
        // NIF_INFO = Display a balloon notification
        self.icon.uFlags = NIF_INFO;
        self.icon.dwInfoFlags = 0;
        APP_NAME
            .copy_to_wchar_buffer(&mut self.icon.szInfoTitle)
            .unwrap();
        message.copy_to_wchar_buffer(&mut self.icon.szInfo).unwrap();
        Shell_NotifyIconW(NIM_MODIFY, &self.icon).ok().unwrap();
    }

    unsafe fn show_tray_menu(&self) {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt);
        let hmenu = CreatePopupMenu().unwrap();
        let exit_msg = "Exit".to_wchar();
        InsertMenuW(
            hmenu,
            0,
            MF_BYPOSITION | MF_STRING,
            IDM_EXIT,
            PCWSTR(exit_msg.as_ptr()),
        );
        let view_log_msg = "View Log".to_wchar();
        InsertMenuW(
            hmenu,
            0,
            MF_BYPOSITION | MF_STRING,
            IDM_SHOW_LOG,
            PCWSTR(view_log_msg.as_ptr()),
        );
        let reapply_dns_msg = "Reapply DNS".to_wchar();
        InsertMenuW(
            hmenu,
            0,
            MF_BYPOSITION | MF_STRING,
            IDM_UPDATE_DNS,
            PCWSTR(reapply_dns_msg.as_ptr()),
        );
        SetForegroundWindow(self.window);
        TrackPopupMenu(
            hmenu,
            TPM_LEFTALIGN | TPM_LEFTBUTTON | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            self.window,
            null(),
        );
    }

    unsafe fn handle_command(&self, w_param: WPARAM) {
        match w_param.0 {
            IDM_EXIT => PostQuitMessage(0),
            IDM_UPDATE_DNS => {
                self.sender.send(RunReason::TrayButton).ok();
            }
            IDM_SHOW_LOG => {
                open::that(&self.log_file_path).ok();
            }
            _ => {}
        }
    }
}
