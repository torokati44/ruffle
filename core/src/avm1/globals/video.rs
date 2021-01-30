//! Video class

use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::globals::display_object;
use crate::avm1::object::Object;
use crate::avm1::value::Value;
use crate::avm1::ScriptObject;
use gc_arena::MutationContext;

/// Implements `Video`
pub fn constructor<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    _this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(Value::Undefined)
}

pub fn create_proto<'gc>(
    gc_context: MutationContext<'gc, '_>,
    proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let object = ScriptObject::object(gc_context, Some(proto));

    display_object::define_display_object_proto(gc_context, object, fn_proto);

    object.into()
}
