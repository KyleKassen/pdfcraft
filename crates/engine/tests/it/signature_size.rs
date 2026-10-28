//! Original tiny appearance signatures: physical sizing, padding and atomic undo.

use std::sync::Arc;

use pdfcraft_cos::{Document as Cos, Object, SaveOptions};
use pdfcraft_engine::{
    Edit, MarkFile, NewAnnotation, Session, Shape, SignatureImage, SignaturePlacementSize, Style, signature_placement_with_size,
    typed_signature_shape,
};

fn page(rotation: u16, unit: f64) -> (Session, pdfcraft_engine::DocId) {
    let mut session = Session::new().with_clock(|| 1_700_000_000);
    let bytes = session.create_blank(300.0, 400.0, 1).unwrap();
    let mut cos = Cos::open(bytes).unwrap();
    let p = pdfcraft_annot::page_refs(&cos).unwrap()[0];
    cos.update_dict(p, |d| {
        let crop = || Object::Array([10.0, 20.0, 310.0, 420.0].map(Object::Real).to_vec());
        d.set(b"MediaBox".to_vec(), crop());
        d.set(b"CropBox".to_vec(), crop());
        d.set(b"Rotate".to_vec(), Object::Int(i64::from(rotation)));
        d.set(b"UserUnit".to_vec(), Object::Real(unit));
    })
    .unwrap();
    let bytes = Arc::new(pdfcraft_cos::write_full(&cos, &SaveOptions::default()).unwrap());
    let id = session.open_new("original-signature-page.pdf", bytes).unwrap();
    (session, id)
}

fn annotation(shape: Shape) -> Edit {
    Edit::AddAnnotation(NewAnnotation { page: 0, style: Style::default_for(&shape), shape, contents: String::new(), author: "Synthetic User".into() })
}

fn image() -> SignatureImage {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 60, 20);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[20, 40, 60, 128].repeat(60 * 20)).unwrap();
    }
    SignatureImage::from_bytes(&bytes).unwrap()
}

fn placement(kind: usize, info: &pdfcraft_render::PageInfo, at: [f64; 2]) -> Edit {
    match kind {
        0 => annotation(typed_signature_shape(at, "Synthetic User", 32.0, i64::from(info.rotation)).unwrap()),
        1 => annotation(Shape::Signature {
            // Uneven segments exercise the smoothed-ink control bounds and fixed padding.
            strokes: vec![vec![[80.0, 160.0], [110.0, 185.0], [155.0, 165.0], [190.0, 180.0]]],
        }),
        _ => image().edit(0, info, at, false, "Synthetic User").unwrap(),
    }
}

#[test]
fn signature_size_matches_physical_rect_for_all_graphics_rotations_and_user_units() {
    for rotation in [0, 90, 180, 270] {
        for unit in [1.0, 2.5] {
            for kind in 0..3 {
                let (mut s, id) = page(rotation, unit);
                let info = s.get(id).unwrap().info.pages[0].clone();
                let at = [100.0, 180.0];
                let size = SignaturePlacementSize::new(65.0, 17.0).unwrap();
                let expected = size.rect_at(&info, at).unwrap();
                let before = s.save_bytes(id).unwrap();
                let edit = signature_placement_with_size(placement(kind, &info, at), &info, at, size).unwrap();
                s.apply(id, edit).unwrap();
                let d = s.get(id).unwrap();
                assert_eq!(d.info.annotations.len(), 1);
                let actual = d.signature_placement_size(0, 0).unwrap().unwrap();
                assert!((actual.width() - 65.0).abs() < 0.001, "{rotation}/{unit}/{kind}: {actual:?}");
                assert!((actual.height() - 17.0).abs() < 0.001, "{rotation}/{unit}/{kind}: {actual:?}");
                for (a, b) in d.info.annotations[0].rect.into_iter().map(f64::from).zip(expected) {
                    assert!((a - b).abs() < 0.001, "the preview and actual anchor must agree");
                }
                let after = s.save_bytes(id).unwrap();
                s.undo(id).unwrap();
                assert_eq!(s.save_bytes(id).unwrap(), before, "one undo removes the complete placement");
                assert!(s.get(id).unwrap().can_undo().is_none());
                s.redo(id).unwrap();
                assert_eq!(s.save_bytes(id).unwrap(), after);
                let reopened = s.open_new("reopened.pdf", after).unwrap();
                let actual = s.get(reopened).unwrap().signature_placement_size(0, 0).unwrap().unwrap();
                assert!((actual.width() - 65.0).abs() < 0.001 && (actual.height() - 17.0).abs() < 0.001);
            }
        }
    }
}

