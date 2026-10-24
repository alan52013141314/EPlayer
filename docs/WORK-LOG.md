# EPlayer 開發紀錄

## 2026-10-22 — 啟動實作與技術驗證

使用者授權：完成所有本機播放器需求，排除 YouTube；不建立伺服器。

### 已完成的實際工作

- 建立 Rust 專案、鎖定依賴、工作區工具鏈腳本；MSVC debug build 成功。
- Symphonia 原生解碼與 FFmpeg 本機 fallback；外部工具以參數陣列呼叫、隱藏視窗，拒絕不存在的本機檔案並限制 file／pipe 協定。
- 有界 ring buffer 音訊輸出，背景串流解碼，CPAL WASAPI；支援 seek、切歌、暫停、循環／隨機的控制路徑，仍需完整回歸。
- 實作初版深淺色主視窗、開檔／拖放、播放清單顯示與浮動播放器。
- 實測三個原生視窗同時存在及中文顯示；歌詞視窗目前只是 T1 技術探針，尚非 LRC 功能。

### 證據

- `verification/initial-format-probe.jsonl`：原生解碼初測，揭露 Opus 缺口與 AAC 邊界差異。
- `verification/format-probe.jsonl`：11 種格式樣本皆可解碼；M4A AAC 修正零秒 seek 後精確輸出 144,000 frames。
- `verification/audio-smoke.txt`：WAV＋FLAC 合計輸出 288,000 frames／48 kHz，underruns=0。
- `verification/native-windows.json`／`.png`：主視窗、Mini 與歌詞技術視窗的實際桌面證據。
- `verification/initial-resource-snapshot.json`：debug 多視窗閒置初測，僅作基準，不代表 release 效能已驗收。

### 已知限制與後續工作

- ADTS AAC 缺少足以推定原始首尾的容器 metadata，來源 padding 不任意裁掉。
- 上述單次短音訊測試不能證明全部格式無縫、兩小時穩定性或所有裝置相容。
- T1 已確認建置、必要格式補充方案、音訊輸出、多視窗技術路徑。穿透、DPI、完整隱藏／恢復與遊戲相容性將以 T4 的完整功能實測；release 效能預算在取得基準後明確記錄。
- T2 初版播放操作已接入 UI，仍需控制流程回歸；音樂庫／儲存、封面 UI、LRC、EQ、頻譜、系統匣／快捷鍵、完整 overlay 設定、正式封裝及 T6 驗收尚未完成。
- 下一步：完成 T2 控制流程測試及必要修正，再進入 T3 持久化與音樂庫。

目標仍為全部本機需求完成，未標記 1.0 完成。

## 2026-10-22 — 本機功能整合（後續進度）

本節取代上節的「下一步」描述。

- T2 控制流程實測通過：暫停時鐘、暫停中 seek、切歌、單曲循環；見 verification/control-test.jsonl。
- T3 已加入 SQLite 背景掃描／搜尋／100 筆分頁、清單新增／改名／刪除／排序／保存、背景封面縮圖與設定保存。自動測試涵蓋 10,000 筆索引、清單順序及設定覆寫；實際 11 個檔案掃描兩次無重複，關閉／重開資料庫仍還原 11 個清單項目，見 verification/library-test.jsonl。
- T4 已加入 LRC 解析、自動同名 LRC／手動載入、真實浮動歌詞、完整／精簡 preset、透明度、置頂、拖曳與縮放、穿透與恢復快捷鍵、系統匣與媒體鍵。桌面實測 LRC 顯示、Ctrl+Alt+Space 暫停、穿透開關及 Ctrl+Alt+L 清除穿透。主視窗隱藏後持續操作、DPI／螢幕移除與遊戲相容性仍待完整驗證。
- T5 已加入 10 段 EQ／前級／bypass 與 48 段 FFT 頻譜。9 個測試通過，包括 EQ bypass 位元不變、1 kHz +6 dB 響應、440 Hz 頻譜定位。verification/spectrum-test.jsonl 證明關閉後 FFT 計數不再增加，輸出 underruns=0。
- scripts/verify-gapless.py 以連續訊號分割成兩首再經實際解碼／重採樣路徑接合，WAV、FLAC、MP3、Vorbis、M4A AAC、Opus 六種格式的長度與接縫檢查通過；見 verification/gapless-report.json。這不代替裝置端長時間驗收。
- 44.1 kHz → 48 kHz 使用整數相位累積，1 秒檔案精確輸出 48,000 frames；見 verification/resample-test.jsonl。
- 第一版 release build 成功；後續修正仍需重新建置。CPU／記憶體最終數字尚未驗收。
- 正在準備獨立 FFmpeg 靜態工具套件、完整桌面回歸與長時間測試。YouTube 沒有實作。

### 尚未完成的主要驗收

1. 正式套件的工具版本／授權／checksum 與解壓即用驗證。
2. UI 全流程、清單編輯與正在播放佇列同步、全部浮動設定及恢復行為的回歸。
3. 全隱藏／頻譜停用的資源行為、release 效能預算與兩小時穩定性。
4. Windows 10／11 乾淨環境、不同 DPI／多螢幕、遊戲顯示模式相容性；目前只掌握本機 Windows build 26200，不能宣稱已完成跨系統驗收。

### 使用者驗收調整

