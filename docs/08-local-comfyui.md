# 08. ローカル ComfyUI 開発環境

アプリの接続先となる ComfyUI を、このリポジトリ内で再現可能に用意するための仕組み。
**アプリ本体のランタイム要件ではなく、開発・検証用のツール**(01 の MVP 方針どおり、アプリは「起動済みの ComfyUI」に接続する)。
ユーザーが既に ComfyUI を持っている場合は、これを使わず設定画面で URL を指定すればよい。

> ステータス: 🟢 承認済み(2026-11-19)

## 1. 方針

- セットアップは **スクリプトで再現可能** にし、ComfyUI・カスタムノード・PyTorch はすべて **バージョン固定**(AGENTS.md「依存バージョンの固定」)。
- 固定値は `tools/comfyui/versions.env` に集約する。更新は単独の変更として行う。
- 生成物(ComfyUI 本体・venv・モデル・出力)は `comfyui/` 配下に置き、**git 管理しない**。
- OS 差異は OS 別スクリプト(macOS: bash / Windows: PowerShell)で吸収し、呼び出しは `npm run` スクリプトに統一する(06 の方針)。
- モデル(数 GB)・ライセンス同意が必要なものは **自動ダウンロードしない**。置き場所と推奨モデルを案内する。

## 2. 構成

```
tools/comfyui/                 # git 管理(スクリプトと固定値)
  versions.env                 # 固定バージョン(ComfyUI タグ / GGUF コミット / Python / PyTorch)
  run.mjs                      # OS を判定して下記スクリプトを呼ぶディスパッチャ
  setup.sh   setup.ps1         # セットアップ(冪等)
  start.sh   start.ps1         # 起動
comfyui/                       # git 管理外(生成物)
  ComfyUI/                     # 固定タグの ComfyUI 本体(読み取り専用扱い)
  venv/                        # Python 仮想環境
  data/                        # --base-directory: models/ custom_nodes/ input/ output/ user/
```

`--base-directory` により、モデル・カスタムノード・出力は `comfyui/data/` に置かれ、ComfyUI 本体を汚さない(本体の更新はタグ変更だけで済む)。

## 3. 使い方

| コマンド | 内容 |
|---|---|
| `npm run comfyui:setup` | ComfyUI(固定タグ)と ComfyUI-GGUF(固定コミット)を取得し、venv と依存(PyTorch 含む)を導入 |
| `npm run comfyui:start` | `127.0.0.1:8188` で ComfyUI を起動(`COMFYUI_PORT` で変更可)。アプリの既定接続先と一致 |

起動後、アプリのサイドバーの接続バッジが緑になれば接続成功。

## 4. 前提と OS ごとの扱い

| | macOS (Apple Silicon) | Windows 10/11 (NVIDIA) |
|---|---|---|
| Python | 3.12(`brew install python@3.12`) | 3.12(`py -3.12` で起動できること) |
| 追加要件 | git | git、最新の NVIDIA ドライバ |
| PyTorch | PyPI 版(MPS 対応) | PyTorch 公式 CUDA 版(`versions.env` の `TORCH_CUDA_INDEX`、既定 cu130) |
| 起動時の設定 | `PYTORCH_ENABLE_MPS_FALLBACK=1` | – |

- Python は ComfyUI/PyTorch が十分に対応する 3.12 に固定する(既定の `python3` が新しすぎる場合に備え、バージョンを明示して探す)。
- Windows 版は macOS 上では実行検証できないため、**初回は実機での確認が必要**(CI の Windows ジョブでは構文確認まで)。代替として ComfyUI 公式の Windows ポータブル版を使い、URL を設定してもよい。
- AMD / Intel GPU は対象外(必要なら `TORCH_*` を調整)。

## 5. モデルの配置(手動)

`comfyui/data/models/` 以下に配置する。

| 種類 | 置き場所 | 用途 |
|---|---|---|
| チェックポイント | `checkpoints/` | SD1.5 / SDXL |
| Flux 拡散モデル(GGUF) | `unet/` | Flux.1 GGUF(量子化推奨: Mac 16GB は Q4 前後) |
| テキストエンコーダ | `clip/` | `clip_l`、`t5xxl`(fp8 または GGUF) |
| VAE | `vae/` | Flux 用 `ae.safetensors` など |
| LoRA | `loras/` | (v0.2 以降) |

Flux.1 dev は非商用ライセンス、schnell は Apache 2.0(01 の注意事項を参照)。

開発中の動作確認には SD 1.5(`stable-diffusion-v1-5/stable-diffusion-v1-5` の `v1-5-pruned-emaonly.safetensors`、約 4.27GB、CreativeML OpenRAIL-M、認証不要)を使用した(09「開発中の実機検証ログ」)。配布元が公表する SHA-256(`6ce01616…fa2fa`)と照合してから使うこと。

## 6. 実機スモークテスト

起動中の ComfyUI の実際の応答を、アプリの Rust 側パーサで解釈できるか確認するテスト(通常の `cargo test` では実行されない)。

```
npm run comfyui:start                               # 別ターミナル
cd src-tauri && cargo test -- --ignored real_comfyui --nocapture
```

ComfyUI のバージョンを上げたとき(§7)に、応答形式の変化を検出するために実行する。接続先は環境変数 `COMFYUI_URL` で変更できる。

## 7. 更新手順

1. `versions.env` の値を変更する(ComfyUI タグ、GGUF コミット、PyTorch)。
2. `npm run comfyui:setup` を再実行(冪等。タグ/コミットを取り直し、依存を再インストール)。
3. アプリと結合して動作確認(接続・モデル一覧・生成)。
4. ComfyUI の API 変更でアダプタに影響が出る場合は 02(R-01)に従い、ワークフローテンプレートを更新する。
