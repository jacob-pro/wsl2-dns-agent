use crate::{RunReason, APP_NAME};
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
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResource, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    RegisterClassW, SetWindowLongPtrW, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, HMENU, MSG,
    WINDOW_EX_STYLE, WM_APP, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPEDWINDOW,
};

const ICON_BYTES: &[u8] = include_bytes!("../assets/icon.png");

const CALLBACK_MSG: u32 = WM_APP + 1;

pub struct TrayProperties {
    pub log_file_path: PathBuf,
    pub sender: mpsc::Sender<RunReason>,
}

pub fn run_tray(properties: TrayProperties) {
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
        let mut window_data = Box::new(properties);
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
        let mut tray_icon = NOTIFYICONDATAW::default();
        let mut name = APP_NAME.to_wchar();
        name.resize(tray_icon.szTip.len(), 0);
        let bytes = &name[..tray_icon.szTip.len()];
        tray_icon.hWnd = hwnd;
        tray_icon.hIcon = hicon;
        tray_icon.uCallbackMessage = CALLBACK_MSG;
        tray_icon.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        tray_icon.szTip.copy_from_slice(bytes);
        Shell_NotifyIconW(NIM_ADD, &tray_icon).ok().unwrap();

        // Start run loop
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
        Shell_NotifyIconW(NIM_DELETE, &tray_icon).ok().unwrap();
    }
}

unsafe extern "system" fn tray_window_proc(
    hwnd: HWND,
    msg: u32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    match hwnd.get_user_data::<TrayProperties>() {
        None => {}
        Some(properties) => match msg {
            CALLBACK_MSG => match LOWORD(l_param.0 as u32) {
                WM_LBUTTONUP | WM_RBUTTONUP => {
                    properties.sender.send(RunReason::TrayButton).ok();
                }
                _ => {}
            },

            _ => {}
        },
    }
    DefWindowProcW(hwnd, msg, w_param, l_param)
}
