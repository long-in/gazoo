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
  features: {
    ggufNode: boolean;               // ComfyUI-GGUF(UnetLoaderGGUF)の有無
    faceDetailer: boolean;           // Impact Pack(FaceDetailer)と Subpack(UltralyticsDetectorProvider)の有無(v0.2)
    controlnetPreprocessors: ("canny" | "depth" | "openpose")[];  // 利用できる前処理。canny はコアの Canny ノード(v0.2)
  };
  error?: "unreachable" | "bad_response";  // connected=false のときの理由
}

interface EngineModels {
  checkpoints: string[];   // CheckpointLoaderSimple.ckpt_name
  unets: string[];         // UNETLoader.unet_name
  ggufUnets: string[];     // UnetLoaderGGUF.unet_name(ノード未導入なら空)
  clips: string[];         // CLIPLoader.clip_name
  vaes: string[];          // VAELoader.vae_name
  loras: string[];         // LoraLoader.lora_name
  controlnets: string[];   // ControlNetLoader.control_net_name(v0.2)
  faceDetectors: string[]; // UltralyticsDetectorProvider.model_name のうち "bbox/" で始まるもの(v0.2。ノード未導入なら空)
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

### 入力画像(v0.2、ControlNet の参照画像。設計: 10 §6)

| コマンド | 引数 | 戻り値 | 説明 |
|---|---|---|---|
| `input_image_pick` | – | `InputImage \| null` | **Rust 側でファイル選択ダイアログを開き**、選ばれた PNG / JPEG を取り込む。キャンセル時は `null` |
| `input_image_from_gallery` | `{ imageId: string }` | `InputImage` | ギャラリーの画像を参照画像として取り込む(`IMAGE_NOT_FOUND` あり) |
| `input_image_get` | `{ path: string }` | `InputImage` | 既存の参照画像の情報を取得する(プリセット・再現の復元時に存在確認も兼ねる。無ければ `IMAGE_NOT_FOUND`) |

```ts
interface InputImage {
  path: string;      // 画像保存先からの相対パス("inputs/<uuid>.png")。GenerationParams.controlnet.units[].image に入れる
  filePath: string;  // 絶対パス。フロントが convertFileSrc で表示する
  width: number; height: number;
}
```

- 取り込みは PNG へ再エンコードして `inputs/` に保存する。形式・サイズが不正なら `VALIDATION`(20MB、長辺 8192px まで)。
- ファイルのパスをフロントから受け取らない設計(任意ファイル読み取りの防止)。
- `job_enqueue` は、有効な ControlNet の参照画像が保存先に無ければ `IMAGE_NOT_FOUND` で断る。待機中に消えた場合は、ジョブが `failed`(`IMAGE_NOT_FOUND`)になる。

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

### AI サポート(v0.3、設計: 11 §8)

| コマンド | 引数 | 戻り値 |
|---|---|---|
| `assistant_status` | – | `{ hasKey; verified; model; consent }` |
| `assistant_set_key` | `{ key }` | `{ verified }`(401 は保存せず `ASSISTANT_AUTH`) |
| `assistant_clear_key` | – | – |
| `assistant_update` | `{ model?; consent?; autoDelete? }` | `status` |
| `assistant_ask` | `{ conversationId: string \| null; text; params: GenerationParams }` | `{ requestId; conversationId; assistantMessageId }` |
| `assistant_conversation_list` / `_get` / `_delete` | – / `{ id }` / `{ ids \| null }`(null = すべて) | 会話の一覧 / 取得 / 削除(最大 30 会話、11 §4.1) |
| `assistant_suggestion_applied` | `{ messageId; index }` | 提案を適用済みとして記録 |
| `assistant_cancel` | `{ requestId }` | – |

- キーは戻り値・イベントに含めない。同意なし `ASSISTANT_NO_CONSENT`、キーなし `ASSISTANT_NO_KEY`、実行中の再送 `ASSISTANT_BUSY`。
- 応答はイベント `assistant://delta` / `assistant://suggestion` / `assistant://done` / `assistant://error`(下記)。

### モデル取得(v0.4、設計: 12 §8。レビュー待ち)

| コマンド | 引数 | 戻り値 |
|---|---|---|
| `models_catalog` | – | `{ catalog; installed; modelsDir; modelsDirSource; freeBytes }`(外部へは通信しない。自動検出のため ComfyUI へは問い合わせる) |
| `models_download_start` | `{ familyId; variantId; consent?: boolean }` | `{ downloadId }` |
| `models_download_cancel` | `{ downloadId; discard?: boolean }` | – |
| `models_download_list` | – | `DownloadState[]` |
| `models_verify` | `{ familyId; variantId }` | `{ ok: boolean }`(導入済みファイルのハッシュ照合) |
| `models_set_dir` | `{ path: string \| null }` | `{ modelsDir; source }`(null で自動検出) |

- 型・検証・取得の仕様は 12 が正。`settings_update` は `engine.comfyui.models_dir` を受け付けない(`models_set_dir` で検証つきで変更)。
- イベント `models://progress` / `models://done`(下記)。エラー: `MODELS_DIR_UNKNOWN` / `MODELS_NO_CONSENT` / `MODELS_HOST_NOT_ALLOWED` / `MODELS_DISK_FULL` / `MODELS_DOWNLOAD_FAILED` / `MODELS_HASH_MISMATCH` / `MODELS_ALREADY_INSTALLED` / `MODELS_NOT_FOUND`。

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
| `job://progress` | `{ jobId; step; totalSteps; node?: string; phase?: "generate" \| "face_detail" }` | 生成中(最大 10 回/秒)。`phase` はどの段階の進捗かを示す(v0.2。進捗を出したノードが顔補正なら `face_detail`、それ以外は `generate`。UI は顔補正のときだけ「顔補正中」と表示) |
| `job://preview` | `{ jobId; dataUrl: string }` | (M2 では未実装。将来) |
| `image://created` | `ImageItem` | 画像が保存された時 |
| `image://deleted` | `{ ids: string[] }` | 削除時 |
| `assistant://delta` | `{ requestId; text }` | AI の回答の断片(v0.3) |
| `assistant://suggestion` | `{ requestId; index; summary; changes }` | 検証済みの設定変更の提案(v0.3) |
| `assistant://done` | `{ requestId; cancelled }` | 回答完了 |
| `assistant://error` | `{ requestId; code; message }` | 失敗 |
| `models://progress` | `DownloadState`(12 §8) | モデル取得の進捗(最大 4 回/秒。v0.4) |
| `models://done` | `DownloadState` | 取得の完了・失敗・キャンセル。完了時はフロントが `engine_list_models` を再取得する |

## エラーコード(初期)

| code | 意味 | UI の対応 |
|---|---|---|
| `ENGINE_UNREACHABLE` | ComfyUI に接続できない | 設定へのリンクと再接続ボタン |
| `ENGINE_BAD_RESPONSE` | 接続先が ComfyUI として期待する応答を返さない(別サービス / 非互換バージョン) | 設定の URL 確認を促す |
| `ENGINE_MODEL_MISSING` | 指定モデルが存在しない(`/prompt` が `value_not_in_list` を返した) | モデル再選択を促す |
| `ENGINE_NODE_MISSING` | 必要なカスタムノードが無い(例: GGUF、顔補正の Impact Pack、ControlNet の前処理)。`details.node` にノード名 | 導入手順を表示 |
| `ENGINE_EXECUTION_FAILED` | 生成失敗(OOM 等)。`details` に原文 | 解像度縮小などを提案 |
| `PRESET_NOT_FOUND` | 存在しないプリセット | トースト |
| `JOB_NOT_FOUND` / `JOB_INVALID_STATE` | 不正なジョブ操作(存在しない ID、状態が合わない操作) | トースト |
| `IMAGE_NOT_FOUND` | 画像/ファイル欠損 | トースト、一覧から除外提案 |
| `VALIDATION` | 入力不正。`details.field` | フォームにインライン表示 |
| `STORAGE_ERROR` | ファイル I/O 失敗 | トースト |
| `DB_ERROR` | DB 失敗 | 再起動案内 |
| `ASSISTANT_NO_KEY` / `ASSISTANT_NO_CONSENT` | API キー未登録 / 同意未取得(v0.3) | 設定への案内 / 同意ダイアログ |
| `ASSISTANT_AUTH` | API キーが無効(401/403) | キーの再登録を促す |
| `ASSISTANT_RATE_LIMIT` | 制限・過負荷(429/529) | 待って再送を促す |
| `ASSISTANT_UNREACHABLE` | api.anthropic.com に接続できない | ネットワーク確認を促す |
| `ASSISTANT_API_ERROR` | その他の API エラー。`details` はステータスとリクエスト ID のみ | トースト |
| `ASSISTANT_BUSY` | 別の相談が実行中 | 完了または停止を待つ |
| `ASSISTANT_CONVERSATION_NOT_FOUND` | 会話が存在しない | 一覧を更新 |
| `INTERNAL` | 想定外 | 詳細ログ参照案内 |
