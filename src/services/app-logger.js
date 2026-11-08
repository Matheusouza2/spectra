import { invoke } from '@tauri-apps/api/core';
import { notifyTaskFinished } from './desktop-shell';

const write = (entry) => invoke('write_log_entry', { entry }).catch((error) => {
  console.error('[RACoN logger] Não foi possível gravar o log:', error);
});

export const logEvent = (level, source, event, message) => write({
  timestamp: String(Math.floor(Date.now() / 1000)),
  level,
  source,
  event,
  message: String(message ?? ''),
});

export const invokeLogged = async (command, args = {}) => {
  const startedAt = Date.now();
  await logEvent('INFO', 'ipc', `${command}.started`, 'Comando iniciado');
  try {
    const result = await invoke(command, args);
    await logEvent('INFO', 'ipc', `${command}.completed`, `Concluído em ${Date.now() - startedAt} ms`);
    notifyTaskFinished(command, Date.now() - startedAt, false);
    return result;
  } catch (error) {
    await logEvent('ERROR', 'ipc', `${command}.failed`, error?.message ?? error);
    notifyTaskFinished(command, Date.now() - startedAt, true);
    throw error;
  }
};

export const readLogEntries = () => invoke('read_log_entries');
