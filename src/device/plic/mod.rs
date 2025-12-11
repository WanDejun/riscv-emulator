pub mod irq_line;

use std::{
    collections::BTreeSet,
    hint::unlikely,
    sync::{Arc, atomic::AtomicU32},
};

use bit_set::BitSet;

use crate::{
    board::virt::RiscvIRQSource,
    config::arch_config::WordType,
    device::{DeviceTrait, MemError, config::PLIC_SIZE, plic::irq_line::PlicIRQHandler},
};

const PLIC_MAX_INTERRUPTS: usize = 1024;
const VIRT_MAX_INTERRUPTS: usize = 64;
const PLIC_MAX_CONTEXTS: usize = 15872;
const VIRT_MAX_CONTEXTS: usize = 16;

const PRIORITY_OFFSET: WordType = 0;
const PENDING_BIT_OFFSET: WordType = 0x001000;
const CONTEXT_ENABLE_BIT_OFFSET: WordType = 0x002000;
const CONTEXT_ENABLE_BIT_SIZE: WordType = 0x80;
const CONTEXT_CONFIG_OFFSET: WordType = 0x200000;
const CONTEXT_CONFIG_SIZE: WordType = 0x1000;

pub type ExternalInterrupt = u32;

pub struct PLICContext {
    enable: [u32; VIRT_MAX_INTERRUPTS / 32], // base + 0x2000 + contextN * 0x80 ~ base + 0x2000 + contextN * 0x80 + 0x7c
    priority_threshold: u32,                 // base + 0x200000 + contextN * 0x1000
    // claim_register: u32,  base + 0x200000 + contextN * 0x1004
    claim: u32,
}

impl PLICContext {
    pub fn new() -> Self {
        PLICContext {
            enable: [0; VIRT_MAX_INTERRUPTS / 32],
            priority_threshold: 0,
            claim: 0,
        }
    }
}

struct PLICPending {
    bits: [AtomicU32; VIRT_MAX_INTERRUPTS / 32],
}