#[test]
fn signature_size_keeps_legacy_geometry_without_an_explicit_request() {
    let (mut s, id) = page(0, 1.0);
    let info = s.get(id).unwrap().info.pages[0].clone();
    s.apply(id, placement(0, &info, [30.0, 250.0])).unwrap();
    assert_eq!(s.get(id).unwrap().signature_placement_size(0, 0).unwrap().unwrap().height(), 32.0);
    s.apply(id, image().edit(0, &info, [30.0, 180.0], true, "").unwrap()).unwrap();
    let actual = s.get(id).unwrap().signature_placement_size(0, 1).unwrap().unwrap();
    assert_eq!((actual.width(), actual.height()), (72.0, 24.0));
    assert_eq!(&image().rgba()[..4], &[20, 40, 60, 128]);
}

#[test]
fn signature_size_rejects_hostile_dimensions_and_transforms() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0, 0.0, 0.009, 14400.01] {
        assert!(SignaturePlacementSize::new(bad, 10.0).is_err());
        assert!(SignaturePlacementSize::new(10.0, bad).is_err());
    }
    let (s, id) = page(0, 1.0);
    let info = &s.get(id).unwrap().info.pages[0];
    let size = SignaturePlacementSize::new(65.0, 17.0).unwrap();
    for at in [[f64::NAN, 1.0], [1.0, f64::INFINITY], [f64::MAX, 1.0]] {
        assert!(size.rect_at(info, at).is_err());
    }
    for kind in 0..6 {
        let mut invalid = info.clone();
        match kind {
            0 => invalid.crop[2] = invalid.crop[0],
            1 => invalid.crop[1] = f32::NAN,
            2 => invalid.width = 0.0,
            3 => invalid.height = f32::INFINITY,
            4 => invalid.rotation = 45,
            _ => invalid.width *= 2.0,
        }
        assert!(size.rect_at(&invalid, [100.0, 180.0]).is_err());
    }
}

#[test]
fn signature_size_rejects_wrong_kinds_and_runtime_failures_atomically() {
    let (mut s, id) = page(0, 1.0);
    let info = s.get(id).unwrap().info.pages[0].clone();
    let size = SignaturePlacementSize::new(65.0, 17.0).unwrap();
    assert!(signature_placement_with_size(annotation(Shape::Rectangle { rect: [10.0, 10.0, 50.0, 50.0] }), &info, [20.0, 100.0], size).is_err());
    let before = s.save_bytes(id).unwrap();
    let before_dirty = s.get(id).unwrap().dirty;
    // A PDF stamp with a forged signature name is not an image signature. Its import
    // succeeds before the kind check fails, exercising rollback of newly added objects.
    let fake = Edit::AddCustomStamp {
        page: 0,
        rect: [20.0, 100.0, 80.0, 120.0],
        name: "Signature".into(),
        file: MarkFile { name: "original.pdf".into(), bytes: before.clone(), page: 0 },
        author: String::new(),
    };
    let fake = signature_placement_with_size(fake, &info, [20.0, 100.0], size).unwrap();
    assert!(s.apply(id, fake).is_err());
    assert_eq!(s.save_bytes(id).unwrap(), before);
    assert_eq!(s.get(id).unwrap().dirty, before_dirty);
    assert!(s.get(id).unwrap().can_undo().is_none());
    let tiny = SignaturePlacementSize::new(0.01, 0.01).unwrap();
    let edit = signature_placement_with_size(placement(1, &info, [20.0, 100.0]), &info, [20.0, 100.0], tiny).unwrap();
    assert!(s.apply(id, edit).is_err(), "an ink rectangle smaller than its stroke padding cannot fit");
    assert_eq!(s.save_bytes(id).unwrap(), before);
    assert_eq!(s.get(id).unwrap().dirty, before_dirty);
    assert!(s.get(id).unwrap().info.annotations.is_empty());
    assert!(s.get(id).unwrap().can_undo().is_none());
    // Refusal does not poison the next successful placement.
    s.apply(id, signature_placement_with_size(placement(2, &info, [20.0, 100.0]), &info, [20.0, 100.0], size).unwrap()).unwrap();
    assert_eq!(s.get(id).unwrap().info.annotations.len(), 1);
}

