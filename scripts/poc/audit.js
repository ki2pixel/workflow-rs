// Audit A-Strict du PoC Tauri.
// Ne renvoie jamais le worker-token brut : uniquement son digest SHA-256.
(async () => {
  if (document.readyState !== 'complete') {
    await new Promise((resolve) => {
      if (document.readyState === 'complete') return resolve();
      window.addEventListener('load', resolve, { once: true });
      setTimeout(resolve, 5000);
    });
  }

  const tokenEl = document.querySelector('meta[name="worker-token"]');
  const token = tokenEl ? (tokenEl.getAttribute('content') || '') : '';

  let tokenSha = '';
  let tokenError = null;
  try {
    const digest = await crypto.subtle.digest(
      'SHA-256',
      new TextEncoder().encode(token)
    );
    tokenSha = [...new Uint8Array(digest)]
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('');
  } catch (error) {
    tokenError = String(error);
  }

  const describeError = (error) =>
    error && error.message ? error.message : String(error);

  async function probe(fn) {
    try {
      await fn();
      return 'resolved';
    } catch (error) {
      return 'rejected:' + describeError(error);
    }
  }

  const internals = window.__TAURI_INTERNALS__;
  const invoke =
    internals && typeof internals.invoke === 'function'
      ? await probe(() => internals.invoke('__audit__'))
      : 'absent';
  const postMessageCall =
    internals && typeof internals.postMessage === 'function'
      ? await probe(() => internals.postMessage('__audit__', {}))
      : 'absent';
  const rawIpcCall =
    window.ipc && typeof window.ipc.postMessage === 'function'
      ? await probe(() => window.ipc.postMessage('__audit__', {}))
      : 'absent';

  const nativeHandler = window.webkit?.messageHandlers?.ipc;
  const nativeIpcCall =
    nativeHandler && typeof nativeHandler.postMessage === 'function'
      ? await probe(() => nativeHandler.postMessage('__audit__'))
      : 'absent';

  const result = {
    token_sha256: tokenSha,
    token_error: tokenError,
    tauri: typeof window.__TAURI__,
    internals: typeof internals,
    invoke,
    postMessage: internals ? typeof internals.postMessage : 'absent',
    postMessage_call: postMessageCall,
    rawIpc: window.ipc ? typeof window.ipc.postMessage : 'absent',
    rawIpc_call: rawIpcCall,
    nativeIpc: nativeHandler ? typeof nativeHandler.postMessage : 'absent',
    nativeIpc_call: nativeIpcCall,
    isTauri: typeof window.isTauri,
    href: location.href,
    title: document.title,
    readyState: document.readyState,
    userAgent: navigator.userAgent,
  };

  location.hash = 'poc=' + encodeURIComponent(JSON.stringify(result));
})();
