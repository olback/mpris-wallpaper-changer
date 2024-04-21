use {
    fast_image_resize as fr,
    fastblur::gaussian_blur,
    fr::pixels::PixelExt,
    image::{imageops, RgbImage},
    std::num::NonZeroU32,
};

pub fn image_background(
    background: &RgbImage,
    display_geometry: [u32; 2],
    blur_radius: Option<u32>,
) -> RgbImage {
    if let Some(blur) = blur_radius {
        add_blur(
            background,
            display_geometry[0],
            display_geometry[1],
            blur as f32,
        )
    } else {
        fast_resize(background, display_geometry[0], display_geometry[1])
    }
}

fn add_blur(image: &RgbImage, nwidth: u32, nheight: u32, blur_radius: f32) -> RgbImage {
    // Downsize the image by a factor of `scale` for faster blurring and upscale back
    // to display_geometry for final image

    let scale = 4;
    let (scaled_width, scaled_height) = (nwidth / scale, nheight / scale);
    let downscaled_image = fast_resize(image, scaled_width, scaled_height);

    let mut pixels: Vec<[u8; 3]> = downscaled_image
        .into_raw()
        .chunks_exact(3)
        .map(|pixel| pixel.try_into().unwrap())
        .collect();

    gaussian_blur(
        &mut pixels,
        scaled_width as usize,
        scaled_height as usize,
        blur_radius / scale as f32,
    );

    let buf = pixels.into_iter().flatten().collect();
    let blurred_image = RgbImage::from_raw(scaled_width, scaled_height, buf).unwrap();
    fast_resize(&blurred_image, nwidth, nheight)
}

pub fn fast_resize(img: &RgbImage, nwidth: u32, nheight: u32) -> RgbImage {
    let width = NonZeroU32::new(img.width()).unwrap();
    let height = NonZeroU32::new(img.height()).unwrap();
    let src_image =
        fr::Image::from_vec_u8(width, height, img.as_raw().to_vec(), fr::PixelType::U8x3).unwrap();
    let resized_image = resize_image_with_cropping(
        src_image.view(),
        NonZeroU32::new(nwidth).unwrap(),
        NonZeroU32::new(nheight).unwrap(),
    );
    RgbImage::from_raw(nwidth, nheight, resized_image.into_vec()).unwrap()
}

fn resize_image_with_cropping(
    mut src_view: fr::DynamicImageView,
    dst_width: NonZeroU32,
    dst_height: NonZeroU32,
) -> fr::Image {
    // Set cropping parameters
    src_view.set_crop_box_to_fit_dst_size(dst_width, dst_height, None);

    // Create container for data of destination image
    let mut dst_image = fr::Image::new(dst_width, dst_height, src_view.pixel_type());
    // Get mutable view of destination image data
    let mut dst_view = dst_image.view_mut();

    // Create Resizer instance and resize source image
    // into buffer of destination image
    let mut resizer = fr::Resizer::new(fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3));
    resizer.resize(&src_view, &mut dst_view).unwrap();

    dst_image
}

pub fn paste_images(
    background: &RgbImage,
    foreground: &RgbImage,
    display_geometry: [u32; 2],
    available_geometry: [u32; 4],
) -> RgbImage {
    let mut base = RgbImage::new(display_geometry[0], display_geometry[1]);

    // Background Paste
    let x = (display_geometry[0] as i64 - background.width() as i64) / 2;
    let y = (display_geometry[1] as i64 - background.height() as i64) / 2;

    imageops::overlay(&mut base, background, x, y);

    // Foreground paste

    let x = (i64::from(available_geometry[0]) - i64::from(foreground.width())) / 2
        + i64::from(available_geometry[2]);

    let y = (i64::from(available_geometry[1]) - i64::from(foreground.height())) / 2
        + i64::from(available_geometry[3]);

    imageops::overlay(&mut base, foreground, x, y);

    base
}
