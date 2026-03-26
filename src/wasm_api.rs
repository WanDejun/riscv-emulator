use std::panic;

use wasm_bindgen::prelude::*;

use crate::{
    Emulator,
    board::{Board, BoardStatus},
    isa::DebugTarget,
};

#[wasm_bindgen]
pub struct WasmEmulator {
    inner: Emulator,
}

#[wasm_bindgen(start)]
fn init_on_wasm() {
    wasm_logger::init(wasm_logger::Config::default());
    panic::set_hook(Box::new(console_error_panic_hook::hook));
}

#[wasm_bindgen]
impl WasmEmulator {
    pub fn from_elf_bytes(bytes: &[u8]) -> Result<Self, JsValue> {
        let inner = Emulator::try_from_elf_bytes(bytes.to_vec())
            .map_err(|e| JsValue::from_str(&format!("ELF load failed: {e}")))?;
        Ok(Self { inner })
    }

    pub fn from_bin_bytes(bytes: &[u8]) -> Self {
        Self {
            inner: Emulator::from_binary_bytes(bytes),
        }
    }

    pub fn step(&mut self) -> Result<(), JsValue> {
        self.inner
            .step()
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))
    }

    pub fn continue_for_steps(&mut self, max_steps: u64) -> Result<u64, JsValue> {
        self.inner
            .run_steps(max_steps)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))
    }

    pub fn is_halted(&self) -> bool {
        self.inner.board().status() == BoardStatus::Halt
    }

    pub fn clock_cycles(&self) -> u64 {
        self.inner.board().clock.now()
    }

    pub fn read_pc(&self) -> u64 {
        self.inner.board().cpu().read_pc() as u64
    }

    pub fn read_reg(&self, idx: u8) -> u64 {
        self.inner.board().cpu().read_reg(idx)
    }

    pub fn push_uart_input(&self, input: &[u8]) {
        self.inner.push_uart_input_bytes(input);
    }

    pub fn take_uart_output(&self) -> Vec<u8> {
        self.inner.take_uart_output_bytes()
    }
}
