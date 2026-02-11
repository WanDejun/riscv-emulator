use std::{collections::VecDeque, fmt::Debug, ops::Add, u64};

use crate::{
    board::Board,
    config::arch_config::WordType,
    device::MemError,
    isa::{
        DebugTarget, DecoderTrait, ISATypes,
        riscv::{
            RawInstrType, RiscvTypes,
            csr_reg::PrivilegeLevel,
            decoder::DecodeInstr,
            executor::{ExcuteInstrInfo, RVCPU},
            instruction::{RVInstrInfo, instr_table::RiscvInstr},
            trap::Exception,
        },
    },
    load::SymTab,
    utils::UnsignedInteger,
};

#[derive(Debug, Clone, PartialEq)]
pub enum DebugEvent {
    StepCompleted,
    BreakpointHit,
    BoardHalted,
}

#[derive(thiserror::Error, Debug)]
pub enum DebugError {
    #[error("target exception: {0:?}")]
    TargetException(<RiscvTypes as ISATypes>::StepException),

    #[error("memory error: {0:?}")]
    MemoryError(MemError),

    #[error("CSR {0:?} not exist")]
    CSRNotExist(WordType),

    #[error("symbol {0} not found in symbol table")]
    SymbolNotFound(String),

    #[error("symbol table not available")]
    NoSymbolTable,
}

impl From<MemError> for DebugError {
    fn from(e: MemError) -> Self {
        DebugError::MemoryError(e)
    }
}

impl DebugTarget<RiscvTypes> for RVCPU {
    fn read_pc(&self) -> WordType {
        self.pc
    }

    fn write_pc(&mut self, new_pc: WordType) {
        self.pc = new_pc;
    }

    fn read_reg(&self, idx: u8) -> WordType {
        self.reg_file[idx as usize]
    }

    fn write_reg(&mut self, idx: u8, value: WordType) {
        self.reg_file.write(idx, value)
    }

    fn read_instr(&mut self, vaddr: WordType) -> Result<u32, MemError> {
        self.memory.debug_ifetch(vaddr, &mut self.csr)
    }

    fn read_instr_directly(&mut self, addr: Address) -> Result<u32, MemError> {
        self.memory.debug_read(addr)
    }

    fn read_memory<T: UnsignedInteger>(&mut self, addr: Address) -> Result<T, MemError> {
        self.memory.debug_read::<T>(addr)
    }

    fn write_memory<T: UnsignedInteger>(&mut self, addr: Address, data: T) -> Result<(), MemError> {
        self.memory.debug_write::<T>(addr, data)
    }

    fn get_current_privilege(&self) -> PrivilegeLevel {
        self.csr.privelege_level()
    }

    fn read_float_reg(&self, idx: u8) -> (f32, f64) {
        (self.fpu.load::<f32>(idx), self.fpu.load::<f64>(idx))
    }

    /// match input {
    ///     Some() => `Read Write`,
    ///     None => `Read Only`,
    /// }
    fn debug_csr(&mut self, addr: WordType, new_value: Option<WordType>) -> Option<WordType> {
        self.csr.debug(addr, new_value)
    }

    fn step(&mut self) -> Result<(), Exception> {
        RVCPU::step(self)
    }

    fn decoded_instr(&self, instr: u32) -> Option<DecodeInstr> {
        self.decoder.decode(instr)
    }

    fn vaddr_to_paddr(&self, vaddr: WordType) -> Option<u64> {
        self.memory.debug_translate_vaddr(vaddr).ok()
    }

