use clap::{Parser, Subcommand};

use crossterm::style::Stylize;
use lazy_static::lazy_static;
use riscv_emulator::{
    board::Board,
    cli_coordinator::CliCoordinator,
    config::arch_config::{FLOAT_REG_NAME, REG_NAME, WordType},
    isa::{
        ISATypes, InstrLen,
        riscv::{
            RiscvTypes,
            csr_reg::csr_macro::CSR_NAME,
            debugger::{DebugEvent, Debugger},
            decoder::DecodeInstr,
            instruction::RVInstrInfo,
        },
    },
};
use rustyline::error::ReadlineError;

// TODO: This file contains too much things. Consider move something out of here.

#[derive(Debug, Parser)]
#[command(multicall = true)]
enum Cli {
    /// Print items such as registers, the PC, or memory.
    #[command(alias = "p", subcommand)]
    Print(PrintCmd),

    /// Display a given item each time the program stops.
    #[command(alias = "d", subcommand)]
    Display(PrintCmd),

    /// Cancel a display request.
    #[command(subcommand)]
    Undisplay(PrintCmd),

    /// List assembly around the current position.
    #[command(alias = "l")]
    List,

    /// Show historical PC values.
    #[command(alias = "his")]
    History {
        #[arg(default_value_t = 20)]
        count: usize,
    },

    /// Step a single instruction.
    #[command(alias = "s")]
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
        /// Address to set/delete the breakpoint; decimal by default, or hex if prefixed with `0x`.
        addr: String,
    },

    /// Show information such as breakpoints.
    #[command(subcommand)]
    Info(InfoCmd),

    /// Quit the debugger
    #[command(name = "quit", aliases = ["q", "exit"]) ]
    Quit,
}

#[derive(Debug, Subcommand)]
enum PrintCmd {
    /// Program counter; decimal by default, or hex if prefixed with `0x`.
    Pc,
    Reg {
        reg: String,
    },
    Mem {
        addr: String,
        #[arg(short, long, default_value_t = 16)]
        len: u32,
    },
    Csr {
        addr: String,
    },
    FReg {
        reg: String,
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
    CSR(WordType),
    FReg(u8),
}

pub struct DebugREPL<'a, I: ISATypes> {
    dbg: Debugger<'a, I>,
    watch_list: Vec<PrintObject>,
    editor: rustyline::DefaultEditor,
}

impl<'a, I: ISATypes + AsmFormattable<I>> DebugREPL<'a, I> {
    pub fn new<B: Board<ISA = I>>(board: &'a mut B) -> Self {
        CliCoordinator::global().pause_uart();

        DebugREPL {
            dbg: Debugger::<I>::new(board.cpu_mut()),
            watch_list: Vec::new(),
            editor: rustyline::DefaultEditor::new().expect("Failed to create line editor of rvdb."),
        }
    }