使用者於詢問 Windows 10 測試環境後回覆「不需要」。因此 Windows 10 實機／虛擬機驗收不再是交付阻擋條件；支援目標仍為 Windows 10／11 x64，交付必須明列實際只在目前 Windows 11 環境驗證，不宣稱測過 Windows 10。

## 2026-10-23 — 候選封裝與長測缺陷修正

- 已下載並校驗 FFmpeg 9.0.2 essentials 靜態套件，SHA-256：60F467265B1E312373DBCD92200C2618A74850F98D3D078E94296BB3FA2047BA。
- 新增 scripts/package.ps1 與 docs/USER-GUIDE.md；candidate1 ZIP 含播放器、ffmpeg／ffprobe、授權資料及檔案雜湊。這是修正前的驗收候選，不能當正式 1.0。
- 將 ZIP 解壓到含中文／空白的路徑、清除工具環境變數、PATH 僅留 Windows 系統目錄後，13 個樣本全部可解碼。見 verification/portable-format-probe.jsonl。這是本機依賴隔離測試，不等同乾淨虛擬機。
- release GUI 不再顯示控制台；GUI 啟動失敗有錯誤對話框，診斷模式保持非零錯誤碼。
- 桌面實測縮至系統匣後 Mini 可重新播放，Ctrl+Alt+L 恢復主視窗，淺色 preset 可切換。更完整的 preset／多 DPI／遊戲回歸仍未完成。
- 第一輪 2 小時長測在 180 秒紀錄已出現 1 次 underrun，已停止；保留 verification/soak-attempt1-interrupted.jsonl，不算通過。
- 將問題縮小為純 WAV 的 300 次連續 seek，修正前 119 次 underrun；修正後 0 次。原因是 callback 可能丟棄更新世代的樣本，以及預緩衝 ready 未真正控制取樣。現在只丟較舊世代，保留較新世代，暫停時仍清理舊緩衝，並按新世代的有效樣本數完成預緩衝。見 verification/seek-stress-before.jsonl／seek-stress-after.jsonl。
- 修正外部解碼來源的 metadata：同時讀取容器與音軌標籤，忽略標籤大小寫，補足 Opus 歌名／歌手資訊。
- 修正後 clippy 與 9 個單元測試通過。release 重新建置及混合格式長測仍需完成；目標維持 active。

## 2026-10-23 — 套件回歸與驗收矩陣

- 產生 candidate2 套件，包含上一輪 seek／Opus metadata 修正。SHA-256 為 9649A36FD5B3003536A983B27D725B25F60B87E3021E869D5C634A42BD65CB0C。
- 新增 docs/ACCEPTANCE.md，以 F01–F16 分列現有證據與未完成驗收，README 已連結。
- 擴充實際音訊控制測試：暫停中 UpdateQueue 不自行恢復播放、編輯後保留播放位置、清空佇列停止。verification/queue-edit-control-test.jsonl 通過，underruns=0。
- verify-gapless.py 加入不同採樣率（44.1／48 kHz）和聲道數（mono／stereo）曲目交界；共八組測試通過，見更新後 gapless-report.json。
- candidate2 可解碼含中文／空白的 MP3 路徑及中文歌名／歌手／專輯；損壞 MP3 回報錯誤且退出碼 1。見 unicode-metadata.jsonl／corrupt-file.jsonl。含封面樣本已準備於 verification/fixtures-ui，尚待 UI 顯示驗證。
- release GUI 播放、FFT 開啟、Mini／Lyrics 關閉的 20 秒基準：CPU 約整機 0.405%（16 邏輯處理器），工作集 144,228,352 bytes，private 151,908,352 bytes；見 release-gui-performance.json。這不是所有場景的最終效能驗收。
- candidate2 的 Mini／Lyrics 視窗已實際顯示，個別可關閉。桌面工具多次偵測到並行使用者輸入，暫停滑鼠測試，避免將座標／輸入衝突誤判成產品缺陷。精簡 preset 等項目仍未標記通過。
- 第二輪兩小時長測的 live session 是 98097，持續寫入 verification/soak-attempt2.jsonl；必須等待其真正完成，不得因觀察超時重啟。最後一次查閱超過八分鐘，underruns=0。最終結果仍待確認。

## 2026-10-23 — 外部解碼來源封面

- 將 Symphonia 的容器 metadata 讀取與建立音訊 decoder 分開，ExternalReader 可取得原生 decoder 不支援的 Opus 封面；音訊解碼仍由隨附 FFmpeg 負責。
- probe 新增 cover_bytes 欄位；帶 128×128 PNG 封面的 MP3 與 Ogg Opus 皆讀到 381 bytes，見 verification/cover-metadata.jsonl。尚待原生 UI 的封面縮圖確認。
- 此修正已在 debug 建置驗證，尚未加入 candidate2 或正在執行的長測版本。最終封裝需更新；不可聲稱 candidate2 已包含它。
- 使用者允許約三分鐘桌面測試，期間設定點擊仍無法確認切換，已停止點擊並交還桌面。尚無證據可判斷是工具座標偏移或實際 UI 輸入問題，必須繼續查明；不算驗收通過。

## 2026-10-23 — 浮動 preset 尺寸回退修正

