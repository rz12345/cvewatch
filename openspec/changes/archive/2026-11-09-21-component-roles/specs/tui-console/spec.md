## ADDED Requirements

### Requirement: TUI component roles
TUI 資產詳情的元件表格 SHALL 在第一欄顯示每個元件所屬組的名稱（角色中文名稱，套件元件為「套件」），並 SHALL 依 asset-inventory 的 Component grouping by role 的組順序排列，同組的元件相鄰且維持原本順序。其餘欄位（元件、版本、來源、更新時間）SHALL 不變。

#### Scenario: Role column in the asset detail
- **WHEN** 使用者在資產畫面開啟有 Ubuntu、iDRAC9 兩個清冊元件與一個 dpkg 套件元件的 `SRV-01`
- **THEN** 元件表格的第一欄標題為「角色」，各列依序為「BMC」（iDRAC9）、「作業系統」（Ubuntu）、「套件」（該套件）

#### Scenario: Single firmware component
- **WHEN** 使用者開啟只有 PAN-OS 元件的 `FW-EDGE`
- **THEN** 元件表格只有一列，角色欄為「韌體」