impl PLICPending {
    pub fn new() -> Self {
        PLICPending {
            bits: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    fn set_bit(&self, interrupt_id: ExternalInterrupt) {
        let index = interrupt_id / 32;
        let bit = interrupt_id % 32;
        self.bits[index as usize].fetch_or(1 << bit, std::sync::atomic::Ordering::SeqCst);
    }

    #[inline]
    fn clear_bit(&self, interrupt_id: ExternalInterrupt) {
        self.take_bit(interrupt_id);
    }

    fn get_bit(&self, interrupt_id: ExternalInterrupt) -> bool {
        let index = (interrupt_id / 32) as usize;
        let bit = interrupt_id % 32;
        (self.bits[index].load(std::sync::atomic::Ordering::SeqCst) & (1 << bit)) != 0
    }

    fn take_bit(&self, interrupt_id: ExternalInterrupt) -> bool {
        let index = (interrupt_id / 32) as usize;
        let bit = interrupt_id % 32;
        let mask = 1 << bit;
        let old = self.bits[index].fetch_and(!(mask), std::sync::atomic::Ordering::SeqCst);
        old & mask != 0
    }
}

/*
    // priority (0x000000 - 0x000ffc)
    base + 0x000000: Reserved (interrupt source 0 does not exist)
    base + 0x000004: Interrupt source 1 priority
    base + 0x000008: Interrupt source 2 priority
    ...
    base + 0x000FFC: Interrupt source 1023 priority

    // pending (0x001000 - 0x00107c)
    base + 0x001000: Interrupt Pending bit 0-31
    base + 0x00107C: Interrupt Pending bit 992-1023

    // enable (0x002000 - 0x1FFFFC)
        // Context 0
        base + 0x002000: Enable bits for sources 0-31 on context 0
        base + 0x002004: Enable bits for sources 32-63 on context 0
        ...
        base + 0x00207C: Enable bits for sources 992-1023 on context 0

        // Context 2
        ...

        // Context 15871
        base + 0x1F1F80: Enable bits for sources 0-31 on context 15871
        base + 0x1F1F84: Enable bits for sources 32-63 on context 15871
        ...
        base + 0x1F1FFC: Enable bits for sources 992-1023 on context 15871

    // claim
        // Context 0
        base + 0x200000: Priority threshold for context 0
        base + 0x200004: Claim/complete for context 0

        // Context 1
        base + 0x201000: Priority threshold for context 1
        base + 0x201004: Claim/complete for context 1

        // Context 2
        ...

        // context 15871
        base + 0x3FFF000: Priority threshold for context 15871
        base + 0x3FFF004: Claim/complete for context 15871
        ...
        base + 0x3FFFFFC: Reserved
*/
pub struct PLICLayout {
    priority: [u32; VIRT_MAX_INTERRUPTS], // base ~ base + 0x0ffc
    pending: Arc<PLICPending>,            // base + 0x1000 ~ base + 0x107c
    contexts: [PLICContext; VIRT_MAX_CONTEXTS], // base + 0x2000 ~ base + 0x3FFFFC

    ordering: BTreeSet<(u32, ExternalInterrupt)>,

    interrupt_sources_busy: BitSet,
}

impl PLICLayout {
    pub fn new() -> Self {
        let priority = [0; VIRT_MAX_INTERRUPTS];
        let mut ordering = BTreeSet::new();

        // interrupt priority default value is 0.
        // interrupt 0 is not exist.
        for i in 1..VIRT_MAX_INTERRUPTS {
            ordering.insert((priority[i], i as u32));
        }
        PLICLayout {
            priority,
            pending: Arc::new(PLICPending::new()),
            contexts: core::array::from_fn(|_| PLICContext::new()),

            ordering,

            interrupt_sources_busy: BitSet::with_capacity(VIRT_MAX_INTERRUPTS),
        }
    }

    #[inline]
    fn get_priority(&self, interrupt_id: ExternalInterrupt) -> u32 {
        self.priority[interrupt_id as usize]
    }

    #[inline]
    fn set_priority(&mut self, interrupt_id: ExternalInterrupt, value: u32) {
        if unlikely(interrupt_id as usize > VIRT_MAX_INTERRUPTS) {
            return;
        }
        let old_priority = self.priority[interrupt_id as usize];
        if value != old_priority {
            // set ordering.
            self.ordering.remove(&(old_priority, interrupt_id));
            self.ordering.insert((value, interrupt_id));

            self.priority[interrupt_id as usize] = value;
        }
    }

    #[inline]
    fn take_pending_bit(&self, interrupt_id: ExternalInterrupt) -> bool {
        self.pending.take_bit(interrupt_id)
    }

    #[inline]
    fn get_enable_bit(&self, context_nr: usize, interrupt_id: ExternalInterrupt) -> bool {
        let index = (interrupt_id / 32) as usize;
        let bit = interrupt_id % 32;
        (self.contexts[context_nr].enable[index] & (1 << bit)) != 0
    }

    #[inline]
    fn get_priority_threshold(&self, context_nr: usize) -> u32 {
        self.contexts[context_nr].priority_threshold
    }

    fn check_interrupt(&mut self, context_nr: usize) -> Option<u32> {
        // context is busy.
        if unlikely(self.contexts[context_nr].claim != 0) {
            return None;
        }

        let priority_threshold = self.contexts[context_nr].priority_threshold;

        // Traverse in descending order of priority.
        for (priority, interrupt_id) in self.ordering.iter().rev() {
            // The upcoming priorities are all equal to 0.
            if *priority == 0 {
                break;
            }

            if self.interrupt_sources_busy.contains(*interrupt_id as usize) {
                continue;
            }

            // The PLIC will mask all PLIC interrupts of a priority less than or equal to threshold.
            // First check the enable_bit, then check the pending_bit.
            if *priority > priority_threshold
                && self.get_enable_bit(context_nr, *interrupt_id)
                && self.take_pending_bit(*interrupt_id)
            {
                self.contexts[context_nr].claim = *interrupt_id;
                self.interrupt_sources_busy.insert(*interrupt_id as usize);

                return Some(*interrupt_id);
            }
        }

        return None;
    }
}

pub struct PLIC {
    layout: PLICLayout,
    irq_line: [Option<crate::board::virt::IRQLine>; VIRT_MAX_CONTEXTS],
}

impl PLIC {
    pub fn new() -> Self {
        PLIC {
            layout: PLICLayout::new(),
            irq_line: core::array::from_fn(|_| None),
        }
    }

    fn read_impl<T>(&mut self, inner_addr: WordType) -> Result<T, super::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check align in MMIO.
        if core::mem::size_of::<T>() != 4 {
            return Err(MemError::LoadFault);
        }

        if inner_addr < PENDING_BIT_OFFSET {
            // priority
            let interrupt_id = (inner_addr / 4) as ExternalInterrupt;
            if interrupt_id == 0 || interrupt_id >= VIRT_MAX_INTERRUPTS as ExternalInterrupt {
                return Err(MemError::LoadFault);
            }
            let data = self.layout.get_priority(interrupt_id);
            Ok(unsafe { core::mem::transmute_copy(&data) })
        } else if inner_addr < CONTEXT_ENABLE_BIT_OFFSET {
            // pending
            if let Some(index) = self.get_pending_index(inner_addr) {
                let data =
                    self.layout.pending.bits[index].load(std::sync::atomic::Ordering::SeqCst);
                Ok(unsafe { core::mem::transmute_copy(&data) })
            } else {
                Err(MemError::LoadFault)
            }
        } else if inner_addr < CONTEXT_CONFIG_OFFSET {
            // enable bits
            if let Some((context_id, interrupt_id_div32)) = self.get_enable_word_index(inner_addr) {
                let data = self.layout.contexts[context_id].enable[interrupt_id_div32];
                Ok(unsafe { core::mem::transmute_copy(&data) })
            } else {
                Err(MemError::LoadFault)
            }
        } else if inner_addr < PLIC_SIZE {
            // config region
            if let Some((context_id, offset_in_context)) = self.get_context_config_index(inner_addr)
            {
                if offset_in_context == 0 {
                    // Priority Threshold
                    let data = self.layout.contexts[context_id].priority_threshold;
                    Ok(unsafe { core::mem::transmute_copy(&data) })
                } else if offset_in_context == 1 {
                    // Claim/Complete
                    let data = self.layout.contexts[context_id].claim;
                    Ok(unsafe { core::mem::transmute_copy(&data) })
                } else {
                    Err(MemError::LoadFault)
                }
            } else {
                Err(MemError::LoadFault)
            }
        } else {
            unreachable!();
        }
    }

    fn write_impl<T>(&mut self, inner_addr: WordType, data: T) -> Result<(), super::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check align in MMIO.
        if core::mem::size_of::<T>() != 4 {
            return Err(MemError::StoreFault);
        }

        if inner_addr < 0x1000 {
            // priority
            let interrupt_id = (inner_addr / 4) as u32;
            if interrupt_id == 0 || interrupt_id >= VIRT_MAX_INTERRUPTS as u32 {
                return Err(MemError::StoreFault);
            }

            self.layout
                .set_priority(interrupt_id, unsafe { core::mem::transmute_copy(&data) });
            Ok(())
        } else if inner_addr < CONTEXT_ENABLE_BIT_OFFSET {
            // pending is read-only
            Err(MemError::StoreFault)
        } else if inner_addr < CONTEXT_CONFIG_OFFSET {
            // enable bits
            if let Some((context_id, interrupt_id_div32)) = self.get_enable_word_index(inner_addr) {
                self.layout.contexts[context_id].enable[interrupt_id_div32] =
                    unsafe { core::mem::transmute_copy(&data) };
                Ok(())
            } else {
                Err(MemError::StoreFault)
            }
        } else if inner_addr < PLIC_SIZE {
            // config region
            if let Some((context_id, offset_in_context)) = self.get_context_config_index(inner_addr)
            {
                if offset_in_context == 0 {
                    // Priority Threshold
                    self.layout.contexts[context_id].priority_threshold =
                        unsafe { core::mem::transmute_copy(&data) };
                    Ok(())
                } else if offset_in_context == 1 {
                    // Claim/Complete
                    let old_claim = &mut self.layout.contexts[context_id].claim;
                    self.layout
                        .interrupt_sources_busy
                        .remove(*old_claim as usize);
                    *old_claim = 0;
                    Ok(())
                } else {
                    Err(MemError::StoreFault)
                }
            } else {
                Err(MemError::StoreFault)
            }
        } else {
            unreachable!();
        }
    }

