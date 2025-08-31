use rustc_apfloat::Status;

use crate::{
    config::arch_config::WordType,
    fpu::{Round, soft_float::*},
    isa::riscv::{
        csr_reg::{csr_index, csr_macro::Fcsr},
        executor::RV32CPU,
        instruction::RVInstrInfo,
        trap::Exception,
    },
    utils::{FloatPoint, TruncateToBits, WordTrait, sign_extend, wrapping_add_as_signed},
};

fn rm_to_round(cpu: &mut RV32CPU, rm: u8) -> Round {
    match rm {
        0b000 => Round::NearestTiesToEven,
        0b001 => Round::TowardZero,
        0b010 => Round::TowardNegative,
        0b011 => Round::TowardPositive,
        0b100 => Round::NearestTiesToAway,
        0b111 => {
            let rm = cpu.csr.get_by_type::<Fcsr>().unwrap().get_rm();
            debug_assert_ne!(rm, 0b111); // TODO: Not sure if we need to raise an invalid instruction here.
            rm_to_round(cpu, rm as u8)
        }
        _ => unreachable!(),
    }
}

fn status_to_fflags(status: Status) -> u8 {
    let old = status.bits() & 0b11111;
    let mut rst: u8 = 0;

    // reverse the low 5 bits
    for i in 0..5 {
        rst |= (old >> i & 1) << (4 - i);
    }

    rst
}

fn save_fflags_to_cpu(cpu: &mut RV32CPU) {
    // The layout of `Status` from rustc_apfloat is identical to RISC-V fcsr.
    let fflags = status_to_fflags(cpu.fpu.last_status());

    cpu.csr
        .get_by_type::<Fcsr>()
        .unwrap()
        .set_fflags(fflags as WordType);
}

pub(super) fn exec_float_load<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::I { rs1, rd, imm } = info {
        let val = cpu.reg_file.read(rs1, 0).0;
        let addr = wrapping_add_as_signed(val, sign_extend(imm, 12));
        let rst = cpu.memory.read::<F::BitsType>(addr);

        match rst {
            Ok(data) => {
                cpu.fpu.store(rd, F::from_bits(data));
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

pub(super) fn exec_float_store<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::S { rs1, rs2, imm } = info {
        let val1 = cpu.reg_file.read(rs1, 0).0;
        let val2 = cpu.fpu.load::<F>(rs2);
        let addr = wrapping_add_as_signed(val1, sign_extend(imm, 12));

        let ret = cpu.memory.write(addr, val2.to_bits());
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

pub(super) fn exec_float_arith_r4_rm<F, Op>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: TernaryOpWithRound<<F as APFloatOf>::Float>,
{
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

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_arith_r_rm<F, Op>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: BinaryOpWithRound<<F as APFloatOf>::Float>,
{
    if let RVInstrInfo::R_rm { rs1, rs2, rd, rm } = info {
        // TODO: The order of FPU exec and here is reversed.
        let rm = rm_to_round(cpu, rm);
        cpu.fpu.exec_binary_r::<Op, F>(rs1, rs2, rd, rm);
        save_fflags_to_cpu(cpu);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_arith_r<F, Op>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: BinaryOp<<F as APFloatOf>::Float>,
{
    if let RVInstrInfo::R { rs1, rs2, rd } = info {
        // TODO: The order of FPU exec and here is reverfsed.
        cpu.fpu.exec_binary::<Op, F>(rs1, rs2, rd);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_unary<F, Op>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: UnaryOp<<F as APFloatOf>::Float>,
{
    if let RVInstrInfo::R_rm {
        rs1,
        rs2: _,
        rd,
        rm: _,
    } = info
    {
        cpu.fpu.exec_unary::<Op, F>(rs1, rd);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_cvt_u_from_f<F, U>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
    U: WordTrait,
{
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

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_cvt_i_from_f<F, U>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
    U: WordTrait,
{
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

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_cvt_f_from_u<F, const BITS: u32>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
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
            .cvt_u_to_f_and_store::<F>(rd, val.truncate_to_bits(BITS) as u128, rm);
        save_fflags_to_cpu(cpu);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_cvt_f_from_i<F, const BITS: u32>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
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
            .cvt_s_to_f_and_store::<F>(rd, val.cast_signed() as i128, rm);
        save_fflags_to_cpu(cpu);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_compare<Op, F>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
    Op: CmpOp<<F as APFloatOf>::Float>,
{
    if let RVInstrInfo::R { rs1, rs2, rd } = info {
        let rst = cpu.fpu.compare::<Op, F>(rs1, rs2);
        save_fflags_to_cpu(cpu);
        cpu.reg_file.write(rd, rst as WordType);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_classify<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
        let rst = cpu.fpu.classify::<F>(rs1);
        cpu.reg_file.write(rd, rst as WordType);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_mv_x_from_f<F, const EXTEND: bool>(
    info: RVInstrInfo,
    cpu: &mut RV32CPU,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
        let mut rst: WordType = cpu.fpu.load::<f32>(rs1).to_bits().into();

        #[cfg(feature = "riscv64")]
        if EXTEND {
            rst = sign_extend(rst, 32); // Do extra sign extension in RV64F
        }

        cpu.reg_file.write(rd, rst);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_mv_x_from_f64(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception> {
    if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
        let rst = cpu.fpu.load::<f64>(rs1).to_bits();
        cpu.reg_file.write(rd, rst.into());
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_mv_f_from_x<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::R { rs1, rs2: _, rd } = info {
        let rst = cpu.reg_file.read(rs1, 0).0;
        cpu.fpu.store_from_bits::<F>(rd, rst as u128);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_min<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::R { rs1, rs2, rd } = info {
        cpu.fpu.min_num::<F>(rs1, rs2, rd);
        save_fflags_to_cpu(cpu);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}

pub(super) fn exec_float_max<F>(info: RVInstrInfo, cpu: &mut RV32CPU) -> Result<(), Exception>
where
    F: FloatPoint,
{
    if let RVInstrInfo::R { rs1, rs2, rd } = info {
        cpu.fpu.max_num::<F>(rs1, rs2, rd);
        save_fflags_to_cpu(cpu);
    } else {
        std::unreachable!();
    }

    cpu.pc = cpu.pc.wrapping_add(4);
    Ok(())
}
