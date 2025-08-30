use std::{cell::UnsafeCell, hint::cold_path, rc::Rc, sync::atomic::Ordering};

use crate::{
    EMULATOR_CONFIG,
    board::{Board, BoardStatus},
    device::{
        config::{Device, POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, UART_SIZE, UART1_ADDR},
        fast_uart::{
            FastUart16550,
            virtual_io::{SerialDestTrait, SerialDestination, SimulationIO, TerminalIO},
        },
        mmio::{MemoryMapIO, MemoryMapItem},
        power_manager::{POWER_OFF_CODE, POWER_STATUS, PowerManager},
    },
    isa::riscv::{RiscvTypes, executor::RV32CPU, mmu::VirtAddrManager, trap::Exception},
    load::load_elf,
    ram::Ram,
    vclock::{Timer, VirtualClockRef},
};

pub struct VirtBoard {
    cpu: RV32CPU,
    clock: VirtualClockRef,
    timer: Rc<Timer>,
    status: BoardStatus,
}

impl VirtBoard {
    pub fn from_elf(elf_bytes: &[u8]) -> Self {
        let mut ram = Ram::new();
        load_elf(&mut ram, elf_bytes);
        Self::from_ram(ram)
    }

    fn init_uart_dest(uart: &FastUart16550) -> Box<dyn SerialDestTrait> {
        if EMULATOR_CONFIG.lock().unwrap().serial_destination == SerialDestination::Stdio {
            Box::new(TerminalIO::new(uart.get_io_channel()))
        } else {
            Box::new(SimulationIO::new(uart.get_io_channel()))
        }
    }

    pub fn from_ram(ram: Ram) -> Self {
        let clock = VirtualClockRef::new();
        let timer = Rc::new(Timer::new(clock.clone()));

        let ram_ref = Rc::new(UnsafeCell::new(ram));

        // Construct devices
        let uart1 = FastUart16550::new();
        Self::init_uart_dest(&uart1);
        let power_manager = Device::PowerManager(PowerManager::new());

        let mmio_items = vec![
            MemoryMapItem::new(POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, power_manager),
            MemoryMapItem::new(UART1_ADDR, UART_SIZE, Device::FastUart16550(uart1)),
        ];

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), mmio_items);
        let vaddr_manager = VirtAddrManager::from_ram_and_mmio(ram_ref.clone(), mmio);

        let cpu = RV32CPU::from_vaddr_manager(vaddr_manager);

        Self {
            cpu: cpu,
            clock: clock,
            timer: timer,
            status: BoardStatus::Running,
        }
    }

    pub fn step_and_halt_if<F>(&mut self, f: &mut F) -> Result<(), Exception>
    where
        F: FnMut(&mut RV32CPU, usize) -> bool,
    {
        self.cpu.step()?;
        self.clock.advance(1);

        if self.clock.now() % 32 == 0 && POWER_STATUS.load(Ordering::Acquire).eq(&POWER_OFF_CODE)
            || f(&mut self.cpu, self.clock.now() as usize)
        {
            cold_path();
            self.cpu.power_off()?;

            log::debug!("iCache hit for {} times.", self.cpu.icache_cnt);
            let rate = self.cpu.icache_cnt as f64 / self.clock.now() as f64;
            log::debug!("iCache hit rate {}", rate);

            self.status = BoardStatus::Halt;
        }

        Ok(())
    }
}

impl Board for VirtBoard {
    type ISA = RiscvTypes;

    fn step(&mut self) -> Result<(), Exception> {
        self.step_and_halt_if(&mut |_, _| false)
    }

    fn status(&self) -> BoardStatus {
        self.status
    }

    fn cpu_mut(&mut self) -> &mut RV32CPU {
        &mut self.cpu
    }
}
