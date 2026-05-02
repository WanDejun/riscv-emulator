use crate::{
    cpu::VectorRegFile,
    isa::riscv::vector::types::{VectorConfig, Vlmul, Vsew},
};

pub mod integer;
pub mod types;
pub const VLEN: usize = 128;
pub const VLEN_BYTE: usize = VLEN >> 3;

pub(super) struct Vector {
    config: VectorConfig,
    vector_regfile: VectorRegFile,
}

impl Vector {
    pub(super) fn new() -> Self {
        Self {
            config: VectorConfig::new(),
            vector_regfile: VectorRegFile::new(),
        }
    }

    #[inline(always)]
    fn set_config(&mut self, lmul_sew_ta_ma_vl: (Vlmul, Vsew, bool, bool, u16)) {
        (
            self.config.vlmul,
            self.config.vsew,
            self.config.tail_agnostic,
            self.config.mask_agnostic,
            self.config.vl,
        ) = lmul_sew_ta_ma_vl;
    }
}
