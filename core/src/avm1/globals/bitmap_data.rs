//! flash.display.BitmapData object

use crate::avm1::error::Error;
use crate::avm1::function::{Executable, FunctionObject};
use crate::avm1::object::bitmap_data::{BitmapDataObject, ChannelOptions, Color};
use crate::avm1::property::Attribute;
use crate::avm1::{activation::Activation, object::bitmap_data::BitmapData};
use crate::avm1::{Object, TObject, Value};
use crate::character::Character;
use crate::display_object::TDisplayObject;
use gc_arena::{GcCell, MutationContext};

pub fn constructor<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let width = args
        .get(0)
        .unwrap_or(&Value::Number(0.0))
        .coerce_to_i32(activation)?;

    let height = args
        .get(1)
        .unwrap_or(&Value::Number(0.0))
        .coerce_to_i32(activation)?;

    if width > 2880 || height > 2880 || width <= 0 || height <= 0 {
        log::warn!("Invalid BitmapData size {}x{}", width, height);
        return Ok(Value::Undefined);
    }

    let transparency = args
        .get(2)
        .unwrap_or(&Value::Bool(true))
        .as_bool(activation.current_swf_version());

    let fill_color = args
        .get(3)
        // can't write this in hex
        // 0xFFFFFFFF as f64;
        .unwrap_or(&Value::Number(4294967295_f64))
        .coerce_to_i32(activation)?;

    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        bitmap_data
            .bitmap_data()
            .write(activation.context.gc_context)
            .init_pixels(width as u32, height as u32, fill_color, transparency);
    }

    Ok(this.into())
}

pub fn height<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            return Ok(bitmap_data.bitmap_data().read().height().into());
        }
    }

    Ok((-1).into())
}

pub fn width<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            return Ok(bitmap_data.bitmap_data().read().width().into());
        }
    }

    Ok((-1).into())
}

pub fn get_transparent<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            return Ok(bitmap_data.bitmap_data().read().transparency().into());
        }
    }

    Ok((-1).into())
}

pub fn get_rectangle<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let proto = activation.context.avm1.prototypes.rectangle_constructor;
            let rect = proto.construct(
                activation,
                &[
                    0.into(),
                    0.into(),
                    bitmap_data.bitmap_data().read().width().into(),
                    bitmap_data.bitmap_data().read().height().into(),
                ],
            )?;
            return Ok(rect);
        }
    }

    Ok((-1).into())
}

pub fn get_pixel<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let (Some(x_val), Some(y_val)) = (args.get(0), args.get(1)) {
                let x = x_val.coerce_to_i32(activation)?;
                let y = y_val.coerce_to_i32(activation)?;
                return Ok(bitmap_data.bitmap_data().read().get_pixel(x, y).into());
            }
        }
    }

    Ok((-1).into())
}

pub fn get_pixel32<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let (Some(x_val), Some(y_val)) = (args.get(0), args.get(1)) {
                let x = x_val.coerce_to_i32(activation)?;
                let y = y_val.coerce_to_i32(activation)?;
                let col: i32 = bitmap_data.bitmap_data().read().get_pixel32(x, y).into();
                return Ok(col.into());
            }
        }
    }

    Ok((-1).into())
}

pub fn set_pixel<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let (Some(x_val), Some(y_val), Some(color_val)) =
                (args.get(0), args.get(1), args.get(2))
            {
                let x = x_val.coerce_to_u32(activation)?;
                let y = y_val.coerce_to_u32(activation)?;
                let color = color_val.coerce_to_i32(activation)?;

                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .set_pixel(x, y, color.into());

                return Ok(Value::Undefined);
            }
        }
    }

    Ok((-1).into())
}

