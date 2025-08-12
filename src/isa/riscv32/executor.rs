use crate::{
    config::arch_config::WordType,
    cpu::RegFile,
    device::Mem,
    isa::riscv32::{
        decoder::Decoder,
        instruction::{
            Exception, RVInstrInfo,
            rv32i_table::{Riscv32Instr, get_exec_func},
        },
    },
    ram_config::DEFAULT_PC_VALUE,
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

    fn execute(&mut self, instr: Riscv32Instr, info: RVInstrInfo) -> Result<(), Exception> {
        let rst = get_exec_func(instr)(info, self);
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
    use rand::{Rng, SeedableRng};
    use rand_chacha::ChaCha12Rng;

    use crate::{config::arch_config::REGFILE_CNT, ram_config::BASE_ADDR, utils::UnsignedInteger};

    use super::*;

    #[test]
    fn test_sign_extend() {
        assert_eq!(sign_extend(0x123, 12), 0x123);
        assert_eq!(sign_extend(0x7FF, 12), 0x7FF);
        assert_eq!(sign_extend(0xFFF, 12), !0 as WordType);
        assert_eq!(sign_extend(0xF0F, 12), (!0 - 0xF0) as WordType);
    }

    #[test]
    fn test_negative_of() {
        assert_eq!(negative_of(1 as WordType), (!0) as WordType);
        assert_eq!(negative_of(2 as WordType), (!0 - 1) as WordType);
    }

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

    fn run_test_exec<F, G>(instr: Riscv32Instr, info: RVInstrInfo, build: F, check: G)
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
            instr: Riscv32Instr,
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

        fn test_rand_r<F>(&mut self, instr: Riscv32Instr, calc: F)
        where
            F: FnOnce(WordType, WordType) -> WordType,
        {
            let (val1, val2) = self.rand_word2();
            self.test_rand_r_with(instr, val1, val2, calc(val1, val2));
        }

        fn test_rand_i_with(
            &mut self,
            instr: Riscv32Instr,
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

        fn test_rand_i<F>(&mut self, instr: Riscv32Instr, calc: F)
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
            Riscv32Instr::ADDI,
            RVInstrInfo::I {
                rd: 2,
                rs1: 3,
                imm: negative_of(5),
            },
            |builder| builder.reg(3, 10).pc(0x2000),
            |checker| checker.reg(2, 5).pc(0x2004),
        );

        for _i in 1..=100 {
            tester.test_rand_r(Riscv32Instr::ADD, |lhs, rhs| lhs.wrapping_add(rhs));
            tester.test_rand_r(Riscv32Instr::SUB, |lhs, rhs| lhs.wrapping_sub(rhs));
            tester.test_rand_i(Riscv32Instr::ADDI, |lhs, imm| lhs.wrapping_add(imm));

            // TODO: Add some handmade data,
            // because tests and actual codes are actually written in similar ways.
            tester.test_rand_i(Riscv32Instr::SLTI, |lhs, imm| {
                ((lhs.cast_signed()) < (sign_extend(imm, 12).cast_signed())) as WordType
            });
            tester.test_rand_i(Riscv32Instr::SLTIU, |lhs, imm| {
                ((lhs) < (sign_extend(imm, 12))) as WordType
            });
        }
    }

    #[test]
    fn test_load_store_decode() {
        run_test_exec_decode(
            0x00812183, // lw x3, 8(x2)
            |builder| builder.reg(2, BASE_ADDR).mem_base::<u64>(8, 123).pc(0x1000),
            |checker| checker.reg(3, 123).pc(0x1004),
        );

        run_test_exec_decode(
            0xfe112c23, // sw x1, -8(x2)
            |builder| builder.reg(2, BASE_ADDR + 16).reg(1, 123),
            |checker| checker.mem_base::<u32>(8, 123),
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
    }
}
