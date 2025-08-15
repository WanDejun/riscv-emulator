// use crate::{config::arch_config::WordType, isa::riscv32::csr_reg::CsrReg};

// pub struct Mstatus {
//     data: *mut WordType,
// }
// impl From<*mut WordType> for Mstatus {
//     fn from(value: *mut WordType) -> Self {
//         Self { data: value }
//     }
// }
// impl CsrReg for Mstatus {
//     fn get_index() -> usize {
//         0x300
//     }
// }
// impl Mstatus {
//     pub fn get_wpri(&self) -> WordType {
//         ((unsafe {self.data.read_volatile()}) & (WordType::from(1u8) << 0)) >> 0
//     }
//     pub fn set_wrpi(&mut self, val: WordType) {
//         assert!(val < (1 << 1));
//         let mut data = unsafe {self.data.read_volatile()};
//         data &= !(WordType::from(1u8) << 0);
//         unsafe {self.data.write_volatile(data | (val << 0))}
//     }

//     pub fn get_sie(&self) -> WordType {
//         ((unsafe {self.data.read_volatile()}) & (WordType::from(1u8) << 1)) >> 1
//     }
//     pub fn set_sie(&mut self, val: WordType) {
//         assert!(val < (1 << 1));
//         let mut data = unsafe {self.data.read_volatile()};
//         data &= !(WordType::from(1u8) << 1);
//         unsafe {self.data.write_volatile(data | (val << 1))}
//     }
// }
