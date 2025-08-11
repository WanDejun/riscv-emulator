use crate::{
    config::arch_config::WordType,
    device::Mem,
    isa::riscv32::{
        executor::RV32CPU,
        instruction::{Exception, RVInstrInfo},
    },
    utils::UnsignedInteger,
};

pub trait ExecTrait<T> {
    fn exec(a: WordType, b: WordType) -> T;
}

pub fn sign_extend(value: WordType, from_bits: u32) -> WordType {
    let sign_bit = (1u64 << (from_bits - 1)) as WordType;

    if (value & sign_bit) != 0 {
        let mask = (!0u64 ^ ((1u64 << from_bits) - 1)) as WordType;
        value | mask
    } else {
        value
    }
}

pub fn exec_arith<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
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

pub fn exec_branch<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
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

pub fn exec_load<T, const EXTEND: bool>(
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

pub fn exec_store<T>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
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

pub struct ExecAdd {}
impl ExecTrait<Result<WordType, Exception>> for ExecAdd {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_add(b))
    }
}

pub struct ExecSub {}
impl ExecTrait<Result<WordType, Exception>> for ExecSub {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a.wrapping_sub(b))
    }
}

pub struct ExecSLL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSLL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a << b)
    }
}

pub struct ExecSRL {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRL {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a >> b)
    }
}

pub struct ExecSRA {}
impl ExecTrait<Result<WordType, Exception>> for ExecSRA {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok((a.cast_signed() >> b.cast_signed()).cast_unsigned())
    }
}

pub struct ExecAnd {}
impl ExecTrait<Result<WordType, Exception>> for ExecAnd {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a & b)
    }
}

pub struct ExecOr {}
impl ExecTrait<Result<WordType, Exception>> for ExecOr {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a | b)
    }
}

pub struct ExecXor {}
impl ExecTrait<Result<WordType, Exception>> for ExecXor {
    fn exec(a: WordType, b: WordType) -> Result<WordType, Exception> {
        Ok(a ^ b)
    }
}

pub struct ExecSignedLess {}
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

pub struct ExecUnsignedLess {}
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

pub struct ExecEqual {}
impl ExecTrait<bool> for ExecEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a == b
    }
}

pub struct ExecNotEqual {}
impl ExecTrait<bool> for ExecNotEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a != b
    }
}

pub struct ExecSignedGreatEqual {}
impl ExecTrait<bool> for ExecSignedGreatEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a.cast_signed() >= b.cast_signed()
    }
}

pub struct ExecUnsignedGreatEqual {}
impl ExecTrait<bool> for ExecUnsignedGreatEqual {
    fn exec(a: WordType, b: WordType) -> bool {
        a >= b
    }
}

pub struct ExecNothing {}
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
