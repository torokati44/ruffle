use crate::avm1::object::bitmap_data::{BitmapData, Color};

fn convolve(
    source_bitmap: &BitmapData,
    dest_bitmap: &mut BitmapData,
    src_rect: (i32, i32, i32, i32),
    dest_point: (i32, i32),
    radius: usize,
    kernel: &Vec<f64>,
    delta: (i32, i32)
) {
    let (src_min_x, src_min_y, src_width, src_height) = src_rect;
    let r = radius as i32;
    for y in src_min_x..(src_min_y + src_height) {
        for x in src_min_x..(src_min_x + src_width) {
            let (mut new_r, mut new_g, mut new_b, mut new_a) = (0.0, 0.0, 0.0, 0.0);

            for (i, coeff) in kernel.iter().enumerate() {

                let xadj = x + (i as i32 - r) * delta.0;
                let yadj = y + (i as i32 - r) * delta.1;

                if source_bitmap.is_point_in_bounds(xadj, yadj) {
                    let s = source_bitmap.get_pixel_raw(xadj as _, yadj as _).unwrap();
                    new_r += s.red() as f64 * coeff;
                    new_g += s.green() as f64 * coeff;
                    new_b += s.blue() as f64 * coeff;
                    new_a += s.alpha() as f64 * coeff;
                }
            }

            if dest_bitmap.is_point_in_bounds(x - src_min_x + dest_point.0, y - src_min_y + dest_point.1) {
                dest_bitmap.set_pixel32_raw((x - src_min_x + dest_point.0) as u32, (y - src_min_y + dest_point.1) as u32, Color::argb(new_a as u8, new_r as u8, new_g as u8, new_b as u8))
            }
        }
    }
}


fn convolve_x(
    source_bitmap: &BitmapData,
    dest_bitmap: &mut BitmapData,
    src_rect: (i32, i32, i32, i32),
    dest_point: (i32, i32),
    radius: usize,
    kernel: &Vec<f64>,
) {
    convolve(
        source_bitmap,
        dest_bitmap,
        src_rect,
        dest_point,
        radius,
        kernel,
        (1, 0)
    )
}


fn convolve_y(
    source_bitmap: &BitmapData,
    dest_bitmap: &mut BitmapData,
    src_rect: (i32, i32, i32, i32),
    dest_point: (i32, i32),
    radius: usize,
    kernel: &Vec<f64>,
) {
    convolve(
        source_bitmap,
        dest_bitmap,
        src_rect,
        dest_point,
        radius,
        kernel,
        (0, 1)
    )
}

fn make_kernel(strength: f64) -> (i32, Vec<f64>) {
    // this is an integer, telling how many _additional_ pixels are gathered _on each side_
    let radius = ((strength - 1.0) / 2.0).ceil() as i32;

    let kernel_size = radius as usize * 2 + 1;
    let mut coeffs = vec![1.0 / strength; kernel_size];

    let edges = (strength / 2.0 - 0.5).fract() / strength;
    if edges != 0.0 {
        coeffs[0] = edges;
        coeffs[kernel_size-1] = edges;
    }

    assert!((&coeffs.iter().fold(0.0, |a, b| a+b) - 1.0).abs() < 0.0001);

    (radius, coeffs)
}

pub fn apply_blur(
    self_bitmap: &mut BitmapData,
    source_bitmap: &mut BitmapData,
    src_rect: (i32, i32, i32, i32),
    dest_point: (i32, i32),
    quality: i32,
    blur_x: f64,
    blur_y: f64,
) {
    // TODO: quality 0 or both blur params <=1 -> only copy
    let (radius_x, kernel_x) = make_kernel(blur_x);
    let (radius_y, kernel_y) = make_kernel(blur_y);


    let (src_min_x, src_min_y, src_width, src_height) = src_rect;

    let mut temp = BitmapData::default();
    temp.init_pixels(
        src_width as u32 + 2 * radius_x as u32,
        src_height as u32 + 2 * radius_y as u32,
        self_bitmap.transparency(),
        0,
    );

    // x, source -> temp
    convolve_x(source_bitmap, &mut temp, src_rect, (radius_x, radius_y), radius_x as usize, &kernel_x);

    if quality > 1 {
        let mut temp2 = temp.clone();

        for _iter in 1..quality {
            //y, temp -> temp2
            convolve_y(&temp, &mut temp2, (0, 0, src_width + 2*radius_x, src_height + 2*radius_y), (0, 0), radius_y as usize, &kernel_y);
            //x, temp2 -> temp
            convolve_x(&temp2, &mut temp, (0, 0, src_width + 2*radius_x , src_height + 2*radius_y), (0, 0), radius_x as usize, &kernel_x);
        }
    }

    // y, temp -> dest
    convolve_y(&temp, self_bitmap, (0, 0, src_width+2*radius_x, src_height+2*radius_y), (dest_point.0-radius_x, dest_point.1-radius_y), radius_y as usize, &kernel_y);

}
