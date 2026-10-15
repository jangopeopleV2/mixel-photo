//! The real menu/Save/Folder continuations with an in-memory host authority and writer.
//! No native panel, bookmark, filesystem write or sandbox exception is used by these tests.

use super::*;
use crate::{FileDialogAnswer, FileDialogReply, FileDialogRequest, Services, menus};
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Debug)]
enum Mode {
    Quick,
    Confirm,
}

const MODES: [Mode; 2] = [Mode::Quick, Mode::Confirm];

struct Access {
    strict: bool,
    folders: RefCell<Vec<PathBuf>>,
    moved: RefCell<Option<(PathBuf, PathBuf)>>,
    checks: Cell<usize>,
    deny_check: Cell<Option<usize>>,
}

impl LayerExportAccess for Access {
    fn requires_directory(&self) -> bool {
        self.strict
    }

    fn resolve(&self, path: &Path) -> Result<PathBuf, String> {
        if let Some((old, new)) = self.moved.borrow().as_ref()
            && let Ok(relative) = path.strip_prefix(old)
        {
            return Ok(new.join(relative));
        }
        Ok(path.to_path_buf())
    }

    fn directory_granted(&self, path: &Path) -> bool {
        let check = self.checks.get() + 1;
        self.checks.set(check);
        self.deny_check.get() != Some(check) && self.folders.borrow().iter().any(|folder| path.starts_with(folder))
    }
}

type Encoded = (String, u32, u32, ExportSettings);
type Written = (String, Vec<u8>);

struct Flow {
    app: PhotocraftApp,
    ctx: egui::Context,
    mode: Mode,
    access: Rc<Access>,
    requests: Rc<RefCell<Vec<FileDialogRequest>>>,
    reply: Rc<RefCell<Option<FileDialogReply>>>,
    encoded: Rc<RefCell<Vec<Encoded>>>,
    written: Rc<RefCell<Vec<Written>>>,
    folder: PathBuf,
}

