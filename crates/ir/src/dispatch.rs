use crate::{decode::Decode, for_each_op, Code, Op, OpCode, UncheckedOpDecoder};

impl UncheckedOpDecoder {
    /// Dispatches for the next [`Op`] from `self`.
    ///
    /// The caller is supposed to use the returned [`OpCode`] to dispatch the correct handler
    /// and to use the returned [`UncheckedOpVariantDecoder`] to decode the dispatched [`Op`] in
    /// the body of the handler.
    ///
    /// # Safety
    ///
    /// - It is the caller's responsibility to ensure that the bytes underlying
    ///   to the [`UncheckedOpDecoder`] can safely be decoded as [`Op`].
    /// - It is the caller's responsibility to use the returned [`UncheckedOpVariantDecoder`]
    ///   to decode the correct [`Op`] variant associated to the returned [`OpCode`].
    #[inline]
    pub unsafe fn dispatch(&mut self) -> (OpCode, UncheckedOpVariantDecoder) {
        let code = OpCode::decode(&mut self.0).unwrap_unchecked();
        let decoder = UncheckedOpVariantDecoder(self);
        (code, decoder)
    }
}

/// Marker trait implemented by all [`Op`] sub-variants.
pub trait OpVariant<'op>: Into<Op<'op>> + Code + Decode<'op> + private::Sealed {}

mod private {
    pub trait Sealed {}
}

macro_rules! impl_op_variant {
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
        $(
            impl<'op> OpVariant<'op> for crate::op::$camel_name $(<$lt>)? {}
            impl$(<$lt>)? private::Sealed for crate::op::$camel_name $(<$lt>)? {}
        )*
    }
}
for_each_op!(impl_op_variant);

/// An implementation of a fast but unsafe [`Op`] variant decoder.
#[derive(Debug)]
pub struct UncheckedOpVariantDecoder<'decoder>(pub(crate) &'decoder mut UncheckedOpDecoder);

impl<'decoder, 'op> UncheckedOpVariantDecoder<'decoder> {
    /// Decode the next `T:`[`OpVariant`] from `self`.
    ///
    /// Returns the decoded `T` as well as the underlying [`UncheckedOpDecoder`] which
    /// can then be used to dispatch the next [`Op`] via [`UncheckedOpDecoder::dispatch`].
    ///
    /// # Safety
    ///
    /// It is the callers responsibility to ensure that the bytes underlying
    /// to the [`UncheckedOpVariantDecoder`] can safely be decoded as `T`.
    #[inline]
    pub unsafe fn decode<T: OpVariant<'op>>(self) -> (&'decoder mut UncheckedOpDecoder, T) {
        let op = <T as Decode<'op>>::decode(&mut self.0 .0).unwrap_unchecked();
        (self.0, op)
    }
}
