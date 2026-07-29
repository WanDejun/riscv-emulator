use std::{
    any::TypeId,
    cell::{Cell, RefCell, UnsafeCell},
    collections::HashMap,
    hint::cold_path,
    rc::Rc,
    sync::atomic::Ordering,
};

use crate::{
    DeviceConfig,
    background::BackgroundExecutor,
    board::{Board, BoardStatus, VirtBoardPlicContextId},
    byte_io::{ByteSinkExt, ByteSource},
    clock::{Timer, VirtualClock},
    config::arch_config::WordType,
    device::{
        self, IdAllocator,
        aclint::Clint,
        config::{
            CLINT_BASE, CLINT_SIZE, PLIC_BASE, PLIC_SIZE, POWER_MANAGER_BASE, POWER_MANAGER_SIZE,
            UART_IRQ, VIRTIO_IRQ_BASE,
        },
        fast_uart::{FastUart16550, UartBytePort},
        mmio::{MemoryMapIO, MemoryMapItem},
        plic::{PLIC, PeriphIrqId, irq_line::PlicIRQSource},
        power_manager::{POWER_OFF_CODE, POWER_STATUS, PowerManager},
        virtio::{
            virtio_blk::VirtIOBlkDeviceBuilder, virtio_device::VirtIODeviceEnum,
            virtio_mmio::VirtIOMMIO,
        },
    },
    isa::{
        DebugTarget,
        riscv::{
            decoder::Decoder,
            executor::{BatchResult, ExecutionHook, RVCPU},
            mmu::VirtAddrManager,
            trap::Interrupt,
        },
    },
    load::{ELFLoader, load_bin},
    ram::Ram,
    ram_config,
    task_spawner::TaskSpawner,
};

#[cfg(feature = "test-device")]
use crate::device::sample_timer::{SAMPLE_TIMER_INTERRUPT_ID, SampleTimerDevice};

pub trait RiscvIRQHandler {
    fn handle_irq(&mut self, interrupt: Interrupt, level: bool);
}

pub trait RiscvIRQSource {
    fn set_irq_line(&mut self, line: IRQLine, id: usize);
}

#[derive(Debug)]
pub struct MemoryImage {
    pub address: WordType,
    pub data: Vec<u8>,
}

impl MemoryImage {
    pub fn new(address: WordType, data: Vec<u8>) -> Self {
        Self { address, data }
    }
}

#[derive(Default)]
pub struct VirtBoardConfig {
    decoder: Option<Decoder>,
    virtio_devices: Vec<DeviceConfig>,
    memory_images: Vec<MemoryImage>,
    initial_registers: Vec<(u8, WordType)>,
}

impl VirtBoardConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_decoder(mut self, decoder: Decoder) -> Self {
        self.decoder = Some(decoder);
        self
    }

    pub fn with_virtio_devices(mut self, devices: Vec<DeviceConfig>) -> Self {
        self.virtio_devices = devices;
        self
    }

    /// Load an additional image into guest physical RAM **after** the primary ELF/binary is loaded.
    pub fn with_memory_image(mut self, image: MemoryImage) -> Self {
        self.memory_images.push(image);
        self
    }

    pub fn with_reg(mut self, register: u8, value: WordType) -> Self {
        self.initial_registers.push((register, value));
        self
    }
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

pub struct RVBoardBuilder {
    extra_plic_devices: Vec<(Rc<RefCell<dyn PlicIRQSource>>, PeriphIrqId)>,
    virtio_devices: Vec<DeviceConfig>,
    mmio_items: Vec<MemoryMapItem>,
    id_allocators: HashMap<TypeId, IdAllocator>,
    background: BackgroundExecutor,
    decoder: Option<Decoder>,
    initial_registers: Vec<(u8, WordType)>,
    spawner: TaskSpawner,
}