impl Flow {
    fn new(mode: Mode, strict: bool) -> Self {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let reply = Rc::new(RefCell::new(None));
        let encoded = Rc::new(RefCell::new(Vec::new()));
        let written = Rc::new(RefCell::new(Vec::new()));
        let access =
            Rc::new(Access { strict, folders: RefCell::new(Vec::new()), moved: RefCell::new(None), checks: Cell::new(0), deny_check: Cell::new(None) });
        let (requests_out, reply_out, encoded_out, written_out) = (requests.clone(), reply.clone(), encoded.clone(), written.clone());
        let services = Services {
            layer_export_access: Some(access.clone()),
            file_dialog: Some(Box::new(move |request, _, reply| {
                requests_out.borrow_mut().push(request);
                assert!(reply_out.borrow_mut().replace(reply).is_none(), "one native request at a time");
            })),
            export: Some(Box::new(move |doc, _, settings| {
                encoded_out.borrow_mut().push((doc.name.clone(), doc.size.width, doc.size.height, settings.clone()));
                Ok((format!("{}:{}x{}", doc.name, doc.size.width, doc.size.height).into_bytes(), Vec::new()))
            })),
            write: Some(Box::new(move |path, bytes| {
                written_out.borrow_mut().push((path.to_string(), bytes.to_vec()));
                Ok(())
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        app.run("file.new", json!({"name": "Original", "width": 4, "height": 4})).unwrap();
        for (name, color) in [("Captured red", "#ff0000"), ("Captured green", "#00ff00")] {
            app.run("layer.new.layer", json!({"name": name})).unwrap();
            app.run("paint.bucket", json!({"x": 0, "y": 0, "color": color})).unwrap();
        }
        app.run("select.allLayers", json!({})).unwrap();
        assert_eq!(app.session.active().unwrap().selected_layers().len(), 2);
        let folder = std::env::current_dir().unwrap().join("__mixel_photo_batch_access_fixture__").join("exports");
        Self { app, ctx: egui::Context::default(), mode, access, requests, reply, encoded, written, folder }
    }

    fn start(&mut self) {
        match self.mode {
            Mode::Quick => {
                menus::invoke(&mut self.app, &self.ctx, "layer.quickExportAsPng", Value::Null).unwrap();
            }
            Mode::Confirm => {
                let result = menus::invoke(&mut self.app, &self.ctx, "layer.exportAs", Value::Null).unwrap();
                let id = result["dialog"].as_u64().unwrap();
                let mut fields = self.app.ui.dialog_mut(id).unwrap().fields.clone();
                assert_eq!(fields["__layers"].as_array().unwrap().len(), 2);
                fields.insert("format".into(), json!("jpg"));
                fields.insert("scale".into(), json!(50));
                fields.insert("quality".into(), json!(37));
                fields.insert("metadata".into(), json!("all"));
                confirm(&mut self.app, &fields).unwrap();
            }
        }
        self.app.poll_file_dialog(&self.ctx, None);
        assert!(matches!(self.requests.borrow().as_slice(), [FileDialogRequest::Save { .. }]));
    }

    fn path(&self) -> String {
        self.folder
            .join(match self.mode {
                Mode::Quick => "selected.png",
                Mode::Confirm => "selected.jpg",
            })
            .to_string_lossy()
            .into_owned()
    }

    fn answer(&mut self, answer: Option<FileDialogAnswer>) {
        self.reply.borrow_mut().take().unwrap().send(answer);
        self.app.poll_file_dialog(&self.ctx, None);
    }

    fn saved(&mut self) {
        self.answer(Some(FileDialogAnswer::SaveTo(self.path())));
    }

    fn show_folder(&mut self) {
        self.app.poll_file_dialog(&self.ctx, None);
        assert!(
            matches!(self.requests.borrow().as_slice(), [FileDialogRequest::Save { .. }, FileDialogRequest::Folder { initial_dir: Some(dir) }] if Path::new(dir) == self.folder)
        );
        self.zero_outputs();
    }

    fn choose(&mut self, folder: PathBuf, retain: bool) {
        // Like the native host: retain the user's explicit selection before answering the UI.
        if retain {
            self.access.folders.borrow_mut().push(folder.clone());
        }
        self.answer(Some(FileDialogAnswer::FolderTo(folder.to_string_lossy().into_owned())));
    }

    fn zero_outputs(&self) {
        assert!(self.encoded.borrow().is_empty(), "permission must precede even the first encoder");
        assert!(self.written.borrow().is_empty(), "no partial first-file export");
    }

    fn completed(&self) {
        assert!(!self.app.ui.status_error, "{}", self.app.ui.status);
        assert!(!self.app.file_dialog_open());
        assert_eq!(self.encoded.borrow().len(), 2);
        assert_eq!(self.written.borrow().len(), 2);
        assert_eq!(self.written.borrow()[0].0, self.path());
        assert!(self.written.borrow().iter().all(|(path, _)| Path::new(path).parent() == Some(self.folder.as_path())));
    }
}

#[test]
fn mas_layer_exports_wait_for_the_explicit_folder_before_any_output() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.start();
        flow.zero_outputs();
        flow.saved();
        flow.zero_outputs();
        assert!(flow.access.folders.borrow().is_empty(), "a Save file must not infer its parent grant");
        assert!(flow.app.file_dialog_open());
        flow.show_folder();
        flow.choose(flow.folder.clone(), true);
        flow.completed();
    }
}

#[test]
fn mas_layer_export_folder_cancel_preserves_the_document_and_writes_nothing() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        let before = flow.app.session.active().unwrap().doc.clone();
        let selected = flow.app.session.active().unwrap().selected_layers();
        flow.start();
        flow.saved();
        flow.show_folder();
        flow.answer(None);
        flow.zero_outputs();
        assert!(!flow.app.file_dialog_open());
        assert!(!flow.app.ui.status_error);
        assert_eq!(flow.app.session.active().unwrap().doc, before);
        assert_eq!(flow.app.session.active().unwrap().selected_layers(), selected);
    }
}

#[test]
fn mas_layer_export_wrong_or_prefix_sibling_folder_never_writes() {
    for mode in MODES {
        for sibling in ["other", "exports-unrelated"] {
            let mut flow = Flow::new(mode, true);
            flow.start();
            flow.saved();
            flow.show_folder();
            // Even a separately acquired correct grant must not turn this wrong answer into consent.
            flow.access.folders.borrow_mut().push(flow.folder.clone());
            flow.choose(flow.folder.with_file_name(sibling), true);
            flow.zero_outputs();
            assert!(flow.app.ui.status_error);
            assert_eq!(flow.app.ui.status, EXPORT_FOLDER);
            assert!(!flow.app.file_dialog_open());
        }
    }
}

