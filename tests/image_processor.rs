use crop_picture::image_processor::{ImageProcessError, LoadedImage, MIN_DIMENSION};
use image::{GenericImageView, Rgba, RgbaImage};
use tempfile::tempdir;

#[test]
fn test_image_load_and_save_formats() {
    let dir = tempdir().unwrap();
    let src_path = dir.path().join("source.png");

    // 1500 x 1200 のテスト画像を生成して保存
    let mut test_img = RgbaImage::new(1500, 1200);
    for (x, y, pixel) in test_img.enumerate_pixels_mut() {
        *pixel = Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255]);
    }
    test_img.save(&src_path).unwrap();

    // 読み込みテスト
    let loaded = LoadedImage::load_from_file(&src_path).expect("画像読み込み成功");
    assert_eq!(loaded.width, 1500);
    assert_eq!(loaded.height, 1200);
    assert_eq!(loaded.preview_width, 1500);
    assert_eq!(loaded.preview_height, 1200);

    // PNG 保存テスト (正方形 1000x1000, 等倍 1.0)
    let out_png = dir.path().join("out.png");
    loaded.save_crop(100, 100, 1000, 1000, 1.0, &out_png).expect("PNG 保存成功");
    let saved_png = image::open(&out_png).unwrap();
    assert_eq!(saved_png.dimensions(), (1000, 1000));

    // JPEG 保存テスト (長方形 1200x630, 拡大 1.25 -> 狭い範囲を1200x630に拡大保存)
    let out_jpg = dir.path().join("out.jpg");
    loaded.save_crop(100, 50, 1200, 630, 1.25, &out_jpg).expect("JPEG 保存成功");
    let saved_jpg = image::open(&out_jpg).unwrap();
    assert_eq!(saved_jpg.dimensions(), (1200, 630));

    // WebP 保存テスト (長方形 800x600, 縮小 0.8 -> 広い範囲を800x600に縮小保存)
    let out_webp = dir.path().join("out.webp");
    loaded.save_crop(0, 0, 800, 600, 0.8, &out_webp).expect("WebP 保存成功");
    let saved_webp = image::open(&out_webp).unwrap();
    assert_eq!(saved_webp.dimensions(), (800, 600));

    // 不正な拡張子エラーの検証
    let out_invalid = dir.path().join("out.txt");
    assert!(loaded.save_crop(0, 0, 1000, 1000, 1.0, &out_invalid).is_err());
}

#[test]
fn test_reject_small_image_file() {
    let dir = tempdir().unwrap();
    let small_path = dir.path().join("small.png");

    let test_img = RgbaImage::new(MIN_DIMENSION - 1, MIN_DIMENSION);
    test_img.save(&small_path).unwrap();

    let res = LoadedImage::load_from_file(&small_path);
    assert!(matches!(res, Err(ImageProcessError::ResolutionTooSmall { .. })));
}
