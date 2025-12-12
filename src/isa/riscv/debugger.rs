use std::{collections::VecDeque, fmt::Debug, ops::Add, u64};

use crate::{
    config::arch_config::WordType,
    device::MemError,
    isa::{
        DebugTarget, DecoderTrait, ISATypes,
        riscv::{
            RiscvTypes, csr_reg::PrivilegeLevel, decoder::DecodeInstr, executor::RVCPU,
            trap::Exception,
        },
    },
    utils::UnsignedInteger,
};

#[derive(Debug, Clone)]
pub enum DebugEvent {
    StepCompleted,
    BreakpointHit,
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

impl DebugTarget<RiscvTypes> for RVCPU {
    fn read_pc(&self) -> WordType {
        self.pc
    }

    fn write_pc(&mut self, new_pc: WordType) {
        self.pc = new_pc;
    }

    fn read_reg(&self, idx: u8) -> WordType {
        self.reg_file[idx as usize]
    }

    fn write_reg(&mut self, idx: u8, value: WordType) {
        self.reg_file.write(idx, value)
    }

    fn read_instr(&mut self, vaddr: WordType) -> Result<u32, MemError> {
        self.memory.debug_ifetch(vaddr, &mut self.csr)
    }

    fn read_instr_directly(&mut self, addr: Address) -> Result<u32, MemError> {
        self.memory.debug_read(addr)
    }

    fn read_memory<T: UnsignedInteger>(&mut self, addr: Address) -> Result<T, MemError> {
        self.memory.debug_read::<T>(addr)
    }

    fn write_memory<T: UnsignedInteger>(&mut self, addr: Address, data: T) -> Result<(), MemError> {
        self.memory.debug_write::<T>(addr, data)
    }

    fn get_current_privilege(&self) -> PrivilegeLevel {
        self.csr.privelege_level()
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
        RVCPU::step(self)
    }

    fn decoded_instr(&self, instr: u32) -> Option<DecodeInstr> {
        self.decoder.decode(instr)
    }

    fn vaddr_to_paddr(&self, vaddr: WordType) -> Option<u64> {
        self.memory.debug_translate_vaddr(vaddr).ok()
    }

