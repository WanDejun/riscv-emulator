//! TODO: Legacy dead code

use std::io::Write;

use crate::board::Board;

use super::printer::Printer;
use super::session::RvdbSession;
use rustyline::error::ReadlineError;

pub struct RustylineREPL<B: Board> {
    editor: rustyline::DefaultEditor,
    session: RvdbSession<B>,
    last_line: String,
}

impl<B: Board> RustylineREPL<B> {
    pub fn new(board: B) -> Self {
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
        let mut stdout = std::io::stdout().lock();
        match self.session.run_script(lines, |output| {
            stdout.write_all(output.as_bytes())?;
            stdout.flush()
        }) {
            Ok(exit) => exit,
            Err(error) => {
                log::error!("failed to write rvdb output: {error}");
                false
            }
        }
    }

    /// REPL main loop.
    pub fn run(&mut self) {
        loop {
            match self.editor.readline(super::PROMPT) {
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
            let _repl = RustylineREPL::new(board);
        });
    }
}