impl RVBoardBuilder {
    pub fn new() -> Self {
        Self {
            extra_plic_devices: Vec::new(),
            virtio_devices: Vec::new(),
            mmio_items: Vec::new(),
            id_allocators: HashMap::new(),
            background: BackgroundExecutor::new(),
            decoder: None,
            initial_registers: Vec::new(),
            #[cfg(not(target_arch = "wasm32"))]
            spawner: TaskSpawner::new(),
        }
    }

    pub fn get_spawner(&self) -> TaskSpawner {
        self.spawner.clone()
    }

    pub fn with_decoder(mut self, decoder: Decoder) -> Self {
        self.decoder = Some(decoder);
        self
    }

    pub fn add_plic_device<D: device::MemMappedDeviceTrait + PlicIRQSource + 'static>(
        mut self,
        device: Rc<RefCell<D>>,
        interrupt_id: PeriphIrqId,
    ) -> Self {
        let type_id = TypeId::of::<D>();
        let allocator = self
            .id_allocators
            .entry(type_id)
            .or_insert_with(|| device::IdAllocator::new::<D>(0, stringify!(D).to_string()));

        let info = allocator.get();
        self.mmio_items
            .push(MemoryMapItem::new(info.base, info.size, device.clone()));

        self.extra_plic_devices.push((device, interrupt_id));

        self
    }

    pub fn add_virtio_devices(mut self, devices: Vec<DeviceConfig>) -> Self {
        self.virtio_devices.extend(devices);
        self
    }

    pub fn with_initial_registers(mut self, registers: Vec<(u8, WordType)>) -> Self {
        self.initial_registers = registers;
        self
    }

    pub fn build(mut self, ram: Ram) -> VirtBoard {
        let cycles = Rc::new(Cell::new(0));
        let clock = VirtualClock::new(cycles.clone());
        let timer = Rc::new(UnsafeCell::new(Timer::new(clock.clone())));
        let ram_ref = Rc::new(UnsafeCell::new(ram));

        // Construct devices
        let (uart1, uart_port1) = FastUart16550::new();
        let uart1 = Rc::new(RefCell::new(uart1));
        self = self.add_plic_device(uart1, UART_IRQ);

        #[cfg(feature = "native-cli")]
        {
            use crate::byte_io::ByteSource;
            use std::io::IsTerminal;

            // TODO: make this configurable
            // uart <-> std I/O
            use crate::byte_io::TerminalIOContext;

            let mut ctx = TerminalIOContext::new();
            let mut uart_port1 = uart_port1.clone();

            let input_term = std::io::stdin().is_terminal();

            self.background.add_polling_task(move || {
                let mut progress: bool = false;
                // stdin -> uart
                if input_term {
                    progress |= ctx.drain_to(&mut uart_port1);
                }

                // uart -> stdout
                progress |= uart_port1.drain_to(&mut ctx);

                progress
            });
        }

        const MTIME_OFFSET: u64 = 0xbff8;
        const MTIMECMP_OFFSET: u64 = 0x4000;

        let power_manager = Rc::new(RefCell::new(PowerManager::new()));
        let clint = Rc::new(RefCell::new(Clint::new(
            1,
            0,
            MTIME_OFFSET,
            MTIMECMP_OFFSET,
            clock.clone(),
            timer.clone(),
        )));

        // PLIC init.
        let plic = Rc::new(RefCell::new(PLIC::new()));
        for (plic_device, interrupt_id) in self.extra_plic_devices {
            plic_device
                .borrow_mut()
                .set_irq_line(&mut *plic.borrow_mut(), interrupt_id);
        }

        self.mmio_items.append(&mut vec![
            MemoryMapItem::new(POWER_MANAGER_BASE, POWER_MANAGER_SIZE, power_manager),
            MemoryMapItem::new(CLINT_BASE, CLINT_SIZE, clint.clone()),
            MemoryMapItem::new(PLIC_BASE, PLIC_SIZE, plic.clone()),
        ]);

        // Add VirtIO device.
        let mut virtio_allocator =
            device::IdAllocator::new::<VirtIOMMIO>(0, String::from("virtio"));
        for (virtio_index, virtio_device_cfg) in self.virtio_devices.iter().enumerate() {
            let virtio_device = match virtio_device_cfg.dev_type {
                VirtIODeviceEnum::VirtIOBlock => {
                    let ram_base = unsafe { &mut ram_ref.as_mut_unchecked()[0] as *mut u8 };
                    VirtIOBlkDeviceBuilder::new(
                        ram_base,
                        String::from(virtio_device_cfg.path.to_str().unwrap()),
                    )
                    .host_feature(crate::device::virtio::virtio_blk::VirtIOBlockFeature::BlockSize)
                    .host_feature(crate::device::virtio::virtio_blk::VirtIOBlockFeature::Flush)
                    .get()
                }
                dev_type => {
                    panic!("unsupport device: {:#?}", dev_type);
                }
            };
            let mut virtio_mmio_device = VirtIOMMIO::new(Box::new(UnsafeCell::new(virtio_device)));
            virtio_mmio_device.set_irq_line(
                &mut *plic.borrow_mut(),
                VIRTIO_IRQ_BASE + virtio_index as u32,
            );
            let virtio_info = virtio_allocator.get();
            self.mmio_items.push(MemoryMapItem::new(
                virtio_info.base,
                virtio_info.size,
                Rc::new(RefCell::new(virtio_mmio_device)),
            ));
        }

        let mmio = MemoryMapIO::from_mmio_items(ram_ref.clone(), self.mmio_items);
        let vaddr_manager = VirtAddrManager::from_ram_and_mmio(ram_ref.clone(), mmio);

        let decoder = self.decoder.take().unwrap_or_else(Decoder::new);
        let mut cpu = Box::new(RVCPU::from_decoder(decoder, vaddr_manager));

        for (register, value) in self.initial_registers {
            cpu.write_reg(register, value);
        }

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

        cpu.time_addr = Some(CLINT_BASE + MTIME_OFFSET);

        // register irq line for plic.
        let plic_machine_irq_line = IRQLine::new(
            &mut *cpu as *mut dyn RiscvIRQHandler,
            Interrupt::MachineExternal,
        );
        let plic_supervisor_irq_line = IRQLine::new(
            &mut *cpu as *mut dyn RiscvIRQHandler,
            Interrupt::SupervisorExternal,
        );

        plic.borrow_mut().set_irq_line(
            plic_machine_irq_line,
            VirtBoardPlicContextId::Cpu0MachineMode.into(),
        );
        plic.borrow_mut().set_irq_line(
            plic_supervisor_irq_line,
            VirtBoardPlicContextId::Cpu0SuperviserMode.into(),
        );

        // Hand device background work to the shared executor and start its worker thread.
        let mut background = self.background;
        background.start();

        VirtBoard {
            background,
            loader: None,
            cpu,
            cycles,
            clock,
            timer,

            clint,
            plic,
            uart_port: uart_port1,

            status: BoardStatus::Running,
        }
    }
}

