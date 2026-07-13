use std::io::IsTerminal;
use std::io::stdin;

use crate::board::Board;

use super::printer::Printer;
use super::session::RvdbSession;
use rustyline::error::ReadlineError;

const PROMPT: &str = "(rvdb) ";

pub struct NativeREPL<B: Board> {
    editor: rustyline::DefaultEditor,
    session: RvdbSession<B>,
    last_line: String,
}

impl<B: Board> NativeREPL<B> {
    pub fn new(board: B) -> Self {
        if stdin().is_terminal() {
            crossterm::terminal::disable_raw_mode().unwrap();
        }

        Self {
            editor: rustyline::DefaultEditor::new().expect("Failed to create line editor of rvdb."),
            session: RvdbSession::with_printer(board, Printer::ansi_color()),
            last_line: String::new(),
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

            match self.session.execute_line(line) {
                Ok(response) => {
                    print!("{}", response.text);
                    if response.exit {
                        return true;
                    }
                }
                Err(err) => println!("Error: {}", err),
            }
        }

        false
    }

    /// REPL main loop.
    pub fn run(&mut self) {
        loop {
            match self.editor.readline(PROMPT) {
                Ok(line) => {
                    let mut line = line.trim();

                    if line.is_empty() == false {
                        self.last_line = line.to_string();
                        let _ = self.editor.add_history_entry(line);
                    } else if self.last_line.is_empty() == false {
                        // Repeat the last command if the current line is empty.
                        line = self.last_line.as_str();
                    } else {
                        continue;
                    }

                    match self.session.execute_line(line) {
                        Ok(response) => {
                            print!("{}", response.text);
                            if response.exit {
                                break;
                            }
                        }
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
    use crate::board::virt::VirtBoard;

    #[test]
    fn drop_should_not_hang() {
        should_success_within(Duration::from_millis(100), || {
            let board = VirtBoard::from_binary_with(&[], Default::default()).unwrap();
            let _repl = NativeREPL::new(board);
        });
    }
}
