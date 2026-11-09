## MODIFIED Requirements

### Requirement: Asset and component model
系統 SHALL 以 `asset_tag` 唯一識別資產，每個資產 SHALL 可擁有多個元件，每個元件記錄產品與原始版本字串。

#### Scenario: Server with several tracked components
- **WHEN** 清冊中 `SRV-01` 的主要產品為 ESXi 8.0 U3c，components 工作表另列 iDRAC 7.10.30.00 與 BIOS 2.16.0
- **THEN** 系統建立一個資產與三個元件

### Requirement: Editing components
系統 SHALL 可對 active 資產新增目錄產品元件、修改元件版本、移除元件；一次儲存的多項編修 SHALL 在單一交易內完成，任何一項失敗時 MUST NOT 寫入其中任何一項。同一資產 MUST NOT 有兩個相同產品的元件；版本修改 SHALL 依 Version change history 記錄舊版本；移除後資產 SHALL 仍至少保留一個清冊元件或未辨識產品。主機套件元件 MUST NOT 被編修。

#### Scenario: Upgrading firmware on screen
- **WHEN** 將 `FW-01` 的 PAN-OS 版本由 `11.1.4-h7` 改為 `11.1.6`
- **THEN** 元件版本為 `11.1.6`，版本歷史中有一筆 `11.1.4-h7` 及變更時間

#### Scenario: Adding a duplicate product
- **WHEN** `SRV-01` 已有 iDRAC9 元件，再新增 iDRAC9 元件
- **THEN** 系統拒絕並說明該資產已有此產品，請改為修改版本

#### Scenario: Removing the last component
- **WHEN** `FW-01` 只剩一個元件且沒有未辨識產品，使用者移除該元件
- **THEN** 系統拒絕並說明資產至少需要一個元件，可改為刪除資產

#### Scenario: One failing change saves nothing
- **WHEN** 一次儲存 `FW-01` 的名稱變更與一個不在產品目錄中的元件
- **THEN** 系統拒絕並說明原因，名稱與元件都不變

#### Scenario: Version text the scheme cannot read
- **WHEN** 將 ESXi 元件版本改為 `8.0 latest`
- **THEN** 系統儲存該版本並顯示與匯入相同的版本格式警告

### Requirement: ISMS asset ID
每個資產 SHALL 可有選填的 ISMS 資產編號，格式 SHALL 為三碼英數字系統代碼、`-`、兩碼大類、`-`、三碼數字（例如 `SYS-HW-001`），大類 MUST 與資產的子類別大類相同。ISMS 編號 MUST NOT 與其他資產（含 retired，不分大小寫）重複；留空表示沒有 ISMS 編號。清冊沒有 `isms_id` 欄時，既有資產的 ISMS 編號 SHALL 保留，匯入報表 SHALL 給出一次提醒。`asset_tag` 仍是資產的識別。

#### Scenario: Valid ISMS ID
- **WHEN** 類別 `HW03` 的 `FW-01` 填寫 ISMS 編號 `SYS-HW-003`
- **THEN** 資產儲存成功，`asset_tag` 仍為 `FW-01`

#### Scenario: Major class mismatch
- **WHEN** 類別 `HW03` 的資產填寫 ISMS 編號 `SYS-SW-003`
- **THEN** 匯入或編修被拒絕，並指出 ISMS 編號的大類 SW 與資產類別 HW03 不符

#### Scenario: Malformed ISMS ID
- **WHEN** 某列 ISMS 編號填寫 `SYSHW001`
- **THEN** 匯入中止並指出該列 ISMS 編號格式應為「系統代碼-大類-流水號」，例如 `SYS-HW-001`

#### Scenario: Duplicate ISMS ID
- **WHEN** `NAS-01` 填寫 `sys-hw-005`，而 retired 的 `NAS-02` 已使用 `SYS-HW-005`
- **THEN** 系統拒絕並指出該 ISMS 編號已被 `NAS-02` 使用

#### Scenario: Old inventory without ISMS column
- **WHEN** `FW-01` 的 ISMS 編號為 `SYS-HW-003`，使用者重匯沒有 `isms_id` 欄的舊清冊
- **THEN** `FW-01` 的 ISMS 編號仍為 `SYS-HW-003`，報表有一筆「清冊沒有 isms_id 欄，保留資料庫中既有資產的 ISMS 編號」提醒
