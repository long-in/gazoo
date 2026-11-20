/** 画面の状態(パネルの開閉・幅・表示の密度。docs/29 §6)。`settings.uiState` に JSON で保存する。 */

export type Density = "compact" | "comfortable";

export interface UiState {
  density: Density;
  /** パネル id → 開いているか。 */
  panels: Record<string, boolean>;
  /** ペイン id → 幅(px)。 */
  widths: Record<string, number>;
}

export const defaultUiState: UiState = { density: "compact", panels: {}, widths: {} };

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);

/** 保存された値を検証して取り込む。壊れた項目は既定に戻す(起動不能にしない)。 */
export function parseUiState(raw: unknown): UiState {
  if (!isRecord(raw)) return defaultUiState;
  const panels: Record<string, boolean> = {};
  if (isRecord(raw.panels)) {
    for (const [k, v] of Object.entries(raw.panels)) if (typeof v === "boolean") panels[k] = v;
  }
  const widths: Record<string, number> = {};
  if (isRecord(raw.widths)) {
    for (const [k, v] of Object.entries(raw.widths)) if (typeof v === "number" && Number.isFinite(v) && v > 0) widths[k] = v;
  }
  return { density: raw.density === "comfortable" ? "comfortable" : "compact", panels, widths };
}

/** 幅を最小・最大に収める。 */
export function clampWidth(width: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, Math.round(width)));
}
