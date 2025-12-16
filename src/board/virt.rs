use std::{
    any::TypeId,
    cell::{RefCell, UnsafeCell},
    collections::HashMap,
    hint::cold_path,
    rc::Rc,
    sync::atomic::Ordering,
};

use crate::{
    DeviceConfig, EMULATOR_CONFIG,
    board::{Board, BoardStatus},
    device::{
        self, DeviceTrait, IdAllocator,
        aclint::Clint,
        config::{
            CLINT_BASE, CLINT_SIZE, PLIC_BASE, PLIC_SIZE, POWER_MANAGER_BASE, POWER_MANAGER_SIZE,
        },
        fast_uart::FastUart16550,
        mmio::{MemoryMapIO, MemoryMapItem},
        plic::{
            PLIC,
            irq_line::{PlicIRQLine, PlicIRQSource},
        },
        power_manager::{POWER_OFF_CODE, POWER_STATUS, PowerManager},
        test_device::TestDevice,
        virtio::{
            virtio_blk::VirtIOBlkDeviceBuilder,
            virtio_mmio::{VirtIODeviceID, VirtIOMMIO},
        },
    },
    device_poller::DevicePoller,
    emulator_panic,
    isa::riscv::{
        RiscvTypes,
        executor::RVCPU,
        mmu::VirtAddrManager,
        trap::{Exception, Interrupt},
    },
    load::{load_bin, load_elf},
    ram::Ram,
    vclock::{Timer, VirtualClockRef},
};

pub trait RiscvIRQHandler {
    fn handle_irq(&mut self, interrupt: Interrupt, level: bool);
}

pub trait RiscvIRQSource {
    fn set_irq_line(&mut self, line: IRQLine, id: usize);
}

/// NOTE: Only used in single-threaded contexts.
pub struct IRQLine {
    target: *mut dyn RiscvIRQHandler,
    interrupt_nr: Interrupt,
}

impl IRQLine {
    pub fn new(target: *mut dyn RiscvIRQHandler, interrupt_nr: Interrupt) -> Self {
        Self {
            target,
            interrupt_nr,
        }
    }

    pub fn set_irq(&mut self, level: bool) {
        unsafe { &mut *self.target }.handle_irq(self.interrupt_nr, level);
    }
}

const PLIC_FREQUENCY_DIVISION: usize = 128;

pub struct RVBoardBuilder {
    extra_plic_devices: Vec<Rc<RefCell<dyn DeviceTrait>>>,
    virtio_devices: Vec<DeviceConfig>,
    mmio_items: Vec<MemoryMapItem>,
    id_allocators: HashMap<TypeId, IdAllocator>,
    device_poller: DevicePoller,
}

impl RVBoardBuilder {
    pub fn new() -> Self {
        Self {
            extra_plic_devices: Vec::new(),
            virtio_devices: Vec::new(),
            mmio_items: Vec::new(),
            id_allocators: HashMap::new(),
            device_poller: DevicePoller::new(),
        }
    }

