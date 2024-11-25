use super::{Executor, InstructionPtr};
use crate::{
    core::{TrapCode, UntypedVal},
    engine::utils::unreachable_unchecked,
    ir::{index::Memory, AnyConst16, Const16, Instruction, Reg},
    store::StoreInner,
    Error,
};

/// The function signature of Wasm store operations.
type WasmStoreOp = fn(
    memory: &mut [u8],
    address: UntypedVal,
    offset: u32,
    value: UntypedVal,
) -> Result<(), TrapCode>;

impl Executor<'_> {
    /// Returns the register `value` and `offset` parameters for a `load` [`Instruction`].
    fn fetch_value_and_offset(&self) -> (Reg, u32) {
        let mut addr: InstructionPtr = self.ip;
        addr.add(1);
        match *addr.get() {
            Instruction::RegisterAndImm32 { reg, imm } => (reg, u32::from(imm)),
            unexpected => {
                // Safety: Wasmi translation guarantees that [`Instruction::RegisterAndImm32`] exists.
                unsafe {
                    unreachable_unchecked!(
                        "expected `Instruction::RegisterAndImm32` but found {unexpected:?}"
                    )
                }
            }
        }
    }

    /// Returns the immediate `value` and `offset` parameters for a `load` [`Instruction`].
    fn fetch_value_and_offset_imm<T>(&self) -> (T, u32)
    where
        T: From<AnyConst16>,
    {
        let mut addr: InstructionPtr = self.ip;
        addr.add(1);
        match *addr.get() {
            Instruction::Imm16AndImm32 { imm16, imm32 } => (T::from(imm16), u32::from(imm32)),
            unexpected => {
                // Safety: Wasmi translation guarantees that [`Instruction::Imm16AndImm32`] exists.
                unsafe {
                    unreachable_unchecked!(
                        "expected `Instruction::Imm16AndImm32` but found {unexpected:?}"
                    )
                }
            }
        }
    }

    /// Fetches the bytes of the default memory at index 0.
    #[inline]
    fn fetch_default_memory_bytes_mut(&mut self) -> &mut [u8] {
        // Safety: the `self.cache.memory` pointer is always synchronized
        //         conservatively whenever it could have been invalidated.
        unsafe { self.cache.memory.data_mut() }
    }

    /// Fetches the bytes of the given `memory`.
    #[inline]
    fn fetch_memory_bytes_mut<'exec, 'store, 'bytes>(
        &'exec mut self,
        memory: Memory,
        store: &'store mut StoreInner,
    ) -> &'bytes mut [u8]
    where
        'exec: 'bytes,
        'store: 'bytes,
    {
        match memory.is_default() {
            true => self.fetch_default_memory_bytes_mut(),
            false => self.fetch_non_default_memory_bytes_mut(memory, store),
        }
    }

    /// Fetches the bytes of the given non-default `memory`.
    #[cold]
    #[inline]
    fn fetch_non_default_memory_bytes_mut<'exec, 'store, 'bytes>(
        &'exec mut self,
        memory: Memory,
        store: &'store mut StoreInner,
    ) -> &'bytes mut [u8]
    where
        'exec: 'bytes,
        'store: 'bytes,
    {
        let memory = self.get_memory(memory);
        store.resolve_memory_mut(&memory).data_mut()
    }

    /// Executes a generic Wasm `store[N]` operation.
    ///
    /// # Note
    ///
    /// This can be used to emulate the following Wasm operands:
    ///
    /// - `{i32, i64, f32, f64}.store`
    /// - `{i32, i64}.store8`
    /// - `{i32, i64}.store16`
    /// - `i64.store32`
    fn execute_store_wrap(
        &mut self,
        store: &mut StoreInner,
        memory: Memory,
        address: UntypedVal,
        offset: u32,
        value: UntypedVal,
        store_wrap: WasmStoreOp,
    ) -> Result<(), Error> {
        let memory = self.fetch_memory_bytes_mut(memory, store);
        store_wrap(memory, address, offset, value)?;
        Ok(())
    }

    /// Executes a generic Wasm `store[N]` operation for the default memory.
    ///
    /// # Note
    ///
    /// This can be used to emulate the following Wasm operands:
    ///
    /// - `{i32, i64, f32, f64}.store`
    /// - `{i32, i64}.store8`
    /// - `{i32, i64}.store16`
    /// - `i64.store32`
    fn execute_store_wrap_mem0(
        &mut self,
        address: UntypedVal,
        offset: u32,
        value: UntypedVal,
        store_wrap: WasmStoreOp,
    ) -> Result<(), Error> {
        let memory = self.fetch_default_memory_bytes_mut();
        store_wrap(memory, address, offset, value)?;
        Ok(())
    }

    fn execute_store(
        &mut self,
        store: &mut StoreInner,
        ptr: Reg,
        memory: Memory,
        store_op: WasmStoreOp,
    ) -> Result<(), Error> {
        let (value, offset) = self.fetch_value_and_offset();
        self.execute_store_wrap(
            store,
            memory,
            self.get_register(ptr),
            offset,
            self.get_register(value),
            store_op,
        )?;
        self.try_next_instr_at(2)
    }

    fn execute_store_imm<T>(
        &mut self,
        store: &mut StoreInner,
        ptr: Reg,
        memory: Memory,
        store_op: WasmStoreOp,
    ) -> Result<(), Error>
    where
        T: From<AnyConst16> + Into<UntypedVal>,
    {
        let (value, offset) = self.fetch_value_and_offset_imm::<T>();
        self.execute_store_wrap(
            store,
            memory,
            self.get_register(ptr),
            offset,
            value.into(),
            store_op,
        )?;
        self.try_next_instr_at(2)
    }

    fn execute_store_offset16(
        &mut self,
        ptr: Reg,
        offset: Const16<u32>,
        value: Reg,
        store_op: WasmStoreOp,
    ) -> Result<(), Error> {
        self.execute_store_wrap_mem0(
            self.get_register(ptr),
            u32::from(offset),
            self.get_register(value),
            store_op,
        )?;
        self.try_next_instr()
    }

    fn execute_store_offset16_imm16<T, V>(
        &mut self,
        ptr: Reg,
        offset: Const16<u32>,
        value: V,
        store_op: WasmStoreOp,
    ) -> Result<(), Error>
    where
        T: From<V> + Into<UntypedVal>,
    {
        self.execute_store_wrap_mem0(
            self.get_register(ptr),
            u32::from(offset),
            T::from(value).into(),
            store_op,
        )?;
        self.try_next_instr()
    }

    fn execute_store_at(
        &mut self,
        store: &mut StoreInner,
        address: u32,
        value: Reg,
        store_op: WasmStoreOp,
    ) -> Result<(), Error> {
        let memory = self.fetch_optional_memory();
        self.execute_store_wrap(
            store,
            memory,
            UntypedVal::from(0u32),
            address,
            self.get_register(value),
            store_op,
        )?;
        self.try_next_instr()
    }

    fn execute_store_at_imm16<T, V>(
        &mut self,
        store: &mut StoreInner,
        address: u32,
        value: V,
        store_op: WasmStoreOp,
    ) -> Result<(), Error>
    where
        T: From<V> + Into<UntypedVal>,
    {
        let memory = self.fetch_optional_memory();
        self.execute_store_wrap(
            store,
            memory,
            UntypedVal::from(0u32),
            address,
            T::from(value).into(),
            store_op,
        )?;
        self.try_next_instr()
    }
}