    pub fn trigger_interrupt(&mut self, interrupt_id: ExternalInterrupt) {
        if unlikely(interrupt_id > VIRT_MAX_INTERRUPTS as ExternalInterrupt) {
            return;
        }
        self.layout.pending.set_bit(interrupt_id);
    }

    // pub fn clear_interrupt(&mut self, interrupt_id: usize) {
    //     if unlikely(interrupt_id > VIRT_MAX_INTERRUPTS) {
    //         return;
    //     }
    //     self.layout.pending.clear_bit(interrupt_id);
    // }

    /// try get interrupt of context_nr, send interrupt signal to cpu, interrupt line existed.
    pub fn try_get_interrupt(&mut self, context_nr: usize) -> Option<u32> {
        if let Some(interrupt_id) = self.layout.check_interrupt(context_nr) {
            if let Some(irq_line) = &mut self.irq_line[context_nr] {
                irq_line.set_irq(true);
            }

            Some(interrupt_id)
        } else {
            None
        }
    }

    // inner_addr point to self.layout.contexts[return.0].enable[return.1]
    fn get_enable_word_index(&self, inner_addr: WordType) -> Option<(usize, usize)> {
        // Out of range.
        if unlikely(inner_addr < CONTEXT_ENABLE_BIT_OFFSET) {
            return None;
        }

        // Out of VIRT_MAX_CONTEXTS.
        let context_id =
            ((inner_addr - CONTEXT_ENABLE_BIT_OFFSET) / CONTEXT_ENABLE_BIT_SIZE) as usize;
        if context_id >= VIRT_MAX_CONTEXTS {
            return None;
        }

        let offset_in_context = (inner_addr - CONTEXT_ENABLE_BIT_OFFSET) % CONTEXT_ENABLE_BIT_SIZE;
        let interrupt_id_div32 = (offset_in_context / 4) as usize;
        if interrupt_id_div32 >= VIRT_MAX_INTERRUPTS / 32 {
            return None;
        }
        Some((context_id, interrupt_id_div32))
    }

