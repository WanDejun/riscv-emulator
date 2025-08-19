use std::io::Write;

use clap::{Parser, Subcommand};

use crossterm::style::Stylize;
use lazy_static::lazy_static;
use riscv_emulator::{
    cli_coordinator::CliCoordinator,
    config::arch_config::{REG_NAME, WordType},
    isa::{
        ISATypes, InstrLen,
        riscv::{
            RiscvTypes,
            debugger::{DebugEvent, Debugger},
            decoder::DecodeInstr,
            instruction::RVInstrInfo,
        },
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

pub struct DebugREPL<I: ISATypes> {
    dbg: Debugger<I>,
    watch_list: Vec<PrintObject>,
}

impl<I: ISATypes + AsmFormattable<I>> DebugREPL<I> {
    pub fn new(cpu: I::CPU) -> Self {
        CliCoordinator::global().pause_uart();

        DebugREPL {
            dbg: Debugger::<I>::new(cpu),
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
        let rst = self.dbg.continue_until(steps);
        CliCoordinator::global().pause_uart();

        match rst {
            Ok(DebugEvent::StepCompleted { pc }) => {
                println!("stepped, pc = {}", format_addr(pc));
            }
            Ok(DebugEvent::BreakpointHit { pc }) => {
                println!("breakpoint hit at pc = {}", format_addr(pc));
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

                // TODO: This may not a valid instruction start for variable-length ISA.
                let mut curr_addr = (self.dbg.read_pc() - LIST_INSTR * 2) as WordType;

                for _ in 0..LIST_INSTR {
                    let is_curr_line = curr_addr == self.dbg.read_pc();

                    if is_curr_line {
                        print!("{} ", palette.arrow(">"));
                    } else {
                        print!("  ");
                    }

                    let raw = self.dbg.read_origin_instr(curr_addr).unwrap();
                    let raw_formatted = <I as AsmFormattable<I>>::format_raw(raw);
                    let asm =
                        <I as AsmFormattable<I>>::format_asm(self.dbg.decoded_info(curr_addr));

                    println!("{}: {} {}", format_addr(curr_addr), raw_formatted, asm);

                    curr_addr += raw.len();
                }
            }

            Cli::Info(InfoCmd::Breakpoints) => {
                println!("Breakpoints:");
                for (idx, bp) in self.dbg.breakpoints().keys().enumerate() {
                    println!("{}: {}", format_idx(idx), format_addr(bp.pc));
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
                    self.dbg.clear_breakpoint(pc);
                    println!("cleared breakpoint at {}", format_addr(pc));
                } else {
                    self.dbg.set_breakpoint(pc);
                    println!("set breakpoint at {}", format_addr(pc));
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
        let pc = self.dbg.read_pc();
        println!("pc = {}", format_addr(pc));
    }

    fn print_reg(&self, idx: u8) -> Result<(), String> {
        let val = self.dbg.read_reg(idx);
        println!(
            "{} = {}",
            palette.reg(REG_NAME[idx as usize]),
            format_data(val)
        );
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
                print!("{}: ", format_addr(curr_addr));
            }
            curr_addr = curr_addr + (i as WordType);
            print!("{} ", self.read_mem_byte_formatted(curr_addr));
            i += 1;
        }

        if len > 0 {
            println!();
        }
    }

    fn read_mem_byte_formatted(&mut self, addr: WordType) -> impl std::fmt::Display {
        self.dbg
            .read_mem::<u8>(addr)
            .map(|v| format!("{:02x}", v))
            .unwrap_or("**".into())
    }

    fn read_mem_word_formatted(&mut self, addr: WordType) -> impl std::fmt::Display {
        self.dbg
            .read_mem::<u32>(addr)
            .map(|v| format!("0x{:08x}", v))
            .unwrap_or("<invalid>".into())
    }
}

lazy_static! {
    static ref palette: OutputPalette = OutputPalette {};
}

struct OutputPalette;

impl OutputPalette {
    fn index(&self, index: &str) -> impl std::fmt::Display {
        index.yellow()
    }

    fn addr(&self, addr: &str) -> impl std::fmt::Display {
        addr.blue()
    }

    fn reg(&self, reg: &str) -> impl std::fmt::Display {
        reg.magenta()
    }

    fn instr(&self, instr: &str) -> impl std::fmt::Display {
        instr.green()
    }

    fn arrow(&self, ch: &str) -> impl std::fmt::Display {
        ch.cyan()
    }

    fn data(&self, value: &str) -> impl std::fmt::Display {
        value.yellow()
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

fn format_idx(idx: usize) -> impl std::fmt::Display {
    palette.index(&idx.to_string()).to_string()
}

fn format_addr(word: WordType) -> impl std::fmt::Display {
    palette.addr(&format!("0x{:08x}", word)).to_string()
}

fn format_data(data: WordType) -> impl std::fmt::Display {
    palette.data(&format!("0x{:08x}", data)).to_string()
}

pub trait AsmFormattable<I: ISATypes> {
    fn format_raw(raw: I::RawInstr) -> impl std::fmt::Display;
    fn format_asm(decode_instr: Option<I::DecodeRst>) -> impl std::fmt::Display;
}

impl AsmFormattable<RiscvTypes> for RiscvTypes {
    fn format_raw(raw: u32) -> impl std::fmt::Display {
        palette.data(&format!("0x{:08x}", raw)).to_string()
    }

    fn format_asm(decode_instr: Option<DecodeInstr>) -> impl std::fmt::Display {
        if decode_instr.is_none() {
            return format!("{}", String::from("<invalid instruction>").red());
        } else {
            let DecodeInstr(instr, info) = decode_instr.unwrap();
            match info {
                RVInstrInfo::I { rd, rs1, imm } => {
                    format!(
                        "{} {},{},{} - type I",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize]),
                        palette.reg(REG_NAME[rs1 as usize]),
                        palette.data(imm.to_string().as_str()),
                    )
                }

                RVInstrInfo::R { rs1, rs2, rd } => {
                    format!(
                        "{} {},{},{} - type R",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize]),
                        palette.reg(REG_NAME[rs1 as usize]),
                        palette.reg(REG_NAME[rs2 as usize])
                    )
                }

                RVInstrInfo::B { rs1, rs2, imm } => {
                    format!(
                        "{} {},{},{} - type B",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rs1 as usize]),
                        palette.reg(REG_NAME[rs2 as usize]),
                        palette.data((imm >> 1).to_string().as_str())
                    )
                }

                RVInstrInfo::J { rd, imm } => {
                    format!(
                        "{} {},{} - type J",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize]),
                        palette.data((imm >> 12).to_string().as_str())
                    )
                }

                RVInstrInfo::S { rs1, rs2, imm } => {
                    format!(
                        "{} {},{},{} - type S",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rs1 as usize]),
                        palette.reg(REG_NAME[rs2 as usize]),
                        palette.data((imm).to_string().as_str())
                    )
                }
                RVInstrInfo::U { rd, imm } => {
                    format!(
                        "{} {},{} - type U",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize]),
                        palette.data((imm >> 12).to_string().as_str())
                    )
                }

                RVInstrInfo::None => {
                    format!("{}", palette.instr(instr.name()))
                }
            }
        }
    }
}
