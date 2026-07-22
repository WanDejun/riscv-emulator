use crate::{
    device::plic::types::PlicContextId,
    isa::riscv::executor::{BatchResult, ExecutionHook, NoopExecutionHook, RVCPU},
};

pub mod virt;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardStatus {
    Running,
    Halt,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(usize)]
pub(crate) enum VirtBoardPlicContextId {
    Cpu0MachineMode,
    Cpu0SuperviserMode,
}

impl Into<PlicContextId> for VirtBoardPlicContextId {
    fn into(self) -> PlicContextId {
        self as PlicContextId
    }
}

pub trait Board {
    /// Execute one cycle. This may be slower than batching; prefer [`Board::step_cycles`]
    /// or [`Board::step_cycles_with_hook`] when possible.
    fn step(&mut self) {
        self.step_cycles(1);
    }

    /// Execute exactly `cycles` CPU cycles unless the board halts first.
    fn step_cycles(&mut self, cycles: u64) -> u64 {
        let mut hook = NoopExecutionHook;
        self.step_cycles_with_hook(cycles, &mut hook).cycles
    }

    fn step_cycles_with_hook<H: ExecutionHook>(&mut self, cycles: u64, hook: &mut H)
    -> BatchResult;

    fn status(&self) -> BoardStatus;

    fn cpu(&self) -> &RVCPU;
    fn cpu_mut(&mut self) -> &mut RVCPU;

    fn loader(&self) -> Option<&crate::load::ELFLoader>;

    fn run(&mut self) {
        self.step_cycles(u64::MAX);
    }

    fn pause_background_work(&mut self);
    fn resume_background_work(&mut self);

    fn take_uart_output(&mut self) -> Vec<u8>;
}
