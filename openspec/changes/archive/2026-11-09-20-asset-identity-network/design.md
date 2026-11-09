# Design

## Context

- 資產欄位存在 `asset` 表，對應 `AssetRecord`（`model.rs`）。驗證集中在 `record.rs` 的 `validate_record`／`validate_import_record`（清冊匯入與畫面編修共用），ISMS 管理欄位的解析另放在 `record_admin.rs`（16 design D2、D8）。`record.rs` 目前 729 行，`edit.rs` 745 行，`frontend/src/asset_form/mod.rs` 788 行，都接近 800 行上限
- 欄位錯誤以 `FieldError { field: StandardField, kind: FieldErrorKind, message }` 回報；frontend 依 `(kind, field)` 換成介面用語（`asset_form::field_message`），不解析 core 的訊息文字
- 清冊：`columns.rs` 定義 `StandardField`、`ASSET_COLUMNS`（18 欄）、`KEEPABLE_FIELDS`（7 欄）與 `SheetKind`（Assets、Components、Systems）。`sheet.rs` 讀 xlsx 的 `assets`／`components`／`systems` 工作表；CSV 只有 assets 檔，以及 `--components` 指定的元件檔。`validate.rs` 的 `attach_components` 依 `asset_tag` 把元件列掛到資產上，`asset_tag` 不在 assets 工作表時回報列錯誤
- 缺欄保留：`Inventory.kept_fields` 由 `plan_systems.rs` 的 `keep_existing` 套用，`kept_columns_notice` 把缺少的欄合併成一則提醒（isms_id 另有原文案）
- ISMS 編號唯一：部分唯一索引 `idx_asset_isms_id (isms_id COLLATE NOCASE)`。匯入時以 `SeenValues` 檢查清冊內的重複，以 `isms_owners` 檢查與清冊以外資產的衝突，寫入前先執行 `release_changed_isms_ids`（處理互換）；編修以 `ensure_isms_id_free` 檢查，frontend 以 `isms_conflict` 在寫入前把錯誤標在欄位旁
- ISMS 匯入（18）：`isms/conversion.rs` 的 `convert_asset` 對既有資產沿用 asset_tag、name、曝露度、location、notes，其餘欄位取自 ISMS 清冊（`..c.record.clone()`）；`isms/import.rs` 的 `to_inventory` 以 `Inventory::default()` 補其他欄位（`kept_fields` 為空）
- 開啟連結：frontend `Workspace::open_url` 先檢查 `LinkPolicy`，再呼叫 `services.opener`（`SystemOpener` 用 frontend 已相依的 `open` crate；測試以 `test_support` 替身取代）。GUI 設為 `LinkPolicy::HttpsOnly`，因為參考連結來自外部 CVE 資料。gui crate 沒有直接相依 `open`
- GUI 資產清單（`screens/asset_list.rs`）在左側 Panel 內，依資產說明分組，每組是一個以代碼為 id 的 `CollapsingHeader`；篩選條件為 frontend 的 `AssetListFilter { include_retired, pending_only, class }`
- 最後一個 migration 是 `0008_isms_admin.sql`；19-system-code-edit 不新增 migration

## Goals / Non-Goals

**Goals:**

- 識別欄位與 IP 沿用現有的驗證路徑：清冊匯入、畫面編修與 frontend 本地驗證共用 core 的規則與錯誤種類
- 匯出後再匯入結果不變（含 `ips` 工作表）；舊清冊與 ISMS 匯入都不會清掉這些資料
- 不新增外部相依

**Non-Goals:**

- 不正規化廠牌與型號的寫法（例如大小寫、空白）。22-asset-lifecycle-eol 以「廠牌＋型號」對應硬體 EoL 時再決定比對方式；23-isms-export 直接使用使用者輸入的文字
- 不改變「修改資產欄位就記錄比對待補」的既有行為（只改識別欄位時仍會觸發一次比對，結果不變）
- 不建立 IP 或主機名稱的跨資產索引

## Decisions

### D1. 資料模型與 migration `0009_asset_identity.sql`

