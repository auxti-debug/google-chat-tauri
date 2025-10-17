#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::{
    AppHandle,
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu, SystemTrayMenuItem,
    WindowBuilder, WindowUrl, UserAttentionType, WindowEvent,
};
use tauri_plugin_deep_link;

// --- INÍCIO: Lógica específica do Windows ---
#[cfg(target_os = "windows")]
use winrt_toast_reborn::{register, Action, Toast, ToastManager};

#[cfg(target_os = "windows")]
const AUMID: &str = "io.github.thatonecalculator.google-chat-tauri";

#[cfg(target_os = "windows")]
#[tauri::command]
async fn show_windows_toast(app: tauri::AppHandle, title: String, body: String) {
    println!("TAURI :: Comando 'show_windows_toast' recebido.");
    let icon_path = app.path_resolver().resolve_resource("icons/icon.ico").expect("failed to resolve icon path");
    if let Err(e) = register(AUMID, "Google Chat", Some(&icon_path)) { eprintln!("Falha ao registrar AUMID: {:?}", e); }
    let manager = ToastManager::new(AUMID).on_activated(None, {
        let app_handle = app.clone();
        move |action| {
            if let Some(act) = action {
                if act.arg == "open" {
                    println!("\n!!! CLIQUE NO TOAST DETECTADO (WinRT), DANDO IMPULSO !!!\n");
                    if let Some(window) = app_handle.get_window("main") {
                        window.show().unwrap_or_default(); window.unminimize().unwrap_or_default(); window.set_focus().unwrap_or_default();
                        window.request_user_attention(Some(UserAttentionType::Critical)).unwrap_or_default();
                    }
                }
            }
        }
    });
    let mut toast = Toast::new(); toast.text1(&title).text2(&body).action(Action::new("Ver", "open", ""));
    if let Err(e) = manager.show(&toast) { eprintln!("Erro ao mostrar toast: {:?}", e); }
    Box::leak(Box::new(manager));
}
// --- FIM: Lógica específica do Windows ---

// --- INÍCIO: Lógica "Dummy" para Não-Windows ---
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn show_windows_toast(app: tauri::AppHandle, title: String, body: String) {
    println!("\n!!! Notificação recebida (Não-Windows), DANDO IMPULSO !!!\n");
    if let Some(window) = app.get_window("main") {
        window.show().unwrap_or_default(); window.unminimize().unwrap_or_default(); window.set_focus().unwrap_or_default();
        window.request_user_attention(Some(UserAttentionType::Critical)).unwrap_or_default();
    }
}
// --- FIM: Lógica "Dummy" para Não-Windows ---

// --- INÍCIO: Novo Comando 'open_url' ---
#[tauri::command]
async fn open_url(app: AppHandle, url: String) {
    println!("TAURI :: Comando 'open_url' recebido para: {}", url);
    // Verificamos se é uma chamada do Google Meet
    if url.contains("meet.google.com") {
        // Criamos a janela de chamada
        let call_window_label = format!("meet-call-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis());
        WindowBuilder::new(&app, &call_window_label, WindowUrl::External(url.parse().unwrap()))
            .title("Chamada do Google Chat").inner_size(400.0, 600.0).always_on_top(true)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
            .build().expect("Falha ao criar a janela de chamada");
    } else {
        // Se não for Meet, abrimos no navegador externo usando a API shell do Tauri
        if let Err(e) = tauri::api::shell::open(&app.shell_scope(), &url, None) {
            eprintln!("Falha ao abrir URL externa {}: {:?}", url, e);
        }
    }
}
// --- FIM: Novo Comando 'open_url' ---

fn main() {
    tauri_plugin_deep_link::prepare("io.github.thatonecalculator.google-chat-tauri");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let hide = CustomMenuItem::new("hide".to_string(), "Hide");
    let tray_menu = SystemTrayMenu::new().add_item(quit).add_native_item(SystemTrayMenuItem::Separator).add_item(hide);

    tauri::Builder::default()
        .plugin(tauri_plugin_persisted_scope::init())
        .system_tray(SystemTray::new().with_menu(tray_menu))
        .on_system_tray_event(|app, event| { /* ... seu código existente ... */
             let window = app.get_window("main").unwrap();
            let item_handle = app.tray_handle().get_item("hide");

            match event {
                SystemTrayEvent::LeftClick {..} => {
                    if window.is_visible().unwrap() { window.hide().unwrap(); item_handle.set_title("Show").unwrap(); }
                    else { window.show().unwrap(); item_handle.set_title("Hide").unwrap(); }
                }
                SystemTrayEvent::MenuItemClick { id, .. } => {
                    match id.as_str() {
                        "quit" => { std::process::exit(0); }
                        "hide" => {
                           if window.is_visible().unwrap() { window.hide().unwrap(); item_handle.set_title("Show").unwrap(); }
                           else { window.show().unwrap(); item_handle.set_title("Hide").unwrap(); }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .on_window_event(|event| match event.event() { /* ... seu código existente ... */
            WindowEvent::Focused(true) => {
                println!("\n!!! EVENTO DE FOCO DETECTADO, DANDO IMPULSO !!!\n");
                let window = event.window();
                window.show().unwrap(); window.unminimize().unwrap(); window.set_focus().unwrap();
                window.request_user_attention(Some(UserAttentionType::Informational)).unwrap();
            }
            WindowEvent::CloseRequested { api, .. } => { event.window().hide().unwrap(); api.prevent_close(); }
            _ => {}
        })
        // REMOVEMOS o .window_handler() daqui

        // Registra os comandos necessários
        .invoke_handler(tauri::generate_handler![
            show_windows_toast,
            open_url // <-- Registra o novo comando
        ])
        .setup(|app| {
            // --- INÍCIO: Novo Ouvinte para Chamadas ---
            let handle = app.handle();
            app.listen_global("incoming-call", move |_event| {
                println!("\n!!! EVENTO 'incoming-call' RECEBIDO, DANDO IMPULSO !!!\n");
                if let Some(window) = handle.get_window("main") {
                    // Apenas foca a janela principal. O resto (window.open)
                    // será tratado pelo patch no preload.js.
                    window.show().unwrap_or_default();
                    window.unminimize().unwrap_or_default();
                    window.set_focus().unwrap_or_default();
                    window.request_user_attention(Some(UserAttentionType::Critical))
                        .unwrap_or_default();
                }
            });
            // Lemos o conteúdo do preload.js
            let preload_script_path = app.path_resolver().resolve_resource("preload.js").expect("failed to resolve preload.js");
            let preload_script = std::fs::read_to_string(&preload_script_path).expect("Failed to read preload.js");

            let app_handle = app.handle();
            WindowBuilder::new(&app_handle, "main", WindowUrl::App("https://mail.google.com/chat/u/0".into()))
                .title("Google Chat")
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
                // Injectamos o init.js PRIMEIRO, para interceptar window.open
                .initialization_script(&preload_script)
                // Injectamos o preload.js DEPOIS, para interceptar Notification
                .initialization_script(&preload_script)
                .build()?;

            // Seu handler de deep-link existente
            let handle = app.handle();
            tauri_plugin_deep_link::register("google-chat", move |request| { /* ... seu código existente ... */
                println!("Deep link received: {}", request);
                if request.ends_with("://focus") {
                    if let Some(window) = handle.get_window("main") {
                        window.unminimize().unwrap(); window.set_focus().unwrap();
                    }
                }
            })?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}