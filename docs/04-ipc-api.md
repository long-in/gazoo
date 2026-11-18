# 04. IPC API(Tauri コマンド / イベント)

フロントエンドと Rust 間のインターフェース定義。**この資料が型の正**であり、フロントの `src/lib/tauri/` は本定義に従う。
命名: コマンドは `snake_case`、イベントは `domain://name`。エラーはすべて `AppError { code, message, details? }`。

## 共通型

```ts
type JobStatus = "queued" | "running" | "completed" | "failed" | "cancelled";
type ModelKind = "sd15" | "sdxl" | "flux" | "flux_gguf";

interface GenerationParams { /* 03-data-model.md の params_json と同一 */ }

interface Job {
  id: string; status: JobStatus; position: number;
  params: GenerationParams; error?: { code: string; message: string };
  imageIds: string[]; createdAt: number; startedAt?: number; finishedAt?: number;
}

interface ImageItem {
  id: string; jobId: string; width: number; height: number; seed: number;
  isFavorite: boolean; createdAt: number;
  filePath: string;   // 絶対パス。フロントが convertFileSrc で URL に変換する
  thumbPath: string;  // 同上(サムネイル。生成できなかった場合は filePath と同一)
}

interface Page<T> { items: T[]; nextCursor?: string }
```

## コマンド

### エンジン

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `engine_status` | – | `EngineStatus`(下記) | 現在の接続状態(即時にヘルスチェックを実行して返す) |
| `engine_list_models` | – | `EngineModels`(下記) | `/object_info/<Node>` から取得。未接続時は `ENGINE_UNREACHABLE` |
| `engine_reconnect` | – | `EngineStatus` | 即時に再接続を試み、結果を `engine://status` でも通知 |

```ts
interface EngineStatus {
  connected: boolean;
  url: string;
  version?: string;        // ComfyUI のバージョン
  device?: string;         // 先頭デバイス名(例: "mps", "NVIDIA GeForce RTX ...")
  vramFree?: number;       // bytes
  features: { ggufNode: boolean };   // ComfyUI-GGUF(UnetLoaderGGUF)の有無
  error?: "unreachable" | "bad_response";  // connected=false のときの理由
}

interface EngineModels {
  checkpoints: string[];   // CheckpointLoaderSimple.ckpt_name
  unets: string[];         // UNETLoader.unet_name
  ggufUnets: string[];     // UnetLoaderGGUF.unet_name(ノード未導入なら空)
  clips: string[];         // CLIPLoader.clip_name
  vaes: string[];          // VAELoader.vae_name
  loras: string[];         // LoraLoader.lora_name
  samplers: string[];      // KSampler.sampler_name
  schedulers: string[];    // KSampler.scheduler
}
```

接続状態の監視: Rust 側が 5 秒間隔でヘルスチェックを行い、状態が変化したときだけ `engine://status` を発火する。ComfyUI の URL が設定で変更されたら即座に再接続する。
`/object_info` は全ノード分だと巨大なため、必要なノードのみ `/object_info/<NodeClass>` で個別取得する。
ComfyUI のバージョンにより選択肢の形式が異なる(`[[...], {}]` と `["COMBO", {"options": [...]}]`)ため、両方を受け付ける。

### ジョブ

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `job_enqueue` | `{ params: GenerationParams; count?: number }` | `Job[]` | キューへ追加。`count`(既定 1、最大 20)回分を個別ジョブとして追加する。`seedRandomize` が true なら各ジョブごとにランダムなシードを確定して保存、false ならシードを `seed, seed+1, …` とする |
| `job_list` | `{ statuses?: JobStatus[]; cursor?: string; limit?: number }` | `Page<Job>` | 一覧 |
| `job_cancel` | `{ id }` | `void` | queued は削除、running は ComfyUI へ interrupt |
| `job_reorder` | `{ id; newPosition }` | `void` | queued のみ。`newPosition` は待機中ジョブ内の 0 始まりの位置 |
| `job_retry` | `{ id }` | `Job` | failed / cancelled を同じパラメータで複製して再投入 |
| `job_clear_finished` | – | `number` | 完了・失敗・キャンセル済みジョブの履歴クリア。画像を持たないジョブは削除、画像を持つジョブはキュー画面から隠す(画像・パラメータは残る。03 参照)。戻り値は対象件数 |

### ギャラリー

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `image_list` | `{ query?: string; favoritesOnly?: boolean; cursor?: string; limit?: number }` | `Page<ImageItem>` | 新しい順。`query` はプロンプトの部分一致。`limit` 既定 60・最大 200。`nextCursor` は不透明な文字列(キーセット方式。03 参照) |
| `image_get` | `{ id }` | `{ image: ImageItem; job: Job }` | 詳細(パラメータ含む) |
| `image_set_favorite` | `{ id; value: boolean }` | `void` | |
| `image_delete` | `{ ids: string[]; deleteFiles: boolean }` | `void` | DB の記録を削除する。`deleteFiles` が true なら原寸とサムネイルを **OS のゴミ箱へ移動**(Q-10)。ファイルが既に無くてもエラーにしない。完了後に `image://deleted` を発火 |
| `image_export` | `{ id; destPath }` | `void` | 原寸をコピーする。`destPath` は保存ダイアログで選んだ絶対パス |
| `image_reveal` | `{ id }` | `void` | エクスプローラー / Finder で表示 |

### プリセット

