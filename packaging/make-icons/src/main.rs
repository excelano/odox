//! Turn the three icon SVGs into every raster the three platforms want.
//!
//! Linux ships the SVGs and lets the desktop rasterize them. Neither other
//! platform has such a step: the Windows shell reads a fixed set of sizes out of
//! an icon directory, an MSIX package declares PNGs at fixed dimensions, a macOS
//! bundle carries one icon family, and every store's listing form wants its own
//! square. So every size has to exist before anything is shipped, and this is
//! the step between. It is why rasterized artefacts are committed to a
//! repository that otherwise holds only sources.
//!
//! **It makes the macOS icons too, which is the difference from the fleet's
//! copies of this tool.** slipcase-desktop, flyleaf, duckling and segler all
//! render their `.icns` with `sips` and `iconutil` inside `build-app.sh`, and
//! both of those exist only on a Mac, so none of them can produce one until it
//! reaches that machine. `icns` is pure Rust and renders here, which takes the
//! icon off the platform session's critical path. Measured on 2026-09-14 and
//! written up in `~/notes/mac_icons_on_linux.md`.
//!
//! **Three drawings, and all three are applications.** Where segler has one
//! application icon and two document icons, odox has three applications, and
//! each one's drawing is also the icon of the format it reads. Because the three
//! ship as three separate store packages, each gets its own asset set rather
//! than sharing one.
//!
//! Nothing is drawn for the launcher. It has no window, no desktop entry and no
//! store package, and it ships only on apt.
//!
//!     cargo run --manifest-path packaging/make-icons/Cargo.toml
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};

use resvg::tiny_skia;
use resvg::usvg;

/// The three applications, which are also the three drawings.
///
/// The launcher is deliberately absent; see the module comment.
const APPLICATIONS: &[&str] = &["xodt", "xods", "xodp"];

// ---------------------------------------------------------------- Windows ---

/// The sizes the Windows shell asks for.
///
/// 16, 32 and 48 are the list, the desktop and the tile; 256 is what the
/// extra-large view and the file properties dialog use. 20, 24, 40, 64 and 128
/// are the same three sizes again at the display scalings Windows offers, and
/// without them the shell picks a neighbour and resamples it, which is visibly
/// softer than rendering the vector at the size wanted.
const ICO_SIZES: &[u32] = &[16, 20, 24, 32, 40, 48, 64, 128, 256];

/// Above this, an icon directory entry is stored as PNG rather than as a bitmap.
///
/// PNG entries are read by Vista and later and nothing older, and a 256-pixel
/// bitmap entry costs 256 KiB where the PNG costs a few. Below the line the
/// bitmap is kept, because it is what every consumer of an icon directory has
/// always understood and the saving there is worth nothing.
const PNG_ABOVE: u32 = 48;

/// The display scalings Windows offers, as the Store spells them.
///
/// Without these there is one bitmap per asset and every other scaling is an
/// upscale of it. The `.ico` carries nine sizes for exactly this reason and the
/// argument does not change because the file is a PNG.
const SCALES: &[u32] = &[100, 125, 150, 200, 400];

/// The sizes the shell asks for when it wants an icon rather than a tile.
const TARGET_SIZES: &[u32] = &[16, 24, 32, 48, 256];

/// The three forms of each target size, and why the list exists.
///
/// `BackgroundColor` in the manifest is `transparent`, so where Windows draws a
/// *plated* icon it fills the plate with the person's accent colour, and the
/// drawing lands on a coloured square while the side-loaded install draws the
/// same icon unplated out of the `.ico`. One application with two faces.
/// slipcase-desktop measured that on a taskbar on 2026-08-28.
///
/// An `altform-unplated` asset is what tells the shell not to plate. The light
/// variant is the same pixels, because these drawings are coloured rather than
/// monochrome and need no separate treatment on a light taskbar, but the
/// qualifier has to exist or Windows 11 falls back to the plated form there.
const ALTFORMS: &[&str] = &["", "_altform-unplated", "_altform-lightunplated"];

/// One image an `AppxManifest.xml` names, at the size the Store wants.
struct Asset {
    stem: &'static str,
    width: u32,
    height: u32,
    /// How much of the shorter side the drawing occupies, centred.
    fill: f32,
    /// Whether the shell ever draws this one as a bare icon rather than on a
    /// tile. Only `Square44x44Logo` is, and only that one gets the target-size
    /// and unplated variants.
    icon: bool,
}