pub struct VirtBoard {
    // Background threads must stop before the devices they touch are dropped, so this is
    // the first field (in rust, "fields of a struct are dropped in declaration order").
    pub background: BackgroundExecutor,

    loader: Option<ELFLoader>,

    pub cpu: Box<RVCPU>,
    cycles: Rc<Cell<u64>>,
    pub clock: VirtualClock,
    pub timer: Rc<UnsafeCell<Timer<VirtualClock>>>,

    // interrupt manager.
    pub clint: Rc<RefCell<Clint>>,
    pub plic: Rc<RefCell<PLIC>>,
    pub uart_port: UartBytePort,

    status: BoardStatus,
}

const STEP_BATCH_CYCLES: u64 = 1024;

impl VirtBoard {
    pub fn from_binary_with(bytes: &[u8], config: VirtBoardConfig) -> Result<Self, String> {
        let mut ram = Ram::new();
        load_bin(&mut ram, bytes);
        Self::from_ram_with(ram, config)
    }

    pub fn from_elf(bytes: Vec<u8>) -> Result<Self, String> {
        Self::from_elf_with(bytes, VirtBoardConfig::new())
    }

    pub fn from_elf_with(bytes: Vec<u8>, config: VirtBoardConfig) -> Result<Self, String> {
        let mut ram = Ram::new();
        let loader = ELFLoader::try_new(bytes).ok_or_else(|| "Invalid ELF file".to_string())?;
        loader.load_to_ram(&mut ram);
        let mut board = Self::from_ram_with(ram, config)?;
        board.loader = Some(loader);
        Ok(board)
    }