    pub fn add_plic_device<D: device::MemMappedDeviceTrait + 'static>(
        mut self,
        device: Rc<RefCell<D>>,
    ) -> Self {
        let type_id = TypeId::of::<D>();
        let allocator = self
            .id_allocators
            .entry(type_id)
            .or_insert_with(|| device::IdAllocator::new::<D>(0, stringify!(D).to_string()));

        let info = allocator.get();
        self.mmio_items
            .push(MemoryMapItem::new(info.base, info.size, device.clone()));

        if let Some(event) = device.borrow_mut().get_poll_event() {
            self.device_poller.add_event(event);
        }

        self.extra_plic_devices.push(device);

        self
    }

    pub fn add_virtio_devices(mut self, devices: &mut Vec<DeviceConfig>) -> Self {
        self.virtio_devices.append(devices);
        self
    }

    pub fn build(mut self, ram: Ram) -> VirtBoard {
        let clock = VirtualClockRef::new();
        let timer = Rc::new(UnsafeCell::new(Timer::new(clock.clone())));
        let ram_ref = Rc::new(UnsafeCell::new(ram));

        // Construct devices
        let uart1 = Rc::new(RefCell::new(FastUart16550::new()));
        self = self.add_plic_device(uart1);

        let power_manager = Rc::new(RefCell::new(PowerManager::new()));
        let clint = Rc::new(RefCell::new(Clint::new(
            1,
            0,
            0x7ff8,
            0x4000,
            clock.clone(),
            timer.clone(),
        )));

        // PLIC init.
        let plic = Rc::new(RefCell::new(PLIC::new()));
        let poller_plic_irq_line = PlicIRQLine::new(&mut *plic.borrow_mut());
        self.device_poller.set_irq_line(poller_plic_irq_line, 0);

        self.mmio_items.append(&mut vec![
            MemoryMapItem::new(POWER_MANAGER_BASE, POWER_MANAGER_SIZE, power_manager),
            MemoryMapItem::new(CLINT_BASE, CLINT_SIZE, clint.clone()),
            MemoryMapItem::new(PLIC_BASE, PLIC_SIZE, plic.clone()),
        ]);

        // Add VirtIO device.
        let mut virtio_allocator =
            device::IdAllocator::new::<VirtIOMMIO>(0, String::from("virtio"));
        for virtio_device_cfg in self.virtio_devices.iter() {
            let virtio_device = match virtio_device_cfg.dev_type {
                VirtIODeviceID::Block => {
                    let ram_raw_base = unsafe { &mut ram_ref.as_mut_unchecked()[0] as *mut u8 };
                    VirtIOBlkDeviceBuilder::new(
                        ram_raw_base,
                        String::from(virtio_device_cfg.path.to_str().unwrap()),
                    )
                    .host_feature(crate::device::virtio::virtio_blk::VirtIOBlockFeature::BlockSize)
                    .get()
                }
                dev_type => {
                    emulator_panic!("unsupport device: {:#?}", dev_type);
                }
            };
            let virtio_mmio_device = VirtIOMMIO::new(Box::new(UnsafeCell::new(virtio_device)));
            let virtio_info = virtio_allocator.get();
            self.mmio_items.push(MemoryMapItem::new(
                virtio_info.base,
                virtio_info.size,
                Rc::new(RefCell::new(virtio_mmio_device)),
            ));
        }

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), self.mmio_items);
        let vaddr_manager = VirtAddrManager::from_ram_and_mmio(ram_ref.clone(), mmio);

        let mut cpu = Box::new(RVCPU::from_vaddr_manager(vaddr_manager));

        // register irq line for timer.
        clint.borrow_mut().set_irq_line(
            IRQLine::new(
                &mut *cpu as *mut dyn RiscvIRQHandler,
                Interrupt::MachineTimer,
            ),
            0,
        );
        clint.borrow_mut().set_irq_line(
            IRQLine::new(
                &mut *cpu as *mut dyn RiscvIRQHandler,
                Interrupt::MachineSoft,
            ),
            1,
        );

        // register irq line for plic.
        let plic_mathine_irq_line = IRQLine::new(
            &mut *cpu as *mut dyn RiscvIRQHandler,
            Interrupt::MachineExternal,
        );
        let plic_supervisor_irq_line = IRQLine::new(
            &mut *cpu as *mut dyn RiscvIRQHandler,
            Interrupt::SupervisorExternal,
        );

        plic.borrow_mut().set_irq_line(plic_mathine_irq_line, 0);
        plic.borrow_mut().set_irq_line(plic_supervisor_irq_line, 1);

        VirtBoard {
            cpu,
            clock,
            timer,

            device_poller: self.device_poller.start_polling(),
            clint,
            plic,
            plic_freq_counter: 0,

            status: BoardStatus::Running,
        }
    }
}

pub struct VirtBoard {
    pub cpu: Box<RVCPU>,
    pub clock: VirtualClockRef,
    pub timer: Rc<UnsafeCell<Timer>>,

    // interrupt manager.
    pub clint: Rc<RefCell<Clint>>,
    pub plic: Rc<RefCell<PLIC>>,
    pub plic_freq_counter: usize,
    pub device_poller: DevicePoller,

    status: BoardStatus,
}

impl VirtBoard {
    pub fn from_binary(bytes: &[u8]) -> Self {
        let mut ram = Ram::new();
        load_bin(&mut ram, bytes);
        Self::from_ram(ram)
    }

    pub fn from_elf(bytes: &[u8]) -> Self {
        let mut ram = Ram::new();
        load_elf(&mut ram, bytes);
        Self::from_ram(ram)
    }

