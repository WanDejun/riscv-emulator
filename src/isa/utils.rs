pub struct ISABuilder<Desc: Clone> {
    instructions: Vec<Desc>,
}

impl<Desc: Clone> ISABuilder<Desc> {
    pub fn new() -> Self {
        ISABuilder {
            instructions: Vec::new(),
        }
    }

    pub fn add(mut self, desc: &[Desc]) -> Self {
        self.instructions.extend_from_slice(desc);
        self
    }

    pub fn build(self) -> Vec<Desc> {
        self.instructions
    }
}

#[macro_export]
macro_rules! define_instr_enum {
    ($isa_name:ident, $($name:ident),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub key: u32,
    pub mask: u32,
}

impl DecodeMask {
    pub fn matches(&self, instr: u32) -> bool {
        (instr & self.mask) == self.key
    }
}

pub fn create_decode_mask(pattern: &'static str) -> DecodeMask {
    let mut len = 0;
    let mut key = 0 as u32;
    let mut mask = 0 as u32;

    for ch in pattern.chars() {
        match ch {
            '0' | '1' | '?' => {
                len += 1;

                key = (key << 1) | (ch == '1') as u32;
                mask = (mask << 1) | (ch != '?') as u32;
            }
            _ => {}
        }
    }

    assert!(len <= 32, "Pattern length exceeds 32 bits");
    assert!(len % 8 == 0, "Pattern length is not a multiple of 8");

    DecodeMask {
        key: key as u32,
        mask: mask as u32,
    }
}
