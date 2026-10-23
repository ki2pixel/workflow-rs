import { getNotificationsArea } from './domElements.js';

export function formatElapsedTime(startTime) {
    if (!startTime) return "";
    const now = new Date();
    let seconds = Math.floor((now - startTime) / 1000);
    let minutes = Math.floor(seconds / 60);
    let hours = Math.floor(minutes / 60);
    seconds %= 60;
    minutes %= 60;
    let timeStr = "";
    if (hours > 0) timeStr += `${hours}h `;
    if (minutes > 0 || hours > 0) timeStr += `${minutes}m `;
    timeStr += `${seconds}s`;
    return timeStr;
}

export function showNotification(message, type = 'info') { // type can be 'info', 'success', 'error', 'warning'
    const notificationsArea = getNotificationsArea();
    if (!notificationsArea) return;
    const notif = document.createElement('div');
    notif.className = `notification ${type}`;
    notif.textContent = message;
    notificationsArea.appendChild(notif);
    setTimeout(() => {
        notif.remove();
    }, 5000);
}

// Web Notifications API helpers
export async function ensureBrowserNotificationsPermission() {
    if (!('Notification' in window)) {
        console.warn('[Notifications] Browser does not support Notification API');
        return false;
    }
    if (Notification.permission === 'granted') return true;
    if (Notification.permission === 'denied') return false;
    try {
        const perm = await Notification.requestPermission();
        return perm === 'granted';
    } catch (e) {
        console.warn('[Notifications] Permission request failed:', e);
        return false;
    }
}

export async function sendBrowserNotification(title, body, options = {}) {
    try {
        const ok = await ensureBrowserNotificationsPermission();
        if (!ok) return false;
        const notif = new Notification(title || 'Notification', { body: body || '', ...options });
        setTimeout(() => notif && notif.close && notif.close(), 8000);
        return true;
    } catch (e) {
        console.debug('[Notifications] Fallback to UI banner due to error:', e);
        showNotification(`${title || 'Notification'}: ${body || ''}`, 'info');
        return false;
    }
}