    pub fn from_ram(ram: Ram) -> Self {
        let mut builder =
            RVBoardBuilder::new().add_virtio_devices(&mut EMULATOR_CONFIG.lock().unwrap().devices);

        #[cfg(feature = "test-device")]
        {
            builder = builder.add_plic_device(Rc::new(RefCell::new(TestDevice::new())));
        }

        builder.build(ram)
    }
}

impl Board for VirtBoard {
    type ISA = RiscvTypes;

    fn step(&mut self) -> Result<(), Exception> {
        self.plic_freq_counter += 1;
        if self.plic_freq_counter >= PLIC_FREQUENCY_DIVISION {
            self.plic_freq_counter = 0;

            // TODO: use external irq lines to trigger plic interrupts.
            self.device_poller.trigger_external_interrupt();

            self.plic.borrow_mut().try_get_interrupt(0);
            self.plic.borrow_mut().try_get_interrupt(1);
        }
        self.cpu.step()?;
        self.clock.advance(1);

        if self.clock.now() % 32 == 0 && POWER_STATUS.load(Ordering::Acquire).eq(&POWER_OFF_CODE) {
            cold_path();
            self.cpu.power_off()?;

            log::info!("iCache hit for {} times.", self.cpu.icache_cnt);
            let rate = self.cpu.icache_cnt as f64 / self.clock.now() as f64;
            log::info!("iCache hit rate {}", rate);

            self.status = BoardStatus::Halt;

            log::info!("Total cycles: {}", self.clock.now());
        }

        unsafe { self.timer.as_mut_unchecked() }.tick();

        Ok(())
    }

    fn status(&self) -> BoardStatus {
        self.status
    }