- 已用輸入診斷確認完整 PointerButton down/up 送達，精簡 preset 可實際勾選；精簡模式 380×105 截圖為 verification/compact-preset.png，完整模式初始截圖為 full-preset.png。
- 實際發現完整模式先變為 440×300、隨後被舊的 380×105 視窗回報覆寫。新增 WindowSettings 的非持久化 observed geometry，僅採納有變化的 OS bounds，避免舊回報撤銷 preset／位置重設／縮放請求。
- 修正後重新執行精簡→完整切換，多次觀察及 settings.json 均維持 440×300。新增 delayed_os_geometry_does_not_undo_preset_or_position_reset 回歸測試，10 個單元測試通過。
- 手動縮放尚未通過 UI 驗證；已將自行累加 pointer delta 的縮放方式換成 Windows 原生 BeginResize(SouthEast)，clippy 通過。此最新改動尚未重建 UI 程序，需要下輪實測。
- 暫時性 DEBUG-input 診斷碼已移除。診斷期間曾因記錄程式巢狀讀取 egui context 造成測試版停滯，已修正並刪除該記錄碼；candidate2 和長測程序未含此診斷碼。
- 正在執行的 debug GUI session 79252 是 geometry 修正版、尚未含最新 native resize；長測 session 98097 繼續運作，1500 秒紀錄 underruns=0。
- candidate2 仍不含 geometry／封面修正，最終 release 與套件需重新建置。

## 2026-10-23 — 原生開檔、清單與 EQ 操作

- 最新 debug GUI session 57664 已正常關閉。實測新建「驗收清單」、下拉切換至空清單時原音樂繼續播放、透過原生檔案對話框加入「中文 歌曲.mp3」並播放。
- verification/unicode-file-dialog-playback.png 顯示中文歌名／歌手／專輯／封面與新清單項目。Opus 封面在主視窗和 Mini 皆實際顯示，截圖為 opus-cover-main.png、opus-cover-mini.png。
- EQ UI 啟用並將 1 kHz 調為 +6.5 dB、前級 -10.5 dB；關閉後讀取 settings.json 確認持久保存，SQLite 唯讀查詢確認新清單及其曲目保存。見 equalizer-ui.png、gui-persistence.json。
- 此處未將「保存」擴大宣稱為全部重啟 UI 驗收；下一步重新開啟確認還原及其他清單操作。
- Sky 不支援 XF86AudioPlay 按鍵，媒體鍵 UI 實測仍未完成，不能以快捷鍵測試替代。
- native BeginResize 的拖曳自動化仍未造成可確認的視窗縮放。尚需比對子視窗收到的座標／按鍵事件，或實際人工拖曳；不標記通過。
- 第二輪長測 1800 秒仍 underruns=0，session 98097 持續執行。沒有重啟或縮短兩小時門檻。

## 2026-10-23 — 快速拖曳命中修正

- 子視窗事件紀錄證實按下座標 [422,282] 在 grip [414,274]–[430,290] 內，clip 正常且 enabled=true，但 egui 的 hovered/drag_started 皆 false；快速拖曳在同一輪更新前移到把手外，最後位置命中方式漏掉起點。
- 以實際 PointerButton 按下位置保留 resize anchor，處理後續 PointerMoved／放開，直接送出 InnerSize；取代前一輪 BeginResize 嘗試。快速拖曳單元回歸通過。
- 同一個原生桌面拖曳實測由 440×300 正確變為 368×248，截圖 metadata 與 settings.json 都確認結果；證據 verification/mini-resized.png。
- 標題亦改為保留螢幕座標的拖曳 anchor，並禁止選取該文字。第一次實測可移動但放開時使用舊 local position 導致重複位移；已修正為放開只結束拖曳，不重新計算位置，並用舊座標放開的回歸測試覆蓋。此最後修正待重新建置與桌面回歸。
- 重啟後主視窗顯示「驗收清單」及中文歌曲，清單還原已確認。EQ UI 重啟還原仍待打開視窗確認。
- 12 個單元測試及 clippy 通過；DEBUG-resize 診斷碼已移除。正在運行的 debug GUI session 15906 尚未含最後的放開事件修正；long soak session 98097 不變。

## 2026-10-23 — 桌面回歸與手動歌詞循環缺陷

- 最新拖曳放開修正實測通過：Mini 水平移動 100 px，Lyrics 水平移動 50 px，未重複位移；Lyrics 縮放為 500×120。證據 mini-moved.json、lyrics-geometry.json。
- EQ 重啟還原已確認，截圖 eq-restored.png；淺色 preset 與驗收清單亦已還原。
- 音樂庫掃描包含兩個正常樣本及一個刻意損壞 MP3，錯誤顯示後正常樣本仍可查詢；中文搜尋「封面」命中正確結果。證據 library-scan-error.png、library-chinese-search.png。
- 原生對話框手動載入 desktop-test.lrc，主視窗、Mini、獨立歌詞皆顯示「EPlayer 本機音樂測試」。循環回到相同音樂後卻顯示尚未載入，缺陷已實際重現。
- 修正為本次程式執行期間按音樂路徑記住手動 LRC 選擇，後續曲目 token 更新優先重新載入該來源；過期對話框結果不套到其他歌曲。此修正尚待重建及桌面回歸，不宣稱驗收通過。
- cargo fmt、12 項單元測試、clippy -D warnings 通過。已送出 Alt+F4 結束本輪 debug 桌面操作，長測程序不受影響。
- 長測 session 98097 最新 2940 秒（49 分鐘）underruns=0；尚未達兩小時通過門檻。candidate2 不包含本輪 GUI／歌詞修正，仍不是最終套件。

