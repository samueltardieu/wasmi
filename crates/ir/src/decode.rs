use crate::*;
use ::core::{fmt, hint, mem, num, slice};
use std::marker::PhantomData;

/// An error that might occur when decoding a byte stream.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum DecodeError {
    /// Signals that there are no more bytes in the byte stream.
    EndOfStream {
        /// The position within the byte stream when the error occurred.
        pos: usize,
    },
    /// Encountered when decoding an [`OpCode`] value with invalid bit pattern.
    InvalidOpCode {
        /// The position within the byte stream when the error occurred.
        pos: usize,
        /// The invalid byte value.
        value: u16,
    },
    /// Encountered when decoding an [`TrapCode`] value with invalid bit pattern.
    InvalidTrapCode {
        /// The position within the byte stream when the error occurred.
        pos: usize,
        /// The invalid byte value.
        value: u8,
    },
    /// Encountered when decoding a `bool` with an invalid bit pattern.
    InvalidBool {
        /// The position within the byte stream when the error occurred.
        pos: usize,
        /// The invalid byte value.
        value: u8,
    },
    /// Encountered when decoding a [`Sign`] with an invalid bit pattern.
    InvalidSign {
        /// The position within the byte stream when the error occurred.
        pos: usize,
        /// The invalid byte value.
        value: u8,
    },
    /// Returned when decoding a `NonZero` type with a zero value.
    InvalidNonZeroValue {
        /// The position within the byte stream when the error occurred.
        pos: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::EndOfStream { pos } => write!(f, "unexpected end of stream at {pos}"),
            DecodeError::InvalidOpCode { pos, value } => {
                write!(f, "invalid op-code at {pos}: {value}")
            }
            DecodeError::InvalidTrapCode { pos, value } => {
                write!(f, "invalid trap code at {pos}: {value}")
            }
            DecodeError::InvalidBool { pos, value } => {
                write!(f, "invalid bool value at {pos}: {value}")
            }
            DecodeError::InvalidSign { pos, value } => {
                write!(f, "invalid sign value at {pos}: {value}")
            }
            DecodeError::InvalidNonZeroValue { pos } => {
                write!(f, "invalid non-zero value at {pos}")
            }
        }
    }
}

/// Trait implemented by byte stream decoders.
pub trait Decoder<'op> {
    /// The error type that represents decoding errors for this [`Decoder`].
    type Error;

    /// Returned when decoding a `NonZero` type with invalid bit pattern.
    fn invalid_non_zero_value(&self) -> Self::Error;

    /// Returned when decoding a `bool` type with invalid bit pattern.
    fn invalid_bool(&self, value: u8) -> Self::Error;

    /// Returned when decoding a [`Sign`] type with invalid bit pattern.
    fn invalid_sign(&self, value: u8) -> Self::Error;

    /// Returned when decoding an [`OpCode`] with invalid bit pattern.
    fn invalid_op_code(&self, value: u16) -> Self::Error;

    /// Returned when decoding a [`TrapCode`] with invalid bit pattern.
    fn invalid_trap_code(&self, value: u8) -> Self::Error;

    /// Reads `N` bytes from the byte stream.
    ///
    /// # Errors
    ///
    /// If the byte stream ran out of enough bytes.
    fn read<const N: usize>(&mut self) -> Result<[u8; N], Self::Error>;

    /// Reads a byte slice of length `n` from the byte stream.
    ///
    /// # Errors
    ///
    /// If the byte stream ran out of enough bytes.
    fn read_slice(&mut self, n: usize) -> Result<&'op [u8], Self::Error>;
}

/// A safe implementation of a [`Decoder`].
#[derive(Debug, Clone)]
pub struct SafeDecoder<'op> {
    /// The bytes underlying to the [`SafeDecoder`].
    bytes: &'op [u8],
    /// The current position within the `bytes` slice.
    pos: usize,
}

