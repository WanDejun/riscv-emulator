use rustc_apfloat::{FloatConvert, Status};

use super::normal_float_exec;
use crate::{
    config::arch_config::WordType,
    fpu::{Round, soft_float::*},
    isa::riscv::{
        csr_reg::{csr_index, csr_macro::Fcsr},
        executor::RVCPU,
        instruction::RVInstrInfo,
        trap::Exception,
    },
    utils::{
        FloatPoint, TruncateTo, TruncateToBits, WordTrait, sign_extend, wrapping_add_as_signed,
    },
};

fn rm_to_round(cpu: &mut RVCPU, rm: u8) -> Round {
    match rm {
        0b000 => Round::NearestTiesToEven,
        0b001 => Round::TowardZero,
        0b010 => Round::TowardNegative,
        0b011 => Round::TowardPositive,
        0b111 => {
            let rm = cpu.csr.get_by_type_existing::<Fcsr>().get_rm();
            rm_to_round(cpu, rm as u8)
        }
        _ => {
            log::warn!(
                "Invalid rounding mode: {:03b}, falling back to Round::NearestTiesToEven",
                rm
            );
            Round::NearestTiesToEven
        }
    }
}

pub fn status_to_fflags(status: Status) -> u8 {
    let old = status.bits();
    let mut rst: u8 = 0;

    // reverse the low 5 bits
    for i in 0..5 {
        rst |= (old >> i & 1) << (4 - i);
    }

    rst
}

pub fn save_fflags_to_cpu(cpu: &mut RVCPU) {
    let fflags = status_to_fflags(cpu.fpu.last_status());

    cpu.csr
        .get_by_type_existing::<Fcsr>()
        .set_fflags(fflags as WordType);
}

pub(super) fn exec_float_load<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::I { rs1, rd, imm } = info {
            let val = cpu.reg_file.read(rs1, 0).0;
            let addr = wrapping_add_as_signed(val, sign_extend(imm, 12));
            let rst = cpu.memory.read::<F::BitsType>(addr, &mut cpu.csr);

            match rst {
                Ok(data) => {
                    cpu.fpu.store_raw::<F>(rd, data.truncate_to());
                }
                Err(err) => {
                    cpu.csr.write_uncheck_privilege(csr_index::mtval, addr);
                    return Err(Exception::from_memory_err(err));
                }
            }
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_store<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::S { rs1, rs2, imm } = info {
            let addr = cpu.reg_file.read(rs1, 0).0;
            let val: F::BitsType = cpu.fpu.load_raw(rs2).truncate_to();
            let addr = wrapping_add_as_signed(addr, sign_extend(imm, 12));

            let ret = cpu.memory.write(addr, val, &mut cpu.csr);
            if let Err(err) = ret {
                cpu.csr.write_uncheck_privilege(csr_index::mtval, addr);
                return Err(Exception::from_memory_err(err));
            }
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_arith_r4_rm<F, Op>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: TernaryOpWithRound<<F as APFloatOf>::Float>,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R4_rm {
            rs1,
            rs2,
            rs3,
            rd,
            rm,
        } = info
        {
            let rm = rm_to_round(cpu, rm);
            cpu.fpu.exec_ternary_r::<Op, F>(rs1, rs2, rs3, rd, rm);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_arith_rm<F, Op>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: BinaryOpWithRound<<F as APFloatOf>::Float>,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm { rs1, rs2, rd, rm } = info {
            // TODO: The order of FPU exec and here is reversed.
            let rm = rm_to_round(cpu, rm);
            cpu.fpu.exec_binary_r::<Op, F>(rs1, rs2, rd, rm);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_arith<F, Op>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: BinaryOp<<F as APFloatOf>::Float>,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2, rd } = info {
            // TODO: The order of FPU exec and here is reverfsed.
            cpu.fpu.exec_binary_ignore_cnan::<Op, F>(rs1, rs2, rd);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_sqrt<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        {
            let round = rm_to_round(cpu, rm);
            cpu.fpu.sqrt::<F>(rs1, rd, round);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_cvt_u_from_f<F, U>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
    U: WordTrait,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        {
            let rm = rm_to_round(cpu, rm);
            let data = U::truncate_from(cpu.fpu.get_and_cvt_unsigned::<F, U>(rs1, rm));
            let data = data.sign_extend_to_wordtype(); // fcvt.wu.[s/d] also needs sign extension
            save_fflags_to_cpu(cpu);
            cpu.reg_file.write(rd, data.into());
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_cvt_i_from_f<F, U>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
    U: WordTrait,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        {
            let rm = rm_to_round(cpu, rm);
            let data = U::truncate_from(cpu.fpu.get_and_cvt_signed::<F, U>(rs1, rm) as u128);
            let data = data.sign_extend_to_wordtype();
            save_fflags_to_cpu(cpu);
            cpu.reg_file.write(rd, data.into());
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_cvt_f_from_u<F, const BITS: u32>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        {
            let val = cpu.reg_file.read(rs1, 0).0;
            let rm = rm_to_round(cpu, rm);
            cpu.fpu
                .cvt_unsigned_and_store::<F>(rd, val.truncate_to_bits(BITS) as u128, rm);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_cvt_f_from_i<F, const BITS: u32>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        {
            let val = cpu.reg_file.read(rs1, 0).0;
            let rm = rm_to_round(cpu, rm);
            cpu.fpu
                .cvt_signed_and_store::<F>(rd, val.cast_signed() as i128, rm);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_cvt_float<F: FloatPoint, T: FloatPoint>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F::Float: FloatConvert<T::Float>,
{
    normal_float_exec(cpu, |cpu| {
        let RVInstrInfo::R_rm {
            rs1,
            rs2: _,
            rd,
            rm,
        } = info
        else {
            std::unreachable!();
        };

        let round = rm_to_round(cpu, rm);
        cpu.fpu.cvt_float_and_store::<F, T>(rd, rs1, round);
        save_fflags_to_cpu(cpu);

        Ok(())
    })
}

pub(super) fn exec_float_compare<Op, F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: CmpOp<<F as APFloatOf>::Float>,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2, rd } = info {
            let rst = cpu.fpu.compare::<Op, F>(rs1, rs2);
            save_fflags_to_cpu(cpu);
            cpu.reg_file.write(rd, rst as WordType);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_classify<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
            let rst = cpu.fpu.classify::<F>(rs1);
            cpu.reg_file.write(rd, rst as WordType);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_mv_x_from_f<F, const EXTEND: bool>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
            let mut rst: WordType = cpu.fpu.load_raw(rs1).truncate_to();

            #[cfg(feature = "riscv64")]
            if EXTEND {
                rst = sign_extend(rst, 32); // Do extra sign extension in RV64F
            }

            cpu.reg_file.write(rd, rst);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_mv_f_from_x<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
            let rst = cpu.reg_file.read(rs1, 0).0;
            cpu.fpu.store_raw::<F>(rd, rst.truncate_to());
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_min<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2, rd } = info {
            cpu.fpu.min_num::<F>(rs1, rs2, rd);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}

pub(super) fn exec_float_max<F>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    normal_float_exec(cpu, |cpu| {
        if let RVInstrInfo::R { rs1, rs2, rd } = info {
            cpu.fpu.max_num::<F>(rs1, rs2, rd);
            save_fflags_to_cpu(cpu);
        } else {
            std::unreachable!();
        }
        Ok(())
    })
}
