use std::{collections::BTreeMap, fmt::Debug, u64};

use crate::{
    config::arch_config::WordType,
    device::{Mem, MemError},
    isa::riscv::{self, executor::RV32CPU, trap::Exception},
    utils::UnsignedInteger,
};

#[derive(Debug, Clone)]
pub enum DebugEvent {
    StepCompleted { pc: WordType },
    BreakpointHit { pc: WordType },
}

#[derive(thiserror::Error, Debug)]
pub enum DebugError {
    #[error("target exception: {0:?}")]
    TargetException(Exception),
}

pub trait DebugTarget {
    type DecodeInstr;

    fn read_pc(&self) -> WordType;
    fn write_pc(&mut self, new_pc: WordType);

    fn read_reg(&self, idx: u8) -> WordType;
    fn write_reg(&mut self, idx: u8, value: WordType);

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> Result<T, MemError>;
    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T) -> Result<(), MemError>;

    fn step(&mut self) -> Result<(), Exception>;

    fn decoded_info(&mut self, addr: u32) -> Option<Self::DecodeInstr>;
}

impl DebugTarget for RV32CPU {
    type DecodeInstr = riscv::decoder::DecodeInstr;

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

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> Result<T, MemError> {
        self.memory.read::<T>(addr)
    }

    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T) -> Result<(), MemError> {
        self.memory.write::<T>(addr, data)
    }

    fn step(&mut self) -> Result<(), Exception> {
        RV32CPU::step(self)
    }

    fn decoded_info(&mut self, instr: u32) -> Option<Self::DecodeInstr> {
        self.decoder.decode(instr).ok()
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

pub struct Debugger<T: DebugTarget> {
    breakpoints: BTreeMap<Breakpoint, u32>,
    target: T,
}

const EBREAK: u32 = 0x0010_0073;

impl<T: DebugTarget> Debugger<T> {
    pub fn new(target: T) -> Self {
        Self {
            breakpoints: BTreeMap::new(),
            target: target,
        }
    }

    pub fn breakpoints(&self) -> &BTreeMap<Breakpoint, u32> {
        &self.breakpoints
    }

    pub fn set_breakpoint(&mut self, pc: WordType) {
        let breakpoint = Breakpoint::new(pc);
        if self.breakpoints.contains_key(&breakpoint) {
            return;
        }
        let orig: u32 = self.read_mem(pc).unwrap();
        self.breakpoints.insert(breakpoint, orig);
        if orig != EBREAK && pc != self.read_pc() {
            self.write_mem(pc, EBREAK).unwrap();
        }
    }

    pub fn clear_breakpoint(&mut self, pc: WordType) {
        if let Some(orig) = self.breakpoints.remove(&Breakpoint { pc }) {
            self.write_mem(pc, orig).unwrap();
        }
    }

    fn on_breakpoint(&mut self) -> bool {
        self.breakpoints
            .contains_key(&Breakpoint::new(self.read_pc()))
    }

    fn place_origin_on_break(&mut self) {
        let pc = self.read_pc();
        log::debug!("Placing origin instruction on breakpoint at {:08x}", pc);

        let instr = self
            .breakpoints
            .get(&Breakpoint::new(pc))
            .expect("Breakpoint should exist");

        self.write_mem::<u32>(pc, *instr).unwrap();
    }

    fn step_over_breakpoint(&mut self) -> Result<(), DebugError> {
        let pc = self.read_pc();
        match self.target.step() {
            Ok(()) => {
                self.write_mem::<u32>(pc, EBREAK).unwrap();
                Ok(())
            }
            Err(e) => Err(DebugError::TargetException(e)),
        }
    }

    pub fn step(&mut self) -> Result<DebugEvent, DebugError> {
        if self.on_breakpoint() {
            self.step_over_breakpoint()?;
            Ok(DebugEvent::StepCompleted { pc: self.read_pc() })
        } else {
            match self.target.step() {
                Ok(()) => Ok(DebugEvent::StepCompleted { pc: self.read_pc() }),
                Err(Exception::Breakpoint) => {
                    self.place_origin_on_break();
                    Ok(DebugEvent::BreakpointHit { pc: self.read_pc() })
                }
                Err(e) => Err(DebugError::TargetException(e)),
            }
        }
    }

    pub fn continue_until(&mut self, max_steps: u64) -> Result<DebugEvent, DebugError> {
        let mut rest = max_steps;
        if self.on_breakpoint() {
            self.step_over_breakpoint()?;
            rest -= 1;
        }

        loop {
            if rest == 0 {
                return Ok(DebugEvent::StepCompleted { pc: self.read_pc() });
            }
            match self.target.step() {
                Ok(()) => {
                    rest -= 1;
                }
                Err(Exception::Breakpoint) => {
                    self.place_origin_on_break();
                    return Ok(DebugEvent::BreakpointHit { pc: self.read_pc() });
                }
                Err(e) => {
                    return Err(DebugError::TargetException(e));
                }
            }
        }
    }

    pub fn continue_run(&mut self) -> Result<DebugEvent, DebugError> {
        self.continue_until(u64::MAX)
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

    pub fn read_mem<V: UnsignedInteger>(&mut self, addr: WordType) -> Result<V, MemError> {
        self.target.read_mem::<V>(addr)
    }

    pub fn read_origin_instr(&mut self, addr: WordType) -> Result<u32, MemError> {
        if let Some(instr) = self.breakpoints.get(&Breakpoint::new(addr)) {
            Ok(*instr)
        } else {
            self.read_mem(addr)
        }
    }

    pub fn write_mem<V: UnsignedInteger>(
        &mut self,
        addr: WordType,
        data: V,
    ) -> Result<(), MemError> {
        self.target.write_mem::<V>(addr, data)
    }

    pub fn decoded_info(&mut self, addr: WordType) -> Option<T::DecodeInstr> {
        let instr = self.read_origin_instr(addr).ok()?;
        self.target.decoded_info(instr)
    }
}

#[cfg(test)]
mod test {
    use crate::{isa::riscv::cpu_tester::TestCPUBuilder, ram_config::BASE_ADDR};

    use super::*;

    #[test]
    fn test_breakpoint() {
        // Test that a breakpoint can be hit
        let cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = Debugger::new(cpu);
        debugger.set_breakpoint(BASE_ADDR + 4);
        debugger.continue_run().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert!(debugger.on_breakpoint());
        assert_eq!(debugger.read_mem::<u32>(BASE_ADDR + 4).unwrap(), 0x02520333);
    }

    #[test]
    fn test_breakpoint_on_current() {
        let cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = Debugger::new(cpu);
        debugger.set_breakpoint(BASE_ADDR);
        assert!(debugger.on_breakpoint());

        debugger.step().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert!(debugger.on_breakpoint() == false);
        assert_eq!(debugger.read_mem::<u32>(BASE_ADDR).unwrap(), 0x0010_0073); // ebreak
    }
}
