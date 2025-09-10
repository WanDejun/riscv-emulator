pub mod csr_macro;

use std::{cmp::Ordering, collections::HashMap};

use crate::{config::arch_config::WordType, isa::riscv::csr_reg::csr_macro::CSR_REG_TABLE};

#[rustfmt::skip]
#[allow(non_upper_case_globals, unused)]
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
    U = 0,
    S = 1,
    V = 2,
    M = 3,
}

#[rustfmt::skip]
const CSR_PRIVILEGE_TABLE: &[(WordType, PrivilegeLevel)] = &[
    // READ WRITE.
    (0x000, PrivilegeLevel::U), // 0x0FF
    (0x100, PrivilegeLevel::S), // 0x1FF
    (0x200, PrivilegeLevel::V), // 0x2FF
    (0x300, PrivilegeLevel::M), // 0x3FF

    (0x400, PrivilegeLevel::U), // 0x4FF
    (0x500, PrivilegeLevel::S), // 0x57F
    (0x580, PrivilegeLevel::S), // 0x5BF
    (0x5C0, PrivilegeLevel::S), // 0x5FF (Custom)
    (0x600, PrivilegeLevel::V), // 0x67F
    (0x680, PrivilegeLevel::V), // 0x6BF
    (0x6C0, PrivilegeLevel::V), // 0x6FF (Custom)
    (0x700, PrivilegeLevel::M), // 0x77F
    (0x780, PrivilegeLevel::M), // 0x7BF
    (0x7A0, PrivilegeLevel::M), // 0x7FF
    (0x7B0, PrivilegeLevel::M), // 0x7BF (Debug-mode-only)
    (0x7C0, PrivilegeLevel::M), // 0x7FF (Custom)

    (0x800, PrivilegeLevel::U), // 0x8FF (Custom)
    (0x900, PrivilegeLevel::S), // 0x97F
    (0x980, PrivilegeLevel::S), // 0x9BF
    (0x9C0, PrivilegeLevel::S), // 0x9FF (Custom)
    (0xA00, PrivilegeLevel::V), // 0xA7F
    (0xA80, PrivilegeLevel::V), // 0xABF
    (0xAC0, PrivilegeLevel::V), // 0xAFF (Custom)
    (0xB00, PrivilegeLevel::M), // 0xB7F
    (0xB80, PrivilegeLevel::M), // 0xBBF
    (0xBC0, PrivilegeLevel::M), // 0xBFF (Custom)

    // READ ONLY.
    (0xC00, PrivilegeLevel::U), // 0xC7F
    (0xC80, PrivilegeLevel::U), // 0xCBF
    (0xCC0, PrivilegeLevel::U), // 0xCFF (Custom)
    (0xD00, PrivilegeLevel::S), // 0xD7F
    (0xD80, PrivilegeLevel::S), // 0xD8F
    (0xDC0, PrivilegeLevel::S), // 0xDFF (Custom)
    (0xE00, PrivilegeLevel::V), // 0xE7F
    (0xE80, PrivilegeLevel::V), // 0xE8F
    (0xEC0, PrivilegeLevel::V), // 0xEFF (Custom)
    (0xF00, PrivilegeLevel::M), // 0xF7F
    (0xF80, PrivilegeLevel::M), // 0xF8F
    (0xFC0, PrivilegeLevel::M), // 0xFFF (Custom)
];

impl PrivilegeLevel {
    /// true for legal.
    /// false for illegal.
    pub fn read_check_privilege(self, csr_addr: WordType) -> bool {
        match CSR_PRIVILEGE_TABLE.binary_search_by(|&(k, _)| {
            if k > csr_addr {
                Ordering::Greater
            } else if k == csr_addr {
                Ordering::Equal
            } else {
                Ordering::Less
            }
        }) {
            Ok(i) => self >= CSR_PRIVILEGE_TABLE[i].1,
            Err(i) => self >= CSR_PRIVILEGE_TABLE[i - 1].1,
        }
    }

    /// true for legal.
    /// false for illegal.
    pub fn write_check_privilege(self, csr_addr: WordType) -> bool {
        if csr_addr >= 0xC00 {
            false
        } else {
            self.read_check_privilege(csr_addr)
        }
    }
}

impl From<u8> for PrivilegeLevel {
    fn from(value: u8) -> PrivilegeLevel {
        match value {
            0 => PrivilegeLevel::U,
            1 => PrivilegeLevel::S,
            2 => PrivilegeLevel::V,
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
    fn data(&self) -> WordType;
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
        if !self.get_current_privileged().read_check_privilege(addr) {
            return None;
        }
        self.read_uncheck_privilege(addr)
    }

    pub fn write(&mut self, addr: WordType, data: WordType) -> Option<()> {
        if !self.get_current_privileged().write_check_privilege(addr) {
            return None;
        }
        self.write_uncheck_privilege(addr, data);
        Some(())
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

    pub fn write_uncheck_privilege(&mut self, addr: WordType, data: WordType) {
        // Special-case fflags and frm; they are subfields of fcsr
        if addr == csr_index::fflags {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                *fcsr = (*fcsr & !0b11111) | (data & 0b11111);
            } // TODO: Raise error
        } else if addr == csr_index::frm {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                *fcsr = (*fcsr & !0b11100000) | ((data & 0b111) << 5);
            } // TODO: Raise error
        } else if addr == csr_index::fcsr {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                // Quoted from RISC-V manual:
                // "Bits 31—8 of the fcsr are reserved for other standard extensions. If these extensions are not present,
                // implementations shall ignore writes to these bits and supply a zero value when read."
                *fcsr = data & 0xFF;
            } // TODO: Raise error
        } else if addr == csr_index::misa {
            // Do nothing, "a value of zero can be returned to indicate the misa register has not been implemented"
            // TODO: Implement misa
        } else {
            if let Some(val) = self.table.get_mut(&addr) {
                *val = data
            } else {
                // TODO: Raise error
            }
        }
    }

    pub fn read_uncheck_privilege(&self, addr: WordType) -> Option<WordType> {
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

    pub fn get_by_type<T>(&mut self) -> Option<T>
    where
        T: CsrReg,
    {
        let val = self.table.get_mut(&T::get_index())?;
        Some(T::from(val as *mut u64))
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
    use crate::isa::riscv::csr_reg::{CsrRegFile, PrivilegeLevel, csr_index, csr_macro::*};

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

    #[test]
    fn test_read_privilege() {
        let mut reg = CsrRegFile::new();
        reg.set_current_privileged(PrivilegeLevel::M);
        assert!(reg.write(csr_index::mcause, 0xFEFE).is_some());
        assert_eq!(
            reg.read_uncheck_privilege(csr_index::mcause).unwrap(),
            0xFEFE
        );

        reg.set_current_privileged(PrivilegeLevel::S);
        assert!(reg.write(csr_index::mcause, 0xFEFE).is_none());
        assert_eq!(
            reg.read_uncheck_privilege(csr_index::mcause).unwrap(),
            0xFEFE
        );
    }
}
