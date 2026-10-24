use crate::config::{APP_ID, APP_NAME, APP_VERSION};
use crate::crop_canvas::CropCanvas;
use crate::image_processor::{ImageProcessError, LoadedImage};
use gdk4::glib;
use gio::SimpleAction;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use libadwaita::prelude::*;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

mod imp {
    use super::*;

    pub struct CropWindow {
        pub(super) toast_overlay: RefCell<Option<libadwaita::ToastOverlay>>,
        pub(super) toolbar_view: RefCell<Option<libadwaita::ToolbarView>>,
        pub(super) window_title: RefCell<Option<libadwaita::WindowTitle>>,
        pub(super) stack: RefCell<Option<gtk4::Stack>>,
        pub(super) crop_canvas: RefCell<Option<CropCanvas>>,
        pub(super) save_btn: RefCell<Option<gtk4::Button>>,
        pub(super) open_btn: RefCell<Option<gtk4::Button>>,
        pub(super) size_button_content: RefCell<Option<libadwaita::ButtonContent>>,
        pub(super) spin_width: RefCell<Option<gtk4::SpinButton>>,
        pub(super) spin_height: RefCell<Option<gtk4::SpinButton>>,
        pub(super) toggle_lock: RefCell<Option<gtk4::ToggleButton>>,
        pub(super) spinner: RefCell<Option<gtk4::Spinner>>,
        pub(super) bottom_bar: RefCell<Option<gtk4::Box>>,
        pub(super) label_dims: RefCell<Option<gtk4::Label>>,
        pub(super) label_pos: RefCell<Option<gtk4::Label>>,
        pub(super) label_scale: RefCell<Option<gtk4::Label>>,

        pub(super) current_image: RefCell<Option<Arc<LoadedImage>>>,
        pub(super) current_file_path: RefCell<Option<PathBuf>>,
        pub(super) current_scale: RefCell<f64>,

        pub(super) crop_width: Cell<u32>,
        pub(super) crop_height: Cell<u32>,
        pub(super) lock_aspect_ratio: Cell<bool>,

        pub(super) settings: RefCell<Option<gio::Settings>>,
        pub(super) last_open_dir: RefCell<Option<PathBuf>>,
        pub(super) last_save_dir: RefCell<Option<PathBuf>>,
    }

