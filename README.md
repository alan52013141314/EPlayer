# EPlayer

Windows 10／11 x64 免安裝音樂播放器，以 Rust 開發，效能優先，介面參考千千靜聽。

目前狀態：1.2.0，音樂庫可直接播放，清單編輯保留目前音訊緩衝。文件日期：2026-10-25。

[下載 Windows x64 免安裝版](https://github.com/alan52013141314/EPlayer/releases/latest)

## 從原始碼建置

需求：Windows 10／11 x64、Rust stable（MSVC 工具鏈）、Visual Studio C++ Build Tools 及 Windows SDK（含 rc.exe）。首次建置需要下載 Cargo 依賴：

```powershell
cargo build --release --locked
cargo test --locked
```

原始碼包含 `vendor/eframe` 的必要修補與原始授權文字；請勿刪除該目錄。一般 WAV／FLAC／MP3 可直接播放；完整 AAC／M4A／Opus 支援需要 FFmpeg／FFprobe，放在執行檔旁的 `tools` 目錄，或以 `EPLAYER_MEDIA_TOOLS` 指定工具資料夾。公開 release 已附帶工具與授權文件。

自行封裝時，將 FFmpeg 9.0.2 essentials build 解壓至 `vendor/ffmpeg/ffmpeg-9.0.2-essentials_build`，再執行 `./scripts/package.ps1 -Label 1.2.0 -ReleaseDirectory target/release`。使用者資料、音樂檔及本機 verification 原始紀錄不包含在公開儲存庫；驗收摘要見 docs/ACCEPTANCE.md。

## 文件入口

- [PR-005：音樂庫直接播放與清單編輯](docs/PR-005-Library-Queue.md)：雙擊選項、不中斷編輯及刪除目前歌曲的行為。

- [PR-003：播放器操作與視窗配置](docs/PR-003-Player-Layout.md)：本次需求與實作計劃。

- [PR-001：產品需求](docs/PR-001-EPlayer.md)：已確認需求、1.0 邊界與驗收條件。
- [PLAN-001：開發計劃](docs/PLAN-001-EPlayer.md)：技術選型、階段、風險與驗證方法。
- [使用說明](docs/USER-GUIDE.md)：開啟音樂、清單、歌詞、浮動模式與快捷鍵。
- [驗收追蹤](docs/ACCEPTANCE.md)：F01–F16 的現有證據與剩餘項目。

此處 PR 指本機產品需求文件，不是 GitHub Pull Request。

1.0 完成本機播放、音樂庫、歌詞、等化器、無縫播放、浮動播放器及可關閉的音訊頻譜。YouTube 相關工作全部延後至 1.0 之後。

使用者已授權完成程式（不含 YouTube）。進度與尚未驗證項目見 [WORK-LOG](docs/WORK-LOG.md)。沒有建立伺服器。

## 開發建置

執行 `./scripts/cargo.ps1 build --offline`；腳本會優先使用工作區既有工具鏈，其他環境可使用 PATH 中的 Cargo。原生介面使用 eframe 0.31.1，依賴已由 Cargo.lock 鎖定。

開發期間，AAC／M4A／Opus 補充解碼工具由 `EPLAYER_MEDIA_TOOLS` 指定含 ffmpeg.exe／ffprobe.exe 的資料夾；免安裝套件將工具放入執行檔旁的 tools 資料夾。使用 scripts/package.ps1 建立帶獨立標籤的 ZIP。

使用者指定後續測試全部在背景執行，耐久測試為 10 分鐘；並接受實體媒體鍵、真實多螢幕／DPI、遊戲覆蓋、系統匣與穿透的未完整實測限制。請閱讀套件內「驗收報告.md」與「使用說明.md」，不要把支援目標解讀成所有環境均已實測。

## 執行

解壓 dist/EPlayer-1.0.0-win-x64.zip 後執行 eplayer.exe，保留同層 tools 與授權文件；不需要 Rust 開發環境。


## 1.0.1 圖示與介面

最新版本：dist/EPlayer-1.0.1-win-x64.zip。採用 icon.png 月紫圖示與一致深淺配色；需求及實作計劃見 [PR-002](docs/PR-002-Icon-Theme.md)。背景驗證完成，未進行桌面視覺操作驗收。

## 1.0.2 視覺驗收修正版

最新套件 dist/EPlayer-1.0.2-win-x64.zip。已實際檢查主視窗深淺色、680px 窄視窗、完整／精簡浮動、長歌詞及等化器；修正間距、控制高度與長文字裁切，詳細證據見 WORK-LOG。