pub fn set_pixel32<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let (Some(x_val), Some(y_val), Some(color_val)) =
                (args.get(0), args.get(1), args.get(2))
            {
                let x = x_val.coerce_to_i32(activation)?;
                let y = y_val.coerce_to_i32(activation)?;
                let color = color_val.coerce_to_i32(activation)?;

                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .set_pixel32(x, y, color.into());
            }

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn copy_channel<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let source_bitmap = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_object(activation);

    let source_rect = args
        .get(1)
        .unwrap_or(&Value::Undefined)
        .coerce_to_object(activation);

    let dest_point = args
        .get(2)
        .unwrap_or(&Value::Undefined)
        .coerce_to_object(activation);

    let source_channel = args
        .get(3)
        .unwrap_or(&Value::Undefined)
        .coerce_to_i32(activation)?;

    let dest_channel = args
        .get(4)
        .unwrap_or(&Value::Undefined)
        .coerce_to_i32(activation)?;

    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let Some(source_bitmap) = source_bitmap.as_bitmap_data_object() {
                //TODO: what if source is disposed
                let min_x = dest_point
                    .get("x", activation)?
                    .coerce_to_u32(activation)?
                    .min(bitmap_data.bitmap_data().read().width());
                let min_y = dest_point
                    .get("y", activation)?
                    .coerce_to_u32(activation)?
                    .min(bitmap_data.bitmap_data().read().height());

                let src_min_x = source_rect
                    .get("x", activation)?
                    .coerce_to_u32(activation)?;
                let src_min_y = source_rect
                    .get("y", activation)?
                    .coerce_to_u32(activation)?;
                let src_width = source_rect
                    .get("width", activation)?
                    .coerce_to_u32(activation)?;
                let src_height = source_rect
                    .get("height", activation)?
                    .coerce_to_u32(activation)?;
                let src_max_x = src_min_x + src_width;
                let src_max_y = src_min_y + src_height;

                let src_bitmap_data = source_bitmap.bitmap_data();

                if GcCell::ptr_eq(bitmap_data.bitmap_data(), src_bitmap_data) {
                    let src_bitmap_data_clone = src_bitmap_data.read().clone();
                    bitmap_data
                        .bitmap_data()
                        .write(activation.context.gc_context)
                        .copy_channel(
                            (min_x, min_y),
                            (src_min_x, src_min_y, src_max_x, src_max_y),
                            &src_bitmap_data_clone,
                            source_channel,
                            dest_channel,
                        );
                } else {
                    bitmap_data
                        .bitmap_data()
                        .write(activation.context.gc_context)
                        .copy_channel(
                            (min_x, min_y),
                            (src_min_x, src_min_y, src_max_x, src_max_y),
                            &src_bitmap_data.read(),
                            source_channel,
                            dest_channel,
                        );
                }
            }

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn fill_rect<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let rectangle = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_object(activation);

    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let Some(color_val) = args.get(1) {
                let color = color_val.coerce_to_i32(activation)?;

                let x = rectangle.get("x", activation)?.coerce_to_u32(activation)?;
                let y = rectangle.get("y", activation)?.coerce_to_u32(activation)?;
                let width = rectangle
                    .get("width", activation)?
                    .coerce_to_u32(activation)?;
                let height = rectangle
                    .get("height", activation)?
                    .coerce_to_u32(activation)?;

                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .fill_rect(x, y, width, height, color.into());
            }
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn clone<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let proto = activation.context.avm1.prototypes.bitmap_data_constructor;
            let new_bitmap_data = proto.construct(
                activation,
                &[
                    bitmap_data.bitmap_data().read().width().into(),
                    bitmap_data.bitmap_data().read().height().into(),
                    bitmap_data.bitmap_data().read().transparency().into(),
                    0xFFFFFF.into(),
                ],
            )?;
            let new_bitmap_data_object = new_bitmap_data
                .coerce_to_object(activation)
                .as_bitmap_data_object()
                .unwrap();

            new_bitmap_data_object
                .bitmap_data()
                .write(activation.context.gc_context)
                .set_pixels(bitmap_data.bitmap_data().read().pixels().to_vec());

            return Ok(new_bitmap_data);
        }
    }

    Ok((-1).into())
}

