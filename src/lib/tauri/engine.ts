import { invoke } from "./invoke";

export interface EngineStatus {
  connected: boolean;
  url: string;
  version?: string;
  device?: string;
  /** bytes */
  vramFree?: number;
  features: {
    ggufNode: boolean;
    /** 顔補正に必要なノード(Impact Pack の FaceDetailer と Subpack の検出モデルローダー)。 */
    faceDetailer: boolean;
    /** 利用できる ControlNet の前処理。Canny はコア、Depth / OpenPose は comfyui_controlnet_aux が必要。 */
    controlnetPreprocessors: ("canny" | "depth" | "openpose")[];
    /** Inpaint に必要なコアノードがそろっているか。 */
    inpaint: boolean;
    /** 画像から生成(img2img)に必要なコアノードがそろっているか。 */
    img2img: boolean;
    /** 参照画像つき生成(Qwen-Image 2.1 の画像編集)に使えるエンコーダの定義があるか(docs/24 §4.2)。 */
    referenceEdit: boolean;
  };
  error?: "unreachable" | "bad_response";
}

export interface EngineModels {
  checkpoints: string[];
  unets: string[];
  ggufUnets: string[];
  clips: string[];
  vaes: string[];
  loras: string[];
  /** 顔補正の検出モデル("bbox/…")。ノード未導入なら空。 */
  faceDetectors: string[];
  /** ControlNet モデル(`ControlNetLoader`)。 */
  controlnets: string[];
  samplers: string[];
  schedulers: string[];
}

export const engineStatus = () => invoke<EngineStatus>("engine_status");
export const engineListModels = () => invoke<EngineModels>("engine_list_models");
export const engineReconnect = () => invoke<EngineStatus>("engine_reconnect");
