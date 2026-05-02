use gdbstub::target;
use gdbstub::target::ext;

use crate::board::Board;

use crate::isa::riscv::debugger::{DebugEvent, Debugger};

mod basic;
mod breakpoints;
mod desc;
mod eventloop;

pub use eventloop::*;

enum ExecMode {
    Continue,
    Step,
}

enum RunEvent {
    IncomingData,
    StopReason(DebugEvent),
}

pub struct GdbDebugger<'a, B: Board> {
    dbg: Debugger<'a, B>,
    exec_mode: ExecMode,
}

impl<'a, B: Board> GdbDebugger<'a, B> {
    pub fn new(board: &'a mut B) -> Self {
        Self {
            dbg: Debugger::new(board),
            exec_mode: ExecMode::Continue,
        }
    }

    fn run_by_mode_until<F: FnMut(&mut Debugger<'a, B>) -> bool>(
        &mut self,
        condition: F,
    ) -> RunEvent {
        match self.exec_mode {
            ExecMode::Step => RunEvent::StopReason(self.dbg.step().unwrap()),
            ExecMode::Continue => match self.dbg.continue_until(condition).unwrap() {
                Some(event) => RunEvent::StopReason(event),
                None => RunEvent::IncomingData,
            },
        }
    }
}
