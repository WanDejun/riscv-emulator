use crate::{
    config::arch_config::WordType,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{
                NamedCsrReg,
                csr_macro::{Vl, Vstart, Vtype},
            },
            executor::RVCPU,
            instruction::{RVInstrInfo, exec_function::ExecMove, normal_vector_exec},
            trap::Exception,
            vector::{
                VectorMemException,
                arithmetic::{
                    VectorOpBitVV, VectorOpIntegerGatherEI16VV, VectorOpIntegerGatherVV,
                    VectorOpIntegerMaskVV, VectorOpIntegerMaskVVM, VectorOpIntegerMaskVX,
                    VectorOpIntegerMaskVXM, VectorOpIntegerV, VectorOpIntegerVV,
                    VectorOpIntegerVVM, VectorOpIntegerVVV, VectorOpIntegerVX, VectorOpIntegerVXM,
                    VectorOpIntegerVXV, VectorOpWideningIntegerVV, VectorOpWideningIntegerVVV,
                    VectorOpWideningIntegerVX, VectorOpWideningIntegerVXV,
                    VectorOpWideningIntegerWV, VectorOpWideningIntegerWX,
                },
                types::Vsew,
            },
        },
    },
    utils::sign_extend,
};

pub(super) struct VectorConfigField {
    vtype: WordType,
    input_len: WordType,
}

pub(super) trait VectorConfigFieldExtractor {
    fn exec(imm: WordType, rs1: u8, cpu: &mut RVCPU) -> VectorConfigField;
}

pub(super) fn exec_vector_config<T>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    T: VectorConfigFieldExtractor,
{
    normal_vector_exec(cpu, |cpu, _vstart| {
        let mut vtype_csr = cpu.csr.get_by_type::<Vtype>().unwrap();
        if let RVInstrInfo::V {
            rs1,
            rs2,
            rd: vd,
            vm,
            func6,
        } = info
        {
            let imm = (func6 as WordType) << 6 | (vm as WordType) << 5 | (rs2 as WordType);
            let configfield = T::exec(imm, rs1, cpu);

            if let Some(maxvl) = vtype_csr.vsetvl(configfield.vtype) {
                let vl = configfield.input_len.min(maxvl);
                let vlmul = ((configfield.vtype & 0b111) as u8).into();
                let vsew = (((configfield.vtype >> 3) & 0b111) as u8).into();
                let vta = ((configfield.vtype >> 6) & 1) != 0;
                let vma = ((configfield.vtype >> 7) & 1) != 0;

                cpu.csr
                    .write_directly(Vl::get_index(), vl)
                    .then_some(())
                    .unwrap();
                cpu.vector.set_config((vlmul, vsew, vta, vma, vl as u16));
                cpu.write_reg(vd, vl);

                Ok(())
            } else {
                cpu.write_reg(vd, 0);
                Err(Exception::IllegalInstruction)
            }
        } else {
            std::unreachable!();
        }
    })
}

pub(super) struct VsetivliFieldExtractor {}
impl VectorConfigFieldExtractor for VsetivliFieldExtractor {
    fn exec(imm: WordType, rs1: u8, _cpu: &mut RVCPU) -> VectorConfigField {
        debug_assert!((imm & 0b1100_0000_0000) == 0b1100_0000_0000);
        let vtype = imm & !0b1100_0000_0000;
        let input_len = rs1 as WordType;
        VectorConfigField { vtype, input_len }
    }
}

pub(super) struct VsetvliFieldExtractor {}
impl VectorConfigFieldExtractor for VsetvliFieldExtractor {
    fn exec(imm: WordType, rs1: u8, cpu: &mut RVCPU) -> VectorConfigField {
        debug_assert!((imm & 0b1000_0000_0000) == 0b0000_0000_0000);
        let vtype = imm & !0b1000_0000_0000;
        let input_len = cpu.read_reg(rs1);
        VectorConfigField { vtype, input_len }
    }
}

pub(super) struct VsetvlFieldExtractor {}
impl VectorConfigFieldExtractor for VsetvlFieldExtractor {
    fn exec(imm: WordType, rs1: u8, cpu: &mut RVCPU) -> VectorConfigField {
        debug_assert!(
            (imm & 0b1111_1110_0000) == 0b1000_0000_0000,
            "imm = {:b}",
            imm
        );
        let rs2 = (imm as u8) & 0b11111;
        let vtype = cpu.read_reg(rs2);
        let input_len = cpu.read_reg(rs1);
        VectorConfigField { vtype, input_len }
    }
}