## 2026-10-23 — 播放順序與 LRC 編碼回歸

- 上一輪有實際缺陷修正及桌面證據，屬進展；本輪重新輪詢 session 98097 確認長測仍存活，3060 秒 underruns=0，未重新啟動。
- 新增 --order-test 診斷，觀察音訊輸出實際消費的 token/index，而非只檢查背景預讀事件。順序播放 0→1→2 後停止、清單循環 2→0→1、單曲循環 1→1→1，以及八首隨機切換皆通過，underruns=0。證據 verification/order-test.jsonl。
- 手動歌詞對話框改綁定開啟時的音樂路徑，而非短暫播放 token；選檔期間同曲循環仍可套用，切到其他歌曲則只保存原曲選擇，避免套錯歌。未載入歌曲時禁用載入 LRC 按鈕。
- 增加真實檔案 UTF-8、UTF-16 LE/BE BOM、Big5 中文歌詞及前後 seek 回歸，通過。GBK 與 Big5 無標記編碼的歧義仍依操作文件列明。
- 13 個單元測試與 clippy -D warnings 通過；手動 LRC 修正仍待新版本 GUI 驗證。
- PR 更新已獲使用者授權的實作狀態及 Windows 10 實機／VM 驗收豁免，未將未實測系統標記為通過。
- 最新 release 另於 target/final 建置（session 84699），避免覆寫長測正在使用的 target/release/eplayer.exe。package.ps1 新增 ReleaseDirectory 參數，後續可明確封裝此次產物，預設原路徑不變。

## 2026-10-23 — candidate3 免安裝與歌詞 GUI 回歸

- release session 84699 成功（74 秒），產物 target/final/release/eplayer.exe；封裝 candidate3 ZIP，SHA256 D00BBB6C780320B28E6BCD9F3CA078B7AEF606D8DF33689B9EBCAC3CA57A0A56。
- 解壓至 verification/免安裝 candidate3，322 個 manifest 檔案 SHA256 全部一致。PATH 限制為 System32、移除 EPLAYER_MEDIA_TOOLS，13 個格式／封面樣本全部成功；MP3／Opus 都有 381 bytes 封面。此為本機限制環境測試，不冒稱乾淨 Windows 實機驗證。
- 新套件 GUI（root HWND 526180）成功啟動並還原清單／淺色／浮動設定。播放三秒中文 MP3 時開啟 LRC 對話框，跨多次循環後選取 desktop-test.lrc；返回後再次循環，主視窗及獨立 Lyrics 仍顯示歌詞。證據 candidate3-manual-lyrics-loop.png，手動選檔 token 過期／循環遺失回歸通過。
- 接續音量 UI 操作遭 Sky user input was detected 阻擋；重新觀察顯示視窗已由使用者移動，停止桌面輸入，音量與頻譜 UI 不標記通過。
- candidate3-invalid.jsonl 確認損壞與不存在來源均回傳錯誤、非零退出；candidate3-control.jsonl 八項裝置端控制檢查通過，underruns=0。
- 長測 session 98097 最近確認 3300 秒仍活躍、underruns=0；保持運行。candidate3 GUI 尚開啟，採 EPLAYER_DATA_DIR=verification/generated/gui-final，未改變交付 ZIP 中的預設資料。

## 2026-10-23 — 長測第二輪新增 underrun，維持未通過

- 重新輪詢 session 98097：3360 秒起 underruns=9，3480／3540 秒仍為 9。推翻先前零中斷趨勢，attempt2 不可作為通過證據；原程序保留運行，不因觀察超時重啟。
- 此時段包含 release 編譯及套件 GUI／控制回歸，但時間相關不能證明原因。按 diagnose 提出開檔供應耗盡、排程競爭、切歌／seek 交接三個可否證假設。
- 補充診斷 counters：開檔中的 underrun 次數、最大開檔耗時；soak 在計數增加時記錄當前播放 index／position、最近預讀來源及開檔狀態，不在音訊 callback 做檔案輸出。
- 新診斷版於 target/final 編譯 session 8969，未修改音訊供應策略；clippy 已通過。candidate3 仍是未含這批 counters 的固定套件，先前封裝與 GUI 證據不變。
- 診斷 release 建置成功（40.96 秒），13 項測試及 fmt --check 通過。啟動 300 秒混合格式診斷 session 22182，報告 verification/soak-open-diagnostic.jsonl；此為原因縮小測試，不替代兩小時門檻。下一步輪詢同一 session，依 underrun_observed 的開檔關聯決定下一個單變量實驗。

## 2026-10-23 — 解碼供應診斷補強

