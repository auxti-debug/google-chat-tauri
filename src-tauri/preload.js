/**
 * VERSÃO 35 (Plano Semaphore-Lock): Usa um semáforo temporizado para evitar todas as notificações duplicadas.
 * - Um único sinalizador `notificationLock` é gerenciado no contexto da janela principal.
 * - Quando uma notificação é criada, o bloqueio é ativado por 2 segundos, ignorando todas as outras tentativas.
 * - O patch de Notification envia uma mensagem para a janela pai (`window.parent.postMessage`).
 * - Um ouvinte de 'message' na janela principal captura a mensagem e chama o `invoke` com segurança.
 * Mantém o patch 'window.open'.
 */
(function() { // Mantém a IIFE
    console.log("TAURI :: Preload script (v35) carregado.");

    const allowedHostnames = ['chat.google.com', 'mail.google.com', 'meet.google.com'];

    if (allowedHostnames.includes(window.location.hostname)) {
        console.log(`TAURI (preload v35): Hostname '${window.location.hostname}' detectado. Aguardando DOM...`);

        window.addEventListener('DOMContentLoaded', () => {
            console.log("TAURI (preload v35): DOM carregado. Aplicando patches...");

            // Verifica se estamos no contexto principal (top-level) para adicionar o ouvinte.
            // O patch de Notification será aplicado em todos os contextos (principal e iframes).
            if (window.self === window.top) {
                console.log("TAURI (preload v35): Contexto principal detectado. Configurando ouvinte de postMessage.");
                
                // Semáforo para controlar o fluxo de notificações. Apenas o contexto principal precisa dele.
                let notificationLock = false;

                const { invoke } = window.__TAURI__.tauri;

                // --- PASSO 1: Ouvinte de Mensagem (no contexto principal) ---
                window.addEventListener('message', (event) => {
                    // Filtra mensagens para garantir que são do nosso patch
                    if (!event.data || event.data.type !== 'GC_NATIVE_NOTIFICATION') {
                        return;
                    }

                    // Se o bloqueio estiver ativo, ignora a mensagem.
                    if (notificationLock) {
                        console.log(`TAURI (preload v35): Bloqueio ativo. Mensagem de notificação ignorada.`);
                        return;
                    }

                    // Ativa o bloqueio e agenda o desbloqueio.
                    notificationLock = true;
                    setTimeout(() => {
                        console.log("TAURI (preload v35): Bloqueio de notificação desativado.");
                        notificationLock = false;
                    }, 2000); // Bloqueio de 2 segundos
                    
                    const { title, options, isCall } = event.data.payload;
                    console.log(`TAURI (preload v35): Mensagem recebida. Chamando 'show_windows_toast' para "${title}".`);
                    invoke('show_windows_toast', { title: title, body: options.body || '', isCall: isCall });
                });
            }

            // --- PASSO 2: Patch do `new Notification` (executa em todos os contextos) ---
            window.Notification = function(title, options) {
                console.log(`TAURI (preload v35): Interceptado (Notification): "${title}". Enviando postMessage.`);
                const isCall = title.includes("chamada") || title.includes("call");
                
                // Envia a mensagem para a janela principal para ser processada.
                window.parent.postMessage({ type: 'GC_NATIVE_NOTIFICATION', payload: { title, options, isCall: isCall } }, '*');

                // Retorna um objeto falso para satisfazer o JS do Google Chat.
                const fakeNotification = new EventTarget();
                fakeNotification.close = () => { /* O método close agora não precisa fazer nada. */ };

                return fakeNotification;
            };
            
            window.Notification.requestPermission = async function() { return 'granted'; };
            window.Notification.permission = 'granted';
            
            console.log("TAURI (preload v35): Patch de notificação (com semáforo) concluído.");

            // --- PATCH 2: window.open (para chamadas - SEM MUDANÇAS) ---
            const originalWindowOpen = window.open;
            window.open = function(url, target, features) {
                console.log(`TAURI (preload v35): Interceptado (window.open): "${url}"`);

                if (url && (url.includes('meet.google.com') || url.includes('/call/'))) {
                    console.log("TAURI (preload v35): URL de chamada detectada. Chamando 'open_url'.");
                    if (window.self === window.top && window.__TAURI__ && window.__TAURI__.tauri) {
                        window.__TAURI__.tauri.invoke('open_url', { url: url });
                    } else {
                        console.warn("TAURI (preload v35): 'invoke' não disponível neste contexto para window.open. A URL pode não abrir.");
                    }
                    return null;
                }

                console.log("TAURI (preload v35): URL normal detectada. Usando 'window.open' original.");
                return originalWindowOpen.call(window, url, target, features);
            };
            console.log("TAURI (preload v35): Patch de window.open concluído.");

        }); // Fim do DOMContentLoaded

    } else {
        console.log(`TAURI (preload v35): Hostname '${window.location.hostname}' ignorado.`);
    }
})();