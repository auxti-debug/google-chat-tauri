#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

#[cfg(target_os = "windows")]
use std::ptr::{null, null_mut};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, UserAttentionType, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_deep_link::DeepLinkExt;

#[cfg(target_os = "windows")]
mod webview_update;

#[cfg(target_os = "windows")]
fn log_debug(msg: &str) {
    use std::fs::{create_dir_all, OpenOptions};
    use std::io::Write;
    use std::path::PathBuf;

    let mut path = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    path.push("Google Chat");
    let _ = create_dir_all(&path);
    path.push("focus.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let line = format!("{timestamp} | {msg}\n");
        let _ = f.write_all(line.as_bytes());
    }
}

#[cfg(not(target_os = "windows"))]
fn log_debug(_msg: &str) {}

fn focus_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(UserAttentionType::Informational));
        set_tray_visibility_label(app, true);
    }
}

#[cfg(target_os = "windows")]
unsafe fn hicon_from_badge(count: u32, size: i32) -> winapi::shared::windef::HICON {
    use winapi::shared::minwindef::DWORD;
    use winapi::shared::windef::{HBITMAP, HBRUSH, HDC, HGDIOBJ, RECT};
    use winapi::um::wingdi::{
        CreateBitmap, CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateSolidBrush,
        DeleteDC, DeleteObject, Ellipse, SelectObject, SetBkMode, SetTextColor, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, RGB,
    };
    use winapi::um::winuser::{
        CreateIconIndirect, DrawTextW, DT_CENTER, DT_SINGLELINE, DT_VCENTER, ICONINFO,
    };

    // DIB 32bpp top-down
    let mut bi: BITMAPINFO = std::mem::zeroed();
    bi.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as DWORD,
        biWidth: size,
        biHeight: -size, // top-down
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut bits_ptr: *mut std::ffi::c_void = null_mut();
    let hbm: HBITMAP = CreateDIBSection(
        null_mut(),
        &bi,
        DIB_RGB_COLORS,
        &mut bits_ptr,
        null_mut(),
        0,
    );
    if hbm.is_null() {
        return null_mut();
    }

    std::ptr::write_bytes(bits_ptr.cast::<u32>(), 0, (size * size) as usize);
    let mem_dc: HDC = CreateCompatibleDC(null_mut());
    if mem_dc.is_null() {
        DeleteObject(hbm as HGDIOBJ);
        return null_mut();
    }
    let old_obj: HGDIOBJ = SelectObject(mem_dc, hbm as HGDIOBJ);

    // Desenha círculo vermelho (aumentado mais um pouco)
    let radius = (size as f32 * 0.82) as i32; // diâmetro ~82% do overlay
    let left = size - radius;
    let top = 0;
    let right = size;
    let bottom = radius;

    let red_brush: HBRUSH = CreateSolidBrush(RGB(220, 0, 0));
    let _ = SelectObject(mem_dc, red_brush as HGDIOBJ);
    Ellipse(mem_dc, left, top, right, bottom);

    // Texto branco
    let text = if count > 99 {
        String::from("99+")
    } else {
        count.to_string()
    };
    SetBkMode(mem_dc, 1); // TRANSPARENT
    SetTextColor(mem_dc, RGB(255, 255, 255));
    let font_height = (radius as f32 * 0.72) as i32; // aumenta a fonte proporcionalmente
    let hfont = CreateFontW(
        -font_height,
        0,
        0,
        0,
        700,
        0,
        0,
        0,
        1,
        0,
        0,
        0,
        0,
        "Segoe UI"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
            .as_ptr(),
    );
    let _old_font = SelectObject(mem_dc, hfont as HGDIOBJ);
    let mut rect = RECT {
        left,
        top,
        right,
        bottom,
    };
    let wstr: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    DrawTextW(
        mem_dc,
        wstr.as_ptr(),
        -1,
        &mut rect,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );

    // Força alpha em pixels desenhados
    let total = (size * size) as usize;
    let buf = std::slice::from_raw_parts_mut(bits_ptr as *mut u32, total);
    for px in buf.iter_mut() {
        if *px & 0x00FFFFFF != 0 {
            *px |= 0xFF000000;
        }
    }

    let _ = SelectObject(mem_dc, old_obj);
    DeleteObject(hfont as HGDIOBJ);
    DeleteObject(red_brush as HGDIOBJ);
    DeleteDC(mem_dc);

    // CreateIconIndirect requires a monochrome mask separate from the ARGB bitmap.
    let mask_bits = vec![0u8; (size as usize).div_ceil(16) * 2 * size as usize];
    let mask = CreateBitmap(size, size, 1, 1, mask_bits.as_ptr().cast());
    let icon = if mask.is_null() {
        null_mut()
    } else {
        let mut info: ICONINFO = std::mem::zeroed();
        info.fIcon = 1;
        info.hbmColor = hbm;
        info.hbmMask = mask;
        let icon = CreateIconIndirect(&mut info);
        DeleteObject(mask as HGDIOBJ);
        icon
    };
    DeleteObject(hbm as HGDIOBJ);
    icon
}

