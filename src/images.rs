//! Build-time image processing: resize to webp and emit srcsets.
//!
//! The corpus carries 21 MB of images, with individual `featured.png` files
//! above 2 MB. The old site let Astro's `Image` component resize them during
//! its build; a static host that shipped the originals would be a serious
//! regression, so the resize happens here, once, at build time.
//!
//! Every raster image becomes a small set of WebP variants, each one narrower
//! than the last, and never wider than the source. Vector and animated images
//! are copied byte-for-byte: resizing an SVG throws away its point, and
//! resizing a GIF throws away its frames.
//!
//! Encoding goes through `libwebp` (the `webp` crate) rather than the `image`
//! crate's own WebP encoder, which is lossless only. Measured against the
//! corpus, that encoder made the images bigger than the files it started
//! from - a 409 KB `featured.jpg` came out at 714 KB - which is the regression
//! this module exists to prevent. libwebp's lossy encoder at quality 80 takes
//! the same image to 20 KB, and the whole corpus from 22 MB to under 3 MB.

use anyhow::{Context, Result, anyhow};
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView, RgbImage};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The widths every raster image is published at, ascending.
///
/// A variant is only emitted when it is strictly narrower than the source,
/// so a 900px image ships 640 alone rather than an upscaled 960.
pub const WIDTHS: [u32; 3] = [640, 960, 1280];

/// The extra width a `featured.*` image gets for link previews.
///
/// Open Graph images want roughly 1200px; that is not one of [`WIDTHS`], so
/// `featured` images get their own variant.
pub const OG_WIDTH: u32 = 1200;

/// WebP quality for every variant. Lossy, because these are photographs and
/// screenshots, not artwork with hard edges.
pub const QUALITY: f32 = 80.0;

/// Where post images are published, relative to the site root.
pub const IMAGE_ROOT: &str = "images/blog";

/// One published file: where it came from and where it went.
///
/// `href` is the site-absolute URL the post body links to. `width` and
/// `height` are the variant's intrinsic size, ready for an `<img>` tag; they
/// are `0` for a copied SVG or GIF, whose size is not read here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessedImage {
    pub source_name: String,
    pub href: String,
    pub width: u32,
    pub height: u32,
}

/// What one post's images were published as, keyed by the file the post names.
///
/// A post writes `./images/image-1.jpg` and the build publishes
/// `image-1-640.webp`, so the path in the prose is not the path in the output.
/// The map carries the source name to the href of the variant an `<img>` should
/// show: the widest one below [`WIDTHS`], or the copy itself for an SVG or GIF.
#[derive(Debug, Default, Clone)]
pub struct PublishedImages {
    by_source: BTreeMap<String, String>,
}

impl PublishedImages {
    /// Record one published file under the source name it came from.
    ///
    /// Variants arrive narrowest first, so the last one recorded wins and the
    /// widest variant is what the prose points at - the narrow ones are there
    /// for a `srcset` a hand-written `<img>` does not have.
    fn record(&mut self, source_name: &str, href: &str) {
        self.by_source
            .insert(source_name.to_string(), href.to_string());
    }

    /// The href to serve for a post-relative `path`, if the build published it.
    ///
    /// `path` is what the post wrote (`./images/image-1.jpg`); the lookup
    /// strips the same prefixes [`crate::markdown::strip_asset_prefix`] does, so
    /// `./images/x.png`, `images/x.png`, and `x.png` all find `x.png`.
    pub fn href_for(&self, path: &str) -> Option<&str> {
        let name = path.rsplit('/').next().unwrap_or(path);
        self.by_source.get(name).map(String::as_str)
    }

    /// Whether anything was published for this post.
    pub fn is_empty(&self) -> bool {
        self.by_source.is_empty()
    }
}

/// Publish every image in `source_dir` into `out_dir`, resizing as it goes.
///
/// `source_dir` is a post's `images/` directory and `out_dir` is where that
/// post's images are published (`dist/images/blog/<slug>/`); `out_dir` is
/// created when needed. A directory that does not exist publishes nothing,
/// because a post without images is legal.
pub fn process_post_images(source_dir: &Path, out_dir: &Path) -> Result<Vec<ProcessedImage>> {
    let mut processed = Vec::new();
    if !source_dir.is_dir() {
        return Ok(processed);
    }

    for path in sorted_files(source_dir)? {
        processed.extend(process_one(&path, out_dir)?);
    }
    Ok(processed)
}

/// Publish a post's images and return the lookup the markdown stage needs.
///
pub fn publish_post_images(source_dir: &Path, out_dir: &Path) -> Result<PublishedImages> {
    let mut published = PublishedImages::default();
    for image in process_post_images(source_dir, out_dir)? {
        published.record(&image.source_name, &image.href);
    }
    Ok(published)
}

