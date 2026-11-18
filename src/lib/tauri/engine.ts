import { invoke } from "./invoke";

export interface EngineStatus {
  connected: boolean;
  url: string;
  version?: string;
  device?: string;
  /** bytes */
  vramFree?: number;
  features: { ggufNode: boolean };
  error?: "unreachable" | "bad_response";
}

export interface EngineModels {
  checkpoints: string[];
  unets: string[];
  ggufUnets: string[];
  clips: string[];
  vaes: string[];
  loras: string[];
  samplers: string[];
  schedulers: string[];
}

export const engineStatus = () => invoke<EngineStatus>("engine_status");
export const engineListModels = () => invoke<EngineModels>("engine_list_models");
export const engineReconnect = () => invoke<EngineStatus>("engine_reconnect");