```sql
ALTER TABLE asset ADD COLUMN brand TEXT;
ALTER TABLE asset ADD COLUMN model TEXT;
ALTER TABLE asset ADD COLUMN serial_number TEXT;
ALTER TABLE asset ADD COLUMN property_number TEXT;
ALTER TABLE asset ADD COLUMN hostname TEXT;
ALTER TABLE asset ADD COLUMN management_url TEXT;
CREATE UNIQUE INDEX idx_asset_property_number
    ON asset (property_number COLLATE NOCASE) WHERE property_number IS NOT NULL;

CREATE TABLE asset_ip (
    id       INTEGER PRIMARY KEY,
    asset_id INTEGER NOT NULL REFERENCES asset (id) ON DELETE CASCADE,
    address  TEXT    NOT NULL,   -- 標準寫法（IpAddr 的 Display）
    purpose  TEXT,
    UNIQUE (asset_id, address)
);
```

- 6 個識別欄位加進 `AssetRecord`，都是 `Option<String>`。`ASSET_COLUMNS` 跟著加入，讀寫集中在 `repo.rs`。以 `ALTER TABLE` 加欄，不重建 `asset` 表（13 design D7 的限制）
- IP 不放進 `AssetRecord`：它是清單，和元件一樣另存一張表。`AssetRecord` 會在匯入差異與畫面判斷時整筆比較，放進清單會讓這些比較與 `ImportPlan` 的欄位差異變複雜。IP 的讀寫由新模組 `assets/ips.rs` 的 `AssetIpRepo` 負責（`for_asset`、`all_by_asset`、`replace(asset_id, &[AssetIp])`）。順序以 `id` 遞增表示，`replace` 先刪除再依輸入順序寫入
- 永久刪除資產時，`ON DELETE CASCADE` 會一併刪除該資產的 IP，`purge_asset` 不必修改
- 既有資產升級後，6 欄都是 NULL、沒有 IP，不需要轉換資料
- 清冊欄名：`brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url`；`ips` 工作表為 `asset_tag`、`ip`、`purpose`。廠牌用 `brand` 而不用 `vendor`：產品目錄已有「廠商（vendor）」，指的是軟體／韌體的發行者，與設備廠牌不一定相同
- `assets` 工作表欄序：…、`personal_data`、`brand`、`model`、`serial_number`、`property_number`、`hostname`、`management_url`、`location`、`notes`（共 24 欄）

### D2. 欄位驗證：`record_identity.rs`

新檔 `assets/record_identity.rs` 負責解析識別欄位，`record.rs` 只增加 `RecordInput` 的 6 個欄位、`value()` 的對應與呼叫，維持在 800 行以下：

- 廠牌、型號、序號、財產編號：沿用 `optional_text`（去除前後空白，空字串為 None）
- 主機名稱：只接受 ASCII 英數字、`-`、`.`、`_`，長度 1–253，保留大小寫。規則刻意寬鬆，只擋明顯貼錯的值（網址、含空白的多個名稱、IP 清單、`:` 連接埠），不套用 RFC 1123 的逐段規則（每段 63 字元、不得以 `-` 開頭等）。設備實際的主機名稱常有底線（Windows／NetBIOS 名稱），嚴格規則會拒絕真實資料。非 ASCII 字元也拒絕，因為設備主機名稱實務上都是 ASCII，中文多半是誤填了名稱
- 管理介面位址：以 core 已相依的 `url` crate 解析，要求 scheme 為 `http` 或 `https`、有 host、原文不含空白或控制字元（同 frontend `links::is_safe_url`），並以去除前後空白後的原文儲存。不存 `Url::to_string()`，因為它會補上結尾的 `/`，造成匯出再匯入時出現差異
  - 含 userinfo（`username()` 非空或 `password()` 為 Some）時拒絕：帳號密碼只能放在 Credential Manager，不得寫進資料庫或匯出的清冊
  - 沒有 scheme（例如 `192.0.2.1`）時不自動補 `https://`，避免猜錯協定或連接埠
- 新增 `FieldErrorKind::Format`（主機名稱、管理介面位址的格式不符）與 `FieldErrorKind::Credentials`（網址含帳號或密碼）。core 的訊息使用清冊欄名（例如「hostname「https://fw-edge」只能包含英數字、-、. 與 _」），frontend 依種類換成介面用語
- 財產編號的唯一性要查資料庫，不放在 `validate_record`，見 D3

### D3. 財產編號唯一

比照 ISMS 編號（13）：

