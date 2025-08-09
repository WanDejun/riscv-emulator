#[macro_export]
macro_rules! define_instr_enum {
    ($isa_name:ident, $($name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub enum $isa_name {
            $($name),*
        }

        impl $isa_name {
            pub fn name(&self) -> &'static str {
                match self {
                    $($isa_name::$name => stringify!($name)),*
                }
            }
        }
    }
}

// WARNING: these functions and macros are untested!

#[macro_export]
macro_rules! define_instr {
    ( $tot_instr_name:ident,
        $( $isa_name:ident, $isa_table_name:ident, {$(
                $name:ident {
                    pattern: $pattern:expr,
                    decode: $decode:expr,
                    execute: $execute:expr,
                }),* $(,)?
            }
        ),* $(,)?
    ) => {

        define_instr_enum!($tot_instr_name, $($($name,)*)*);

        $(
            pub const $isa_table_name: &[(DecodeMask, $tot_instr_name, DecodeFn, ExecuteFn)] = &[
                $(
                    (
                        create_decode_mask($pattern),
                        $tot_instr_name::$name,
                        $decode,
                        $execute,
                    )
                ),*
            ];
        )*
    };
}

pub struct DecodeMask {
    key: u64,
    mask: u64,
}

pub fn create_decode_mask(pattern: &'static str) -> DecodeMask {
    let mut len = 0;
    let mut key = 0 as u64;
    let mut mask = 0 as u64;

    for ch in pattern.chars() {
        match ch {
            '0' | '1' | '?' => {
                len += 1;

                key = (key << 1) | (ch == '1') as u64;
                mask = (mask << 1) | (ch != '?') as u64;
            }
            _ => {}
        }
    }

    assert!(len <= 64, "Pattern length exceeds 64 bits");
    assert!(len % 8 == 0, "Pattern length is not a multiple of 8");

    DecodeMask { key, mask }
}
