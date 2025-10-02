#[macro_use]
mod validator;

pub mod csr_macro;

use std::{cmp::Ordering, collections::HashMap};

use crate::{
    config::arch_config::WordType,
    isa::riscv::csr_reg::{
        csr_macro::{CSR_REG_TABLE, Mstatus, Satp},
        validator::Validator,
    },
};

/// Constants in this module are not complete. Use `get_index` static method for each CSR type, like [`Mstatus::get_index`].
// TODO: Consider replace all uses of `csr_index` with the corresponding `CSRType::get_index`, 
// then remove the `csr_index` module.
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
pub enum PrivilegeLevel {
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

impl PrivilegeLevel {}

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

pub(crate) trait NamedCsrReg {
    fn new(data: *mut CsrReg, ctx: *mut CsrContext) -> Self;
    fn get_index() -> WordType;
    fn data(&self) -> WordType;
}

/// Write `value` to the bits specified by `mask`.
pub(crate) struct CsrWriteOp {
    mask: WordType,
}

impl CsrWriteOp {
    #[inline]
    fn new(mask: WordType) -> CsrWriteOp {
        CsrWriteOp { mask }
    }

    #[inline]
    fn new_write_all() -> CsrWriteOp {
        CsrWriteOp { mask: !0 }
    }

    #[inline]
    fn apply(&self, target: &mut WordType, value: WordType) {
        *target = self.get_new_value(*target, value);
    }

    #[inline]
    fn get_new_value(&self, old_value: WordType, value: WordType) -> WordType {
        (old_value & !self.mask) | (value & self.mask)
    }