- 資料庫：部分唯一索引（D1），作為最後防線
- 清冊內：`SeenValues` 新增 `property_numbers`（`to_ascii_lowercase` 為鍵），錯誤為「第 5 列：財產編號「p-0001」與第 3 列重複」
- 與清冊以外的既有資產：`plan_identity.rs` 的 `property_owners`／`property_conflict` 比照 `isms_owners`，訊息由共用的 `property_in_use_message`（「財產編號 P-0002 已被 NAS-02 使用（該資產已退役）」）產生，匯入與編修共用
- 互換：`import.rs` 寫入前先執行 `release_changed_property_numbers`（`AssetRepo::clear_property_number`），比照 `release_changed_isms_ids`
- 編修與新增：`edit.rs` 新增 `ensure_property_number_free`（`AssetRepo::property_number_in_use`，排除自身），在交易內檢查
- frontend：`property_number_conflict(form, data)` 比照 `isms_conflict`，以快照先行檢查，讓錯誤在寫入前就標在欄位旁
- 比對大小寫：只比 ASCII 大小寫（與 SQLite `COLLATE NOCASE`、ISMS 編號一致），不做全形轉半形

### D4. IP 的型別與驗證：`assets/ips.rs`

```rust
pub struct AssetIp { pub address: IpAddr, pub purpose: Option<String> }
pub struct IpInput<'a> { pub address: &'a str, pub purpose: &'a str }   // 畫面與清冊的原文
pub struct IpError { pub index: usize, pub kind: IpErrorKind, pub message: String }
pub enum IpErrorKind { Missing, Invalid, Duplicate }
pub fn validate_ips(inputs: &[IpInput<'_>]) -> Result<Vec<AssetIp>, Vec<IpError>>;
```

- 解析：`IpAddr::from_str`（標準函式庫）。它不接受網段（`/24`）、位址範圍、zone ID（`%eth0`）、IPv4 各段的前導零（`192.000.002.010`）與方括號，符合 spec 的「單一位址」。拒絕時，若原文含 `/`，訊息另外說明只能填單一位址、不能填網段
- 儲存與顯示一律使用 `IpAddr` 的 Display（IPv6 為 RFC 5952 的小寫壓縮寫法），重複以 `IpAddr` 相等判斷，所以 `2001:DB8:0:0::1` 與 `2001:db8::1` 視為相同。IPv4-mapped IPv6（`::ffff:192.0.2.1`）與 `192.0.2.1` 視為不同位址，不另外轉換
- 用途：去除前後空白，空字串為 None
- 不同資產可有相同位址（HA 的虛擬 IP、NAT 後的相同私有位址、實驗網段都很常見），所以不做跨資產檢查，也不給提醒
- `index` 是輸入清單中的位置：畫面用來把錯誤標在 IP 列旁，清冊匯入則換成 `ips` 工作表的列號

### D5. `ips` 工作表與 CSV IP 檔

- `SheetKind::Ips`：必填 `asset_tag`、`ip`，選填 `purpose`；`StandardField` 新增 `Ip`、`Purpose`；`RowRef` 顯示為「ips 工作表第 N 列」。`ips` 工作表與 `systems` 工作表一樣**只認標準欄名**，不套用 `[import.columns]`（code review 修正：原本比照 `components` 套用自訂欄名，但使用者若已把舊清冊的「IP」欄對應到 `notes`，匯出檔 `ips` 工作表的 `ip` 表頭就會被解讀成 notes，匯出檔無法重匯；`ips` 工作表是本版新增，沒有既有的自訂欄名需要相容）
- `sheet.rs`：`Workbook.ips: Option<Sheet>`。`read_inventory(path, components_csv, ips_csv)` 新增 `ips_csv` 參數；Excel 清冊又指定 `--ips` 時，比照 `--components` 以錯誤結束（「--ips 只用於 CSV；Excel 請使用 ips 工作表」）
- 新檔 `validate_ips.rs`：逐列讀取，比照 `attach_components` 依 `asset_tag` 分組；`asset_tag` 不在 assets 工作表時回報列錯誤（該資產列本身有錯時不重複提示）。每個資產的 IP 交給 `validate_ips`，錯誤換成列號
- `Inventory.ips: IpsInput`：

  ```rust
  pub enum IpsInput {
      /// 不處理 IP，也不提醒（Default；ISMS 匯入使用）
      #[default] Untouched,
      /// 清冊沒有 ips 工作表（或 CSV 沒有 --ips）：保留既有 IP，需要時提醒
      Missing,
      /// asset_tag → 該資產的 IP（依列序）；清冊中沒有列出的資產為空清單
      Sheet(BTreeMap<String, Vec<AssetIp>>),
  }
  ```

  `parse_inventory` 依 `Workbook.ips` 設為 `Missing` 或 `Sheet`。`Untouched` 設為 Default，所以 ISMS 匯入的 `to_inventory`（`..Inventory::default()`）不必修改就不會碰 IP