    pub fn from_ram_with(mut ram: Ram, config: VirtBoardConfig) -> Result<Self, String> {
        let VirtBoardConfig {
            decoder,
            virtio_devices,
            memory_images,
            initial_registers,
        } = config;

        for image in memory_images {
            let offset = image
                .address
                .checked_sub(ram_config::BASE_ADDR)
                .ok_or_else(|| {
                    format!(
                        "memory image address 0x{:x} is below RAM base 0x{:x}",
                        image.address,
                        ram_config::BASE_ADDR
                    )
                })?;
            ram.try_insert_section(&image.data, offset)
                .map_err(|error| {
                    format!(
                        "failed to load memory image at 0x{:x}: {error}",
                        image.address
                    )
                })?;
        }

        let mut builder = RVBoardBuilder::new();

        if let Some(decoder) = decoder {
            builder = builder.with_decoder(decoder);
        }

        builder = builder
            .add_virtio_devices(virtio_devices)
            .with_initial_registers(initial_registers);

        #[cfg(feature = "test-device")]
        {
            let spawner = builder.get_spawner();
            builder = builder.add_plic_device(
                Rc::new(RefCell::new(SampleTimerDevice::new(spawner))),
                SAMPLE_TIMER_INTERRUPT_ID,
            );
        }

        Ok(builder.build(ram))
    }

    pub fn push_uart_input(&mut self, bytes: &[u8]) {
        self.uart_port.receive(bytes);
    }

    pub fn take_uart_output(&mut self) -> Vec<u8> {
        let mut vec = Vec::new();
        self.uart_port.drain_to(&mut vec);
        vec
    }

    pub fn uart_port(&self) -> UartBytePort {
        self.uart_port.clone()
    }

    pub fn cycles(&self) -> u64 {
        self.cycles.get()
    }

    fn prepare_cpu_batch(&mut self) {
        unsafe { self.timer.as_mut_unchecked() }.tick();
        self.background.poll_if_single_thread_mode();
        self.plic.borrow_mut().update_context_irq_lines(&[
            VirtBoardPlicContextId::Cpu0MachineMode.into(),
            VirtBoardPlicContextId::Cpu0SuperviserMode.into(),
        ]);
    }

    fn finish_cpu_batch(&mut self, cycles: u64) {
        self.cycles.set(self.cycles.get().wrapping_add(cycles));
        // TODO: We can simply read from `PowerManager` if VirtBoard owns `PowerManager`.
        if POWER_STATUS.load(Ordering::Acquire).eq(&POWER_OFF_CODE) {
            cold_path();
            self.cpu.power_off();
            self.status = BoardStatus::Halt;
            log::info!("Total cycles: {}", self.cycles());
        }
    }

    #[inline]
    fn step_batch_with_hook<H: ExecutionHook>(&mut self, cycles: u64, hook: &mut H) -> BatchResult {
        if self.status != BoardStatus::Running {
            return BatchResult {
                cycles: 0,
                hook_stopped: false,
            };
        }

        self.prepare_cpu_batch();
        let result = self.cpu.step_batch_with_hook(cycles, hook);
        self.finish_cpu_batch(result.cycles);
        result
    }
}

