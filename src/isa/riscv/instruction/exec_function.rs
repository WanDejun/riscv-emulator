use std::{hint::unlikely, marker::PhantomData};

pub(super) use super::exec_float_function::*;

use crate::{
    config::arch_config::{SignedWordType, WordType},
    isa::riscv::{
        csr_reg::csr_index, executor::RV32CPU, instruction::RVInstrInfo, trap::Exception,
    },
    utils::{
        TruncateFrom, TruncateToBits, UnsignedInteger, sign_extend, sign_extend_u32,
        wrapping_add_as_signed,
    },
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
        let addr = wrapping_add_as_signed(val, sign_extend(imm, 12));
        let ret = cpu.memory.read::<T>(addr);

        match ret {
            Ok(data) => {
                let data_64: u64 = data.into();
                let mut data = data_64 as WordType;
                if EXTEND {
                    data = sign_extend(data, (size_of::<T>() as u32) * 8);
                }
                cpu.reg_file.write(rd, data);
            }
            Err(err) => {
                cpu.csr.write_uncheck_privilege(csr_index::mtval, addr);
                return Err(Exception::from_memory_err(err));
            }
        }
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
        let addr = wrapping_add_as_signed(val1, sign_extend(imm, 12));

        let ret = cpu.memory.write(addr, T::truncate_from(val2));
        if let Err(err) = ret {
            cpu.csr.write_uncheck_privilege(csr_index::mtval, addr);
            return Err(Exception::from_memory_err(err));
        }
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_csrw<const UIMM: bool>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception> {
    if let RVInstrInfo::I { rs1, rd, imm } = info {
        // read generate register.
        let new_val;
        if UIMM {
            new_val = rs1 as WordType;
        } else {
            new_val = cpu.reg_file.read(rs1, rs1).0;
        }

        // write generate register.
        if rd != 0 {
            let value = cpu.csr.read(imm).ok_or(Exception::IllegalInstruction)?;
            cpu.reg_file.write(rd, value);
        }

        // write csr.
        cpu.csr.write(imm, new_val);
    }

    cpu.pc = cpu.pc.wrapping_add(4);

    Ok(())
}

pub(super) fn exec_csr_bit<const SET: bool, const UIMM: bool>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception> {
    if let RVInstrInfo::I { rs1, rd, imm } = info {
        let rhs = if UIMM {
            rs1 as WordType
        } else {
            cpu.reg_file.read(rs1, rs1).0
        };

        let value = cpu.csr.read(imm).ok_or(Exception::IllegalInstruction)?;
        if rd != 0 || UIMM {
            cpu.reg_file.write(rd, value);
        }

        let data = if SET { value | rhs } else { value & !rhs };
        cpu.csr.write(imm, data);
    }

    cpu.pc = cpu.pc.wrapping_add(4);

    Ok(())
}

pub(super) fn exec_nop(_info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception> {
    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

// =============================================
//                  ExecTrait
// =============================================
// Arith
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

pub(super) struct ExecMulHighUnsigned<U> {
    phantom: PhantomData<U>,
}

#[cfg(feature = "riscv32")]
impl ExecTrait<Result<u32, Exception>> for ExecMulHighUnsigned<u32> {
    fn exec(a: u32, b: u32) -> Result<u32, Exception> {
        let a = a as u64;
        let b = b as u64;

        return Ok((a.wrapping_mul(b) >> 32) as u32);
    }
}

#[cfg(feature = "riscv64")]
impl ExecTrait<Result<u64, Exception>> for ExecMulHighUnsigned<u64> {
    fn exec(a: u64, b: u64) -> Result<u64, Exception> {
        let a = a as u128;
        let b = b as u128;

        return Ok((a.wrapping_mul(b) >> 64) as u64);
    }
}

// NOTE: This version is slow and deprecated.

// pub(super) struct ExecMulHighUnsigned {}
// impl ExecTrait<Result<WordType, Exception>> for ExecMulHighUnsigned {
//     fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
//         const XLEN: WordType = (size_of::<WordType>() << 3) as WordType;
//         const HALF_XLEN: WordType = XLEN >> 1;
//         const HALF_XLEN_MAX: WordType = WordType::MAX >> (XLEN >> 1);

//         let lhs_hi = a >> HALF_XLEN;
//         let lhs_lo = a & HALF_XLEN_MAX;
//         let rhs_hi = b >> HALF_XLEN;
//         let rhs_lo = b & HALF_XLEN_MAX;

//         // 4个部分
//         let p1 = lhs_hi * rhs_hi; // 高高
//         let p2 = lhs_hi * rhs_lo; // 高低
//         let p3 = lhs_lo * rhs_hi; // 低高
//         let p4 = lhs_lo * rhs_lo; // 低低

//         // 合并高位
//         let mid = (p2 & HALF_XLEN_MAX) + (p3 & HALF_XLEN_MAX) + (p4 >> HALF_XLEN);
//         let high = p1 + (p2 >> HALF_XLEN) + (p3 >> HALF_XLEN) + (mid >> HALF_XLEN);

//         Ok(high)
//     }
// }

pub(super) struct ExecMulHighSigned<U> {
    phantom: PhantomData<U>,
}

#[cfg(feature = "riscv32")]
impl ExecTrait<Result<u32, Exception>> for ExecMulHighSigned<u32> {
    fn exec(a: u32, b: u32) -> Result<u32, Exception> {
        let a = a as i32 as i64;
        let b = b as i32 as i64;

        return Ok((a.wrapping_mul(b) >> 32) as u32);
    }
}

#[cfg(feature = "riscv64")]
impl ExecTrait<Result<u64, Exception>> for ExecMulHighSigned<u64> {
    fn exec(a: u64, b: u64) -> Result<u64, Exception> {
        let a = a as i64 as i128;
        let b = b as i64 as i128;

        return Ok((a.wrapping_mul(b) >> 64) as u64);
    }
}

pub(super) struct ExecMulHighSignedUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulHighSignedUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        let lhs_neg = (a as SignedWordType) < 0;

        let high = ExecMulHighUnsigned::exec(a, b)?;

        if lhs_neg {
            // Let M = 1 << XLEN, given a *negative* integer `a` bewteen [-M/2, 0).
            // Then a * b = (a + M) * b - M * b.
            // Here, a + M equals to the unsigned reinterpretation of `a`.
            Ok(high.wrapping_sub(b))
        } else {
            Ok(high)
        }
    }
}

pub(super) struct ExecDivSigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivSigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(WordType::MAX);
        }
        Ok((a.cast_signed().wrapping_div(b.cast_signed())) as WordType)
    }
}