- 提醒：`Missing` 且這次清冊中有既有資產原本就有 IP 時，加入 `IPS_KEPT_NOTICE`「清冊沒有 ips 工作表，保留資料庫中既有資產的 IP」（整批一次，比照 `ISMS_KEPT_NOTICE` 只在真的有值被保留時提醒）。CSV 的使用說明另外註明以 `--ips` 指定
- CSV 作法比照 `--components`，不另設檔名慣例，也不讓 CSV 清冊在 assets 檔用欄位寫多筆 IP。一欄多值需要另訂分隔符號，也會與 xlsx 的格式不一致

### D6. 匯入計畫與寫入

- `CurrentAsset` 新增 `ips: Vec<AssetIp>`，`load_snapshot` 以 `AssetIpRepo::all_by_asset` 一次讀出
- `PlannedAsset` 新增 `ips: Option<Vec<AssetIp>>`：`Sheet` 時為該資產的清單（沒有列出時為空），其他情況為 None（不寫入）
- `AssetUpdate` 新增 `ip_change: Option<IpChange { from: Vec<String>, to: Vec<String> }>`，以「位址（用途）」的有序清單比較。順序也算在內：匯出依儲存順序寫出，所以往返不會產生差異。有 IP 變更的資產列為更新；一般匯入預覽的欄位清單（`field_names`）加上 `ip`，明細行列出新舊位址。IP 不影響比對，所以 `affects_matching` 不變
- 寫入：`apply_asset` 在 `planned.ips` 為 Some 時呼叫 `AssetIpRepo::replace`；新增的資產先 insert 再寫 IP
- 新邏輯（財產編號衝突、IP 差異、IP 提醒）放在新檔 `plan_identity.rs`，避免 `plan.rs`（479 行）繼續變大

### D7. 缺少識別欄位：沿用 `kept_fields`

- `KEEPABLE_FIELDS` 加入 6 個識別欄位，`keep_existing` 對這些欄位比照辦理。`kept_columns_notice` 原本就會列出所有缺少的可保留欄位（isms_id 除外），所以 16 格式的清冊會得到一則「清冊沒有 brand、model、serial_number、property_number、hostname、management_url 欄，保留資料庫中既有資產的值」。更早的清冊會在同一則提醒列出更多欄位
- 舊清冊沒有 `property_number` 欄時，保留的值不會與清冊內其他列衝突；與清冊以外資產的衝突仍照常檢查（保留的值本來就唯一）

### D8. ISMS 匯入不碰識別欄位

- `convert_asset` 對既有資產，把 6 個識別欄位從 `e.record` 複製過來（與 asset_tag、name、location 等沿用欄位放在一起）；新資產則為 None
- IP：`to_inventory` 維持 `IpsInput::Untouched`（Default），不處理 IP，也不會給提醒
- 不以 `kept_fields` 處理：那樣會讓 `kept_columns_notice` 在 ISMS 報表出現「清冊沒有 brand… 欄」，與 spec 的 ISMS import keeps identity fields 不符
- 23-isms-export 會讀這些欄位寫出 ISMS「資產描述」，但 ISMS 清冊匯入時仍不反向解析描述中的廠牌型號

### D9. 編修：`EditOp::SetIps` 與檔案拆分

