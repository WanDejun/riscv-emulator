use std::io::Write;

use clap::{Parser, Subcommand};

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use riscv_emulator::{
    config::arch_config::{REG_NAME, WordType},
    isa::riscv::{
        debugger::{DebugEvent, Debugger},
        executor::RV32CPU,
    },
};

#[derive(Debug, Parser)]
#[command(multicall = true)]
enum Cli {
    /// Print registers, PC, or memory
    #[command(alias = "p", subcommand)]
    Print(PrintCmd),

    /// Step instruction
    #[command(alias = "s")]
    Si,

    /// Continue running
    #[command(name = "continue", aliases = ["c"])]
    Continue,

    /// Set or delete breakpoint
    #[command(name = "break", alias = "b")]
    Break {
        #[arg(short = 'd', long = "delete")]
        delete: bool,
        addr: String,
    },

    #[command(name = "quit", aliases = ["q", "exit"]) ]
    Quit,
}

#[derive(Debug, Subcommand)]
enum PrintCmd {
    Pc,
    Reg {
        reg: String,
    },
    Mem {
        addr: String,
        #[arg(short, long, default_value_t = 4)]
        len: u32,
    },
}

const PROMPT: &str = "(rvdb) ";

pub struct DebugREPL {
    dbg: Debugger<RV32CPU>,
    cpu: RV32CPU,
}

impl DebugREPL {
    pub fn new(cpu: RV32CPU) -> Self {
        disable_raw_mode().expect("Failed to disable raw mode");
        DebugREPL {
            dbg: Debugger::<RV32CPU>::new(),
            cpu,
        }
    }

    pub fn run(&mut self) {
        loop {
            if let Ok(line) = readline() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                match self.respond(line) {
                    Ok(quit) => {
                        if quit {
                            break;
                        }
                    }
                    Err(err) => {
                        eprintln!("Error occurred while processing command: {}", err);
                    }
                }
            } else {
                eprintln!("Error reading line");
            }
        }
    }

    fn handle_continue(&mut self, steps: u64) -> Result<(), String> {
        enable_raw_mode().map_err(|e| e.to_string())?;
        let rst = self.dbg.continue_until(&mut self.cpu, steps);
        disable_raw_mode().map_err(|e| e.to_string())?;

        match rst {
            Ok(DebugEvent::StepCompleted { pc }) => {
                println!("stepped, pc = {}", fmt_word(pc));
            }
            Ok(DebugEvent::BreakpointHit { pc }) => {
                println!("breakpoint hit at pc = {}", fmt_word(pc));
            }
            Err(e) => return Err(format!("step failed: {}", e)),
        }

        Ok(())
    }

    fn respond(&mut self, line: &str) -> Result<bool, String> {
        let argv = line.split_whitespace().map(|s| s.to_string());
        let cli = Cli::try_parse_from(argv).map_err(|e| e.to_string())?;

        disable_raw_mode().map_err(|e| e.to_string())?;

        match cli {
            Cli::Print(PrintCmd::Pc) => {
                let pc = self.dbg.read_pc(&self.cpu);
                println!("pc = {}", fmt_word(pc));
            }
            Cli::Print(PrintCmd::Reg { reg }) => {
                let idx = parse_reg(&reg).ok_or_else(|| format!("invalid register: {}", reg))?;
                let val = self.dbg.read_reg(&self.cpu, idx);
                println!("x{} = {}", idx, fmt_word(val));
            }
            Cli::Print(PrintCmd::Mem { addr, len }) => {
                let addr = parse_u64(&addr)?;
                self.print_mem(addr, len);
            }

            Cli::Si => {
                self.handle_continue(1)?;
            }

            Cli::Continue => {
                self.handle_continue(u64::MAX)?;
            }

            Cli::Break { delete, addr } => {
                let pc = parse_word(&addr)?;
                if delete {
                    self.dbg.clear_breakpoint(&mut self.cpu, pc);
                    println!("cleared breakpoint at {}", fmt_word(pc));
                } else {
                    self.dbg.set_breakpoint(&mut self.cpu, pc);
                    println!("set breakpoint at {}", fmt_word(pc));
                }
            }
            Cli::Quit => return Ok(true),
        }
        Ok(false)
    }

    fn print_mem(&mut self, addr: WordType, len: u32) {
        const BYTE_PER_LINE: u32 = 8;

        let mut curr_addr = addr;
        let mut i = 0 as u32;
        while i < len {
            if i % BYTE_PER_LINE == 0 {
                if i != 0 {
                    println!();
                }
                print!("{}: ", fmt_word(curr_addr));
            }
            curr_addr = curr_addr + (i as WordType);
            print!("{:02x} ", self.dbg.read_mem::<u8>(&mut self.cpu, curr_addr));
            i += 1;
        }

        if len > 0 {
            println!();
        }
    }
}

// helpers

fn readline() -> Result<String, String> {
    print!("{}", PROMPT);
    std::io::stdout().flush().map_err(|e| e.to_string())?;

    let mut buffer = String::new();
    std::io::stdin()
        .read_line(&mut buffer)
        .map_err(|e| e.to_string())?;

    Ok(buffer)
}

fn parse_u64(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16).map_err(|e| e.to_string())
    } else {
        s.parse::<u64>().map_err(|e| e.to_string())
    }
}

fn parse_word(s: &str) -> Result<WordType, String> {
    parse_u64(s).map(|v| v as WordType)
}

fn fmt_word<T>(v: T) -> String
where
    T: Into<u128>,
{
    format!("0x{:x}", v.into())
}

fn parse_reg(s: &str) -> Option<u8> {
    let t = s.trim();
    if let Some(index) = REG_NAME.iter().position(|&r| r == t) {
        return Some(index as u8);
    }

    if let Some(rest) = t.strip_prefix('x') {
        if let Ok(n) = rest.parse::<u8>() {
            if n < 32 {
                return Some(n);
            }
        }
    }

    None
}
