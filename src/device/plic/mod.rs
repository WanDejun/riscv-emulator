use std::{
    hint::unlikely,
    sync::{Arc, atomic::AtomicU32},
};

use crate::{
    config::arch_config::WordType,
    device::{Mem, MemError, config::PLIC_SIZE},
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

pub struct PLICContext {
    enable: [u32; VIRT_MAX_INTERRUPTS / 32], // base + 0x2000 + contextN * 0x80 ~ base + 0x2000 + contextN * 0x80 + 0x7c
    priority_threshold: u32,                 // base + 0x200000 + contextN * 0x1000
                                             // claim_register: u32,  base + 0x200000 + contextN * 0x1004
}

impl PLICContext {
    pub fn new() -> Self {
        PLICContext {
            enable: [0; VIRT_MAX_INTERRUPTS / 32],
            priority_threshold: 0,
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

    pub fn set_bit(&self, interrupt_id: usize) {
        let index = interrupt_id / 32;
        let bit = interrupt_id % 32;
        self.bits[index].fetch_or(1 << bit, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn clear_bit(&self, interrupt_id: usize) {
        let index = interrupt_id / 32;
        let bit = interrupt_id % 32;
        self.bits[index].fetch_and(!(1 << bit), std::sync::atomic::Ordering::SeqCst);
    }

    pub fn get_bit(&self, interrupt_id: usize) -> bool {
        let index = interrupt_id / 32;
        let bit = interrupt_id % 32;
        (self.bits[index].load(std::sync::atomic::Ordering::SeqCst) & (1 << bit)) != 0
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
}

impl PLICLayout {
    pub fn new() -> Self {
        PLICLayout {
            priority: [0; VIRT_MAX_INTERRUPTS],
            pending: Arc::new(PLICPending::new()),
            contexts: core::array::from_fn(|_| PLICContext::new()),
        }
    }

    #[inline]
    fn get_priority(&self, interrupt_id: usize) -> u32 {
        self.priority[interrupt_id]
    }

    #[inline]
    fn set_priority(&mut self, interrupt_id: usize, value: u32) {
        self.priority[interrupt_id] = value;
    }

    #[inline]
    fn get_pending_bit(&self, interrupt_id: usize) -> bool {
        self.pending.get_bit(interrupt_id)
    }

    #[inline]
    fn get_enable_bit(&self, context_id: usize, interrupt_id: usize) -> bool {
        let index = interrupt_id / 32;
        let bit = interrupt_id % 32;
        (self.contexts[context_id].enable[index] & (1 << bit)) != 0
    }

    #[inline]
    fn get_priority_threshold(&self, context_id: usize) -> u32 {
        self.contexts[context_id].priority_threshold
    }
}

pub struct PLIC {
    layout: PLICLayout,
}

impl PLIC {
    pub fn new() -> Self {
        PLIC {
            layout: PLICLayout::new(),
        }
    }

    // inner_addr point to self.layout.contexts[return.0].enable[return.1]
    fn check_and_index_enable_word(&self, inner_addr: WordType) -> Option<(usize, usize)> {
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

    fn check_and_index_pending(&self, inner_addr: WordType) -> Option<usize> {
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

    fn check_and_index_context_config(&self, inner_addr: WordType) -> Option<(usize, usize)> {
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

impl Mem for PLIC {
    fn read<T>(&mut self, inner_addr: WordType) -> Result<T, super::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check align in MMIO.
        if core::mem::size_of::<T>() != 4 {
            return Err(MemError::LoadFault);
        }

        if inner_addr < PENDING_BIT_OFFSET {
            // priority
            let interrupt_id = (inner_addr / 4) as usize;
            if interrupt_id == 0 || interrupt_id >= VIRT_MAX_INTERRUPTS {
                return Err(MemError::LoadFault);
            }
            let data = self.layout.get_priority(interrupt_id);
            Ok(unsafe { core::mem::transmute_copy(&data) })
        } else if inner_addr < CONTEXT_ENABLE_BIT_OFFSET {
            // pending
            if let Some(index) = self.check_and_index_pending(inner_addr) {
                let data =
                    self.layout.pending.bits[index].load(std::sync::atomic::Ordering::SeqCst);
                Ok(unsafe { core::mem::transmute_copy(&data) })
            } else {
                Err(MemError::LoadFault)
            }
        } else if inner_addr < CONTEXT_CONFIG_OFFSET {
            // enable bits
            if let Some((context_id, interrupt_id_div32)) =
                self.check_and_index_enable_word(inner_addr)
            {
                let data = self.layout.contexts[context_id].enable[interrupt_id_div32];
                Ok(unsafe { core::mem::transmute_copy(&data) })
            } else {
                Err(MemError::LoadFault)
            }
        } else if inner_addr < PLIC_SIZE {
            // config region
            if let Some((context_id, offset_in_context)) =
                self.check_and_index_context_config(inner_addr)
            {
                if offset_in_context == 0 {
                    // Priority Threshold
                    let data = self.layout.contexts[context_id].priority_threshold;
                    Ok(unsafe { core::mem::transmute_copy(&data) })
                } else if offset_in_context == 1 {
                    // Claim/Complete
                    todo!();
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

    fn write<T>(&mut self, inner_addr: WordType, data: T) -> Result<(), super::MemError>
    where
        T: crate::utils::UnsignedInteger,
    {
        // check align in MMIO.
        if core::mem::size_of::<T>() != 4 {
            return Err(MemError::StoreFault);
        }

        if inner_addr < 0x1000 {
            // priority
            let interrupt_id = (inner_addr / 4) as usize;
            if interrupt_id == 0 || interrupt_id >= VIRT_MAX_INTERRUPTS {
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
            if let Some((context_id, interrupt_id_div32)) =
                self.check_and_index_enable_word(inner_addr)
            {
                self.layout.contexts[context_id].enable[interrupt_id_div32] =
                    unsafe { core::mem::transmute_copy(&data) };
                Ok(())
            } else {
                Err(MemError::StoreFault)
            }
        } else if inner_addr < PLIC_SIZE {
            // config region
            if let Some((context_id, offset_in_context)) =
                self.check_and_index_context_config(inner_addr)
            {
                if offset_in_context == 0 {
                    // Priority Threshold
                    self.layout.contexts[context_id].priority_threshold =
                        unsafe { core::mem::transmute_copy(&data) };
                    Ok(())
                } else if offset_in_context == 4 {
                    // Claim/Complete
                    todo!();
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
}

#[cfg(test)]
mod test {
    use super::*;

    impl PLIC {
        fn get_priority(&mut self, interrupt_id: WordType) -> Result<u32, MemError> {
            self.read(interrupt_id * 4)
        }

        fn set_priority(&mut self, interrupt_id: WordType, value: u32) -> Result<(), MemError> {
            self.write(interrupt_id * 4, value)
        }

        fn get_pending_bit(&mut self, interrupt_id: WordType) -> Result<bool, MemError> {
            let addr = PENDING_BIT_OFFSET + (interrupt_id / 32 * 4) as WordType;
            let word = self.read::<u32>(addr)?;
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
            self.read::<u32>(addr)
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
            self.write(addr, value)
        }

        fn get_priority_threshold(&mut self, context_id: WordType) -> Result<u32, MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE);
            self.read::<u32>(addr)
        }

        fn get_claim_complete(&mut self, context_id: WordType) -> Result<u32, MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE) + 4;
            self.read::<u32>(addr)
        }

        fn set_priority_threshold(
            &mut self,
            context_id: WordType,
            value: u32,
        ) -> Result<(), MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE);
            self.write(addr, value)
        }

        fn set_claim_complete(&mut self, context_id: WordType, value: u32) -> Result<(), MemError> {
            let addr = CONTEXT_CONFIG_OFFSET + (context_id * CONTEXT_CONFIG_SIZE) + 4;
            self.write(addr, value)
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
            plic.write(
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
            plic.read::<u32>(PENDING_BIT_OFFSET + VIRT_MAX_INTERRUPTS as WordType / 8)
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
}