- 上一輪有候選封裝驗收與 underrun 診斷實作，屬進展。再次確認 session 22182、98097 均仍存活，沒有因等待或空輸出重啟。
- 第一輪開檔診斷至 120 秒無 underrun；原 attempt2 至 3780 秒仍累計 9，未再增加。尚不能定位原因或宣稱修復。
- 發現僅量測 MediaReader::open 不包含 FFmpeg 第一次 pipe read 等待，因此在真實 packet 邊界補充 max_read_micros／decoder_reading／underruns_while_reading；每分鐘同時記錄最大開檔及讀取耗時。只在 packet 邊界量測，不在每個 sample 記錄時間，不在 callback 輸出日誌。
- 補強版 debug 已建置，13 項測試及 clippy 通過；正在執行的 release 診斷 session 22182 尚未包含讀取耗時補強。其結束後才更新同一路徑 release，避免覆寫執行中的檔案。
- candidate3 GUI 在主視窗／完整 Mini／獨立歌詞設定、頻譜啟用、音量 0、EQ 啟用下觀察約 30 秒，設定前後未變。16 邏輯處理器總容量口徑：平均 CPU 0.57%、2 秒樣本峰值 0.73%、工作集峰值 186155008 bytes（約 177.5 MiB）。證據 verification/candidate3-gui-performance.json。此時另有音訊診斷程序運行；只代表這個觀察條件，不代表所有場景或 GPU 驗收。
- 開檔診斷 session 22182 已正常完成：300 秒、48 次載入、3 次掃描、0 underrun，暖機後記憶體增加 147456 bytes。只證明這五分鐘未重現，未解釋原長測 9 次中斷。
- 已啟動補強讀取耗時版本的 release 建置並銜接第二個 300 秒診斷，報告 verification/soak-read-diagnostic.jsonl；下一步以每分鐘 max_open_micros／max_read_micros 判定距離 0.5 秒緩衝上限的餘裕。原 attempt2 至 3900 秒仍為 9 次。

## 2026-10-23 — 路徑別名歌詞、暫停 seek 缺陷及頻譜 UI

- 上一輪有診斷補強及效能實測，屬進展。本輪確認 session 16207、98097 存活；讀取診斷到 180 秒零 underrun，max_open_micros=46178、max_read_micros=34565，尚無證據支持直接增加緩衝或宣稱已修復中斷。
- candidate3 GUI 再觀察發現手動歌詞隔一首消失。SQLite 唯讀查詢證實驗收清單兩筆其實指向同檔案，但一筆有 Windows extended prefix、另一筆沒有。先前同路徑循環通過不能涵蓋別名輪替。
- lyrics::source_key 以 canonicalize 統一存在的本機檔案識別，用於手動來源儲存、套用及讀取；真實檔案一般／canonical 路徑回歸通過。candidate3 不含此修正。
- 主視窗因部分位於螢幕外，座標輸入未確認命中；改由原生 UIA 最大化按鈕取得完整 1920×1032 畫面，控制與清單完整可見。低音量調整同步至 Mini，440 Hz 樣本的真實頻譜顯示，關閉後兩個視窗皆隱藏；證據 candidate3-spectrum-on.png、candidate3-spectrum-off.png。
- 主視窗暫停後 Mini 同步為播放按鈕。拖曳 seek 揭露時鐘顯示回零；在 control_test 增加暫停時位置必須為 1.5 秒且不恢復播放的檢查，修正前明確失敗為 0（paused-seek-before.txt）。
- reset 現在將請求位置發布至 position_frames，避免必須等 callback 消費才更新；修正後控制測試通過，underruns=0（paused-seek-after.jsonl）。14 項單元測試及 clippy 通過，debug 已重建。兩項修正仍待 GUI 回歸。
- candidate3 GUI 現在暫停、頻譜關閉，保持視窗供後續回歸；未對使用者其他程式發送輸入。
- 讀取診斷 session 16207 正常完成：300 秒、48 次載入、3 次掃描、零 underrun、暖機後記憶體增加 159744 bytes。此結果未定位原 failure。
- paused clock 修正後再跑 300 次 seek 壓測，underruns=0，證據 seek-stress-paused-clock-fix.jsonl。
- 啟動最新 release 建置，成功後銜接 7200 秒具開檔／讀取耗時紀錄的 attempt3，報告 verification/soak-attempt3-instrumented.jsonl。這是新一輪完整測試，不覆寫或把 attempt2 的 9 次中斷當成通過；原 attempt2 至 4260 秒仍為 9。

## 2026-10-23 — GUI 診斷、暫停 seek 回歸及隱藏主視窗修正