// ---------------------------------------
//          Vector Load/Store
// ---------------------------------------
pub(super) fn vector_load<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V { func6, .. } = info {
            let mop = func6 & 0b11;
            match mop {
                0b00 => do_vector_unit_stride_load::<EEW>(info, cpu, vstart),
                0b01 => do_vector_indexed_unordered_load::<EEW>(info, cpu, vstart),
                0b10 => do_vector_constant_stride_load::<EEW>(info, cpu, vstart),
                0b11 => do_vector_indexed_ordered_load::<EEW>(info, cpu, vstart),
                _ => Err(Exception::IllegalInstruction),
            }
        } else {
            unreachable!()
        }
    })
}

struct Func6Uop {
    nf: u8,
    mew: u8,
    mop: u8,
}

#[inline(always)]
fn load_store_func6_decode(func6: u8) -> Func6Uop {
    let nf = (func6 >> 3) & 0b111;
    let mew = (func6 >> 2) & 0b1;
    let mop = func6 & 0b11;
    Func6Uop { nf, mew, mop }
}

#[inline]
fn finish_vector_memory_access(
    cpu: &mut RVCPU,
    res: Result<(), VectorMemException>,
) -> Result<(), Exception> {
    match res {
        Ok(()) => {
            // A completed vector memory instruction always leaves no pending
            // partial progress to resume.
            cpu.csr
                .write_directly(Vstart::get_index(), 0)
                .then_some(())
                .unwrap();
            Ok(())
        }
        Err(err) => {
            // Only precise memory faults carry an element index. Other errors
            // are raised as-is and do not pretend to be resumable traps.
            if let Some(index) = err.fault_index() {
                cpu.csr
                    .write_directly(Vstart::get_index(), index as WordType)
                    .then_some(())
                    .unwrap();
            }
            Err(err.exception())
        }
    }
}

fn do_vector_unit_stride_load<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1,
        rs2: lumop,
        rd: vd,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b00);

        let base_addr = cpu.reg_file.read(rs1, 0).0;
        let vector = &mut cpu.vector;
        let res;
        match lumop {
            // unit-stride load
            0b000 => {
                res = vector.stride_load(
                    vd,
                    EEW.into(),
                    nf + 1,
                    None,
                    !vm,
                    vstart,
                    base_addr,
                    &mut cpu.memory.mmio,
                );
            }
            // unit-stride, whole register load
            0b01000 => {
                if (mop, vm) != (0b00, true) {
                    return Err(Exception::IllegalInstruction);
                }
                res = vector.load_whole_register(vd, nf, vstart, base_addr, &mut cpu.memory.mmio);
            }
            // unit-stride, mask load, EEW=8
            0b01011 => unimplemented!(),
            // unit-stride fault-only-first
            0b10000 => unimplemented!(),
            _ => return Err(Exception::IllegalInstruction),
        }

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

fn do_vector_indexed_unordered_load<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
    _vstart: usize,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_constant_stride_load<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1,
        rs2,
        rd: vd,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b10);

        let (base_addr, stride) = cpu.reg_file.read(rs1, rs2);
        let vector = &mut cpu.vector;
        let res = vector.stride_load(
            vd,
            EEW.into(),
            nf + 1,
            Some(stride),
            !vm,
            vstart,
            base_addr,
            &mut cpu.memory.mmio,
        );

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

fn do_vector_indexed_ordered_load<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1: base_addr,
        rs2: index_arr_base,
        rd: vd,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b11);

        let (base_addr, index_arr_base) = cpu.reg_file.read(base_addr, index_arr_base);
        let vector = &mut cpu.vector;
        let res = vector.indexed_ordered_load(
            vd,
            EEW.into(),
            nf + 1,
            index_arr_base,
            !vm,
            vstart,
            base_addr,
            &mut cpu.memory.mmio,
        );

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

pub(super) fn vector_store<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V { func6, .. } = info {
            let mop = func6 & 0b11;
            match mop {
                0b00 => do_vector_unit_stride_store::<EEW>(info, cpu, vstart),
                0b01 => do_vector_indexed_unordered_store::<EEW>(info, cpu, vstart),
                0b10 => do_vector_constant_stride_store::<EEW>(info, cpu, vstart),
                0b11 => do_vector_indexed_ordered_store::<EEW>(info, cpu, vstart),
                _ => Err(Exception::IllegalInstruction),
            }
        } else {
            unreachable!()
        }
    })
}

