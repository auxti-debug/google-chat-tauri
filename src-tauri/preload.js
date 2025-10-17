/**
 * VERSÃO 29 (Plano S - Revisitado): Separa IPC.
 * - Mensagens: Usa invoke('show_windows_toast').
 * - Chamadas: Usa event.emit('incoming-call') para sinalizar o Rust.
 * Mantém o patch 'window.open'.
 */
(function() { // Mantém a IIFE
    console.log("TAURI :: Preload script (v29) carregado.");

    const allowedHostnames = ['chat.google.com', 'mail.google.com', 'meet.google.com'];

    if (allowedHostnames.includes(window.location.hostname)) {
        console.log(`TAURI (preload v29): Hostname '${window.location.hostname}' detectado. Aguardando DOM...`);

        window.addEventListener('DOMContentLoaded', () => {
            console.log("TAURI (preload v29): DOM carregado. Aplicando patches...");

            if (!window.__TAURI__ || !window.__TAURI__.tauri || !window.__TAURI__.event) {
                // Agora precisamos do 'event' também
                console.error("TAURI (preload v29): Erro crítico: APIs __TAURI__ (tauri, event) não encontradas.");
                return;
            }

            const { invoke } = window.__TAURI__.tauri;
            const { emit } = window.__TAURI__.event; // Pega a função 'emit'
            const originalWindowOpen = window.open;
            // Não precisamos mais do OriginalNotification aqui

            // --- PATCH 1: new Notification (IPC SEPARADO) ---
            window.Notification = function(title, options) {
                
                if (title.includes("chamada") || title.includes("call")) {
                    // É UMA CHAMADA:
                    console.log(`TAURI (preload v29): Interceptado (Chamada): "${title}". Emitindo 'incoming-call'.`);
                    
                    // Tenta emitir o evento global para o Rust focar a janela
                    try {
                         emit('incoming-call', { notificationTitle: title }); 
                    } catch (e) {
                        console.error("TAURI (preload v29): Falha ao emitir 'incoming-call':", e);
                    }
                    // Apenas sinaliza o Rust e retorna o objeto falso.
                    return new EventTarget();

                } else {
                    // É UMA MENSAGEM:
                    console.log(`TAURI (preload v29): Interceptado (Mensagem): "${title}". Chamando 'show_windows_toast'.`);
                    // Chama o comando Rust para o toast clicável (funciona)
                    invoke('show_windows_toast', { title: title, body: options.body || '' }); //
                    return new EventTarget();
                }
            };
            
            window.Notification.requestPermission = async function() { return 'granted'; }; // "Mente"
            window.Notification.permission = 'granted'; // "Mente"
            
            console.log("TAURI (preload v29): Patch de notificação (IPC Separado) concluído.");

            // --- PATCH 2: window.open (para chamadas - SEM MUDANÇAS) ---
            window.open = function(url, target, features) {
                console.log(`TAURI (preload v29): Interceptado (window.open): "${url}"`);

                if (url && (url.includes('meet.google.com') || url.includes('/call/'))) { //
                    console.log("TAURI (preload v29): URL de chamada detectada. Chamando 'open_url'.");
                    invoke('open_url', { url: url }); //
                    return null;
                }

                console.log("TAURI (preload v29): URL normal detectada. Usando 'window.open' original.");
                return originalWindowOpen.call(window, url, target, features);
            };
            console.log("TAURI (preload v29): Patch de window.open concluído.");

        }); // Fim do DOMContentLoaded

    } else {
        console.log(`TAURI (preload v29): Hostname '${window.location.hostname}' ignorado.`);
    }
})();