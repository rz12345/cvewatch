## ADDED Requirements

### Requirement: Asset identity in the assets screen
TUI 資產詳情 SHALL 顯示廠牌、型號、序號、財產編號、主機名稱、管理介面位址與 IP（每筆「位址（用途）」，以「、」分隔），未填的欄位與沒有 IP 時 SHALL 顯示「-」。TUI MUST NOT 提供開啟管理介面位址或編修這些欄位的操作。

#### Scenario: Identity in the asset detail
- **WHEN** 使用者在資產畫面選取廠牌「Aruba」、型號 `XYZ-100`、IP 為 `192.0.2.1`（用途「管理」）與 `2001:db8::1`、沒有序號的 `FW-EDGE`
- **THEN** 詳情顯示廠牌 Aruba、型號 `XYZ-100`、序號 -，以及 IP `192.0.2.1（管理）、2001:db8::1`