    impl Default for CropWindow {
        fn default() -> Self {
            Self {
                toast_overlay: RefCell::new(None),
                toolbar_view: RefCell::new(None),
                window_title: RefCell::new(None),
                stack: RefCell::new(None),
                crop_canvas: RefCell::new(None),
                save_btn: RefCell::new(None),
                open_btn: RefCell::new(None),
                size_button_content: RefCell::new(None),
                spin_width: RefCell::new(None),
                spin_height: RefCell::new(None),
                toggle_lock: RefCell::new(None),
                spinner: RefCell::new(None),
                bottom_bar: RefCell::new(None),
                label_dims: RefCell::new(None),
                label_pos: RefCell::new(None),
                label_scale: RefCell::new(None),
                current_image: RefCell::new(None),
                current_file_path: RefCell::new(None),
                current_scale: RefCell::new(1.0),
                crop_width: Cell::new(1000),
                crop_height: Cell::new(1000),
                lock_aspect_ratio: Cell::new(true),
                settings: RefCell::new(None),
                last_open_dir: RefCell::new(None),
                last_save_dir: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CropWindow {
        const NAME: &'static str = "CropWindow";
        type Type = super::CropWindow;
        type ParentType = libadwaita::ApplicationWindow;
    }

    impl ObjectImpl for CropWindow {
        fn constructed(&self) {
            self.parent_constructed();
            let window = self.obj();
            window.setup_settings();
            window.setup_ui();
            window.setup_actions();
            window.setup_drop_target();
        }
    }

    impl WidgetImpl for CropWindow {}
    impl WindowImpl for CropWindow {}
    impl ApplicationWindowImpl for CropWindow {}
    impl libadwaita::subclass::application_window::AdwApplicationWindowImpl for CropWindow {}
}

glib::wrapper! {
    pub struct CropWindow(ObjectSubclass<imp::CropWindow>)
        @extends gtk4::Widget, gtk4::Window, gtk4::ApplicationWindow, libadwaita::ApplicationWindow,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Native, gtk4::Root, gtk4::ShortcutManager, gio::ActionGroup, gio::ActionMap;
}

impl CropWindow {
    pub fn new<P: IsA<gtk4::Application>>(app: &P) -> Self {
        glib::Object::builder()
            .property("application", app)
            .property("title", APP_NAME)
            .property("default-width", 960)
            .property("default-height", 720)
            .build()
    }

    fn setup_settings(&self) {
        let imp = self.imp();
        let schema_source = gio::SettingsSchemaSource::default();
        let has_schema = schema_source
            .and_then(|src| src.lookup(APP_ID, true))
            .is_some();

        if has_schema {
            let settings = gio::Settings::new(APP_ID);

            // 前回開いたフォルダの読み込み
            let open_str = settings.string("last-open-folder").to_string();
            if !open_str.is_empty() {
                let p = PathBuf::from(open_str);
                if p.is_dir() {
                    *imp.last_open_dir.borrow_mut() = Some(p);
                }
            }

            // 前回保存したフォルダの読み込み
            let save_str = settings.string("last-save-folder").to_string();
            if !save_str.is_empty() {
                let p = PathBuf::from(save_str);
                if p.is_dir() {
                    *imp.last_save_dir.borrow_mut() = Some(p);
                }
            }

            // 切り抜きサイズの読み込み
            let cw = settings.int("crop-width");
            if (100..=10000).contains(&cw) {
                imp.crop_width.set(cw as u32);
            }
            let ch = settings.int("crop-height");
            if (100..=10000).contains(&ch) {
                imp.crop_height.set(ch as u32);
            }
            let lock = settings.boolean("lock-aspect-ratio");
            imp.lock_aspect_ratio.set(lock);

            *imp.settings.borrow_mut() = Some(settings);
        }
    }

    fn set_crop_size_from_ui(&self, w: u32, h: u32, update_spins: bool) {
        let imp = self.imp();
        let w = w.clamp(100, 10000);
        let h = h.clamp(100, 10000);
        imp.crop_width.set(w);
        imp.crop_height.set(h);

        if let Some(content) = imp.size_button_content.borrow().as_ref() {
            content.set_label(&format!("{} × {}", w, h));
        }

        if update_spins {
            if let Some(spin_w) = imp.spin_width.borrow().as_ref() {
                if (spin_w.value() as u32) != w {
                    spin_w.set_value(w as f64);
                }
            }
            if let Some(spin_h) = imp.spin_height.borrow().as_ref() {
                if (spin_h.value() as u32) != h {
                    spin_h.set_value(h as f64);
                }
            }
        }

        if let Some(canvas) = imp.crop_canvas.borrow().as_ref() {
            canvas.set_crop_size(w, h);
        }

        if let Some(settings) = imp.settings.borrow().as_ref() {
            let _ = settings.set_int("crop-width", w as i32);
            let _ = settings.set_int("crop-height", h as i32);
        }
    }

    fn update_last_open_dir(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let parent_buf = parent.to_path_buf();
            let imp = self.imp();
            *imp.last_open_dir.borrow_mut() = Some(parent_buf.clone());

            if let Some(settings) = imp.settings.borrow().as_ref() {
                let _ = settings.set_string("last-open-folder", parent.to_string_lossy().as_ref());
            }
        }
    }

    fn update_last_save_dir(&self, path: &PathBuf) {
        if let Some(parent) = path.parent() {
            let parent_buf = parent.to_path_buf();
            let imp = self.imp();
            *imp.last_save_dir.borrow_mut() = Some(parent_buf.clone());

            if let Some(settings) = imp.settings.borrow().as_ref() {
                let _ = settings.set_string("last-save-folder", parent.to_string_lossy().as_ref());
            }
        }
    }

    fn setup_ui(&self) {
        let imp = self.imp();

        let toast_overlay = libadwaita::ToastOverlay::new();
        let toolbar_view = libadwaita::ToolbarView::new();

        // --- Header Bar ---
        let header_bar = libadwaita::HeaderBar::new();

        // 開くボタン
        let open_btn = gtk4::Button::new();
        let open_content = libadwaita::ButtonContent::builder()
            .icon_name("document-open-symbolic")
            .label("開く")
            .build();
        open_btn.set_child(Some(&open_content));
        open_btn.set_action_name(Some("win.open"));
        open_btn.set_tooltip_text(Some("画像を開く (Ctrl+O)"));
        header_bar.pack_start(&open_btn);

        // 切り抜きサイズ設定ボタン (MenuButton + Popover)
        let size_btn = gtk4::MenuButton::new();
        size_btn.set_tooltip_text(Some("切り抜きサイズを変更"));
        let size_content = libadwaita::ButtonContent::builder()
            .icon_name("view-fullscreen-symbolic")
            .label(&format!("{} × {}", imp.crop_width.get(), imp.crop_height.get()))
            .build();
        size_btn.set_child(Some(&size_content));

        let popover = gtk4::Popover::new();
        let pop_box = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        pop_box.set_margin_start(14);
        pop_box.set_margin_end(14);
        pop_box.set_margin_top(14);
        pop_box.set_margin_bottom(14);

        // プリセット
        let preset_label = gtk4::Label::new(Some("プリセット"));
        preset_label.add_css_class("heading");
        preset_label.set_halign(gtk4::Align::Start);
        pop_box.append(&preset_label);

        let grid = gtk4::Grid::new();
        grid.set_column_spacing(6);
        grid.set_row_spacing(6);

        let presets = [
            ("1000 × 1000 (正方形)", 1000, 1000),
            ("1200 × 630 (Web OGP)", 1200, 630),
            ("1920 × 1080 (Full HD)", 1920, 1080),
            ("800 × 800 (アイコン)", 800, 800),
        ];

        for (i, (title, w, h)) in presets.iter().enumerate() {
            let p_btn = gtk4::Button::with_label(title);
            let win_weak = self.downgrade();
            let pop_weak = popover.downgrade();
            let pw = *w;
            let ph = *h;
            p_btn.connect_clicked(move |_| {
                if let Some(win) = win_weak.upgrade() {
                    win.set_crop_size_from_ui(pw, ph, true);
                }
                if let Some(pop) = pop_weak.upgrade() {
                    pop.popdown();
                }
            });
            let col = (i % 2) as i32;
            let row = (i / 2) as i32;
            grid.attach(&p_btn, col, row, 1, 1);
        }
        pop_box.append(&grid);

        let sep = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        pop_box.append(&sep);

        // カスタムサイズ入力
        let custom_label = gtk4::Label::new(Some("カスタムサイズ (px)"));
        custom_label.add_css_class("heading");
        custom_label.set_halign(gtk4::Align::Start);
        pop_box.append(&custom_label);

        let custom_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        custom_box.set_valign(gtk4::Align::Center);

        let spin_w = gtk4::SpinButton::with_range(100.0, 10000.0, 10.0);
        spin_w.set_value(imp.crop_width.get() as f64);
        spin_w.set_tooltip_text(Some("幅"));

        let x_label = gtk4::Label::new(Some("×"));
        x_label.add_css_class("dim-label");

        let spin_h = gtk4::SpinButton::with_range(100.0, 10000.0, 10.0);
        spin_h.set_value(imp.crop_height.get() as f64);
        spin_h.set_tooltip_text(Some("高さ"));

        let toggle_lock = gtk4::ToggleButton::new();
        toggle_lock.set_icon_name("link-symbolic");
        toggle_lock.set_active(imp.lock_aspect_ratio.get());
        toggle_lock.set_tooltip_text(Some("アスペクト比を固定"));

        custom_box.append(&spin_w);
        custom_box.append(&x_label);
        custom_box.append(&spin_h);
        custom_box.append(&toggle_lock);
        pop_box.append(&custom_box);

        popover.set_child(Some(&pop_box));
        size_btn.set_popover(Some(&popover));
        header_bar.pack_start(&size_btn);

        // スピンボタンの変更イベント連動
        let updating = Rc::new(Cell::new(false));

        let win_weak = self.downgrade();
        let updating_w = updating.clone();
        let spin_h_weak = spin_h.downgrade();
        let toggle_weak = toggle_lock.downgrade();
        spin_w.connect_value_changed(move |sw| {
            if updating_w.get() {
                return;
            }
            updating_w.set(true);

            let new_w = sw.value() as u32;
            let win = match win_weak.upgrade() {
                Some(w) => w,
                None => return,
            };
            let is_locked = toggle_weak.upgrade().map(|t| t.is_active()).unwrap_or(false);

            let new_h = if is_locked {
                let cur_w = win.imp().crop_width.get().max(1) as f64;
                let cur_h = win.imp().crop_height.get().max(1) as f64;
                let ratio = cur_h / cur_w;
                let calculated_h = (new_w as f64 * ratio).round().clamp(100.0, 10000.0) as u32;
                if let Some(sh) = spin_h_weak.upgrade() {
                    sh.set_value(calculated_h as f64);
                }
                calculated_h
            } else if let Some(sh) = spin_h_weak.upgrade() {
                sh.value() as u32
            } else {
                win.imp().crop_height.get()
            };

            win.set_crop_size_from_ui(new_w, new_h, false);
            updating_w.set(false);
        });

        let win_weak = self.downgrade();
        let updating_h = updating.clone();
        let spin_w_weak = spin_w.downgrade();
        let toggle_weak = toggle_lock.downgrade();
        spin_h.connect_value_changed(move |sh| {
            if updating_h.get() {
                return;
            }
            updating_h.set(true);

            let new_h = sh.value() as u32;
            let win = match win_weak.upgrade() {
                Some(w) => w,
                None => return,
            };
            let is_locked = toggle_weak.upgrade().map(|t| t.is_active()).unwrap_or(false);

            let new_w = if is_locked {
                let cur_w = win.imp().crop_width.get().max(1) as f64;
                let cur_h = win.imp().crop_height.get().max(1) as f64;
                let ratio = cur_w / cur_h;
                let calculated_w = (new_h as f64 * ratio).round().clamp(100.0, 10000.0) as u32;
                if let Some(sw) = spin_w_weak.upgrade() {
                    sw.set_value(calculated_w as f64);
                }
                calculated_w
            } else if let Some(sw) = spin_w_weak.upgrade() {
                sw.value() as u32
            } else {
                win.imp().crop_width.get()
            };

            win.set_crop_size_from_ui(new_w, new_h, false);
            updating_h.set(false);
        });

        let win_weak = self.downgrade();
        toggle_lock.connect_toggled(move |btn| {
            if let Some(win) = win_weak.upgrade() {
                let active = btn.is_active();
                win.imp().lock_aspect_ratio.set(active);
                if let Some(settings) = win.imp().settings.borrow().as_ref() {
                    let _ = settings.set_boolean("lock-aspect-ratio", active);
                }
            }
        });

        // タイトル
        let window_title = libadwaita::WindowTitle::new(APP_NAME, "");
        header_bar.set_title_widget(Some(&window_title));

        // メインメニュー
        let menu_btn = gtk4::MenuButton::new();
        menu_btn.set_icon_name("open-menu-symbolic");
        menu_btn.set_tooltip_text(Some("メインメニュー"));

        let menu = gio::Menu::new();
        menu.append(Some("キーボードショートカット"), Some("win.show-help-overlay"));
        menu.append(Some("Crop Picture について"), Some("win.about"));
        menu_btn.set_menu_model(Some(&menu));
        header_bar.pack_end(&menu_btn);

        // 保存ボタン
        let save_btn = gtk4::Button::new();
        let save_content = libadwaita::ButtonContent::builder()
            .icon_name("document-save-symbolic")
            .label("保存")
            .build();
        save_btn.set_child(Some(&save_content));
        save_btn.set_action_name(Some("win.save"));
        save_btn.set_tooltip_text(Some("切り抜いて保存 (Ctrl+S)"));
        save_btn.add_css_class("suggested-action");
        save_btn.set_sensitive(false);
        header_bar.pack_end(&save_btn);

        toolbar_view.add_top_bar(&header_bar);

        // --- Content Stack ---
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::Crossfade);

        // 1. Empty State (AdwStatusPage)
        let status_page = libadwaita::StatusPage::builder()
            .icon_name("image-x-generic-symbolic")
            .title("画像を読み込む")
            .description("切り抜く画像をここにドラッグ＆ドロップするか、ファイルを選択してください")
            .build();
        let empty_open_btn = gtk4::Button::builder()
            .label("画像を開く…")
            .action_name("win.open")
            .css_classes(["pill", "suggested-action"])
            .halign(gtk4::Align::Center)
            .build();
        status_page.set_child(Some(&empty_open_btn));
        stack.add_named(&status_page, Some("empty"));

        // 2. Loading State (GtkSpinner)
        let spinner_box = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        spinner_box.set_valign(gtk4::Align::Center);
        spinner_box.set_halign(gtk4::Align::Center);
        let spinner = gtk4::Spinner::new();
        spinner.set_size_request(48, 48);
        spinner_box.append(&spinner);
        let loading_label = gtk4::Label::new(Some("画像を読み込み中…"));
        loading_label.add_css_class("dim-label");
        spinner_box.append(&loading_label);
        stack.add_named(&spinner_box, Some("loading"));

        // 3. Image State (CropCanvas)
        let crop_canvas = CropCanvas::new();
        stack.add_named(&crop_canvas, Some("image"));

        toolbar_view.set_content(Some(&stack));

        // --- Bottom Information Bar ---
        let bottom_bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 24);
        bottom_bar.set_halign(gtk4::Align::Center);
        bottom_bar.set_margin_top(8);
        bottom_bar.set_margin_bottom(8);
        bottom_bar.set_visible(false);

