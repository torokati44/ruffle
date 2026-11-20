use crate::avm2::Error;
use crate::avm2::activation::Activation;
use crate::avm2::object::script_object::ScriptObjectData;
use crate::avm2::object::{ClassObject, Object, TObject};
use crate::avm2::value::Hint;
use crate::context::UpdateContext;
use core::fmt;
use gc_arena::{Collect, Gc, GcWeak};
use ruffle_common::utils::HasPrefixField;
use std::cell::Cell;

/// A class instance allocator that allocates Date objects.
pub fn date_allocator<'gc>(
    class: ClassObject<'gc>,
    activation: &mut Activation<'_, 'gc>,
) -> Result<Object<'gc>, Error<'gc>> {
    Ok(DateObject(Gc::new(
        activation.gc(),
        DateObjectData {
            base: ScriptObjectData::new(class),
            time: Cell::new(None),
        },
    ))
    .into())
}
#[derive(Clone, Collect, Copy)]
#[collect(no_drop)]
pub struct DateObject<'gc>(pub Gc<'gc, DateObjectData<'gc>>);

#[derive(Clone, Collect, Copy, Debug)]
#[collect(no_drop)]
pub struct DateObjectWeak<'gc>(pub GcWeak<'gc, DateObjectData<'gc>>);

impl fmt::Debug for DateObject<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DateObject")
            .field("ptr", &Gc::as_ptr(self.0))
            .finish()
    }
}

/// The largest distance from the epoch that a valid date can have, in milliseconds.
const MAX_TIME: u64 = 8_640_000_000_000_000;

/// ECMA-262 TimeClip - Returns `time` (in milliseconds since the Unix epoch)
/// if it's within the range of valid dates.
pub fn clip_time(time: i64) -> Option<i64> {
    (time.unsigned_abs() <= MAX_TIME).then_some(time)
}

impl<'gc> DateObject<'gc> {
    /// Creates a `Date` holding `time` (see [`Self::time`]).
    pub fn from_time(context: &mut UpdateContext<'gc>, time: Option<i64>) -> Object<'gc> {
        let class = context.avm2.classes().date;
        let base = ScriptObjectData::new(class);

        DateObject(Gc::new(
            context.gc(),
            DateObjectData {
                base,
                time: Cell::new(time),
            },
        ))
        .into()
    }

    pub fn for_prototype(
        context: &mut UpdateContext<'gc>,
        date_class: ClassObject<'gc>,
    ) -> Object<'gc> {
        let object_class = context.avm2.classes().object;
        let base = ScriptObjectData::custom_new(
            date_class.inner_class_definition(),
            Some(object_class.prototype()),
            date_class.instance_vtable(),
        );

        let instance: Object<'gc> = DateObject(Gc::new(
            context.gc(),
            DateObjectData {
                base,
                time: Cell::new(None),
            },
        ))
        .into();

        instance
    }

    /// The time in milliseconds since the Unix epoch, or `None` for an invalid date.
    pub fn time(self) -> Option<i64> {
        self.0.time.get()
    }

    pub fn set_time(self, time: Option<i64>) {
        self.0.time.set(time);
    }
}

#[derive(Clone, Collect, HasPrefixField)]
#[collect(no_drop)]
#[repr(C, align(8))]
pub struct DateObjectData<'gc> {
    /// Base script object
    base: ScriptObjectData<'gc>,

    /// Milliseconds since the Unix epoch, or `None` for an invalid date.
    time: Cell<Option<i64>>,
}

impl<'gc> TObject<'gc> for DateObject<'gc> {
    fn gc_base(&self) -> Gc<'gc, ScriptObjectData<'gc>> {
        HasPrefixField::as_prefix_gc(self.0)
    }

    fn default_hint(&self) -> Hint {
        Hint::String
    }
}
