use crate::{
    config::arch_config::WordType,
    cpu::RegFile,
    device::Mem,
    isa::riscv32::{
        decoder::Decoder,
        instr::{Exception, RVInstrInfo, Riscv32Instr},
    },
    vaddr::VirtAddrManager,
};

fn sign_extend(value: WordType, from_bits: u32) -> WordType {
    let sign_bit = (1u64 << (from_bits - 1)) as WordType;

    if (value & sign_bit) != 0 {
        let mask = (!0u64 ^ ((1u64 << from_bits) - 1)) as WordType;
        value | mask
    } else {
        value
    }
}

/// get the negative of given number of [`WordType`] in 2's complement.
fn negative_of(value: WordType) -> WordType {
    !value + 1
}

pub struct RV32CPU {
    reg_file: RegFile,
    memory: VirtAddrManager,
    pc: WordType,
    decoder: Decoder,
}

impl RV32CPU {
    fn new() -> Self {
        Self {
            reg_file: RegFile::new(),
            memory: VirtAddrManager::new(),
            pc: 0,
            decoder: Decoder::new(),
        }
    }

    /// Process arithmetic instructions with `rs1`, (`rs2` or `imm`) and `rd` in RV32I,
    /// which means this CANNOT handle `slli`, `srli`, `srai`.
    ///
    /// CAUTION: This function will handle [`Self::pc`].
    ///
    /// # NOTE
    ///
    /// Not sure about extended ISAs.
    fn execute_arith<F>(&mut self, info: RVInstrInfo, exec: F) -> Result<(), Exception>
    where
        F: Fn(WordType, WordType) -> Result<WordType, Exception>,
    {
        let (rd, rst) = match info {
            RVInstrInfo::R { rs1, rs2, rd } => {
                let (val1, val2) = self.reg_file.read(rs1, rs2);
                (rd, exec(val1, val2)?)
            }
            RVInstrInfo::I { rs1, rd, imm } => {
                let val1 = self.reg_file.read(rs1, 0).0;
                let simm = sign_extend(imm, 12);
                (rd, exec(val1, simm)?)
            }
            _ => std::unreachable!(),
        };

        self.reg_file.write(rd, rst);
        self.pc = self.pc.wrapping_add(4);

        Ok(())
    }

    fn execute(&mut self, instr: Riscv32Instr, info: RVInstrInfo) -> Result<(), Exception> {
        match instr {
            Riscv32Instr::ADD => self.execute_arith(info, |a, b| Ok(a.wrapping_add(b))),
            Riscv32Instr::SUB => self.execute_arith(info, |a, b| Ok(a.wrapping_sub(b))),
            Riscv32Instr::ADDI => self.execute_arith(info, |a, b| Ok(a.wrapping_add(b))),
            _ => todo!(),
        }?;

        self.reg_file[0] = 0;
        Ok(())
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        let instr_bytes = self.memory.read::<u32>(self.pc);
        let (instr, info) = self.decoder.decode(instr_bytes)?;
        self.execute(instr, info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_extend() {
        assert_eq!(sign_extend(0x123, 12), 0x123);
        assert_eq!(sign_extend(0x7FF, 12), 0x7FF);
        assert_eq!(sign_extend(0xFFF, 12), !0 as WordType);
        assert_eq!(sign_extend(0xF0F, 12), (!0 - 0xF0) as WordType);
    }

    #[test]
    fn test_get_negative_of() {
        assert_eq!(negative_of(1 as WordType), (!0) as WordType);
        assert_eq!(negative_of(2 as WordType), (!0 - 1) as WordType);
    }

    fn check_execute_arith(
        instr: Riscv32Instr,
        info: RVInstrInfo,
        value: (WordType, WordType),
        expected: WordType,
    ) {
        let mut cpu = RV32CPU::new();

        let old_pc = cpu.pc;

        match info {
            RVInstrInfo::R { rs1, rs2, rd } => {
                cpu.reg_file.write(rs1, value.0);
                cpu.reg_file.write(rs2, value.1);
                cpu.execute(instr, info).unwrap();
                assert_eq!(cpu.reg_file.read(1, 2), value);
                assert_eq!(cpu.reg_file.read(rd, 0).0, expected);
            }
            RVInstrInfo::I { rs1, rd, imm: _imm } => {
                cpu.reg_file.write(rs1, value.0);
                cpu.execute(instr, info).unwrap();
                assert_eq!(cpu.reg_file.read(rd, 0).0, expected);
            }
            _ => panic!("Unsupported instruction info type"),
        }

        assert_eq!(cpu.pc, old_pc.wrapping_add(4));
    }

    #[test]
    fn test_execute_arith() {
        check_execute_arith(
            Riscv32Instr::ADD,
            RVInstrInfo::R {
                rs1: 1,
                rs2: 2,
                rd: 3,
            },
            (10, 5),
            15,
        );
        check_execute_arith(
            Riscv32Instr::SUB,
            RVInstrInfo::R {
                rs1: 1,
                rs2: 2,
                rd: 3,
            },
            (10, 5),
            5,
        );
        check_execute_arith(
            Riscv32Instr::SUB,
            RVInstrInfo::R {
                rs1: 1,
                rs2: 2,
                rd: 3,
            },
            (5, 10),
            negative_of(5),
        );
        check_execute_arith(
            Riscv32Instr::ADDI,
            RVInstrInfo::I {
                rs1: 1,
                rd: 3,
                imm: 10,
            },
            (5, 10),
            15,
        );
    }
}