impl<'op> SafeDecoder<'op> {
    pub fn new(bytes: &'op [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
}

impl<'op> Decoder<'op> for SafeDecoder<'op> {
    type Error = DecodeError;

    fn invalid_non_zero_value(&self) -> Self::Error {
        Self::Error::InvalidNonZeroValue { pos: self.pos }
    }

    fn invalid_bool(&self, value: u8) -> Self::Error {
        Self::Error::InvalidBool {
            pos: self.pos,
            value,
        }
    }

    fn invalid_sign(&self, value: u8) -> Self::Error {
        Self::Error::InvalidSign {
            pos: self.pos,
            value,
        }
    }

    fn invalid_op_code(&self, value: u16) -> Self::Error {
        Self::Error::InvalidOpCode {
            pos: self.pos,
            value,
        }
    }

    fn invalid_trap_code(&self, value: u8) -> Self::Error {
        Self::Error::InvalidTrapCode {
            pos: self.pos,
            value,
        }
    }

    fn read<const N: usize>(&mut self) -> Result<[u8; N], Self::Error> {
        let Some((chunk, rest)) = self.bytes.split_first_chunk::<N>() else {
            return Err(DecodeError::EndOfStream { pos: self.pos });
        };
        self.bytes = rest;
        self.pos += N;
        Ok(*chunk)
    }

    fn read_slice(&mut self, n: usize) -> Result<&'op [u8], Self::Error> {
        if self.bytes.len() < n {
            return Err(DecodeError::EndOfStream { pos: self.pos });
        }
        let (chunk, rest) = self.bytes.split_at(n);
        self.bytes = rest;
        self.pos += n;
        Ok(chunk)
    }
}

/// An unsafe and fast implementation of a [`Decoder`].
#[derive(Debug, Copy, Clone)]
#[repr(transparent)]
pub struct UnsafeDecoder {
    /// The underlying bytes of encoded data.
    ptr: *const u8,
}

impl UnsafeDecoder {
    /// Creates a new [`UnsafeDecoder`].
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to provide a `ptr` that points to valid encoded data.
    #[inline]
    pub unsafe fn new(ptr: *const u8) -> Self {
        assert!(!ptr.is_null());
        Self { ptr }
    }

    /// Offsets the underlying pointer of the [`UnsafeDecoder`] by `offset`.
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to provide an `offset` that makes
    /// the underlying pointer point to valid encoded data.
    #[inline]
    pub unsafe fn offset(&self, offset: isize) -> Self {
        Self {
            ptr: self.ptr.offset(offset),
        }
    }

    /// Returns the underlying pointer to encoded data.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }
}

/// Dummy error type that can never be constructed, similar to the unstable never type.
#[derive(Debug)]
pub enum NeverError {}

impl<'op> Decoder<'op> for UnsafeDecoder {
    type Error = NeverError;

    #[inline]
    fn invalid_non_zero_value(&self) -> Self::Error {
        unsafe { hint::unreachable_unchecked() }
    }

    #[inline]
    fn invalid_bool(&self, _value: u8) -> Self::Error {
        unsafe { hint::unreachable_unchecked() }
    }

    #[inline]
    fn invalid_sign(&self, _value: u8) -> Self::Error {
        unsafe { hint::unreachable_unchecked() }
    }

    #[inline]
    fn invalid_op_code(&self, _value: u16) -> Self::Error {
        unsafe { hint::unreachable_unchecked() }
    }

    #[inline]
    fn invalid_trap_code(&self, _value: u8) -> Self::Error {
        unsafe { hint::unreachable_unchecked() }
    }

    #[inline]
    fn read<const N: usize>(&mut self) -> Result<[u8; N], Self::Error> {
        debug_assert!(!self.ptr.is_null());
        let bytes = unsafe { self.ptr.cast::<[u8; N]>().read() };
        self.ptr = unsafe { self.ptr.add(N) };
        Ok(bytes)
    }