    fn translate(&self, vaddr: WordType) -> Option<u64> {
        if self.get_current_privilege() == PrivilegeLevel::M {
            Some(vaddr as u64)
        } else {
            self.memory.debug_translate_vaddr(vaddr).ok()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Address {
    Virt(WordType),
    Phys(u64),
}

impl Address {
    pub fn value(&self) -> u64 {
        match self.clone() {
            Address::Virt(vaddr) => vaddr as u64,
            Address::Phys(paddr) => paddr,
        }
    }
}

impl Add<u64> for Address {
    type Output = Address;

    fn add(self, rhs: u64) -> Self::Output {
        match self {
            Address::Phys(addr) => Address::Phys(addr + rhs),
            Address::Virt(addr) => Address::Virt(addr + rhs as WordType),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Breakpoint {
    pub id: usize,
    pub addr: Address,
    // TODO: add symbol_name: Option<String> for better user experience
}

#[derive(Debug, Clone, PartialEq)]
pub enum FuncTrace {
    Call { name: Option<String>, addr: u64 },
    Return { name: Option<String>, addr: u64 },
}

const MAX_HISTORY: usize = 128;
const MAX_FTRACE: usize = MAX_HISTORY;

pub struct Debugger<'a, B: Board> {
    breakpoints: Vec<Breakpoint>,
    board: &'a mut B,
    history: VecDeque<(WordType, Option<RawInstrType>)>,
    ftrace: VecDeque<FuncTrace>,
    symtab: Option<SymTab>,
}

impl<'a, B: Board> Debugger<'a, B> {
    pub fn new(board: &'a mut B) -> Self {
        board.cpu_mut().debug = true;
        let symtab = board.loader().and_then(|loader| loader.get_symbol_table());

        Self {
            breakpoints: Vec::new(),
            board,
            history: VecDeque::with_capacity(MAX_HISTORY),
            ftrace: VecDeque::with_capacity(MAX_FTRACE),
            symtab: symtab,
        }
    }

    pub fn set_symbol_table(&mut self, symtab: SymTab) {
        self.symtab = Some(symtab);
    }

    /// TODO: Use `last_instr_info` for performance.
    /// FIXME: History may be incorrect if we have interrupts, use `last_instr_info`.
    fn push_history(&mut self) {
        if self.history.len() == MAX_HISTORY {
            self.history.pop_front();
        }
        let instr = self.read_instr(self.read_pc());
        self.history.push_back((self.read_pc(), instr));
    }

    pub fn pc_history(&self) -> impl DoubleEndedIterator<Item = (WordType, Option<RawInstrType>)> {
        self.history.iter().copied()
    }

    pub fn curr_ftrace(&self) -> Option<FuncTrace> {
        let Some(DecodeInstr(instr_kind, info)) = self.last_instr_info().instr else {
            return None;
        };

        let pc = self.read_pc();
        let symbol = self.symbol_in_addr_range(pc).ok().cloned();

        if instr_kind == RiscvInstr::JALR
            && let RVInstrInfo::I { rs1, rd, imm } = info
        {
            if rd == 0 && rs1 == 1 && imm == 0 {
                // `jalr zero, 0(ra)` -> `ret`
                return Some(FuncTrace::Return {
                    name: symbol,
                    addr: pc,
                });
            } else if rd == 1 && rs1 == 1 {
                // jalr ra, imm(ra) -> `call`
                return Some(FuncTrace::Call {
                    name: symbol,
                    addr: pc,
                });
            } else if rd == 0 && rs1 == 6 {
                // jalr zero, imm(x6) -> `tail`
                return Some(FuncTrace::Call {
                    name: symbol,
                    addr: pc,
                });
            }
        } else if instr_kind == RiscvInstr::JAL
            && let RVInstrInfo::J { imm: _, rd } = info
        {
            if rd == 1 {
                // jal ra, imm -> `call`
                return Some(FuncTrace::Call {
                    name: symbol,
                    addr: pc,
                });
            }
        }

        None
    }

    pub fn ftrace(&self) -> impl Iterator<Item = FuncTrace> {
        self.ftrace.iter().cloned()
    }

    pub fn breakpoints(&self) -> &Vec<Breakpoint> {
        &self.breakpoints
    }

    pub fn symbol_table(&self) -> Option<&SymTab> {
        self.symtab.as_ref()
    }

    pub fn symbol_by_addr(&self, addr: u64) -> Result<&String, DebugError> {
        let Some(symtab) = &self.symtab else {
            return Err(DebugError::NoSymbolTable);
        };
        symtab
            .func_name_by_addr(addr)
            .ok_or(DebugError::SymbolNotFound(format!(
                "Function at address 0x{:08x} not found",
                addr
            )))
    }

    pub fn symbol_in_addr_range(&self, addr: u64) -> Result<&String, DebugError> {
        let Some(symtab) = &self.symtab else {
            return Err(DebugError::NoSymbolTable);
        };
        symtab
            .func_name_in_addr_range(addr)
            .ok_or(DebugError::SymbolNotFound(format!(
                "Function at address 0x{:08x} not found",
                addr
            )))
    }

    pub fn addr_by_symbol(&self, func_name: &str) -> Result<u64, DebugError> {
        let Some(symtab) = &self.symtab else {
            return Err(DebugError::NoSymbolTable);
        };
        symtab
            .func_addr_by_name(func_name)
            .ok_or(DebugError::SymbolNotFound(func_name.to_string()))
    }

    /// Returns true if a new breakpoint is added, otherwise the breakpoint already exists.
    pub fn set_breakpoint(&mut self, addr: Address) -> Result<bool, DebugError> {
        if let Some(_) = self.breakpoints.iter().find(|bp| bp.addr == addr) {
            return Ok(false);
        }
        let breakpoint = Breakpoint {
            id: self.breakpoints.len(),
            addr,
        };
        self.breakpoints.push(breakpoint);

        Ok(true)
    }

    /// Returns true if any breakpoint is removed.
    pub fn clear_breakpoint(&mut self, addr: Address) -> Result<bool, DebugError> {
        let original_len = self.breakpoints.len();
        self.breakpoints.retain(|bp| bp.addr != addr);
        Ok(self.breakpoints.len() != original_len)
    }

    pub fn on_breakpoint(&self) -> bool {
        if let Some(pc_paddr) = self.board.cpu().translate(self.read_pc()) {
            self.breakpoints
                .iter()
                .find(|bp| self.unify_to_phys_addr(bp.addr) == Some(pc_paddr))
                .is_some()
        } else {
            false
        }
    }

    pub fn step(&mut self) -> Result<DebugEvent, DebugError> {
        self.continue_until_step(1).map(|(event, _steps)| event)
    }

    fn cpu_step_internal(&mut self) -> Result<(), DebugError> {
        self.push_history();

        let rst = self
            .board
            .step()
            .map_err(|e| DebugError::TargetException(e));

        if let Some(ftrace) = self.curr_ftrace() {
            self.ftrace.push_back(ftrace);
            if self.ftrace.len() > MAX_FTRACE {
                self.ftrace.pop_front();
            }
        }

        rst
    }

    /// Continue running until a breakpoint is hit, `max_steps` steps are executed or the board is halted.
    /// Returns the event that caused the stop and the actual steps executed.
    pub fn continue_until_step(&mut self, max_steps: u64) -> Result<(DebugEvent, u64), DebugError> {
        let mut remain = max_steps;

        loop {
            if remain == 0 {
                return Ok((DebugEvent::StepCompleted, max_steps));
            }

            if self.board.status() == crate::board::BoardStatus::Halt {
                return Ok((DebugEvent::BoardHalted, max_steps - remain));
            }

            self.cpu_step_internal()?;

            remain -= 1;

            if self.on_breakpoint() {
                return Ok((DebugEvent::BreakpointHit, max_steps - remain));
            }
        }
    }

    /// See [`Self::continue_until_step`].
    pub fn continue_run(&mut self) -> Result<(DebugEvent, u64), DebugError> {
        self.continue_until_step(u64::MAX)
    }

    pub fn last_instr_info(&self) -> ExcuteInstrInfo {
        self.board.cpu().debug_info.last_instr.clone()
    }

    pub fn next_instr(&mut self) -> Option<RawInstrType> {
        let pc = self.board.cpu().read_pc();
        self.board.cpu_mut().read_instr(pc).ok()
    }

    pub fn unify_to_phys_addr(&self, addr: Address) -> Option<u64> {
        match addr {
            Address::Phys(paddr) => Some(paddr),
            Address::Virt(vaddr) => self.vaddr_to_paddr(vaddr),
        }
    }

    // re-export methods from `DebugTarget`

    pub fn read_reg(&self, idx: u8) -> WordType {
        self.board.cpu().read_reg(idx)
    }

    pub fn write_reg(&mut self, idx: u8, val: WordType) {
        self.board.cpu_mut().write_reg(idx, val)
    }

    pub fn read_pc(&self) -> WordType {
        self.board.cpu().read_pc()
    }

    pub fn write_pc(&mut self, val: WordType) {
        self.board.cpu_mut().write_pc(val)
    }

    pub fn read_float_reg(&self, idx: u8) -> (f32, f64) {
        self.board.cpu().read_float_reg(idx)
    }

    pub fn read_instr(&mut self, addr: WordType) -> Option<RawInstrType> {
        self.board.cpu_mut().read_instr(addr).ok()
    }

    pub fn read_memory<V: UnsignedInteger>(&mut self, addr: Address) -> Result<V, MemError> {
        self.board.cpu_mut().read_memory(addr)
    }

    pub fn write_memory<V: UnsignedInteger>(
        &mut self,
        addr: Address,
        data: V,
    ) -> Result<(), MemError> {
        self.board.cpu_mut().write_memory::<V>(addr, data)
    }

    pub fn read_csr(&mut self, addr: WordType) -> Option<WordType> {
        self.board.cpu_mut().debug_csr(addr, None)
    }

    pub fn write_csr(&mut self, addr: WordType, data: WordType) -> Result<(), DebugError> {
        self.board
            .cpu_mut()
            .debug_csr(addr, Some(data))
            .ok_or(DebugError::CSRNotExist(addr))?;
        Ok(())
    }

    pub fn get_current_privilege(&mut self) -> PrivilegeLevel {
        self.board.cpu_mut().get_current_privilege()
    }

    pub fn decoded_info(&self, raw: RawInstrType) -> Option<<RiscvTypes as ISATypes>::DecodeRst> {
        self.board.cpu().decoded_instr(raw)
    }

    pub fn vaddr_to_paddr(&self, vaddr: WordType) -> Option<u64> {
        self.board.cpu().vaddr_to_paddr(vaddr)
    }

    pub fn translate(&self, addr: WordType) -> Option<u64> {
        self.board.cpu().translate(addr)
    }
}

#[cfg(test)]
mod test {
    use crate::{isa::riscv::cpu_tester::TestCPUBuilder, ram_config::BASE_ADDR};

    use super::*;

    struct TestEmptyBoard {
        cpu: RVCPU,
    }

    impl TestEmptyBoard {
        fn new(cpu: RVCPU) -> Self {
            Self { cpu }
        }
    }

    impl Board for TestEmptyBoard {
        fn step(&mut self) -> Result<(), Exception> {
            self.cpu.step()
        }

        fn status(&self) -> crate::board::BoardStatus {
            crate::board::BoardStatus::Running
        }

        fn cpu(&self) -> &RVCPU {
            &self.cpu
        }

        fn cpu_mut(&mut self) -> &mut RVCPU {
            &mut self.cpu
        }

        fn loader(&self) -> Option<&crate::load::ELFLoader> {
            None
        }
    }

    fn create_debugger(cpu: RVCPU) -> Debugger<'static, TestEmptyBoard> {
        Debugger::new(Box::leak(Box::new(TestEmptyBoard::new(cpu))))
    }

    #[test]
    fn test_breakpoint_riscv() {
        // Test that a breakpoint can be hit
        let cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = create_debugger(cpu);

        debugger
            .set_breakpoint(Address::Phys(BASE_ADDR + 4))
            .unwrap();
        debugger.continue_run().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
        assert_eq!(
            debugger
                .read_memory::<u32>(Address::Phys(BASE_ADDR + 4))
                .unwrap(),
            0x02520333
        );

        debugger.step().unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 8);

        debugger
            .set_breakpoint(Address::Phys(BASE_ADDR + 12))
            .unwrap();

        debugger.continue_until_step(2).unwrap();
        assert_eq!(debugger.read_pc(), BASE_ADDR + 12);
        assert_eq!(
            debugger
                .read_memory::<u32>(Address::Phys(BASE_ADDR + 12))
                .unwrap(),
            0x02520333
        );
    }

    #[test]
    fn test_breakpoint_riscv_on_current() {
        let cpu = TestCPUBuilder::new()
            .program(&[
                0x02520333, // mul x6, x4, x5
                0x02520333, // mul x6, x4, x5
            ])
            .build();

        let mut debugger = create_debugger(cpu);
        debugger.set_breakpoint(Address::Phys(BASE_ADDR)).unwrap();

        debugger.step().unwrap();

        assert_eq!(debugger.read_pc(), BASE_ADDR + 4);
    }
}
