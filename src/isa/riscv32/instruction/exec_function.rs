use crate::{
    config::arch_config::WordType,
    device::Mem,
    isa::riscv32::{
        executor::RV32CPU,
        instruction::{Exception, RVInstrInfo},
    },
    utils::{TruncateTo, UnsignedInteger, sign_extend},
};

/// ExecTrait will generate operation result to `exec_xxx` function.
/// ExecTrait::exec only do calculate.
/// `exec_xxx` function interact with other mod in CPU.
pub(super) trait ExecTrait<T> {
    fn exec(a: WordType, b: WordType) -> T;
}

/// Process arithmetic instructions with `rs1`, (`rs2` or `imm`) and `rd` in RV32I.
///
/// # NOTE
///
/// Not sure about extended ISAs.
///
/// This will always do signed extension to `imm` as 12 bit.
pub(super) fn exec_arith<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: ExecTrait<Result<WordType, Exception>>,
{
    let (rd, rst) = match info {
        RVInstrInfo::R { rs1, rs2, rd } => {
            let (val1, val2) = cpu.reg_file.read(rs1, rs2);
            (rd, F::exec(val1, val2)?)
        }
        RVInstrInfo::I { rs1, rd, imm } => {
            let val1 = cpu.reg_file.read(rs1, 0).0;
            let simm = sign_extend(imm, 12);
            (rd, F::exec(val1, simm)?)
        }
        _ => std::unreachable!(),
    };

    cpu.reg_file.write(rd, rst);
    cpu.pc = cpu.pc.wrapping_add(4);

    Ok(())
}

pub(super) fn exec_branch<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: ExecTrait<bool>,
{
    if let RVInstrInfo::B { rs1, rs2, imm } = info {
        let (val1, val2) = cpu.reg_file.read(rs1, rs2);

        if F::exec(val1, val2) {
            cpu.pc = cpu.pc.wrapping_add(sign_extend(imm, 13));
        } else {
            cpu.pc = cpu.pc.wrapping_add(4);
        }
    } else {
        std::unreachable!();
    }

    Ok(())
}

pub(super) fn exec_load<T, const EXTEND: bool>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    T: UnsignedInteger,
{
    if let RVInstrInfo::I { rs1, rd, imm } = info {
        let val = cpu.reg_file.read(rs1, 0).0;
        let addr = val.wrapping_add(sign_extend(imm, 12));
        let mut data: WordType = cpu.memory.read::<T>(addr).into();
        if EXTEND {
            data = sign_extend(data, 12);
        }
        cpu.reg_file.write(rd, data);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_store<T>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    T: UnsignedInteger,
{
    if let RVInstrInfo::S { rs1, rs2, imm } = info {
        let (val1, val2) = cpu.reg_file.read(rs1, rs2);
        let addr = val1.wrapping_add(sign_extend(imm, 12));
        cpu.memory.write(addr, T::truncate_from(val2));
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub fn exec_todo<T>(_info: RVInstrInfo, _cpu: &mut RV32CPU) -> Result<(), Exception> {
    todo!();
}

// =============================================
//                  ExecTrait
// =============================================
pub(super) struct ExecAdd {}
impl ExecTrait<Result<WordType, Exception>> for ExecAdd {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_add(b))
    }
}

pub(super) struct ExecSub {}
impl ExecTrait<Result<WordType, Exception>> for ExecSub {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_sub(b))
    }
}

pub(super) struct ExecMulLow {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulLow {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_mul(b))
    }
}

pub(super) struct ExecMulHighSighed {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulHighSighed {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((((a as u64)
            .cast_signed()
            .wrapping_mul((b as u64).cast_signed()))
            >> 32) as WordType)
    }
}

pub(super) struct ExecMulHighUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulHighUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((u64::from(a).wrapping_mul(b as u64)) >> 32)
    }
}

pub(super) struct ExecMulHighSignedUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulHighSignedUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(((a as isize >> (size_of::<WordType>() >> 1))
            * ((b >> (size_of::<WordType>() >> 1)) as isize)) as WordType)
    }
}

pub(super) struct ExecDivSigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivSigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed() / b.cast_signed()) as WordType)
    }
}

pub(super) struct ExecDivUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a / b)
    }
}

pub(super) struct ExecRemSigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemSigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed() % b.cast_signed()) as WordType)
    }
}

pub(super) struct ExecRemUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a % b)
    }
}

pub(super) struct ExecSLL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSLL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a << b) // TODO: Do we need to check for shift amount and throw Invalid Instruction?
    }
}

pub(super) struct ExecSRL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a >> b)
    }
}

pub(super) struct ExecSRA {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRA {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed() >> b.cast_signed()).cast_unsigned())
    }
}

pub(super) struct ExecSLLW {}
impl ExecTrait<Result<WordType, Exception>> for ExecSLLW {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend((a << b).truncate_to(32), 32))
    }
}

pub(super) struct ExecSRLW {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRLW {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend((a >> b).truncate_to(32), 32))
    }
}

pub(super) struct ExecAnd {}
impl ExecTrait<Result<WordType, Exception>> for ExecAnd {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a & b)
    }
}

pub(super) struct ExecOr {}
impl ExecTrait<Result<WordType, Exception>> for ExecOr {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a | b)
    }
}

pub(super) struct ExecXor {}
impl ExecTrait<Result<WordType, Exception>> for ExecXor {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a ^ b)
    }
}

pub(super) struct ExecSignedLess {}
impl ExecTrait<bool> for ExecSignedLess {
    fn exec(a: WordType, b: WordType) -> bool {
        a.cast_signed() < b.cast_signed()
    }
}
impl ExecTrait<Result<WordType, Exception>> for ExecSignedLess {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed() < b.cast_signed()) as WordType)
    }
}

pub(super) struct ExecUnsignedLess {}
impl ExecTrait<bool> for ExecUnsignedLess {
    fn exec(a: WordType, b: WordType) -> bool {
        a < b
    }
}
impl ExecTrait<Result<WordType, Exception>> for ExecUnsignedLess {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a < b) as WordType)
    }
}

pub(super) struct ExecEqual {}
impl ExecTrait<bool> for ExecEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a == b
    }
}

pub(super) struct ExecNotEqual {}
impl ExecTrait<bool> for ExecNotEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a != b
    }
}

pub(super) struct ExecSignedGreatEqual {}
impl ExecTrait<bool> for ExecSignedGreatEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a.cast_signed() >= b.cast_signed()
    }
}

pub(super) struct ExecUnsignedGreatEqual {}
impl ExecTrait<bool> for ExecUnsignedGreatEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a >= b
    }
}

pub(super) struct ExecNothing {}
impl ExecTrait<bool> for ExecNothing {
    fn exec(_: WordType, _: WordType) -> bool {
        todo!()
    }
}
impl ExecTrait<Result<WordType, Exception>> for ExecNothing {
    fn exec(_: WordType, _: WordType) -> Result<WordType, Exception> {
        todo!()
    }
}
