//! 允許值固定的字串列舉（設定、資料庫欄位、CLI 參數共用）。

/// 定義允許值的字串列舉：`as_str`、不分大小寫的 `parse`、`ALL`。
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name { $($variant),+ }

        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self { $($name::$variant => $text),+ }
            }

            pub fn parse(value: &str) -> Option<Self> {
                let v = value.trim();
                Self::ALL.iter().copied().find(|x| x.as_str().eq_ignore_ascii_case(v))
            }

            /// 以「、」串起所有允許值，用於錯誤訊息。
            pub fn allowed() -> String {
                Self::ALL.iter().map(|x| x.as_str()).collect::<Vec<_>>().join("、")
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}