/// The five images each package's manifest names, and nothing else.
///
/// The dimensions are the Store's and are not a choice. `fill` is: a tile is
/// drawn on a coloured plate and Microsoft's tile guidance leaves the icon about
/// two thirds of it, where an icon-shaped asset is drawn at the size it is given
/// and wants the whole of it, which is also what the `.ico` does at every size.
/// Only a look at real tiles settles the two thirds, and slipcase-desktop took
/// that look.
///
/// `FileTypeLogo` is the one association each package declares: `.odt` for one
/// package, `.ods` for the next, `.odp` for the last. It is 256 because that is
/// the largest the shell asks for and the only place a document icon is drawn
/// big.
const ASSETS: &[Asset] = &[
    Asset { stem: "StoreLogo",         width:  50, height:  50, fill: 1.00, icon: false },
    Asset { stem: "Square44x44Logo",   width:  44, height:  44, fill: 1.00, icon: true  },
    Asset { stem: "Square150x150Logo", width: 150, height: 150, fill: 0.66, icon: false },
    Asset { stem: "Wide310x150Logo",   width: 310, height: 150, fill: 0.66, icon: false },
    Asset { stem: "FileTypeLogo",      width: 256, height: 256, fill: 1.00, icon: false },
];

/// The sizes Partner Center's *Store logo* listing field accepts.
///
/// Both rather than only the smaller: the field takes either and the larger is
/// what survives a future listing that wants more pixels. Neither goes in the
/// package, and the comment where they are written says why.
const LISTING_SIZES: &[u32] = &[1080, 2160];

// ------------------------------------------------------------------ macOS ---

/// The ten elements an `.iconset` carries, which is what `iconutil` consumes,
/// as the pixel count to render and the type that count belongs to.
///
/// Both halves matter. Two pairs render at the same pixel count under different
/// types, because macOS distinguishes a 32-pixel icon from a 16-pixel icon on a
/// 2x display, and the crate's own `add_icon` picks from the pixel size alone
/// and so cannot tell them apart.
const ICNS_ELEMENTS: &[(u32, icns::IconType)] = &[
    (16, icns::IconType::RGBA32_16x16),
    (32, icns::IconType::RGBA32_16x16_2x),
    (32, icns::IconType::RGBA32_32x32),
    (64, icns::IconType::RGBA32_32x32_2x),
    (128, icns::IconType::RGBA32_128x128),
    (256, icns::IconType::RGBA32_128x128_2x),
    (256, icns::IconType::RGBA32_256x256),
    (512, icns::IconType::RGBA32_256x256_2x),
    (512, icns::IconType::RGBA32_512x512),
    (1024, icns::IconType::RGBA32_512x512_2x),
];

// ----------------------------------------------------------- the listings ---