macro_rules! impl_execute_istore {
    ( $(
        (
            ($from_ty:ty => $to_ty:ty),
            (Instruction::$var_store_imm:ident, $fn_store_imm:ident),
            (Instruction::$var_store_off16_imm16:ident, $fn_store_off16_imm16:ident),
            (Instruction::$var_store_at_imm16:ident, $fn_store_at_imm16:ident),
            $impl_fn:expr $(,)?
        )
    ),* $(,)? ) => {
        $(
            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_imm), "`].")]
            pub fn $fn_store_imm(&mut self, store: &mut StoreInner, ptr: Reg, memory: Memory) -> Result<(), Error> {
                self.execute_store_imm::<$to_ty>(store, ptr, memory, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_off16_imm16), "`].")]
            pub fn $fn_store_off16_imm16(
                &mut self,
                ptr: Reg,
                offset: Const16<u32>,
                value: $from_ty,
            ) -> Result<(), Error> {
                self.execute_store_offset16_imm16::<$to_ty, _>(ptr, offset, value, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_at_imm16), "`].")]
            pub fn $fn_store_at_imm16(
                &mut self,
                store: &mut StoreInner,
                address: u32,
                value: $from_ty,
            ) -> Result<(), Error> {
                self.execute_store_at_imm16::<$to_ty, _>(store, address, value, $impl_fn)
            }
        )*
    };
}
impl Executor<'_> {
    impl_execute_istore! {
        (
            (Const16<i32> => i32),
            (Instruction::I32StoreImm16, i32_store_imm16),
            (Instruction::I32StoreOffset16Imm16, i32_store_offset16_imm16),
            (Instruction::I32StoreAtImm16, i32_store_at_imm16),
            UntypedVal::store32,
        ),
        (
            (Const16<i64> => i64),
            (Instruction::I64StoreImm16, i64_store_imm16),
            (Instruction::I64StoreOffset16Imm16, i64_store_offset16_imm16),
            (Instruction::I64StoreAtImm16, i64_store_at_imm16),
            UntypedVal::store64,
        ),
    }
}

