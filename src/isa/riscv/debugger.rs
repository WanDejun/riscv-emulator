use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Debug,
    u64,
};

use crate::{
    config::arch_config::WordType,
    device::MemError,
    isa::{
        DebugTarget, DecoderTrait, HasBreakpointException, ISATypes,
        icache::ICache,
        riscv::{
            RiscvTypes, csr_reg::PrivilegeLevel, decoder::DecodeInstr, executor::RV32CPU,
            trap::Exception,
        },
    },
    utils::UnsignedInteger,
};

#[derive(Debug, Clone)]
pub enum DebugEvent {
    StepCompleted { pc: WordType }, // TODO: Remove useless arg `pc` here.
    BreakpointHit { pc: WordType },
}

#[derive(thiserror::Error, Debug)]
pub enum DebugError<I: ISATypes> {
    #[error("target exception: {0:?}")]
    TargetException(I::StepException),

    #[error("memory error: {0:?}")]
    MemoryError(MemError),

    #[error("CSR {0:?} not exist")]
    CSRNotExist(WordType),
}

impl<I: ISATypes> From<MemError> for DebugError<I> {
    fn from(e: MemError) -> Self {
        DebugError::MemoryError(e)
    }
}

impl DebugTarget<RiscvTypes> for RV32CPU {
    fn read_pc(&self) -> WordType {
        self.pc
    }

    fn write_pc(&mut self, new_pc: WordType) {
        self.pc = new_pc;
    }

    fn read_instr(&mut self, addr: WordType) -> Result<u32, MemError> {
        self.memory
            .get_instr_code_without_side_effect::<u32>(addr, &mut self.csr)
    }

    fn write_back_instr(&mut self, instr: u32, addr: WordType) -> Result<(), MemError> {
        self.memory.write_by_paddr(addr, instr)?;
        self.icache.invalidate(addr);
        Ok(())
    }

    fn read_reg(&self, idx: u8) -> WordType {
        self.reg_file[idx as usize]
    }

