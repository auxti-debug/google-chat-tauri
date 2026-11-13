use std::{
    cell::RefCell,
    fs,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::SystemTime,
};

use serde_json::{json, Value};
use tauri::{
    AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
use webview2_com::{
    CoTaskMemPWSTR, DownloadStartingEventHandler,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2DownloadOperation, ICoreWebView2_4,
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED,
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED,
    },
};
use winapi::{
    shared::windef::HWND,
    um::winuser::{KillTimer, SetTimer},
};
use windows_core::Interface;

const PANEL: &str = "downloads";
const TIMER: usize = 0x444C;
const HISTORY_LIMIT: usize = 30;

thread_local! {
    // WebView2 download operations must stay on the Windows UI thread.
    static DOWNLOADS: RefCell<Downloads> = RefCell::new(Downloads::default());
}

#[derive(Default)]
struct Downloads {
    entries: Vec<Download>,
    ready: bool,
    open_when_ready: bool,
    next_id: u64,
}

struct Download {
    id: u64,
    name: String,
    received: i64,
    total: i64,
    status: &'static str,
    active: Option<ActiveDownload>,
}

struct ActiveDownload {
    window: WebviewWindow,
    close_popup: bool,
    operation: ICoreWebView2DownloadOperation,
    path: PathBuf,
    partial_path: PathBuf,
    existing: Option<(Option<SystemTime>, u64)>,
}

pub fn setup(main: &WebviewWindow) -> tauri::Result<()> {
    let panel = WebviewWindowBuilder::new(
        main.app_handle(),
        PANEL,
        WebviewUrl::App("downloads.html".into()),
    )
    .title("Downloads")
    .inner_size(360.0, 420.0)
    .min_inner_size(300.0, 220.0)
    .maximizable(false)
    .minimizable(false)
    .skip_taskbar(true)
    .visible(false)
    .parent(main)?
    .on_page_load(|window, page| {
        if page.event() != tauri::webview::PageLoadEvent::Finished {
            return;
        }

        let open = DOWNLOADS.with(|downloads| {
            let mut downloads = downloads.borrow_mut();
            downloads.ready = true;
            std::mem::take(&mut downloads.open_when_ready)
        });
        render(&window);
        if open {
            show(window.app_handle(), false);
        }
    })
    .build()?;
    let hide = panel.clone();
    panel.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hide.hide();
        }
    });
    Ok(())
}

pub fn show(app: &AppHandle, focus: bool) {
    let Some(panel) = app.get_webview_window(PANEL) else {
        return;
    };
    let ready = DOWNLOADS.with(|downloads| {
        let mut downloads = downloads.borrow_mut();
        downloads.open_when_ready = !downloads.ready;
        downloads.ready
    });
    if !ready {
        return;
    }

    if !matches!(panel.is_visible(), Ok(true)) {
        if let Some(main) = app.get_webview_window("main") {
            position_panel(&main, &panel);
        }
        render(&panel);
        let _ = panel.show();
    }
    if focus {
        let _ = panel.set_focus();
    }
}

fn position_panel(main: &WebviewWindow, panel: &WebviewWindow) {
    let (Ok(position), Ok(size), Ok(panel_size), Ok(Some(monitor))) = (
        main.outer_position(),
        main.outer_size(),
        panel.outer_size(),
        main.current_monitor(),
    ) else {
        return;
    };

    let area = monitor.work_area();
    let right = area.position.x + area.size.width as i32 - panel_size.width as i32;
    let bottom = area.position.y + area.size.height as i32 - panel_size.height as i32;
    let x = (position.x + size.width as i32 - panel_size.width as i32 - 14)
        .clamp(area.position.x, right.max(area.position.x));
    let y = (position.y + 38).clamp(area.position.y, bottom.max(area.position.y));
    let _ = panel.set_position(PhysicalPosition::new(x, y));
}

fn render(panel: &WebviewWindow) {
    let items = DOWNLOADS.with(|downloads| {
        downloads
            .borrow()
            .entries
            .iter()
            .rev()
            .map(|item| {
                json!({
                    "id": item.id, "name": item.name, "received": item.received,
                    "total": item.total, "status": item.status,
                })
            })
            .collect::<Vec<Value>>()
    });
    let json = Value::Array(items);
    let _ = panel.eval(format!("window.renderDownloads?.({json})"));
}

