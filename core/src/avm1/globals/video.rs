//! Video class

use ruffle_macros::istr;

use crate::avm1::Object;
use crate::avm1::activation::Activation;
use crate::avm1::error::Error;
use crate::avm1::function::{FunctionObject, NativeFunction};
use crate::avm1::object::NativeObject;
use crate::avm1::property::Attribute;
use crate::avm1::property_decl::{DeclContext, PropertyOrder, StaticDeclarations, SystemClass};
use crate::avm1::value::Value;
use crate::display_object::Video;

macro_rules! video_method {
    ( $fn: expr ) => {
        |activation, this, args| {
            if let Some(display_object) = this.as_display_object() {
                if let Some(video) = display_object.as_video() {
                    return $fn(video, activation, args);
                }
            }
            Ok(Value::Undefined)
        }
    };
}

const PROTO_DECLS: StaticDeclarations = declare_static_properties! {
    "attachVideo" => method(video_method!(attach_video); DONT_ENUM | DONT_DELETE | VERSION_6);
};

pub fn create_class<'gc>(
    context: &mut DeclContext<'_, 'gc>,
    super_proto: Object<'gc>,
) -> SystemClass<'gc> {
    let class = context.empty_class(super_proto, PropertyOrder::PrototypeFirst);
    context.define_properties_on(class.proto, PROTO_DECLS(context));
    class
}

/// Defines `width`, `height`, `smoothing` and `deblocking` on `proto`.
///
/// Flash Player doesn't define these on `Video.prototype` up front, but only when
/// the first Video display object is created, and only on the prototype that Video
/// gets. They are not defined again for later Videos, even if they were deleted.
pub fn define_lazy_proto_properties<'gc>(activation: &mut Activation<'_, 'gc>, proto: Object<'gc>) {
    let fn_proto = activation.resolve_prototype([istr!("Function")]);
    let properties: [(_, NativeFunction, Option<NativeFunction>); 4] = [
        (istr!("width"), video_method!(width), None),
        (istr!("height"), video_method!(height), None),
        (
            istr!("smoothing"),
            video_method!(smoothing),
            Some(video_method!(set_smoothing)),
        ),
        (
            istr!("deblocking"),
            video_method!(deblocking),
            Some(video_method!(set_deblocking)),
        ),
    ];

    for (name, getter, setter) in properties {
        let getter = FunctionObject::native(getter).build(activation.strings(), fn_proto, None);
        let setter = setter.map(|setter| {
            FunctionObject::native(setter).build(activation.strings(), fn_proto, None)
        });
        proto.add_property(
            activation.gc(),
            name.into(),
            getter,
            setter,
            Attribute::VERSION_6,
        );
    }
}

pub fn attach_video<'gc>(
    video: Video<'gc>,
    activation: &mut Activation<'_, 'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let source = args
        .get(0)
        .cloned()
        .unwrap_or(Value::Undefined)
        .coerce_to_object_or_bare(activation)?;

    if let NativeObject::NetStream(ns) = source.native() {
        video.attach_netstream(activation.context, ns);
    } else {
        tracing::warn!("Cannot use object of type {:?} as video source", source);
    }

    Ok(Value::Undefined)
}

fn width<'gc>(
    video: Video<'gc>,
    _activation: &mut Activation<'_, 'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(video.decoded_size().0.into())
}

fn height<'gc>(
    video: Video<'gc>,
    _activation: &mut Activation<'_, 'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(video.decoded_size().1.into())
}

fn smoothing<'gc>(
    video: Video<'gc>,
    _activation: &mut Activation<'_, 'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(video.smoothing().into())
}

fn set_smoothing<'gc>(
    video: Video<'gc>,
    activation: &mut Activation<'_, 'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let smoothing = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .as_bool(activation.swf_version());
    video.set_smoothing(smoothing);
    Ok(Value::Undefined)
}

fn deblocking<'gc>(
    video: Video<'gc>,
    _activation: &mut Activation<'_, 'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    Ok(video.deblocking().into())
}

fn set_deblocking<'gc>(
    video: Video<'gc>,
    activation: &mut Activation<'_, 'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let value = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_f64(activation)?;
    // Values outside of the i32 range (including NaN) become 0.
    let deblocking = if (0.0..2_147_483_648.0).contains(&value) {
        (value as i32).min(7) as u8
    } else {
        0
    };
    video.set_deblocking(deblocking);
    Ok(Value::Undefined)
}
