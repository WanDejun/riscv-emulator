use crate::{
    config::arch_config::WordType,
    isa::{
        DebugTarget,
        riscv::{
            csr_reg::{
                NamedCsrReg,
                csr_macro::{Vl, Vtype},
            },
            executor::RVCPU,
            instruction::{RVInstrInfo, normal_exec},
            trap::Exception,
        },
    },
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
    normal_exec(cpu, |cpu| {
        let mut vtype_csr = cpu.csr.get_by_type::<Vtype>().unwrap();
        if let RVInstrInfo::V {
            rs1,
            rs2,
            rd,
            vm,
            func6,
        } = info
        {
            let imm = (func6 as WordType) << 6 | (vm as WordType) << 5 | (rs2 as WordType);
            let configfield = T::exec(imm, rs1, cpu);

            if let Some(maxvl) = vtype_csr.vsetvl(configfield.vtype) {
                let vl = configfield.input_len.min(maxvl);
                let vluml = ((configfield.vtype & 0b111) as u8).into();
                let vsew = (((configfield.vtype >> 3) & 0b111) as u8).into();
                let vta = ((configfield.vtype >> 4) & 1) != 0;
                let vma = ((configfield.vtype >> 5) & 1) != 0;

                cpu.csr.write_directly(Vl::get_index(), vl).unwrap();
                cpu.vector.set_config((vluml, vsew, vta, vma, vl as u16));
                cpu.write_reg(rd, vl);

                Ok(())
            } else {
                cpu.write_reg(rd, 0);
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
    normal_exec(cpu, |cpu| {
        if let RVInstrInfo::V { func6, .. } = info {
            let mop = func6 & 0b11;
            match mop {
                0b00 => do_vector_unit_stride_load::<EEW>(info, cpu),
                0b01 => do_vector_indexed_ordered_load::<EEW>(info, cpu),
                0b10 => do_vector_constant_stride_load::<EEW>(info, cpu),
                0b11 => do_vector_indexed_unordered_load::<EEW>(info, cpu),
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
fn load_store_func6_docode(func6: u8) -> Func6Uop {
    let nf = (func6 >> 3) & 0b111;
    let mew = (func6 >> 2) & 0b1;
    let mop = func6 & 0b11;
    Func6Uop { nf, mew, mop }
}

fn do_vector_unit_stride_load<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception> {
    if let RVInstrInfo::V {
        rs1,
        rs2,
        rd,
        vm: _vm,
        func6,
    } = info
    {
        let Func6Uop { nf, mew: _mew, mop } = load_store_func6_docode(func6);
        let lumop = rs2;
        debug_assert_eq!(mop, 0b00);
        let vector = &mut cpu.vector;

        let base_addr = cpu.reg_file.read(rs1, 0).0;
        let res;
        match lumop {
            0b000 => {
                res = vector.unit_stride_load(
                    rd,
                    EEW.into(),
                    nf as u8 + 1,
                    base_addr,
                    &mut cpu.memory.mmio,
                );
            }
            _ => todo!(),
        }

        match res {
            Ok(()) => Ok(()),
            Err(err) => Err(Exception::from_memory_err(err)),
        }
    } else {
        unreachable!()
    }
}

fn do_vector_indexed_unordered_load<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_constant_stride_load<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_indexed_ordered_load<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

pub(super) fn vector_store<const EEW: u8>(
    info: RVInstrInfo,
    cpu: &mut RVCPU,
) -> Result<(), Exception> {
    normal_exec(cpu, |cpu| {
        if let RVInstrInfo::V { func6, .. } = info {
            let mop = func6 & 0b11;
            match mop {
                0b00 => do_vector_unit_stride_store::<EEW>(info, cpu),
                0b01 => do_vector_indexed_ordered_store::<EEW>(info, cpu),
                0b10 => do_vector_constant_stride_store::<EEW>(info, cpu),
                0b11 => do_vector_indexed_unordered_store::<EEW>(info, cpu),
                _ => Err(Exception::IllegalInstruction),
            }
        } else {
            unreachable!()
        }
    })
}

fn do_vector_unit_stride_store<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_indexed_unordered_store<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_constant_stride_store<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

fn do_vector_indexed_ordered_store<const EEW: u8>(
    _info: RVInstrInfo,
    _cpu: &mut RVCPU,
) -> Result<(), Exception> {
    unimplemented!()
}

#[cfg(test)]
mod test {
    use std::{cell::UnsafeCell, rc::Rc};

    use crate::{
        device::mmio::MemoryMapIO,
        isa::riscv::{
            cpu_tester::run_test_exec_decode,
            csr_reg::csr_macro::Mstatus,
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
        cpu.vector.set_config((
            TEST_VLMUL,
            TEST_VSEW,
            false,
            false,
            VLEN_BYTE as u16 * TEST_VLMUL.get_lmul() as u16 / TEST_VSEW.get_sew() as u16,
        ));

        let instr_info = RVInstrInfo::V {
            rs1: 1,
            rs2: 0, // lumop
            rd: 0,
            vm: false,
            func6: 0b000000,
        };
        cpu.reg_file.write(1, TEST_DATA_BASE);
        do_vector_unit_stride_load::<2>(instr_info, &mut cpu).unwrap();
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
            0b000_0_00_0_00000_00001_101_00000_0000111,
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
}