        let label_dims = gtk4::Label::new(None);
        label_dims.add_css_class("caption");
        label_dims.add_css_class("dim-label");
        label_dims.add_css_class("numeric");

        let label_pos = gtk4::Label::new(None);
        label_pos.add_css_class("caption");
        label_pos.add_css_class("dim-label");
        label_pos.add_css_class("numeric");

        let label_scale = gtk4::Label::new(None);
        label_scale.add_css_class("caption");
        label_scale.add_css_class("dim-label");
        label_scale.add_css_class("numeric");

        bottom_bar.append(&label_dims);
        bottom_bar.append(&label_pos);
        bottom_bar.append(&label_scale);

        toolbar_view.add_bottom_bar(&bottom_bar);

        toast_overlay.set_child(Some(&toolbar_view));
        self.set_content(Some(&toast_overlay));

        // アダプティブ ブレークポイント (幅 500sp 以下)
        let breakpoint = libadwaita::Breakpoint::new(libadwaita::BreakpointCondition::new_length(
            libadwaita::BreakpointConditionLengthType::MaxWidth,
            500.0,
            libadwaita::LengthUnit::Sp,
        ));
        self.add_breakpoint(breakpoint);

        // 参照の保持
        *imp.toast_overlay.borrow_mut() = Some(toast_overlay);
        *imp.toolbar_view.borrow_mut() = Some(toolbar_view);
        *imp.window_title.borrow_mut() = Some(window_title);
        *imp.stack.borrow_mut() = Some(stack);
        *imp.crop_canvas.borrow_mut() = Some(crop_canvas.clone());
        *imp.save_btn.borrow_mut() = Some(save_btn);
        *imp.open_btn.borrow_mut() = Some(open_btn);
        *imp.size_button_content.borrow_mut() = Some(size_content);
        *imp.spin_width.borrow_mut() = Some(spin_w);
        *imp.spin_height.borrow_mut() = Some(spin_h);
        *imp.toggle_lock.borrow_mut() = Some(toggle_lock);
        *imp.spinner.borrow_mut() = Some(spinner);
        *imp.bottom_bar.borrow_mut() = Some(bottom_bar);
        *imp.label_dims.borrow_mut() = Some(label_dims.clone());
        *imp.label_pos.borrow_mut() = Some(label_pos.clone());
        *imp.label_scale.borrow_mut() = Some(label_scale.clone());