- `EditOp::SetIps(Vec<IpInput<'a>>)`（位址、用途的原文）：在交易內以 `validate_ips` 驗證，錯誤合併成 `CoreError::Invalid`，再以 `AssetIpRepo::replace` 寫入。`affects_matching` 對它回傳 false
- `create_asset` 新增參數 `ips: &[IpInput<'_>]`（共 7 個參數，不觸發 clippy 的 `too_many_arguments`）
- 識別欄位跟著 `EditOp::Fields` 一起寫入（`update_record` 寫 6 個新欄位）；財產編號在 `update_fields` 與 `create_asset` 內以 `ensure_property_number_free` 檢查
- `edit.rs` 已 745 行：先把 `retire_asset`、`restore_asset`、`purge_preview`、`purge_asset` 與它們的私有函式原樣移到 `assets/edit_lifecycle.rs`，由 `edit` 重新匯出（呼叫端不變），再加入新功能

### D10. frontend

- `asset_form/mod.rs` 已 788 行：先把 `FormErrors`、`field_label`、`field_message` 與錯誤用語常數原樣移到 `asset_form/errors.rs`（由 `asset_form` 重新匯出），識別欄位與 IP 草稿放在新檔 `asset_form/identity.rs`
- `AssetForm` 新增 6 個字串欄位與 `ips: Vec<IpDraft { address, purpose }>`。`from_entry` 帶入既有值，`record_input` 傳入新欄位。`validate` 另外以 core 的 `validate_ips` 驗證非空白的 IP 列（完全空白的列忽略，儲存時也不送出）。錯誤放在新的 `FormErrors.ips: Vec<(usize, String)>`，以表單列的索引定位
- 介面用語：`(Format, Hostname)`「只能包含英數字、-、. 與 _」；`(Format, ManagementUrl)`「需以 http:// 或 https:// 開頭」；`Credentials`「帳號與密碼不得寫入管理介面位址」；IP 的 `Invalid`「不是有效的 IPv4 或 IPv6 位址」（含 `/` 時加上「只能填單一位址」）、`Duplicate`「與第 N 列重複」
- `workspace/assets.rs`：`fields_changed` 原本就比較整筆 `AssetRecord`，所以會涵蓋新欄位。IP 清單（略過空白列後的位址與用途）與 `entry.ips` 不同時，加入步驟「修改 IP」（`EditOp::SetIps`）；新增資產時把 IP 傳給 `create_asset`。只改 IP 時 `needs_match` 為 false
- `data.rs`：`AssetEntry.ips` 由 `AssetIpRepo::all_by_asset` 一次讀出。`AssetListFilter` 新增 `search: String`，`asset_list` 在現有篩選之後套用 `matches_asset_search`（放在 `filter.rs`）：
  - 查詢字去除前後空白後轉小寫；空字串不篩選
  - 依序比對編號、名稱、主機名稱、序號、財產編號與每筆 IP 的標準寫法（都轉小寫後以 `contains` 判斷）
  - 查詢字可以解析為 `IpAddr` 時，也比對位址是否相等，所以輸入 `2001:DB8:0::1` 也找得到 `2001:db8::1`
  - 資產數量是數百筆等級，每個畫面幀重新計算即可，不另外快取搜尋鍵
- `display.rs`：`identity_rows(record, ips) -> Vec<(&'static str, String)>`（廠牌、型號、序號、財產編號、主機名稱、管理介面位址、IP；未填為「-」）與 `ip_text(&AssetIp)`（「192.0.2.1（管理）」），GUI、TUI 共用；CLI 另外只列出有值的項目
- `workspace/links.rs` 新增 `open_management_url(url)`：只檢查 `links::is_safe_url`（http 或 https、不含空白與控制字元），不套用 `LinkPolicy`，再呼叫 `services.opener`。GUI 的 `HttpsOnly` 是用來防範外部 CVE 資料中的連結，管理介面位址則是使用者自己輸入、且已通過 D2 驗證的資料；許多設備的管理介面只提供 http，若套用 `HttpsOnly`，spec 要求的 http 連結就無法開啟

### D11. GUI

