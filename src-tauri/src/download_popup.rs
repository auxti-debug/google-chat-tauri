use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use tauri::WebviewWindow;
use webview2_com::{
    CoTaskMemPWSTR, DownloadStartingEventHandler, Microsoft::Web::WebView2::Win32::ICoreWebView2_4,
};
use windows_core::Interface;

pub fn watch(window: &WebviewWindow) -> tauri::Result<()> {
    let popup = window.clone();
    window.with_webview(move |platform| {
        let result = unsafe {
            platform
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.cast::<ICoreWebView2_4>())
                .and_then(|webview| {
                    let window = popup.clone();
                    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else { return Ok(()) };
                        // A visible popup has loaded a page and can manage its own downloads.
                        if matches!(window.is_visible(), Ok(false)) {
                            let mut destination = Default::default();
                            args.ResultFilePath(&mut destination)?;
                            let destination =
                                PathBuf::from(CoTaskMemPWSTR::from(destination).to_string());
                            let mut total = -1;
                            if let Ok(operation) = args.DownloadOperation() {
                                let _ = operation.TotalBytesToReceive(&mut total);
                            }
                            close_after_save(
                                window.clone(),
                                destination,
                                u64::try_from(total).ok(),
                            );
                        }
                        Ok(())
                    }));
                    webview.add_DownloadStarting(&handler, &mut 0)
                })
        };
        if let Err(error) = result {
            crate::log_debug(&format!("[popup] could not watch downloads: {error}"));
            let _ = popup.show();
        }
    })
}

fn close_after_save(window: WebviewWindow, destination: PathBuf, expected_size: Option<u64>) {
    // WebView2 saves to a temporary file and commits the final path on completion.
    // Its StateChanged event is not reliable for downloads started by popup navigation.
    let existing = std::fs::metadata(&destination)
        .ok()
        .map(|file| (file.modified().ok(), file.len()));
    tauri::async_runtime::spawn_blocking(move || {
        let deadline = Instant::now() + Duration::from_secs(1800);
        while Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(350));
            if !matches!(window.is_visible(), Ok(false)) {
                break;
            }

            if let Ok(file) = std::fs::metadata(&destination) {
                if file.is_file()
                    && expected_size.is_none_or(|size| file.len() == size)
                    && Some((file.modified().ok(), file.len())) != existing
                {
                    let _ = window.close();
                    break;
                }
            }
        }
    });
}