pub fn watch(window: &WebviewWindow, popup: bool) -> tauri::Result<()> {
    let window = window.clone();
    let observer = window.clone();
    window.with_webview(move |platform| {
        let result = unsafe {
            platform
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.cast::<ICoreWebView2_4>())
                .and_then(|webview| {
                    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        let Some(main) = observer.app_handle().get_webview_window("main") else {
                            return Ok(());
                        };
                        let Ok(hwnd) = main.hwnd() else { return Ok(()) };
                        let operation = args.DownloadOperation()?;
                        let mut path = Default::default();
                        args.ResultFilePath(&mut path)?;
                        let path = PathBuf::from(CoTaskMemPWSTR::from(path).to_string());
                        if SetTimer(hwnd.0.cast(), TIMER, 200, Some(update_progress)) == 0 {
                            return Ok(());
                        }

                        let close_popup = popup && matches!(observer.is_visible(), Ok(false));
                        let name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Arquivo".into());
                        let mut total = -1;
                        operation.TotalBytesToReceive(&mut total).ok();
                        let existing = file_stamp(&path);
                        let mut partial = path.as_os_str().to_os_string();
                        partial.push(".crdownload");
                        let partial_path = PathBuf::from(partial);

                        DOWNLOADS.with(|downloads| {
                            let mut downloads = downloads.borrow_mut();
                            let id = downloads.next_id;
                            downloads.next_id += 1;
                            downloads.entries.push(Download {
                                id,
                                name,
                                received: 0,
                                total,
                                status: "active",
                                active: Some(ActiveDownload {
                                    window: observer.clone(),
                                    close_popup,
                                    operation,
                                    path,
                                    partial_path,
                                    existing,
                                }),
                            });
                            if downloads.entries.len() > HISTORY_LIMIT {
                                if let Some(index) = downloads
                                    .entries
                                    .iter()
                                    .position(|entry| entry.active.is_none())
                                {
                                    downloads.entries.remove(index);
                                }
                            }
                        });

                        if let Some(panel) = observer.app_handle().get_webview_window(PANEL) {
                            // Handled suppresses the browser's popup while WebView2 still saves the file.
                            args.SetHandled(true)?;
                            render(&panel);
                            show(observer.app_handle(), false);
                        }
                        Ok(())
                    }));
                    webview.add_DownloadStarting(&handler, &mut 0)
                })
        };
        if let Err(error) = result {
            crate::log_debug(&format!(
                "[downloads] could not register WebView2 handler: {error}"
            ));
        }
    })
}

fn file_stamp(path: &Path) -> Option<(Option<SystemTime>, u64)> {
    let file = fs::metadata(path).ok()?;
    file.is_file().then(|| (file.modified().ok(), file.len()))
}

impl Download {
    fn refresh(&mut self) -> Option<WebviewWindow> {
        let active = self.active.as_ref()?;
        let operation = &active.operation;

        let mut total = self.total;
        let mut received = self.received;
        let mut state = Default::default();
        unsafe {
            operation.TotalBytesToReceive(&mut total).ok();
            operation.BytesReceived(&mut received).ok();
            operation.State(&mut state).ok();
        }
        // Popup downloads can report zero until they finish. WebView2 may write
        // to the destination directly or use a temporary .crdownload file.
        if let Some((_, size)) = file_stamp(&active.partial_path) {
            received = received.max(i64::try_from(size).unwrap_or(i64::MAX));
        }
        let written = file_stamp(&active.path).filter(|stamp| Some(*stamp) != active.existing);
        if let Some((_, size)) = written {
            received = received.max(i64::try_from(size).unwrap_or(i64::MAX));
        }
        self.received = received.max(0);
        self.total = total;

        // Verify that the browser has released the file before closing its popup.
        let saved = written.filter(|stamp| {
            (total < 0 || stamp.1 == total as u64)
                && fs::OpenOptions::new()
                    .read(true)
                    .share_mode(0)
                    .open(&active.path)
                    .is_ok()
        });
        if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED || saved.is_some() {
            self.status = "complete";
            if let Some((_, size)) = saved {
                self.received = i64::try_from(size).unwrap_or(i64::MAX);
            }
        } else if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
            let mut reason = Default::default();
            unsafe { operation.InterruptReason(&mut reason).ok() };
            self.status = if reason == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED {
                "canceled"
            } else {
                "interrupted"
            };
        } else if active
            .window
            .app_handle()
            .get_webview_window(active.window.label())
            .is_none()
        {
            self.status = "interrupted";
        } else {
            return None;
        }

        self.active
            .take()
            .filter(|active| active.close_popup)
            .map(|active| active.window)
    }
}

unsafe extern "system" fn update_progress(hwnd: HWND, _: u32, _: usize, _: u32) {
    let (app, running, popups) = DOWNLOADS.with(|downloads| {
        let mut downloads = downloads.borrow_mut();
        let app = downloads.entries.iter().find_map(|item| {
            item.active
                .as_ref()
                .map(|active| active.window.app_handle().clone())
        });
        let popups: Vec<_> = downloads
            .entries
            .iter_mut()
            .filter_map(Download::refresh)
            .collect();
        let running = downloads.entries.iter().any(|item| item.active.is_some());
        (app, running, popups)
    });
    for window in popups {
        let still_downloading = DOWNLOADS.with(|downloads| {
            downloads.borrow().entries.iter().any(|item| {
                item.active
                    .as_ref()
                    .is_some_and(|active| active.window.label() == window.label())
            })
        });
        if !still_downloading && matches!(window.is_visible(), Ok(false)) {
            let _ = window.close();
        }
    }
    if let Some(panel) = app.and_then(|app| app.get_webview_window(PANEL)) {
        if matches!(panel.is_visible(), Ok(true)) {
            render(&panel);
        }
    }
    if !running {
        KillTimer(hwnd, TIMER);
    }
}
