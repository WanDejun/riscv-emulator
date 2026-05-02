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
        let mut vtype = cpu.csr.get_by_type::<Vtype>().unwrap();
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
            if let Some(maxvl) = vtype.vsetvl(configfield.vtype) {
                let vl = configfield.input_len.min(maxvl);
                cpu.write_reg(rd, vl);
                cpu.csr.write_directly(Vl::get_index(), vl).unwrap();
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

pub(super) fn vector_unit_stride_load<const EEW: u8>(
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
        let nf = (func6 >> 3) & 0b111;
        let _mew = (func6 >> 2) & 0b1;
        let mop = func6 & 0b11;
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
                    nf as u8,
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
