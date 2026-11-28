//! App-icoon: uit één afbeelding alle formaten voor Android, iOS en web genereren.
//! (Adaptive icons en de splash-screen laat Studio door Claude opzetten met de passende packages.)

use crate::error::{AppError, AppResult};
use image::imageops::FilterType;
use image::{DynamicImage, Rgba, RgbaImage};
use serde::Serialize;
use std::path::Path;

const MAX_SOURCE_BYTES: u64 = 25 * 1024 * 1024;

#[derive(Debug, Serialize, Default, PartialEq)]
pub struct IconReport {
    pub written: Vec<String>,
    pub warnings: Vec<String>,
}

const ANDROID: [(&str, u32); 5] = [("mdpi", 48), ("hdpi", 72), ("xhdpi", 96), ("xxhdpi", 144), ("xxxhdpi", 192)];

/// (naam zoals `flutter create` die gebruikt, pixels)
const IOS: [(&str, u32); 15] = [
    ("Icon-App-20x20@1x", 20),
    ("Icon-App-20x20@2x", 40),
    ("Icon-App-20x20@3x", 60),
    ("Icon-App-29x29@1x", 29),
    ("Icon-App-29x29@2x", 58),
    ("Icon-App-29x29@3x", 87),
    ("Icon-App-40x40@1x", 40),
    ("Icon-App-40x40@2x", 80),
    ("Icon-App-40x40@3x", 120),
    ("Icon-App-60x60@2x", 120),
    ("Icon-App-60x60@3x", 180),
    ("Icon-App-76x76@1x", 76),
    ("Icon-App-76x76@2x", 152),
    ("Icon-App-83.5x83.5@2x", 167),
    ("Icon-App-1024x1024@1x", 1024),
];

const WEB: [(&str, u32); 4] = [("Icon-192.png", 192), ("Icon-512.png", 512), ("Icon-maskable-192.png", 192), ("Icon-maskable-512.png", 512)];

/// Middenvierkant uitsnijden.
fn square(img: DynamicImage) -> RgbaImage {
    let (w, h) = (img.width(), img.height());
    let s = w.min(h);
    img.crop_imm((w - s) / 2, (h - s) / 2, s, s).to_rgba8()
}

fn flatten_on_white(img: &RgbaImage) -> RgbaImage {
    let mut out = RgbaImage::from_pixel(img.width(), img.height(), Rgba([255, 255, 255, 255]));
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3] as u32;
        let o = out.get_pixel_mut(x, y);
        for c in 0..3 {
            o[c] = ((p[c] as u32 * a + 255 * (255 - a)) / 255) as u8;
        }
    }
    out
}

fn save_png(img: &RgbaImage, size: u32, path: &Path) -> AppResult<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let resized = image::imageops::resize(img, size, size, FilterType::Lanczos3);
    resized.save_with_format(path, image::ImageFormat::Png).map_err(|e| AppError::CommandFailed(format!("saving icon: {e}")))
}

pub fn generate_icons(project_dir: &Path, source: &Path) -> AppResult<IconReport> {
    let ext = source.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !["png", "jpg", "jpeg"].contains(&ext.as_str()) {
        return Err(AppError::Invalid("the icon must be a PNG or JPG image".into()));
    }
    if std::fs::metadata(source)?.len() > MAX_SOURCE_BYTES {
        return Err(AppError::Invalid("the image is larger than 25 MB".into()));
    }
    let img = image::open(source).map_err(|e| AppError::Invalid(format!("could not read the image: {e}")))?;
    let mut report = IconReport::default();
    if img.width().min(img.height()) < 512 {
        report.warnings.push("small_source".into());
    }
    if img.width() != img.height() {
        report.warnings.push("not_square".into());
    }
    let base = square(img);
    let opaque = flatten_on_white(&base);

    let mut put = |rel: String, img: &RgbaImage, size: u32| -> AppResult<()> {
        save_png(img, size, &project_dir.join(&rel))?;
        report.written.push(rel);
        Ok(())
    };
    if project_dir.join("android").is_dir() {
        for (dpi, px) in ANDROID {
            put(format!("android/app/src/main/res/mipmap-{dpi}/ic_launcher.png"), &base, px)?;
        }
    }
    if project_dir.join("ios").is_dir() {
        // App Store en iOS eisen een icoon zonder transparantie.
        for (name, px) in IOS {
            put(format!("ios/Runner/Assets.xcassets/AppIcon.appiconset/{name}.png"), &opaque, px)?;
        }
    }
    if project_dir.join("web").is_dir() {
        for (name, px) in WEB {
            put(format!("web/icons/{name}"), &base, px)?;
        }
        put("web/favicon.png".into(), &base, 32)?;
    }
    // Voor de winkelvermelding (Play: 512×512).
    put("store/icon-512.png".into(), &opaque, 512)?;
    if report.written.len() == 1 {
        report.warnings.push("no_platforms".into());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_every_size_and_flattens_ios() {
        let d = std::env::temp_dir().join(format!("studio-icons-{}", uuid::Uuid::new_v4()));
        for sub in ["android", "ios", "web"] {
            std::fs::create_dir_all(d.join(sub)).unwrap();
        }
        // transparante bron, niet vierkant (800×600)
        let src = d.join("logo.png");
        let mut img = RgbaImage::from_pixel(800, 600, Rgba([79, 70, 229, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
        img.save(&src).unwrap();

        let r = generate_icons(&d, &src).unwrap();
        assert_eq!(r.written.len(), 5 + 15 + 4 + 1 + 1);
        assert!(r.warnings.contains(&"not_square".to_string()) && !r.warnings.contains(&"small_source".to_string()));
        let size = |rel: &str| image::image_dimensions(d.join(rel)).unwrap();
        assert_eq!(size("android/app/src/main/res/mipmap-xxxhdpi/ic_launcher.png"), (192, 192));
        assert_eq!(size("ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png"), (1024, 1024));
        assert_eq!(size("ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-83.5x83.5@2x.png"), (167, 167));
        assert_eq!(size("web/icons/Icon-512.png"), (512, 512));
        assert_eq!(size("store/icon-512.png"), (512, 512));
        let ios = image::open(d.join("ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-60x60@3x.png")).unwrap().to_rgba8();
        assert!(ios.pixels().all(|p| p[3] == 255), "iOS icons must have no transparency");
    }

    #[test]
    fn rejects_wrong_files_and_warns_for_small_sources() {
        let d = std::env::temp_dir().join(format!("studio-icons-bad-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(d.join("android")).unwrap();
        std::fs::write(d.join("x.txt"), "no").unwrap();
        assert!(matches!(generate_icons(&d, &d.join("x.txt")), Err(AppError::Invalid(_))));
        std::fs::write(d.join("broken.png"), "not an image").unwrap();
        assert!(matches!(generate_icons(&d, &d.join("broken.png")), Err(AppError::Invalid(_))));
        let small = d.join("small.png");
        RgbaImage::from_pixel(100, 100, Rgba([1, 2, 3, 255])).save(&small).unwrap();
        let r = generate_icons(&d, &small).unwrap();
        assert!(r.warnings.contains(&"small_source".to_string()));
    }
}