- 上一輪有路徑別名與暫停 seek 修正，屬進展。attempt3 建置成功並於 session 96246 持續執行；至 600 秒仍零 underrun，最大開檔 107933 us、讀取 54266 us。音訊長測未含下述 UI 程式庫修正。
- 加入 opt-in EPLAYER_GUI_REPORT：不存在環境變數時不建立檔案／執行緒；每秒 snapshot 透過容量 8 的 try_send 背景寫入，create_new 防覆寫，退出時排空。用隔離資料 verification/generated/gui-telemetry，原驗收資料不變。
- 暫停 seek GUI 回歸通過：主視窗約 5 秒拖至 22.8 秒，主／Mini 同時顯示 22 秒及對應歌詞，保持暫停；Mini 再反向拖至約 7 秒，歌詞立即切換。證據 paused-seek-gui-fixed.png、gui-telemetry.jsonl。
- 新診斷揭露原生 root 隱藏後主迴圈不再處理更新：gui-telemetry.jsonl 停於 137036 ms，浮動更新及 Ctrl+Alt+L 還原均停滯。不能把先前截圖推論為隱藏功能完整通過。
- 在固定 eframe 0.31.1 的本機副本 vendor/eframe/src/native/run.rs 補上 hidden Windows scheduled redraw 直接執行 update，保留原可見視窗流程。Cargo.toml 以 path patch 固定，保留原授權與 EPLAYER-PATCH.md；package.ps1 也收錄本機依賴授權，不再跳過所有 source=null 套件。
- 第一版 patch 因在既有 event-loop context 再建立 guard 而 panic，session 78344 已退出；保留 gui-hidden-fixed.jsonl，沒有宣稱通過。第二版檢查已有 context，避免巢狀 guard；GUI session 18406 為目前存活版本。
- 第二版實測：main_hidden=true 時 25 筆、24.746 秒，FFT 分析固定 1188，output_frames 持續增加、歌詞持續改變，underruns=0；Ctrl+Alt+L 成功還原，FFT 恢復增加。證據 gui-hidden-fixed2.jsonl、hidden-root-check.json、hidden-root-restored.png。
- 14 個單元測試通過，最終 patch clippy 通過。candidate3 不含本輪 GUI 診斷／路徑別名／暫停 seek／eframe 隱藏修正，不能作最終交付。
- 目前 GUI root HWND 16123844、Mini 17369104、Lyrics 526644，資料目錄 gui-telemetry，靜音播放並已恢復主視窗；仍需手動 LRC 路徑別名 GUI 回歸、清單編輯、穿透／透明度、系統匣選單、完整效能與最終打包。
- 補充輪詢：舊 attempt2 在 4380 秒累計增至 13 次 underrun，5280 秒仍為 13；仍不通過。具耗時診斷的 attempt3 保留獨立報告，需以其實際結果追查，不以兩輪混算。

## 2026-10-23 — 原生浮動視窗不透明度修正

- 上一輪完成隱藏更新修正及實際回歸，屬進展。本輪重新輪詢 attempt3 session 96246，960 秒零 underrun；程序持續執行。
- 在 0.4 不透明度下，Mini 呈現實心灰色。先以 root GL transparent=true 嘗試共用 alpha 設定，但移至主視窗文字上方後仍遮住底層文字，此嘗試已撤回，不作為最終方案。
- 改用 SetLayeredWindowAttributes(LWA_ALPHA)，僅枚舉目前 UI 執行緒的 EPlayer Mini／Lyrics 視窗，保留其他 extended styles（包含滑鼠穿透）。相同 alpha 與 style 不重設；以原生不透明度套用整個視窗，內容改為正常不透明繪製。
- Microsoft 文件：https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setlayeredwindowattributes 。操作文件同步改為視窗不透明度，文字／按鈕一起淡化。
- GUI 實測 Mini 可透出下方文字及主視窗；證據 overlay-opacity-before.png、overlay-opacity-after.png。14 個測試、clippy、debug 建置通過。Lyrics 的低不透明度及兩個穿透／恢復仍需實測。
- 桌面工具在最新 root UIA tree 顯示最大化 index 5 後，連續兩次回報 element 5 is not available in cached app state for eplayer.exe；已停止該操作，不將其視為應用程式功能通過。
- 目前 GUI session 69285，報告 gui-overlay-native-alpha.jsonl，隔離資料 gui-telemetry；Mini 不透明度 0.4，root 未播放，位置曾用內建重設並拖曳。主視窗與 Lyrics 仍存在。
- 最新 release 於 target/gui-release 另建，避免覆寫兩個長測執行檔；後續以 package.ps1 -ReleaseDirectory target/gui-release/release 封裝，會包含 vendor/eframe 的原授權。
- attempt3 最新 1560 秒（26 分鐘）零 underrun；release UI 建置 session 35418 仍在編譯 eplayer，不能宣稱完成。

## 2026-10-23 — 使用者授權桌面驗收時段

- 實際在浮動設定先啟用 Mini 與 Lyrics 滑鼠穿透，畫面確認兩個選項均勾選；按 Ctrl+Alt+L 後兩者均解除。此項證明快捷鍵恢復狀態，不替代點擊穿過視窗到底層控制項的驗收，後者仍待測。
- Lyrics 不透明度調為 0.400，畫面可見底層文字，原生透明效果成立；證據 verification/lyrics-native-opacity-040.png。驗收後恢復為 0.930，兩個穿透均關閉。
- attempt3 報告到 1860 秒零 underrun；attempt2 到 6240 秒累計 44 次，仍不通過。兩者不可混算。
- 最新 target/gui-release 的 release 編譯已成功完成（session 35418 exit 0），尚未完成新封裝驗證。

## 2026-10-23 — candidate5 免安裝回歸

- 上一輪有實際 GUI 驗證及紀錄更新，屬進展；本輪確認 session 96246 長測程序仍存活，未重新啟動。
- candidate4 驗收發現 eframe 發布 crate 根目錄未包含授權全文，先前「保留原授權」敘述不正確。已從上游 0.31.1 標籤取得 LICENSE-MIT／LICENSE-APACHE，修正 EPLAYER-PATCH.md，package.ps1 增加缺檔失敗檢查並附修正說明。candidate4 不作交付。
- candidate5 使用已成功編譯的 target/gui-release/release（本輪只改授權與封裝）；ZIP SHA256 10AC69DEA3491E0E458D4A4F70C416FE7BC13229F97FDCA6AA56E9574FFA1C53。
- 解壓至含中文／空白目錄，325 個 manifest 檔案雜湊全部一致，且確認 eframe 兩種授權與修改說明存在。PATH 僅保留 System32、移除 EPLAYER_MEDIA_TOOLS，13 個音訊／封面樣本均解碼成功。
- 同一解壓套件八項控制測試通過，underruns=0。證據 candidate5-package-check.json、candidate5-portable-formats.jsonl、candidate5-control.jsonl。
- 尚未以此套件完成全部 GUI／效能驗收；仍為候選版本，不能標記 1.0 完成。