- 資產對話框：新檔 `dialogs/asset_identity.rs` 繪製「識別與網路」區塊，作為一般欄位、ISMS 管理之後的第三欄（`horizontal_top` 內再加一個 `vertical`，寬度 `FIELD_WIDTH`）。放在下方會讓對話框變高，元件區被捲出可視範圍（16 apply 決定的同一個理由）。IP 清單每列為「IP」（寬 160）、「用途」（寬 120）與「移除」，列下方顯示錯誤；按「新增 IP」後焦點移到新列的 IP 欄位。`asset_edit.rs` 只增加呼叫，維持在 800 行以下
- 資產詳情：在管理欄位之後加入識別欄位與 IP。管理介面位址用 `ui.link(url)`，點擊後呼叫 `GuiApp::open_management_link` → `Workspace::open_management_url`，結果顯示在狀態列；滑鼠提示顯示完整網址。不用 `egui::Hyperlink`：它會送出 egui 的 `OpenUrl` 輸出，由 eframe 自行開啟，既不經過 frontend 的 `Opener`（測試替身無法攔截），也不會在狀態列回報結果。gui crate 也因此不需要新增 `open` 相依
- 搜尋框：`GuiApp.assets_search: String`，放在左側 Panel 頂端（`TextEdit::singleline`，提示文字「搜尋編號、名稱、主機名稱、IP、序號、財產編號」，右側有「清除」按鈕），存入 `AssetListFilter.search`。搜尋字不為空時，各組改用 `id_salt(("asset-search", group.key()))` 並設 `default_open(true)`，所以搜尋中一律展開；原本以代碼為 id 的收合狀態不受影響，清除搜尋後自然恢復。沒有結果時顯示「沒有符合「{查詢字}」的資產」，與「尚無資產」的訊息分開。搜尋框取得焦點時，Enter／Delete 不會觸發清單操作（`assets_keys` 已在有焦點時略過）。選取中的資產被篩掉時，選取與詳情保持不變（與收合組的現有行為一致），但 Enter／Delete 不作用：清單中看不到的資產不以按鍵編輯或退役（code review 修正）
- 匯入對話框：CSV 清冊在「元件檔」下方新增「IP 檔：…」與「選擇 IP 檔」按鈕；`ImportArgs.ips` 轉成 `--ips`；`FileDialogs` 新增 `open_ips`（.csv 篩選），測試替身同步實作

### D12. TUI 與 CLI

- TUI 資產詳情：在「資產說明 … 保管人 …」之後加兩行：「廠牌 X  型號 Y  序號 Z  財產編號 W」與「主機名稱 H  管理介面 U  IP a（用途）、b」，未填為「-」。只顯示，不提供開啟或編修
- CLI `asset show`：在 `admin_lines` 之後輸出 `identity_lines`，每個有值的欄位一行（「廠牌：Aruba」「管理介面：https://192.0.2.1/」），每筆 IP 一行（「IP：192.0.2.1（管理）」）。以純函式產生，用單元測試驗證（CLI 整合測試不能開啟加密資料庫，16 的既有作法）
- `cvewatch asset import` 新增 `--ips <CSV>`（說明「CSV 匯入時的 IP 檔（欄位 asset_tag、ip、purpose）」）；匯入報表的提醒與預覽明細沿用現有輸出。`asset template` 完成後的說明文字加入 `ips` 工作表

### D13. 範本與匯出

- 範本 `assets` 的範例列加上虛構值：`FW-01` 廠牌 `Palo Alto Networks`、型號 `XYZ-100`、序號 `SN0001`、財產編號 `P-0001`、主機名稱 `fw01`、管理介面 `https://192.0.2.1/`；其他範例列留空。`ips` 工作表的範例列：`FW-01`、`192.0.2.1`、`管理介面`
- 匯出：`assets` 寫出 6 欄（未填為空白），`ips` 依資產編號、儲存順序寫出（沒有 IP 時只有表頭）。`InventoryRows` 新增 `ips`
- 範本與匯出都寫 `ips` 工作表，因此「匯出 → 編輯 → 匯入」時，IP 一律以檔案內容為準

### D14. 測試與 fixture

