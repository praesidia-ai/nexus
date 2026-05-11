/**
 * Control mode state store.
 * Manages the operational mode of the Nexus system.
 */

export type ControlMode = "autonomous" | "supervised" | "manual" | "locked";

export interface CircuitBreaker {
  id: string;
  name: string;
  description: string;
  triggered: boolean;
  triggeredAt?: string;
}

interface ControlState {
  mode: ControlMode;
  circuitBreakers: CircuitBreaker[];
  capabilities: Record<string, string>;
}

type Listener = () => void;

let state: ControlState = {
  mode: "supervised",
  circuitBreakers: [],
  capabilities: {},
};

const listeners = new Set<Listener>();

function notify() {
  listeners.forEach((fn) => fn());
}

export const controlStore = {
  getState(): ControlState {
    return state;
  },

  getMode(): ControlMode {
    return state.mode;
  },

  setMode(mode: ControlMode) {
    state = { ...state, mode };
    notify();
  },

  setCircuitBreakers(breakers: CircuitBreaker[]) {
    state = { ...state, circuitBreakers: breakers };
    notify();
  },

  setCapabilities(capabilities: Record<string, string>) {
    state = { ...state, capabilities };
    notify();
  },

  /** Update state from API response */
  updateFromApi(apiState: { mode: string; limits: unknown; capabilities: Record<string, string> }) {
    const modeMap: Record<string, ControlMode> = {
      safe: "manual",
      assisted: "supervised",
      autonomous: "autonomous",
    };
    state = {
      mode: modeMap[apiState.mode] ?? "supervised",
      circuitBreakers: state.circuitBreakers,
      capabilities: apiState.capabilities,
    };
    notify();
  },

  subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  },

  getSnapshot(): ControlState {
    return state;
  },
};