## 2026-10-23 — 背景測試與十分鐘耐久門檻

- 使用者明確要求所有測試在背景執行，耐久測試改為 10 分鐘；已同步 PLAN 與 ACCEPTANCE，停止桌面輸入。診斷命令本來即接受 duration 參數，無須改動產品二進位。
- 重查兩個舊 session 均不存在；一般與提升權限程序清單亦無 eplayer，確認已停止才另啟新測試。attempt2 最終 failed／7200 秒／44 underruns；attempt3 最後 2940 秒零 underrun，沒有終態，記為中斷。
- candidate5 在沙盒啟動即退出（明確等待所得 exit -1073741816）；保留 soak-candidate5-10min.jsonl／stderr。改在可存取音訊裝置環境以 Hidden 啟動，靜音且不建立 GUI；session 1047，PID 7532，報告 soak-candidate5-10min-background.jsonl，600 秒，仍待終態。
- 上一輪 GUI 證據補記：改名與上移成功，candidate5 重啟仍保留；所有視窗隱藏後 FFT 固定 180，播放持續。15 秒 CPU 約 0.123%，工作集約 130 MiB。快捷鍵恢復呼叫中斷，不宣稱通過。

## 2026-10-23 — 正式版交付範圍確認與背景回歸

- 使用者已同意將實體媒體鍵、多螢幕／DPI、遊戲覆蓋、系統匣／穿透未完整實測列為限制，背景驗收通過後交付。已更新 PR、使用說明與驗收報告；封裝加入驗收報告。
- 版本更新為 1.0.0，無產品功能原始碼修改。正式 release 建置成功；執行檔 SHA256 34EE29FDF6C0C9D071F48F7C539A233AF5A1C0035759C72EB6C20450DF3C689B。
- 14 項單元測試在正常權限環境通過；初次沙盒測試 settings_replace_and_restart 在覆寫暫存設定時存取拒絕，保留此限制，不宣稱沙盒測試全數通過。Clippy -D warnings 與 fmt --check 通過。
- release-1.0-control-test.jsonl、release-1.0-seek-stress.jsonl、release-1.0-spectrum-test.jsonl、release-1.0-order-test.jsonl 均通過，underruns=0；release-1.0-gapless-report.json 八組離線接縫測試通過。
- verify-gapless.py 新增 CREATE_NO_WINDOW，確保外部工具背景運行；新增 --report 指定獨立結果，保留歷史報告，不修改產品執行檔。
- 正式版耐久程序 PID 1000／session 89610，600 秒，report release-1.0-soak.jsonl。候選版 PID 7532／session 1047 仍獨立執行；兩者結果分開，不互相替代。

正式版背景補充：13 個格式／封面樣本及損壞／遺失來源處理通過（release-1.0-formats.jsonl、release-1.0-invalid.jsonl）。45 秒取樣平均總 CPU 0.1234%，峰值工作集 27787264 bytes，private bytes 5754880；這是無 GUI 的音訊診斷程序，不能與完整 GUI 工作集混用。正式耐久測試至 180 秒零 underrun，尚未完成。

## 2026-10-23 — 最終背景結果核對

- candidate5 十分鐘測試已正常退出（session 1047 exit 0），600 秒／109 次載入／5 次掃描，underruns=0，暖機後工作集增加 999424 bytes。
- release-1.0-library.jsonl：42 筆索引、兩輪掃描、重啟還原 42 筆，通過。CIM 查詢在受限環境遭拒，改用唯讀登錄與 Environment API 記錄 Windows 11 25H2 build 26200 x64、16 邏輯處理器。
- release-1.0-audit.json 已從實際 JSON 逐項 assert：13 格式、2 錯誤來源、8 控制、300 seek、4 模式、8 接縫、42 索引均通過；正式耐久與最終封裝欄位仍 pending，沒有提前宣稱完成。

## 2026-10-23 — 1.0 完成交付

- 正式版 session 89610 exit 0：600 秒／109 次載入／5 次掃描／零 underrun，暖機後工作集增加 770048 bytes，峰值工作集 27328512 bytes。
- dist/EPlayer-1.0.0-win-x64.zip 已產生，SHA256 CEADBCB5171D74A97FEA37C74A2CF74B234F36250921DFAFD9593FC3FD7D11A9。
- 解壓到中文／空白路徑，326 個 manifest 檔案雜湊一致；執行檔與通過背景測試者完全相同。移除開發工具 PATH 與 EPLAYER_MEDIA_TOOLS 後，13 個音訊／封面樣本再次通過。套件不含使用者資料或測試資料庫。
- release-1.0-audit.json 逐項核對背景回歸、耐久終態與套件檢查結果，status=passed_with_documented_user_accepted_limits。
- 交付包含執行檔、FFmpeg／ffprobe、使用說明、驗收報告、授權與 manifest；PR／PLAN／README 更新完成狀態。不包含 YouTube 或伺服器。
- Windows 10、媒體鍵、多螢幕／DPI、遊戲與未完整系統匣／穿透實測依使用者同意明列限制。其他未完整 GUI 回歸及乾淨 OS 限制也在文件如實列出，未冒稱驗證通過。