#[cfg(target_os = "windows")]
fn set_taskbar_overlay_count(hwnd: winapi::shared::windef::HWND, count: u32) {
    use std::iter::once;
    use winapi::shared::{winerror::SUCCEEDED, wtypesbase::CLSCTX_INPROC_SERVER};
    use winapi::um::{
        combaseapi::{CoCreateInstance, CoInitializeEx, CoUninitialize},
        objbase::COINIT_APARTMENTTHREADED,
        shobjidl_core::{CLSID_TaskbarList, ITaskbarList3},
        winuser::DestroyIcon,
    };
    use winapi::Interface;

    unsafe {
        let initialized = SUCCEEDED(CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED));
        let mut taskbar: *mut ITaskbarList3 = null_mut();
        let hr = CoCreateInstance(
            &CLSID_TaskbarList,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &ITaskbarList3::uuidof(),
            &mut taskbar as *mut _ as *mut _,
        );
        if SUCCEEDED(hr) && !taskbar.is_null() {
            let _ = (*taskbar).HrInit();
            if count == 0 {
                let _ = (*taskbar).SetOverlayIcon(hwnd, null_mut(), null());
            } else {
                let hicon = hicon_from_badge(count, 32);
                if !hicon.is_null() {
                    let desc: Vec<u16> = count.to_string().encode_utf16().chain(once(0)).collect();
                    if !SUCCEEDED((*taskbar).SetOverlayIcon(hwnd, hicon, desc.as_ptr())) {
                        log_debug("[taskbar] could not set the unread badge");
                    }
                    DestroyIcon(hicon);
                } else {
                    log_debug("[taskbar] could not create the unread badge");
                }
            }
            let _ = (*taskbar).Release();
        } else {
            log_debug("[taskbar] CoCreateInstance ITaskbarList3 failed");
        }
        if initialized {
            CoUninitialize();
        }
    }
}

#[tauri::command]
fn set_unread_count(window: WebviewWindow, count: u32) {
    #[cfg(target_os = "windows")]
    if let Ok(hwnd) = window.hwnd() {
        set_taskbar_overlay_count(hwnd.0.cast(), count);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (window, count);
}

#[cfg(target_os = "windows")]
use winrt_toast_reborn::{Toast, ToastManager};

#[cfg(target_os = "windows")]
const AUMID: &str = "io.github.thatonecalculator.google-chat-tauri";

#[cfg(target_os = "windows")]
const NOTIFICATION_ICON_RESOURCE: &str = "icons/Square30x30Logo.png";

#[cfg(target_os = "windows")]
fn shell_compatible_path(path: std::path::PathBuf) -> std::path::PathBuf {
    let raw_path = path.to_string_lossy();

    if let Some(unc_path) = raw_path.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{unc_path}"));
    }

    if let Some(drive_path) = raw_path.strip_prefix(r"\\?\") {
        return std::path::PathBuf::from(drive_path);
    }

    drop(raw_path);
    path
}