pub(super) struct ExecDivUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(WordType::MAX);
        }
        Ok(a.wrapping_div(b))
    }
}

pub(super) struct ExecRemSigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemSigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(a);
        }
        Ok((a.cast_signed().wrapping_rem(b.cast_signed())) as WordType)
    }
}

pub(super) struct ExecRemUnsigned {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemUnsigned {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(a);
        }
        Ok(a % b)
    }
}

// Arith word
pub(super) struct ExecAddw {}
impl ExecTrait<Result<WordType, Exception>> for ExecAddw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend(a.wrapping_add(b).truncate_to_bits(32), 32))
    }
}

pub(super) struct ExecSubw {}
impl ExecTrait<Result<WordType, Exception>> for ExecSubw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend(a.wrapping_sub(b).truncate_to_bits(32), 32))
    }
}

pub(super) struct ExecMulw {}
impl ExecTrait<Result<WordType, Exception>> for ExecMulw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend((a.wrapping_mul(b)).truncate_to_bits(32), 32))
    }
}

pub(super) struct ExecDivw {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        let [sa, sb] = [a, b].map(|x| u32::truncate_from(x).cast_signed());
        if unlikely(sb == 0) {
            return Ok(WordType::MAX);
        }

        Ok(sign_extend(
            (sa.wrapping_div(sb)).cast_unsigned() as WordType,
            32,
        ))
    }
}

pub(super) struct ExecRemw {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(a as WordType);
        }
        let [sa, sb] = [a, b].map(|x| u32::truncate_from(x).cast_signed());
        Ok(sign_extend(
            (sa.wrapping_rem(sb)).cast_unsigned() as WordType,
            32,
        ))
    }
}

pub(super) struct ExecDivuw {}
impl ExecTrait<Result<WordType, Exception>> for ExecDivuw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        let [sa, sb] = [a, b].map(|x| x.truncate_to_bits(32));
        if unlikely(sb == 0) {
            return Ok(WordType::MAX);
        }
        Ok(sign_extend((sa / sb) as WordType, 32))
    }
}

pub(super) struct ExecRemuw {}
impl ExecTrait<Result<WordType, Exception>> for ExecRemuw {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        if unlikely(b == 0) {
            return Ok(a);
        }
        let [sa, sb] = [a, b].map(|x| u32::truncate_from(x));
        Ok(sign_extend((sa % sb) as WordType, 32))
    }
}

// Bit
pub(super) struct ExecSLL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSLL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_shl(b as u32))
    }
}

pub(super) struct ExecSRL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_shr(b as u32))
    }
}

pub(super) struct ExecSRA {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRA {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed().wrapping_shr(b.cast_signed() as u32)).cast_unsigned())
    }
}

pub(super) struct ExecSLLW {}
impl ExecTrait<Result<WordType, Exception>> for ExecSLLW {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend_u32((a as u32).wrapping_shl((b & 0x1F) as u32)))
    }
}

pub(super) struct ExecSRLW {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRLW {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend_u32((a as u32).wrapping_shr(b as u32)))
    }
}

pub(super) struct ExecSRAW {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRAW {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(sign_extend_u32(
            (a.cast_signed() as i32).wrapping_shr(b as u32) as u32,
        ))
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

// Compare
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
