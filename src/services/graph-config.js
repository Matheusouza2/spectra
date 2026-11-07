export const GRAPH_CONFIG_DEFAULTS = {
  maxNodes: 1_000_000,
  maxEdges: 5_000_000,
  memoryLimitMb: 1_024,
};

export const GRAPH_CONFIG_LIMITS = {
  maxNodes: { min: 1, max: 10_000_000 },
  maxEdges: { min: 1, max: 50_000_000 },
  memoryLimitMb: { min: 64, max: 8_192 },
};

const STORAGE_KEY = 'racon.graph-generation-config.v1';

export function loadGraphGenerationConfig() {
  try {
    const parsed = JSON.parse(localStorage.getItem(STORAGE_KEY) || 'null');
    if (!parsed || typeof parsed !== 'object') return { ...GRAPH_CONFIG_DEFAULTS };
    return Object.fromEntries(Object.entries(GRAPH_CONFIG_DEFAULTS).map(([key, fallback]) => {
      const value = Number(parsed[key]);
      const { min, max } = GRAPH_CONFIG_LIMITS[key];
      return [key, Number.isSafeInteger(value) && value >= min && value <= max ? value : fallback];
    }));
  } catch {
    return { ...GRAPH_CONFIG_DEFAULTS };
  }
}

export function saveGraphGenerationConfig(config) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(config));
  } catch {
    // The settings still apply to this request if local storage is unavailable.
  }
}

// Returns null when valid, otherwise { key, min, max } describing the violated limit.
export function validateGraphGenerationConfig(config) {
  for (const [key, { min, max }] of Object.entries(GRAPH_CONFIG_LIMITS)) {
    const value = Number(config[key]);
    if (!Number.isSafeInteger(value) || value < min || value > max) {
      return { key, min, max };
    }
  }
  return null;
}