/// The sizes a submission form asks for.
///
/// 1024 is App Store Connect's, 1080 and 2160 are Partner Center's *Store logo*,
/// and 256 and 512 are what a support page or a press kit wants. All square: no
/// listing field on either store takes a wide one.
const SUBMISSION_SIZES: &[u32] = &[256, 512, 1024, 1080, 2160];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut args = std::env::args_os().skip(1);
    let at = |given: Option<std::ffi::OsString>, fallback: &str| {
        given.map_or_else(|| here.join(fallback), PathBuf::from)
    };
    let icons = at(args.next(), "../linux/icons");
    let windows = at(args.next(), "../windows");
    let macos = at(args.next(), "../macos");
    let submission = at(args.next(), "../icons");

    // Written fresh every time, all four. A renamed or dropped asset would
    // otherwise be left behind by a rebuild, and the workflows compare these
    // directories against a rebuild, so a stale file would sit in a package and
    // in the tree with nothing objecting to it.
    for directory in [
        &windows.join("assets"),
        &windows.join("listing"),
        &submission,
    ] {
        if directory.exists() {
            std::fs::remove_dir_all(directory)?;
        }
        std::fs::create_dir_all(directory)?;
    }
    std::fs::create_dir_all(&macos)?;

    let mut counts = Vec::new();
    for &app in APPLICATIONS {
        let source = std::fs::read_to_string(icons.join(format!("{app}.svg")))
            .map_err(|e| format!("{}/{app}.svg: {e}", icons.display()))?;
        let tree = usvg::Tree::from_data(source.as_bytes(), &usvg::Options::default())?;

        write_ico(&tree, &windows.join(format!("{app}.ico")))?;
        write_icns(&tree, &macos.join(format!("{app}.icns")))?;

        // One asset set per application, because the three ship as three
        // packages and each manifest names its own.
        let assets = windows.join("assets").join(app);
        std::fs::create_dir_all(&assets)?;
        let images = write_assets(&tree, &assets)?;

        // The store listing logo, which is not in the package and must not be.
        //
        // Partner Center's *Store logo* field is a listing image rather than a
        // package asset and refuses anything but 1080 or 2160 square, measured
        // by slipcase-desktop against the live form after that repository had
        // believed the 300 the older documentation describes. It is written
        // beside the package assets rather than into them because a file added
        // to `assets` lands in the MSIX, and a package that gains a file has to
        // be certified again for an image no installed copy would ever read.
        let listing = windows.join("listing").join(app);
        std::fs::create_dir_all(&listing)?;
        for &size in LISTING_SIZES {
            write_png(
                &tree,
                &listing,
                &format!("store-logo-{size}.png"),
                size,
                size,
                1.0,
            )?;
        }

        // Both shapes, for the submission forms.
        //
        // Neither goes in a package and neither is read at run time. Each store
        // treats the shape differently: Apple's tooling masks a square itself
        // and wants the square, and a form that draws what it is handed wants
        // the rounded one. Which to upload is decided at the form, so both are
        // written and neither is the default.
        let round = rounded(&source)?;
        let round_tree = usvg::Tree::from_data(round.as_bytes(), &usvg::Options::default())?;
        std::fs::write(submission.join(format!("{app}-square.svg")), &source)?;
        std::fs::write(submission.join(format!("{app}-rounded.svg")), &round)?;
        for (shape, drawn) in [("square", &tree), ("rounded", &round_tree)] {
            for &size in SUBMISSION_SIZES {
                write_png(
                    drawn,
                    &submission,
                    &format!("{app}-{shape}-{size}.png"),
                    size,
                    size,
                    1.0,
                )?;
            }
        }

        counts.push((app, images));
    }

    for (app, images) in &counts {
        println!(
            "{app}: {}.ico with {} sizes, {}.icns with {} elements, {images} package images, \
             {} listing images, {} submission images",
            app,
            ICO_SIZES.len(),
            app,
            ICNS_ELEMENTS.len(),
            LISTING_SIZES.len(),
            2 * SUBMISSION_SIZES.len(),
        );
    }
    Ok(())
}

/// The Windows icon directory: every size the shell asks for, in one file.
fn write_ico(tree: &usvg::Tree, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
    for &size in ICO_SIZES {
        let (width, height, rgba) = straight(tree, size)?;
        let image = ico::IconImage::from_rgba_data(width, height, rgba);
        icon.add_entry(if size > PNG_ABOVE {
            ico::IconDirEntry::encode_as_png(&image)?
        } else {
            ico::IconDirEntry::encode_as_bmp(&image)?
        });
    }
    icon.write(std::fs::File::create(path)?)?;
    Ok(())
}

/// The macOS icon family: the same ten elements `iconutil` writes.
fn write_icns(tree: &usvg::Tree, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut family = icns::IconFamily::new();
    for &(size, kind) in ICNS_ELEMENTS {
        let (width, height, rgba) = straight(tree, size)?;
        let image = icns::Image::from_data(icns::PixelFormat::RGBA, width, height, rgba)?;
        family.add_icon_with_type(&image, kind)?;
    }
    family.write(std::io::BufWriter::new(std::fs::File::create(path)?))?;
    Ok(())
}

/// Every PNG one package's manifest names. Returns how many were written.
fn write_assets(tree: &usvg::Tree, into: &Path) -> Result<usize, Box<dyn std::error::Error>> {
    let mut written = 0;
    for asset in ASSETS {
        // The unqualified name as well as the qualified ones. The manifest names
        // this one, and it is what resolves when nothing indexes the package, so
        // keeping it means the assets are correct with or without
        // `resources.pri` rather than only with.
        write_png(
            tree,
            into,
            &format!("{}.png", asset.stem),
            asset.width,
            asset.height,
            asset.fill,
        )?;
        written += 1;

        for &scale in SCALES {
            write_png(
                tree,
                into,
                &format!("{}.scale-{scale}.png", asset.stem),
                scaled(asset.width, scale),
                scaled(asset.height, scale),
                asset.fill,
            )?;
            written += 1;
        }

        if asset.icon {
            for &size in TARGET_SIZES {
                for altform in ALTFORMS {
                    write_png(
                        tree,
                        into,
                        &format!("{}.targetsize-{size}{altform}.png", asset.stem),
                        size,
                        size,
                        1.0,
                    )?;
                    written += 1;
                }
            }
        }
    }
    Ok(written)
}