pub fn dispose<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            bitmap_data.dispose(activation.context.gc_context);
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn flood_fill<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let (Some(x_val), Some(y_val), Some(color_val)) =
                (args.get(0), args.get(1), args.get(2))
            {
                let x = x_val.coerce_to_u32(activation)?;
                let y = y_val.coerce_to_u32(activation)?;
                let color = color_val.coerce_to_i32(activation)?;

                let color: Color = color.into();
                let color: Color =
                    color.to_premultiplied_alpha(bitmap_data.bitmap_data().read().transparency());

                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .flood_fill(x, y, color);
            }
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn noise<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let low = args
        .get(1)
        .unwrap_or(&Value::Number(0.0))
        .coerce_to_u32(activation)? as u8;

    let high = args
        .get(2)
        .unwrap_or(&Value::Number(255.0))
        .coerce_to_u32(activation)? as u8;

    let channel_options = args
        .get(3)
        .unwrap_or(&Value::Number(ChannelOptions::rgb().0 as f64))
        .coerce_to_u32(activation)?;

    let gray_scale = args
        .get(4)
        .unwrap_or(&Value::Bool(false))
        .as_bool(activation.current_swf_version());

    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            if let Some(random_seed_val) = args.get(0) {
                let random_seed = random_seed_val.coerce_to_i32(activation)?;
                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .noise(
                        random_seed,
                        low,
                        high.max(low),
                        channel_options.into(),
                        gray_scale,
                    )
            }

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn apply_filter<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            //public applyFilter(sourceBitmap:BitmapData, sourceRect:Rectangle,   destPoint:Point, filter:BitmapFilter) : Number

            let source_bitmap = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let source_rect = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let src_min_x = source_rect
                .get("x", activation)?
                .coerce_to_i32(activation)?;
            let src_min_y = source_rect
                .get("y", activation)?
                .coerce_to_i32(activation)?;
            let src_width = source_rect
                .get("width", activation)?
                .coerce_to_i32(activation)?;
            let src_height = source_rect
                .get("height", activation)?
                .coerce_to_i32(activation)?;

            let dest_point = args
                .get(2)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let dest_x = dest_point.get("x", activation)?.coerce_to_i32(activation)?;
            let dest_y = dest_point.get("y", activation)?.coerce_to_i32(activation)?;

            if let Some(src_bitmap) = source_bitmap.as_bitmap_data_object() {
                if !src_bitmap.disposed() {
                    let mut src_clone;
                    {
                        // needed to avoid aliasing if src == dest
                        src_clone = src_bitmap.bitmap_data().read().clone();
                    }

                    let obj = args
                        .get(3)
                        .unwrap_or(&Value::Undefined)
                        .coerce_to_object(activation);

                    match obj {
                        Object::BlurFilterObject(bfd) => {
                            log::warn!("issa blur");
                            let bfo = bfd.as_blur_filter_object().unwrap();
                            /*
                            bitmap_data
                                .bitmap_data()
                                .write(activation.context.gc_context)
                                .apply_blur(
                                    &mut src_clone,
                                    (src_min_x, src_min_y, src_width, src_height),
                                    (dest_x, dest_y),
                                    bfo.get_quality(),
                                    bfo.get_blur_x(),
                                    bfo.get_blur_y(),
                                );*/
                        }

                        Object::BevelFilterObject(bfd) => {
                            log::warn!("issa bevel");
                        }
                        Object::GlowFilterObject(bfd) => {
                            log::warn!("issa glow");
                        }
                        Object::DropShadowFilterObject(bfd) => {
                            log::warn!("issa dropped shadow");
                        }
                        _ => {
                            log::warn!("issa smth else");
                            /*
                            // STUB
                            bitmap_data
                                .bitmap_data()
                                .write(activation.context.gc_context)
                                .copy_pixels(
                                    &src_clone,
                                    (src_min_x, src_min_y, src_width, src_height),
                                    (dest_x, dest_y),
                                );
                                */
                        }
                    };
                }
            }

            return Ok(Value::Number(0.0));
        }
    }

    Ok((-1).into())
}