        // 座標・倍率・サイズ更新ハンドラ
        let win_clone = self.clone();
        crop_canvas.connect_position_changed(move |x, y, display_scale, image_scale, cw, ch| {
            win_clone.update_status_labels(x, y, display_scale, image_scale, cw, ch);
        });
    }

    fn update_status_labels(
        &self,
        x: u32,
        y: u32,
        display_scale: f64,
        image_scale: f64,
        crop_w: u32,
        crop_h: u32,
    ) {
        let imp = self.imp();
        *imp.current_scale.borrow_mut() = display_scale;

        if let Some(lbl) = imp.label_pos.borrow().as_ref() {
            lbl.set_text(&format!("切り抜き範囲: {} × {} px (位置: X {}, Y {})", crop_w, crop_h, x, y));
        }
        if let Some(lbl) = imp.label_scale.borrow().as_ref() {
            let img_pct = (image_scale * 100.0).round() as u32;
            let disp_pct = (display_scale * 100.0).round() as u32;
            lbl.set_text(&format!("画像倍率: {}% (表示: {}%)", img_pct, disp_pct));
        }
    }

    fn setup_actions(&self) {
        let win = self.clone();

        // win.open アクション
        let open_action = SimpleAction::new("open", None);
        open_action.connect_activate(glib::clone!(
            #[weak]
            win,
            move |_, _| {
                win.show_open_dialog();
            }
        ));
        self.add_action(&open_action);

        // win.save アクション
        let save_action = SimpleAction::new("save", None);
        save_action.connect_activate(glib::clone!(
            #[weak]
            win,
            move |_, _| {
                win.show_save_dialog();
            }
        ));
        self.add_action(&save_action);

        // win.about アクション
        let about_action = SimpleAction::new("about", None);
        about_action.connect_activate(glib::clone!(
            #[weak]
            win,
            move |_, _| {
                win.show_about_dialog();
            }
        ));
        self.add_action(&about_action);

        // win.show-help-overlay アクション
        let help_action = SimpleAction::new("show-help-overlay", None);
        help_action.connect_activate(glib::clone!(
            #[weak]
            win,
            move |_, _| {
                win.show_shortcuts_window();
            }
        ));
        self.add_action(&help_action);
    }

