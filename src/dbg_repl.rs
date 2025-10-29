use std::u8;

use clap::{Parser, Subcommand};

use crossterm::style::Stylize;
use lazy_static::lazy_static;
use riscv_emulator::{
    board::Board,
    cli_coordinator::CliCoordinator,
    config::arch_config::{FLOAT_REG_NAME, REG_NAME, REGFILE_CNT, WordType},
    isa::{
        ISATypes, InstrLen,
        riscv::{
            RiscvTypes,
            csr_reg::{
                PrivilegeLevel,
                csr_macro::{CSR_ADDRESS, CSR_NAME},
            },
            debugger::{Address, DebugEvent, Debugger},
            decoder::DecodeInstr,
            instruction::{RVInstrInfo, rv32i_table::RiscvInstr},
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
    #[command(aliases = ["l", "ls"])]
    List,

    /// Show historical PC values.
    #[command(alias = "his")]
    History {
        #[arg(default_value_t = 20)]
        count: usize,
    },

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
        /// Address to set/delete the breakpoint; decimal by default, or hex if prefixed with `0x`.
        addr: String,

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
enum PrintCmd {
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
enum InfoCmd {
    #[command(aliases = ["b", "bp", "break"])]
    Breakpoints,
}

const PROMPT: &str = "(rvdb) ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PrintObject {
    Pc,
    Reg(u8),
    Regs(u8, u8),
    Mem(u64, u32, bool),
    CSR(WordType),
    FReg(u8),
    Privilege,
}

fn make_address(addr: u64, virt: bool) -> Address {
    if virt {
        Address::Virt(addr)
    } else {
        Address::Phys(addr)
    }
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
                        self.editor.add_history_entry(line).unwrap();
                    }

                    match self.respond(line) {
                        Ok(quit) => {
                            if quit {
                                break;
                            }
                        }
                        Err(err) => {
                            eprintln!("{}", err);
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
            Ok(DebugEvent::StepCompleted) => {
                println!(
                    "{}: {}",
                    format_addr(self.dbg.read_pc()),
                    self.current_asm_formatted(),
                );
            }
            Ok(DebugEvent::BreakpointHit) => {
                println!(
                    "breakpoint hit at pc = {}: {}",
                    format_addr(self.dbg.read_pc()),
                    self.current_asm_formatted()
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
                PrintObject::Regs(start, len) => self.print_regs(start, len)?,
                PrintObject::Mem(addr, len, is_virt) => self.print_mem(addr, len, is_virt),
                PrintObject::CSR(addr) => self.print_csr(addr),
                PrintObject::FReg(idx) => self.print_float_reg(idx)?,
                PrintObject::Privilege => self.print_privilege(),
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
            Cli::Print(PrintCmd::Regs { start, len }) => {
                self.print_regs(start, len)?;
            }
            Cli::Print(PrintCmd::Mem { addr, len, virt }) => {
                self.print_mem(parse_u64(&addr)?, len, virt);
            }
            Cli::Print(PrintCmd::Csr { addr }) => {
                self.print_csr(parse_csr(&addr)?);
            }
            Cli::Print(PrintCmd::FReg { reg }) => {
                self.print_float_reg(parse_float_reg(&reg)?)?;
            }
            Cli::Print(PrintCmd::Priv) => {
                self.print_privilege();
            }

            Cli::Display(PrintCmd::Pc) => {
                self.watch_list.push(PrintObject::Pc);
            }
            Cli::Display(PrintCmd::Reg { reg }) => {
                self.watch_list
                    .push(PrintObject::Reg(parse_common_reg(&reg)?));
            }
            Cli::Display(PrintCmd::Regs { start, len }) => {
                self.watch_list.push(PrintObject::Regs(start, len));
            }
            Cli::Display(PrintCmd::Mem {
                addr,
                len,
                virt: is_virt,
            }) => {
                self.watch_list
                    .push(PrintObject::Mem(parse_word(&addr)?, len, is_virt));
            }
            Cli::Display(PrintCmd::Csr { addr }) => {
                self.watch_list.push(PrintObject::CSR(parse_csr(&addr)?));
            }
            Cli::Display(PrintCmd::FReg { reg }) => {
                self.watch_list
                    .push(PrintObject::FReg(parse_float_reg(&reg)?));
            }
            Cli::Display(PrintCmd::Priv) => {
                self.watch_list.push(PrintObject::Privilege);
            }

            Cli::Undisplay(PrintCmd::Pc) => {
                self.watch_list.retain(|&item| item != PrintObject::Pc);
            }
            Cli::Undisplay(PrintCmd::Reg { reg }) => {
                let reg_idx = parse_common_reg(&reg)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Reg(reg_idx));
            }
            Cli::Undisplay(PrintCmd::Regs { start, len }) => {
                self.watch_list
                    .retain(|&item| item != PrintObject::Regs(start, len));
            }
            Cli::Undisplay(PrintCmd::Mem {
                addr,
                len,
                virt: is_virt,
            }) => {
                let addr = parse_word(&addr)?;
                self.watch_list
                    .retain(|&item| item != PrintObject::Mem(addr, len, is_virt));
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
            Cli::Undisplay(PrintCmd::Priv) => {
                self.watch_list
                    .retain(|&item| item != PrintObject::Privilege);
            }

            Cli::History { count } => {
                let history = self.dbg.pc_history().collect::<Vec<_>>();
                let skip_len = history.len().saturating_sub(count);
                for (idx, (addr, instr)) in history.into_iter().skip(skip_len).enumerate() {
                    println!(
                        "{}: pc = {}, {}",
                        format_idx(idx),
                        format_addr(addr),
                        self.asm_formatted(instr)
                    );
                }
            }

            Cli::List => {
                const LIST_INSTR: WordType = 10;

                // TODO: This may not a valid instruction start for variable-length ISA.
                let curr_addr = (self.dbg.read_pc() - LIST_INSTR * 2) as WordType;
                let mut curr_addr = Address::Virt(curr_addr);

                for _ in 0..LIST_INSTR {
                    let is_curr_line = curr_addr == Address::Virt(self.dbg.read_pc());

                    if is_curr_line {
                        print!("{} ", palette.arrow(">"));
                    } else {
                        print!("  ");
                    }

                    let raw = self.dbg.read_instr(curr_addr.value());
                    let (raw_formatted, asm) = self.raw_and_asm_formatted(raw);

                    println!("{}: {} {}", format_address(curr_addr), raw_formatted, asm);

                    match raw {
                        Some(raw) => {
                            curr_addr = curr_addr + raw.len();
                        }
                        None => {
                            curr_addr = curr_addr + 4; // TODO: How long should I go if I failed to read raw instruction? 
                        }
                    }
                }
            }

            Cli::Info(InfoCmd::Breakpoints) => {
                println!("Breakpoints:");
                // TODO: Unnecessary copy to by pass borrow check.
                let breakpoints: Vec<_> = self.dbg.breakpoints().clone();
                for (idx, bp) in breakpoints.into_iter().enumerate() {
                    println!(
                        "{}: {}, {}",
                        format_idx(idx),
                        format_address(bp.addr),
                        self.asm_formatted_at(bp.addr),
                    );
                }
            }

            Cli::Si => {
                self.handle_continue(1)?;
            }

            Cli::Continue { steps } => {
                self.handle_continue(steps)?;
            }

            Cli::Breakpoint { delete, addr, virt } => {
                let addr = make_address(parse_word(&addr)?, virt);
                if delete {
                    self.dbg.clear_breakpoint(addr).map_err(|e| e.to_string())?;
                    println!(
                        "cleared breakpoint at {}: {}",
                        format_address(addr),
                        self.asm_formatted_at(addr)
                    );
                } else {
                    self.dbg.set_breakpoint(addr).map_err(|e| e.to_string())?;
                    println!(
                        "set breakpoint at {}: {}",
                        format_address(addr),
                        self.asm_formatted_at(addr)
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
            palette.reg(REG_NAME[idx as usize], 5),
            format_data(val)
        );
        Ok(())
    }

    fn print_regs(&self, start: u8, len: u8) -> Result<(), String> {
        let reg_base_name = String::from("x");
        for i in start..start + len {
            if i >= REGFILE_CNT as u8 {
                return Err(String::from("register index out of range."));
            }
            print!("{:<4} ", reg_base_name.clone() + &i.to_string() + ".",);
            self.print_reg(i as u8)?;
        }
        Ok(())
    }

    fn print_float_reg(&self, idx: u8) -> Result<(), String> {
        let val = self.dbg.read_float_reg(idx);
        println!(
            "{} = {:.4}",
            palette.reg(FLOAT_REG_NAME[idx as usize], 0),
            val
        );
        Ok(())
    }

    fn print_mem(&mut self, addr: WordType, len: u32, is_virt: bool) {
        const BYTE_PER_LINE: u32 = 16;

        let mut curr_addr = make_address(addr, is_virt);
        let mut i = 0 as u32;
        while i < len {
            if i % BYTE_PER_LINE == 0 {
                if i != 0 {
                    println!();
                }
                print!("{}: ", format_address(curr_addr));
            }
            print!("{} ", self.read_byte_formatted(curr_addr));
            curr_addr = curr_addr + 1;
            i += 1;
        }

        if len > 0 {
            println!();
        }
    }

    fn print_csr(&mut self, csr_addr: WordType) {
        if let Some(v) = self.dbg.read_csr(csr_addr) {
            #[cfg(feature = "riscv64")]
            println!("{}", format_data_64(v));
            #[cfg(feature = "riscv32")]
            println!("{}", format_data(v));
        } else {
            println!("Illegal CSR.")
        }
    }

    fn print_privilege(&mut self) {
        let privilege = self.dbg.get_current_privilege();
        println!("{}", format_privilege(privilege))
    }

    fn read_byte_formatted(&mut self, addr: Address) -> impl std::fmt::Display {
        self.dbg
            .read_memory::<u8>(addr)
            .map(|b| format!("{:02x}", b))
            .unwrap_or("**".into())
    }

    fn read_word_formatted(&mut self, addr: Address) -> impl std::fmt::Display {
        let word = self.dbg.read_memory::<u32>(addr);
        word.map(|w| format!("0x{:08x}", w))
            .unwrap_or("<invalid>".into())
    }

    fn asm_formatted(&mut self, raw: Option<I::RawInstr>) -> impl std::fmt::Display {
        self.raw_and_asm_formatted(raw).1
    }

    fn asm_formatted_at(&mut self, addr: Address) -> impl std::fmt::Display {
        let raw = self.dbg.read_instr_directly(addr);
        self.asm_formatted(raw)
    }

    fn current_asm_formatted(&mut self) -> impl std::fmt::Display {
        let raw = self.dbg.current_instr();
        self.asm_formatted(raw)
    }

    fn raw_and_asm_formatted(
        &self,
        raw: Option<I::RawInstr>,
    ) -> (impl std::fmt::Display, impl std::fmt::Display) {
        (
            <I as AsmFormattable<I>>::format_raw(raw),
            <I as AsmFormattable<I>>::format_asm(raw.and_then(|raw| self.dbg.decoded_info(raw))),
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

    fn reg(&self, reg: &str, padding: usize) -> impl std::fmt::Display {
        format!("{:<width$}", reg, width = padding).magenta()
    }

    fn csr(&self, csr: &str) -> impl std::fmt::Display {
        csr.dark_grey()
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

    fn privilege(&self, value: &str) -> impl std::fmt::Display {
        value.dark_grey()
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
    if let Some(index) = CSR_ADDRESS.get(t) {
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

fn format_address(addr: Address) -> impl std::fmt::Display {
    match addr {
        Address::Phys(addr) | Address::Virt(addr) => format_addr(addr),
    }
}

fn format_data(data: WordType) -> impl std::fmt::Display {
    palette.data(&format!("0x{:08x}", data)).to_string()
}

fn format_privilege(privilege: PrivilegeLevel) -> impl std::fmt::Display {
    palette.privilege(&format!("{:?}", privilege)).to_string()
}

// TODO: format into 0x01234_4567_89ab_cdef
fn format_data_64(data: WordType) -> impl std::fmt::Display {
    palette.data(&format!("0x{:016x}", data)).to_string()
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
            RVInstrInfo::I { rd, rs1, imm } => match instr {
                RiscvInstr::CSRRC | RiscvInstr::CSRRS | RiscvInstr::CSRRW => {
                    format!(
                        "{} {},{},{} - type I",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize], 0),
                        palette.csr(
                            CSR_NAME
                                .get(&imm)
                                .unwrap_or(&format!("csr[0x{:03x}]", imm).as_str())
                        ),
                        palette.reg(REG_NAME[rs1 as usize], 0),
                    )
                }
                RiscvInstr::CSRRCI | RiscvInstr::CSRRSI | RiscvInstr::CSRRWI => {
                    format!(
                        "{} {},{},{} - type I",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize], 0),
                        palette.csr(
                            CSR_NAME
                                .get(&imm)
                                .unwrap_or(&format!("csr[0x{:03x}]", imm).as_str())
                        ),
                        palette.data(rs1.to_string().as_str()),
                    )
                }
                _ => {
                    format!(
                        "{} {},{},{} - type I",
                        palette.instr(instr.name()),
                        palette.reg(REG_NAME[rd as usize], 0),
                        palette.reg(REG_NAME[rs1 as usize], 0),
                        palette.data(imm.to_string().as_str()),
                    )
                }
            },

            RVInstrInfo::R { rs1, rs2, rd } => {
                format!(
                    "{} {},{},{} - type R",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0)
                )
            }

            RVInstrInfo::R_rm { rs1, rs2, rd, rm } => {
                format!(
                    "{} {},{},{} rm={} - type R",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0),
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
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0),
                    palette.reg(REG_NAME[rs3 as usize], 0),
                    rm
                )
            }

            RVInstrInfo::B { rs1, rs2, imm } => {
                format!(
                    "{} {},{},{} - type B",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0),
                    palette.data((imm >> 1).to_string().as_str())
                )
            }

            RVInstrInfo::J { rd, imm } => {
                format!(
                    "{} {},{} - type J",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.data((imm >> 12).to_string().as_str())
                )
            }

            RVInstrInfo::S { rs1, rs2, imm } => {
                format!(
                    "{} {},{},{} - type S",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0),
                    palette.data((imm).to_string().as_str())
                )
            }
            RVInstrInfo::U { rd, imm } => {
                format!(
                    "{} {},{} - type U",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
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