pub fn draw<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.draw - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn generate_filter_rect<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.generateFilterRect - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn color_transform<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let rectangle = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let color_transform = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let x = rectangle.get("x", activation)?.coerce_to_i32(activation)?;
            let y = rectangle.get("y", activation)?.coerce_to_i32(activation)?;
            let width = rectangle
                .get("width", activation)?
                .coerce_to_i32(activation)?;
            let height = rectangle
                .get("height", activation)?
                .coerce_to_i32(activation)?;

            let min_x = x.max(0) as u32;
            let end_x = (x + width) as u32;
            let min_y = y.max(0) as u32;
            let end_y = (y + height) as u32;

            if let Some(color_transform) = color_transform.as_color_transform_object() {
                bitmap_data
                    .bitmap_data()
                    .write(activation.context.gc_context)
                    .color_transform(min_x, min_y, end_x, end_y, color_transform);
            }

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn get_color_bounds_rect<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let find_color = args
                .get(2)
                .unwrap_or(&Value::Bool(true))
                .as_bool(activation.current_swf_version());

            if let (Some(mask_val), Some(color_val)) = (args.get(0), args.get(1)) {
                let mask = mask_val.coerce_to_i32(activation)?;
                let color = color_val.coerce_to_i32(activation)?;

                let (x, y, w, h) = bitmap_data
                    .bitmap_data()
                    .read()
                    .color_bounds_rect(find_color, mask, color);

                let proto = activation.context.avm1.prototypes.rectangle_constructor;
                let rect =
                    proto.construct(activation, &[x.into(), y.into(), w.into(), h.into()])?;
                return Ok(rect);
            }
        }
    }

    Ok((-1).into())
}

pub fn perlin_noise<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let base_x = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_f64(activation)?;
            let base_y = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_f64(activation)?;
            let num_octaves = args
                .get(2)
                .unwrap_or(&Value::Undefined)
                .coerce_to_u32(activation)? as usize;
            let seed = args
                .get(3)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)? as i64;
            let stitch = args
                .get(4)
                .unwrap_or(&Value::Undefined)
                .as_bool(activation.swf_version());
            let fractal_noise = args
                .get(5)
                .unwrap_or(&Value::Undefined)
                .as_bool(activation.swf_version());
            let channel_options = args
                .get(6)
                .unwrap_or(&Value::Number((1 | 2 | 4) as f64))
                .coerce_to_u16(activation)? as u8;
            let grayscale = args
                .get(7)
                .unwrap_or(&Value::Undefined)
                .as_bool(activation.swf_version());
            let offsets = args
                .get(8)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let mut octave_offsets = vec![];
            for i in 0..num_octaves {
                octave_offsets.push(if let Value::Object(e) = offsets.array_element(i) {
                    let x = e.get("x", activation)?.coerce_to_f64(activation)?;
                    let y = e.get("y", activation)?.coerce_to_f64(activation)?;
                    (x, y)
                } else {
                    (0.0, 0.0)
                });
            }

            bitmap_data
                .bitmap_data()
                .write(activation.context.gc_context)
                .perlin_noise(
                    (base_x, base_y),
                    num_octaves,
                    seed,
                    stitch,
                    fractal_noise,
                    channel_options,
                    grayscale,
                    octave_offsets,
                );
        }
    }

    Ok((-1).into())
}