    fn cpu_mut(&mut self) -> &mut RVCPU {
        &mut self.cpu
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::arch_config::XLEN;
    use crate::isa::DebugTarget;
    use crate::isa::riscv::csr_reg::csr_macro::Mcause;
    use crate::isa::riscv::csr_reg::{NamedCsrReg, csr_index};
    use crate::ram_config;

    fn create_test_board() -> VirtBoard {
        let mut ram = Ram::new();
        for i in 0..=0x100000 {
            ram.write::<u32>(4 * i, 0x13).unwrap(); // NOP
        }

        let mut board = VirtBoard::from_ram(ram);
        board.cpu.debug_csr(csr_index::mtvec, Some(0x8000_2000));
        board
    }

    #[test]
    fn test_clint_mmio_access() {
        let board = create_test_board();

        // 直接测试 CLINT 设备
        let mut clint = board.clint.borrow_mut();
        // 测试 mtime 读取
        let _ = clint.read_u64(0x7ff8).unwrap();

        // 测试 mtime 写入
        let test_time = 0x123456789abcdef0u64;
        let write_result = clint.write_u64(0x7ff8, test_time);
        assert!(
            write_result.is_ok(),
            "Failed to write to mtime: {:?}",
            write_result
        );

        // 验证写入后的读取
        let read_time: u64 = clint.read_u64(0x7ff8).unwrap();
        assert_eq!(read_time, test_time, "mtime write/read mismatch");

        // 测试 mtimecmp 访问
        let timecmp_value = 0xfedcba9876543210u64;
        let write_result = clint.write_u64(0x4000, timecmp_value);
        assert!(
            write_result.is_ok(),
            "Failed to write to mtimecmp: {:?}",
            write_result
        );

        let read_timecmp: u64 = clint.read_u64(0x4000).unwrap();
        assert_eq!(read_timecmp, timecmp_value, "mtimecmp write/read mismatch");
    }

    #[test]
    fn test_clint_timer_interrupt() {
        let mut board = create_test_board();

        let interrupt_handler_addr = ram_config::BASE_ADDR + 0x1000;
        board
            .cpu_mut()
            .debug_csr(csr_index::mtvec, Some(interrupt_handler_addr));

        // Enable MIE in mstatus
        board.cpu_mut().debug_csr(csr_index::mstatus, Some(1 << 3));

        // Enable MTIE
        board.cpu_mut().debug_csr(csr_index::mie, Some(1 << 7));

        let target_time = 5;
        {
            let mut clint = board.clint.borrow_mut();
            clint.write_u64(0x4000, target_time).unwrap();
        }

        println!("Running board steps to test timer interrupt...");

        let mut reach_mtvec = false;
        for i in 0..128 {
            board.step().unwrap();

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

        // Test MSIP (software interrupt)
        board.cpu_mut().write_pc(ram_config::BASE_ADDR);

        // Re-enable MIE in mstatus
        board.cpu_mut().debug_csr(csr_index::mstatus, Some(1 << 3));

        // Disable MTIE and enable MSIE
        board.cpu_mut().debug_csr(csr_index::mie, Some(1 << 3));

        {
            let mut clint = board.clint.borrow_mut();
            clint.write_u64(0x0, 1).unwrap();
        }

        board.step().unwrap();
        assert!(board.cpu_mut().read_pc() == interrupt_handler_addr);

        let mcause = board
            .cpu_mut()
            .debug_csr(Mcause::get_index(), None)
            .unwrap();
        assert_eq!(mcause, (1u64 << (XLEN - 1)) | 0b11)
    }

    #[cfg(feature = "test-device")]
    #[test]
    fn test_plic() {
        use std::{thread::sleep, time::Duration};

        use crate::device::config::TEST_DEVICE_BASE;
        use crate::device::test_device::TEST_DEVICE_INTERRUPT_ID;
        use crate::ram_config;
        use crate::{config::arch_config::WordType, isa::riscv::debugger::Address};
        const PRIORITY_OFFSET: WordType = 0;
        const PENDING_BIT_OFFSET: WordType = 0x001000;
        const CONTEXT_ENABLE_BIT_OFFSET: WordType = 0x002000;
        const CONTEXT_ENABLE_BIT_SIZE: WordType = 0x80;
        const CONTEXT_CONFIG_OFFSET: WordType = 0x200000;
        const CONTEXT_CONFIG_SIZE: WordType = 0x1000;

        let mut board = create_test_board();
        let mstatus = board.cpu.debug_csr(csr_index::mstatus, None).unwrap() | 1 << 3; // enable MIE
        board.cpu.debug_csr(csr_index::mstatus, Some(mstatus));
        board.cpu.debug_csr(csr_index::mie, Some(1 << 11)); // enable MEIE

        {
            let mut plic = board.plic.borrow_mut();
            // priority_threshold
            let addr = CONTEXT_CONFIG_OFFSET + (0 * CONTEXT_CONFIG_SIZE);
            plic.write_u32(addr, 1).unwrap();

            // test_device interrupt priority
            plic.write_u32(TEST_DEVICE_INTERRUPT_ID as WordType * 4, 5)
                .unwrap();

            // interrupt enable.
            let addr = CONTEXT_ENABLE_BIT_OFFSET + (0 * CONTEXT_ENABLE_BIT_SIZE) + 4;
            plic.write_u32(addr, 0xffffffff).unwrap();
        }

        // data register 0
        board
            .cpu
            .write_memory(
                Address::Phys(TEST_DEVICE_BASE + 2 * size_of::<u32>() as WordType),
                100_000u32,
            )
            .unwrap();
        // data register 1
        board
            .cpu
            .write_memory(
                Address::Phys(TEST_DEVICE_BASE + 3 * size_of::<u32>() as WordType),
                0u32,
            )
            .unwrap();
        board
            .cpu
            .write_memory(
                Address::Phys(TEST_DEVICE_BASE + 1 * size_of::<u32>() as WordType),
                1u32,
            )
            .unwrap();
        sleep(Duration::from_millis(20));

        for _ in 0..200 {
            assert!(board.step().is_ok());
        }

        let meip = 1 << 11;
        assert_eq!(board.cpu.debug_csr(csr_index::mip, None).unwrap(), meip);

        // let mecause: WordType = Trap::Interrupt(Interrupt::MachineExternal).into();
        // assert_eq!(
        //     board.cpu.debug_csr(csr_index::mcause, None).unwrap(),
        //     mecause
        // );

        let addr = CONTEXT_CONFIG_OFFSET + (0 * CONTEXT_CONFIG_SIZE) + 4;
        let mut plic = board.plic.borrow_mut();
        let claimed_id = plic.read_u32(addr).unwrap();
        assert_eq!(claimed_id as u32, TEST_DEVICE_INTERRUPT_ID);

        let mepc = board.cpu.debug_csr(csr_index::mepc, None).unwrap();
        assert!(mepc >= ram_config::BASE_ADDR);
    }
}
