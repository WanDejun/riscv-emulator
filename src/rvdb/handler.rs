use std::fs;

use super::*;

#[cfg(not(test))]
use riscv_emulator::cli_coordinator::CliCoordinator;

use riscv_emulator::{
    board::Board,
    config::arch_config::{FLOAT_REG_NAME, REG_NAME, REGFILE_CNT, WordType},
    isa::riscv::{
        csr_reg::csr_macro::{CSR_ADDRESS, CSR_NAME},
        debugger::{Address, Debugger},
        mmu::AccessType,
    },
    load::ELFLoader,
};

pub struct Handler<'a, B: Board> {
    dbg: Debugger<'a, B>,
    watch_list: Vec<PrintObject>,
}

impl<'a, B: Board> Handler<'a, B> {
    pub fn new(board: &'a mut B) -> Self {
        Self {
            dbg: Debugger::new(board),
            watch_list: Vec::new(),
        }
    }

    pub fn handle(&mut self, cli: Cli) -> Result<CommandOutput, String> {
        match cli {
            Cli::Print(cmd) => self.handle_print(cmd),
            Cli::Display(cmd) => self.handle_display(cmd),
            Cli::Undisplay(cmd) => self.handle_undisplay(cmd),
            Cli::Translate { addr, access } => self.handle_translate(addr, access.into()),
            Cli::List => self.handle_list(),
            Cli::History { count } => self.handle_history(count),
            Cli::FTrace { count } => self.handle_ftrace(count),
            Cli::Si => self.handle_step(),
            Cli::Continue { steps } => self.handle_continue(steps),
            Cli::Breakpoint {
                delete,
                symbol,
                virt,
            } => self.handle_breakpoint(delete, symbol, virt),
            Cli::Info(cmd) => self.handle_info(cmd),
            Cli::Quit => Ok(CommandOutput::Exit),
            Cli::SymbolFile { path } => self.handle_symbol_file(path),
        }
    }

    fn handle_translate(
        &mut self,
        addr: String,
        kind: AccessType,
    ) -> Result<CommandOutput, String> {
        let virt_addr = parse_u64(&addr)?;
        let phys_addr = self
            .dbg
            .translate(virt_addr, kind)
            .map_err(|e| format!("{:?}", e))?;
        Ok(CommandOutput::Translate {
            phys_addr,
            virt_addr,
        })
    }

    fn handle_symbol_file(&mut self, path: String) -> Result<CommandOutput, String> {
        let bytes = fs::read(&path).map_err(|e| e.to_string() + ", when reading " + &path)?;
        let loader = ELFLoader::try_new(bytes).ok_or("Failed to parse ELF file")?;
        if let Some(symtab) = loader.get_symbol_table() {
            self.dbg.set_symbol_table(symtab);
            Ok(CommandOutput::None)
        } else {
            return Err("No symbol table found in ELF file".to_string());
        }
    }

    fn handle_ftrace(&mut self, count: usize) -> Result<CommandOutput, String> {
        Ok(CommandOutput::FTrace(
            self.dbg.ftrace().take(count).collect(),
        ))
    }

