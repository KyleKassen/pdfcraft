use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::{DocId, Edit, NewAnnotation, Shape, SignaturePlacementSize, Style};

use crate::PdfCraftApp;
use crate::fill_sign::{SavedSig, SignatureSizeRequest};

fn add_document(app: &mut PdfCraftApp) -> DocId {
    let bytes = app.session.create_blank(300.0, 400.0, 1).unwrap();
    app.open_bytes("signature-request.pdf", None, bytes.as_ref().clone()).unwrap();
    let id = app.views[app.active.unwrap()].id;
    for signature in [true, true, false] {
        let shape = if signature {
            pdfcraft_engine::typed_signature_shape([30.0, 200.0], "Synthetic User", 32.0, 0).unwrap()
        } else {
            Shape::Rectangle { rect: [30.0, 180.0, 100.0, 220.0] }
        };
        app.session
            .apply(
                id,
                Edit::AddAnnotation(NewAnnotation {
                    page: 0,
                    style: Style::default_for(&shape),
                    shape,
                    contents: String::new(),
                    author: String::new(),
                }),
            )
            .unwrap();
    }
    app.views[app.active.unwrap()].comments.selected = Some((0, 0));
    id
}

fn app() -> (PdfCraftApp, DocId) {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "en").unwrap();
    app.signature = Some(SavedSig::Typed("Synthetic User".into()));
    app.signature_placement_size = Some(SignaturePlacementSize::new(70.0, 16.0).unwrap());
    let id = add_document(&mut app);
    (app, id)
}

fn paint_menu(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    let view = &mut app.views[app.active.unwrap()];
    let doc = app.session.get(view.id).unwrap();
    crate::comments::signature_size_items(ui, view, doc, app.signature.is_some(), app.initials.is_some());
}

fn cache_menu(ctx: &egui::Context, app: &mut PdfCraftApp) {
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| paint_menu(ui, app));
    output.textures_delta.clear();
}

#[derive(Debug, PartialEq)]
struct DocumentState {
    id: DocId,
    bytes: Vec<u8>,
    dirty: bool,
    undo: Option<String>,
    redo: Option<String>,
}

fn documents(app: &PdfCraftApp) -> Vec<DocumentState> {
    app.session
        .docs()
        .iter()
        .map(|doc| DocumentState {
            id: doc.id,
            bytes: app.session.save_bytes(doc.id).unwrap().as_ref().clone(),
            dirty: doc.dirty,
            undo: doc.can_undo().map(str::to_owned),
            redo: doc.can_redo().map(str::to_owned),
        })
        .collect()
}

#[test]
fn signature_size_request_rejects_stale_identity_and_removed_target_and_always_clears_pending() {
    for change in 0..5 {
        let (mut app, id) = app();
        let request = SignatureSizeRequest { doc: id, page: 0, annotation: 0, initials: false };
        let ctx = egui::Context::default();
        cache_menu(&ctx, &mut app);
        match change {
            0 => {
                add_document(&mut app);
            }
            1 => app.views[0].comments.selected = Some((0, 1)),
            2 => app.signature = None,
            3 => app.views[0].comments.selected = None,
            _ => app.active = None,
        }
        let before = documents(&app);
        let preferences = (app.signature_placement_size, app.initials_placement_size);
        assert!(!app.dispatch_signature_size_request(request), "stale request case {change} must be refused");
        assert_eq!((app.signature_placement_size, app.initials_placement_size), preferences);
        assert_eq!(documents(&app), before);
        assert!(app.pending_signature_size_request.is_none(), "early refusal must not leave a request that affects a later command");
    }
}

#[test]
fn signature_size_request_command_freshly_reads_resized_source_and_refuses_deleted_source_after_menu_cache() {
    let (mut app, id) = app();
    let ctx = egui::Context::default();
    cache_menu(&ctx, &mut app);
    app.session.apply(id, Edit::ResizeAnnotation { page: 0, index: 0, rect: [30.0, 180.0, 100.0, 197.0] }).unwrap();
    let size = app.session.get(id).unwrap().signature_placement_size(0, 0).unwrap().unwrap();
    let before = documents(&app);
    assert!(app.execute("sign.fill.signature.size"));
    assert_eq!(app.signature_placement_size, Some(size), "execution reads the current geometry rather than a cached placement size");
    assert_eq!(documents(&app), before);
    assert!(app.pending_signature_size_request.is_none());

    cache_menu(&ctx, &mut app);
    // Remove both suitable sources so the ordinary rectangle reuses the cached source index.
    for _ in 0..2 {
        app.session.apply(id, Edit::DeleteAnnotation { page: 0, index: 0 }).unwrap();
    }
    let before = documents(&app);
    assert!(!app.execute("sign.fill.signature.size"));
    assert_eq!(app.signature_placement_size, Some(size));
    assert_eq!(documents(&app), before);
    assert!(app.pending_signature_size_request.is_none());
}

#[test]
fn signature_size_menu_cache_actual_items_update_across_frames_source_changes_and_target_changes() {
    let (app, id) = app();
    let mut h = Harness::builder().with_size(egui::vec2(600.0, 300.0)).build_ui_state(paint_menu, app);
    h.run_steps(4);
    assert!(h.query_by_label("Use this size for saved signature").is_some());
    assert!(h.query_by_label("Use this size for saved initials").is_none());
    h.state_mut().views[0].comments.selected = Some((0, 2));
    h.run_steps(3);
    assert!(h.query_by_label("Use this size for saved signature").is_none(), "changing selection invalidates the previous positive result");
    h.state_mut().views[0].comments.selected = Some((0, 0));
    h.run_steps(3);
    assert!(h.query_by_label("Use this size for saved signature").is_some());
    for _ in 0..2 {
        h.state_mut().session.apply(id, Edit::DeleteAnnotation { page: 0, index: 0 }).unwrap();
    }
    h.run_steps(3);
    assert!(h.query_by_label("Use this size for saved signature").is_none(), "an ordinary source at the same index is not eligible");
    h.state_mut().session.undo(id).unwrap();
    h.run_steps(3);
    assert!(h.query_by_label("Use this size for saved signature").is_some());
    h.state_mut().signature = None;
    h.run_steps(3);
    assert!(h.query_by_label("Use this size for saved signature").is_none());
    h.state_mut().initials = Some(SavedSig::Typed("SU".into()));
    h.run_steps(3);
    h.get_by_label("Use this size for saved initials").click();
    h.run_steps(2);
    let request = h.state_mut().views[0].comments.signature_size_request.take().unwrap();
    assert_eq!(request, SignatureSizeRequest { doc: id, page: 0, annotation: 0, initials: true });
    let before = documents(h.state());
    assert!(h.state_mut().dispatch_signature_size_request(request));
    assert!(h.state().initials_placement_size.is_some());
    assert_eq!(documents(h.state()), before);
    assert!(h.state().pending_signature_size_request.is_none());
}
