mod handler;
mod printer;
mod repl;

use clap::{Parser, Subcommand};
use riscv_emulator::config::arch_config::REGFILE_CNT;
use riscv_emulator::config::arch_config::WordType;
use riscv_emulator::isa::riscv::csr_reg::PrivilegeLevel;
use riscv_emulator::isa::riscv::debugger;
use riscv_emulator::isa::riscv::mmu::AccessType;
use riscv_emulator::isa::riscv::{debugger::Address, decoder::DecodeInstr};

pub use repl::DebugREPL;

#[derive(clap::ValueEnum, Debug, Clone)]
enum ClapAccessType {
    Read,
    Write,
}

impl ToString for ClapAccessType {
    fn to_string(&self) -> String {
        match self {
            ClapAccessType::Read => "read".to_string(),
            ClapAccessType::Write => "write".to_string(),
        }
    }
}

impl From<ClapAccessType> for AccessType {
    fn from(value: ClapAccessType) -> Self {
        match value {
            ClapAccessType::Read => AccessType::Read,
            ClapAccessType::Write => AccessType::Write,
        }
    }
}

#[derive(Debug, Parser)]
#[command(multicall = true)]
enum Cli {
    /// Print items such as registers, the PC, or memory.
    #[command(alias = "p", subcommand)]
    Print(PrintCmd),

    /// Display a given item each time the program stops.
    #[command(alias = "d", subcommand)]
    Display(PrintCmd),
    /// Translate a given address as a real instruction would.
    /// This respects the current CPU state (e.g., privilege level and CSR settings)
    /// and page table flags, based on the given access type.
    #[command(aliases = ["t", "trans"])]
    Translate {
        addr: String,
        #[arg(default_value_t = ClapAccessType::Read)]
        access: ClapAccessType,
    },

    /// Cancel a display request.
    #[command(subcommand)]
    Undisplay(PrintCmd),

    /// List assembly around the current position.
    #[command(aliases = ["l", "ls"])]
    List,

    /// Show historical PC values.
    #[command(alias = "his")]
    History {
        #[arg(default_value_t = 20)]
        count: usize,
    },

    /// Show function call trace.
    #[command(aliases = ["ft", "ftrace"])]
    FTrace {
        #[arg(default_value_t = 20)]
        count: usize,
    },

    /// Load an ELF symbol file.
    #[command(aliases = ["symbol", "file"])]
    SymbolFile { path: String },

    /// Step a single instruction.
    #[command(aliases = ["s", "step"])]
    Si,

    /// Continue running.
    #[command(name = "continue", aliases = ["c"])]
    Continue {
        #[arg(default_value_t = u64::MAX)]
        steps: u64,
    },

    /// Set or delete a breakpoint.
    #[command(name = "break", alias = "b")]
    Breakpoint {
        #[arg(short = 'd', long = "delete")]
        delete: bool,
        /// Address or function symbol name to set/delete a breakpoint.
        /// Address should be decimal by default, or hex if prefixed with `0x`.
        symbol: String,

        /// Whether the address is virtual or physical.
        #[arg(short, long, default_value_t = false)]
        virt: bool,
    },

    /// Show information such as breakpoints.
    #[command(subcommand)]
    Info(InfoCmd),

    /// Quit the debugger
    #[command(name = "quit", aliases = ["q", "exit"]) ]
    Quit,
}

#[derive(Debug, Subcommand)]
pub enum PrintCmd {
    /// Program counter
    Pc,
    /// General-purpose register
    Reg {
        /// Register name
        reg: String,
    },
    /// Some general-purpose registers
    Regs {
        /// Starting register index
        #[arg(long, default_value_t = 0)]
        start: u8,
        /// Number of registers
        #[arg(short, long, default_value_t = REGFILE_CNT as u8)]
        len: u8,
    },
    /// Memory (in virtual or physical address space)
    Mem {
        addr: String,
        #[arg(short, long, default_value_t = 16)]
        len: u32,
        /// Whether the address is virtual or physical.
        #[arg(short, long, default_value_t = false)]
        virt: bool,
    },
    /// Control and status register
    Csr { addr: String },
    /// Floating-point register
    FReg { reg: String },
    /// Privilege level
    Priv,
}

#[derive(Debug, Subcommand)]
pub enum InfoCmd {
    #[command(aliases = ["b", "bp", "break"])]
    Breakpoints,
    #[command(aliases = ["sym", "symbol"])]
    Symbols,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrintObject {
    Pc,
    Reg(u8),
    Regs(u8, u8),
    Mem(u64, u32, bool), // addr, len, is_virt
    CSR(WordType),
    FReg(u8),
    Privilege,
}

#[derive(Debug, PartialEq)]
pub struct DbgInstrLine {
    pub addr: u64,
    pub raw: Option<u32>,
    pub decoded: Option<DecodeInstr>,
    pub symbol: Option<String>,
    pub is_current_pc: bool,
}

#[derive(Debug, PartialEq)]
pub enum CommandOutput {
    None,
    Exit,

    Pc(WordType),
    Reg {
        name: String,
        val: WordType,
    },
    Regs(Vec<(&'static str, WordType)>),
    FReg {
        name: String,
        f32_val: f32,
        f64_val: f64,
    },
    Csr {
        name: String,
        val: Option<WordType>,
    },
    Mem {
        addr: Address,
        data: Vec<u8>,
    },

    Translate {
        virt_addr: WordType,
        phys_addr: u64,
    },

    Privilege(PrivilegeLevel),

    History(Vec<DbgInstrLine>),
    CodeList(Vec<DbgInstrLine>),
    Breakpoints(Vec<debugger::Breakpoint>),
    Symbols(Vec<(String, WordType)>),
    FTrace(Vec<debugger::FuncTrace>),

    ContinueDone {
        instr: DbgInstrLine,
        watch_results: Vec<CommandOutput>,
        event: debugger::DebugEvent,
        actual_steps: u64,
    },

    BreakpointSet {
        ok: bool,
        addr: Address,
        symbol: Option<String>,
    },
    BreakpointCleared {
        ok: bool,
        addr: Address,
        symbol: Option<String>,
    },
}