pub fn hit_test<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.hitTest - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn copy_pixels<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let source_bitmap = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let source_rect = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let src_min_x = source_rect
                .get("x", activation)?
                .coerce_to_i32(activation)?;
            let src_min_y = source_rect
                .get("y", activation)?
                .coerce_to_i32(activation)?;
            let src_width = source_rect
                .get("width", activation)?
                .coerce_to_i32(activation)?;
            let src_height = source_rect
                .get("height", activation)?
                .coerce_to_i32(activation)?;

            let dest_point = args
                .get(2)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let dest_x = dest_point.get("x", activation)?.coerce_to_i32(activation)?;
            let dest_y = dest_point.get("y", activation)?.coerce_to_i32(activation)?;

            if let Some(src_bitmap) = source_bitmap.as_bitmap_data_object() {
                if !src_bitmap.disposed() {
                    // dealing with object aliasing...
                    let src_bitmap_clone: BitmapData; // only initialized if source is the same object as self
                    let src_bitmap_data_cell = src_bitmap.bitmap_data();
                    let src_bitmap_gc_ref; // only initialized if source is a different object than self
                    let source_bitmap_ref = // holds the reference to either of the ones above
                        if GcCell::ptr_eq(src_bitmap.bitmap_data(), bitmap_data.bitmap_data()) {
                            src_bitmap_clone = src_bitmap_data_cell.read().clone();
                            &src_bitmap_clone
                        } else {
                            src_bitmap_gc_ref = src_bitmap_data_cell.read();
                            &src_bitmap_gc_ref
                        };

                    if args.len() >= 5 {
                        let alpha_point = args
                            .get(4)
                            .unwrap_or(&Value::Undefined)
                            .coerce_to_object(activation);

                        let alpha_x = alpha_point
                            .get("x", activation)?
                            .coerce_to_i32(activation)?;

                        let alpha_y = alpha_point
                            .get("y", activation)?
                            .coerce_to_i32(activation)?;

                        let alpha_bitmap = args
                            .get(3)
                            .unwrap_or(&Value::Undefined)
                            .coerce_to_object(activation);

                        if let Some(alpha_bitmap) = alpha_bitmap.as_bitmap_data_object() {
                            if !alpha_bitmap.disposed() {
                                // dealing with aliasing the same way as for the source
                                let alpha_bitmap_clone: BitmapData;
                                let alpha_bitmap_data_cell = alpha_bitmap.bitmap_data();
                                let alpha_bitmap_gc_ref;
                                let alpha_bitmap_ref = if GcCell::ptr_eq(
                                    alpha_bitmap.bitmap_data(),
                                    bitmap_data.bitmap_data(),
                                ) {
                                    alpha_bitmap_clone = alpha_bitmap_data_cell.read().clone();
                                    &alpha_bitmap_clone
                                } else {
                                    alpha_bitmap_gc_ref = alpha_bitmap_data_cell.read();
                                    &alpha_bitmap_gc_ref
                                };

                                let merge_alpha = if args.len() >= 6 {
                                    args.get(5)
                                        .unwrap_or(&Value::Undefined)
                                        .as_bool(activation.swf_version())
                                } else {
                                    true
                                };

                                bitmap_data
                                    .bitmap_data()
                                    .write(activation.context.gc_context)
                                    .copy_pixels(
                                        source_bitmap_ref,
                                        (src_min_x, src_min_y, src_width, src_height),
                                        (dest_x, dest_y),
                                        Some((alpha_bitmap_ref, (alpha_x, alpha_y), merge_alpha)),
                                    );
                            }
                        }
                    } else {
                        bitmap_data
                            .bitmap_data()
                            .write(activation.context.gc_context)
                            .copy_pixels(
                                source_bitmap_ref,
                                (src_min_x, src_min_y, src_width, src_height),
                                (dest_x, dest_y),
                                None,
                            );
                    }
                }
            }

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn merge<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let source_bitmap = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let source_rect = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let dest_point = args
                .get(2)
                .unwrap_or(&Value::Undefined)
                .coerce_to_object(activation);

            let red_mult = args
                .get(3)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;

            let green_mult = args
                .get(4)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;

            let blue_mult = args
                .get(5)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;

            let alpha_mult = args
                .get(6)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;
            /*
                            if let Some(src_bitmap) = source_bitmap.as_bitmap_data_object() {
                                if !src_bitmap.disposed() {

                                    bitmap_data
                                        .bitmap_data()
                                        .write(activation.context.gc_context)
                                        .merge(src_bitmap.bitmap_data());
                                }
                            }
            */
        }
    }

    Ok(Value::Undefined)
}

pub fn palette_map<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.paletteMap - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn pixel_dissolve<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.pixelDissolve - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn scroll<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            let x = args
                .get(0)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;
            let y = args
                .get(1)
                .unwrap_or(&Value::Undefined)
                .coerce_to_i32(activation)?;

            bitmap_data
                .bitmap_data()
                .write(activation.context.gc_context)
                .scroll(x, y);

            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn threshold<'gc>(
    _activation: &mut Activation<'_, 'gc, '_>,
    this: Object<'gc>,
    _args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    if let Some(bitmap_data) = this.as_bitmap_data_object() {
        if !bitmap_data.disposed() {
            log::warn!("BitmapData.threshold - not yet implemented");
            return Ok(Value::Undefined);
        }
    }

    Ok((-1).into())
}

