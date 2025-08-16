pub mod csr_macro;
pub mod machine_mode;

use std::collections::HashMap;

use crate::{
    config::arch_config::WordType,
    isa::riscv::csr_reg::csr_macro::{CSR_REG_TABLE, UniversalCsr},
};

const CSR_SIZE: usize = 8;

#[rustfmt::skip]
#[allow(non_upper_case_globals)]
mod csr_index {
    use crate::config::arch_config::WordType;

    pub const mstatus   : WordType  = 0x300;    // CPU 状态寄存器，控制中断使能、特权级别
    pub const misa      : WordType  = 0x301;    // ISA 特性寄存器，标明 CPU 支持的指令集扩展
    pub const mie       : WordType  = 0x304;    // 机器中断使能寄存器
    pub const mtvec     : WordType  = 0x305;    // 异常/中断向量基址
    pub const mscratch  : WordType  = 0x340;    // 痕迹寄存器, 一般用于在模式切换时, 保存寄存器信息(如栈信息等)
    pub const mepc      : WordType  = 0x341;    // 异常返回地址
    pub const mcause    : WordType  = 0x342;    // 异常原因
    pub const mtval     : WordType  = 0x343;    // 异常附加信息（例如非法访问地址）
    pub const mip       : WordType  = 0x344;    // 中断挂起寄存器
    // pub const mhartid   : WordType  = 0xF14;    // CPU hart ID（多核情况下）
}

pub trait CsrReg: From<*mut WordType> {
    fn get_index() -> WordType;
    fn clear_by_mask(&mut self, mask: WordType);
    fn set_by_mask(&mut self, mask: WordType);
}

pub struct CsrRegFile {
    table: HashMap<WordType, WordType>,
}

impl CsrRegFile {
    pub fn new() -> Self {
        Self::from(CSR_REG_TABLE)
    }

    pub fn from(csr_table: &[(WordType, WordType)]) -> Self {
        let mut table = HashMap::new();
        for (addr, default_value) in csr_table.iter() {
            table.insert(*addr, *default_value);
        }
        Self { table }
    }

    pub fn read(&self, addr: WordType) -> Option<WordType> {
        self.table.get(&addr).copied()
    }

    pub fn write(&mut self, addr: WordType, data: WordType) {
        if let Some(val) = self.table.get_mut(&addr) {
            *val = data
        }
    }

    pub fn get_by_type<T>(&mut self) -> T
    where
        T: CsrReg,
    {
        let val = self.table.get_mut(&T::get_index()).unwrap();
        T::from(val as *mut u64)
    }

    pub fn get<'a>(&'a mut self, addr: WordType) -> UniversalCsr {
        let val = self.table.get_mut(&addr).unwrap();
        UniversalCsr::from(val as *mut u64)
    }
}

#[cfg(test)]
mod test {
    use crate::isa::riscv::csr_reg::{CsrRegFile, csr_index, csr_macro::*};

    #[test]
    fn test_rw_by_addr() {
        let mut reg = CsrRegFile::new();
        reg.write(csr_index::mcause, 3);
        reg.write(csr_index::mepc, 0x1234_5678);

        let mcause = reg.read(csr_index::mcause).unwrap();
        let mepc = reg.read(csr_index::mepc).unwrap();

        assert_eq!(mcause, 3);
        assert_eq!(mepc, 0x1234_5678);
    }

    #[test]
    fn test_rw_by_type() {
        let mut reg = CsrRegFile::new();
        let mstatus = reg.get_by_type::<Mstatus>();

        mstatus.set_mpp(3);
        mstatus.set_sie(1);

        assert_eq!(mstatus.get_mpp(), 3);
        assert_eq!(mstatus.get_sie(), 1);
        let mstatus_val = reg.read(csr_index::mstatus).unwrap();
        assert_eq!(mstatus_val, (1 << 1 | 3 << 11));

        mstatus.set_mpp(0b10);
        assert_eq!(mstatus.get_mpp(), 0b10);
    }
}