    fn get_pending_index(&self, inner_addr: WordType) -> Option<usize> {
        // Out of range.
        if unlikely(inner_addr < PENDING_BIT_OFFSET) {
            return None;
        }

        let offset_in_pending = inner_addr - PENDING_BIT_OFFSET;
        let index = offset_in_pending as usize / size_of::<u32>();
        if index >= VIRT_MAX_INTERRUPTS / 32 {
            return None;
        }
        Some(index)
    }

    fn get_context_config_index(&self, inner_addr: WordType) -> Option<(usize, usize)> {
        // Out of range.
        if unlikely(inner_addr < CONTEXT_CONFIG_OFFSET) {
            return None;
        }

        let context_id = ((inner_addr - CONTEXT_CONFIG_OFFSET) / CONTEXT_CONFIG_SIZE) as usize;
        if context_id >= VIRT_MAX_CONTEXTS {
            return None;
        }

        let offset_in_context = (inner_addr - CONTEXT_CONFIG_OFFSET) % CONTEXT_CONFIG_SIZE;
        Some((context_id, offset_in_context as usize / 4))
    }
}

impl DeviceTrait for PLIC {
    dispatch_read_write! { read_impl, write_impl }

    fn get_poll_enent(&mut self) -> Option<crate::async_poller::PollingEvent> {
        None
    }
    fn sync(&mut self) {
        // nothing to do.
    }
}

// Send the external interrupt resulting from the arbitration to the CPU through the IRQLine.
impl RiscvIRQSource for PLIC {
    fn set_irq_line(&mut self, line: crate::board::virt::IRQLine, id: usize) {
        assert!(id < VIRT_MAX_CONTEXTS);
        // plic external interrupt source id will be write to plic.claim register.
        self.irq_line[id] = Some(line);
    }
}