- core：migration 測試 `tests/db_migration_identity.rs`（從 0008 狀態升級：既有資產的欄位為 NULL、沒有 IP、命中數不變；永久刪除會連帶刪除 IP）；`record_identity.rs`、`ips.rs` 的單元測試；匯入測試 `tests/assets_import_identity.rs`（spec 的 ips 工作表、財產編號、缺欄保留、CSV `--ips`）；匯出往返（含 IP）
- `gen_inventory_fixtures.rs` 新增 `inventory_identity.xlsx`（含 `ips` 工作表與識別欄位）與 `inventory_ips.csv`；既有 fixture 不改，繼續作為「沒有識別欄位的舊清冊」使用。資料一律使用虛構值（`FW-EDGE`、`SRV-01`、192.0.2.x、2001:db8::、`XYZ-100`、`P-0001`），commit 前執行 `scripts/check-sensitive.ps1`
- frontend：表單驗證（IP 列錯誤、空白列忽略、財產編號衝突）、`matches_asset_search`、編修步驟
- GUI：`egui_kittest` 重現 gui-console 的 scenario；連結點擊以 `test_support` 的 Opener 替身確認收到的網址
- TUI：畫面測試斷言詳情的識別行

## Risks / Trade-offs

- [16 格式之前的舊清冊提醒變長：一則提醒列出最多 12 欄] → 仍只有一則；文件說明可以匯出一次新格式的清冊來消除提醒
- [使用者既有的 `[import.columns]` 把「財產編號」對應到 `asset_tag`] → 行為不變；文件說明若要分開記錄，可改為對應到 `property_number`
- [IPv6 以標準寫法儲存，與使用者輸入的寫法不同] → 預覽與詳情都顯示標準寫法；搜尋時若輸入完整位址，會以位址相等比對
- [`ips` 工作表會清空「列在 assets 工作表、卻沒有 IP 列」的資產的 IP] → 與 `components` 工作表的覆寫行為一致；預覽列出 IP 變更，GUI 的覆寫提醒涵蓋這種情況
- [主機名稱規則會拒絕含其他字元的既有名稱] → 錯誤訊息列出允許的字元，使用者可以把原文寫進備註
- [GUI 以 http 開啟管理介面，與視窗版「只開 https」的連結規則不一致] → 只限使用者輸入、且已驗證的管理介面位址；外部資料的連結仍只開 https（見文末「與既有規則的差異」）
- [對話框多一欄後變寬，視窗較窄時需要水平捲動] → Workflow follow-up 列出手動目視檢查；必要時改為可收合區塊
- [`EditOp::Fields` 只改識別欄位也會觸發比對] → 結果不變，只多一次背景比對；維持既有的簡單規則

## Migration Plan

1. `0009_asset_identity.sql` 隨程式升級自動執行；加密資料庫的 migration 流程不變
2. 升級後，既有資產的識別欄位為空、沒有 IP。可以在 GUI 補上，或匯出清冊、補上新欄與 `ips` 工作表後再匯入
3. 回退：不提供降版 migration。舊版程式遇到較新的 schema 版本會拒絕開啟資料庫，要回退請從升級前的備份還原

## 與既有規則的差異

- GUI 會以 http 開啟管理介面位址（D10），與視窗版「參考連結只開 https」的規則不同。這是依需求「只接受 http／https 網址，點擊後以預設瀏覽器開啟」所做的決定；若之後改為只開 https，需要修改 gui-console 的 GUI asset identity detail scenario 與 D10

## Apply 決策（/opsx-auto）

apply 時依建議做法自行決定、未另行詢問的項目：

