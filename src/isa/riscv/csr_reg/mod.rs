pub mod csr_macro;

use std::collections::HashMap;

use crate::{
    config::arch_config::WordType,
    isa::riscv::csr_reg::csr_macro::{CSR_REG_TABLE, UniversalCsr},
};

#[rustfmt::skip]
#[allow(non_upper_case_globals)]
pub(crate) mod csr_index {
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

    // Floating-Point CSR
    pub const fflags    : WordType  = 0x001;
    pub const frm       : WordType  = 0x002;
    pub const fcsr      : WordType  = 0x003;
}

#[repr(u8)]
#[derive(Debug, PartialEq, PartialOrd, Ord, Eq, Clone, Copy)]
/// Only support machine_mode now.
pub(crate) enum PrivilegeLevel {
    // U = 0,
    // S = 1,
    // V = 2,
    M = 3,
}

impl From<u8> for PrivilegeLevel {
    fn from(value: u8) -> PrivilegeLevel {
        match value {
            // 0 => PrivilegeLevel::U,
            // 1 => PrivilegeLevel::S,
            // 2 => PrivilegeLevel::V,
            3 => PrivilegeLevel::M,
            _ => unreachable!("Invalid privilege level: {}", value),
        }
    }
}

const DEFAULT_PRIVILEGE_LEVEL: PrivilegeLevel = PrivilegeLevel::M;

pub(crate) trait CsrReg: From<*mut WordType> {
    fn get_index() -> WordType;
    fn clear_by_mask(&mut self, mask: WordType);
    fn set_by_mask(&mut self, mask: WordType);
}

pub(crate) struct CsrRegFile {
    table: HashMap<WordType, WordType>,
    cpl: PrivilegeLevel, // current privileged level
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
        Self {
            table,
            cpl: DEFAULT_PRIVILEGE_LEVEL,
        }
    }

    pub fn read(&self, addr: WordType) -> Option<WordType> {
        // Special-case fflags and frm; they are subfields of fcsr
        if addr == csr_index::fflags {
            self.table
                .get(&csr_index::fcsr)
                .copied()
                .map(|x| x & 0b11111)
        } else if addr == csr_index::frm {
            self.table
                .get(&csr_index::fcsr)
                .copied()
                .map(|x| (x >> 5) & 0b111)
        } else {
            self.table.get(&addr).copied()
        }
    }

    pub fn write(&mut self, addr: WordType, data: WordType) {
        // Special-case fflags and frm; they are subfields of fcsr
        if addr == csr_index::fflags {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                *fcsr = (*fcsr & !0b11111) | (data & 0b11111);
            } // TODO: Raise error
        } else if addr == csr_index::frm {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                *fcsr = (*fcsr & !0b11100000) | ((data & 0b111) << 5);
            } // TODO: Raise error
        }

        if let Some(val) = self.table.get_mut(&addr) {
            *val = data
        } else {
            // TODO: Raise error
        }
    }

    /// ONLY used in debugger. Read & write without side-effect.
    pub fn debug(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType> {
        if let Some(val) = self.table.get_mut(&addr) {
            let old = *val;
            if let Some(new) = new_value {
                *val = new;
            }
            Some(old)
        } else {
            None
        }
    }

    pub fn get_by_type<T>(&mut self) -> Option<T>
    where
        T: CsrReg,
    {
        let val = self.table.get_mut(&T::get_index())?;
        Some(T::from(val as *mut u64))
    }

    pub fn get<'a>(&'a mut self, addr: WordType) -> Option<UniversalCsr> {
        let val = self.table.get_mut(&addr)?;
        Some(UniversalCsr::from(val as *mut u64))
    }

    pub fn get_current_privileged(&self) -> PrivilegeLevel {
        self.cpl
    }

    pub fn set_current_privileged(&mut self, new_level: PrivilegeLevel) {
        self.cpl = new_level
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
        let mstatus = reg.get_by_type::<Mstatus>().unwrap();

        mstatus.set_mpp(3);
        mstatus.set_sie(1);

        assert_eq!(mstatus.get_mpp(), 3);
        assert_eq!(mstatus.get_sie(), 1);
        let mstatus_val = reg.read(csr_index::mstatus).unwrap();
        assert_eq!(mstatus_val, (1 << 1 | 3 << 11));

        mstatus.set_mpp(0b10);
        assert_eq!(mstatus.get_mpp(), 0b10);

        let mtvec = reg.get_by_type::<Mtvec>().unwrap();
        reg.write(csr_index::mtvec, 0x114514);
        assert_eq!(mtvec.get_base(), 0x114514 >> 2);
    }
}
