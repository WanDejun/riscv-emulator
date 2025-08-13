use crate::{
    config::arch_config::{REG_NAME, REGFILE_CNT, WordType},
    cpu::RegFile,
    device::Mem,
    isa::riscv32::{
        decoder::Decoder,
        instruction::{
            Exception, RVInstrInfo, exec_mapping::get_exec_func, rv32i_table::RiscvInstr,
        },
    },
    ram_config::DEFAULT_PC_VALUE,
    vaddr::VirtAddrManager,
};

pub struct RV32CPU {
    pub(super) reg_file: RegFile,
    pub(super) memory: VirtAddrManager,
    pub(super) pc: WordType,
    pub(super) decoder: Decoder,
}

impl RV32CPU {
    pub fn new() -> Self {
        Self {
            reg_file: RegFile::new(),
            memory: VirtAddrManager::new(),
            pc: DEFAULT_PC_VALUE,
            decoder: Decoder::new(),
        }
    }

    // TODO: A builder struct may be useful for future use.
    pub fn from_memory(v_memory: VirtAddrManager) -> Self {
        Self {
            reg_file: RegFile::new(),
            memory: v_memory,
            pc: DEFAULT_PC_VALUE,
            decoder: Decoder::new(),
        }
    }

    fn execute(&mut self, instr: RiscvInstr, info: RVInstrInfo) -> Result<(), Exception> {
        let rst = get_exec_func(instr)(info, self);
        self.reg_file[0] = 0;

        rst
    }

    // TODO: Move or delete this when the debugger is implemented
    fn debug_reg_string(&self) -> String {
        let mut s = String::new();
        for i in 0..REGFILE_CNT {
            if self.reg_file[i] == 0 {
                continue;
            }

            s.push_str(&format!("{}: 0x{:x}", REG_NAME[i], self.reg_file[i]));
            if i != REGFILE_CNT - 1 {
                s.push_str(", ");
            }
        }
        s
    }