/// Publish one image, returning every file it produced.
fn process_one(path: &Path, out_dir: &Path) -> Result<Vec<ProcessedImage>> {
    let source_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("image name is not valid UTF-8: {}", path.display()))?
        .to_string();
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    match extension.as_str() {
        // Vector and animated images are already as small as they get.
        "svg" | "gif" => {
            let dest = out_dir.join(&source_name);
            fs::create_dir_all(out_dir)
                .with_context(|| format!("creating {}", out_dir.display()))?;
            fs::copy(path, &dest)
                .with_context(|| format!("copying {} to {}", path.display(), dest.display()))?;
            Ok(vec![ProcessedImage {
                href: href_for(out_dir, &source_name)?,
                source_name,
                width: 0,
                height: 0,
            }])
        }
        "png" | "jpg" | "jpeg" => resize_rasters(path, &source_name, out_dir),
        // A stray `.puml` or anything else is not an image we can serve.
        _ => Ok(Vec::new()),
    }
}

/// Emit the WebP variants for one raster image.
fn resize_rasters(path: &Path, source_name: &str, out_dir: &Path) -> Result<Vec<ProcessedImage>> {
    let image = image::open(path).with_context(|| format!("opening {}", path.display()))?;
    let (source_width, _) = image.dimensions();
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| anyhow!("image name is not valid UTF-8: {}", path.display()))?
        .to_string();

    let mut processed = Vec::new();
    for width in variant_widths(&stem, source_width) {
        let variant = image.resize(width, u32::MAX, FilterType::Lanczos3);
        let (width, height) = variant.dimensions();
        let file_name = format!("{stem}-{width}.webp");
        write_webp(&variant, &out_dir.join(&file_name))?;
        processed.push(ProcessedImage {
            href: href_for(out_dir, &file_name)?,
            source_name: source_name.to_string(),
            width,
            height,
        });
    }
    Ok(processed)
}

/// The widths one image is published at: [`WIDTHS`] below the source width,
/// plus [`OG_WIDTH`] for a `featured` image, ascending and deduplicated.
///
/// An image already narrower than every target width is published at its own
/// width. Dropping it instead would leave the post pointing at a file that
/// never shipped - two of the corpus images are 509px and 615px screenshots -
/// and upscaling it to 640 would blur it for no benefit.
fn variant_widths(stem: &str, source_width: u32) -> Vec<u32> {
    let mut widths: Vec<u32> = WIDTHS
        .iter()
        .copied()
        .filter(|width| *width < source_width)
        .collect();
    if stem.starts_with("featured") && OG_WIDTH < source_width {
        widths.push(OG_WIDTH);
    }
    widths.sort_unstable();
    widths.dedup();
    if widths.is_empty() {
        widths.push(source_width);
    }
    widths
}

/// Encode `image` as lossy WebP at [`QUALITY`] and write it to `dest`.
fn write_webp(image: &DynamicImage, dest: &Path) -> Result<()> {
    let rgb: RgbImage = image.to_rgb8();
    let encoded = webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height())
        .encode_simple(false, QUALITY)
        .map_err(|error| anyhow!("encoding {} as webp: {error:?}", dest.display()))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(dest, &*encoded).with_context(|| format!("writing {}", dest.display()))?;
    Ok(())
}

/// The site-absolute URL a published file is served at.
///
/// `out_dir` is a post's published directory, so its last segment is the slug
/// the URL carries: `dist/images/blog/<slug>/` publishes `<file>` at
/// `/images/blog/<slug>/<file>`.
fn href_for(out_dir: &Path, file_name: &str) -> Result<String> {
    let slug = out_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("no slug in {}", out_dir.display()))?;
    Ok(format!("/{IMAGE_ROOT}/{slug}/{file_name}"))
}

/// The files in `dir`, in name order, so a build is reproducible.
fn sorted_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    Ok(files)
}

