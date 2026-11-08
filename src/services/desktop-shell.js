// Desktop-shell integrations: system tray labels and background-completion notifications.
// Everything here is a no-op outside the Tauri webview (browser-only dev).
import { translate } from '../i18n/I18nContext';
import { DEFAULT_LANGUAGE } from '../locales';

// Notify only for runs at least this long, and only while the window is not in focus.
const NOTIFY_MIN_MS = 5000;

const inTauri = () => typeof window !== 'undefined' && Boolean(window.__TAURI_INTERNALS__);

const currentLanguage = () => {
  try { return localStorage.getItem('racon-language') || DEFAULT_LANGUAGE; } catch { return DEFAULT_LANGUAGE; }
};

export async function syncTrayLabels(language) {
  if (!inTauri()) return;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('set_tray_labels', {
      tooltip: translate(language, 'tray.tooltip'),
      show: translate(language, 'tray.show'),
      quit: translate(language, 'tray.quit'),
    });
  } catch (error) {
    console.warn('[SPECTRA] could not update the tray labels:', error);
  }
}

const windowInBackground = async () => {
  if (typeof document !== 'undefined' && document.hasFocus()) return false;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    return !(await getCurrentWindow().isFocused());
  } catch {
    return true;
  }
};

export async function notifyTaskFinished(command, elapsedMs, failed) {
  if (!inTauri() || elapsedMs < NOTIFY_MIN_MS) return;
  const language = currentLanguage();
  const taskKey = `tray.tasks.${command}`;
  const label = translate(language, taskKey);
  if (label === taskKey) return; // only announce user-visible analysis tasks
  try {
    if (!(await windowInBackground())) return;
    const { isPermissionGranted, requestPermission, sendNotification } = await import('@tauri-apps/plugin-notification');
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === 'granted';
    if (!granted) return;
    const params = { task: label, seconds: (elapsedMs / 1000).toFixed(1) };
    sendNotification({
      title: translate(language, failed ? 'tray.failedTitle' : 'tray.doneTitle'),
      body: translate(language, failed ? 'tray.failedBody' : 'tray.doneBody', params),
    });
  } catch (error) {
    console.warn('[SPECTRA] could not send the notification:', error);
  }
}