1. **`IpError` 多兩個欄位**：`first`（重複時先出現那筆的位置）與 `looks_like_network`（原文含 `/`）。frontend 據此產生「與第 N 列重複」與「只能填單一位址，不能填網段」，不必解析 core 的訊息文字（D4 只列出 `index`、`kind`、`message`）
2. **`FieldErrorKind::TooLong`**：主機名稱超過 253 字元時的種類，與字元不符的 `Format` 分開，畫面才能說明「最長 253 個字元」而不是「只能包含英數字…」
3. **管理介面位址的錯誤不帶出密碼**：`Credentials` 的訊息不含原文；`Format` 錯誤在原文含 `@` 時也不帶出原文（例如沒有 scheme 的 `admin:pw@192.0.2.1`）。另外要求原文以 `http://`／`https://` 開頭，因為 `url` crate 也接受 `http:192.0.2.1` 這種寫法
4. **清冊 `ips` 工作表的列錯誤**：`asset_tag` 或 `ip` 空白回報「缺少 …」；同資產重複位址為「ips 工作表第 5 列：NAS-01 的 ip「…」與第 4 列重複」；只有用途、沒有位址的畫面列回報「請填寫 IP」（`IpErrorKind::Missing`）
5. **CLI 匯入預覽的 IP 明細**：IP 有變更時，欄位變更列出 `ip`，同一行再加上「IP 舊清單 → 新清單」（沒有 IP 時為「-」）
6. **測試檔位置**：畫面編修的識別欄位與 IP 測試放在新檔 `crates/core/tests/asset_edit_identity.rs`，而不是任務 2.2／2.3 寫的 `tests/asset_edit.rs`（該檔已 1140 行）；GUI 測試放在 `crates/gui/tests/asset_identity.rs`（任務 6.1 指定）。frontend 的測試仍加在 `crates/frontend/tests/asset_edit.rs`
7. **frontend 檔案大小**：`asset_form/mod.rs` 加入 6 個欄位與 IP 後仍超過 700 行，另把 `used_by`、`rename_conflict`、`isms_conflict` 原樣移到 `asset_form/conflicts.rs`（由 `asset_form` 重新匯出，呼叫端不變），`mod.rs` 為 674 行
8. **GUI 的 IP 列與搜尋框標籤**：IP 列顯示「IP 1」「IP 2」…與「用途」標籤（同時作為輔助技術標籤），按鈕「移除」；移除一列後清除 IP 列錯誤（索引已不對應）。搜尋框左側有「搜尋」標籤，搜尋字為空時「清除」停用
9. **TUI 第二行的標籤**：與 GUI 共用 `display::identity_rows`，因此寫「管理介面位址」而非 D12 的「管理介面」；CLI `asset show` 仍為「管理介面：…」
10. **fixture 重新產生**：`gen_inventory_fixtures` 重新產生時既有 xlsx 只有內部時間戳記不同，已以 `git checkout` 還原，只新增 `inventory_identity.xlsx` 與 `inventory_ips.csv`（任務 3.7 的完成條件）
11. **`open_management_url` 拒絕時為 Warning**：「只能開啟 http:// 或 https:// 位址」，與 `open_url` 拒絕時的等級相同

## Code review 修正（/opsx-auto 一輪）

1. **`ips` 工作表只認標準欄名**：不套用 `[import.columns]`，避免舊的自訂欄名（例如把「IP」對應到 `notes`）讓匯出檔無法重匯（D5 已同步修改；`columns.rs` 的測試改為斷言自訂欄名不套用）
2. **TUI 識別欄位轉成單行**：清冊值可能含換行或控制字元，TUI 詳情的兩行改用 `sanitize`（`display::one_line`）
3. **被搜尋篩掉的選取資產**：詳情保留，但 Enter／Delete 不再作用（D11 已同步修改；GUI 測試 `keys_do_not_act_on_an_asset_hidden_by_the_search`）
4. **唯一欄位的邏輯合併**：ISMS 編號與財產編號共用新檔 `assets/unique.rs` 的 `UniqueField`（資料庫查詢 `AssetRepo::value_in_use`／`clear_value`、匯入的 `owners`／`conflict` 與寫入前的 `release_changed_unique_values`、編修的 `ensure_unique_free`）。D3 所列的 `property_number_in_use`、`clear_property_number`、`property_owners`、`property_conflict`、`release_changed_property_numbers`、`ensure_property_number_free` 都由這些共用函式取代；`isms_id_in_use` 保留為包裝（資產說明代碼修改使用）
5. **搜尋只正規化一次**：`filter::AssetSearch` 在篩選前處理查詢字（小寫、解析 IP），再逐一比對資產；`matches_asset_search` 保留為單一資產的便利函式
6. **重複程式碼**：`validate_ips.rs` 改用 `validate.rs` 的 `RowReader`；`record_identity.rs` 改用 `record.rs` 的 `optional_text`；匯入對話框的元件檔與 IP 檔共用 `SideFile` 的列與挑選流程
7. 其他：`filter.rs` 中錯置的 `desc_score` 文件註解歸位；`repo.rs` 的 `ASSET_COLUMNS` 字串補回換行接續

## 架構圖

不需更新：沒有 crate 增刪或 workspace 內部相依的改變，也沒有新的執行檔或外部整合。新增的 `asset_ip` 表與識別欄位屬資料庫 schema 與 core 內部模組；開啟管理介面位址沿用 frontend 既有的 Opener（系統瀏覽器），不是新的外部系統互動。