    fn handle_print(&mut self, cmd: PrintCmd) -> Result<CommandOutput, String> {
        match cmd {
            PrintCmd::Pc => Ok(CommandOutput::Pc(self.dbg.read_pc())),
            PrintCmd::Reg { reg } => {
                let idx = parse_common_reg(&reg)?;
                Ok(CommandOutput::Reg {
                    name: REG_NAME[idx as usize].to_string(),
                    val: self.dbg.read_reg(idx),
                })
            }
            PrintCmd::Regs { start, len } => {
                let mut regs = Vec::new();
                for i in start..start + len {
                    if i >= REGFILE_CNT as u8 {
                        break;
                    }
                    regs.push((REG_NAME[i as usize], self.dbg.read_reg(i)));
                }
                Ok(CommandOutput::Regs(regs))
            }
            PrintCmd::Mem { addr, len, virt } => {
                let addr_val = parse_u64(&addr)?;
                let start_addr = make_address(addr_val, virt);
                let mut data = Vec::new();
                let mut curr = start_addr;
                for _ in 0..len {
                    let byte = self.dbg.read_memory::<u8>(curr).ok();
                    data.push(byte);
                    curr = curr + 1;
                }
                Ok(CommandOutput::Mem {
                    addr: start_addr,
                    data,
                })
            }
            PrintCmd::Csr { addr } => {
                let csr_addr = parse_csr(&addr)?;
                let name = CSR_NAME
                    .get(&csr_addr)
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("0x{:03x}", csr_addr));
                Ok(CommandOutput::Csr {
                    name,
                    val: self.dbg.read_csr(csr_addr),
                })
            }
            PrintCmd::FReg { reg } => {
                let idx = parse_float_reg(&reg)?;
                let (f32_val, f64_val) = self.dbg.read_float_reg(idx);
                Ok(CommandOutput::FReg {
                    name: FLOAT_REG_NAME[idx as usize].to_string(),
                    f32_val,
                    f64_val,
                })
            }
            PrintCmd::Priv => Ok(CommandOutput::Privilege(self.dbg.get_current_privilege())),
        }
    }

    fn handle_display(&mut self, cmd: PrintCmd) -> Result<CommandOutput, String> {
        let obj = match cmd {
            PrintCmd::Pc => PrintObject::Pc,
            PrintCmd::Reg { reg } => PrintObject::Reg(parse_common_reg(&reg)?),
            PrintCmd::Regs { start, len } => PrintObject::Regs(start, len),
            PrintCmd::Mem { addr, len, virt } => PrintObject::Mem(parse_u64(&addr)?, len, virt),
            PrintCmd::Csr { addr } => PrintObject::CSR(parse_csr(&addr)?),
            PrintCmd::FReg { reg } => PrintObject::FReg(parse_float_reg(&reg)?),
            PrintCmd::Priv => PrintObject::Privilege,
        };
        self.watch_list.push(obj);
        Ok(CommandOutput::None)
    }

    fn handle_undisplay(&mut self, cmd: PrintCmd) -> Result<CommandOutput, String> {
        let target = match cmd {
            PrintCmd::Pc => PrintObject::Pc,
            PrintCmd::Reg { reg } => PrintObject::Reg(parse_common_reg(&reg)?),
            PrintCmd::Regs { start, len } => PrintObject::Regs(start, len),
            PrintCmd::Mem { addr, len, virt } => PrintObject::Mem(parse_u64(&addr)?, len, virt),
            PrintCmd::Csr { addr } => PrintObject::CSR(parse_csr(&addr)?),
            PrintCmd::FReg { reg } => PrintObject::FReg(parse_float_reg(&reg)?),
            PrintCmd::Priv => PrintObject::Privilege,
        };
        self.watch_list.retain(|item| *item != target);
        Ok(CommandOutput::None)
    }

    fn handle_list(&mut self) -> Result<CommandOutput, String> {
        const NUM_LINES: WordType = 20;

        let pc = self.dbg.read_pc();
        // FIXME: Cannot handle compressed instructions.
        let start_addr = pc.saturating_sub(NUM_LINES * 4 / 2);
        let mut lines = Vec::new();

        for i in 0..NUM_LINES {
            let addr = start_addr + i * 4;

            lines.push(self.instr_from_addr(addr));
        }
        Ok(CommandOutput::CodeList(lines))
    }

    fn handle_history(&mut self, count: usize) -> Result<CommandOutput, String> {
        let history: Vec<_> = self
            .dbg
            .pc_history()
            .take(count)
            .map(|(addr, raw)| DbgInstrLine {
                addr,
                raw,
                decoded: raw.and_then(|r| self.dbg.decoded_info(r)),
                symbol: self.dbg.symbol_by_addr(addr).ok().cloned(),
                is_current_pc: addr == self.dbg.read_pc(),
            })
            .collect();
        Ok(CommandOutput::History(history))
    }

    fn handle_step(&mut self) -> Result<CommandOutput, String> {
        self.handle_continue(1)
    }

    fn handle_continue(&mut self, steps: u64) -> Result<CommandOutput, String> {
        #[cfg(not(test))]
        CliCoordinator::global().resume_uart();

        let rst = self.dbg.continue_until_step(steps);

        #[cfg(not(test))]
        CliCoordinator::global().pause_uart();

        let (event, actual_steps) = match rst {
            Ok(rst) => rst,
            Err(e) => return Err(format!("step failed: {}", e)),
        };

        let watch_results = self.collect_watch_results()?;
        let pc = self.dbg.read_pc();

        Ok(CommandOutput::ContinueDone {
            instr: self.instr_from_addr(pc),
            watch_results,
            event,
            actual_steps,
        })
    }

    fn collect_watch_results(&mut self) -> Result<Vec<CommandOutput>, String> {
        let mut results = Vec::new();
        let watch_list = self.watch_list.clone();

        for item in watch_list {
            let output = match item {
                PrintObject::Pc => self.handle_print(PrintCmd::Pc)?,
                PrintObject::Reg(idx) => {
                    let name = REG_NAME[idx as usize].to_string();
                    self.handle_print(PrintCmd::Reg { reg: name })?
                }
                PrintObject::Regs(start, len) => {
                    self.handle_print(PrintCmd::Regs { start, len })?
                }
                PrintObject::Mem(addr, len, virt) => {
                    let addr_str = format!("0x{:x}", addr);
                    self.handle_print(PrintCmd::Mem {
                        addr: addr_str,
                        len,
                        virt,
                    })?
                }
                PrintObject::CSR(addr) => {
                    let addr_str = format!("0x{:x}", addr);
                    self.handle_print(PrintCmd::Csr { addr: addr_str })?
                }
                PrintObject::FReg(idx) => {
                    let name = FLOAT_REG_NAME[idx as usize].to_string();
                    self.handle_print(PrintCmd::FReg { reg: name })?
                }
                PrintObject::Privilege => self.handle_print(PrintCmd::Priv)?,
            };
            results.push(output);
        }
        Ok(results)
    }

    fn handle_breakpoint(
        &mut self,
        delete: bool,
        symbol: String,
        virt: bool,
    ) -> Result<CommandOutput, String> {
        let (addr_val, symbol_name) = if let Ok(addr) = parse_u64(&symbol) {
            (addr, None)
        } else if let Ok(addr) = self.dbg.addr_by_symbol(&symbol) {
            (addr, Some(symbol))
        } else {
            return Err(format!("Symbol not found: {}", symbol));
        };

        let address = make_address(addr_val, virt);

        if delete {
            let ok = self
                .dbg
                .clear_breakpoint(address)
                .map_err(|err| err.to_string())?;

            Ok(CommandOutput::BreakpointCleared {
                addr: address,
                symbol: symbol_name,
                ok,
            })
        } else {
            let ok = self
                .dbg
                .set_breakpoint(address)
                .map_err(|err| err.to_string())?;

            Ok(CommandOutput::BreakpointSet {
                ok,
                addr: address,
                symbol: symbol_name,
            })
        }
    }

    fn handle_info(&mut self, cmd: InfoCmd) -> Result<CommandOutput, String> {
        match cmd {
            InfoCmd::Breakpoints => Ok(CommandOutput::Breakpoints(self.dbg.breakpoints().clone())),
            InfoCmd::Symbols => {
                let Some(symbol_table) = self.dbg.symbol_table() else {
                    return Err("No symbol table available".to_string());
                };

                Ok(CommandOutput::Symbols(
                    symbol_table.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                ))
            }
        }
    }

    fn instr_from_addr(&mut self, addr: WordType) -> DbgInstrLine {
        let raw = self.dbg.read_instr(addr);
        let decoded = raw.and_then(|r| self.dbg.decoded_info(r));
        let symbol = self.dbg.symbol_by_addr(addr).ok().cloned();

        DbgInstrLine {
            addr,
            raw,
            decoded,
            symbol,
            is_current_pc: addr == self.dbg.read_pc(),
        }
    }
}