    fn setup_drop_target(&self) {
        let target = gtk4::DropTarget::new(gdk4::FileList::static_type(), gdk4::DragAction::COPY);
        let win = self.clone();
        target.connect_drop(move |_, value, _, _| {
            if let Ok(file_list) = value.get::<gdk4::FileList>() {
                let files = file_list.files();
                if let Some(first_file) = files.first() {
                    if files.len() > 1 {
                        win.show_toast("最初の 1 枚のみ読み込みました", None, None);
                    }
                    if let Some(path) = first_file.path() {
                        win.load_image_async(path);
                        return true;
                    }
                }
            }
            false
        });
        self.add_controller(target);
    }

    fn show_open_dialog(&self) {
        let dialog = gtk4::FileDialog::builder()
            .title("画像を開く")
            .modal(true)
            .build();

        let imp = self.imp();
        let initial_folder = if let Some(dir) = imp.last_open_dir.borrow().as_ref() {
            Some(dir.clone())
        } else {
            glib::user_special_dir(glib::UserDirectory::Pictures)
        };

        if let Some(folder) = initial_folder {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let filter = gtk4::FileFilter::new();
        filter.set_name(Some("対応画像 (*.jpg, *.png, *.webp, *.gif, *.bmp, *.tiff)"));
        filter.add_mime_type("image/jpeg");
        filter.add_mime_type("image/png");
        filter.add_mime_type("image/webp");
        filter.add_mime_type("image/gif");
        filter.add_mime_type("image/bmp");
        filter.add_mime_type("image/tiff");

        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&filter));