/// Copy everything under `src` to the same relative path under `out`.
///
/// This is what publishes `static/styles/site.css`, `static/diagrams/*.svg`,
/// and the fonts: the site refers to them at their path below `static/`, so
/// the relative layout has to survive the copy. A `src` that does not exist
/// copies nothing.
pub fn copy_static(src: &Path, out: &Path) -> Result<()> {
    if !src.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;

    let entries = fs::read_dir(src).with_context(|| format!("reading {}", src.display()))?;
    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .collect();
    paths.sort();

    for path in paths {
        let name = path
            .file_name()
            .ok_or_else(|| anyhow!("no file name in {}", path.display()))?;
        let dest = out.join(name);
        if path.is_dir() {
            copy_static(&path, &dest)?;
        } else {
            fs::copy(&path, &dest)
                .with_context(|| format!("copying {} to {}", path.display(), dest.display()))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_png(path: &std::path::Path, w: u32, h: u32) {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([10, 20, 30]));
        img.save(path).unwrap();
    }

    #[test]
    fn emits_webp_variants_up_to_the_source_width() {
        let dir = std::env::temp_dir().join("ethang-img-wide");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("featured.png"), 1500, 900);
        let images = process_post_images(&src, &out).unwrap();
        let widths: Vec<u32> = images.iter().map(|i| i.width).collect();
        assert!(widths.contains(&640));
        assert!(widths.contains(&960));
        assert!(widths.contains(&1280));
    }

    #[test]
    fn never_upscales() {
        let dir = std::env::temp_dir().join("ethang-img-small");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("small.png"), 400, 300);
        let images = process_post_images(&src, &out).unwrap();
        assert!(
            images.iter().all(|i| i.width <= 400),
            "upscaled: {images:?}"
        );
    }

    #[test]
    fn copies_svg_byte_for_byte() {
        let dir = std::env::temp_dir().join("ethang-img-svg");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"></svg>";
        std::fs::write(src.join("chart.svg"), svg).unwrap();
        let images = process_post_images(&src, &out).unwrap();
        let copied = std::fs::read_to_string(out.join("chart.svg")).unwrap();
        assert_eq!(copied, svg);
        assert!(images.iter().any(|i| i.source_name == "chart.svg"));
    }

    #[test]
    fn emits_a_1200_wide_og_image_for_featured() {
        let dir = std::env::temp_dir().join("ethang-img-og");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("featured.png"), 2000, 1200);
        process_post_images(&src, &out).unwrap();
        assert!(out.join("featured-1200.webp").exists() || out.join("featured-og.webp").exists());
    }

    #[test]
    fn writes_every_variant_as_a_webp_file() {
        let dir = std::env::temp_dir().join("ethang-img-files");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("chart.png"), 1500, 750);
        let images = process_post_images(&src, &out).unwrap();

        assert_eq!(images.len(), 3, "variants: {images:?}");
        for image in &images {
            let file_name = image.href.rsplit('/').next().unwrap();
            let bytes = std::fs::read(out.join(file_name)).unwrap();
            assert_eq!(&bytes[..4], b"RIFF", "not a webp: {file_name}");
            assert_eq!(&bytes[8..12], b"WEBP", "not a webp: {file_name}");
        }
    }

    #[test]
    fn hrefs_are_rooted_at_the_post_slug() {
        let dir = std::env::temp_dir().join("ethang-img-href");
        let src = dir.join("src");
        let out = dir.join("images").join("blog").join("demo-post");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("chart.png"), 1000, 500);
        let images = process_post_images(&src, &out).unwrap();

        assert_eq!(images[0].href, "/images/blog/demo-post/chart-640.webp");
        assert_eq!(images[0].source_name, "chart.png");
    }

    #[test]
    fn keeps_the_source_aspect_ratio() {
        let dir = std::env::temp_dir().join("ethang-img-aspect");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("wide.png"), 1280, 640);
        let images = process_post_images(&src, &out).unwrap();

        let variant = images.iter().find(|i| i.width == 640).unwrap();
        assert_eq!(variant.height, 320, "aspect: {images:?}");
    }

    #[test]
    fn skips_a_width_equal_to_the_source() {
        let dir = std::env::temp_dir().join("ethang-img-exact");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("exact.png"), 960, 480);
        let images = process_post_images(&src, &out).unwrap();

        assert!(
            images.iter().all(|i| i.width != 960),
            "upscaled: {images:?}"
        );
        assert!(
            images.iter().all(|i| i.width == 640),
            "variants: {images:?}"
        );
    }

    #[test]
    fn copies_gif_byte_for_byte() {
        let dir = std::env::temp_dir().join("ethang-img-gif");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        let gif: [u8; 16] = [
            b'G', b'I', b'F', b'8', b'9', b'a', 1, 0, 1, 0, 0x80, 0, 0, 0, 0, 0,
        ];
        std::fs::write(src.join("spin.gif"), gif).unwrap();
        let images = process_post_images(&src, &out).unwrap();

        assert_eq!(std::fs::read(out.join("spin.gif")).unwrap(), gif);
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].href, "/images/blog/out/spin.gif");
    }

    #[test]
    fn ignores_files_that_are_not_images() {
        let dir = std::env::temp_dir().join("ethang-img-stray");
        let src = dir.join("src");
        let out = dir.join("out");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("diagram.puml"), "@startuml\n@enduml\n").unwrap();
        assert!(process_post_images(&src, &out).unwrap().is_empty());
    }

    #[test]
    fn a_missing_source_directory_publishes_nothing() {
        let dir = std::env::temp_dir().join("ethang-img-missing");
        let out = dir.join("out");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            process_post_images(&dir.join("src"), &out)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn copy_static_preserves_the_relative_tree() {
        let dir = std::env::temp_dir().join("ethang-static-tree");
        let src = dir.join("static");
        let out = dir.join("dist");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(src.join("diagrams")).unwrap();
        std::fs::create_dir_all(src.join("styles")).unwrap();
        std::fs::write(src.join("styles").join("site.css"), "body{}").unwrap();
        std::fs::write(src.join("diagrams").join("a.svg"), "<svg/>").unwrap();
        std::fs::write(src.join("favicon.svg"), "<svg/>").unwrap();

        copy_static(&src, &out).unwrap();

        assert_eq!(
            "body{}",
            std::fs::read_to_string(out.join("styles").join("site.css")).unwrap()
        );
        assert_eq!(
            "<svg/>",
            std::fs::read_to_string(out.join("diagrams").join("a.svg")).unwrap()
        );
        assert!(out.join("favicon.svg").exists());
    }

    #[test]
    fn copy_static_copies_an_svg_byte_for_byte() {
        let dir = std::env::temp_dir().join("ethang-static-bytes");
        let src = dir.join("static");
        let out = dir.join("dist");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(src.join("diagrams")).unwrap();
        let svg: Vec<u8> = (0..=255u8).collect();
        std::fs::write(src.join("diagrams").join("raw.svg"), &svg).unwrap();

        copy_static(&src, &out).unwrap();

        assert_eq!(
            svg,
            std::fs::read(out.join("diagrams").join("raw.svg")).unwrap()
        );
    }

    #[test]
    fn copy_static_tolerates_a_missing_source() {
        let dir = std::env::temp_dir().join("ethang-static-missing");
        let _ = std::fs::remove_dir_all(&dir);
        copy_static(&dir.join("static"), &dir.join("dist")).unwrap();
        assert!(!dir.join("dist").exists());
    }

    #[test]
    fn the_published_map_points_at_the_widest_variant() {
        let dir = std::env::temp_dir().join("ethang-img-map");
        let src = dir.join("src");
        let out = dir.join("images").join("blog").join("demo-post");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("image-1.png"), 1500, 750);
        std::fs::write(src.join("chart.svg"), "<svg/>").unwrap();

        let published = publish_post_images(&src, &out).unwrap();

        // 1500px wide publishes 640, 960, and 1280; the widest is the one an
        // <img> without a srcset should show.
        assert_eq!(
            published.href_for("image-1.png"),
            Some("/images/blog/demo-post/image-1-1280.webp")
        );
        // An SVG is copied, so it keeps its own name.
        assert_eq!(
            published.href_for("chart.svg"),
            Some("/images/blog/demo-post/chart.svg")
        );
        assert!(!published.is_empty());
    }

    #[test]
    fn the_published_map_accepts_every_prefix_a_post_writes() {
        let dir = std::env::temp_dir().join("ethang-img-prefix");
        let src = dir.join("src");
        let out = dir.join("out");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("shot.png"), 800, 400);
        let published = publish_post_images(&src, &out).unwrap();

        for path in ["./images/shot.png", "images/shot.png", "shot.png"] {
            assert!(
                published.href_for(path).is_some(),
                "{path} did not resolve: {published:?}"
            );
        }
        assert_eq!(published.href_for("missing.png"), None);
    }

    #[test]
    fn a_post_with_no_images_publishes_an_empty_map() {
        let dir = std::env::temp_dir().join("ethang-img-none");
        let _ = std::fs::remove_dir_all(&dir);
        let published = publish_post_images(&dir.join("src"), &dir.join("out")).unwrap();
        assert!(published.is_empty());
    }

    #[test]
    fn a_narrow_image_is_published_at_its_own_width() {
        let dir = std::env::temp_dir().join("ethang-img-narrow");
        let src = dir.join("src");
        let out = dir.join("out");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&src).unwrap();
        // Narrower than every target width: dropping it would leave the post
        // pointing at a file that never shipped.
        write_png(&src.join("statusline.png"), 509, 103);
        let published = publish_post_images(&src, &out).unwrap();

        assert_eq!(
            published.href_for("statusline.png"),
            Some("/images/blog/out/statusline-509.webp")
        );
        assert!(out.join("statusline-509.webp").exists());
    }

    #[test]
    fn every_source_image_is_published_under_some_name() {
        let dir = std::env::temp_dir().join("ethang-img-all");
        let src = dir.join("src");
        let out = dir.join("out");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&src).unwrap();
        write_png(&src.join("tiny.png"), 200, 100);
        write_png(&src.join("mid.png"), 800, 400);
        write_png(&src.join("wide.png"), 2000, 1000);

        let published = publish_post_images(&src, &out).unwrap();

        for source in ["tiny.png", "mid.png", "wide.png"] {
            assert!(
                published.href_for(source).is_some(),
                "{source} was never published: {published:?}"
            );
        }
    }
}
