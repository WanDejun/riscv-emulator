use std::io::Write;

use clap::{Parser, Subcommand};

use crossterm::style::Stylize;
use riscv_emulator::{
    cli_coordinator::CliCoordinator,
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

    /// Display given item each time the program stops.
    #[command(alias = "d", subcommand)]
    Display(PrintCmd),

    /// Cancel display request.
    #[command(subcommand)]
    Undisplay(PrintCmd),

    #[command(alias = "l")]
    List,

    /// Step instruction
    #[command(alias = "s")]
    Si,

    /// Continue running
    #[command(name = "continue", aliases = ["c"])]
    Continue,

    /// Set or delete breakpoint
    #[command(name = "break", alias = "b")]
    Breakpoint {
        #[arg(short = 'd', long = "delete")]
        delete: bool,
        addr: String,
    },

    /// show debug infos such as breakpoints.
    #[command(subcommand)]
    Info(InfoCmd),

    /// Quit debugger
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

#[derive(Debug, Subcommand)]
enum InfoCmd {
    #[command(aliases = ["b", "bp", "break"])]
    Breakpoints,
}

const PROMPT: &str = "(rvdb) ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintObject {
    Pc,
    Reg(u8),
    Mem(WordType, u32),
}

pub struct DebugREPL {
    dbg: Debugger<RV32CPU>,
    cpu: RV32CPU,
    watch_list: Vec<PrintObject>,
}

impl DebugREPL {
    pub fn new(cpu: RV32CPU) -> Self {
        CliCoordinator::global().pause_uart();

        DebugREPL {
            dbg: Debugger::<RV32CPU>::new(),
            cpu,
            watch_list: Vec::new(),
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
        CliCoordinator::global().resume_uart();
        let rst = self.dbg.continue_until(&mut self.cpu, steps);
        CliCoordinator::global().pause_uart();

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

        match cli {
            Cli::Print(PrintCmd::Pc) => {
                self.print_pc();
            }
            Cli::Print(PrintCmd::Reg { reg }) => {
                self.print_reg(parse_reg(&reg)?)?;
            }
            Cli::Print(PrintCmd::Mem { addr, len }) => {
                self.print_mem(parse_u64(&addr)?, len);
            }

            Cli::Display(PrintCmd::Pc) => {
                self.watch_list.push(PrintObject::Pc);
            }
            Cli::Display(PrintCmd::Reg { reg }) => {
                self.watch_list.push(PrintObject::Reg(parse_reg(&reg)?));
            }
            Cli::Display(PrintCmd::Mem { addr, len }) => {
                self.watch_list
                    .push(PrintObject::Mem(parse_word(&addr)?, len));
            }

            Cli::Undisplay(PrintCmd::Pc) => {
                self.watch_list.retain(|&item| item != PrintObject::Pc);
            }
            Cli::Undisplay(PrintCmd::Reg { reg }) => {
                let reg_idx = parse_reg(&reg)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Reg(reg_idx));
            }
            Cli::Undisplay(PrintCmd::Mem { addr, len }) => {
                let addr = parse_word(&addr)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Mem(addr, len));
            }

            Cli::List => {
                const LIST_INSTR: WordType = 10;
                let base_addr = self.dbg.read_pc(&mut self.cpu) - LIST_INSTR * 4 / 2;

                for i in 0..LIST_INSTR {
                    let curr_addr = base_addr + (i * 4);
                    let is_curr_line = curr_addr == self.dbg.read_pc(&mut self.cpu);

                    if is_curr_line {
                        print!("{} ", ">".cyan());
                    } else {
                        print!("  ");
                    }

                    let instr = self.dbg.decoded_info(&mut self.cpu, curr_addr);
                    println!(
                        "0x{:08x}: {} {}",
                        curr_addr,
                        self.mem_word_formatted(curr_addr),
                        instr
                    );
                }
            }

            Cli::Info(InfoCmd::Breakpoints) => {
                println!("Breakpoints:");
                for (idx, bp) in self.dbg.breakpoints().keys().enumerate() {
                    println!("{}: {}", idx, fmt_word(bp.pc));
                }
            }

            Cli::Si => {
                self.handle_continue(1)?;
            }

            Cli::Continue => {
                self.handle_continue(u64::MAX)?;
            }

            Cli::Breakpoint { delete, addr } => {
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

        for idx in 0..self.watch_list.len() {
            let item = self.watch_list[idx]; // bypass borrow check by index and copy
            match item {
                PrintObject::Pc => self.print_pc(),
                PrintObject::Reg(idx) => self.print_reg(idx)?,
                PrintObject::Mem(addr, len) => self.print_mem(addr, len),
            }
        }

        Ok(false)
    }

    fn print_pc(&self) {
        let pc = self.dbg.read_pc(&self.cpu);
        println!("pc = {}", fmt_word(pc));
    }

    fn print_reg(&self, idx: u8) -> Result<(), String> {
        let val = self.dbg.read_reg(&self.cpu, idx);
        println!("{} = {}", REG_NAME[idx as usize], fmt_word(val));
        Ok(())
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
            print!("{} ", self.mem_byte_formatted(curr_addr));
            i += 1;
        }

        if len > 0 {
            println!();
        }
    }

    fn mem_byte_formatted(&mut self, addr: WordType) -> impl std::fmt::Display {
        self.dbg
            .read_mem::<u8>(&mut self.cpu, addr)
            .map(|v| format!("{:02x}", v))
            .unwrap_or("**".into())
    }

    fn mem_addr_formatted(addr: WordType) -> impl std::fmt::Display {
        format!("0x{:08x}", addr).blue()
    }

    fn mem_word_formatted(&mut self, addr: WordType) -> impl std::fmt::Display {
        self.dbg
            .read_mem::<u32>(&mut self.cpu, addr)
            .map(|v| format!("0x{:08x}", v))
            .unwrap_or("<invalid>".into())
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

fn parse_reg(s: &str) -> Result<u8, String> {
    let t = s.trim();
    if let Some(index) = REG_NAME.iter().position(|&r| r == t) {
        return Ok(index as u8);
    }

    if let Some(rest) = t.strip_prefix('x') {
        if let Ok(n) = rest.parse::<u8>() {
            if n < 32 {
                return Ok(n);
            }
        }
    }

    Err(format!("invalid register: {}", s))
}