    #[inline]
    fn read_slice(&mut self, n: usize) -> Result<&'op [u8], Self::Error> {
        let ptr = self.ptr;
        self.ptr = unsafe { self.ptr.add(n) };
        let slice = unsafe { slice::from_raw_parts(ptr, n) };
        Ok(slice)
    }
}

/// Trait implemented by types that can be decoded via a [`Decoder`].
pub trait Decode<'a>: Sized {
    /// Decodes `Self` from a `decoder` byte stream.
    ///
    /// # Errors
    ///
    /// If the byte stream cannot be decoded into an instance of `Self`.
    fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
    where
        T: Decoder<'a>;
}

macro_rules! impl_decode_for_int {
    ( $($ty:ty),* ) => {
        $(
            impl<'a> Decode<'a> for $ty {
                fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
                where
                    T: Decoder<'a>,
                {
                    decoder.read::<{mem::size_of::<Self>()}>().map(<Self>::from_ne_bytes)
                }
            }
        )*
    };
}
impl_decode_for_int!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128);

impl<'a> Decode<'a> for bool {
    fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
    where
        T: Decoder<'a>,
    {
        let byte = decoder.read::<1>()?[0];
        match byte {
            0 => Ok(false),
            1 => Ok(true),
            value => Err(decoder.invalid_bool(value)),
        }
    }
}

macro_rules! impl_decode_for_non_zero {
    ( $( $nz_int:ty => $int:ty ),* $(,)? ) => {
        $(
            impl<'a> Decode<'a> for $nz_int {
                fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
                where
                    T: Decoder<'a>,
                {
                    let value = decoder
                        .read::<{mem::size_of::<Self>()}>()
                        .map(<$int>::from_ne_bytes)?;
                    Self::new(value).ok_or_else(|| decoder.invalid_non_zero_value())
                }
            }
        )*
    };
}
impl_decode_for_non_zero!(
    num::NonZeroU8 => u8,
    num::NonZeroI8 => i8,
    num::NonZeroU16 => u16,
    num::NonZeroI16 => i16,
    num::NonZeroU32 => u32,
    num::NonZeroI32 => i32,
    num::NonZeroU64 => u64,
    num::NonZeroI64 => i64,
    num::NonZeroU128 => u128,
    num::NonZeroI128 => i128,
);