#[test]
fn mas_layer_export_a_folder_answer_without_a_retained_grant_never_writes() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.start();
        flow.saved();
        flow.show_folder();
        flow.choose(flow.folder.clone(), false);
        flow.zero_outputs();
        assert!(flow.app.ui.status_error);
        assert_eq!(flow.app.ui.status, EXPORT_FOLDER);
    }
}

#[test]
fn mas_layer_export_unavailable_unexpected_or_disconnected_folder_never_writes() {
    for mode in MODES {
        for kind in 0..3 {
            let mut flow = Flow::new(mode, true);
            flow.start();
            flow.saved();
            flow.show_folder();
            match kind {
                0 => flow.answer(Some(FileDialogAnswer::Unavailable("native folder failed".into()))),
                1 => flow.answer(Some(FileDialogAnswer::SaveTo(flow.path()))),
                _ => {
                    drop(flow.reply.borrow_mut().take());
                    flow.app.poll_file_dialog(&flow.ctx, None);
                }
            }
            flow.zero_outputs();
            assert!(!flow.app.file_dialog_open());
            assert_eq!(flow.app.ui.status_error, kind != 2, "only a dropped reply is silent Cancel");
        }
    }
}

#[test]
fn mas_layer_export_an_existing_directory_grant_needs_no_second_panel() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.access.folders.borrow_mut().push(flow.folder.clone());
        flow.start();
        flow.saved();
        flow.completed();
        assert_eq!(flow.requests.borrow().len(), 1);
    }
}

#[test]
fn mas_layer_export_accepts_an_explicit_containing_folder() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.start();
        flow.saved();
        flow.show_folder();
        flow.choose(flow.folder.parent().unwrap().to_path_buf(), true);
        flow.completed();
    }
}

#[test]
fn mas_layer_export_keeps_captured_sources_and_options_across_both_panels() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        let original = flow.app.session.active().unwrap().doc.clone();
        let selection = flow.app.session.active().unwrap().selected_layers();
        let names: Vec<_> = selection.iter().map(|id| photocraft_engine::layer_menu_cmds::layer_document(&original, *id).unwrap().name).collect();
        flow.start();
        flow.app.run("layer.select", json!({"layer": selection[0].0})).unwrap();
        flow.saved();
        flow.show_folder();
        flow.app.run("file.new", json!({"name": "Unrelated", "width": 9, "height": 7})).unwrap();
        let active = flow.app.session.active().unwrap().doc.id;
        flow.choose(flow.folder.clone(), true);
        flow.completed();
        assert_eq!(flow.app.session.active().unwrap().doc.id, active, "do not retarget or switch the current document");
        assert_eq!(flow.app.session.documents().iter().find(|d| d.doc.id == original.id).unwrap().doc, original);
        assert_eq!(flow.encoded.borrow().iter().map(|v| v.0.clone()).collect::<Vec<_>>(), names);
        for (_, w, h, settings) in flow.encoded.borrow().iter() {
            match mode {
                Mode::Quick => {
                    assert_eq!((*w, *h), (4, 4));
                    assert!(!settings.xmp_all);
                }
                Mode::Confirm => {
                    assert_eq!((*w, *h), (2, 2));
                    assert_eq!(settings.jpeg_quality, Some(37));
                    assert!(settings.xmp_all);
                }
            }
        }
    }
}

#[test]
fn mas_layer_export_preflights_every_actual_path_before_the_first_encoder() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.access.folders.borrow_mut().push(flow.folder.clone());
        // Initial directory check, planning check, first output, second output: reject the last.
        flow.access.deny_check.set(Some(4));
        flow.start();
        flow.saved();
        assert_eq!(flow.access.checks.get(), 4);
        flow.zero_outputs();
        assert!(flow.app.ui.status_error);
        assert_eq!(flow.app.ui.status, EXPORT_FOLDER);
    }
}

