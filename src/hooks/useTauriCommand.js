import { useState } from 'react';

/**
 * Custom hook for managing Tauri IPC command execution.
 * Handles loading states and error management.
 */
export const useTauriCommand = (commandFn) => {
  const [data, setData] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  
  const execute = async (...args) => {
    setLoading(true);
    setError(null);
    try {
      const result = await commandFn(...args);
      setData(result);
      return result;
    } catch (err) {
      console.error(`[useTauriCommand] Error executing ${commandFn.name}:`, err);
      setError(err.message || err);
      throw err;
    } finally {
      setLoading(false);
    }
  };
  
  return { data, loading, error, execute };
};