#[cfg(target_os = "windows")]
fn register_notification_identity(icon_path: &std::path::Path) -> std::io::Result<()> {
    use winreg::{
        enums::{HKEY_CURRENT_USER, REG_EXPAND_SZ},
        types::ToRegValue,
        RegKey,
    };

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu.create_subkey(format!(r"SOFTWARE\Classes\AppUserModelId\{AUMID}"))?;

    let mut display_name = "Google Chat".to_reg_value();
    display_name.vtype = REG_EXPAND_SZ;
    key.set_raw_value("DisplayName", &display_name)?;

    let mut icon_uri = icon_path.as_os_str().to_reg_value();
    icon_uri.vtype = REG_EXPAND_SZ;
    key.set_raw_value("IconUri", &icon_uri)?;

    key.set_value("IconBackgroundColor", &"00000000")?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[tauri::command]
async fn show_windows_toast(
    app: tauri::AppHandle,
    title: String,
    body: String,
    is_call: bool,
) -> Result<(), String> {
    log_debug(&format!(
        "[toast] request received (title_len={}, body_len={}, call={})",
        title.chars().count(),
        body.chars().count(),
        is_call
    ));

    let manager = ToastManager::new(AUMID)
        .on_activated(None, {
            let app_handle = app.clone();
            move |action| {
                if action.is_none() {
                    log_debug("[toast] activated -> focusing main window");
                    focus_main_window(&app_handle);
                }
            }
        })
        .on_failed(|failure| {
            log_debug(&format!("[toast] Windows delivery failed: {:?}", failure));
        });

    let mut toast = Toast::new();
    toast.text1(&title).text2(&body);

    match manager.show(&toast) {
        Ok(()) => {
            log_debug("[toast] displayed successfully");
            Ok(())
        }
        Err(error) => {
            log_debug(&format!("[toast] show failed: {:?}", error));
            Err(format!(
                "Windows could not display the notification: {error:?}"
            ))
        }
    }
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn show_windows_toast(
    app: tauri::AppHandle,
    _title: String,
    _body: String,
    _is_call: bool,
) -> Result<(), String> {
    println!("\n!!! Notificação recebida (Não-Windows), DANDO IMPULSO !!!\n");
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(UserAttentionType::Critical));
    }
    Ok(())
}

#[tauri::command]
fn is_main_window_visible(window: WebviewWindow) -> Result<bool, String> {
    window.is_visible().map_err(|error| error.to_string())
}

// Abre apenas chamadas HTTPS do Google Meet em uma janela separada.
#[tauri::command]
async fn open_url(app: AppHandle, url: String) -> Result<(), String> {
    let parsed_url =
        url::Url::parse(&url).map_err(|_| "The Google Meet URL is invalid.".to_string())?;

    if parsed_url.scheme() != "https"
        || parsed_url.host_str() != Some("meet.google.com")
        || parsed_url.port().is_some()
        || !parsed_url.username().is_empty()
        || parsed_url.password().is_some()
    {
        log_debug("[meet] rejected URL outside the Google Meet HTTPS origin");
        return Err("Only HTTPS links from meet.google.com are allowed.".to_string());
    }

    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_CALL: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_CALL.fetch_add(1, Ordering::Relaxed);
    let call_window_label = format!("meet-call-{id}");

    WebviewWindowBuilder::new(&app, &call_window_label, WebviewUrl::External(parsed_url))
        .title("Chamada do Google Chat")
        .inner_size(400.0, 600.0)
        .always_on_top(true)
        .build()
        .map_err(|error| format!("The Google Meet window could not be created: {error}"))?;

    log_debug("[meet] call window opened");
    Ok(())
}

fn set_tray_visibility_label(app: &AppHandle, visible: bool) {
    if let Some(item) = app.try_state::<MenuItem<tauri::Wry>>() {
        let _ = item.set_text(if visible { "Fechar Janela" } else { "Abrir" });
    }
}

