//! 画像のデコード、EXIF Orientation 適用、プレビュー生成、1000x1000 切り抜き、エンコードを行うバックエンドモジュール。

use image::imageops::FilterType;
use image::metadata::Orientation;
use image::{
    DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader,
    Limits, RgbaImage,
};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub const MAX_PIXELS: u64 = 100_000_000; // 1億画素
pub const MIN_DIMENSION: u32 = 100;
pub const MAX_PREVIEW_DIMENSION: u32 = 4096;

#[derive(Debug)]
pub enum ImageProcessError {
    IoError(std::io::Error),
    ImageError(image::ImageError),
    UnsupportedFormat(String),
    ResolutionTooSmall { width: u32, height: u32 },
    ImageTooLarge { pixels: u64 },
    InvalidExtension(String),
}

impl std::fmt::Display for ImageProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "ファイル入出力エラー: {}", e),
            Self::ImageError(e) => write!(f, "画像処理エラー: {}", e),
            Self::UnsupportedFormat(ext) => write!(f, "非対応の画像フォーマットです: {}", ext),
            Self::ResolutionTooSmall { width, height } => write!(
                f,
                "画像の解像度が不足しています（100px以上必要、現在: {}x{}px）",
                width, height
            ),
            Self::ImageTooLarge { pixels } => write!(
                f,
                "画像が大きすぎます（最大1億画素、現在: {}画素）",
                pixels
            ),
            Self::InvalidExtension(ext) => write!(
                f,
                "保存形式に対応していない拡張子です（.png, .jpg, .jpeg, .webp のみ対応）: {}",
                ext
            ),
        }
    }
}

impl std::error::Error for ImageProcessError {}

impl From<std::io::Error> for ImageProcessError {
    fn from(err: std::io::Error) -> Self {
        Self::IoError(err)
    }
}

impl From<image::ImageError> for ImageProcessError {
    fn from(err: image::ImageError) -> Self {
        Self::ImageError(err)
    }
}

/// 読み込み済みの画像データ構造体
pub struct LoadedImage {
    /// EXIF Orientation 適用後のフル解像度画像
    pub full_image: DynamicImage,
    /// 幅
    pub width: u32,
    /// 高さ
    pub height: u32,
    /// 表示用プレビュー（長辺 4096px 以下、RGBA8）
    pub preview_rgba: RgbaImage,
    pub preview_width: u32,
    pub preview_height: u32,
}

impl LoadedImage {
    /// ファイルパスから画像を読み込み、Orientation 適用とプレビュー生成を行う
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, ImageProcessError> {
        let path = path.as_ref();
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);

        let mut image_reader = ImageReader::new(&mut reader);
        let mut limits = Limits::default();
        limits.max_image_width = Some(20_000);
        limits.max_image_height = Some(20_000);
        limits.max_alloc = Some(500 * 1024 * 1024); // 500MB
        image_reader.limits(limits);

        let image_reader = image_reader.with_guessed_format()?;
        let format = image_reader
            .format()
            .ok_or_else(|| {
                ImageProcessError::UnsupportedFormat(
                    path.extension()
                        .and_then(|s| s.to_str())
                        .unwrap_or("不明")
                        .to_string(),
                )
            })?;

        // 対応フォーマット検証 (JPEG, PNG, WebP, GIF, BMP, TIFF)
        match format {
            ImageFormat::Jpeg
            | ImageFormat::Png
            | ImageFormat::WebP
            | ImageFormat::Gif
            | ImageFormat::Bmp
            | ImageFormat::Tiff => {}
            _ => {
                return Err(ImageProcessError::UnsupportedFormat(format!("{:?}", format)));
            }
        }

        let mut decoder = image_reader.into_decoder()?;
        let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
        let mut img = DynamicImage::from_decoder(decoder)?;

        // EXIF Orientation の適用
        img.apply_orientation(orientation);

        let (width, height) = img.dimensions();
        let pixels = width as u64 * height as u64;

        if pixels > MAX_PIXELS {
            return Err(ImageProcessError::ImageTooLarge { pixels });
        }

        if width < MIN_DIMENSION || height < MIN_DIMENSION {
            return Err(ImageProcessError::ResolutionTooSmall { width, height });
        }