// Receive the interrupt signal from peripherals.
impl PlicIRQHandler for PLIC {
    fn handle_irq(&mut self, interrupt: ExternalInterrupt, level: bool) {
        if level {
            self.trigger_interrupt(interrupt);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // all methods go through mmio interface.
    impl PLIC {
        fn get_priority(&mut self, interrupt_id: WordType) -> Result<u32, MemError> {
            self.read_impl(interrupt_id * 4)
        }

        fn set_priority(&mut self, interrupt_id: WordType, value: u32) -> Result<(), MemError> {
            self.write_impl(interrupt_id * 4, value)
        }

        fn get_pending_bit(&mut self, interrupt_id: WordType) -> Result<bool, MemError> {
            let addr = PENDING_BIT_OFFSET + (interrupt_id / 32 * 4) as WordType;
            let word = self.read_impl::<u32>(addr)?;
            let bit = interrupt_id % 32;
            Ok((word & (1 << bit)) != 0)
        }

        fn get_enable_word(
            &mut self,
            interrupt_id: WordType,
            inner_index: WordType,
        ) -> Result<u32, MemError> {
            let addr = CONTEXT_ENABLE_BIT_OFFSET
                + (interrupt_id * CONTEXT_ENABLE_BIT_SIZE)
                + (inner_index * 4);
            self.read_impl::<u32>(addr)
        }

        fn set_enable_word(
            &mut self,
            context_id: WordType,
            inner_index: WordType,
            value: u32,
        ) -> Result<(), MemError> {
            let addr = CONTEXT_ENABLE_BIT_OFFSET
                + (context_id * CONTEXT_ENABLE_BIT_SIZE)
                + (inner_index * 4);
            self.write_impl(addr, value)
        }

        fn get_priority_threshold(&mut self, context_id: WordType) -> Result<u32, MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE);
            self.read_impl::<u32>(addr)
        }

        fn get_claim_complete(&mut self, context_id: WordType) -> Result<u32, MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE) + 4;
            self.read_impl::<u32>(addr)
        }

        fn set_priority_threshold(
            &mut self,
            context_id: WordType,
            value: u32,
        ) -> Result<(), MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE);
            self.write_impl(addr, value)
        }

        fn set_claim_complete(
            &mut self,
            context_id: WordType,
            interrupt_id: u32,
        ) -> Result<(), MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE) + 4;
            self.write_impl(addr, interrupt_id)
        }
    }

    #[test]
    fn plic_layout_test() {
        let mut plic = PLIC::new();

        // =======================
        // ====== priority =======
        // =======================``
        assert!(plic.set_priority(0, 0).is_err()); // (interrupt source 0 does not exist)
        assert!(
            plic.get_priority((VIRT_MAX_INTERRUPTS * size_of::<u32>()) as WordType)
                .is_err()
        ); // over max priority index.
        plic.set_priority(1, 5u32).unwrap();
        assert_eq!(plic.get_priority(1).unwrap(), 5u32);

        // =======================
        // ======= pending =======
        // =======================
        assert!(
            plic.write_impl(
                PENDING_BIT_OFFSET + 1 * size_of::<u32>() as WordType,
                0x1234_5678u32
            )
            .is_err()
        ); // pending is read-only
        assert_eq!(plic.get_pending_bit(5).unwrap(), false);
        assert!(
            plic.get_pending_bit(VIRT_MAX_INTERRUPTS as WordType)
                .is_err()
        ); // over max pending index
        assert!(
            plic.read_impl::<u32>(PENDING_BIT_OFFSET + VIRT_MAX_INTERRUPTS as WordType / 8)
                .is_err()
        ); // over max pending index

        // =======================
        // ===== enable bits =====
        // =======================
        plic.set_enable_word(0, 1, 0xdead_beefu32).unwrap();
        assert_eq!(plic.get_enable_word(0, 1).unwrap(), 0xdead_beefu32); // assert the value read is consistent with the one written.
        assert!(
            plic.set_enable_word(VIRT_MAX_CONTEXTS as WordType, 0, 0x1234_5678u32)
                .is_err()
        ); // over max context index
        assert!(
            plic.set_enable_word(0, VIRT_MAX_INTERRUPTS as WordType / 32, 0x1234_5678u32)
                .is_err()
        ); // over max context index

        // =======================
        // === context config ====
        // =======================
        plic.set_priority_threshold(0, 3).unwrap();
        assert_eq!(plic.get_priority_threshold(0).unwrap(), 3);
        assert!(
            plic.set_priority_threshold(VIRT_MAX_CONTEXTS as WordType, 0)
                .is_err()
        ); // over max context index

        // TODO: test claim/complete.
    }

    #[test]
    fn interrupt_test() {
        let mut plic = PLIC::new();
        plic.set_priority(1, 5).unwrap();
        plic.set_priority(2, 7).unwrap();
        plic.trigger_interrupt(1);
        plic.trigger_interrupt(2);
        assert!(plic.try_get_interrupt(0).is_none());

        // context 0 <- interrupt 2 (receiveed)
        plic.set_enable_word(0, 0, 0xffffffff).unwrap();
        assert_eq!(plic.try_get_interrupt(0), Some(2)); // receive but do not complete.
        assert!(plic.try_get_interrupt(0).is_none()); // context is busy for interrupt 2

        // context 1 <- interrupt 1 (receiveed)
        plic.set_enable_word(1, 0, 0xffffffff).unwrap();
        assert_eq!(plic.try_get_interrupt(1), Some(1)); // receive but do not complete.
        // context 1 <- interrupt 1 (completed)
        plic.set_claim_complete(1, 1).unwrap();

        plic.trigger_interrupt(2);
        assert!(plic.try_get_interrupt(1).is_none()); // interrupt 2 is not completed.

        // context 0 <- interrupt 2 (completed)
        plic.set_claim_complete(0, 2).unwrap();

        // context 1 <- interrupt 2 (receiveed)
        assert_eq!(plic.try_get_interrupt(1), Some(2));

        // context 0 <- None
        assert!(plic.try_get_interrupt(0).is_none());
    }
}
