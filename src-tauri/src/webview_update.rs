use std::cell::Cell;

use tauri::WebviewWindow;
use webview2_com::{CoTaskMemPWSTR, NewBrowserVersionAvailableEventHandler};

pub fn watch(window: &WebviewWindow) {
    if let Err(error) = window.with_webview(|webview| {
        let environment = webview.environment();
        let mut version = Default::default();
        if unsafe { environment.BrowserVersionString(&mut version) }.is_ok() {
            let version = CoTaskMemPWSTR::from(version);
            crate::log_debug(&format!("[webview2] runtime {version}"));
        }

        let notified = Cell::new(false);
        let handler = NewBrowserVersionAvailableEventHandler::create(Box::new(move |_, _| {
            if notified.get() {
                return Ok(());
            }

            crate::log_debug("[webview2] an Evergreen update is available");
            let mut toast = crate::Toast::new();
            toast
                .text1("Atualização do navegador disponível")
                .text2("Use Reiniciar no menu da bandeja para atualizar o WebView2.");
            match crate::ToastManager::new(crate::AUMID).show(&toast) {
                Ok(()) => notified.set(true),
                Err(error) => {
                    crate::log_debug(&format!("[webview2] notification failed: {error:?}"))
                }
            }
            Ok(())
        }));

        let mut token = 0;
        if let Err(error) =
            unsafe { environment.add_NewBrowserVersionAvailable(&handler, &mut token) }
        {
            crate::log_debug(&format!("[webview2] update listener unavailable: {error}"));
        }
    }) {
        crate::log_debug(&format!("[webview2] could not access runtime: {error}"));
    }
}
