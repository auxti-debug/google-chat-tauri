#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::{
    AppHandle, CustomMenuItem, Manager, SystemTray, SystemTrayEvent, SystemTrayMenu,
    SystemTrayMenuItem, UserAttentionType, WindowBuilder, WindowEvent, WindowUrl,
};
use tauri_plugin_deep_link;

// --- INÍCIO: Lógica específica do Windows ---
#[cfg(target_os = "windows")]
use winrt_toast_reborn::{register, Toast, ToastManager};

#[cfg(target_os = "windows")]
const AUMID: &str = "io.github.thatonecalculator.google-chat-tauri";

#[cfg(target_os = "windows")]
#[tauri::command]
async fn show_windows_toast(app: tauri::AppHandle, title: String, body: String, _is_call: bool) {
    println!("TAURI :: Comando 'show_windows_toast' recebido.");
    
    // O 'register' foi movido para o .setup() para ser executado apenas uma vez.
    
    // O resto do 'manager' continua igual...
    let manager = ToastManager::new(AUMID).on_activated(None, {
        let app_handle = app.clone();
        move |action| {
            if action.is_none() {
                println!("\n!!! CLIQUE NO CORPO DO TOAST DETECTADO (WinRT), DANDO IMPULSO !!!\n");
                if let Some(window) = app_handle.get_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = window.request_user_attention(Some(UserAttentionType::Critical));
                }
            }
        }
    });

    // 'toast' precisa ser mutável
    let mut toast = Toast::new(); 
    
    // Apenas adicionar o texto
    toast.text1(&title).text2(&body);

    if let Err(e) = manager.show(&toast) {
        eprintln!("Erro ao mostrar toast: {:?}", e);
    }
    
    Box::leak(Box::new(manager));
}

// --- FIM: Lógica específica do Windows ---

// --- INÍCIO: Lógica "Dummy" para Não-Windows ---
#[cfg(not(target_os = "windows"))]
#[tauri::command]
async fn show_windows_toast(app: tauri::AppHandle, _title: String, _body: String, _is_call: bool) {
    println!("\n!!! Notificação recebida (Não-Windows), DANDO IMPULSO !!!\n");
    if let Some(window) = app.get_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = window.request_user_attention(Some(UserAttentionType::Critical));
    }
}
// --- FIM: Lógica "Dummy" para Não-Windows ---

// --- INÍCIO: Novo Comando 'open_url' ---
#[tauri::command]
async fn open_url(app: AppHandle, url: String) {
    println!("TAURI :: Comando 'open_url' recebido para: {}", url);
    // Verificamos se é uma chamada do Google Meet
    if url.contains("meet.google.com") {
        // Criamos a janela de chamada com um label único para evitar conflitos
        if let Ok(duration) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            let call_window_label = format!("meet-call-{}", duration.as_millis());
            if let Ok(parsed_url) = url.parse() {
                let _ = WindowBuilder::new(&app, &call_window_label, WindowUrl::External(parsed_url))
                    .title("Chamada do Google Chat").inner_size(400.0, 600.0).always_on_top(true)
                    .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
                    .build();
            } else {
                eprintln!("Falha ao parsear a URL da chamada: {}", url);
            }
        } else {
            eprintln!("Falha ao obter o tempo do sistema para o label da janela.");
        }
    } else {
        // Se não for Meet, abrimos no navegador externo usando a API shell do Tauri
        if let Err(e) = tauri::api::shell::open(&app.shell_scope(), &url, None) {
            eprintln!("Falha ao abrir URL externa {}: {:?}", url, e);
        }
    }
}
// --- FIM: Novo Comando 'open_url' ---

// Função auxiliar para alternar a visibilidade da janela, removendo duplicação de código.
fn toggle_window_visibility(app: &AppHandle) {
    if let Some(window) = app.get_window("main") {
        if let Ok(is_visible) = window.is_visible() {
            let item_handle = app.tray_handle().get_item("hide");
            if is_visible {
                let _ = window.hide();
                let _ = item_handle.set_title("Abrir");
            } else {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = item_handle.set_title("Fechar Janela");
            }
        }
    }
}

fn main() {
    tauri_plugin_deep_link::prepare("io.github.thatonecalculator.google-chat-tauri");
    let quit = CustomMenuItem::new("quit".to_string(), "Sair");
    let hide = CustomMenuItem::new("hide".to_string(), "Fechar Janela");
    let tray_menu = SystemTrayMenu::new().add_item(quit).add_native_item(SystemTrayMenuItem::Separator).add_item(hide);

    tauri::Builder::default()
        .plugin(tauri_plugin_persisted_scope::init())
        .system_tray(SystemTray::new().with_menu(tray_menu))
        .on_system_tray_event(|app, event| {
            match event {
                SystemTrayEvent::LeftClick { .. } => toggle_window_visibility(app),
                SystemTrayEvent::MenuItemClick { id, .. } => {
                    match id.as_str() {
                        "quit" => { std::process::exit(0); }
                        "hide" => toggle_window_visibility(app),
                        _ => {}
                    }
                }
                _ => {}
            }
        })
        .on_window_event(|event| match event.event() {
            WindowEvent::Focused(true) => {
                println!("\n!!! EVENTO DE FOCO DETECTADO, DANDO IMPULSO !!!\n");
                let window = event.window();
                let _ = window.show(); let _ = window.unminimize(); let _ = window.set_focus();
                let _ = window.request_user_attention(Some(UserAttentionType::Informational));
            }
            // Quando o usuário clica no 'X', a janela é escondida.
            // Atualizamos o menu da bandeja aqui.
            WindowEvent::CloseRequested { api, .. } => {
                let _ = event.window().hide();
                let _ = event.window().app_handle().tray_handle().get_item("hide").set_title("Abrir");
                api.prevent_close();
            }
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
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                    let _ = window.request_user_attention(Some(UserAttentionType::Critical));
                }
            });
            // Lemos o conteúdo do preload.js
            let preload_script_path = app.path_resolver().resolve_resource("preload.js").expect("failed to resolve preload.js");
            let preload_script = std::fs::read_to_string(&preload_script_path).expect("Failed to read preload.js");

            let app_handle = app.handle();
            WindowBuilder::new(&app_handle, "main", WindowUrl::App("https://mail.google.com/chat/u/0".into()))
                .title("Google Chat")
                .inner_size(1200.0, 800.0)
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
                // Injeta o script de pré-carregamento uma única vez.
                .initialization_script(&preload_script)
                .build()?;

            // --- INÍCIO: Registro do AUMID para Notificações do Windows ---
            #[cfg(target_os = "windows")]
            {
                use std::path::Path;
                let icon_path = Path::new("C:\\temp\\icon.ico");
                if let Err(e) = register(AUMID, "Google Chat", Some(&icon_path)) {
                    eprintln!("Falha ao registrar AUMID com caminho fixo: {:?}", e);
                } else {
                    println!("AUMID registrado com sucesso usando caminho fixo: C:\\temp\\icon.ico");
                }
            }
            // --- FIM: Registro do AUMID ---

            // Seu handler de deep-link existente
            let handle = app.handle();
            tauri_plugin_deep_link::register("google-chat", move |request| {
                println!("Deep link received: {}", request);
                if request.ends_with("://focus") {
                    if let Some(window) = handle.get_window("main") {
                        let _ = window.unminimize();
                        let _ = window.set_focus();
                    }
                }
            })?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}