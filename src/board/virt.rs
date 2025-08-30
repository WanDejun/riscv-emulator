use std::{
    cell::{RefCell, UnsafeCell},
    hint::cold_path,
    rc::Rc,
    sync::atomic::Ordering,
};

use crate::{
    board::{Board, BoardStatus},
    config::arch_config::WordType,
    device::{
        aclint::Clint,
        config::{Device, POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, UART_SIZE, UART1_ADDR},
        fast_uart::FastUart16550,
        mmio::{MemoryMapIO, MemoryMapItem},
        power_manager::{POWER_OFF_CODE, POWER_STATUS, PowerManager},
    },
    isa::riscv::{
        RiscvTypes,
        executor::RV32CPU,
        mmu::VirtAddrManager,
        trap::{Exception, Interrupt},
    },
    load::load_elf,
    ram::Ram,
    vclock::{Timer, VirtualClockRef},
};

pub trait IRQHandler {
    fn handle_irq(&mut self, id: u8, level: bool);
}

pub trait IRQSource {
    fn set_irq_line(&mut self, line: IRQLine, id: u8);
}

/// NOTE: Only used in single-threaded contexts.
pub struct IRQLine {
    target: *mut dyn IRQHandler,
    id: u8,
}

impl IRQLine {
    pub fn new(target: *mut dyn IRQHandler, id: u8) -> Self {
        Self { target, id }
    }

    pub fn set_irq(&mut self, level: bool) {
        unsafe { &mut *self.target }.handle_irq(self.id, level);
    }
}

pub struct VirtBoard {
    cpu: Box<RV32CPU>,
    clock: VirtualClockRef,
    timer: Rc<UnsafeCell<Timer>>,
    status: BoardStatus,
    clint: Rc<RefCell<Device>>,
}

impl VirtBoard {
    pub fn from_binary(bytes: &[u8]) -> Self {
        let mut ram = Ram::new();
        load_elf(&mut ram, bytes);
        Self::from_ram(ram)
    }

    pub fn from_ram(ram: Ram) -> Self {
        let clock = VirtualClockRef::new();
        let timer = Rc::new(UnsafeCell::new(Timer::new(clock.clone())));
        let ram_ref = Rc::new(UnsafeCell::new(ram));

        // Construct devices
        let uart1 = Rc::new(RefCell::new(Device::FastUart16550(FastUart16550::new())));
        let power_manager = Rc::new(RefCell::new(Device::PowerManager(PowerManager::new())));
        let clint = Rc::new(RefCell::new(Device::Clint(Clint::new(
            1,
            0x7ff8,
            0,
            clock.clone(),
            timer.clone(),
        ))));

        let mmio_items = vec![
            MemoryMapItem::new(POWER_MANAGER_ADDR, POWER_MANAGER_SIZE, power_manager),
            MemoryMapItem::new(UART1_ADDR, UART_SIZE, uart1),
            MemoryMapItem::new(0x200_0000, 0x10000, clint.clone()),
        ];

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), mmio_items);
        let vaddr_manager = VirtAddrManager::from_ram_and_mmio(ram_ref.clone(), mmio);

        let mut cpu = Box::new(RV32CPU::from_vaddr_manager(vaddr_manager));

        let timer_irq_line = IRQLine::new(
            &mut *cpu as *mut dyn IRQHandler,
            Into::<WordType>::into(Interrupt::MachineTimer) as u8,
        );

        let clint_clone = clint.clone();
        let clint_mut_ref = &mut *(clint_clone.borrow_mut());
        if let Device::Clint(clint_inner) = clint_mut_ref {
            clint_inner.set_irq_line(timer_irq_line, 0);
        }

        Self {
            cpu: cpu,
            clock: clock,
            timer: timer,
            status: BoardStatus::Running,
            clint: clint,
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

        unsafe { self.timer.as_mut_unchecked() }.tick();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::Mem;
    use crate::isa::DebugTarget;
    use crate::isa::riscv::csr_reg::csr_index;

    fn create_test_board() -> VirtBoard {
        let mut ram = Ram::new();
        for i in 0..=0x10000 {
            ram.write::<u32>(4 * i, 0x13).unwrap(); // NOP
        }

        VirtBoard::from_ram(ram)
    }

    #[test]
    fn test_clint_mmio_access() {
        let board = create_test_board();

        // 直接测试 CLINT 设备
        let mut clint_borrowed = board.clint.borrow_mut();
        if let Device::Clint(clint) = &mut *clint_borrowed {
            // 测试 mtime 读取
            let initial_time: u64 = clint.read(0x7ff8).unwrap_or(0);
            println!("Initial mtime: {:#x}", initial_time);

            // 测试 mtime 写入
            let test_time = 0x123456789abcdef0u64;
            let write_result = clint.write(0x7ff8, test_time);
            assert!(
                write_result.is_ok(),
                "Failed to write to mtime: {:?}",
                write_result
            );

            // 验证写入后的读取
            let read_time: u64 = clint.read(0x7ff8).unwrap();
            assert_eq!(read_time, test_time, "mtime write/read mismatch");

            // 测试 mtimecmp 访问 (mtimecmp_base = 0)
            let timecmp_value = 0xfedcba9876543210u64;
            let write_result = clint.write(0x0, timecmp_value);
            assert!(
                write_result.is_ok(),
                "Failed to write to mtimecmp: {:?}",
                write_result
            );

            let read_timecmp: u64 = clint.read(0x0).unwrap();
            assert_eq!(read_timecmp, timecmp_value, "mtimecmp write/read mismatch");

            println!("CLINT MMIO access test passed!");
        } else {
            panic!("CLINT device not found");
        }
    }

    #[test]
    fn test_clint_timer_interrupt() {
        let mut board = create_test_board();

        let interrupt_handler_addr = 0x1000;
        board
            .cpu_mut()
            .debug_csr(csr_index::mtvec, Some(interrupt_handler_addr));

        // Enable MIE in mstatus
        board.cpu_mut().debug_csr(csr_index::mstatus, Some(1 << 3));

        // Enable MTIE
        board.cpu_mut().debug_csr(csr_index::mie, Some(1 << 7));

        let target_time = 5;
        {
            let mut clint_borrowed = board.clint.borrow_mut();
            if let Device::Clint(clint) = &mut *clint_borrowed {
                clint.write(0x0, target_time).unwrap();
            }
        }

        println!("Running board steps to test timer interrupt...");

        let mut reach_mtvec = false;
        for i in 0..20 {
            assert!(board.step().is_ok());

            let pc = board.cpu_mut().read_pc();

            if pc == interrupt_handler_addr {
                println!("PC jumped to interrupt handler at step {}!", i);
                reach_mtvec = true;
                break;
            }
        }

        assert!(reach_mtvec);
        assert_eq!(
            board.cpu_mut().debug_csr(csr_index::mip, None),
            Some(1 << 7)
        );
        assert!(board.clock.now() >= target_time);
    }
}
