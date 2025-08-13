pub mod csr_macro;
pub mod machine_mode;

use std::collections::HashMap;

use crate::{config::arch_config::WordType, isa::riscv32::csr_reg::csr_index::mstatus};

const CSR_SIZE: usize = 8;

#[rustfmt::skip]
#[allow(non_upper_case_globals)]
mod csr_index {
    pub const mstatus: usize    = 0x300;    // CPU 状态寄存器，控制中断使能、特权级别
    pub const misa: usize       = 0x301;    // ISA 特性寄存器，标明 CPU 支持的指令集扩展
    pub const mie: usize        = 0x304;    // 机器中断使能寄存器
    pub const mtvec: usize      = 0x305;    // 异常/中断向量基址
    pub const mepc: usize       = 0x341;    // 异常返回地址
    pub const mcause: usize     = 0x342;    // 异常原因
    pub const mtval: usize      = 0x343;    // 异常附加信息（例如非法访问地址）
    pub const mip: usize        = 0x344;    // 中断挂起寄存器
    pub const mhartid: usize    = 0xF14;    // CPU hart ID（多核情况下）
}

const TABLE: [(usize, WordType); 1] = [(mstatus, 1)];

pub trait CsrReg: From<*mut WordType> {
    fn get_index() -> usize;
}

pub struct CsrRegFile {
    table: HashMap<WordType, WordType>,
}

impl CsrRegFile {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
        }
    }
}
