# Spec Delta

## Purpose

讓使用者把主機的 OS 套件清單（SBOM 或 dpkg/rpm 輸出）匯入為帶 purl 的套件元件，作為套件層級漏洞比對的基礎。

## ADDED Requirements

### Requirement: Supported package list formats
系統 SHALL 接受 CycloneDX JSON、SPDX JSON、`dpkg-query -W` 與 `rpm -qa` 的文字輸出，並 SHALL 在未指定格式時自動判斷。

#### Scenario: Importing a syft CycloneDX file
- **WHEN** 使用者執行 `cvewatch asset import-packages WEB-01 web01.cdx.json`
- **THEN** 檔案中每個帶 purl 的元件成為 `WEB-01` 的套件元件

#### Scenario: Unknown format
- **WHEN** 檔案內容不屬於任何支援格式
- **THEN** 匯入失敗並列出支援的格式

### Requirement: Distribution is required for plain package lists
匯入 dpkg 或 rpm 文字輸出時 SHALL 要求 `--distro`，並 SHALL 依此產生含發行版的 purl。

#### Scenario: dpkg list without distro
- **WHEN** 匯入 dpkg 輸出但未提供 `--distro`
- **THEN** 匯入失敗並提示需要 `--distro`，例如 `debian-12`

#### Scenario: dpkg list with distro
- **WHEN** 匯入含 `openssl\t3.0.15-1~deb12u1` 的 dpkg 輸出，`--distro debian-12`
- **THEN** 產生元件 purl `pkg:deb/debian/openssl@3.0.15-1~deb12u1?distro=debian-12`

### Requirement: Package list replaces previous snapshot
同一資產的套件匯入 SHALL 整批取代先前的套件元件；版本變更 SHALL 寫入元件歷史。

#### Scenario: Package removed from host
- **WHEN** 新清單中已沒有 `libfoo`
- **THEN** `libfoo` 元件被刪除，其他套件依新版本更新

### Requirement: Package import targets an existing asset
套件匯入 SHALL 只接受已存在且為 active 的 `asset_tag`。

#### Scenario: Unknown asset tag
- **WHEN** 指定的 `asset_tag` 不存在
- **THEN** 匯入失敗並提示先以 `asset import` 建立資產