macro_rules! impl_execute_istore_trunc {
    ( $(
        (
            ($from_ty:ty => $to_ty:ty),
            (Instruction::$var_store:ident, $fn_store:ident),
            (Instruction::$var_store_imm:ident, $fn_store_imm:ident),
            (Instruction::$var_store_off16:ident, $fn_store_off16:ident),
            (Instruction::$var_store_off16_imm16:ident, $fn_store_off16_imm16:ident),
            (Instruction::$var_store_at:ident, $fn_store_at:ident),
            (Instruction::$var_store_at_imm16:ident, $fn_store_at_imm16:ident),
            $impl_fn:expr $(,)?
        )
    ),* $(,)? ) => {
        $(
            impl_execute_istore! {
                (
                    ($from_ty => $to_ty),
                    (Instruction::$var_store_imm, $fn_store_imm),
                    (Instruction::$var_store_off16_imm16, $fn_store_off16_imm16),
                    (Instruction::$var_store_at_imm16, $fn_store_at_imm16),
                    $impl_fn,
                )
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store), "`].")]
            pub fn $fn_store(&mut self, store: &mut StoreInner, ptr: Reg, memory: Memory) -> Result<(), Error> {
                self.execute_store(store, ptr, memory, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_off16), "`].")]
            pub fn $fn_store_off16(
                &mut self,
                ptr: Reg,
                offset: Const16<u32>,
                value: Reg,
            ) -> Result<(), Error> {
                self.execute_store_offset16(ptr, offset, value, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_at), "`].")]
            pub fn $fn_store_at(&mut self, store: &mut StoreInner, address: u32, value: Reg) -> Result<(), Error> {
                self.execute_store_at(store, address, value, $impl_fn)
            }
        )*
    };
}
impl Executor<'_> {
    impl_execute_istore_trunc! {
        (
            (i8 => i8),
            (Instruction::I32Store8, i32_store8),
            (Instruction::I32Store8Imm, i32_store8_imm),
            (Instruction::I32Store8Offset16, i32_store8_offset16),
            (Instruction::I32Store8Offset16Imm, i32_store8_offset16_imm),
            (Instruction::I32Store8At, i32_store8_at),
            (Instruction::I32Store8AtImm, i32_store8_at_imm),
            UntypedVal::i32_store8,
        ),
        (
            (i16 => i16),
            (Instruction::I32Store16, i32_store16),
            (Instruction::I32Store16Imm, i32_store16_imm),
            (Instruction::I32Store16Offset16, i32_store16_offset16),
            (Instruction::I32Store16Offset16Imm, i32_store16_offset16_imm),
            (Instruction::I32Store16At, i32_store16_at),
            (Instruction::I32Store16AtImm, i32_store16_at_imm),
            UntypedVal::i32_store16,
        ),
        (
            (i8 => i8),
            (Instruction::I64Store8, i64_store8),
            (Instruction::I64Store8Imm, i64_store8_imm),
            (Instruction::I64Store8Offset16, i64_store8_offset16),
            (Instruction::I64Store8Offset16Imm, i64_store8_offset16_imm),
            (Instruction::I64Store8At, i64_store8_at),
            (Instruction::I64Store8AtImm, i64_store8_at_imm),
            UntypedVal::i64_store8,
        ),
        (
            (i16 => i16),
            (Instruction::I64Store16, i64_store16),
            (Instruction::I64Store16Imm, i64_store16_imm),
            (Instruction::I64Store16Offset16, i64_store16_offset16),
            (Instruction::I64Store16Offset16Imm, i64_store16_offset16_imm),
            (Instruction::I64Store16At, i64_store16_at),
            (Instruction::I64Store16AtImm, i64_store16_at_imm),
            UntypedVal::i64_store16,
        ),
        (
            (Const16<i32> => i32),
            (Instruction::I64Store32, i64_store32),
            (Instruction::I64Store32Imm16, i64_store32_imm16),
            (Instruction::I64Store32Offset16, i64_store32_offset16),
            (Instruction::I64Store32Offset16Imm16, i64_store32_offset16_imm16),
            (Instruction::I64Store32At, i64_store32_at),
            (Instruction::I64Store32AtImm16, i64_store32_at_imm16),
            UntypedVal::i64_store32,
        ),
    }
}

