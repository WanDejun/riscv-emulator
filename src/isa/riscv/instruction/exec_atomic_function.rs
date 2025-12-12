use std::sync::atomic;

use crate::{
    isa::riscv::{
        csr_reg::csr_macro::Minstret, executor::RVCPU, instruction::RVInstrInfo, trap::Exception,
    },
    utils::UnsignedInteger,
};

// ----------------------------------
// Atomic Memory Operation Traits
// ----------------------------------
pub(super) trait AMOTrait<T>
where
    T: UnsignedInteger,
{
    fn exec(a: &T::AtomicType, b: &T::AtomicType, order: atomic::Ordering) -> Result<T, Exception>;
}

pub(super) struct ExecAmoAdd {}
impl AMOTrait<u64> for ExecAmoAdd {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoAdd {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoAnd {}
impl AMOTrait<u64> for ExecAmoAnd {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoAnd {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoOr {}
impl AMOTrait<u64> for ExecAmoOr {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoOr {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoXor {}
impl AMOTrait<u64> for ExecAmoXor {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoXor {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoMax {}
impl AMOTrait<u64> for ExecAmoMax {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoMax {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoMin {}
impl AMOTrait<u64> for ExecAmoMin {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoMin {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoMaxU {}
impl AMOTrait<u64> for ExecAmoMaxU {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoMaxU {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoMinU {}
impl AMOTrait<u64> for ExecAmoMinU {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoMinU {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

pub(super) struct ExecAmoSwap {}
impl AMOTrait<u64> for ExecAmoSwap {
    fn exec(
        _lhs: &<u64 as UnsignedInteger>::AtomicType,
        _rhs: &<u64 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u64, Exception> {
        todo!()
    }
}
impl AMOTrait<u32> for ExecAmoSwap {
    fn exec(
        _lhs: &<u32 as UnsignedInteger>::AtomicType,
        _rhs: &<u32 as UnsignedInteger>::AtomicType,
        _oreer: atomic::Ordering,
    ) -> Result<u32, Exception> {
        todo!()
    }
}

// ----------------------------------
// Atomic Memory Operation executor
// ----------------------------------
fn get_amo_order(aq: bool, rl: bool) -> std::sync::atomic::Ordering {
    match (aq, rl) {
        (false, false) => std::sync::atomic::Ordering::Relaxed,
        (true, false) => std::sync::atomic::Ordering::Acquire,
        (false, true) => std::sync::atomic::Ordering::Release,
        (true, true) => std::sync::atomic::Ordering::AcqRel,
    }
}

/// could ONLY used in single hart.  
/// let t = mem[x[[rs1]]];  
/// x[[rd]] = t;
/// mem[x[[rs1]]] = t OP x[[rs2]];
pub(crate) fn exec_atomic_memory_operation<F, T>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    T: UnsignedInteger,
    F: AMOTrait<T>,
{
    if let RVInstrInfo::A {
        rs1,
        rs2,
        rd,
        aq,
        rl,
    } = info
    {
        let (val1, _) = cpu.reg_file.read(rs1, rs2);
        let order = get_amo_order(aq, rl);
        let res = cpu
            .memory
            .modify_mem_by(val1, 0, |l, r| F::exec(l, r, order))?
            .into();
        cpu.reg_file.write(rd, res);
    } else {
        panic!("Invalid RVInstrInfo for AMO instruction");
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    cpu.csr.get_by_type_existing::<Minstret>().wrapping_add(1);

    Ok(())
}
