# 07. 未決事項・レビューで決めてほしい点

「推奨」は設計資料 01〜06 で前提としている案です。異論がなければ「推奨で OK」で確定します。

## A. 決定が必要(実装前に確定したいもの)

| ID | 論点 | 選択肢 | 推奨 |
|---|---|---|---|
| Q-01 | **画像生成エンジンの方式** | (a) 既存 ComfyUI に HTTP/WS で接続<br>(b) アプリが ComfyUI を同梱・自動起動<br>(c) Rust 組み込み推論(stable-diffusion.cpp / candle 等) | **(a) を MVP、(b) を v0.2**。(c) は Flux 対応・GPU 最適化の工数が大きく、エンジン抽象を挟んで将来拡張 |
| Q-02 | アプリ名 / 識別子 | 例 `gazoo` / `dev.gazoo.app` | `gazoo` / `dev.gazoo.app` → **識別子は Q-18 で `dev.gazoo.desktop` に変更** |
| Q-03 | パッケージマネージャ | pnpm / npm / bun | pnpm → **npm に変更**(2026-11-18、ユーザー指示。下記 D・E 参照) |
| Q-04 | 状態管理 | zustand / Redux 等 / TanStack Query 併用 | zustand のみ(05 §5) |
| Q-05 | ブランチ戦略の補足 | `main` を設けるか(リリース用)/ `develop` のみ | `develop` 起点。`main` はリリース時に導入(MVP 完了時に再検討) |
| Q-18 | **アプリ識別子の再検討(Q-02)**。`dev.gazoo.app` は `.app` で終わり、macOS のバンドル拡張子と衝突する(`tauri build` が警告する)。識別子はデータ保存先のフォルダ名にもなる | **`dev.gazoo.desktop` に変更で決定**(2026-11-19)。データフォルダが変わるため、開発中のデータは引き継がれない(未リリースのため影響は開発環境のみ) |
| Q-06 | **初回コミットの扱い** | 現在 `develop` はコミット 0 件のため、作業ブランチの分岐元が存在しない | 初回のみ `develop` に空の initial commit を作成(`AGENTS.md` §4 の例外)し、以降は作業ブランチ運用 |

## B. 仕様上の確認(推奨で進めてよいか)

| ID | 論点 | 推奨 |
|---|---|---|
| Q-10 | 画像削除時の挙動 | OS のゴミ箱へ移動(`trash` クレート)。Windows/macOS 両対応 |
| Q-11 | 保存先変更時の既存画像 | 「移動する / しない」を選択するダイアログ |
| Q-12 | UI 言語 | MVP は日本語・英語の 2 言語(OS ロケール追従) |
| Q-13 | 対象 GPU | Windows は NVIDIA のみ検証、AMD/Intel は ComfyUI 側の対応次第で非保証 |
| Q-14 | プロンプト検索 | MVP は `LIKE`、1 万件超で性能問題が出たら FTS5 |
| Q-15 | LoRA | v0.2 で対応(MVP はモデル一覧の取得のみ) |
| Q-16 | 自動アップデート | MVP 対象外(`tauri-plugin-updater` は将来) |
| Q-17 | コード署名 | MVP 対象外(Windows SmartScreen 警告・macOS 未公証の旨を README に記載) |

## C. 技術リスク(認識共有)

| ID | リスク | 対策 |
|---|---|---|
| R-01 | ComfyUI の API / ノード仕様変更でワークフローが壊れる | テンプレート JSON を `resources/workflows/` に分離、`/object_info` で検証、対応 ComfyUI バージョンを README に明記 |
| R-02 | Flux(GGUF)にはカスタムノード(ComfyUI-GGUF)が必要 | ノード有無を検出して導入手順を案内(Q-01 (b) で自動化を検討) |
| R-03 | Mac 16GB で Flux dev は低速・メモリ逼迫 | 生成前にモデル/解像度の警告、schnell の推奨表示 |
| R-04 | Next.js 16 の静的エクスポートの制約(動的ルート等) | クエリパラメータ方式で回避(02 §3.1) |
| R-05 | Tauri asset protocol の scope 設定ミスによる情報漏えい | 保存先ディレクトリのみに限定し、スコープ変更をテスト |
| R-06 | Windows での CI ビルド時間 | キャッシュ活用、PR では `tauri build` を省略し main/develop push のみで実施する案 |

## D. レビュー記録

| 日付 | レビュワー | 対象 | 結果 / コメント |
|---|---|---|---|
| 2026-11-19 | ユーザー | Q-18・docs/08・docs/09 | Q-18: 識別子を `dev.gazoo.desktop` に変更。docs/08(ローカル ComfyUI 環境)と docs/09(リリース前チェックリスト)を **承認**。実機検証のモデルは **SD 1.5** に決定 |
| 2026-11-18 | ユーザー | Q-03 変更 | パッケージマネージャを pnpm から **npm** へ変更するよう指示 |
| 2026-11-18 | ユーザー | 01〜07 全体 | A・B の全項目を **推奨案どおりで決定**。01〜06 を承認。実装フェーズへ移行 |

## E. 決定事項サマリ(2026-11-18)

- Q-01: MVP は既存 ComfyUI に HTTP/WS 接続(同梱・自動起動は v0.2)
- Q-02: アプリ名 `gazoo` / identifier ~~`dev.gazoo.app`~~ → **`dev.gazoo.desktop`**(Q-18、2026-11-19 に変更)
- Q-03: ~~pnpm~~ → **npm**(2026-11-18 に変更)
- Q-04: zustand のみ
- Q-05: `develop` 起点、`main` はリリース時に再検討
- Q-06: 初回のみ `develop` に空の initial commit を作成(AGENTS.md §4 に例外として明記)
- Q-10〜Q-17: すべて推奨どおり
