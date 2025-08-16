use std::{collections::BTreeMap, fmt::Debug, marker::PhantomData, u64};

use crate::{
    config::arch_config::WordType,
    device::Mem,
    isa::riscv::{executor::RV32CPU, instruction::Exception},
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
    fn read_pc(&self) -> WordType;
    fn write_pc(&mut self, new_pc: WordType);

    fn read_reg(&self, idx: u8) -> WordType;
    fn write_reg(&mut self, idx: u8, value: WordType);

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> T;
    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T);

    fn step(&mut self) -> Result<(), Exception>;
}

impl DebugTarget for RV32CPU {
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

    fn read_mem<T: UnsignedInteger>(&mut self, addr: WordType) -> T {
        self.memory.read::<T>(addr)
    }

    fn write_mem<T: UnsignedInteger>(&mut self, addr: WordType, data: T) {
        self.memory.write::<T>(addr, data)
    }

    fn step(&mut self) -> Result<(), Exception> {
        RV32CPU::step(self)
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

// TODO: Refactor to contain a RV32CPU instead of passing &mut every time.
pub struct Debugger<T: DebugTarget> {
    breakpoints: BTreeMap<Breakpoint, u32>,
    _marker: PhantomData<T>,
}

const EBREAK: u32 = 0x0010_0073;

impl<T: DebugTarget> Debugger<T> {
    pub fn new() -> Self {
        Self {
            breakpoints: BTreeMap::new(),
            _marker: PhantomData,
        }
    }

    pub fn set_breakpoint(&mut self, target: &mut T, pc: WordType) {
        let breakpoint = Breakpoint::new(pc);
        if self.breakpoints.contains_key(&breakpoint) {
            return;
        }
        let orig: u32 = target.read_mem(pc);
        if orig != EBREAK {
            target.write_mem(pc, EBREAK);
        }

        self.breakpoints.insert(breakpoint, orig);
    }

    pub fn clear_breakpoint(&mut self, target: &mut T, pc: WordType) {
        if let Some(orig) = self.breakpoints.remove(&Breakpoint { pc }) {
            target.write_mem(pc, orig);
        }
    }

    fn on_breakpoint(&mut self, target: &mut T) -> bool {
        self.breakpoints
            .contains_key(&Breakpoint::new(target.read_pc()))
    }

    fn step_over_breakpoint(&mut self, target: &mut T) -> Result<(), DebugError> {
        let pc = target.read_pc();
        if let Some(instr) = self.breakpoints.get(&Breakpoint { pc }).copied() {
            target.write_mem::<u32>(pc, instr);
            match target.step() {
                Ok(()) => {
                    target.write_mem::<u32>(pc, EBREAK);
                    Ok(())
                }
                Err(e) => Err(DebugError::TargetException(e)),
            }
        } else {
            Ok(())
        }
    }

    pub fn step(&mut self, target: &mut T) -> Result<DebugEvent, DebugError> {
        if self.on_breakpoint(target) {
            self.step_over_breakpoint(target)?;
            Ok(DebugEvent::StepCompleted {
                pc: target.read_pc(),
            })
        } else {
            match target.step() {
                Ok(()) => Ok(DebugEvent::StepCompleted {
                    pc: target.read_pc(),
                }),
                Err(Exception::EBreak) => Ok(DebugEvent::BreakpointHit {
                    pc: target.read_pc(),
                }),
                Err(e) => Err(DebugError::TargetException(e)),
            }
        }
    }

    pub fn continue_until(
        &mut self,
        target: &mut T,
        max_steps: u64,
    ) -> Result<DebugEvent, DebugError> {
        let mut rest = max_steps;
        if self.on_breakpoint(target) {
            self.step_over_breakpoint(target)?;
            rest -= 1;
        }

        loop {
            if rest == 0 {
                return Ok(DebugEvent::StepCompleted {
                    pc: target.read_pc(),
                });
            }
            match target.step() {
                Ok(()) => {
                    rest -= 1;
                }
                Err(Exception::EBreak) => {
                    return Ok(DebugEvent::BreakpointHit {
                        pc: target.read_pc(),
                    });
                }
                Err(e) => {
                    return Err(DebugError::TargetException(e));
                }
            }
        }
    }

    pub fn continue_run(&mut self, target: &mut T) -> Result<DebugEvent, DebugError> {
        self.continue_until(target, u64::MAX)
    }

    pub fn read_reg(&self, target: &T, idx: u8) -> WordType {
        target.read_reg(idx)
    }

    pub fn write_reg(&self, target: &mut T, idx: u8, val: WordType) {
        target.write_reg(idx, val)
    }

    pub fn read_pc(&self, target: &T) -> WordType {
        target.read_pc()
    }

    pub fn write_pc(&self, target: &mut T, val: WordType) {
        target.write_pc(val)
    }

    pub fn read_mem<V: UnsignedInteger>(&self, target: &mut T, addr: WordType) -> V {
        target.read_mem::<V>(addr)
    }

    pub fn write_mem<V: UnsignedInteger>(&self, target: &mut T, addr: WordType, data: V) {
        target.write_mem::<V>(addr, data)
    }
}
