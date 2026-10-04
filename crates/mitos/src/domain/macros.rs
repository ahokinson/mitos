macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        enum $name:ident ($label:literal) {
            $( $(#[$variant_meta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $( $(#[$variant_meta])* #[serde(rename = $text)] $variant ),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $text ),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.pad(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::domain::ParseError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $( $text => Ok(Self::$variant), )+
                    other => Err($crate::domain::ParseError {
                        label: $label,
                        value: other.to_owned(),
                    }),
                }
            }
        }
    };
}
