pub use serde;

pub trait SerializeWithContext {
    type Context;
    fn serialize<S: serde::Serializer>(
        &self,
        context: &Self::Context,
        serializer: S,
    ) -> Result<S::Ok, S::Error>;
}

pub struct ContextWrapper<'a, T: SerializeWithContext> {
    base: &'a T,
    context: &'a T::Context,
}

impl<T: SerializeWithContext> serde::Serialize for ContextWrapper<'_, T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.base.serialize(self.context, serializer)
    }
}

pub trait SerializerExt {
    type Error;
    fn serialize_field_with_context<T: SerializeWithContext>(
        &mut self,
        name: &'static str,
        base: &T,
        context: &T::Context,
    ) -> Result<(), Self::Error>;
}

impl<S: serde::ser::SerializeStruct> SerializerExt for S {
    type Error = S::Error;
    fn serialize_field_with_context<T: SerializeWithContext>(
        &mut self,
        name: &'static str,
        base: &T,
        context: &T::Context,
    ) -> Result<(), Self::Error> {
        self.serialize_field(name, &ContextWrapper { base, context })
    }
}

#[cfg(feature = "derive")]
pub use serde_context_derive::SerializeWithContext;
