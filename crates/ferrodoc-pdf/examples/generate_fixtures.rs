//! Regenerates the small, redistribution-safe PDF fixtures used by the workspace.

#[macro_use]
extern crate lopdf;

use std::{error::Error, fs, path::Path};

use ferrodoc_pdf::{PdfDocument, PdfLimits};
use lopdf::{
    Document, EncryptionState, EncryptionVersion, Object, Permissions, Stream,
    content::{Content, Operation},
};

const PAGE_WIDTH: i64 = 595;
const PAGE_HEIGHT: i64 = 842;

fn main() -> Result<(), Box<dyn Error>> {
    let fixture_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/pdf");
    fs::create_dir_all(&fixture_dir)?;

    let born_digital = native_pdf(
        &[
            (24, 72, 760, "FERRODOC FIXTURE HEADING"),
            (
                13,
                72,
                710,
                "First paragraph appears before the second paragraph.",
            ),
            (
                13,
                72,
                680,
                "Second paragraph confirms deterministic reading order.",
            ),
        ],
        None,
        None,
    )?;
    fs::write(fixture_dir.join("born-digital.pdf"), &born_digital)?;
    let mut encrypted = Document::load_mem(&born_digital)?;
    encrypted.trailer.set(
        "ID",
        vec![
            Object::string_literal("ferrodoc-encrypted-fixture"),
            Object::string_literal("ferrodoc-encrypted-fixture"),
        ],
    );
    let encryption = EncryptionState::try_from(EncryptionVersion::V1 {
        document: &encrypted,
        owner_password: "fixture-owner",
        user_password: "fixture-user",
        permissions: Permissions::empty(),
    })?;
    encrypted.encrypt(&encryption)?;
    encrypted.save(fixture_dir.join("encrypted.pdf"))?;

    let scan_source = native_pdf(
        &[
            (28, 70, 530, "SCANNED FERRODOC PAGE"),
            (18, 70, 480, "Optical text survives the CPU path."),
        ],
        None,
        None,
    )?;
    let raster = PdfDocument::from_bytes(scan_source, PdfLimits::default())?.render_page(0, 144)?;
    let image_rgb = rgba_to_rgb(&raster.rgba);
    let scanned = image_pdf(&image_rgb, raster.width, raster.height, &[], None, None)?;
    fs::write(fixture_dir.join("image-only.pdf"), scanned)?;

    let hybrid = image_pdf(
        &image_rgb,
        raster.width,
        raster.height,
        &[(20, 72, 790, "HYBRID NATIVE HEADING")],
        None,
        None,
    )?;
    fs::write(fixture_dir.join("hybrid.pdf"), hybrid)?;

    let rotated = native_pdf(
        &[(18, 72, 500, "ROTATED CROPPED PAGE")],
        Some(90),
        Some([20, 30, 420, 630]),
    )?;
    fs::write(fixture_dir.join("rotated-cropped.pdf"), rotated)?;
    fs::write(
        fixture_dir.join("malformed.pdf"),
        b"%PDF-1.7\nthis is deliberately truncated\n",
    )?;
    Ok(())
}

fn native_pdf(
    lines: &[(i64, i64, i64, &str)],
    rotation: Option<i64>,
    crop_box: Option<[i64; 4]>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    build_pdf(lines, None, rotation, crop_box)
}

fn image_pdf(
    rgb: &[u8],
    width: u32,
    height: u32,
    lines: &[(i64, i64, i64, &str)],
    rotation: Option<i64>,
    crop_box: Option<[i64; 4]>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    build_pdf(lines, Some((rgb, width, height)), rotation, crop_box)
}

fn build_pdf(
    lines: &[(i64, i64, i64, &str)],
    image: Option<(&[u8], u32, u32)>,
    rotation: Option<i64>,
    crop_box: Option<[i64; 4]>,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut document = Document::with_version("1.7");
    let pages_id = document.new_object_id();
    let font_id = document.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let image_id = image.map(|(rgb, width, height)| {
        let mut stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => i64::from(width),
                "Height" => i64::from(height),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => 8,
            },
            rgb.to_vec(),
        );
        stream.compress().expect("fixture image compression");
        document.add_object(stream)
    });

    let mut operations = Vec::new();
    if image_id.is_some() {
        operations.extend([
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    PAGE_WIDTH.into(),
                    0.into(),
                    0.into(),
                    PAGE_HEIGHT.into(),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Scan".to_vec())]),
            Operation::new("Q", vec![]),
        ]);
    }
    for (size, x, y, text) in lines {
        operations.extend([
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec!["F1".into(), (*size).into()]),
            Operation::new("Td", vec![(*x).into(), (*y).into()]),
            Operation::new("Tj", vec![Object::string_literal(*text)]),
            Operation::new("ET", vec![]),
        ]);
    }
    let content = Content { operations };
    let content_id = document.add_object(Stream::new(dictionary! {}, content.encode()?));

    let mut resources = dictionary! { "Font" => dictionary! { "F1" => font_id } };
    if let Some(image_id) = image_id {
        resources.set("XObject", dictionary! { "Scan" => image_id });
    }
    let resources_id = document.add_object(resources);
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "Contents" => content_id,
    };
    if let Some(rotation) = rotation {
        page.set("Rotate", rotation);
    }
    if let Some([x1, y1, x2, y2]) = crop_box {
        page.set("CropBox", vec![x1.into(), y1.into(), x2.into(), y2.into()]);
    }
    let page_id = document.add_object(page);
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
            "Resources" => resources_id,
            "MediaBox" => vec![0.into(), 0.into(), PAGE_WIDTH.into(), PAGE_HEIGHT.into()],
        }),
    );
    let catalog_id = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    document.trailer.set("Root", catalog_id);
    document.compress();
    let mut bytes = Vec::new();
    document.save_to(&mut bytes)?;
    Ok(bytes)
}

fn rgba_to_rgb(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}
