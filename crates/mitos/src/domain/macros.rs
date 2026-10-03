macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        enum $name:ident ($label:literal) {
            $( $(#[$variant_meta:meta])* $variant:ident => $text:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $name {
            $( $(#[$variant_meta])* $variant ),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self {
                    $( Self::$variant => $text ),+
                }
            }
        }

        impl std::str::FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                match value {
                    $( $text => Ok(Self::$variant), )+
                    other => anyhow::bail!(concat!("unknown ", $label, " {:?}"), other),
                }
            }
        }
    };
}
