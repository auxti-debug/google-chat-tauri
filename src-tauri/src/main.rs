// src-tauri/src-main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use tauri_plugin_deep_link;
use tauri::{
    CustomMenuItem, Manager, SystemTray, SystemTrayEvent,
    SystemTrayMenu, SystemTrayMenuItem, WindowBuilder, WindowUrl,
    UserAttentionType, WindowEvent
};

fn main() {
    tauri_plugin_deep_link::prepare("io.github.thatonecalculator.google-chat-tauri");
    let quit = CustomMenuItem::new("quit".to_string(), "Quit");
    let hide = CustomMenuItem::new("hide".to_string(), "Hide");
    let tray_menu = SystemTrayMenu::new().add_item(quit).add_native_item(SystemTrayMenuItem::Separator).add_item(hide);

    tauri::Builder::default()
        .plugin(tauri_plugin_persisted_scope::init())
        .system_tray(SystemTray::new().with_menu(tray_menu))
        .on_system_tray_event(|app, event| {
            if let SystemTrayEvent::MenuItemClick { id, .. } = event {
                let item_handle = app.tray_handle().get_item(&id);
                if id.as_str() == "quit" { std::process::exit(0); }
                if id.as_str() == "hide" {
                    let window = app.get_window("main").unwrap();
                    if window.is_visible().unwrap() {
                        window.hide().unwrap();
                        item_handle.set_title("Show").unwrap();
                    } else {
                        window.show().unwrap();
                        item_handle.set_title("Hide").unwrap();
                    }
                }
            }
        })
        .on_window_event(|event| match event.event() {
            // O gatilho que sabemos que funciona
            WindowEvent::Focused(true) => {
                println!("\n!!! EVENTO DE FOCO DETECTADO, DANDO IMPULSO !!!\n");
                let window = event.window();
                // Executa a sequência de foco de forma explícita
                window.show().unwrap();
                window.unminimize().unwrap();
                window.set_focus().unwrap();
                window.request_user_attention(Some(UserAttentionType::Informational)).unwrap();
            }
            WindowEvent::CloseRequested { api, .. } => {
                event.window().hide().unwrap();
                api.prevent_close();
            }
            _ => {}
        })
        .setup(|app| {
            let handle = app.handle();
            app.listen_global("focus-window", move |_event| {
                if let Some(window) = handle.get_window("main") {
                    println!("\n!!! EVENTO DE FOCO RECEBIDO, DANDO IMPULSO !!!\n");
                    window.show().unwrap();
                    window.unminimize().unwrap();
                    window.set_focus().unwrap();
                    window.request_user_attention(Some(UserAttentionType::Informational)).unwrap();
                }
            });

            // Usamos a criação de janela que sabemos que injeta o script corretamente.
            let resource_path = app.path_resolver().resolve_resource("preload.js").expect("failed to resolve resource");
            let preload_script = std::fs::read_to_string(&resource_path).expect("Failed to read preload.js");
            let app_handle = app.handle();

            WindowBuilder::new(&app_handle, "main", WindowUrl::App("https://mail.google.com/chat/u/0".into()))
                .title("Google Chat")
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/123.0.0.0 Safari/537.36")
                .initialization_script(&preload_script)
                .build()?;

            let handle = app.handle();
            tauri_plugin_deep_link::register(
                "google-chat",
                move |request| {
                    println!("Deep link received: {}", request);
                    if request.ends_with("://focus") {
                        if let Some(window) = handle.get_window("main") {
                            window.unminimize().unwrap();
                            window.set_focus().unwrap();
                        }
                    }
                },
            )?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri app");
}