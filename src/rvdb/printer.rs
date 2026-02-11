use crate::rvdb::DbgInstrLine;

use super::CommandOutput;
use crossterm::style::Stylize;
use lazy_static::lazy_static;
use riscv_emulator::{
    config::arch_config::{REG_NAME, WordType},
    isa::riscv::{
        RawInstrType,
        csr_reg::{PrivilegeLevel, csr_macro::CSR_NAME},
        debugger::{self, Address},
        decoder::DecodeInstr,
        instruction::{RVInstrInfo, instr_table::RiscvInstr},
    },
};

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

    fn identifier(&self, value: &str) -> impl std::fmt::Display {
        value.yellow()
    }

    fn invalid(&self, value: &str) -> impl std::fmt::Display {
        value.red()
    }

    fn privilege(&self, value: &str) -> impl std::fmt::Display {
        value.dark_grey()
    }
}

pub struct Printer;

impl Printer {
    pub fn new() -> Self {
        Self
    }

    pub fn print(&self, output: &CommandOutput) {
        match output {
            CommandOutput::None => {}
            CommandOutput::Exit => {}

            CommandOutput::Pc(pc) => {
                println!("pc = {}", format_addr(*pc));
            }
            CommandOutput::Reg { name, val } => {
                println!("{} = {}", palette.reg(name, 3), format_data(*val));
            }
            CommandOutput::Regs(regs) => {
                for (idx, (name, val)) in regs.iter().enumerate() {
                    print!("x{:<3} ", idx);
                    println!("{} = {}", palette.reg(name, 5), format_data(*val));
                }
            }
            CommandOutput::FReg {
                name,
                f32_val,
                f64_val,
            } => {
                println!(
                    "{} = {{f32: {}, f64: {}}}",
                    palette.reg(name, 0),
                    f32_val,
                    f64_val,
                );
            }
            CommandOutput::Csr { name, val } => {
                if let Some(v) = val {
                    #[cfg(feature = "riscv64")]
                    println!("{} = {}", name, format_data_64(*v));
                    #[cfg(feature = "riscv32")]
                    println!("{} = {}", name, format_data(*v));
                } else {
                    println!("Illegal CSR.");
                }
            }
            CommandOutput::Mem { addr, data } => {
                const BYTE_PER_LINE: u32 = 16;
                let mut curr_addr = *addr;
                let mut i = 0;
                let len = data.len() as u32;

                while i < len {
                    if i % BYTE_PER_LINE == 0 {
                        print!("{}: ", format_address(curr_addr));
                    }
                    print!("{} ", format!("{:02x}", data[i as usize]));

                    curr_addr = curr_addr + 1;
                    i += 1;
                    if i % BYTE_PER_LINE == 0 {
                        println!();
                    }
                }
                if len > 0 && len % BYTE_PER_LINE != 0 {
                    println!();
                }
            }
            CommandOutput::Privilege(privilege) => {
                println!("{}", format_privilege(*privilege));
            }

            CommandOutput::History(history) => {
                for (i, line) in history.iter().enumerate() {
                    println!("  [{}] {}", format_idx(i), format_instr(line),);
                }
            }
            CommandOutput::CodeList(lines) => {
                for line in lines {
                    if line.is_current_pc {
                        print!("{} ", palette.arrow(">"));
                    } else {
                        print!("  ");
                    }

                    println!("{}", format_instr_detailed(line));
                }
            }
            CommandOutput::Breakpoints(bps) => {
                for bp in bps {
                    println!("{}: {}", format_idx(bp.id), format_address(bp.addr));
                }
            }
            CommandOutput::Symbols(symbols) => {
                for (name, addr) in symbols {
                    println!("{}: {}", format_addr(*addr), palette.identifier(name));
                }
            }

            CommandOutput::FTrace(traces) => {
                for trace in traces {
                    match trace {
                        debugger::FuncTrace::Call { name, addr } => {
                            let name = name.clone().unwrap_or("???".to_string());
                            println!(
                                "Call   -> [{}@{}]",
                                palette.identifier(&name),
                                format_addr(*addr)
                            );
                        }
                        debugger::FuncTrace::Return { name, addr } => {
                            let name = name.clone().unwrap_or("???".to_string());
                            println!(
                                "Return <- [{}@{}]",
                                palette.identifier(&name),
                                format_addr(*addr)
                            );
                        }
                    }
                }
            }

            CommandOutput::ContinueDone {
                instr,
                watch_results,
                event,
                actual_steps,
            } => {
                match event {
                    debugger::DebugEvent::StepCompleted => {
                        println!("Step completed, next: {}", format_instr(instr));
                    }
                    debugger::DebugEvent::BreakpointHit => {
                        println!("Breakpoint hit: {}", format_instr(instr));
                    }
                    debugger::DebugEvent::BoardHalted => {
                        if *actual_steps == 0 {
                            println!("Board already halted");
                            return;
                        } else {
                            println!(
                                "Board halted after {} steps: {}",
                                actual_steps,
                                format_instr(instr)
                            );
                        }
                    }
                }
                for res in watch_results {
                    self.print(res);
                }
            }

            CommandOutput::BreakpointSet { ok, addr, symbol } => {
                if *ok {
                    if let Some(sym) = symbol {
                        println!("Breakpoint set at {} <{}>", sym, format_address(*addr));
                    } else {
                        println!("Breakpoint set at {}", format_address(*addr));
                    }
                } else {
                    println!("Breakpoint already exists at {}", format_address(*addr));
                }
            }
            CommandOutput::BreakpointCleared { ok, addr, symbol } => {
                if *ok {
                    if let Some(sym) = symbol {
                        println!("Breakpoint removed at {} <{}>", sym, format_address(*addr));
                    } else {
                        println!("Breakpoint removed at {}", format_address(*addr));
                    }
                } else {
                    println!("Breakpoint not found at {}", format_address(*addr));
                }
            }
        }
    }
}

