/**
 * VERSÃO 17: Adiciona mail.google.com ao filtro de hostname.
 */
console.log("TAURI :: Preload script (v17) carregado.");

const allowedHostnames = ['chat.google.com', 'mail.google.com'];

// Só executa a lógica se estiver em um dos domínios permitidos.
if (allowedHostnames.includes(window.location.hostname)) {
    console.log(`TAURI :: Hostname '${window.location.hostname}' detectado. Aguardando DOM...`);

    window.addEventListener('DOMContentLoaded', () => {
        console.log("TAURI :: DOM carregado.");

        if (!window.__TAURI__ || !window.__TAURI__.notification || !window.__TAURI__.event) {
            console.error("TAURI :: Erro crítico: APIs __TAURI__ (notification, event) não encontradas.");
            return;
        }

        const { notification, event } = window.__TAURI__;

        const OriginalNotification = window.Notification;

        class TauriNotification extends EventTarget {
            constructor(title, options) {
                super();
                console.log(`TAURI :: Interceptado: "${title}". Disparando notificação e foco.`);
                
                // Guarda a nossa classe customizada
                const CustomNotification = window.Notification;
                // Restaura a original para evitar o loop
                window.Notification = OriginalNotification;

                // 1. Envia a notificação nativa usando a API do Tauri (que agora usará a Notification original)
                notification.sendNotification({ title, body: options.body });

                // 2. Re-aplica o nosso patch
                window.Notification = CustomNotification;

                // 3. Emite um evento para o backend do Rust para focar a janela.
                event.emit('focus-window');
            }
            close() {}

            static async requestPermission() {
                if (await notification.isPermissionGranted()) {
                    return 'granted';
                }
                return await notification.requestPermission();
            }
        }

        TauriNotification.permission = OriginalNotification.permission;
        window.Notification = TauriNotification;

        console.log("TAURI :: Patch de notificação para focar na criação concluído.");
    });
} else {
    console.log(`TAURI :: Hostname '${window.location.hostname}' ignorado.`);
}