    /// Merge two write operations into one.
    ///
    /// XXX: You'd better not to pass overlapped masks, but there's no check for this.
    #[inline]
    fn merge(&self, rhs: &CsrWriteOp) -> CsrWriteOp {
        CsrWriteOp {
            mask: self.mask | rhs.mask,
        }
    }
}

pub(crate) struct CsrContext {
    pub extension: WordType, // Used in `misa`
    pub xlen: u8,            // 32 or 64
}

impl CsrContext {
    fn new() -> CsrContext {
        CsrContext {
            extension: 0,
            xlen: 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct CsrReg {
    value: WordType,
    validator: Option<Validator>,
}

impl CsrReg {
    fn new(value: WordType, validator: Option<Validator>) -> CsrReg {
        CsrReg { value, validator }
    }

    fn value(&self) -> WordType {
        self.value
    }

    fn write(&mut self, new_value: WordType, context: &CsrContext) {
        if let Some(validator) = self.validator {
            let op = validator(new_value, context);
            op.apply(&mut self.value, new_value);
        } else {
            self.value = new_value;
        }
    }

    /// Write directly without any validation.
    fn write_directly(&mut self, new_value: WordType) {
        self.value = new_value;
    }
}

pub(crate) struct CsrRegFile {
    table: HashMap<WordType, CsrReg>,
    cpl: PrivilegeLevel, // current privileged level
    pub(super) ctx: CsrContext,
}

impl CsrRegFile {
    pub fn new() -> Self {
        Self::from(CSR_REG_TABLE)
    }

    pub fn from(csr_table: &[(WordType, WordType, Validator)]) -> Self {
        let mut table = HashMap::new();
        for (addr, default_value, validator) in csr_table.iter() {
            table.insert(*addr, CsrReg::new(*default_value, Some(*validator)));
        }

        Self {
            table,
            cpl: DEFAULT_PRIVILEGE_LEVEL,
            ctx: CsrContext::new(),
        }
    }

    fn is_read_priv_legal(&mut self, csr_addr: WordType) -> bool {
        if csr_addr == Satp::get_index() && self.get_by_type_existing::<Mstatus>().get_tvm() == 1 {
            return false;
        }

        match CSR_PRIVILEGE_TABLE.binary_search_by(|&(k, _)| {
            if k > csr_addr {
                Ordering::Greater
            } else if k == csr_addr {
                Ordering::Equal
            } else {
                Ordering::Less
            }
        }) {
            Ok(i) => self.privelege_level() >= CSR_PRIVILEGE_TABLE[i].1,
            Err(i) => self.privelege_level() >= CSR_PRIVILEGE_TABLE[i - 1].1,
        }
    }

    fn is_write_priv_legal(&mut self, csr_addr: WordType) -> bool {
        if csr_addr >= 0xC00 {
            false
        } else {
            self.is_read_priv_legal(csr_addr)
        }
    }

    #[must_use]
    pub fn read(&mut self, addr: WordType) -> Option<WordType> {
        if !self.is_read_priv_legal(addr) {
            return None;
        }
        self.read_uncheck_privilege(addr)
    }

    /// Write with privilege check and validation.
    ///
    /// XXX: In most cases, you should use [`RV32CPU::write_csr`] in `executor.rs` instead of this function directly,
    /// because writting to CSR may have other side-effects in CPU.
    ///
    /// [`RV32CPU::write_csr`]: crate::isa::riscv::executor::RV32CPU::write_csr
    ///
    /// TODO: Why use `Option<()>` instead of a simple `bool`? Fix `write_directly` below as well.
    #[must_use]
    pub(crate) fn write(&mut self, addr: WordType, data: WordType) -> Option<()> {
        if !self.is_write_priv_legal(addr) {
            return None;
        }
        self.write_uncheck_privilege(addr, data);
        Some(())
    }

    /// ONLY used in debugger. Read & write without side-effect.
    /// TODO: Consider remove this.
    pub fn debug(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType> {
        if let Some(reg) = self.table.get_mut(&addr) {
            let old = *reg;
            if let Some(new) = new_value {
                reg.write(new, &self.ctx);
            }
            Some(old.value)
        } else {
            None
        }
    }

    /// Write directly without any check or validation and have no other side effects.
    /// TODO: Some old code uses `write_uncheck_privilege` may need to be changed to use this function.
    #[must_use]
    pub fn write_directly(&mut self, addr: WordType, data: WordType) -> Option<()> {
        if let Some(reg) = self.table.get_mut(&addr) {
            reg.write_directly(data);
            Some(())
        } else {
            None
        }
    }

    /// Write without check for privilege level, but still have validation.
    /// If you want to write without any check or validation, use [`Self::write_directly`] instead.
    pub fn write_uncheck_privilege(&mut self, addr: WordType, data: WordType) {
        // Special-case fflags and frm, they have their own addr but are subfields of fcsr.
        if addr == csr_index::fflags {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                fcsr.value = (fcsr.value & !0b11111) | (data & 0b11111);
            } // TODO: Raise error
        } else if addr == csr_index::frm {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                fcsr.value = (fcsr.value & !0b11100000) | ((data & 0b111) << 5);
            } // TODO: Raise error
        } else if addr == csr_index::fcsr {
            if let Some(fcsr) = self.table.get_mut(&csr_index::fcsr) {
                // Quoted from RISC-V manual:
                // "Bits 31—8 of the fcsr are reserved for other standard extensions. If these extensions are not present,
                // implementations shall ignore writes to these bits and supply a zero value when read."
                fcsr.value = data & 0xFF;
            } // TODO: Raise error
        } else {
            if let Some(val) = self.table.get_mut(&addr) {
                val.write(data, &self.ctx);
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
                .map(|reg| reg.value & 0b11111)
        } else if addr == csr_index::frm {
            self.table
                .get(&csr_index::fcsr)
                .copied()
                .map(|reg| (reg.value >> 5) & 0b111)
        } else {
            self.table.get(&addr).copied().map(|reg| reg.value)
        }
    }

    /// NOTE: Given that this the CSR type is known, so this function won't check privilege level.
    /// If you ensure the CSR exists, use [`Self::get_by_type_existing`] instead for better performance.
    pub fn get_by_type<T>(&mut self) -> Option<T>
    where
        T: NamedCsrReg,
    {
        let reg = self.table.get_mut(&T::get_index())?;
        Some(NamedCsrReg::new(
            reg as *mut CsrReg,
            &mut self.ctx as *mut CsrContext,
        ))
    }

    /// Similar to `get_by_type`, but this function assumes the CSR definitely exists.
    ///
    /// - In debug builds, this function will panic if the CSR does not exist.
    /// - In release builds, this function will skip the runtime check for performance.
    /// TODO: Almost all old code can assumes the CSR exists, replace them with this function.
    pub fn get_by_type_existing<T>(&mut self) -> T
    where
        T: NamedCsrReg,
    {
        if cfg!(debug_assertions) {
            self.get_by_type::<T>().unwrap()
        } else {
            unsafe { self.get_by_type::<T>().unwrap_unchecked() }
        }
    }

    pub fn privelege_level(&self) -> PrivilegeLevel {
        self.cpl
    }

    pub fn set_current_privileged(&mut self, new_level: PrivilegeLevel) {
        log::debug!("Privilege level change: {:?} -> {:?}", self.cpl, new_level);
        self.cpl = new_level
    }
}

#[cfg(test)]
mod test {
    use crate::isa::riscv::csr_reg::{CsrRegFile, PrivilegeLevel, csr_index, csr_macro::*};

    #[test]
    fn test_rw_by_addr() {
        let mut reg = CsrRegFile::new();
        reg.write(csr_index::mcause, 3).unwrap();
        reg.write(csr_index::mepc, 0x1234_5678).unwrap();

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
        reg.write(csr_index::mtvec, 0x114514).unwrap();
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
