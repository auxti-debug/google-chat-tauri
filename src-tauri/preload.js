/**
 * Google Chat -> Tauri notification bridge (v37).
 *
 * Google Chat can deliver desktop notifications through either:
 *   1. new Notification(...), or
 *   2. ServiceWorkerRegistration.showNotification(...).
 *
 * The second path became the primary one after Chat moved to chat.google.com.
 * WebView2 does not grant that persistent-notification path reliably, so both
 * browser APIs are bridged to the native Tauri command.
 */
(function () {
  "use strict";

  const VERSION = "37";
  const PATCH_MARKER = "__GC_TAURI_PRELOAD_VERSION__";
  const NOTIFICATION_MESSAGE = "GC_NATIVE_NOTIFICATION";
  const OPEN_URL_MESSAGE = "GC_OPEN_URL";
  const IDENTIFIED_DEDUPE_WINDOW_MS = 1500;
  const UNIDENTIFIED_DEDUPE_WINDOW_MS = 100;
  const ALLOWED_HOSTNAMES = new Set([
    "chat.google.com",
    "mail.google.com",
    "meet.google.com",
  ]);
  const ALLOWED_ORIGINS = new Set(
    Array.from(ALLOWED_HOSTNAMES, (hostname) => `https://${hostname}`),
  );

  if (window[PATCH_MARKER]) {
    return;
  }

  Object.defineProperty(window, PATCH_MARKER, {
    value: VERSION,
    configurable: false,
    enumerable: false,
    writable: false,
  });

  if (!ALLOWED_ORIGINS.has(window.location.origin)) {
    return;
  }

  console.info(
    `TAURI :: Notification bridge v${VERSION} loaded for ${window.location.hostname}.`,
  );

  const toText = (value) => {
    if (value === undefined || value === null) {
      return "";
    }

    try {
      return String(value);
    } catch (_) {
      return "";
    }
  };

  const readOption = (options, key) => {
    try {
      return options && options[key] !== undefined ? options[key] : undefined;
    } catch (_) {
      return undefined;
    }
  };

  const fingerprintData = (value) => {
    if (value === undefined || value === null) {
      return "";
    }

    let serialized;
    try {
      serialized = JSON.stringify(value);
    } catch (_) {
      return "";
    }

    if (!serialized) {
      return "";
    }

    // FNV-1a keeps message metadata out of the bridge while retaining a stable
    // identity. Google includes its message ID in notification options.data.
    let hash = 2166136261;
    for (let index = 0; index < serialized.length; index += 1) {
      hash ^= serialized.charCodeAt(index);
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0).toString(36);
  };

  const normalizeNotification = (title, options) => {
    const normalizedTitle = toText(title) || "Google Chat";
    const body = toText(readOption(options, "body"));
    const tag = toText(readOption(options, "tag"));
    const searchableTitle = normalizedTitle
      .toLocaleLowerCase()
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "");

    return {
      title: normalizedTitle,
      body,
      tag,
      dataIdentity: fingerprintData(readOption(options, "data")),
      isCall:
        searchableTitle.includes("chamada") ||
        searchableTitle.includes("ligacao") ||
        searchableTitle.includes("call"),
    };
  };

  const postToTop = (type, payload) => {
    try {
      const message = {
        type,
        bridgeVersion: VERSION,
        payload,
      };

      let topOrigin;
      try {
        topOrigin = window.top.location.origin;
      } catch (_) {
        // Cross-origin frames cannot inspect the top window's location.
      }

      if (ALLOWED_ORIGINS.has(topOrigin)) {
        window.top.postMessage(message, topOrigin);
        return;
      }

      // Only one call can match. Using the explicit allowlist avoids sending
      // notification contents to an unrelated embedding origin.
      for (const allowedOrigin of ALLOWED_ORIGINS) {
        window.top.postMessage(message, allowedOrigin);
      }
    } catch (error) {
      console.error(`TAURI :: Failed to post ${type} to the top window.`, error);
    }
  };

  let dispatchNativeNotification;

  if (window.self === window.top) {
    const recentNotifications = new Map();
    let unreadCount = 0;

    const getInvoke = () => window.__TAURI_INTERNALS__?.invoke;

    const clearUnreadCount = () => {
      unreadCount = 0;
      const invoke = getInvoke();
      if (typeof invoke === "function") {
        Promise.resolve(invoke("set_unread_count", { count: 0 })).catch(
          (error) => {
            console.error("TAURI :: Failed to clear the unread badge.", error);
          },
        );
      }
    };

    const reserveNotification = (payload) => {
      const now = Date.now();

      for (const [fingerprint, timestamp] of recentNotifications) {
        if (now - timestamp > IDENTIFIED_DEDUPE_WINDOW_MS) {
          recentNotifications.delete(fingerprint);
        }
      }

      const fingerprint = JSON.stringify([
        payload.title,
        payload.body,
        payload.tag,
        payload.dataIdentity,
      ]);
      const previousTimestamp = recentNotifications.get(fingerprint);
      const dedupeWindow =
        payload.tag || payload.dataIdentity
          ? IDENTIFIED_DEDUPE_WINDOW_MS
          : UNIDENTIFIED_DEDUPE_WINDOW_MS;
      const duplicate =
        previousTimestamp !== undefined &&
        now - previousTimestamp <= dedupeWindow;

      if (!duplicate) {
        recentNotifications.set(fingerprint, now);
      }

      return { duplicate, fingerprint, timestamp: now };
    };

    dispatchNativeNotification = (payload) => {
      const reservation = reserveNotification(payload);
      if (reservation.duplicate) {
        console.debug(
          "TAURI :: Duplicate Google Chat notification ignored.",
        );
        return Promise.resolve();
      }

      const invoke = getInvoke();
      if (typeof invoke !== "function") {
        recentNotifications.delete(reservation.fingerprint);
        console.error("TAURI :: Native invoke API is unavailable.");
        return Promise.resolve();
      }

      console.info("TAURI :: Forwarding Google Chat notification to Windows.");

      let nativeDelivery;
      try {
        nativeDelivery = invoke("show_windows_toast", {
          title: payload.title,
          body: payload.body,
          isCall: payload.isCall,
        });
      } catch (error) {
        recentNotifications.delete(reservation.fingerprint);
        console.error("TAURI :: Native notification delivery failed.", error);
        return Promise.resolve();
      }

      const delivery = Promise.resolve(nativeDelivery).catch((error) => {
        if (
          recentNotifications.get(reservation.fingerprint) ===
          reservation.timestamp
        ) {
          recentNotifications.delete(reservation.fingerprint);
        }
        console.error("TAURI :: Native notification delivery failed.", error);
      });

      const updateUnreadCount = async () => {
        let nativeWindowVisible = document.visibilityState === "visible";
        try {
          nativeWindowVisible = await invoke("is_main_window_visible");
        } catch (_) {
          // Fall back to the document state when native state is unavailable.
        }

        const documentFocused =
          typeof document.hasFocus !== "function" || document.hasFocus();
        const isForeground =
          nativeWindowVisible &&
          document.visibilityState === "visible" &&
          documentFocused;

        if (!isForeground) {
          unreadCount = Math.min(unreadCount + 1, 999);
          await invoke("set_unread_count", { count: unreadCount });
        }
      };

      updateUnreadCount().catch((error) => {
        console.error("TAURI :: Failed to update the unread badge.", error);
      });

      return delivery;
    };

    window.addEventListener("message", (event) => {
      if (!ALLOWED_ORIGINS.has(event.origin)) {
        return;
      }

      let sourceBelongsToWindow = event.source === window;
      if (!sourceBelongsToWindow && event.source) {
        try {
          sourceBelongsToWindow = event.source.top === window;
        } catch (_) {
          sourceBelongsToWindow = false;
        }
      }
      if (!sourceBelongsToWindow) {
        return;
      }

      const message = event.data;
      if (
        !message ||
        message.bridgeVersion !== VERSION ||
        typeof message.payload !== "object" ||
        message.payload === null
      ) {
        return;
      }

      if (message.type === NOTIFICATION_MESSAGE) {
        const payload = normalizeNotification(message.payload.title, {
          body: message.payload.body,
          tag: message.payload.tag,
        });
        payload.dataIdentity = toText(message.payload.dataIdentity);
        dispatchNativeNotification(payload);
        return;
      }

      if (message.type === OPEN_URL_MESSAGE) {
        const url = toText(message.payload.url);
        let parsedUrl;

        try {
          parsedUrl = new URL(url);
        } catch (_) {
          return;
        }

        if (
          parsedUrl.protocol === "https:" &&
          parsedUrl.hostname === "meet.google.com"
        ) {
          const invoke = getInvoke();
          if (typeof invoke === "function") {
            Promise.resolve(invoke("open_url", { url: parsedUrl.href })).catch(
              (error) => {
                console.error("TAURI :: Failed to open Google Meet.", error);
              },
            );
          }
        }
      }
    });

    window.addEventListener("focus", clearUnreadCount);
    document.addEventListener("visibilitychange", () => {
      if (document.visibilityState === "visible") {
        clearUnreadCount();
      }
    });
  }

  const emitNotification = (title, options) => {
    const payload = normalizeNotification(title, options);

    if (
      window.self === window.top &&
      typeof dispatchNativeNotification === "function"
    ) {
      return dispatchNativeNotification(payload);
    }

    postToTop(NOTIFICATION_MESSAGE, payload);
    return Promise.resolve();
  };

  const patchNotificationConstructor = () => {
    const OriginalNotification = window.Notification;
    if (
      typeof OriginalNotification !== "function" ||
      OriginalNotification.__gcTauriPatched
    ) {
      return;
    }

    function TauriNotification(title, options) {
      emitNotification(title, options);

      const notification = new EventTarget();
      try {
        Object.setPrototypeOf(notification, TauriNotification.prototype);
      } catch (_) {
        // EventTarget alone is still sufficient for Chat's listeners.
      }

      const normalized = normalizeNotification(title, options);
      for (const [key, value] of Object.entries({
        title: normalized.title,
        body: normalized.body,
        tag: normalized.tag,
        data: readOption(options, "data"),
      })) {
        try {
          Object.defineProperty(notification, key, {
            value,
            configurable: true,
            enumerable: true,
          });
        } catch (_) {
          // These compatibility properties are optional.
        }
      }

      Object.defineProperties(notification, {
        close: {
          value: () => {},
          configurable: true,
        },
        onclick: {
          value: null,
          configurable: true,
          writable: true,
        },
        onclose: {
          value: null,
          configurable: true,
          writable: true,
        },
        onerror: {
          value: null,
          configurable: true,
          writable: true,
        },
        onshow: {
          value: null,
          configurable: true,
          writable: true,
        },
      });

      return notification;
    }

    TauriNotification.prototype = Object.create(EventTarget.prototype, {
      constructor: {
        value: TauriNotification,
        configurable: true,
        writable: true,
      },
    });

    Object.defineProperties(TauriNotification, {
      permission: {
        get: () => "granted",
        configurable: true,
      },
      requestPermission: {
        value: () => Promise.resolve("granted"),
        configurable: true,
      },
      __gcTauriPatched: {
        value: true,
      },
    });

    if ("maxActions" in OriginalNotification) {
      try {
        Object.defineProperty(TauriNotification, "maxActions", {
          value: OriginalNotification.maxActions,
          configurable: true,
        });
      } catch (_) {
        // maxActions is optional and unused by Google Chat.
      }
    }

    try {
      Object.defineProperty(window, "Notification", {
        value: TauriNotification,
        configurable: true,
        writable: true,
      });
      console.info("TAURI :: Notification constructor patched.");
    } catch (error) {
      console.error("TAURI :: Failed to patch Notification.", error);
    }
  };

  const patchServiceWorkerNotifications = () => {
    const Registration = window.ServiceWorkerRegistration;
    const prototype = Registration?.prototype;
    const originalShowNotification = prototype?.showNotification;

    if (
      typeof originalShowNotification !== "function" ||
      originalShowNotification.__gcTauriPatched
    ) {
      return;
    }

    const showNotification = function (title, options) {
      return emitNotification(title, options);
    };
    Object.defineProperty(showNotification, "__gcTauriPatched", {
      value: true,
    });

    try {
      Object.defineProperty(prototype, "showNotification", {
        value: showNotification,
        configurable: true,
        enumerable: false,
        writable: true,
      });
      console.info(
        "TAURI :: ServiceWorkerRegistration.showNotification patched.",
      );
    } catch (error) {
      console.error(
        "TAURI :: Failed to patch service-worker notifications.",
        error,
      );
    }
  };

  const createGrantedPermissionStatus = () => {
    const status = new EventTarget();

    try {
      if (window.PermissionStatus?.prototype) {
        Object.setPrototypeOf(status, window.PermissionStatus.prototype);
      }
    } catch (_) {
      // The EventTarget fallback implements the API Chat currently consumes.
    }

    Object.defineProperties(status, {
      state: {
        get: () => "granted",
        configurable: true,
      },
      onchange: {
        value: null,
        configurable: true,
        writable: true,
      },
    });

    return status;
  };

  const patchNotificationPermissionQuery = () => {
    const permissions = navigator.permissions;
    const originalQuery = permissions?.query;

    if (
      typeof originalQuery !== "function" ||
      originalQuery.__gcTauriPatched
    ) {
      return;
    }

    const notificationStatus = createGrantedPermissionStatus();
    const query = function (descriptor) {
      if (descriptor?.name === "notifications") {
        return Promise.resolve(notificationStatus);
      }

      return Reflect.apply(originalQuery, permissions, [descriptor]);
    };
    Object.defineProperty(query, "__gcTauriPatched", { value: true });

    try {
      Object.defineProperty(permissions, "query", {
        value: query,
        configurable: true,
        writable: true,
      });
      console.info("TAURI :: Notification permission query patched.");
    } catch (error) {
      console.error(
        "TAURI :: Failed to patch the notification permission query.",
        error,
      );
    }
  };

  const patchWindowOpen = () => {
    const originalWindowOpen = window.open;
    if (
      typeof originalWindowOpen !== "function" ||
      originalWindowOpen.__gcTauriPatched
    ) {
      return;
    }

    const patchedWindowOpen = function (url, target, features) {
      const urlText = toText(url);
      let parsedUrl;

      try {
        parsedUrl = new URL(urlText, window.location.href);
      } catch (_) {
        parsedUrl = null;
      }

      if (
        parsedUrl?.protocol === "https:" &&
        parsedUrl.hostname === "meet.google.com"
      ) {
        postToTop(OPEN_URL_MESSAGE, { url: parsedUrl.href });
        return null;
      }

      return Reflect.apply(originalWindowOpen, window, [
        url,
        target,
        features,
      ]);
    };
    Object.defineProperty(patchedWindowOpen, "__gcTauriPatched", {
      value: true,
    });

    try {
      Object.defineProperty(window, "open", {
        value: patchedWindowOpen,
        configurable: true,
        writable: true,
      });
    } catch (error) {
      console.error("TAURI :: Failed to patch window.open.", error);
    }
  };

  // Initialization scripts run before page scripts. Apply every bridge here so
  // Google Chat cannot capture an unpatched API during its own bootstrap.
  patchNotificationConstructor();
  patchServiceWorkerNotifications();
  patchNotificationPermissionQuery();
  patchWindowOpen();

  // Keep a defensive retry for WebViews where an interface appears later.
  document.addEventListener(
    "DOMContentLoaded",
    () => {
      patchNotificationConstructor();
      patchServiceWorkerNotifications();
      patchNotificationPermissionQuery();
      patchWindowOpen();
    },
    { once: true },
  );
})();