#[test]
fn mas_layer_export_resolves_a_moved_destination_before_planning_siblings() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        let old = flow.folder.clone();
        flow.start();
        flow.saved();
        flow.show_folder();
        flow.folder = old.with_file_name("moved-exports");
        *flow.access.moved.borrow_mut() = Some((old, flow.folder.clone()));
        flow.choose(flow.folder.clone(), true);
        flow.completed();
    }
}

#[test]
fn mas_layer_export_relative_and_parent_traversal_destinations_never_write() {
    for mode in MODES {
        for relative in [true, false] {
            let mut flow = Flow::new(mode, true);
            let path = if relative { "relative.png".into() } else { flow.folder.join("..").join("escaped.png").to_string_lossy().into_owned() };
            flow.start();
            flow.answer(Some(FileDialogAnswer::SaveTo(path)));
            flow.zero_outputs();
            assert!(flow.app.ui.status_error);
            assert_eq!(flow.requests.borrow().len(), 1);
        }
    }
}

#[test]
fn cancelling_the_original_batch_save_never_requests_a_folder_or_writes() {
    for mode in MODES {
        let mut flow = Flow::new(mode, true);
        flow.start();
        flow.answer(None);
        flow.zero_outputs();
        assert_eq!(flow.requests.borrow().len(), 1);
        assert!(!flow.app.ui.status_error);
    }
}

#[test]
fn quick_layer_batch_rejects_non_png_before_requesting_folder_access() {
    let mut flow = Flow::new(Mode::Quick, true);
    flow.start();
    flow.answer(Some(FileDialogAnswer::SaveTo(flow.folder.join("wrong.jpg").to_string_lossy().into_owned())));
    flow.zero_outputs();
    assert!(flow.app.ui.status_error);
    assert_eq!(flow.requests.borrow().len(), 1);
}

#[test]
fn ordinary_layer_batches_keep_the_original_one_save_panel_flow() {
    for mode in MODES {
        let mut flow = Flow::new(mode, false);
        flow.start();
        flow.saved();
        flow.completed();
        assert_eq!(flow.requests.borrow().len(), 1);
        assert_eq!(flow.access.checks.get(), 0);
    }
}

#[test]
fn mas_single_layer_quick_export_still_uses_only_its_save_file_grant() {
    let mut flow = Flow::new(Mode::Quick, true);
    let layer = flow.app.session.active().unwrap().selected_layers()[0];
    flow.app.run("layer.select", json!({"layer": layer.0})).unwrap();
    menus::invoke(&mut flow.app, &flow.ctx, "layer.quickExportAsPng", Value::Null).unwrap();
    flow.app.poll_file_dialog(&flow.ctx, None);
    flow.saved();
    assert!(!flow.app.ui.status_error, "{}", flow.app.ui.status);
    assert_eq!(flow.written.borrow().len(), 1);
    assert_eq!(flow.encoded.borrow().len(), 1);
    assert_eq!(flow.requests.borrow().len(), 1);
    assert_eq!(flow.access.checks.get(), 0);
}

#[test]
fn mas_single_layer_export_as_still_uses_only_its_save_file_grant() {
    let mut flow = Flow::new(Mode::Confirm, true);
    let layer = flow.app.session.active().unwrap().selected_layers()[0];
    flow.app.run("layer.select", json!({"layer": layer.0})).unwrap();
    let result = menus::invoke(&mut flow.app, &flow.ctx, "layer.exportAs", Value::Null).unwrap();
    let mut fields = flow.app.ui.dialog_mut(result["dialog"].as_u64().unwrap()).unwrap().fields.clone();
    assert!(fields.get("__layers").is_none());
    fields.insert("format".into(), json!("jpg"));
    confirm(&mut flow.app, &fields).unwrap();
    flow.app.poll_file_dialog(&flow.ctx, None);
    flow.saved();
    assert!(!flow.app.ui.status_error, "{}", flow.app.ui.status);
    assert_eq!(flow.written.borrow().len(), 1);
    assert_eq!(flow.encoded.borrow().len(), 1);
    assert_eq!(flow.requests.borrow().len(), 1);
    assert_eq!(flow.access.checks.get(), 0);
}
