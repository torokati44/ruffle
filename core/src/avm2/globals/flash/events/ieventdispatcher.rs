//! `flash.events.IEventDispatcher` builtin

use crate::avm2::activation::Activation;
use crate::avm2::class::{Class, ClassAttributes};
use crate::avm2::method::{Method, NativeMethod};
use crate::avm2::names::{Namespace, QName};
use crate::avm2::object::Object;
use crate::avm2::value::Value;
use crate::avm2::Error;
use gc_arena::{GcCell, MutationContext};

/// Emulates attempts to execute bodiless methods.
pub fn bodiless_method<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    _this: Option<Object<'gc>>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error> {
    Err("Cannot execute non-native method without body".into())
}

/// Implements `flash.events.IEventDispatcher`'s class constructor.
pub fn class_init<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    _this: Option<Object<'gc>>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error> {
    Ok(Value::Undefined)
}

/// Construct `IEventDispatcher`'s class.
pub fn create_interface<'gc>(mc: MutationContext<'gc, '_>) -> GcCell<'gc, Class<'gc>> {
    let class = Class::new(
        QName::new(Namespace::package("flash.events"), "IEventDispatcher"),
        None,
        Method::from_builtin(bodiless_method),
        Method::from_builtin(class_init),
        mc,
    );

    let mut write = class.write(mc);

    write.set_attributes(ClassAttributes::INTERFACE);

    const PUBLIC_INSTANCE_METHODS: &[(&str, NativeMethod)] = &[
        ("addEventListener", bodiless_method),
        ("dispatchEvent", bodiless_method),
        ("hasEventListener", bodiless_method),
        ("removeEventListener", bodiless_method),
        ("willTrigger", bodiless_method),
    ];
    write.define_public_builtin_instance_methods(PUBLIC_INSTANCE_METHODS);

    class
}