/// The same drawing, clipped to a rounded square.
///
/// Done to the SVG text rather than to rendered pixels, so that the vector
/// written beside the PNGs is a real one rather than a trace, and so that both
/// shapes come out of the one rasterizing path. The source's own content is
/// wrapped in a clipped group and nothing in it moves; the insert goes after the
/// opening tag, so the `<svg` a sniffer looks for stays where it is.
///
/// 14.317 of 64 is 22.37%, the proportion of the side Apple's icon grid gives
/// the corner. `rx` draws that as a circular arc where Apple's own tooling draws
/// a continuous curve; the two are indistinguishable below about 512 pixels, and
/// where the difference would show, Icon Composer on a Mac is what produces
/// Apple's shape.
fn rounded(source: &str) -> Result<String, Box<dyn std::error::Error>> {
    let open = source
        .find("<svg")
        .and_then(|at| source[at..].find('>').map(|end| at + end + 1))
        .ok_or("the icon source has no opening <svg> tag")?;
    let close = source
        .rfind("</svg>")
        .ok_or("the icon source has no closing </svg> tag")?;
    let mut out = String::with_capacity(source.len() + 160);
    out.push_str(&source[..open]);
    out.push_str(
        "\n  <clipPath id=\"rounded\"><rect width=\"64\" height=\"64\" rx=\"14.317\"/></clipPath>\
         \n  <g clip-path=\"url(#rounded)\">",
    );
    out.push_str(&source[open..close]);
    out.push_str("</g>\n");
    out.push_str(&source[close..]);
    Ok(out)
}

/// One asset dimension at one scaling, the way the Store rounds it.
///
/// 50 at 125% is 62.5 and the Store's own table says 63, so this rounds rather
/// than truncating. Getting it wrong by a pixel is not a build failure; it is an
/// image the shell quietly rescales, which is the whole thing these variants
/// exist to avoid.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled(size: u32, scale: u32) -> u32 {
    (f64::from(size) * f64::from(scale) / 100.0).round() as u32
}

fn write_png(
    tree: &usvg::Tree,
    dir: &Path,
    name: &str,
    width: u32,
    height: u32,
    fill: f32,
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(dir.join(name), draw(tree, width, height, fill)?.encode_png()?)?;
    Ok(())
}

/// The drawing centred on a canvas of the given size.
///
/// Every raster here comes through this, so the wide tile is the same rendering
/// as the square one rather than a square image somebody stretched. The vector
/// is rasterized at the size wanted rather than rendered once and resampled,
/// which is the whole reason no read-back assertion is needed anywhere here:
/// there is no upscale to guard against.
fn draw(
    tree: &usvg::Tree,
    width: u32,
    height: u32,
    fill: f32,
) -> Result<tiny_skia::Pixmap, Box<dyn std::error::Error>> {
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or("a pixmap of that size could not be made")?;
    #[allow(clippy::cast_precision_loss)]
    let (w, h) = (width as f32, height as f32);
    let side = w.min(h) * fill;
    let scale = side / tree.size().width();
    resvg::render(
        tree,
        tiny_skia::Transform::from_translate((w - side) / 2.0, (h - side) / 2.0)
            .pre_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// The drawing at one size, as straight rather than premultiplied alpha.
///
/// `tiny_skia` renders into premultiplied pixels; both an icon directory and an
/// icon family hold straight ones. Handing the pixmap's bytes over unconverted
/// looks right everywhere the drawing is opaque and wrong along every
/// antialiased edge, which on these drawings is the whole rounded page outline.
fn straight(
    tree: &usvg::Tree,
    size: u32,
) -> Result<(u32, u32, Vec<u8>), Box<dyn std::error::Error>> {
    let pixmap = draw(tree, size, size, 1.0)?;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for pixel in pixmap.pixels() {
        let s = pixel.demultiply();
        rgba.extend_from_slice(&[s.red(), s.green(), s.blue(), s.alpha()]);
    }
    Ok((size, size, rgba))
}
