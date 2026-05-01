use crate::{
    config::arch_config::{WordType, XLEN},
    isa::riscv::{
        VECTOR_LEN,
        csr_reg::{NamedCsrReg, csr_macro::*},
    },
};

impl Minstret {
    pub fn wrapping_add(&self, rhs: WordType) {
        let v = self.get_minstret() + rhs;
        self.set_minstret(v);
    }
}

impl Vtype {
    // If new vtype is supported return Some(vl). Otherwise return None.
    pub fn vsetvl(&mut self, vtype: WordType) -> Option<WordType> {
        let new_vlmul = vtype & 0b111;
        let new_vsew = (vtype >> 3) & 0b111;

        let lmul_legal = new_vlmul != 0b100;
        let vsew_legal = new_vsew < 0b100;
        let illegal = !(lmul_legal & vsew_legal);
        self.set_data_directly(vtype | (illegal as WordType) << (XLEN - 1));

        if illegal {
            None
        } else {
            let part_vl = VECTOR_LEN as WordType >> (new_vsew as WordType + 3); // vlen / 8 / (vsew + 1)
            let vl = match new_vlmul {
                0b000 => part_vl,      // 1
                0b001 => part_vl << 1, // 2
                0b010 => part_vl << 2, // 4
                0b011 => part_vl << 3, // 8
                0b111 => part_vl >> 1, // 1 / 2
                0b110 => part_vl >> 2, // 1 / 4
                0b101 => part_vl >> 4, // 1 / 8
                _ => 0,
            };
            Some(vl)
        }
    }
}