macro_rules! impl_execute_store {
    ( $(
        (
            (Instruction::$var_store:ident, $fn_store:ident),
            (Instruction::$var_store_off16:ident, $fn_store_off16:ident),
            (Instruction::$var_store_at:ident, $fn_store_at:ident),
            $impl_fn:expr $(,)?
        )
    ),* $(,)? ) => {
        $(
            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store), "`].")]
            pub fn $fn_store(&mut self, store: &mut StoreInner, ptr: Reg, memory: Memory) -> Result<(), Error> {
                self.execute_store(store, ptr, memory, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_off16), "`].")]
            pub fn $fn_store_off16(
                &mut self,
                ptr: Reg,
                offset: Const16<u32>,
                value: Reg,
            ) -> Result<(), Error> {
                self.execute_store_offset16(ptr, offset, value, $impl_fn)
            }

            #[doc = concat!("Executes an [`Instruction::", stringify!($var_store_at), "`].")]
            pub fn $fn_store_at(&mut self, store: &mut StoreInner,address: u32, value: Reg) -> Result<(), Error> {
                self.execute_store_at(store, address, value, $impl_fn)
            }
        )*
    }
}

impl Executor<'_> {
    impl_execute_store! {
        (
            (Instruction::Store32, store32),
            (Instruction::Store32Offset16, store32_offset16),
            (Instruction::Store32At, store32_at),
            UntypedVal::store32,
        ),
        (
            (Instruction::Store64, store64),
            (Instruction::Store64Offset16, store64_offset16),
            (Instruction::Store64At, store64_at),
            UntypedVal::store64,
        ),
    }
}