fn do_vector_unit_stride_store<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1,
        rs2: sumop,
        rd: vs3,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b00);

        let base_addr = cpu.reg_file.read(rs1, 0).0;
        let vector = &mut cpu.vector;
        let res;
        match sumop {
            0b00000 => {
                res = vector.stride_store(
                    vs3,
                    EEW.into(),
                    nf + 1,
                    None,
                    !vm,
                    vstart,
                    base_addr,
                    &mut cpu.memory.mmio,
                );
            }
            // unit-stride, whole register store
            0b01000 => {
                if (mew, mop, vm) != (0, 0b00, true) {
                    return Err(Exception::IllegalInstruction);
                }
                res = vector.store_whole_register(vs3, nf, vstart, base_addr, &mut cpu.memory.mmio);
            }
            // unit-stride, mask store, EEW=8
            0b01011 => unimplemented!(),
            _ => return Err(Exception::IllegalInstruction),
        }

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

fn do_vector_indexed_unordered_store<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
    _vstart: usize,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_constant_stride_store<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1,
        rs2,
        rd: vs3,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b10);

        let (base_addr, stride) = cpu.reg_file.read(rs1, rs2);
        let vector = &mut cpu.vector;
        let res = vector.stride_store(
            vs3,
            EEW.into(),
            nf + 1,
            Some(stride),
            !vm,
            vstart,
            base_addr,
            &mut cpu.memory.mmio,
        );

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

fn do_vector_indexed_ordered_store<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
    vstart: usize,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1: base_addr,
        rs2: index_arr_base,
        rd: vs3,
        vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_decode(func6);
        debug_assert_eq!(mop, 0b11);

        let (base_addr, index_arr_base) = cpu.reg_file.read(base_addr, index_arr_base);
        let vector = &mut cpu.vector;
        let res = vector.indexed_ordered_store(
            vs3,
            EEW.into(),
            nf + 1,
            index_arr_base,
            !vm,
            vstart,
            base_addr,
            &mut cpu.memory.mmio,
        );

        finish_vector_memory_access(cpu, res)
    } else {
        unreachable!()
    }
}

// ---------------------------------------
//          Vector Integer
// ---------------------------------------
pub(super) fn vec_integer_op_vv<OpIVV>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    OpIVV: VectorOpIntegerVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_vv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_bit_op_vv<OpIVV>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    OpIVV: VectorOpBitVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector.exec_bit_vv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            std::unreachable!();
        }
    })
}

