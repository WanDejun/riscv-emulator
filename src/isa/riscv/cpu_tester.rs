#![cfg(test)]
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;

use crate::{
    config::arch_config::{REGFILE_CNT, WordType},
    device::Mem,
    isa::{
        DecoderTrait,
        riscv::{
            decoder::DecodeInstr,
            executor::RV32CPU,
            instruction::{RVInstrInfo, rv32i_table::RiscvInstr},
        },
    },
    ram_config::{self, BASE_ADDR},
    utils::{UnsignedInteger, sign_extend},
};

pub(super) struct TestCPUBuilder {
    cpu: RV32CPU,
}

impl TestCPUBuilder {
    pub(super) fn new() -> Self {
        Self {
            cpu: RV32CPU::new(),
        }
    }

    pub(super) fn reg(mut self, idx: u8, value: WordType) -> Self {
        self.cpu.reg_file.write(idx, value);
        self
    }

    pub(super) fn pc(mut self, value: WordType) -> Self {
        self.cpu.pc = value;
        self
    }

    pub(super) fn mem<T: UnsignedInteger>(mut self, addr: WordType, value: T) -> Self {
        self.cpu.memory.write(addr, value).unwrap();
        self
    }

    pub(super) fn mem_base<T: UnsignedInteger>(mut self, addr: WordType, value: T) -> Self {
        self.cpu.memory.write(BASE_ADDR + addr, value).unwrap();
        self
    }

    pub(super) fn program(mut self, instrs: &[u32]) -> Self {
        let mut addr = BASE_ADDR;
        for instr in instrs {
            self.cpu.memory.write(addr, *instr).unwrap();
            addr += 4;
        }
        self
    }

    pub(super) fn csr(mut self, csr_addr: WordType, value: WordType) -> Self {
        self.cpu.csr.write(csr_addr, value);
        self
    }

    pub(super) fn build(self) -> RV32CPU {
        self.cpu
    }
}

pub(super) struct CPUChecker<'a> {
    pub(super) cpu: &'a mut RV32CPU,
}

impl<'a> CPUChecker<'a> {
    pub(super) fn new(cpu: &'a mut RV32CPU) -> Self {
        Self { cpu }.reg(0, 0) // x0 is always 0
    }

    pub(super) fn reg(self, idx: u8, value: WordType) -> Self {
        assert_eq!(
            self.cpu.reg_file.read(idx, 0).0,
            value,
            "Register #{} incorrect",
            idx,
        );
        self
    }

    pub(super) fn pc(self, value: WordType) -> Self {
        assert_eq!(self.cpu.pc, value, "PC incorrect");
        self
    }

    pub(super) fn mem<T>(self, addr: WordType, value: WordType) -> Self
    where
        T: UnsignedInteger,
    {
        assert_eq!(
            self.cpu.memory.read::<T>(addr).unwrap().into(),
            value,
            "Memory value incorrect at pos {}",
            addr
        );
        self
    }

    pub(super) fn mem_base<T>(self, addr: WordType, value: WordType) -> Self
    where
        T: UnsignedInteger,
    {
        self.mem::<T>(BASE_ADDR + addr, value)
    }

    pub(super) fn csr(self, addr: WordType, value: WordType) -> Self {
        assert_eq!(self.cpu.csr.read(addr).unwrap(), value);
        self
    }

    pub(super) fn customized<F>(self, f: F) -> Self
    where
        F: FnOnce(Self) -> Self,
    {
        f(self)
    }
}

pub(super) fn run_test_exec<F, G>(instr: RiscvInstr, info: RVInstrInfo, build: F, check: G)
where
    F: FnOnce(TestCPUBuilder) -> TestCPUBuilder,
    G: FnOnce(CPUChecker) -> CPUChecker,
{
    let mut cpu = build(TestCPUBuilder::new()).build();
    cpu.execute(instr, info).unwrap();
    check(CPUChecker::new(&mut cpu));
}

pub(super) fn run_test_exec_decode<F, G>(raw_instr: u32, build: F, check: G)
where
    F: FnOnce(TestCPUBuilder) -> TestCPUBuilder,
    G: FnOnce(CPUChecker) -> CPUChecker,
{
    let mut cpu = build(TestCPUBuilder::new()).build();
    let DecodeInstr(instr, info) = cpu.decoder.decode(raw_instr).unwrap();
    cpu.execute(instr, info).unwrap();
    check(CPUChecker::new(&mut cpu));
}

pub(super) fn run_test_cpu_step<F, G>(raw_instrs: &[u32], build: F, check: G)
where
    F: FnOnce(TestCPUBuilder) -> TestCPUBuilder,
    G: FnOnce(CPUChecker) -> CPUChecker,
{
    let mut builder = build(TestCPUBuilder::new());
    for (i, inst) in raw_instrs.iter().enumerate() {
        builder = builder.mem(i as WordType + ram_config::BASE_ADDR, *inst);
    }
    let mut cpu = builder.build();
    for _ in 0..raw_instrs.len() {
        cpu.step().unwrap()
    }
    check(CPUChecker::new(&mut cpu));
}

pub(super) struct ExecTester {
    rng: ChaCha12Rng,
}

impl ExecTester {
    pub(super) fn new() -> Self {
        Self {
            rng: ChaCha12Rng::seed_from_u64(0721),
        }
    }

    pub(super) fn rand_imm12(&mut self) -> WordType {
        self.rng.random_range(0..=4095) as WordType
    }

    pub(super) fn rand_word(&mut self) -> WordType {
        self.rng.random_range(0..=WordType::MAX)
    }

    pub(super) fn rand_word2(&mut self) -> (WordType, WordType) {
        (self.rand_word(), self.rand_word())
    }

    pub(super) fn rand_reg_idx(&mut self) -> u8 {
        self.rng.random_range(1..REGFILE_CNT) as u8
    }

    pub(super) fn rand_reg_idx2(&mut self) -> (u8, u8) {
        (self.rand_reg_idx(), self.rand_reg_idx())
    }

    pub(super) fn rand_unique_reg_idx2(&mut self) -> (u8, u8) {
        let idx1 = self.rand_reg_idx();
        let mut idx2 = self.rand_reg_idx();
        while idx1 == idx2 {
            idx2 = self.rand_reg_idx();
        }
        (idx1, idx2)
    }

    pub(super) fn test_rand_r_with(
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

    pub(super) fn test_rand_r<F>(&mut self, instr: RiscvInstr, calc: F)
    where
        F: FnOnce(WordType, WordType) -> WordType,
    {
        let (val1, val2) = self.rand_word2();
        self.test_rand_r_with(instr, val1, val2, calc(val1, val2));
    }

    pub(super) fn test_rand_i_with(
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

    pub(super) fn test_rand_i<F>(&mut self, instr: RiscvInstr, calc: F)
    where
        F: FnOnce(WordType, WordType) -> WordType,
    {
        let val = self.rand_word();
        let imm = self.rand_imm12();
        self.test_rand_i_with(instr, val, imm, calc(val, sign_extend(imm, 12)));
    }
}