impl Board for VirtBoard {
    fn step_cycles_with_hook<H: ExecutionHook>(
        &mut self,
        cycles: u64,
        hook: &mut H,
    ) -> BatchResult {
        let initial_cycles = self.cycles();
        let target_cycles = self.cycles() + cycles;

        while self.cycles() < target_cycles && self.status == BoardStatus::Running {
            let batch_cycles = (target_cycles - self.cycles()).min(STEP_BATCH_CYCLES);
            let result = self.step_batch_with_hook(batch_cycles, hook);

            if result.hook_stopped {
                return BatchResult {
                    cycles: self.cycles().wrapping_sub(initial_cycles),
                    hook_stopped: true,
                };
            }
        }

        BatchResult {
            cycles: self.cycles().wrapping_sub(initial_cycles),
            hook_stopped: false,
        }
    }

    fn status(&self) -> BoardStatus {
        self.status
    }

    fn cpu(&self) -> &RVCPU {
        &self.cpu
    }

    fn cpu_mut(&mut self) -> &mut RVCPU {
        &mut self.cpu
    }

    fn loader(&self) -> Option<&crate::load::ELFLoader> {
        self.loader.as_ref()
    }

    fn pause_background_work(&mut self) {
        self.background.pause_and_wait();
    }

    fn resume_background_work(&mut self) {
        self.background.resume();
    }