        let win = self.clone();
        dialog.open(Some(self), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    win.load_image_async(path);
                }
            }
        });
    }

    fn load_image_async(&self, path: PathBuf) {
        let imp = self.imp();

        // 状態をローディングに切り替え
        if let Some(stack) = imp.stack.borrow().as_ref() {
            stack.set_visible_child_name("loading");
        }
        if let Some(sp) = imp.spinner.borrow().as_ref() {
            sp.start();
        }
        if let Some(btn) = imp.save_btn.borrow().as_ref() {
            btn.set_sensitive(false);
        }

        let win = self.clone();
        let path_clone = path.clone();

        glib::spawn_future_local(async move {
            let res = gio::spawn_blocking(move || LoadedImage::load_from_file(&path_clone)).await;
            if let Ok(res) = res {
                win.on_image_loaded(path, res);
            }
        });
    }

    fn on_image_loaded(&self, path: PathBuf, result: Result<LoadedImage, ImageProcessError>) {
        let imp = self.imp();
        if let Some(sp) = imp.spinner.borrow().as_ref() {
            sp.stop();
        }

        match result {
            Ok(img) => {
                let img_arc = Arc::new(img);
                let w = img_arc.width;
                let h = img_arc.height;

                let cw = imp.crop_width.get();
                let ch = imp.crop_height.get();

                if let Some(canvas) = imp.crop_canvas.borrow().as_ref() {
                    canvas.set_image(&img_arc.preview_rgba, w, h, cw, ch);
                }

                *imp.current_image.borrow_mut() = Some(img_arc);
                *imp.current_file_path.borrow_mut() = Some(path.clone());
                self.update_last_open_dir(&path);

                if let Some(title) = imp.window_title.borrow().as_ref() {
                    let filename = path
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("");
                    title.set_subtitle(filename);
                }

                if let Some(lbl) = imp.label_dims.borrow().as_ref() {
                    lbl.set_text(&format!("元画像: {} × {} px", w, h));
                }

                if let Some(bar) = imp.bottom_bar.borrow().as_ref() {
                    bar.set_visible(true);
                }

                if let Some(btn) = imp.save_btn.borrow().as_ref() {
                    btn.set_sensitive(true);
                }

                if let Some(stack) = imp.stack.borrow().as_ref() {
                    stack.set_visible_child_name("image");
                }
            }
            Err(err) => {
                let had_image = imp.current_image.borrow().is_some();
                if !had_image {
                    if let Some(stack) = imp.stack.borrow().as_ref() {
                        stack.set_visible_child_name("empty");
                    }
                } else if let Some(stack) = imp.stack.borrow().as_ref() {
                    stack.set_visible_child_name("image");
                    if let Some(btn) = imp.save_btn.borrow().as_ref() {
                        btn.set_sensitive(true);
                    }
                }

                self.show_error_dialog(&err);
            }
        }
    }

    fn show_save_dialog(&self) {
        let imp = self.imp();
        let current_path = imp.current_file_path.borrow().clone();
        let (cw, ch) = imp
            .crop_canvas
            .borrow()
            .as_ref()
            .map(|c| c.current_crop_size())
            .unwrap_or((1000, 1000));

        let default_name = if let Some(path) = &current_path {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
            let ext = path
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("png")
                .to_lowercase();
            let out_ext = match ext.as_str() {
                "jpg" | "jpeg" => "jpg",
                "webp" => "webp",
                _ => "png",
            };
            format!("{}_crop{}x{}.{}", stem, cw, ch, out_ext)
        } else {
            format!("crop_{}x{}.png", cw, ch)
        };

        let dialog = gtk4::FileDialog::builder()
            .title("切り抜いた画像を保存")
            .modal(true)
            .initial_name(&default_name)
            .build();

        let initial_folder = if let Some(dir) = imp.last_save_dir.borrow().as_ref() {
            Some(dir.clone())
        } else if let Some(parent) = current_path.as_ref().and_then(|p| p.parent()) {
            Some(parent.to_path_buf())
        } else {
            glib::user_special_dir(glib::UserDirectory::Pictures)
        };

        if let Some(folder) = initial_folder {
            dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
        }

        let win = self.clone();
        dialog.save(Some(self), gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    win.save_crop_async(path);
                }
            }
        });
    }

    fn save_crop_async(&self, save_path: PathBuf) {
        let imp = self.imp();
        let img = match imp.current_image.borrow().clone() {
            Some(i) => i,
            None => return,
        };

        let (pos, image_scale, (cw, ch)) = match imp.crop_canvas.borrow().as_ref() {
            Some(c) => match c.current_position() {
                Some(p) => (p, c.current_image_scale(), c.current_crop_size()),
                None => return,
            },
            None => return,
        };

        if let Some(btn) = imp.save_btn.borrow().as_ref() {
            btn.set_sensitive(false);
        }

        let win = self.clone();
        let path_clone = save_path.clone();

        glib::spawn_future_local(async move {
            let res =
                gio::spawn_blocking(move || img.save_crop(pos.x, pos.y, cw, ch, image_scale, &path_clone))
                    .await;
            if let Ok(res) = res {
                win.on_crop_saved(save_path, res);
            }
        });
    }

    fn on_crop_saved(&self, save_path: PathBuf, result: Result<(), ImageProcessError>) {
        let imp = self.imp();
        if let Some(btn) = imp.save_btn.borrow().as_ref() {
            btn.set_sensitive(true);
        }

        match result {
            Ok(()) => {
                self.update_last_save_dir(&save_path);

                let filename = save_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("画像")
                    .to_string();
                let msg = format!("{} を保存しました", filename);

                let saved_file = gio::File::for_path(&save_path);
                self.show_toast(&msg, Some("フォルダを開く"), Some(saved_file));
            }
            Err(err) => {
                self.show_error_dialog(&err);
            }
        }
    }

    fn show_toast(&self, text: &str, action_name: Option<&str>, target_file: Option<gio::File>) {
        let imp = self.imp();
        if let Some(overlay) = imp.toast_overlay.borrow().as_ref() {
            let toast = libadwaita::Toast::new(text);
            toast.set_timeout(5);

            if let (Some(btn_text), Some(file)) = (action_name, target_file) {
                toast.set_button_label(Some(btn_text));
                let win = self.clone();
                toast.connect_button_clicked(move |_| {
                    let launcher = gtk4::FileLauncher::new(Some(&file));
                    launcher.open_containing_folder(Some(&win), gio::Cancellable::NONE, |_| {});
                });
            }

            overlay.add_toast(toast);
        }
    }

    fn show_error_dialog(&self, err: &ImageProcessError) {
        let (heading, body) = match err {
            ImageProcessError::ResolutionTooSmall { width, height } => (
                "画像の解像度が不足しています",
                format!("幅・高さともに 100 px 以上の画像を選択してください（選択した画像: {} × {} px）", width, height),
            ),
            ImageProcessError::ImageTooLarge { pixels } => (
                "画像が大きすぎます",
                format!("1 億画素以下の画像を選択してください（選択した画像: 約 {:.1} 万画素）", *pixels as f64 / 10_000.0),
            ),
            ImageProcessError::InvalidExtension(_) => (
                "画像を保存できませんでした",
                "拡張子 .png / .jpg / .jpeg / .webp のいずれかを指定してください".to_string(),
            ),
            _ => (
                "処理に失敗しました",
                format!("{}", err),
            ),
        };

        let dialog = libadwaita::AlertDialog::builder()
            .heading(heading)
            .body(&body)
            .build();
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        dialog.present(Some(self));
    }

    fn show_about_dialog(&self) {
        let about = libadwaita::AboutDialog::builder()
            .application_name(APP_NAME)
            .application_icon(APP_ID)
            .developer_name("iontrump")
            .version(APP_VERSION)
            .comments("Web画像用 カスタムサイズ切り抜きネイティブアプリケーション")
            .website("https://github.com/iontrump/crop-picture")
            .issue_url("https://github.com/iontrump/crop-picture/issues")
            .license_type(gtk4::License::MitX11)
            .build();
        about.present(Some(self));
    }

    fn show_shortcuts_window(&self) {
        let shortcuts = gtk4::ShortcutsWindow::builder()
            .modal(true)
            .transient_for(self)
            .build();

        let section = gtk4::ShortcutsSection::builder().build();
        let group_general = gtk4::ShortcutsGroup::builder().title("一般").build();
        let group_edit = gtk4::ShortcutsGroup::builder().title("切り抜き枠操作").build();

        let sc_open = gtk4::ShortcutsShortcut::builder()
            .title("画像を開く")
            .accelerator("<Ctrl>O")
            .build();
        let sc_save = gtk4::ShortcutsShortcut::builder()
            .title("切り抜いて保存")
            .accelerator("<Ctrl>S")
            .build();
        let sc_quit = gtk4::ShortcutsShortcut::builder()
            .title("アプリを終了")
            .accelerator("<Ctrl>Q")
            .build();

        let sc_move = gtk4::ShortcutsShortcut::builder()
            .title("枠を 1 px 移動")
            .accelerator("Up Down Left Right")
            .build();
        let sc_move_fast = gtk4::ShortcutsShortcut::builder()
            .title("枠を 10 px 移動")
            .accelerator("<Shift>Up <Shift>Down <Shift>Left <Shift>Right")
            .build();
        let sc_zoom_in = gtk4::ShortcutsShortcut::builder()
            .title("画像を拡大（切り抜き範囲を狭く）")
            .accelerator("Page_Up")
            .build();
        let sc_zoom_out = gtk4::ShortcutsShortcut::builder()
            .title("画像を縮小（切り抜き範囲を広く）")
            .accelerator("Page_Down")
            .build();

        group_general.append(&sc_open);
        group_general.append(&sc_save);
        group_general.append(&sc_quit);

        group_edit.append(&sc_move);
        group_edit.append(&sc_move_fast);
        group_edit.append(&sc_zoom_in);
        group_edit.append(&sc_zoom_out);

        section.append(&group_general);
        section.append(&group_edit);
        shortcuts.add_section(&section);

        shortcuts.present();
    }
}