## 2026-10-23 — 1.0.1 月紫圖示與 UI

- 按使用者 icon.png 配色更新主視窗、浮動播放器、浮動歌詞、控制按鈕、選取狀態與頻譜，預設夜紫；淺色 preset 使用淡紫白／深紫文字。原有主題設定保留。
- Windows EXE 嵌入七尺寸 ICO（16 至 256），工作列／視窗／系統匣使用同一來源；主介面品牌及無封面預設圖改用原圖。執行期 128px RGBA 只解碼一次，品牌 GPU 紋理一次建立，主題僅切換時套用。
- build.rs 用既有 Windows SDK rc.exe 編譯資源；可用 RC 指定 SDK 工具。原始 icon.png 未更改，不引入伺服器或音訊功能變動。
- fmt、clippy -D warnings、14 個既有單元測試及 release 通過；PE 資源實際解析確認七尺寸圖示存在（release-1.0.1-icons.json）。ZIP manifest 與建置執行檔一致（release-1.0.1-package-check.json）。
- dist/EPlayer-1.0.1-win-x64.zip 已交付；依背景測試要求未開啟 GUI，未宣稱視覺實機驗收完成。1.0 耐久測試不重標為 1.0.1。

## 2026-10-24 — 1.0.2 視覺驗收與對齊修正

- 使用者明確允許視覺驗收，使用獨立 verification/visual-qa 設定與靜音測試 WAV，沒有改動使用者的正式資料。
- 發現 1.0.1 一般按鈕太矮、列距太密、浮動標題未保留控制空間。修正為一致文字大小、28px 最小互動高度、32px 播放控制，主視窗內距 16、標題列內距 12；主視窗至少 680×560。
- 長標題、長清單文字與獨立歌詞使用省略及 hover 全文；完整／精簡浮動各自設最小尺寸；完整浮動空封面採品牌圖。工具列可換行，等化器預設寬度 720，窄視窗可水平捲動。
- 實際畫面確認：800px 深淺主視窗、680px 窄主視窗、完整／精簡浮動、長歌詞、浮動設定、一般與窄等化器。截圖 final-main-dark.png、final-main-light.png、final-equalizer-light.png、main-dark-narrow.png、full-dark.png、lyrics-dark.png、eq-narrow-scroll.png 均位於 verification/visual-qa。
- 窄視窗測試以 Windows 原生「大小」選單及滑鼠完成；最初兩次邊界拖曳沒有改變大小，不計為驗證通過。
- 初次單元測試發現尺寸替換誤改既有測試 fixture 的 440×300，已還原原 fixture 與原斷言，14 項重新通過；未修改拖曳演算法。Clippy、fmt、release 建置通過。
- 本輪視覺變更未改音訊核心，不將先前十分鐘耐久結果冒稱為本版新測試。多螢幕／不同 DPI／遊戲相容性限制仍保留。


## 2026-10-24：PR-003／1.1.0

- 整合使用者九項浮動／主視窗需求至 PR-003，按該計劃實作。
- 新增 widgets 模組：固定 32×32 圓形操作鍵、向量播放／暫停／切歌／設定／返回／視窗控制；以向量月亮與鉛筆取代缺字字型。
- 浮動歌名用裁切跑馬燈與單一提示，溢出時才以約 30 FPS 更新；2 秒停留、30 px/s、尾端 1.5 秒後回到開頭。
- 精簡模式高 60，完整模式保留既有內容；返回／關閉浮動恢復主視窗，進入浮動自動隱藏主視窗。設定使用獨立 viewport，含捲動內容區。
- 主視窗左側可調分隔線、清單名稱與操作分列，右側保留播放區；原生標題列替換為自訂控制。
- 真實桌面觀察修正設定底部裁切、部分字型缺字及清單工具列不整齊換行。使用獨立 verification/visual-qa 資料，未改使用者正在執行的 1.0.2 設定。
- 15 項測試及 clippy 無警告；圖像證據、限制見 ACCEPTANCE.md。沒有新增伺服器或 YouTube 功能。


## 2026-10-24：PR-004／1.1.1

- 未選曲文字改為「未選取任何音樂」。
- 根因：把觀察到的 OS 位置反向餵給 viewport builder，與自製拖曳的移動命令競爭；延遲座標重播測試先失敗，修正後通過。
- 初始化位置與持續位置觀察分離；正常拖曳使用 StartDrag，跨畫面或單畫面快速放開、原生未移動時僅補一次位移。PointerMoved 本身不再產生移動命令。
- 捕捉桌面事件後以真實序列補強回歸，正反快速拖曳確認可移動且位置穩定。16 tests、clippy、release 及獨立公開來源建置核對完成。
- 公開內容包含自有來源、文件、圖示、Cargo.lock 及必要 eframe 修補與授權；排除音樂、設定、資料庫、原始 verification、target 和 dist。FFmpeg 工具隨 release ZIP 附授權與來源連結。
- 發布目標：公開 alan52013141314/EPlayer，Git 作者採 GitHub noreply 地址。
