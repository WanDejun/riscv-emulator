use std::rc::Rc;

use crate::vclock::{Timer, VirtualClockRef};

pub struct Clint {
    clock: VirtualClockRef,
    timer: Rc<Timer>,
}

impl Clint {
    pub fn new(clock: VirtualClockRef, timer: Rc<Timer>) -> Self {
        Self { clock, timer }
    }
}