        // 表示用プレビューテクスチャの生成
        let (preview_rgba, preview_width, preview_height) = {
            let max_dim = width.max(height);
            if max_dim > MAX_PREVIEW_DIMENSION {
                let scale = MAX_PREVIEW_DIMENSION as f64 / max_dim as f64;
                let pw = ((width as f64 * scale).round() as u32).max(1);
                let ph = ((height as f64 * scale).round() as u32).max(1);
                let scaled = img.resize(pw, ph, FilterType::Triangle);
                let (w, h) = scaled.dimensions();
                (scaled.to_rgba8(), w, h)
            } else {
                (img.to_rgba8(), width, height)
            }
        };

        Ok(Self {
            full_image: img,
            width,
            height,
            preview_rgba,
            preview_width,
            preview_height,
        })
    }

    /// 指定された位置 (x, y)、切り抜きサイズ (crop_w, crop_h)、および画像倍率から画像を切り抜く
    pub fn crop_with_scale(
        &self,
        x: u32,
        y: u32,
        crop_width: u32,
        crop_height: u32,
        image_scale: f64,
    ) -> DynamicImage {
        let scale = if image_scale <= 0.0 { 1.0 } else { image_scale };
        let cw = crop_width.max(1);
        let ch = crop_height.max(1);

        // 元画像上での座標とサイズ
        let ox = ((x as f64) / scale).round() as u32;
        let oy = ((y as f64) / scale).round() as u32;
        let ow = ((cw as f64) / scale).round() as u32;
        let oh = ((ch as f64) / scale).round() as u32;

        let ox = ox.min(self.width.saturating_sub(1));
        let oy = oy.min(self.height.saturating_sub(1));
        let ow = ow.min(self.width - ox).max(1);
        let oh = oh.min(self.height - oy).max(1);

        let cropped = self.full_image.crop_imm(ox, oy, ow, oh);

        if ow == cw && oh == ch {
            cropped
        } else {
            cropped.resize_exact(cw, ch, FilterType::Lanczos3)
        }
    }

    /// 切り抜いた画像をファイルへ保存する
    pub fn save_crop<P: AsRef<Path>>(
        &self,
        x: u32,
        y: u32,
        crop_width: u32,
        crop_height: u32,
        image_scale: f64,
        save_path: P,
    ) -> Result<(), ImageProcessError> {
        let save_path = save_path.as_ref();
        let ext = save_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let cropped = self.crop_with_scale(x, y, crop_width, crop_height, image_scale);
        let cw = crop_width.max(1);
        let ch = crop_height.max(1);

        match ext.as_str() {
            "png" => {
                cropped.save_with_format(save_path, ImageFormat::Png)?;
            }
            "jpg" | "jpeg" => {
                // JPEG の場合はアルファを白背景に合成
                let rgb_img = if cropped.color().has_alpha() {
                    let rgba = cropped.to_rgba8();
                    let mut rgb = image::RgbImage::new(cw, ch);
                    for (x_coord, y_coord, pixel) in rgba.enumerate_pixels() {
                        let a = pixel[3] as f32 / 255.0;
                        let r = ((pixel[0] as f32 * a) + 255.0 * (1.0 - a)).round() as u8;
                        let g = ((pixel[1] as f32 * a) + 255.0 * (1.0 - a)).round() as u8;
                        let b = ((pixel[2] as f32 * a) + 255.0 * (1.0 - a)).round() as u8;
                        rgb.put_pixel(x_coord, y_coord, image::Rgb([r, g, b]));
                    }
                    DynamicImage::ImageRgb8(rgb)
                } else {
                    DynamicImage::ImageRgb8(cropped.to_rgb8())
                };

                let mut file = File::create(save_path)?;
                let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut file, 90);
                encoder.encode_image(&rgb_img)?;
            }
            "webp" => {
                let rgba = cropped.to_rgba8();
                let file = File::create(save_path)?;
                let encoder = image::codecs::webp::WebPEncoder::new_lossless(file);
                encoder.encode(
                    rgba.as_raw(),
                    cw,
                    ch,
                    image::ExtendedColorType::Rgba8,
                )?;
            }
            _ => {
                return Err(ImageProcessError::InvalidExtension(ext));
            }
        }

        Ok(())
    }
}
