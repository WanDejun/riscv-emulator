use crate::{config::arch_config::WordType, isa::riscv::csr_reg::csr_macro::*};

impl Minstret {
    pub fn wrapping_add(&self, rhs: WordType) {
        let v = self.get_minstret() + rhs;
        self.set_minstret(v);
    }
}