    fn write_reg(&mut self, idx: u8, value: WordType) {
        self.reg_file.write(idx, value)
    }

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> Result<T, MemError> {
        self.memory.read_by_paddr::<T>(addr)
    }

    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T) -> Result<(), MemError> {
        self.memory.write_by_paddr::<T>(addr, data)
    }

    fn get_current_privilege(&self) -> PrivilegeLevel {
        self.csr.get_current_privileged()
    }

    fn read_float_reg(&self, idx: u8) -> f64 {
        self.fpu.load::<f32>(idx) as f64 // TODO: Support debugging f64 (which needs to implement NAN boxing).
    }

    /// match input {
    ///     Some() => `Read Write`,
    ///     None => `Read Only`,
    /// }
    fn debug_csr(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType> {
        self.csr.debug(addr, new_value)
    }

    fn step(&mut self) -> Result<(), Exception> {
        RV32CPU::step(self)
    }

    fn decoded_info(&mut self, instr: u32) -> Option<DecodeInstr> {
        self.decoder.decode(instr)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Breakpoint {
    pub pc: WordType,
}

impl Breakpoint {
    pub fn new(pc: WordType) -> Self {
        Self { pc }
    }
}

const SAVE_PC_CNT: usize = 50;

pub struct Debugger<'a, I: ISATypes> {
    breakpoints: BTreeMap<Breakpoint, I::RawInstr>,
    target: &'a mut I::CPU,
    pc_history: VecDeque<WordType>,
}

impl<'a, I: ISATypes> Debugger<'a, I> {
    pub fn new(target: &'a mut I::CPU) -> Self {
        Self {
            breakpoints: BTreeMap::new(),
            target: target,
            pc_history: VecDeque::with_capacity(SAVE_PC_CNT),
        }
    }

    fn push_history(&mut self) {
        if self.pc_history.len() == SAVE_PC_CNT {
            self.pc_history.pop_front();
        }
        self.pc_history.push_back(self.read_pc());
    }

    pub fn pc_history(&self) -> impl Iterator<Item = WordType> {
        self.pc_history.iter().copied()
    }

    pub fn breakpoints(&self) -> &BTreeMap<Breakpoint, I::RawInstr> {
        &self.breakpoints
    }

    pub fn set_breakpoint(&mut self, addr: WordType) -> Result<(), DebugError<I>> {
        let breakpoint = Breakpoint::new(addr);
        if self.breakpoints.contains_key(&breakpoint) {
            return Ok(());
        }
        let orig: I::RawInstr = self.target.read_instr(addr)?;
        self.breakpoints.insert(breakpoint, orig);
        if addr != self.read_pc() {
            self.target.write_back_instr(I::EBREAK, addr)?;
        }

        Ok(())
    }

    pub fn clear_breakpoint(&mut self, pc: WordType) -> Result<(), DebugError<I>> {
        if let Some(orig) = self.breakpoints.remove(&Breakpoint { pc }) {
            self.target.write_back_instr(orig, pc)?;
        }

        Ok(())
    }

    fn on_breakpoint(&mut self) -> bool {
        self.breakpoints
            .contains_key(&Breakpoint::new(self.read_pc()))
    }

    fn place_origin_on_break(&mut self) -> Result<(), DebugError<I>> {
        let pc = self.read_pc();
        log::debug!("Placing origin instruction on breakpoint at {:08x}", pc);

        // We cannot panic here because the original program could contains "ebreak"
        if let Some(instr) = self.breakpoints.get(&Breakpoint::new(pc)) {
            self.target.write_back_instr(*instr, pc)?;
        } else {
            log::debug!("No original instruction found for breakpoint at {:08x}", pc);
        }

        Ok(())
    }

    fn step_over_breakpoint(&mut self) -> Result<(), DebugError<I>> {
        let pc = self.read_pc();
        match self.target.step() {
            Ok(()) => {
                self.target.write_back_instr(I::EBREAK, pc)?;
                Ok(())
            }
            Err(e) => Err(DebugError::TargetException(e)),
        }
    }

    pub fn step(&mut self) -> Result<DebugEvent, DebugError<I>> {
        self.continue_until_step(1)
    }

    pub fn continue_until_step(&mut self, max_steps: u64) -> Result<DebugEvent, DebugError<I>> {
        let mut rest = max_steps;
        if self.on_breakpoint() {
            self.step_over_breakpoint()?;
            rest -= 1;
        }

        loop {
            if rest == 0 {
                return Ok(DebugEvent::StepCompleted { pc: self.read_pc() });
            }

            self.push_history();
            match self.target.step() {
                Ok(()) => {
                    rest -= 1;
                }
                Err(e) => {
                    if e.is_breakpoint() {
                        self.place_origin_on_break()?;
                        return Ok(DebugEvent::BreakpointHit { pc: self.read_pc() });
                    } else {
                        return Err(DebugError::TargetException(e));
                    }
                }
            }
        }
    }

    pub fn continue_run(&mut self) -> Result<DebugEvent, DebugError<I>> {
        self.continue_until_step(u64::MAX)
    }

    pub fn read_reg(&self, idx: u8) -> WordType {
        self.target.read_reg(idx)
    }

    pub fn write_reg(&mut self, idx: u8, val: WordType) {
        self.target.write_reg(idx, val)
    }

    pub fn read_pc(&self) -> WordType {
        self.target.read_pc()
    }

    pub fn write_pc(&mut self, val: WordType) {
        self.target.write_pc(val)
    }

    pub fn read_float_reg(&self, idx: u8) -> f64 {
        self.target.read_float_reg(idx)
    }

    pub fn read_mem<V: UnsignedInteger>(&mut self, addr: WordType) -> Result<V, MemError> {
        self.target.read_mem::<V>(addr)
    }

    pub fn read_origin_instr(&mut self, addr: WordType) -> Result<I::RawInstr, MemError> {
        if let Some(instr) = self.breakpoints.get(&Breakpoint::new(addr)) {
            Ok(*instr)
        } else {
            self.target.read_instr(addr)
        }
    }

    pub fn write_mem<V: UnsignedInteger>(
        &mut self,
        addr: WordType,
        data: V,
    ) -> Result<(), MemError> {
        self.target.write_mem::<V>(addr, data)
    }

    pub fn read_csr(&mut self, addr: WordType) -> Option<WordType> {
        self.target.debug_csr(addr, None)
    }

    pub fn write_csr(&mut self, addr: WordType, data: WordType) -> Result<(), DebugError<I>> {
        self.target
            .debug_csr(addr, Some(data))
            .ok_or(DebugError::<I>::CSRNotExist(addr))?;
        Ok(())
    }

    pub fn get_current_privilege(&mut self) -> PrivilegeLevel {
        self.target.get_current_privilege()
    }

    pub fn decoded_info(&mut self, raw: I::RawInstr) -> Option<I::DecodeRst> {
        self.target.decoded_info(raw)
    }
}

#[cfg(test)]
mod test {
    use crate::{isa::riscv::cpu_tester::TestCPUBuilder, ram_config::BASE_ADDR};

    use super::*;

    #[test]
    fn test_breakpoint_riscv() {
        // Test that a breakpoint can be hit
        let mut cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = Debugger::<RiscvTypes>::new(&mut cpu);
        debugger.set_breakpoint(BASE_ADDR + 4).unwrap();
        debugger.continue_run().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert!(debugger.on_breakpoint());
        assert_eq!(debugger.read_mem::<u32>(BASE_ADDR + 4).unwrap(), 0x02520333);

        debugger.step().unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 8);

        debugger.set_breakpoint(BASE_ADDR + 12).unwrap();
        assert_eq!(
            debugger.read_origin_instr(BASE_ADDR + 12).unwrap(),
            0x02520333
        );

        debugger.continue_until_step(2).unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 12);
        assert_eq!(
            debugger.read_mem::<u32>(BASE_ADDR + 12).unwrap(),
            0x02520333
        );
    }

    #[test]
    fn test_breakpoint_riscv_on_current() {
        let mut cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = Debugger::<RiscvTypes>::new(&mut cpu);
        debugger.set_breakpoint(BASE_ADDR).unwrap();
        assert!(debugger.on_breakpoint());

        debugger.step().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert!(debugger.on_breakpoint() == false);
        assert_eq!(debugger.read_mem::<u32>(BASE_ADDR).unwrap(), 0x0010_0073); // ebreak
    }
}