fn format_idx(idx: usize) -> impl std::fmt::Display {
    palette.index(&format!("{:2}", idx)).to_string()
}

fn format_addr(word: WordType) -> impl std::fmt::Display {
    palette.addr(&format!("0x{:08x}", word)).to_string()
}

fn format_address(addr: Address) -> impl std::fmt::Display {
    match addr {
        Address::Phys(addr) => format!("paddr({})", format_addr(addr)),
        Address::Virt(addr) => format!("vaddr({})", format_addr(addr)),
    }
}

fn format_data(data: WordType) -> impl std::fmt::Display {
    palette.data(&format!("0x{:08x}", data)).to_string()
}

fn format_privilege(privilege: PrivilegeLevel) -> impl std::fmt::Display {
    palette.privilege(&format!("{:?}", privilege)).to_string()
}

fn format_data_64(data: WordType) -> impl std::fmt::Display {
    palette.data(&format!("0x{:016x}", data)).to_string()
}

fn format_instr(instr: &DbgInstrLine) -> impl std::fmt::Display {
    if let Some(symbol) = &instr.symbol {
        format!(
            "{}: {} {}",
            format_addr(instr.addr),
            format_asm(instr.decoded),
            palette.identifier(&symbol)
        )
    } else {
        format!("{}: {}", format_addr(instr.addr), format_asm(instr.decoded))
    }
}

fn format_instr_detailed(instr: &DbgInstrLine) -> impl std::fmt::Display {
    if let Some(symbol) = &instr.symbol {
        format!(
            "{}: {} {} {}",
            format_addr(instr.addr),
            format_raw(instr.raw),
            format_asm(instr.decoded),
            palette.identifier(&symbol)
        )
    } else {
        format!(
            "{}: {} {}",
            format_addr(instr.addr),
            format_raw(instr.raw),
            format_asm(instr.decoded)
        )
    }
}

fn format_raw(raw: Option<RawInstrType>) -> impl std::fmt::Display {
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
        RVInstrInfo::I { rd, rs1, imm } => match instr {
            RiscvInstr::CSRRC | RiscvInstr::CSRRS | RiscvInstr::CSRRW => {
                format!(
                    "{} {},{},{}",
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
                    "{} {},{},{}",
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
                    "{} {},{},{}",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    palette.data(imm.to_string().as_str()),
                )
            }
        },

        RVInstrInfo::R { rs1, rs2, rd } => {
            format!(
                "{} {},{},{}",
                palette.instr(instr.name()),
                palette.reg(REG_NAME[rd as usize], 0),
                palette.reg(REG_NAME[rs1 as usize], 0),
                palette.reg(REG_NAME[rs2 as usize], 0)
            )
        }

        RVInstrInfo::R_rm { rs1, rs2, rd, rm } => {
            format!(
                "{} {},{},{} rm={}",
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
                "{} {},{},{},{} rm={}",
                palette.instr(instr.name()),
                palette.reg(REG_NAME[rd as usize], 0),
                palette.reg(REG_NAME[rs1 as usize], 0),
                palette.reg(REG_NAME[rs2 as usize], 0),
                palette.reg(REG_NAME[rs3 as usize], 0),
                rm
            )
        }

        RVInstrInfo::A {
            rs1,
            rs2,
            rd,
            rl,
            aq,
        } => {
            if instr.name().starts_with("amo") {
                format!(
                    "{} {},{},({}) rl={}, aq={}",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs2 as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    rl,
                    aq,
                )
            } else {
                // lr or sc
                format!(
                    "{} {},({}) rl={}, aq={}",
                    palette.instr(instr.name()),
                    palette.reg(REG_NAME[rd as usize], 0),
                    palette.reg(REG_NAME[rs1 as usize], 0),
                    rl,
                    aq,
                )
            }
        }

        RVInstrInfo::B { rs1, rs2, imm } => {
            format!(
                "{} {},{},{}",
                palette.instr(instr.name()),
                palette.reg(REG_NAME[rs1 as usize], 0),
                palette.reg(REG_NAME[rs2 as usize], 0),
                palette.data((imm >> 1).to_string().as_str())
            )
        }

        RVInstrInfo::J { rd, imm } => {
            format!(
                "{} {},{}",
                palette.instr(instr.name()),
                palette.reg(REG_NAME[rd as usize], 0),
                palette.data((imm >> 12).to_string().as_str())
            )
        }

        RVInstrInfo::S { rs1, rs2, imm } => {
            format!(
                "{} {},{},{}",
                palette.instr(instr.name()),
                palette.reg(REG_NAME[rs1 as usize], 0),
                palette.reg(REG_NAME[rs2 as usize], 0),
                palette.data((imm).to_string().as_str())
            )
        }
        RVInstrInfo::U { rd, imm } => {
            format!(
                "{} {},{}",
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