    fn translate(&self, vaddr: WordType) -> Option<u64> {
        if self.get_current_privilege() == PrivilegeLevel::M {
            Some(vaddr as u64)
        } else {
            self.memory.debug_translate_vaddr(vaddr).ok()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Address {
    Virt(WordType),
    Phys(u64),
}

impl Address {
    pub fn value(&self) -> u64 {
        match self.clone() {
            Address::Virt(vaddr) => vaddr as u64,
            Address::Phys(paddr) => paddr,
        }
    }
}

impl Add<u64> for Address {
    type Output = Address;

    fn add(self, rhs: u64) -> Self::Output {
        match self {
            Address::Phys(addr) => Address::Phys(addr + rhs),
            Address::Virt(addr) => Address::Virt(addr + rhs as WordType),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Breakpoint {
    pub id: usize,
    pub addr: Address,
}

const SAVE_PC_CNT: usize = 50;

pub struct Debugger<'a, I: ISATypes> {
    breakpoints: Vec<Breakpoint>,
    target: &'a mut I::CPU,
    history: VecDeque<(WordType, Option<I::RawInstr>)>,
}

impl<'a, I: ISATypes> Debugger<'a, I> {
    pub fn new(target: &'a mut I::CPU) -> Self {
        Self {
            breakpoints: Vec::new(),
            target: target,
            history: VecDeque::with_capacity(SAVE_PC_CNT),
        }
    }

    fn push_history(&mut self) {
        if self.history.len() == SAVE_PC_CNT {
            self.history.pop_front();
        }
        let instr = self.read_instr(self.read_pc());
        self.history.push_back((self.read_pc(), instr));
    }

    pub fn pc_history(&self) -> impl Iterator<Item = (WordType, Option<I::RawInstr>)> {
        self.history.iter().copied()
    }

    pub fn breakpoints(&self) -> &Vec<Breakpoint> {
        &self.breakpoints
    }

    pub fn set_breakpoint(&mut self, addr: Address) -> Result<(), DebugError<I>> {
        if let Some(_) = self.breakpoints.iter().find(|bp| bp.addr == addr) {
            return Ok(());
        }
        let breakpoint = Breakpoint {
            id: self.breakpoints.len(),
            addr,
        };
        self.breakpoints.push(breakpoint);

        Ok(())
    }

    pub fn clear_breakpoint(&mut self, addr: Address) -> Result<(), DebugError<I>> {
        self.breakpoints.retain(|bp| bp.addr != addr);
        Ok(())
    }

    pub fn on_breakpoint(&self) -> bool {
        if let Some(pc_paddr) = self.target.translate(self.read_pc()) {
            self.breakpoints
                .iter()
                .find(|bp| self.unify_to_phys_addr(bp.addr) == Some(pc_paddr))
                .is_some()
        } else {
            log::warn!(
                "Cannot translate current PC 0x{:08x} to physical address.",
                self.read_pc()
            );
            false
        }
    }

    pub fn step(&mut self) -> Result<DebugEvent, DebugError<I>> {
        self.continue_until_step(1)
    }

    pub fn continue_until_step(&mut self, max_steps: u64) -> Result<DebugEvent, DebugError<I>> {
        if max_steps == 0 {
            return Ok(DebugEvent::StepCompleted);
        }

        let mut remain = max_steps;
        if self.on_breakpoint() {
            self.push_history();
            if let Err(e) = self.target.step() {
                return Err(DebugError::TargetException(e));
            }
            remain -= 1;
        }

        loop {
            if remain == 0 {
                return Ok(DebugEvent::StepCompleted);
            }

            if self.on_breakpoint() {
                return Ok(DebugEvent::BreakpointHit);
            }

            self.push_history();
            if let Err(e) = self.target.step() {
                return Err(DebugError::TargetException(e));
            }
            remain -= 1;
        }
    }

    pub fn continue_run(&mut self) -> Result<DebugEvent, DebugError<I>> {
        self.continue_until_step(u64::MAX)
    }

    // helper functions that don't need support from `DebugTarget`

    pub fn current_instr(&mut self) -> Option<I::RawInstr> {
        self.target.read_instr(self.target.read_pc()).ok()
    }

    pub fn read_instr_directly(&mut self, addr: Address) -> Option<I::RawInstr> {
        self.target.read_instr_directly(addr).ok()
    }

    pub fn unify_to_phys_addr(&self, addr: Address) -> Option<u64> {
        match addr {
            Address::Phys(paddr) => Some(paddr),
            Address::Virt(vaddr) => self.vaddr_to_paddr(vaddr),
        }
    }

    // re-export methods from `DebugTarget`

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

    pub fn read_instr(&mut self, addr: WordType) -> Option<I::RawInstr> {
        self.target.read_instr(addr).ok()
    }

    pub fn read_memory<V: UnsignedInteger>(&mut self, addr: Address) -> Result<V, MemError> {
        self.target.read_memory(addr)
    }

    pub fn write_memory<V: UnsignedInteger>(
        &mut self,
        addr: Address,
        data: V,
    ) -> Result<(), MemError> {
        self.target.write_memory::<V>(addr, data)
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

    pub fn decoded_info(&self, raw: I::RawInstr) -> Option<I::DecodeRst> {
        self.target.decoded_instr(raw)
    }

    pub fn vaddr_to_paddr(&self, vaddr: WordType) -> Option<u64> {
        self.target.vaddr_to_paddr(vaddr)
    }

    pub fn translate(&self, addr: WordType) -> Option<u64> {
        self.target.translate(addr)
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
        debugger
            .set_breakpoint(Address::Phys(BASE_ADDR + 4))
            .unwrap();
        debugger.continue_run().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert_eq!(
            debugger
                .read_memory::<u32>(Address::Phys(BASE_ADDR + 4))
                .unwrap(),
            0x02520333
        );

        debugger.step().unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 8);

        debugger
            .set_breakpoint(Address::Phys(BASE_ADDR + 12))
            .unwrap();

        debugger.continue_until_step(2).unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 12);
        assert_eq!(
            debugger
                .read_memory::<u32>(Address::Phys(BASE_ADDR + 12))
                .unwrap(),
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
        debugger.set_breakpoint(Address::Phys(BASE_ADDR)).unwrap();

        debugger.step().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
    }
}
