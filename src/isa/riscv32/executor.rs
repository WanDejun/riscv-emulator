use crate::{
    config::arch_config::WordType,
    cpu::RegFile,
    device::Mem,
    isa::riscv32::{
        decoder::Decoder,
        instr::{Exception, RVInstrInfo, Riscv32Instr},
    },
    utils::UnsignedInteger,
    vaddr::VirtAddrManager,
};

// TODO: Move some of the codes about number to utils in the root.

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
pub fn negative_of(value: WordType) -> WordType {
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

    /// Process arithmetic instructions with `rs1`, (`rs2` or `imm`) and `rd` in RV32I.
    ///
    /// # NOTE
    ///
    /// Not sure about extended ISAs.
    ///
    /// This will always do signed extension to `imm` as 12 bit.
    fn exec_arith<F>(&mut self, info: RVInstrInfo, exec: F) -> Result<(), Exception>
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

    fn exec_branch<F>(&mut self, info: RVInstrInfo, cond: F) -> Result<(), Exception>
    where
        F: FnOnce(WordType, WordType) -> bool,
    {
        if let RVInstrInfo::B { rs1, rs2, imm } = info {
            let (val1, val2) = self.reg_file.read(rs1, rs2);

            if cond(val1, val2) {
                self.pc = self.pc.wrapping_add(sign_extend(imm, 13));
            } else {
                self.pc = self.pc.wrapping_add(4);
            }
        } else {
            std::unreachable!();
        }

        Ok(())
    }

    fn exec_load<T>(&mut self, info: RVInstrInfo, extend: bool) -> Result<(), Exception>
    where
        T: UnsignedInteger,
    {
        if let RVInstrInfo::I { rs1, rd, imm } = info {
            let val = self.reg_file.read(rs1, 0).0;
            let addr = val.wrapping_add(sign_extend(imm, 12));
            let mut data: WordType = self.memory.read::<T>(addr).into();
            if extend {
                data = sign_extend(data, 12);
            }
            self.reg_file.write(rd, data);
        } else {
            std::unreachable!();
        }

        self.pc = self.pc.wrapping_add(4);
        Ok(())
    }

    fn exec_store<T>(&mut self, info: RVInstrInfo) -> Result<(), Exception>
    where
        T: UnsignedInteger,
    {
        if let RVInstrInfo::S { rs1, rs2, imm } = info {
            let (val1, val2) = self.reg_file.read(rs1, rs2);
            let addr = val1.wrapping_add(sign_extend(imm, 12));
            self.memory.write(addr, T::truncate_from(val2));
        } else {
            std::unreachable!();
        }

        self.pc = self.pc.wrapping_add(4);
        Ok(())
    }

    fn execute(&mut self, instr: Riscv32Instr, info: RVInstrInfo) -> Result<(), Exception> {
        let rst = match instr {
            // Arithmetic
            Riscv32Instr::ADD => self.exec_arith(info, |a, b| Ok(a.wrapping_add(b))),
            Riscv32Instr::SUB => self.exec_arith(info, |a, b| Ok(a.wrapping_sub(b))),
            Riscv32Instr::ADDI => self.exec_arith(info, |a, b| Ok(a.wrapping_add(b))),

            // Shift
            Riscv32Instr::SLL => self.exec_arith(info, |a, b| Ok(a << b)),
            Riscv32Instr::SRL => self.exec_arith(info, |a, b| Ok(a >> b)),
            Riscv32Instr::SRA => {
                // Rust do arithmetic right shift on signed, logical on unsigned.
                self.exec_arith(info, |a, b| {
                    Ok((a.cast_signed() >> b.cast_signed()).cast_unsigned())
                })
            }

            // Cond set
            Riscv32Instr::SLT | Riscv32Instr::SLTI => self.exec_arith(info, |a, b| {
                Ok((a.cast_signed() < b.cast_signed()) as WordType)
            }),
            Riscv32Instr::SLTU | Riscv32Instr::SLTIU => {
                self.exec_arith(info, |a, b| Ok((a < b) as WordType))
            }

            // Bit
            Riscv32Instr::AND | Riscv32Instr::ANDI => self.exec_arith(info, |a, b| Ok(a & b)),
            Riscv32Instr::OR | Riscv32Instr::ORI => self.exec_arith(info, |a, b| Ok(a | b)),
            Riscv32Instr::XOR | Riscv32Instr::XORI => self.exec_arith(info, |a, b| Ok(a ^ b)),

            // Branch
            Riscv32Instr::BEQ => self.exec_branch(info, |a, b| a == b),
            Riscv32Instr::BNE => self.exec_branch(info, |a, b| a != b),
            Riscv32Instr::BLT => self.exec_branch(info, |a, b| a.cast_signed() < b.cast_signed()),
            Riscv32Instr::BGE => self.exec_branch(info, |a, b| a.cast_signed() >= b.cast_signed()),
            Riscv32Instr::BLTU => self.exec_branch(info, |a, b| a < b),
            Riscv32Instr::BGEU => self.exec_branch(info, |a, b| a >= b),

            // Load
            Riscv32Instr::LB => self.exec_load::<u8>(info, true),
            Riscv32Instr::LBU => self.exec_load::<u8>(info, false),
            Riscv32Instr::LH => self.exec_load::<u16>(info, true),
            Riscv32Instr::LHU => self.exec_load::<u16>(info, false),
            Riscv32Instr::LW => self.exec_load::<u32>(info, false),

            // Store
            Riscv32Instr::SB => self.exec_store::<u8>(info),
            Riscv32Instr::SH => self.exec_store::<u16>(info),
            Riscv32Instr::SW => self.exec_store::<u32>(info),

            // Jump and link
            Riscv32Instr::JAL => {
                if let RVInstrInfo::J { rd, imm } = info {
                    self.reg_file.write(rd, self.pc.wrapping_add(4));
                    self.pc = self.pc.wrapping_add(sign_extend(imm, 21));
                } else {
                    std::unreachable!();
                }
                Ok(())
            }

            Riscv32Instr::JALR => {
                if let RVInstrInfo::I { rs1, rd, imm } = info {
                    let t = self.pc + 4;
                    let val = self.reg_file.read(rs1, 0).0;
                    self.pc = (val.wrapping_add(sign_extend(imm, 12)) & !1) as WordType;
                    self.reg_file.write(rd, t);
                } else {
                    std::unreachable!();
                }

                Ok(())
            }

            // U-Type
            Riscv32Instr::AUIPC => {
                if let RVInstrInfo::U { rd, imm } = info {
                    self.reg_file
                        .write(rd, self.pc.wrapping_add(sign_extend(imm, 32)));
                    self.pc = self.pc.wrapping_add(4);
                    Ok(())
                } else {
                    std::unreachable!();
                }
            }

            Riscv32Instr::LUI => {
                if let RVInstrInfo::U { rd, imm } = info {
                    self.reg_file.write(rd, sign_extend(imm, 32));
                    self.pc = self.pc.wrapping_add(4);
                    Ok(())
                } else {
                    std::unreachable!();
                }
            }

            Riscv32Instr::FENCE => {
                // XXX: We don't need to handle `fence`, at present.
                Ok(())
            }

            Riscv32Instr::ECALL | Riscv32Instr::EBREAK => {
                todo!()
            }
        };

        self.reg_file[0] = 0;

        rst
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
        // TODO: Organize these tests better, and ADD MORE TESTS

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