macro_rules! impl_decode_for_newtype {
    (
        $(
            $( #[$docs:meta] )*
            struct $name:ident($vis:vis $ty:ty);
        )*
    ) => {
        $(
            impl<'a> Decode<'a> for $name {
                fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
                where
                    T: Decoder<'a>,
                {
                    <_ as Decode>::decode(decoder).map(Self)
                }
            }
        )*
    };
}
for_each_newtype!(impl_decode_for_newtype);

impl<'a> Decode<'a> for crate::RegSpan {
    fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
    where
        T: Decoder<'a>,
    {
        <_ as Decode>::decode(decoder).map(|head| Self { head })
    }
}

impl<'a> Decode<'a> for crate::BranchTableTarget {
    fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
    where
        T: Decoder<'a>,
    {
        <i32 as Decode>::decode(decoder).map(|value| Self { value })
    }
}

impl<'a, T> Decode<'a> for crate::Unalign<T>
where
    T: Decode<'a>,
{
    fn decode<D>(decoder: &mut D) -> Result<Self, D::Error>
    where
        D: Decoder<'a>,
    {
        T::decode(decoder).map(Self::from)
    }
}

impl<'a, T> Decode<'a> for crate::Slice<'a, T>
where
    T: Copy + Decode<'a>,
{
    fn decode<D>(decoder: &mut D) -> Result<Self, D::Error>
    where
        D: Decoder<'a>,
    {
        let len = u16::decode(decoder)?;
        let len_bytes = (len as usize).wrapping_mul(2);
        let bytes = decoder.read_slice(len_bytes)?;
        let data = bytes.as_ptr() as *const crate::Unalign<T>;
        Ok(Self {
            len,
            data,
            lt: PhantomData,
        })
    }
}

impl<'a> Decode<'a> for crate::Sign {
    fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
    where
        T: Decoder<'a>,
    {
        match decoder.read::<1>()?[0] {
            0 => Ok(Self::Pos),
            1 => Ok(Self::Neg),
            invalid => Err(decoder.invalid_sign(invalid)),
        }
    }
}

macro_rules! impl_decode_for_op_code {
    (
        $(
            $( #[doc = $doc:literal] )*
            #[snake_name($snake_name:ident)]
            $camel_name:ident $(<$lt:lifetime>)? $( {
                $(
                    $( #[$field_attr:meta ] )*
                    $field_ident:ident: $field_ty:ty
                ),* $(,)?
            } )?
        ),* $(,)?
    ) => {
        impl<'a> Decode<'a> for crate::OpCode {
            fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
            where
                T: Decoder<'a>,
            {
                /// Returns the maximum value of the `arr`.
                const fn max_array<const N: usize>(arr: &[u16; N]) -> u16 {
                    /// Returns the maximum value of `a` and `b`.
                    const fn max(a: u16, b: u16) -> u16 {
                        if a > b { a } else { b }
                    }

                    let mut m = 0;
                    let mut i = 0;
                    while i < N {
                        m = max(m, arr[i]);
                        i += 1;
                    }
                    m
                }

                /// Meta information about the `OpCode` enum.
                trait OpCodeInfo {
                    /// The underlying integer type of the `OpCode` enum discriminant.
                    type Repr;

                    /// The maximum discriminant value of all `OpCode` enum variants.
                    const MAX: Self::Repr;
                }

                impl OpCodeInfo for crate::OpCode {
                    type Repr = u16;

                    const MAX: Self::Repr = max_array(&[
                        $(
                            crate::OpCode::$camel_name as Self::Repr
                        ),*
                    ]);
                }

                let tag = <<Self as OpCodeInfo>::Repr>::from_ne_bytes(decoder.read()?);
                if tag > <Self as OpCodeInfo>::MAX {
                    return Err(decoder.invalid_op_code(tag))
                }
                // Safety: We defined `OpCode` without holes in its representation therefore if `tag`
                //         is within bounds of 0..MAX it is a valid `OpCode` and the below cast is safe.
                Ok(unsafe { ::core::mem::transmute::<<Self as OpCodeInfo>::Repr, Self>(tag) })
            }
        }
    };
}
for_each_op!(impl_decode_for_op_code);

macro_rules! impl_decode_for_ops {
    (
        $(
            $( #[doc = $doc:literal] )*
            #[snake_name($snake_name:ident)]
            $camel_name:ident $(<$lt:lifetime>)? $( {
                $(
                    $( #[$field_attr:meta ] )*
                    $field_ident:ident: $field_ty:ty
                ),* $(,)?
            } )?
        ),* $(,)?
    ) => {
        impl<'op> Decode<'op> for crate::Op<'op> {
            fn decode<T>(__decoder: &mut T) -> Result<Self, T::Error>
            where
                T: Decoder<'op>,
            {
                match crate::OpCode::decode(__decoder)? {
                    $(
                        crate::OpCode::$camel_name => {
                            <crate::op::$camel_name as Decode<'op>>::decode(__decoder).map(Self::from)
                        },
                    )*
                }
            }
        }

        $(
            impl<'op> Decode<'op> for crate::op::$camel_name $(<$lt>)? {
                fn decode<T>(__decoder: &mut T) -> Result<Self, T::Error>
                where
                    T: Decoder<'op>,
                {
                    Ok(crate::op::$camel_name {
                        $( $(
                            $field_ident: <$field_ty as Decode<'op>>::decode(__decoder)?,
                        )* )?
                    })
                }
            }
        )*
    };
}
for_each_op!(impl_decode_for_ops);

macro_rules! impl_decode_for_trap_code {
    ( $($name:ident),* $(,)? ) => {
        impl<'op> Decode<'op> for crate::TrapCode {
            fn decode<T>(decoder: &mut T) -> Result<Self, T::Error>
            where
                T: Decoder<'op>,
            {
                #[allow(non_upper_case_globals)]
                trait TrapCodeConsts {
                    type Repr;

                    $( const $name: Self::Repr; )*
                }

                impl TrapCodeConsts for crate::TrapCode {
                    type Repr = u8;

                    $( const $name: Self::Repr = crate::TrapCode::$name as Self::Repr; )*
                }

                let tag = <<Self as TrapCodeConsts>::Repr>::from_ne_bytes(decoder.read()?);
                match tag {
                    $(
                        <Self as TrapCodeConsts>::$name => Ok(Self::$name),
                    )*
                    invalid => Err(decoder.invalid_trap_code(invalid)),
                }
            }
        }
    }
}
for_each_trap_code!(impl_decode_for_trap_code);

/// An implementation of a safe [`Op`] decoder.
#[derive(Debug, Clone)]
pub struct SafeOpDecoder<'op>(pub(crate) SafeDecoder<'op>);

impl<'op> SafeOpDecoder<'op> {
    /// Creates a new [`SafeOpDecoder`] from the given byte slice.
    pub fn new(bytes: &'op [u8]) -> Self {
        Self(SafeDecoder::new(bytes))
    }
}

impl<'op> SafeOpDecoder<'op> {
    /// Decode the next [`Op`] from `self`.
    ///
    /// # Errors
    ///
    /// If an [`Op`] cannot be decoded from the underlying bytes in `self`.
    pub fn decode(&mut self) -> Result<Op<'op>, DecodeError> {
        <Op as Decode<'op>>::decode(&mut self.0)
    }
}

/// An implementation of a fast but unsafe [`Op`] decoder.
#[derive(Debug, Clone)]
pub struct UnsafeOpDecoder(pub(crate) UnsafeDecoder);

impl UnsafeOpDecoder {
    /// Creates a new [`UnsafeOpDecoder`] from the given pointer to bytes.
    pub fn new(ptr: *const u8) -> Self {
        // We disable the warning here since it is a false-positive clippy warnings as described in the linked issue:
        // <https://github.com/rust-lang/rust-clippy/issues/3045#issuecomment-1437288944>
        // TODO: remove this silencing of the warning when the issue has been fixed.
        #![allow(clippy::not_unsafe_ptr_arg_deref)]
        // Safety: Creating an instance of an [`UnsafeOpDecoder`] isn't unsafe
        //         since its decode methods are instead marked as `unsafe` which
        //         makes more sense since the decoding is where undefined behavior
        //         might occur when using this abstraction improperly.
        //
        // Unfortuantely the same rules cannot be applied to `UnsafeDecoder`'s API
        // since it implements `Decoder` which offers a safe API and thus at least
        // its constructor has to be marked as `unsafe` to indicate to users that
        // the underlying API is actually unsafe to use.
        Self(unsafe { UnsafeDecoder::new(ptr) })
    }
}

impl<'op> UnsafeOpDecoder {
    /// Decode the next [`Op`] from `self`.
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to ensure that the bytes underlying
    /// to the [`UnsafeOpDecoder`] can safely be decoded as [`Op`].
    #[inline]
    pub unsafe fn decode(&mut self) -> Op<'op> {
        <Op as Decode<'op>>::decode(&mut self.0).unwrap_unchecked()
    }

    /// Offsets the position within the [`UnsafeOpDecoder`] by `offset` bytes.
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to ensure that the new byte position
    /// contains safely decodable [`Op`] items and does not cross boundaries of
    /// encoded [`Op`]s.
    #[inline]
    pub unsafe fn offset(&self, offset: isize) -> Self {
        Self(self.0.offset(offset))
    }

    /// Returns the underlying byte pointer of the [`UnsafeOpDecoder`].
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }
}