#[test]
fn signature_size_query_ignores_marks_widgets_and_unrelated_stamps() {
    let (mut s, id) = page(0, 1.0);
    for shape in [
        Shape::Rectangle { rect: [10.0, 10.0, 50.0, 50.0] },
        Shape::Mark { rect: [60.0, 10.0, 80.0, 30.0], mark: pdfcraft_engine::FillMark::Check },
        Shape::Typewriter { rect: [90.0, 10.0, 140.0, 30.0], font_size: 10.0 },
        Shape::Stamp { rect: [150.0, 10.0, 220.0, 40.0], stamp: pdfcraft_engine::StampKind::Draft, by: None },
    ] {
        s.apply(id, annotation(shape)).unwrap();
    }
    for index in 0..4 {
        assert_eq!(s.get(id).unwrap().signature_placement_size(0, index).unwrap(), None);
    }
    assert!(s.get(id).unwrap().signature_placement_size(0, 99).is_err());
    assert!(s.get(id).unwrap().signature_placement_size(1, 0).is_err());
}

#[test]
fn signature_size_does_not_resize_existing_instances_and_renders_the_full_image() {
    let (mut s, id) = page(90, 2.5);
    let info = s.get(id).unwrap().info.pages[0].clone();
    let at = [100.0, 180.0];
    let size = SignaturePlacementSize::new(65.0, 17.0).unwrap();
    s.apply(id, signature_placement_with_size(placement(2, &info, at), &info, at, size).unwrap()).unwrap();
    let original = s.get(id).unwrap().info.annotations[0].rect;
    let rendered = pdfcraft_render::PageRenderer::new(s.get(id).unwrap().display.clone(), pdfcraft_render::RenderConfig::default())
        .render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(rendered.error.is_none(), "{:?}", rendered.error);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (i, pixel) in rendered.rgba.as_chunks::<4>().0.iter().enumerate() {
        if pixel[0] < 200 && pixel[1] < 200 && pixel[2] < 200 {
            let x = i as u32 % rendered.width;
            let y = i as u32 / rendered.width;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    assert!(x0 <= x1 && y0 <= y1, "the original transparent image must draw");
    assert!((64..=67).contains(&(x1 - x0 + 1)), "physical width: {}", x1 - x0 + 1);
    assert!((16..=19).contains(&(y1 - y0 + 1)), "physical height: {}", y1 - y0 + 1);
    let second = SignaturePlacementSize::new(90.0, 25.0).unwrap();
    let at = [150.0, 270.0];
    s.apply(id, signature_placement_with_size(placement(2, &info, at), &info, at, second).unwrap()).unwrap();
    let document = s.get(id).unwrap();
    assert_eq!(document.info.annotations.len(), 2);
    // Inspection sorts annotations by their displayed vertical position, independently
    // of their stable page /Annots indices. The higher second placement sorts first.
    let first = document.info.annotations.iter().find(|a| a.page == 0 && a.index == 0).unwrap();
    assert_eq!(first.rect, original);
    let actual = s.get(id).unwrap().signature_placement_size(0, 1).unwrap().unwrap();
    assert!((actual.width() - 90.0).abs() < 0.001 && (actual.height() - 25.0).abs() < 0.001);
}

#[test]
fn signature_size_uses_normal_annotation_permissions() {
    let (mut s, id) = page(0, 1.0);
    s.apply(
        id,
        Edit::Protect(pdfcraft_engine::Protection {
            permissions_password: Some("synthetic-owner".into()),
            algorithm: pdfcraft_cos::Algorithm::Aes128,
            ..Default::default()
        }),
    )
    .unwrap();
    let protected = s.save_bytes(id).unwrap();
    let restricted = s.open_new("restricted.pdf", protected).unwrap();
    let info = s.get(restricted).unwrap().info.pages[0].clone();
    let edit =
        signature_placement_with_size(placement(0, &info, [100.0, 180.0]), &info, [100.0, 180.0], SignaturePlacementSize::new(65.0, 17.0).unwrap())
            .unwrap();
    let before = s.save_bytes(restricted).unwrap();
    let before_dirty = s.get(restricted).unwrap().dirty;
    assert!(matches!(s.apply(restricted, edit), Err(pdfcraft_engine::EditError::NotPermitted("comments"))));
    assert_eq!(s.save_bytes(restricted).unwrap(), before);
    assert_eq!(s.get(restricted).unwrap().dirty, before_dirty);
    assert!(s.get(restricted).unwrap().can_undo().is_none());
}