fn make_address(addr: u64, virt: bool) -> Address {
    if virt {
        Address::Virt(addr)
    } else {
        Address::Phys(addr)
    }
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

    Err(format!("invalid csr: {}", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    use riscv_emulator::board::virt::VirtBoard;

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

    fn create_board() -> VirtBoard {
        VirtBoard::from_binary(&[])
    }

    #[test]
    fn test_breakpoint_ops() {
        let mut board = create_board();
        let mut handler = Handler::new(&mut board);

        const ADDR: WordType = 0x80001000;

        // Set breakpoint
        let result = handler
            .handle(Cli::Breakpoint {
                delete: false,
                symbol: ADDR.to_string(),
                virt: false,
            })
            .unwrap();

        assert_eq!(
            result,
            CommandOutput::BreakpointSet {
                ok: true,
                addr: Address::Phys(ADDR),
                symbol: None
            }
        );

        // Physical breakpoint cannot be removed by virtual address
        let result = handler
            .handle(Cli::Breakpoint {
                delete: true,
                symbol: ADDR.to_string(),
                virt: true,
            })
            .unwrap();

        assert_eq!(
            result,
            CommandOutput::BreakpointCleared {
                ok: false,
                addr: Address::Virt(ADDR),
                symbol: None
            }
        );

        // Remove breakpoint
        let result = handler
            .handle(Cli::Breakpoint {
                delete: true,
                symbol: ADDR.to_string(),
                virt: false,
            })
            .unwrap();

        assert_eq!(
            result,
            CommandOutput::BreakpointCleared {
                ok: true,
                addr: Address::Phys(ADDR),
                symbol: None
            }
        );
    }
}
