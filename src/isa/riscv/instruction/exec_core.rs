use crate::{
    config::arch_config::WordType,
    isa::riscv::{executor::RVCPU, trap::Exception},
    utils::{FloatPoint, TruncateTo, UnsignedInteger, sign_extend},
};

#[inline(always)]
pub(super) fn handle_load<T, const EXTEND: bool>(
    cpu: &mut RVCPU,
    rd: u8,
    addr: WordType,
) -> Result<(), Exception>
where
    T: UnsignedInteger,
{
    let ret = cpu.memory.read::<T>(addr, &mut cpu.csr);

    match ret {
        Ok(data) => {
            let data_64: u64 = data.into();
            let mut data = data_64 as WordType;
            if EXTEND {
                data = sign_extend(data, (size_of::<T>() as u32) * 8);
            }
            cpu.reg_file.write(rd, data);
        }
        Err(err) => {
            cpu.pending_tval = Some(addr);

            return Err(Exception::from_memory_err(err));
        }
    }
    Ok(())
}

#[inline(always)]
pub(super) fn handle_float_load<F>(cpu: &mut RVCPU, addr: WordType, rd: u8) -> Result<(), Exception>
where
    F: FloatPoint,
{
    let rst = cpu.memory.read::<F::BitsType>(addr, &mut cpu.csr);

    match rst {
        Ok(data) => {
            cpu.fpu.store_raw::<F>(rd, data.truncate_to());
            Ok(())
        }
        Err(err) => {
            cpu.pending_tval = Some(addr);
            Err(Exception::from_memory_err(err))
        }
    }
}

#[inline(always)]
pub(super) fn handle_store<T>(
    cpu: &mut RVCPU,
    addr: WordType,
    data: WordType,
) -> Result<(), Exception>
where
    T: UnsignedInteger,
{
    let ret = cpu.memory.write(addr, T::truncate_from(data), &mut cpu.csr);
    if let Err(err) = ret {
        cpu.pending_tval = Some(addr);
        return Err(Exception::from_memory_err(err));
    }
    Ok(())
}

#[inline(always)]
pub(super) fn handle_float_store<F>(
    cpu: &mut RVCPU,
    addr: WordType,
    data: F::BitsType,
) -> Result<(), Exception>
where
    F: FloatPoint,
{
    let ret = cpu.memory.write(addr, data, &mut cpu.csr);
    if let Err(err) = ret {
        cpu.pending_tval = Some(addr);
        return Err(Exception::from_memory_err(err));
    }
    Ok(())
}