    pub fn run(&mut self) {
        let mut last_line = String::new();
        loop {
            match self.editor.readline(PROMPT) {
                Ok(line) => {
                    let mut line = line.trim();

                    if line.is_empty() {
                        if last_line.is_empty() {
                            continue;
                        } else {
                            line = last_line.as_str();
                        }
                    } else {
                        last_line = line.to_string();
                        self.editor.add_history_entry(line).ok();
                    }

                    match self.respond(line) {
                        Ok(quit) => {
                            if quit {
                                break;
                            }
                        }
                        Err(err) => {
                            eprintln!("Error occurred: {}", err);
                        }
                    }
                }

                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    break;
                }

                Err(ex) => {
                    eprintln!("Error occurred while reading line: {}", ex);
                }
            }
        }
    }

    fn handle_continue(&mut self, steps: u64) -> Result<(), String> {
        CliCoordinator::global().resume_uart();
        let rst = self.dbg.continue_until_step(steps);
        CliCoordinator::global().pause_uart();

        match rst {
            Ok(DebugEvent::StepCompleted { pc }) => {
                println!(
                    "stepped, pc = {}: {}",
                    format_addr(pc),
                    self.asm_formatted_at(pc)
                );
            }
            Ok(DebugEvent::BreakpointHit { pc }) => {
                println!(
                    "breakpoint hit at pc = {}: {}",
                    format_addr(pc),
                    self.asm_formatted_at(pc)
                );
            }
            Err(e) => return Err(format!("step failed: {}", e)),
        }

        // Showing `display` items.
        for idx in 0..self.watch_list.len() {
            let item = self.watch_list[idx]; // bypass borrow check by index and copy
            match item {
                PrintObject::Pc => self.print_pc(),
                PrintObject::Reg(idx) => self.print_reg(idx)?,
                PrintObject::Mem(addr, len) => self.print_mem(addr, len),
                PrintObject::CSR(addr) => self.print_csr(addr),
                PrintObject::FReg(idx) => self.print_float_reg(idx)?,
            }
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
                // TODO: unify handling of reg, csr, and freg
                self.print_reg(parse_common_reg(&reg)?)?;
            }
            Cli::Print(PrintCmd::Mem { addr, len }) => {
                self.print_mem(parse_u64(&addr)?, len);
            }
            Cli::Print(PrintCmd::Csr { addr }) => {
                self.print_csr(parse_csr(&addr)?);
            }
            Cli::Print(PrintCmd::FReg { reg }) => {
                self.print_float_reg(parse_float_reg(&reg)?)?;
            }

            Cli::Display(PrintCmd::Pc) => {
                self.watch_list.push(PrintObject::Pc);
            }
            Cli::Display(PrintCmd::Reg { reg }) => {
                self.watch_list
                    .push(PrintObject::Reg(parse_common_reg(&reg)?));
            }
            Cli::Display(PrintCmd::Mem { addr, len }) => {
                self.watch_list
                    .push(PrintObject::Mem(parse_word(&addr)?, len));
            }
            Cli::Display(PrintCmd::Csr { addr }) => {
                self.watch_list.push(PrintObject::CSR(parse_csr(&addr)?));
            }
            Cli::Display(PrintCmd::FReg { reg }) => {
                self.watch_list
                    .push(PrintObject::FReg(parse_float_reg(&reg)?));
            }

            Cli::Undisplay(PrintCmd::Pc) => {
                self.watch_list.retain(|&item| item != PrintObject::Pc);
            }
            Cli::Undisplay(PrintCmd::Reg { reg }) => {
                let reg_idx = parse_common_reg(&reg)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Reg(reg_idx));
            }
            Cli::Undisplay(PrintCmd::Mem { addr, len }) => {
                let addr = parse_word(&addr)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Mem(addr, len));
            }
            Cli::Undisplay(PrintCmd::Csr { addr }) => {
                let csr_addr = parse_csr(&addr)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::CSR(csr_addr));
            }
            Cli::Undisplay(PrintCmd::FReg { reg }) => {
                let reg_idx = parse_float_reg(&reg)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::FReg(reg_idx));
            }

            Cli::History { count } => {
                let history = self.dbg.pc_history().collect::<Vec<WordType>>();
                let skip_len = history.len().saturating_sub(count);
                for (idx, pc) in history.into_iter().skip(skip_len).enumerate() {
                    println!(
                        "{}: pc = {}, {}",
                        format_idx(idx),
                        format_addr(pc),
                        self.asm_formatted_at(pc)
                    );
                }
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

                    let raw = self.dbg.read_origin_instr(curr_addr).ok();
                    let (raw_formatted, asm) = self.raw_and_asm_formatted(raw);

                    println!("{}: {} {}", format_addr(curr_addr), raw_formatted, asm);

                    match raw {
                        Some(raw) => {
                            curr_addr += raw.len();
                        }
                        None => {
                            curr_addr += 4; // TODO: How long should I go if I failed to read raw instruction? 
                        }
                    }
                }
            }

            Cli::Info(InfoCmd::Breakpoints) => {
                println!("Breakpoints:");
                // TODO: Unnecessary copy to by pass borrow check.
                let breakpoints: Vec<_> = self.dbg.breakpoints().keys().copied().collect();
                for (idx, bp) in breakpoints.into_iter().enumerate() {
                    println!(
                        "{}: {}, {}",
                        format_idx(idx),
                        format_addr(bp.pc),
                        self.asm_formatted_at(bp.pc),
                    );
                }
            }

            Cli::Si => {
                self.handle_continue(1)?;
            }

            Cli::Continue { steps } => {
                self.handle_continue(steps)?;
            }

            Cli::Breakpoint { delete, addr } => {
                let pc = parse_word(&addr)?;
                if delete {
                    self.dbg.clear_breakpoint(pc).map_err(|e| e.to_string())?;
                    println!(
                        "cleared breakpoint at {}: {}",
                        format_addr(pc),
                        self.asm_formatted_at(pc)
                    );
                } else {
                    self.dbg.set_breakpoint(pc).map_err(|e| e.to_string())?;
                    println!(
                        "set breakpoint at {}: {}",
                        format_addr(pc),
                        self.asm_formatted_at(pc)
                    );
                }
            }
            Cli::Quit => return Ok(true),
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

    fn print_float_reg(&self, idx: u8) -> Result<(), String> {
        let val = self.dbg.read_float_reg(idx);
        println!("{} = {:.4}", palette.reg(FLOAT_REG_NAME[idx as usize]), val);
        Ok(())
    }

    fn print_mem(&mut self, addr: WordType, len: u32) {
        const BYTE_PER_LINE: u32 = 16;

        let mut curr_addr = addr;
        let mut i = 0 as u32;
        while i < len {
            if i % BYTE_PER_LINE == 0 {
                if i != 0 {
                    println!();
                }
                print!("{}: ", format_addr(curr_addr));
            }
            print!("{} ", self.read_mem_byte_formatted(curr_addr));
            curr_addr += 1;
            i += 1;
        }

        if len > 0 {
            println!();
        }
    }

    fn print_csr(&mut self, csr_addr: WordType) {
        if let Some(v) = self.dbg.read_csr(csr_addr) {
            println!("{}", format_data(v))
        } else {
            println!("Illegal CSR.")
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

    fn asm_formatted_at(&mut self, addr: WordType) -> impl std::fmt::Display {
        let raw = self.dbg.read_origin_instr(addr).ok();
        self.raw_and_asm_formatted(raw).1
    }

    fn decode_info_option(&mut self, raw: Option<I::RawInstr>) -> Option<I::DecodeRst> {
        if let Some(raw) = raw {
            self.dbg.decoded_info(raw)
        } else {
            None
        }
    }

    fn raw_and_asm_formatted(
        &mut self,
        raw: Option<I::RawInstr>,
    ) -> (impl std::fmt::Display, impl std::fmt::Display) {
        (
            <I as AsmFormattable<I>>::format_raw(raw),
            <I as AsmFormattable<I>>::format_asm(self.decode_info_option(raw)),
        )
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

    fn invalid(&self, value: &str) -> impl std::fmt::Display {
        value.red()
    }
}

// helpers

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

fn parse_reg(s: &str, reg_list: &[&str], prefix: char) -> Result<u8, String> {
    let t = s.trim();
    if let Some(index) = reg_list.iter().position(|s| s.split("/").any(|r| r == t)) {
        return Ok(index as u8);
    }

    if let Some(rest) = t.strip_prefix(prefix) {
        if let Ok(n) = rest.parse::<u8>() {
            if n < 32 {
                return Ok(n);
            }
        }
    }

    Err(format!("invalid register: {}", s))
}

fn parse_common_reg(s: &str) -> Result<u8, String> {
    parse_reg(s, &REG_NAME, 'x')
}

fn parse_float_reg(s: &str) -> Result<u8, String> {
    parse_reg(s, &FLOAT_REG_NAME, 'f')
}

fn parse_csr(s: &str) -> Result<WordType, String> {
    let t = s.trim();
    if let Some(index) = CSR_NAME.get(t) {
        return Ok(*index);
    }

    if let Ok(n) = parse_word(s) {
        return Ok(n);
    }

    Err(format!("invaild csr: {}", s))
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
    fn format_raw(raw: Option<I::RawInstr>) -> impl std::fmt::Display;
    fn format_asm(decode_instr: Option<I::DecodeRst>) -> impl std::fmt::Display;
}

impl AsmFormattable<RiscvTypes> for RiscvTypes {
    fn format_raw(raw: Option<u32>) -> impl std::fmt::Display {
        match raw {
            Some(raw) => palette.data(&format!("0x{:08x}", raw)).to_string(),
            None => palette.invalid("<invalid>").to_string(),
        }
    }

    fn format_asm(decode_instr: Option<DecodeInstr>) -> impl std::fmt::Display {
        if decode_instr.is_none() {
            return format!("{}", palette.invalid("<invalid instruction>"));
        }
        let DecodeInstr(instr, info) = unsafe { decode_instr.unwrap_unchecked() };
        match info {
            // TODO: Cannot tell float register and common register.
            // Implement a disassembler for this.
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

            RVInstrInfo::R_rm { rs1, rs2, rd, rm } => {
                format!(
                    "{} {},{},{} rm={} - type R",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize]),
                    palette.reg(REG_NAME[rs1 as usize]),
                    palette.reg(REG_NAME[rs2 as usize]),
                    rm,
                )
            }

            RVInstrInfo::R4_rm {
                rs1,
                rs2,
                rs3,
                rd,
                rm,
            } => {
                format!(
                    "{} {},{},{},{} rm={} - type R4",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize]),
                    palette.reg(REG_NAME[rs1 as usize]),
                    palette.reg(REG_NAME[rs2 as usize]),
                    palette.reg(REG_NAME[rs3 as usize]),
                    rm
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    #[cfg(feature = "riscv64")]
    fn test_parse_reg_riscv64() {
        assert_eq!(parse_common_reg("x0"), Ok(0));
        assert_eq!(parse_common_reg("a5"), Ok(15));
        assert_eq!(parse_common_reg("x31"), Ok(31));
        assert!(matches!(parse_common_reg("x32"), Err(_)));

        assert!(REG_NAME[parse_common_reg("s0").unwrap() as usize] == "s0/fp");
        assert!(REG_NAME[parse_common_reg("fp").unwrap() as usize] == "s0/fp");
    }
}
