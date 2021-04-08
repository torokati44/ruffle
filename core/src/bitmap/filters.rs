use crate::avm1::object::bitmap_data::{BitmapData, Color};


pub fn apply_blur(
    self_bitmap : &mut BitmapData,
    source_bitmap: &mut BitmapData,
    src_rect: (i32, i32, i32, i32),
    dest_point: (i32, i32),
    quality: i32,
    blur_x: f64,
    blur_y: f64,
) {
    let blur_x_odd = (blur_x / 2.0).floor() as i32 * 2 + 1;
    let blur_y_odd = (blur_y / 2.0).floor() as i32 * 2 + 1;

    let (src_min_x, src_min_y, src_width, src_height) = src_rect;

    let mut temp = source_bitmap.clone();
    let tw = temp.width() as i32;

    for _iter in 0..quality {
        // x, source -> temp
        for y in src_min_x..(src_min_y + src_height) {
            for x in src_min_x..(src_min_x + src_width) {
                if temp.is_point_in_bounds(x as i32, y as i32) {
                    let mut r = 0i32;
                    let mut g = 0i32;
                    let mut b = 0i32;
                    let mut a = 0i32;

                    for ix in x as i32 - (blur_x_odd / 2)..x as i32 + (blur_x_odd / 2) + 1 {
                        if source_bitmap.is_point_in_bounds(ix as i32, y as i32) {
                            let source_color =
                                temp.pixels.get((ix + y as i32 * tw) as usize).unwrap();
                            r += source_color.red() as i32;
                            g += source_color.green() as i32;
                            b += source_color.blue() as i32;
                            a += source_color.alpha() as i32;
                        }
                    }

                    r /= blur_x_odd;
                    g /= blur_x_odd;
                    b /= blur_x_odd;
                    a /= blur_x_odd;

                    temp.set_pixel32_raw(
                        x as u32,
                        y as u32,
                        Color::argb(a as u8, r as u8, g as u8, b as u8),
                    );
                }
            }
        }

        // y, temp -> source
        for y in src_min_x..(src_min_y + src_height) {
            for x in src_min_x..(src_min_x + src_width) {
                if source_bitmap.is_point_in_bounds(x as i32, y as i32) {
                    let mut r = 0i32;
                    let mut g = 0i32;
                    let mut b = 0i32;
                    let mut a = 0i32;

                    for iy in y as i32 - (blur_y_odd / 2)..y as i32 + (blur_y_odd / 2) + 1 {
                        if temp.is_point_in_bounds(x as i32, iy as i32) {
                            let source_color =
                                temp.pixels.get((x as i32 + iy * tw) as usize).unwrap();
                            r += source_color.red() as i32;
                            g += source_color.green() as i32;
                            b += source_color.blue() as i32;
                            a += source_color.alpha() as i32;
                        }
                    }

                    r /= blur_y_odd;
                    g /= blur_y_odd;
                    b /= blur_y_odd;
                    a /= blur_y_odd;

                    source_bitmap.set_pixel32_raw(
                        x as u32,
                        y as u32,
                        Color::argb(a as u8, r as u8, g as u8, b as u8),
                    );
                }
            }
        }
    }

    self_bitmap.copy_pixels(source_bitmap, src_rect, dest_point, None);
}