pub(super) fn vec_integer_op_vx<OpIVX>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_vx::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vvv<OpIVV>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    OpIVV: VectorOpIntegerVVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_vvv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vxv<OpIVX>(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVXV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_vxv::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_slideup_op_vx<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVXV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            if vd == vs2 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_integer_slideup::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_slideup_op_vi<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVXV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: uimm,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_slideup::<OpIVX>(uimm as WordType, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_slidedown_op_vx<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_slidedown::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_slidedown_op_vi<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: uimm,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_slidedown::<OpIVX>(uimm as WordType, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_vv<OpIVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVV: VectorOpWideningIntegerVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_widening_integer_vv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_vvv<OpIVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVV: VectorOpWideningIntegerVVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_widening_integer_vvv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_vxv<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpWideningIntegerVXV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_widening_integer_vxv::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_vx<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpWideningIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_widening_integer_vx::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_wv<OpIVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVV: VectorOpWideningIntegerWV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_widening_integer_wv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_widening_integer_op_wx<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpWideningIntegerWX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_widening_integer_wx::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vi_signed<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: simm5,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let imm = sign_extend(simm5 as WordType, 5);
            cpu.vector
                .exec_integer_vx::<OpIVX>(imm, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vi_unsigned<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: uimm,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_vx::<OpIVX>(uimm as WordType, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_gather_op_vv<OpIVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVV: VectorOpIntegerGatherVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            if vd == vs2 || vd == vs1 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_integer_gather_vv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_gather_op_ei16_vv<OpIVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVV: VectorOpIntegerGatherEI16VV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            if vd == vs2 || vd == vs1 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_integer_gather_ei16_vv::<OpIVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_gather_op_vx<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            if vd == vs2 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_integer_gather_vx::<OpIVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_gather_op_vi<OpIVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVX: VectorOpIntegerVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: imm,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_gather_vx::<OpIVX>(imm as WordType, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vvm<OpIVVM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVVM: VectorOpIntegerVVM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_vvm::<OpIVVM>(vs1, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vxm<OpIVXM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVXM: VectorOpIntegerVXM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_vxm::<OpIVXM>(x1, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_op_vim<OpIVXM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIVXM: VectorOpIntegerVXM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: simm5,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            let imm = sign_extend(simm5 as WordType, 5);
            cpu.vector
                .exec_integer_vxm::<OpIVXM>(imm, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_move_op_v(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            if vs2 != 0 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_integer_v::<ExecMove<u64>>(vs1, vd, Vsew::E64, Vsew::E64, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_move_op_vx(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            if vs2 != 0 {
                return Err(Exception::IllegalInstruction);
            }
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_scalar_move::<ExecMove<u64>, u64>(x1 as u64, vd, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_move_op_vi(info: RVInstrInfo, cpu: &mut RVCPU) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: simm5,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            if vs2 != 0 {
                return Err(Exception::IllegalInstruction);
            }
            let imm = sign_extend(simm5 as WordType, 5);
            cpu.vector
                .exec_integer_scalar_move::<ExecMove<u64>, u64>(imm as u64, vd, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_whole_register_move_op_v<const LMUL: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception> {
    normal_vector_exec(cpu, |cpu, vstart| {
        let expected_rs1 = match LMUL {
            1 => 0,
            2 => 1,
            4 => 3,
            8 => 7,
            _ => return Err(Exception::IllegalInstruction),
        };
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            if rs1 != expected_rs1 {
                return Err(Exception::IllegalInstruction);
            }
            cpu.vector
                .exec_whole_register_move::<ExecMove<u64>>(vs2, vd, LMUL, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vv<OpIMVV>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVV: VectorOpIntegerMaskVV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_mask_vv::<OpIMVV>(vs1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vx<OpIMVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVX: VectorOpIntegerMaskVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_mask_vx::<OpIMVX>(x1, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vi<OpIMVX>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVX: VectorOpIntegerMaskVX,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: simm5,
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            let imm = sign_extend(simm5 as WordType, 5);
            cpu.vector
                .exec_integer_mask_vx::<OpIMVX>(imm, vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vvm<OpIMVVM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVVM: VectorOpIntegerMaskVVM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: vs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_mask_vvm::<OpIMVVM>(vs1, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vxm<OpIMVXM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVXM: VectorOpIntegerMaskVXM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            let x1 = cpu.reg_file.read(rs1, 0).0;
            cpu.vector
                .exec_integer_mask_vxm::<OpIMVXM>(x1, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_mask_op_vim<OpIMVXM>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIMVXM: VectorOpIntegerMaskVXM,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs1: simm5,
            rs2: vs2,
            rd: vd,
            ..
        } = info
        {
            let imm = sign_extend(simm5 as WordType, 5);
            cpu.vector
                .exec_integer_mask_vxm::<OpIMVXM>(imm, vs2, 0, vd, false, vstart)
        } else {
            unreachable!()
        }
    })
}

pub(super) fn vec_integer_ext_op_v<OpIV, const FACTOR: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception>
where
    OpIV: VectorOpIntegerV,
{
    normal_vector_exec(cpu, |cpu, vstart| {
        if let RVInstrInfo::V {
            rs2: vs2,
            rd: vd,
            vm,
            ..
        } = info
        {
            cpu.vector
                .exec_integer_v_ext::<OpIV, FACTOR>(vs2, vd, !vm, vstart)
        } else {
            unreachable!()
        }
    })
}

#[cfg(test)]
mod test {
    use std::{cell::UnsafeCell, rc::Rc};

    use crate::{
        device::mmio::MemoryMapIO,
        isa::riscv::{
            cpu_tester::{CPUChecker, TestCPUBuilder, run_test_exec, run_test_exec_decode},
            csr_reg::{
                NamedCsrReg,
                csr_macro::{Mstatus, Vstart},
            },
            instruction::instr_table::RiscvInstr,
            mmu::VirtAddrManager,
            vector::{
                VLEN_BYTE,
                types::{Vlmul, Vsew},
            },
        },
        ram::Ram,
        ram_config::BASE_ADDR,
    };

    use super::*;

    const TEST_DATA_ADDR_OFFSET: WordType = 0x1000;
    const TEST_DATA_BASE: WordType = BASE_ADDR + TEST_DATA_ADDR_OFFSET;

    fn u32_vec_to_bytes(data: &[u32]) -> Vec<u8> {
        data.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn vector_add_vv_test_cpu() {
        let vs1 = [1_u32, 2, 3, u32::MAX];
        let vs2 = [10_u32, 20, 30, 1];
        let expected = [11_u32, 22, 33, 0];

        run_test_exec(
            RiscvInstr::VADD_VV,
            RVInstrInfo::V {
                rs1: 1,
                rs2: 2,
                rd: 3,
                vm: true,
                func6: 0,
            },
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E32, false, false)
                    .reg_vec(1, 1, &u32_vec_to_bytes(&vs1))
                    .reg_vec(1, 2, &u32_vec_to_bytes(&vs2))
                    .pc(0x2000)
            },
            |checker| checker.reg_vec(3, &expected).pc(0x2004),
        );
    }

    #[test]
    fn vector_arithmetic_respects_vstart_and_clears_it() {
        let vs1 = [1_u32, 2, 3, 4];
        let vs2 = [10_u32, 20, 30, 40];
        let old_vd = [0xaaaa_0001_u32, 0xaaaa_0002, 0xaaaa_0003, 0xaaaa_0004];
        let expected = [old_vd[0], old_vd[1], 33, 44];

        run_test_exec(
            RiscvInstr::VADD_VV,
            RVInstrInfo::V {
                rs1: 1,
                rs2: 2,
                rd: 3,
                vm: true,
                func6: 0,
            },
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E32, false, false)
                    .csr(Vstart::get_index(), 2)
                    .reg_vec(1, 1, &u32_vec_to_bytes(&vs1))
                    .reg_vec(1, 2, &u32_vec_to_bytes(&vs2))
                    .reg_vec(1, 3, &u32_vec_to_bytes(&old_vd))
                    .pc(0x2000)
            },
            |checker| {
                checker
                    .reg_vec(3, &expected)
                    .csr(Vstart::get_index(), 0)
                    .pc(0x2004)
            },
        );
    }

    #[test]
    fn vector_add_vx_test_cpu() {
        let vs2 = [10_u32, 20, 30, u32::MAX];
        let expected = [15_u32, 25, 35, 4];

        run_test_exec(
            RiscvInstr::VADD_VX,
            RVInstrInfo::V {
                rs1: 5,
                rs2: 2,
                rd: 3,
                vm: true,
                func6: 0,
            },
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E32, false, false)
                    .reg(5, 5)
                    .reg_vec(1, 2, &u32_vec_to_bytes(&vs2))
                    .pc(0x2000)
            },
            |checker| checker.reg_vec(3, &expected).pc(0x2004),
        );
    }

    #[test]
    fn vector_add_vi_test_cpu() {
        let vs2 = [10_u32, 20, 30, 0];
        let expected = [9_u32, 19, 29, u32::MAX];

        run_test_exec(
            RiscvInstr::VADD_VI,
            RVInstrInfo::V {
                rs1: 0b11111,
                rs2: 2,
                rd: 3,
                vm: true,
                func6: 0,
            },
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E32, false, false)
                    .reg_vec(1, 2, &u32_vec_to_bytes(&vs2))
                    .pc(0x2000)
            },
            |checker| checker.reg_vec(3, &expected).pc(0x2004),
        );
    }

    #[test]
    fn whole_register_move_rejects_bad_rs1_encoding() {
        let mut cpu = TestCPUBuilder::new()
            .vector_status(Vlmul::M1, Vsew::E8, false, false)
            .pc(0x2000)
            .build();

        let err = vec_whole_register_move_op_v::<2>(
            RVInstrInfo::V {
                rs1: 0,
                rs2: 8,
                rd: 16,
                vm: true,
                func6: 0,
            },
            &mut cpu,
        )
        .unwrap_err();

        assert_eq!(err, Exception::IllegalInstruction);
        assert_eq!(cpu.pc, 0x2000);
    }

    #[test]
    fn vsetvli_preserves_tail_policy_in_vector_config() {
        let vs1 = [1_u32, 2, 3, 4];
        let vs2 = [10_u32, 20, 30, 40];
        let old_vd = [0xdead_beefu32; 4];
        let expected = [11_u32, 0xdead_beef, 0xdead_beef, 0xdead_beef];
        let mut cpu = TestCPUBuilder::new()
            .reg(1, 1)
            .reg_vec(1, 1, &u32_vec_to_bytes(&vs1))
            .reg_vec(1, 2, &u32_vec_to_bytes(&vs2))
            .reg_vec(1, 3, &u32_vec_to_bytes(&old_vd))
            .pc(0x2000)
            .build();

        exec_vector_config::<VsetvliFieldExtractor>(
            RVInstrInfo::V {
                rs1: 1,
                rs2: (Vsew::E32 as u8) << 3,
                rd: 5,
                vm: false,
                func6: Vlmul::M1 as u8,
            },
            &mut cpu,
        )
        .unwrap();

        cpu.execute(
            RiscvInstr::VADD_VV,
            RVInstrInfo::V {
                rs1: 1,
                rs2: 2,
                rd: 3,
                vm: true,
                func6: 0,
            },
        )
        .unwrap();

        CPUChecker::new(&mut cpu)
            .reg(5, 1)
            .reg_vec(3, &expected)
            .pc(0x2008);
    }

    #[test]
    fn unit_stride_load_test() {
        const TOTAL_DATA_LEN: WordType = 128;
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M8;
        type ElemType = u32;
        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        for i in 0..TOTAL_DATA_LEN {
            let addr = TEST_DATA_ADDR_OFFSET + i * size_of::<ElemType>() as WordType;
            unsafe {
                ram_ref
                    .as_mut_unchecked()
                    .write(addr, i as ElemType + 1)
                    .unwrap();
            }
        }
        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1); // Enable FPU by default for convienience
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 0, // lumop
            rd: 0,
            vm: !false, // disable mask
            func6: 0b000000,
        };
        cpu.reg_file.write(1, TEST_DATA_BASE);
        do_vector_unit_stride_load::<2>(instr_info, &mut cpu, 0).unwrap();
        let vreg = cpu.vector.read_as_type::<ElemType>(0).unwrap();
        assert_eq!(
            vreg.len(),
            VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>()
        );
        for i in 0..vreg.len() {
            assert_eq!(vreg[i], i as ElemType + 1);
        }
    }

    #[test]
    fn unit_stride_load_test_cpu() {
        let data: Vec<u16> = (0..(VLEN_BYTE / size_of::<u16>()))
            .map(|i| (i + 1000) as u16)
            .collect();
        run_test_exec_decode(
            0b000_0_00_1_00000_00001_101_00000_0000111,
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E16, false, false)
                    .reg(1, TEST_DATA_BASE)
                    .mem_range(0..128, |i| {
                        (
                            TEST_DATA_BASE + (i * size_of::<u16>()) as WordType,
                            (i + 1000) as u16,
                        )
                    })
                    .pc(0x2000)
            },
            |checker| checker.reg_vec(0, data.as_slice()).pc(0x2004),
        );
    }

    #[test]
    fn vector_load_fault_records_vstart_and_stops() {
        let mut cpu = TestCPUBuilder::new()
            .vector_status(Vlmul::M1, Vsew::E32, false, false)
            .reg(1, TEST_DATA_BASE)
            .reg(2, 1)
            .mem::<u32>(TEST_DATA_BASE, 0x1234_5678)
            .pc(0x2000)
            .build();
        cpu.vector
            .write_as_type(1, 4, &[0xaaaa_aaaau32; VLEN_BYTE / 4]);

        let result = do_vector_constant_stride_load::<2>(
            RVInstrInfo::V {
                rs1: 1,
                rs2: 2,
                rd: 4,
                vm: true,
                func6: 0b000010,
            },
            &mut cpu,
            0,
        );

        assert_eq!(result, Err(Exception::LoadMisaligned));
        CPUChecker::new(&mut cpu)
            .csr(Vstart::get_index(), 1)
            .reg_vec(4, &[0x1234_5678u32, 0xaaaa_aaaa, 0xaaaa_aaaa, 0xaaaa_aaaa])
            .pc(0x2000);
    }

    #[test]
    fn vector_load_resumes_from_vstart_and_clears_it() {
        let data: Vec<u32> = (0..(VLEN_BYTE / size_of::<u32>()))
            .map(|i| (i + 0x40) as u32)
            .collect();
        let mut builder = TestCPUBuilder::new()
            .vector_status(Vlmul::M1, Vsew::E32, false, false)
            .csr(Vstart::get_index(), 2)
            .reg(1, TEST_DATA_BASE);
        for (index, value) in data.iter().enumerate() {
            builder = builder.mem(
                TEST_DATA_BASE + (index * size_of::<u32>()) as WordType,
                *value,
            );
        }
        let mut cpu = builder.pc(0x2000).build();
        cpu.vector
            .write_as_type(1, 4, &[0xaaaa_aaaau32; VLEN_BYTE / 4]);

        do_vector_unit_stride_load::<2>(
            RVInstrInfo::V {
                rs1: 1,
                rs2: 0,
                rd: 4,
                vm: true,
                func6: 0,
            },
            &mut cpu,
            2,
        )
        .unwrap();

        let mut expected = data;
        expected[0] = 0xaaaa_aaaa;
        expected[1] = 0xaaaa_aaaa;
        CPUChecker::new(&mut cpu)
            .csr(Vstart::get_index(), 0)
            .reg_vec(4, &expected)
            .pc(0x2000);
    }

    #[test]
    fn const_stride_load_test() {
        const TOTAL_DATA_LEN: WordType = 128;
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M8;
        const STRIDE: WordType = (size_of::<u32>() as WordType) * 2;
        type ElemType = u32;
        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        for i in 0..TOTAL_DATA_LEN {
            let addr = TEST_DATA_ADDR_OFFSET + i * STRIDE;
            unsafe {
                ram_ref
                    .as_mut_unchecked()
                    .write(addr, i as ElemType + 1)
                    .unwrap();
            }
        }
        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1); // Enable FPU by default for convienience
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 2,
            rd: 0,
            vm: !false, // disable mask
            func6: 0b000010,
        };
        cpu.reg_file.write(1, TEST_DATA_BASE);
        cpu.reg_file.write(2, STRIDE);
        do_vector_constant_stride_load::<2>(instr_info, &mut cpu, 0).unwrap();
        let vreg = cpu.vector.read_as_type::<ElemType>(0).unwrap();
        assert_eq!(
            vreg.len(),
            VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>()
        );
        for i in 0..vreg.len() {
            assert_eq!(vreg[i], i as ElemType + 1);
        }
    }

    #[test]
    fn indexed_ordered_load_test() {
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M4;
        const TEST_SEG: usize = 2;
        type ElemType = u32;

        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        let index_base = TEST_DATA_BASE;
        let data_base = TEST_DATA_BASE + 0x4000;

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1);
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let vl = VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>();
        let elem_cnt = vl * TEST_SEG;
        let mut expected = vec![0 as ElemType; elem_cnt];

        // 非线性索引，避免退化成 unit-stride；为 seg=2 准备总计 vl*2 个索引
        for i in 0..elem_cnt {
            let idx_addr = index_base + (i * size_of::<ElemType>()) as WordType;
            let data_index = ((i * 5 + 3) % elem_cnt) as WordType;
            let data_off = data_index * size_of::<ElemType>() as WordType;
            let data_addr = data_base + data_off;
            let val = ((i as ElemType) * 17 + 101) ^ 0x5A5A_1234;

            cpu.memory
                .write::<ElemType>(idx_addr, data_off as ElemType, &mut cpu.csr)
                .unwrap();
            cpu.memory
                .write::<ElemType>(data_addr, val, &mut cpu.csr)
                .unwrap();
            expected[i] = val;
        }

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 2,
            rd: 0,
            vm: !false, // disable mask
            // nf=1(seg=2), mop=0b11(indexed ordered), mew=0
            func6: 0b001011,
        };
        cpu.reg_file.write(1, data_base);
        cpu.reg_file.write(2, index_base);
        do_vector_indexed_ordered_load::<2>(instr_info, &mut cpu, 0).unwrap();

        let vreg = cpu
            .vector
            .read_with_seg(0, TEST_VSEW, TEST_SEG as u8)
            .unwrap();
        let actual: Vec<ElemType> = vreg.iter().map(|e| e.get::<ElemType>()).collect();
        assert_eq!(actual.len(), elem_cnt, "vreg len mismatch for seg=2");
        for i in 0..elem_cnt {
            assert_eq!(
                actual[i], expected[i],
                "Load mismatch at flattened index {}",
                i
            );
        }
    }

    #[test]
    fn unit_stride_store_test() {
        const TOTAL_DATA_LEN: WordType = 128;
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M8;
        type ElemType = u32;
        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1);
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let vreg_len = VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>();
        let data: Vec<ElemType> = (0..vreg_len).map(|i| (i * 7 + 13) as ElemType).collect();
        cpu.vector
            .write_as_type::<ElemType>(TEST_VLMUL.get_lmul(), 0, &data);

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 0,
            rd: 0,
            vm: !false, // disable mask
            func6: 0b000000,
        };
        cpu.reg_file.write(1, TEST_DATA_BASE);
        do_vector_unit_stride_store::<2>(instr_info, &mut cpu, 0).unwrap();

        for i in 0..vreg_len {
            let addr = TEST_DATA_BASE + (i * size_of::<ElemType>()) as WordType;
            let expected = data[i];
            let read: ElemType = cpu.memory.read::<ElemType>(addr, &mut cpu.csr).unwrap();
            assert_eq!(read, expected, "Memory mismatch at index {}", i);
        }
    }

    #[test]
    fn unit_stride_store_test_cpu() {
        let data: Vec<u16> = (0..(VLEN_BYTE / size_of::<u16>()))
            .map(|i| (i + 2000) as u16)
            .collect();
        let byte_data: Vec<u8> = data.iter().flat_map(|x| x.to_le_bytes()).collect();

        let raw_instr = 0b000_0_00_1_00000_00001_101_00000_0100111;
        run_test_exec_decode(
            raw_instr,
            |builder| {
                builder
                    .vector_status(Vlmul::M1, Vsew::E16, false, false)
                    .reg(1, TEST_DATA_BASE)
                    .reg_vec(1, 0, byte_data.as_slice())
                    .pc(0x2000)
            },
            |mut checker| {
                let stride = size_of::<u16>() as WordType;
                for (i, &val) in data.iter().enumerate() {
                    let addr = TEST_DATA_BASE + (i as WordType) * stride;
                    checker = checker.mem::<u16>(addr, val as WordType);
                }
                checker.pc(0x2004)
            },
        );
    }

    #[test]
    fn const_stride_store_test() {
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M8;
        const STRIDE: WordType = (size_of::<u32>() as WordType) * 2;
        type ElemType = u32;
        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1);
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let vreg_len = VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>();
        let data: Vec<ElemType> = (0..vreg_len).map(|i| (i * 7 + 13) as ElemType).collect();
        cpu.vector
            .write_as_type::<ElemType>(TEST_VLMUL.get_lmul(), 0, &data);

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 2,
            rd: 0,
            vm: !false, // disable mask
            func6: 0b000010,
        };
        cpu.reg_file.write(1, TEST_DATA_BASE);
        cpu.reg_file.write(2, STRIDE);
        do_vector_constant_stride_store::<2>(instr_info, &mut cpu, 0).unwrap();

        for i in 0..vreg_len {
            let addr = TEST_DATA_BASE + (i as WordType) * STRIDE;
            let expected = data[i];
            let read: ElemType = cpu.memory.read::<ElemType>(addr, &mut cpu.csr).unwrap();
            assert_eq!(read, expected, "Memory mismatch at index {}", i);
        }
    }

    #[test]
    fn indexed_ordered_store_test() {
        const TEST_VSEW: Vsew = Vsew::E32;
        const TEST_VLMUL: Vlmul = Vlmul::M1;
        type ElemType = u32;

        let ram_ref = Rc::new(UnsafeCell::new(Ram::new()));
        let index_base = TEST_DATA_BASE;
        let data_base = TEST_DATA_BASE + 0x1000;

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), vec![]);
        let mut cpu = RVCPU::from_vaddr_manager(VirtAddrManager::from_ram_and_mmio(ram_ref, mmio));
        cpu.csr.get_by_type_existing::<Mstatus>().set_fs(1);
        cpu.csr.get_by_type_existing::<Mstatus>().set_vs_directly(1);
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.into_byte_width() as u16,
        ));

        let elem_cnt = VLEN_BYTE * TEST_VLMUL.get_lmul() as usize / size_of::<ElemType>();
        let data: Vec<ElemType> = (0..elem_cnt).map(|i| (i as ElemType) * 3 + 7).collect();
        cpu.vector
            .write_as_type::<ElemType>(TEST_VLMUL.get_lmul(), 0, &data);

        for i in 0..elem_cnt {
            let idx_addr = index_base + (i * size_of::<ElemType>()) as WordType;
            let data_off = (i * size_of::<ElemType>()) as WordType;
            cpu.memory
                .write::<ElemType>(idx_addr, data_off as u32, &mut cpu.csr)
                .unwrap();
        }

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 2,
            rd: 0,
            vm: !false, // disable mask
            func6: 0b000011,
        };
        cpu.reg_file.write(1, data_base);
        cpu.reg_file.write(2, index_base);
        do_vector_indexed_ordered_store::<2>(instr_info, &mut cpu, 0).unwrap();

        for (i, expected) in data.iter().enumerate() {
            let addr = data_base + (i * size_of::<ElemType>()) as WordType;
            let read: ElemType = cpu.memory.read::<ElemType>(addr, &mut cpu.csr).unwrap();
            assert_eq!(read, *expected, "Store mismatch at index {}", i);
        }
    }
}
