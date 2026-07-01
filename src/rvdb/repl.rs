use std::io::IsTerminal;
use std::io::stdin;
use std::process::exit;

use super::Cli;

use super::CommandOutput;
use super::handler::Handler;
use super::printer::Printer;
use clap::Parser;
use riscv_emulator::board::Board;
use rustyline::error::ReadlineError;

const PROMPT: &str = "(rvdb) ";

pub struct DebugREPL<'a, B: Board> {
    editor: rustyline::DefaultEditor,
    handler: Handler<'a, B>,
    printer: Printer,
}

impl<'a, B: Board> DebugREPL<'a, B> {
    pub fn new(board: &'a mut B) -> Self {
        if stdin().is_terminal() {
            crossterm::terminal::disable_raw_mode().unwrap();
        }
        board.pause_background_work();

        Self {
            editor: rustyline::DefaultEditor::new().expect("Failed to create line editor of rvdb."),
            handler: Handler::new(board),
            printer: Printer::new(),
        }
    }

    /// Run multiple lines of commands in sequence.
    ///
    /// Return true if the script contains an exit command, and false otherwise.
    pub fn run_script(&mut self, lines: &[String]) -> bool {
        for line in lines {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            println!("{}{}", PROMPT, line);

            match self.process_line(line) {
                Ok(CommandOutput::Exit) => {
                    exit(0);
                }
                Ok(output) => self.printer.print(&output),
                Err(err) => println!("Error: {}", err),
            }
        }

        false
    }

    /// REPL main loop.
    pub fn run(&mut self) {
        let mut last_line = String::new();

        loop {
            match self.editor.readline(PROMPT) {
                Ok(line) => {
                    let mut line = line.trim();

                    if line.is_empty() == false {
                        last_line = line.to_string();
                        self.editor.add_history_entry(line).unwrap();
                    } else if last_line.is_empty() == false {
                        // Repeat the last command if the current line is empty.
                        line = last_line.as_str();
                    }

                    let _ = self.editor.add_history_entry(line);
                    match self.process_line(&line) {
                        Ok(CommandOutput::Exit) => break,
                        Ok(output) => self.printer.print(&output),
                        Err(err) => println!("Error: {}", err),
                    }
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    break;
                }
                Err(ex) => {
                    println!("Error: {:?}", ex);
                    break;
                }
            }
        }
    }

    fn process_line(&mut self, line: &str) -> Result<CommandOutput, String> {
        let argv = line.split_whitespace().map(|s| s.to_string());
        let cli = Cli::try_parse_from(argv).map_err(|e| e.to_string())?;
        self.handler.handle(cli)
    }
}

#[cfg(test)]
mod test {
    use std::{sync::mpsc, thread, time::Duration};

    fn should_end_within<T, F>(
        d: Duration,
        f: F,
    ) -> Result<T, Box<dyn std::any::Any + Send + 'static>>
    where
        T: Send + 'static,
        F: Send + 'static + FnOnce() -> T,
    {
        let (done_tx, done_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let val = f();
            done_tx.send(()).expect("Unable to send completion signal");
            val
        });

        match done_rx.recv_timeout(d) {
            Ok(_) => handle.join(),
            Err(_) => panic!("Thread took too long"),
        }
    }

    fn should_success_within<T, F>(d: Duration, f: F) -> T
    where
        T: Send + 'static,
        F: Send + 'static + FnOnce() -> T,
    {
        should_end_within(d, f).expect("thread panicked")
    }

    use super::*;
    use riscv_emulator::board::virt::VirtBoard;

    #[test]
    fn drop_should_not_hang() {
        should_success_within(Duration::from_millis(100), || {
            let mut board = VirtBoard::from_binary(&[]);
            let _repl = DebugREPL::new(&mut board);
        });
    }
}
