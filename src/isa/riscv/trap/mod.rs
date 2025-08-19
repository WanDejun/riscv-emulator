use core::panic;

use crate::{
    config::arch_config::{WordType, XLEN},
    device::MemError,
    isa::HasBreakpointException,
};
pub mod trap_controller;

/// Trap Cause
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Trap {
    Interrupt(Interrupt),
    Exception(Exception),
}

/// Interrupt
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Interrupt {
    UserSoft,
    SupervisorSoft,
    MachineSoft,
    UserTimer,
    SupervisorTimer,
    MachineTimer,
    UserExternal,
    SupervisorExternal,
    MachineExternal,
    Unknown,
}

/// Exception
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Exception {
    InstructionMisaligned,
    InstructionFault,
    IllegalInstruction,
    Breakpoint,
    LoadMisaligned,
    LoadFault,
    StoreMisaligned,
    StoreFault,
    UserEnvCall,
    SupervisorEnvCall,
    MachineEnvCall,
    InstructionPageFault,
    LoadPageFault,
    StorePageFault,
    Unknown,
}

impl Interrupt {
    pub fn from(nr: usize) -> Self {
        match nr {
            0 => Interrupt::UserSoft,
            1 => Interrupt::SupervisorSoft,
            3 => Interrupt::MachineSoft,
            4 => Interrupt::UserTimer,
            5 => Interrupt::SupervisorTimer,
            7 => Interrupt::MachineTimer,
            8 => Interrupt::UserExternal,
            9 => Interrupt::SupervisorExternal,
            11 => Interrupt::MachineExternal,
            _ => Interrupt::Unknown,
        }
    }
}

impl Into<WordType> for Interrupt {
    fn into(self) -> WordType {
        match self {
            Interrupt::UserSoft => 0,
            Interrupt::SupervisorSoft => 1,
            Interrupt::MachineSoft => 3,
            Interrupt::UserTimer => 4,
            Interrupt::SupervisorTimer => 5,
            Interrupt::MachineTimer => 7,
            Interrupt::UserExternal => 8,
            Interrupt::SupervisorExternal => 9,
            Interrupt::MachineExternal => 11,
            _ => {
                panic!("Unknown Exception.")
            }
        }
    }
}

impl Exception {
    pub fn from(nr: usize) -> Self {
        match nr {
            0 => Exception::InstructionMisaligned,
            1 => Exception::InstructionFault,
            2 => Exception::IllegalInstruction,
            3 => Exception::Breakpoint,
            4 => Exception::LoadMisaligned,
            5 => Exception::LoadFault,
            6 => Exception::StoreMisaligned,
            7 => Exception::StoreFault,
            8 => Exception::UserEnvCall,
            9 => Exception::SupervisorEnvCall,
            11 => Exception::MachineEnvCall,
            12 => Exception::InstructionPageFault,
            13 => Exception::LoadPageFault,
            15 => Exception::StorePageFault,
            _ => Exception::Unknown,
        }
    }

    pub fn from_memory_err(err: MemError) -> Self {
        match err {
            MemError::LoadMisaligned => Exception::LoadMisaligned,
            MemError::LoadFault => Exception::LoadFault,
            MemError::StoreMisaligned => Exception::StoreMisaligned,
            MemError::StoreFault => Exception::StoreFault,
        }
    }

    pub fn from_instr_fetch_err(err: MemError) -> Self {
        match err {
            MemError::LoadMisaligned => Exception::InstructionMisaligned,
            MemError::LoadFault => Exception::InstructionFault,
            _ => {
                unreachable!()
            }
        }
    }
}

impl Into<WordType> for Exception {
    fn into(self) -> WordType {
        match self {
            Exception::InstructionMisaligned => 0,
            Exception::InstructionFault => 1,
            Exception::IllegalInstruction => 2,
            Exception::Breakpoint => 3,
            Exception::LoadMisaligned => 4,
            Exception::LoadFault => 5,
            Exception::StoreMisaligned => 6,
            Exception::StoreFault => 7,
            Exception::UserEnvCall => 8,
            Exception::SupervisorEnvCall => 9,
            Exception::MachineEnvCall => 11,
            Exception::InstructionPageFault => 12,
            Exception::LoadPageFault => 13,
            Exception::StorePageFault => 15,
            Exception::Unknown => {
                panic!("Unknown Exception.")
            }
        }
    }
}

impl Into<WordType> for Trap {
    fn into(self) -> WordType {
        match self {
            Self::Interrupt(nr) => {
                let nr: WordType = nr.into();
                nr | (1u64 << (XLEN - 1))
            }
            Self::Exception(nr) => nr.into(),
        }
    }
}

impl HasBreakpointException for Exception {
    fn is_breakpoint(&self) -> bool {
        matches!(self, Exception::Breakpoint)
    }
}
