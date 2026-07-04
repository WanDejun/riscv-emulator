use super::{CommandOutput, PrintObject, RvdbCommand, printer::Printer};
use crate::{
    board::Board,
    isa::riscv::debugger::Debugger,
    load::{ELFLoader, SymTab},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RvdbCommandResponse {
    pub text: String,
    pub exit: bool,
}

pub struct RvdbSession<B: Board> {
    pub(super) dbg: Debugger<B>,
    pub(super) watch_list: Vec<PrintObject>,
    printer: Printer,
}

impl<B: Board> RvdbSession<B> {
    pub fn new(board: B) -> Self {
        Self::with_printer(board, Printer::plain())
    }

    pub fn with_printer(mut board: B, printer: Printer) -> Self {
        board.pause_background_work();
        Self {
            dbg: Debugger::new(board),
            watch_list: Vec::new(),
            printer,
        }
    }

    pub fn execute_line(&mut self, line: &str) -> Result<RvdbCommandResponse, String> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(RvdbCommandResponse {
                text: String::new(),
                exit: false,
            });
        }

        let cmd = self.parse_line(line)?;
        self.execute_command(cmd)
    }

    pub(super) fn execute_command(
        &mut self,
        cmd: RvdbCommand,
    ) -> Result<RvdbCommandResponse, String> {
        let output = self.handle_command(cmd)?;

        Ok(RvdbCommandResponse {
            text: self.printer.render(&output),
            exit: output == CommandOutput::Exit,
        })
    }

    pub(super) fn render_output(&self, output: &CommandOutput) -> String {
        self.printer.render(output)
    }

    pub fn load_symbol_file_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        let loader = ELFLoader::try_new(bytes).ok_or("Failed to parse ELF file")?;
        if let Some(symtab) = loader.get_symbol_table() {
            self.set_symbol_table(symtab);
            Ok(())
        } else {
            Err("No symbol table found in ELF file".to_string())
        }
    }

    pub fn set_symbol_table(&mut self, symtab: SymTab) {
        self.dbg.set_symbol_table(symtab);
    }

    pub fn board(&self) -> &B {
        self.dbg.board()
    }

    pub fn board_mut(&mut self) -> &mut B {
        self.dbg.board_mut()
    }

    pub fn into_board(self) -> B {
        self.dbg.into_board()
    }
}