fn toggle_window_visibility(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        match window.is_visible() {
            Ok(true) => {
                let _ = window.hide();
                set_tray_visibility_label(app, false);
            }
            Ok(false) => focus_main_window(app),
            Err(_) => {}
        }
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "Fechar Janela", true, None::<&str>)?;
    let restart = MenuItem::with_id(app, "restart", "Reiniciar", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&hide, &restart, &separator, &quit])?;
    app.manage(hide);

    let mut tray = TrayIconBuilder::new()
        .tooltip("Google Chat")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "restart" => app.request_restart(),
            "hide" => toggle_window_visibility(app),

            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window_visibility(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

fn main() {
    log_debug("[boot] main starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            focus_main_window(app)
        }))
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(tauri::generate_handler![
            show_windows_toast,
            set_unread_count,
            is_main_window_visible,
            open_url
        ])
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            match event {
                WindowEvent::Focused(true) => {
                    set_tray_visibility_label(window.app_handle(), true);
                    #[cfg(target_os = "windows")]
                    if let Ok(hwnd) = window.hwnd() {
                        set_taskbar_overlay_count(hwnd.0.cast(), 0);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    let _ = window.hide();
                    set_tray_visibility_label(window.app_handle(), false);
                    api.prevent_close();
                }
                _ => {}
            }
        })
        .setup(|app| {
            setup_tray(app)?;

            #[cfg(target_os = "windows")]
            {
                use tauri::path::BaseDirectory;

                match app
                    .path()
                    .resolve(NOTIFICATION_ICON_RESOURCE, BaseDirectory::Resource)
                {
                    Ok(path) if path.is_file() => {
                        match register_notification_identity(&shell_compatible_path(path)) {
                            Ok(()) => log_debug("[toast] Windows notification identity registered"),
                            Err(error) => {
                                log_debug(&format!("[toast] registration failed: {error}"))
                            }
                        }
                    }
                    Ok(_) => log_debug("[toast] bundled notification icon was not found"),
                    Err(error) => log_debug(&format!("[toast] icon path unavailable: {error}")),
                }
            }

            let window = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External("https://chat.google.com/u/0/".parse()?),
            )
            .title("Google Chat")
            .inner_size(1200.0, 800.0)
            .initialization_script_for_all_frames(include_str!("../preload.js"))
            .build()?;

            #[cfg(target_os = "windows")]
            webview_update::watch(&window);
            #[cfg(not(target_os = "windows"))]
            let _ = window;

            // Windows installers register the protocol. On Linux, register it
            // for launches outside an installed desktop bundle as well.
            #[cfg(target_os = "linux")]
            if let Err(error) = app.deep_link().register_all() {
                eprintln!("Could not register the google-chat protocol: {error}");
            }

            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |event| {
                if event
                    .urls()
                    .iter()
                    .any(|url| url.scheme() == "google-chat" && url.host_str() == Some("focus"))
                {
                    focus_main_window(&handle);
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running the Tauri app");
}

#[cfg(all(test, target_os = "windows"))]
mod tests {
    use super::{hicon_from_badge, shell_compatible_path};
    use std::path::PathBuf;

    #[test]
    fn creates_a_windows_icon_for_the_unread_badge() {
        unsafe {
            let icon = hicon_from_badge(101, 32);
            assert!(!icon.is_null());
            assert_ne!(winapi::um::winuser::DestroyIcon(icon), 0);
        }
    }

    #[test]
    fn converts_verbatim_drive_path_for_windows_shell() {
        assert_eq!(
            shell_compatible_path(PathBuf::from(
                r"\\?\C:\Program Files\Google Chat\Square30x30Logo.png"
            )),
            PathBuf::from(r"C:\Program Files\Google Chat\Square30x30Logo.png")
        );
    }

    #[test]
    fn converts_verbatim_unc_path_for_windows_shell() {
        assert_eq!(
            shell_compatible_path(PathBuf::from(
                r"\\?\UNC\server\share\Google Chat\Square30x30Logo.png"
            )),
            PathBuf::from(r"\\server\share\Google Chat\Square30x30Logo.png")
        );
    }
}