| コマンド | 引数 | 戻り値 |
|---|---|---|
| `preset_list` | – | `Preset[]`(名前の昇順) |
| `preset_save` | `{ id?; name; params }` | `Preset` |
| `preset_delete` | `{ id }` | `void` |

```ts
interface Preset { id: string; name: string; params: GenerationParams; createdAt: number; updatedAt: number }
```

- `name` は前後の空白を除いて 1〜60 文字。**同じ名前が既にあり、かつ `id` が別(または未指定)の場合は `VALIDATION`(`details.field = "name"`)**。上書きしたいときは、フロントが既存プリセットの `id` を指定して保存する(確認ダイアログはフロントの責務)。
- `params` は `GenerationParams` として解釈できればよい(プロンプトが空でも保存できる。生成時の検証は `job_enqueue` で行う)。
- 存在しない `id` の更新・削除は `PRESET_NOT_FOUND`。

### 設定

| コマンド | 引数 | 戻り値 |
|---|---|---|
| `settings_get` | – | `Settings` |
| `settings_update` | `{ patch: SettingsPatch }` | `Settings` |

```ts
interface Settings {
  comfyuiUrl: string;            // 既定 http://127.0.0.1:8188(http/https のみ)
  imagesDir: string | null;      // null = {app_data}/images(絶対パスのみ)
  theme: "light" | "dark" | "system";
  language: "ja" | "en" | null;  // null = OS 追従
  generationDefaults: GenerationParams | null;
}
// 未指定キーは変更しない。imagesDir / language に null を渡すと既定に戻す。
// 検証エラー時は一切書き込まれず VALIDATION(details.field)を返す。
type SettingsPatch = Partial<Settings>;
```

`generationDefaults`(生成フォームの下書き。03 の `generation.default_params`)は `GenerationParams | null` で、フォームの入力内容を自動保存・復元するために使う(null = 組み込みの既定値)。Rust 側は JSON としてそのまま保存し、`job_enqueue` の時点で検証する。
**`settings_update` は `imagesDir` を受け付けない**(渡すと `VALIDATION`)。保存先の変更は、既存画像の扱いを伴うため専用コマンドで行う(下記)。

### 保存先の変更(Q-11)

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `storage_change_images_dir` | `{ newDir: string \| null; moveExisting: boolean }` | `{ moved: number; missing: number }` | 画像の保存先を変更する。`newDir = null` は既定(`{app_data}/images`)に戻す |

- `moveExisting = true`: 既存画像(原寸・サムネイル)を新しい保存先へ **コピー → 検証 → 設定を切り替え → 旧ファイルを OS のゴミ箱へ移動** の順で行う。コピーに失敗したら、作ったコピーを削除して中止し、設定は変えない。`moved` はコピーしたファイル数、`missing` は元ファイルが無くスキップした数。
- `moveExisting = false`: 設定だけを切り替える。**既存画像のファイルは旧フォルダに残るが、DB には保存先からの相対パスしか無いため、保存先を元に戻すまで表示できなくなる**(UI の確認ダイアログで明示する)。
- 次の場合は実行しない: 新しい保存先が相対パス / 現在の保存先と入れ子(内側・外側)/ 書き込めない(`VALIDATION`、`details.field = "newDir"`)、生成ジョブが実行中(`JOB_INVALID_STATE`)。現在の保存先と同じなら何もしない(`moved = 0`)。
- 成功時、新しい保存先を asset プロトコルのスコープに追加する。
- 大量の画像では時間がかかる(進捗イベントは未提供。UI は処理中表示のみ)。

## イベント(Rust → フロント)

| イベント | ペイロード | 発火タイミング |
|---|---|---|
| `engine://status` | `engine_status` の戻り値 | 接続状態の変化 |
| `job://updated` | `Job` | ステータス変更・追加・並び替え |
| `job://progress` | `{ jobId; step; totalSteps; node?: string }` | 生成中(最大 10 回/秒) |
| `job://preview` | `{ jobId; dataUrl: string }` | (M2 では未実装。将来) |
| `image://created` | `ImageItem` | 画像が保存された時 |
| `image://deleted` | `{ ids: string[] }` | 削除時 |

## エラーコード(初期)

| code | 意味 | UI の対応 |
|---|---|---|
| `ENGINE_UNREACHABLE` | ComfyUI に接続できない | 設定へのリンクと再接続ボタン |
| `ENGINE_BAD_RESPONSE` | 接続先が ComfyUI として期待する応答を返さない(別サービス / 非互換バージョン) | 設定の URL 確認を促す |
| `ENGINE_MODEL_MISSING` | 指定モデルが存在しない(`/prompt` が `value_not_in_list` を返した) | モデル再選択を促す |
| `ENGINE_NODE_MISSING` | 必要なカスタムノードが無い(例: GGUF) | 導入手順を表示 |
| `ENGINE_EXECUTION_FAILED` | 生成失敗(OOM 等)。`details` に原文 | 解像度縮小などを提案 |
| `PRESET_NOT_FOUND` | 存在しないプリセット | トースト |
| `JOB_NOT_FOUND` / `JOB_INVALID_STATE` | 不正なジョブ操作(存在しない ID、状態が合わない操作) | トースト |
| `IMAGE_NOT_FOUND` | 画像/ファイル欠損 | トースト、一覧から除外提案 |
| `VALIDATION` | 入力不正。`details.field` | フォームにインライン表示 |
| `STORAGE_ERROR` | ファイル I/O 失敗 | トースト |
| `DB_ERROR` | DB 失敗 | 再起動案内 |
| `INTERNAL` | 想定外 | 詳細ログ参照案内 |