pub fn create_proto<'gc>(
    gc_context: MutationContext<'gc, '_>,
    proto: Object<'gc>,
    fn_proto: Object<'gc>,
) -> Object<'gc> {
    let bitmap_data_object = BitmapDataObject::empty_object(gc_context, Some(proto));
    let mut object = bitmap_data_object.as_script_object().unwrap();

    object.add_property(
        gc_context,
        "height",
        FunctionObject::function(
            gc_context,
            Executable::Native(height),
            Some(fn_proto),
            fn_proto,
        ),
        None,
        Attribute::empty(),
    );

    object.add_property(
        gc_context,
        "width",
        FunctionObject::function(
            gc_context,
            Executable::Native(width),
            Some(fn_proto),
            fn_proto,
        ),
        None,
        Attribute::empty(),
    );

    object.add_property(
        gc_context,
        "transparent",
        FunctionObject::function(
            gc_context,
            Executable::Native(get_transparent),
            Some(fn_proto),
            fn_proto,
        ),
        None,
        Attribute::empty(),
    );

    object.add_property(
        gc_context,
        "rectangle",
        FunctionObject::function(
            gc_context,
            Executable::Native(get_rectangle),
            Some(fn_proto),
            fn_proto,
        ),
        None,
        Attribute::empty(),
    );

    object.force_set_function(
        "getPixel",
        get_pixel,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "getPixel32",
        get_pixel32,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "setPixel",
        set_pixel,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "setPixel32",
        set_pixel32,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "copyChannel",
        copy_channel,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "fillRect",
        fill_rect,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "clone",
        clone,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "dispose",
        dispose,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "floodFill",
        flood_fill,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "noise",
        noise,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "colorTransform",
        color_transform,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "getColorBoundsRect",
        get_color_bounds_rect,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "perlinNoise",
        perlin_noise,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "applyFilter",
        apply_filter,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function("draw", draw, gc_context, Attribute::empty(), Some(fn_proto));
    object.force_set_function(
        "hitTest",
        hit_test,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "generateFilterRect",
        generate_filter_rect,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "copyPixels",
        copy_pixels,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "merge",
        merge,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "paletteMap",
        palette_map,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "pixelDissolve",
        pixel_dissolve,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "scroll",
        scroll,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );
    object.force_set_function(
        "threshold",
        threshold,
        gc_context,
        Attribute::empty(),
        Some(fn_proto),
    );

    bitmap_data_object.into()
}

pub fn load_bitmap<'gc>(
    activation: &mut Activation<'_, 'gc, '_>,
    _this: Object<'gc>,
    args: &[Value<'gc>],
) -> Result<Value<'gc>, Error<'gc>> {
    let name = args
        .get(0)
        .unwrap_or(&Value::Undefined)
        .coerce_to_string(activation)?;

    let library = &*activation.context.library;

    let movie = activation.target_clip_or_root()?.movie();

    let renderer = &mut activation.context.renderer;

    let character = movie
        .and_then(|m| library.library_for_movie(m))
        .and_then(|l| l.character_by_export_name(name.as_str()));

    if let Some(Character::Bitmap(bitmap_object)) = character {
        if let Some(bitmap) = renderer.get_bitmap_pixels(bitmap_object.bitmap_handle()) {
            let proto = activation.context.avm1.prototypes.bitmap_data_constructor;
            let new_bitmap =
                proto.construct(activation, &[bitmap.width.into(), bitmap.height.into()])?;
            let new_bitmap_object = new_bitmap
                .coerce_to_object(activation)
                .as_bitmap_data_object()
                .unwrap();

            let pixels: Vec<i32> = bitmap.data.into();

            new_bitmap_object
                .bitmap_data()
                .write(activation.context.gc_context)
                .set_pixels(pixels.into_iter().map(|p| p.into()).collect());

            return Ok(new_bitmap);
        }
    }

    Ok(Value::Undefined)
}

pub fn create_bitmap_data_object<'gc>(
    gc_context: MutationContext<'gc, '_>,
    bitmap_data_proto: Object<'gc>,
    fn_proto: Option<Object<'gc>>,
) -> Object<'gc> {
    let object = FunctionObject::constructor(
        gc_context,
        Executable::Native(constructor),
        constructor_to_fn!(constructor),
        fn_proto,
        bitmap_data_proto,
    );
    let mut script_object = object.as_script_object().unwrap();

    script_object.force_set_function(
        "loadBitmap",
        load_bitmap,
        gc_context,
        Attribute::empty(),
        fn_proto,
    );

    object
}