    fn take_uart_output(&mut self) -> Vec<u8> {
        let mut vec = Vec::new();
        self.uart_port.drain_to(&mut vec);
        vec
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use crate::config::arch_config::XLEN;
    use crate::device::DeviceTrait;
    use crate::isa::DebugTarget;
    use crate::isa::riscv::csr_reg::csr_macro::Mcause;
    use crate::isa::riscv::csr_reg::{NamedCsrReg, csr_index};
    use crate::ram_config;

    fn create_test_board() -> VirtBoard {
        let mut ram = Ram::new();
        for i in 0..=0x100000 {
            ram.write::<u32>(4 * i, 0x13).unwrap(); // NOP
        }

        let mut board = VirtBoard::from_ram_with(ram, VirtBoardConfig::new()).unwrap();
        board.cpu.debug_csr(csr_index::mtvec, Some(0x8000_2000));
        board
    }

    #[test]
    fn test_step_cycles_advances_board_clock() {
        let mut board = create_test_board();
        let requested = STEP_BATCH_CYCLES + 3;

        let executed = board.step_cycles(requested);

        assert_eq!(executed, requested);
        assert_eq!(board.cycles(), requested);
        assert_eq!(board.clock.now(), requested >> 3);
        assert_eq!(board.cpu.read_pc(), ram_config::BASE_ADDR + requested * 4);
    }

    #[test]
    fn test_memory_image_and_initial_register_config() {
        use crate::isa::riscv::debugger::Address;

        let image_address = ram_config::BASE_ADDR + 0x2000;
        let image_offset = image_address - ram_config::BASE_ADDR;
        let image = vec![0xd0, 0x0d, 0xfe, 0xed];
        let original = vec![0xaa; image.len()];
        let mut ram = Ram::new();
        ram.try_insert_section(&original, image_offset).unwrap();

        let config = VirtBoardConfig::new()
            .with_memory_image(MemoryImage::new(image_address, image.clone()))
            .with_reg(11, image_address);

        let mut board = VirtBoard::from_ram_with(ram, config).unwrap();

        assert_eq!(board.cpu.read_reg(11), image_address);
        for (offset, expected) in image.into_iter().enumerate() {
            assert_eq!(
                board
                    .cpu
                    .read_memory::<u8>(Address::Phys(image_address as u64 + offset as u64))
                    .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn test_clint_mmio_access() {
        let board = create_test_board();

        // 直接测试 CLINT 设备
        let mut clint = board.clint.borrow_mut();
        // 测试 mtime 读取
        let _ = clint.read_u64(0xbff8).unwrap();

        // 测试 mtime 写入
        let test_time = 0x123456789abcdef0u64;
        let write_result = clint.write_u64(0xbff8, test_time);
        assert!(
            write_result.is_ok(),
            "Failed to write to mtime: {:?}",
            write_result
        );

        // 验证写入后的读取
        let read_time: u64 = clint.read_u64(0xbff8).unwrap();
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
            board.step();

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

        board.step();
        assert!(board.cpu_mut().read_pc() == interrupt_handler_addr);

        let mcause = board
            .cpu_mut()
            .debug_csr(Mcause::get_index(), None)
            .unwrap();
        assert_eq!(mcause, (1u64 << (XLEN - 1)) | 0b11)
    }

    #[cfg(feature = "test-device")]
    #[test]
    fn sample_timer_rearms_after_control_reset() {
        use std::{
            thread::sleep,
            time::{Duration, Instant},
        };

        use crate::device::config::SAMPLE_TIMER_BASE;
        use crate::device::sample_timer::SAMPLE_TIMER_INTERRUPT_ID;
        use crate::{config::arch_config::WordType, isa::riscv::debugger::Address};
        const CONTEXT_ENABLE_BIT_OFFSET: WordType = 0x002000;
        const CONTEXT_ENABLE_BIT_SIZE: WordType = 0x80;
        const CONTEXT_CONFIG_OFFSET: WordType = 0x200000;
        const CONTEXT_CONFIG_SIZE: WordType = 0x1000;
        const CLAIM_COMPLETE_OFFSET: WordType =
            CONTEXT_CONFIG_OFFSET + (0 * CONTEXT_CONFIG_SIZE) + 4;

        fn wait_for_claim(board: &mut VirtBoard, claim_addr: WordType) -> u32 {
            let deadline = Instant::now() + Duration::from_secs(1);
            loop {
                board.step();
                let claimed_id = board.plic.borrow_mut().read_u32(claim_addr).unwrap();
                if claimed_id != 0 {
                    return claimed_id;
                }
                assert!(
                    Instant::now() < deadline,
                    "sample timer interrupt timed out"
                );
                sleep(Duration::from_millis(1));
            }
        }

        let mut board = create_test_board();

        {
            let mut plic = board.plic.borrow_mut();
            // priority_threshold
            let addr = CONTEXT_CONFIG_OFFSET + (0 * CONTEXT_CONFIG_SIZE);
            plic.write_u32(addr, 1).unwrap();

            // Sample timer interrupt priority.
            plic.write_u32(SAMPLE_TIMER_INTERRUPT_ID as WordType * 4, 5)
                .unwrap();

            // interrupt enable.
            let addr = CONTEXT_ENABLE_BIT_OFFSET + (0 * CONTEXT_ENABLE_BIT_SIZE) + 4;
            plic.write_u32(addr, 0xffffffff).unwrap();
        }

        // Mask writes cancel an outstanding deadline, so configure it before
        // the interval registers that schedule the timer.
        board
            .cpu
            .write_memory(
                Address::Phys(SAMPLE_TIMER_BASE + size_of::<u32>() as WordType),
                1u32,
            )
            .unwrap();
        board
            .cpu
            .write_memory(
                Address::Phys(SAMPLE_TIMER_BASE + 2 * size_of::<u32>() as WordType),
                10u32,
            )
            .unwrap();
        board
            .cpu
            .write_memory(
                Address::Phys(SAMPLE_TIMER_BASE + 3 * size_of::<u32>() as WordType),
                0u32,
            )
            .unwrap();

        let first_claim = wait_for_claim(&mut board, CLAIM_COMPLETE_OFFSET);
        assert_eq!(first_claim, SAMPLE_TIMER_INTERRUPT_ID);

        // Clear the device before completing the level-triggered PLIC interrupt.
        board
            .cpu
            .write_memory(Address::Phys(SAMPLE_TIMER_BASE), 1u32)
            .unwrap();
        board
            .plic
            .borrow_mut()
            .write_u32(CLAIM_COMPLETE_OFFSET, first_claim)
            .unwrap();
        assert_eq!(
            board
                .plic
                .borrow_mut()
                .read_u32(CLAIM_COMPLETE_OFFSET)
                .unwrap(),
            0
        );

        let second_claim = wait_for_claim(&mut board, CLAIM_COMPLETE_OFFSET);
        assert_eq!(second_claim, SAMPLE_TIMER_INTERRUPT_ID);
    }
}