    pub fn step(&mut self) -> Result<(), Exception> {
        let instr_bytes = self.memory.read::<u32>(self.pc);
        log::trace!("raw instruction: {:#x} at pc {:#x}", instr_bytes, self.pc);
        let (instr, info) = self.decoder.decode(instr_bytes)?;
        log::trace!("Decoded instruction: {:#?}, info: {:?}", instr, info);
        self.execute(instr, info)?;

        log::trace!("{}", self.debug_reg_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha12Rng;

    use crate::{
        config::arch_config::REGFILE_CNT,
        ram_config::BASE_ADDR,
        utils::{UnsignedInteger, negative_of, sign_extend},
    };

    use super::*;

    struct TestCPUBuilder {
        cpu: RV32CPU,
    }

    impl TestCPUBuilder {
        fn new() -> Self {
            Self {
                cpu: RV32CPU::new(),
            }
        }

        fn reg(mut self, idx: u8, value: WordType) -> Self {
            self.cpu.reg_file.write(idx, value);
            self
        }

        fn pc(mut self, value: WordType) -> Self {
            self.cpu.pc = value;
            self
        }

        fn mem<T: UnsignedInteger>(mut self, addr: WordType, value: T) -> Self {
            self.cpu.memory.write(addr, value);
            self
        }

        fn mem_base<T: UnsignedInteger>(mut self, addr: WordType, value: T) -> Self {
            self.cpu.memory.write(BASE_ADDR + addr, value);
            self
        }

        fn build(self) -> RV32CPU {
            self.cpu
        }
    }

    struct CPUChecker<'a> {
        cpu: &'a mut RV32CPU,
    }

    impl<'a> CPUChecker<'a> {
        fn new(cpu: &'a mut RV32CPU) -> Self {
            Self { cpu }.reg(0, 0) // x0 is always 0
        }

        fn reg(self, idx: u8, value: WordType) -> Self {
            assert_eq!(
                self.cpu.reg_file.read(idx, 0).0,
                value,
                "Register #{} incorrect",
                idx,
            );
            self
        }

        fn pc(self, value: WordType) -> Self {
            assert_eq!(self.cpu.pc, value, "PC incorrect");
            self
        }

        fn mem<T>(self, addr: WordType, value: WordType) -> Self
        where
            T: UnsignedInteger,
        {
            assert_eq!(
                self.cpu.memory.read::<T>(addr).into(),
                value,
                "Memory value incorrect at pos {}",
                addr
            );
            self
        }

        fn mem_base<T>(self, addr: WordType, value: WordType) -> Self
        where
            T: UnsignedInteger,
        {
            self.mem::<T>(BASE_ADDR + addr, value)
        }
    }

    fn run_test_exec<F, G>(instr: RiscvInstr, info: RVInstrInfo, build: F, check: G)
    where
        F: FnOnce(TestCPUBuilder) -> TestCPUBuilder,
        G: FnOnce(CPUChecker) -> CPUChecker,
    {
        let mut cpu = build(TestCPUBuilder::new()).build();
        cpu.execute(instr, info).unwrap();
        check(CPUChecker::new(&mut cpu));
    }

    fn run_test_exec_decode<F, G>(raw_instr: u32, build: F, check: G)
    where
        F: FnOnce(TestCPUBuilder) -> TestCPUBuilder,
        G: FnOnce(CPUChecker) -> CPUChecker,
    {
        let mut cpu = build(TestCPUBuilder::new()).build();
        let (instr, info) = cpu.decoder.decode(raw_instr).unwrap();
        cpu.execute(instr, info).unwrap();
        check(CPUChecker::new(&mut cpu));
    }

    struct ExecTester {
        rng: ChaCha12Rng,
    }

    impl ExecTester {
        fn new() -> Self {
            Self {
                rng: ChaCha12Rng::seed_from_u64(0721),
            }
        }

        fn rand_imm12(&mut self) -> WordType {
            self.rng.random_range(0..=4095) as WordType
        }

        fn rand_word(&mut self) -> WordType {
            self.rng.random_range(0..=WordType::MAX)
        }

        fn rand_word2(&mut self) -> (WordType, WordType) {
            (self.rand_word(), self.rand_word())
        }

        fn rand_reg_idx(&mut self) -> u8 {
            self.rng.random_range(1..REGFILE_CNT) as u8
        }

        fn rand_reg_idx2(&mut self) -> (u8, u8) {
            (self.rand_reg_idx(), self.rand_reg_idx())
        }

        fn rand_unique_reg_idx2(&mut self) -> (u8, u8) {
            let idx1 = self.rand_reg_idx();
            let mut idx2 = self.rand_reg_idx();
            while idx1 == idx2 {
                idx2 = self.rand_reg_idx();
            }
            (idx1, idx2)
        }

        fn test_rand_r_with(
            &mut self,
            instr: RiscvInstr,
            lhs: WordType,
            rhs: WordType,
            expected: WordType,
        ) {
            let rd = self.rand_reg_idx();
            let (rs1, rs2) = self.rand_unique_reg_idx2();
            let info = RVInstrInfo::R { rd, rs1, rs2 };

            run_test_exec(
                instr,
                info,
                |builder| builder.reg(rs1, lhs).reg(rs2, rhs).pc(0x1000),
                |checker| checker.reg(rd, expected).pc(0x1004),
            );
        }

        fn test_rand_r<F>(&mut self, instr: RiscvInstr, calc: F)
        where
            F: FnOnce(WordType, WordType) -> WordType,
        {
            let (val1, val2) = self.rand_word2();
            self.test_rand_r_with(instr, val1, val2, calc(val1, val2));
        }

        fn test_rand_i_with(
            &mut self,
            instr: RiscvInstr,
            lhs: WordType,
            imm: WordType,
            expected: WordType,
        ) {
            let (rd, rs1) = self.rand_reg_idx2();
            let info = RVInstrInfo::I { rd, rs1, imm };

            run_test_exec(
                instr,
                info,
                |builder| builder.reg(rs1, lhs).pc(0x1000),
                |checker| checker.reg(rd, expected).pc(0x1004),
            );
        }

        fn test_rand_i<F>(&mut self, instr: RiscvInstr, calc: F)
        where
            F: FnOnce(WordType, WordType) -> WordType,
        {
            let val = self.rand_word();
            let imm = self.rand_imm12();
            self.test_rand_i_with(instr, val, imm, calc(val, sign_extend(imm, 12)));
        }
    }

    #[test]
    fn test_exec_arith() {
        let mut tester = ExecTester::new();

        run_test_exec(
            RiscvInstr::ADDI,
            RVInstrInfo::I {
                rd: 2,
                rs1: 3,
                imm: negative_of(5),
            },
            |builder| builder.reg(3, 10).pc(0x2000),
            |checker| checker.reg(2, 5).pc(0x2004),
        );

        for _i in 1..=100 {
            tester.test_rand_r(RiscvInstr::ADD, |lhs, rhs| lhs.wrapping_add(rhs));
            tester.test_rand_r(RiscvInstr::SUB, |lhs, rhs| lhs.wrapping_sub(rhs));
            tester.test_rand_i(RiscvInstr::ADDI, |lhs, imm| lhs.wrapping_add(imm));

            // TODO: Add some handmade data,
            // because tests and actual codes are actually written in similar ways.
            tester.test_rand_i(RiscvInstr::SLTI, |lhs, imm| {
                ((lhs.cast_signed()) < (sign_extend(imm, 12).cast_signed())) as WordType
            });
            tester.test_rand_i(RiscvInstr::SLTIU, |lhs, imm| {
                ((lhs) < (sign_extend(imm, 12))) as WordType
            });
        }
    }

    #[test]
    fn test_exec_arith_decode() {
        // TODO: add checks for boundary cases
        run_test_exec_decode(
            0x02520333, // mul x6, x4, x5
            |builder| builder.reg(4, 5).reg(5, 10).pc(0x1000),
            |checker| checker.reg(6, 50).pc(0x1004),
        );
    }

    #[test]
    fn test_load_store_decode() {
        run_test_exec_decode(
            0x00812183, // lw x3, 8(x2)
            |builder| builder.reg(2, BASE_ADDR).mem_base::<u32>(8, 123).pc(0x1000),
            |checker| checker.reg(3, 123).pc(0x1004),
        );

        run_test_exec_decode(
            0xfec42783, // lw a5,-20(s0)
            |builder| {
                builder
                    .reg(8, BASE_ADDR + 36)
                    .mem_base(16, 123 as u32)
                    .pc(0x1000)
            },
            |checker| checker.reg(15, 123).pc(0x1004),
        );

        run_test_exec_decode(
            0xfe112c23, // sw x1, -8(x2)
            |builder| builder.reg(2, BASE_ADDR + 16).reg(1, 123),
            |checker| checker.mem_base::<u32>(8, 123),
        );

        run_test_exec_decode(
            0xfcf42e23, //          	sw	a5,-36(s0)
            |builder| builder.reg(15, 123).reg(8, BASE_ADDR + 72),
            |checker| checker.mem_base::<u32>(36, 123),
        );
    }

    #[test]
    fn test_u_types_decode() {
        // TODO: Test signed extend of `auipc`
        run_test_exec_decode(
            0x12233097, // auipc x1, 0x112233
            |builder| builder.reg(1, 3).pc(0x1000),
            |checker| checker.reg(1, 0x12234000).pc(0x1004),
        );

        run_test_exec_decode(
            0x123451b7, //lui x3, 0x12345
            |builder| builder.reg(3, 0x54321).pc(0x1000),
            |checker| checker.reg(3, 0x12345000).pc(0x1004),
        );
    }

    #[test]
    fn test_branch_decode() {
        run_test_exec_decode(
            0xf8c318e3, // bne x6, x12, -112
            |builder| builder.reg(6, 5).reg(12, 10).pc(0x2000),
            |checker| checker.pc(0x2000 - 112),
        );

        run_test_exec_decode(
            0xf8c318e3, // bne x6, x12, -112
            |builder| builder.reg(6, 5).reg(12, 5).pc(0x2000),
            |checker| checker.pc(0x2004),
        );
    }

    #[test]
    fn test_jump_decode() {
        run_test_exec_decode(
            0xf81ff06f, // jal x0, -128
            |builder| builder.reg(0, 0).pc(0x1234),
            |checker| checker.pc(0x1234 - 128),
        );

        run_test_exec_decode(
            0x00078067, // jr a5
            |builder| builder.reg(15, 0x2468).pc(0x1234),
            |checker| checker.pc(0x2468),
        );
    }
}
