use crate::crop_state::{CanvasMetrics, CropPosition, CropState};
use gdk4::glib;
use gdk4::MemoryTexture;
use gtk4::graphene::Rect;
use gtk4::gsk::ScalingFilter;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use image::RgbaImage;
use std::cell::RefCell;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct CropCanvas {
        pub(super) texture: RefCell<Option<MemoryTexture>>,
        pub(super) crop_state: RefCell<Option<CropState>>,
        pub(super) drag_start_pos: RefCell<Option<CropPosition>>,
        pub(super) is_dragging: RefCell<bool>,
        pub(super) on_position_changed: RefCell<Option<Box<dyn Fn(u32, u32, f64, f64, u32, u32)>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CropCanvas {
        const NAME: &'static str = "CropCanvas";
        type Type = super::CropCanvas;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for CropCanvas {
        fn constructed(&self) {
            self.parent_constructed();
            let widget = self.obj();
            widget.set_focusable(true);
            widget.set_can_focus(true);
            widget.set_hexpand(true);
            widget.set_vexpand(true);
            widget.set_accessible_role(gtk4::AccessibleRole::Img);
            widget.update_property(&[gtk4::accessible::Property::Label("切り抜き範囲")]);

            // ドラッグジェスチャー
            let gesture = gtk4::GestureDrag::new();
            let widget_clone = widget.clone();
            gesture.connect_drag_begin(glib::clone!(
                #[weak]
                widget_clone,
                move |gesture, x, y| {
                    widget_clone.on_drag_begin(gesture, x, y);
                }
            ));

            let widget_clone = widget.clone();
            gesture.connect_drag_update(glib::clone!(
                #[weak]
                widget_clone,
                move |_, offset_x, offset_y| {
                    widget_clone.on_drag_update(offset_x, offset_y);
                }
            ));

            let widget_clone = widget.clone();
            gesture.connect_drag_end(glib::clone!(
                #[weak]
                widget_clone,
                move |_, _, _| {
                    widget_clone.on_drag_end();
                }
            ));
            widget.add_controller(gesture);

            // マウススクロール（ホイールによる拡大縮小）
            let scroll = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
            let widget_clone = widget.clone();
            scroll.connect_scroll(glib::clone!(
                #[weak]
                widget_clone,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, _, dy| {
                    widget_clone.on_scroll(dy)
                }
            ));
            widget.add_controller(scroll);

            // マウス移動・カーソル制御
            let motion = gtk4::EventControllerMotion::new();
            let widget_clone = widget.clone();
            motion.connect_motion(glib::clone!(
                #[weak]
                widget_clone,
                move |_, x, y| {
                    widget_clone.on_mouse_motion(x, y);
                }
            ));
            widget.add_controller(motion);

            // キーボード入力
            let key = gtk4::EventControllerKey::new();
            let widget_clone = widget.clone();
            key.connect_key_pressed(glib::clone!(
                #[weak]
                widget_clone,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, keyval, _, state| {
                    widget_clone.on_key_pressed(keyval, state)
                }
            ));
            widget.add_controller(key);
        }
    }

    impl WidgetImpl for CropCanvas {
        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let width = widget.width() as f64;
            let height = widget.height() as f64;

            let texture_guard = self.texture.borrow();
            let state_guard = self.crop_state.borrow();

            let (texture, state) = match (texture_guard.as_ref(), state_guard.as_ref()) {
                (Some(t), Some(s)) => (t, s),
                _ => return,
            };

            let sw = state.scaled_width();
            let sh = state.scaled_height();
            let metrics = CanvasMetrics::compute(width, height, sw, sh);

            // 1. 画像プレビュー描画 (GSK append_scaled_texture)
            let img_bounds = Rect::new(
                metrics.offset_x as f32,
                metrics.offset_y as f32,
                (sw as f64 * metrics.display_scale) as f32,
                (sh as f64 * metrics.display_scale) as f32,
            );
            snapshot.append_scaled_texture(texture, ScalingFilter::Trilinear, &img_bounds);

            // 2. ディミング（半透明黒オーバーレイ）
            let (crop_x, crop_y, crop_w, crop_h) =
                metrics.crop_rect_on_canvas(state.position, state.crop_width, state.crop_height);
            let dim_color = gdk4::RGBA::new(0.0, 0.0, 0.0, 0.55);

            let (ix, iy, iw, ih) = metrics.image_rect_on_canvas();

            // 上側の矩形
            if crop_y > iy {
                snapshot.append_color(
                    &dim_color,
                    &Rect::new(ix as f32, iy as f32, iw as f32, (crop_y - iy) as f32),
                );
            }
            // 下側の矩形
            if (crop_y + crop_h) < (iy + ih) {
                snapshot.append_color(
                    &dim_color,
                    &Rect::new(
                        ix as f32,
                        (crop_y + crop_h) as f32,
                        iw as f32,
                        (iy + ih - (crop_y + crop_h)) as f32,
                    ),
                );
            }
            // 左側の矩形
            if crop_x > ix {
                snapshot.append_color(
                    &dim_color,
                    &Rect::new(
                        ix as f32,
                        crop_y as f32,
                        (crop_x - ix) as f32,
                        crop_h as f32,
                    ),
                );
            }
            // 右側の矩形
            if (crop_x + crop_w) < (ix + iw) {
                snapshot.append_color(
                    &dim_color,
                    &Rect::new(
                        (crop_x + crop_w) as f32,
                        crop_y as f32,
                        (ix + iw - (crop_x + crop_w)) as f32,
                        crop_h as f32,
                    ),
                );
            }

            // 3. クロップ枠線 (白2px + 黒1px の二重線、フォーカス時はアクセントカラー)
            let is_focused = widget.has_focus();
            let border_rect = Rect::new(
                crop_x as f32,
                crop_y as f32,
                crop_w as f32,
                crop_h as f32,
            );

            if is_focused {
                let accent_color = gdk4::RGBA::new(0.2, 0.5, 0.9, 1.0);
                let rounded = gtk4::gsk::RoundedRect::from_rect(border_rect, 0.0);
                snapshot.append_border(
                    &rounded,
                    &[2.0, 2.0, 2.0, 2.0],
                    &[accent_color, accent_color, accent_color, accent_color],
                );
            } else {
                // 外側白 2px
                let white = gdk4::RGBA::WHITE;
                let rounded_white = gtk4::gsk::RoundedRect::from_rect(border_rect, 0.0);
                snapshot.append_border(
                    &rounded_white,
                    &[2.0, 2.0, 2.0, 2.0],
                    &[white, white, white, white],
                );

                // 内側黒 1px
                let black = gdk4::RGBA::BLACK;
                let inner_rect = Rect::new(
                    (crop_x + 2.0) as f32,
                    (crop_y + 2.0) as f32,
                    (crop_w - 4.0).max(0.0) as f32,
                    (crop_h - 4.0).max(0.0) as f32,
                );
                let rounded_black = gtk4::gsk::RoundedRect::from_rect(inner_rect, 0.0);
                snapshot.append_border(
                    &rounded_black,
                    &[1.0, 1.0, 1.0, 1.0],
                    &[black, black, black, black],
                );
            }

            // 4. ガイドマーカー（四隅のL字マーカーおよび中央の十字）
            let marker_color = gdk4::RGBA::WHITE;
            let corner_len = 16.0f32;
            let corner_thick = 3.0f32;

            // 左上 L字
            snapshot.append_color(&marker_color, &Rect::new(crop_x as f32, crop_y as f32, corner_len, corner_thick));
            snapshot.append_color(&marker_color, &Rect::new(crop_x as f32, crop_y as f32, corner_thick, corner_len));

            // 右上 L字
            snapshot.append_color(&marker_color, &Rect::new((crop_x + crop_w) as f32 - corner_len, crop_y as f32, corner_len, corner_thick));
            snapshot.append_color(&marker_color, &Rect::new((crop_x + crop_w) as f32 - corner_thick, crop_y as f32, corner_thick, corner_len));

            // 左下 L字
            snapshot.append_color(&marker_color, &Rect::new(crop_x as f32, (crop_y + crop_h) as f32 - corner_thick, corner_len, corner_thick));
            snapshot.append_color(&marker_color, &Rect::new(crop_x as f32, (crop_y + crop_h) as f32 - corner_len, corner_thick, corner_len));

            // 右下 L字
            snapshot.append_color(&marker_color, &Rect::new((crop_x + crop_w) as f32 - corner_len, (crop_y + crop_h) as f32 - corner_thick, corner_len, corner_thick));
            snapshot.append_color(&marker_color, &Rect::new((crop_x + crop_w) as f32 - corner_thick, (crop_y + crop_h) as f32 - corner_len, corner_thick, corner_len));

            // 中央十字 (長さ12px, 太さ1px)
            let center_x = (crop_x + crop_w / 2.0) as f32;
            let center_y = (crop_y + crop_h / 2.0) as f32;
            snapshot.append_color(&marker_color, &Rect::new(center_x - 6.0, center_y - 0.5, 12.0, 1.0));
            snapshot.append_color(&marker_color, &Rect::new(center_x - 0.5, center_y - 6.0, 1.0, 12.0));
        }
    }
}

glib::wrapper! {
    pub struct CropCanvas(ObjectSubclass<imp::CropCanvas>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for CropCanvas {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl CropCanvas {
    pub fn new() -> Self {
        Self::default()
    }

    /// 画像プレビューとクロップ状態を設定
    pub fn set_image(
        &self,
        preview_rgba: &RgbaImage,
        width: u32,
        height: u32,
        crop_width: u32,
        crop_height: u32,
    ) {
        let imp = self.imp();
        let bytes = glib::Bytes::from(preview_rgba.as_raw());
        let texture = MemoryTexture::new(
            preview_rgba.width() as i32,
            preview_rgba.height() as i32,
            gdk4::MemoryFormat::R8g8b8a8,
            &bytes,
            (preview_rgba.width() * 4) as usize,
        );

        let crop_state = match CropState::new_centered(width, height, crop_width, crop_height) {
            Ok(s) => s,
            Err(_) => return,
        };

        *imp.texture.borrow_mut() = Some(texture);
        *imp.crop_state.borrow_mut() = Some(crop_state);

        self.notify_position_changed();
        self.queue_draw();
        self.grab_focus();
    }

    /// 切り抜きサイズ (幅, 高さ) を変更
    pub fn set_crop_size(&self, crop_width: u32, crop_height: u32) {
        let imp = self.imp();
        let mut state_guard = imp.crop_state.borrow_mut();
        if let Some(state) = state_guard.as_mut() {
            state.set_crop_size(crop_width, crop_height);
            drop(state_guard);
            self.notify_position_changed();
            self.queue_draw();
        }
    }

    /// 現在の切り抜きサイズ (幅, 高さ) を取得
    pub fn current_crop_size(&self) -> (u32, u32) {
        self.imp()
            .crop_state
            .borrow()
            .as_ref()
            .map(|s| (s.crop_width, s.crop_height))
            .unwrap_or((1000, 1000))
    }

    /// 画像をクリア
    pub fn clear(&self) {
        let imp = self.imp();
        *imp.texture.borrow_mut() = None;
        *imp.crop_state.borrow_mut() = None;
        self.queue_draw();
    }

    /// 現在のクロップ位置 (X, Y) を取得
    pub fn current_position(&self) -> Option<CropPosition> {
        self.imp().crop_state.borrow().map(|s| s.position)
    }

    /// 現在の画像倍率を取得
    pub fn current_image_scale(&self) -> f64 {
        self.imp().crop_state.borrow().map(|s| s.image_scale).unwrap_or(1.0)
    }

    /// 画像の拡大縮小 (ズーム)
    pub fn zoom_by(&self, factor: f64) {
        let imp = self.imp();
        let mut state_guard = imp.crop_state.borrow_mut();
        if let Some(state) = state_guard.as_mut() {
            state.zoom_by(factor);
            drop(state_guard);
            self.notify_position_changed();
            self.queue_draw();
        }
    }

    /// 座標・倍率・サイズ更新通知コールバックの登録
    pub fn connect_position_changed<F: Fn(u32, u32, f64, f64, u32, u32) + 'static>(
        &self,
        callback: F,
    ) {
        *self.imp().on_position_changed.borrow_mut() = Some(Box::new(callback));
    }

    fn notify_position_changed(&self) {
        let imp = self.imp();
        let state = match imp.crop_state.borrow().as_ref() {
            Some(s) => *s,
            None => return,
        };

        let metrics = CanvasMetrics::compute(
            self.width() as f64,
            self.height() as f64,
            state.scaled_width(),
            state.scaled_height(),
        );

        // アクセシブル説明の更新
        self.update_property(&[gtk4::accessible::Property::Description(&format!(
            "X {}, Y {}, {} × {} ピクセル、スケール {:.0}%",
            state.position.x, state.position.y, state.crop_width, state.crop_height, state.image_scale * 100.0
        ))]);

        if let Some(cb) = imp.on_position_changed.borrow().as_ref() {
            cb(
                state.position.x,
                state.position.y,
                metrics.display_scale,
                state.image_scale,
                state.crop_width,
                state.crop_height,
            );
        }
    }

    fn on_drag_begin(&self, gesture: &gtk4::GestureDrag, x: f64, y: f64) {
        let imp = self.imp();
        let state = match imp.crop_state.borrow().as_ref() {
            Some(s) => *s,
            None => return,
        };

        let metrics = CanvasMetrics::compute(
            self.width() as f64,
            self.height() as f64,
            state.scaled_width(),
            state.scaled_height(),
        );

        if metrics.is_point_inside_crop_box(x, y, state.position, state.crop_width, state.crop_height) {
            *imp.drag_start_pos.borrow_mut() = Some(state.position);
            *imp.is_dragging.borrow_mut() = true;
            self.set_cursor_from_name(Some("grabbing"));
            self.grab_focus();
        } else {
            // 枠外のドラッグは無効化
            gesture.set_state(gtk4::EventSequenceState::Denied);
        }
    }

    fn on_drag_update(&self, offset_x: f64, offset_y: f64) {
        let imp = self.imp();
        if !*imp.is_dragging.borrow() {
            return;
        }

        let start_pos = match *imp.drag_start_pos.borrow() {
            Some(p) => p,
            None => return,
        };

        let mut state_guard = imp.crop_state.borrow_mut();
        let state = match state_guard.as_mut() {
            Some(s) => s,
            None => return,
        };

        let metrics = CanvasMetrics::compute(
            self.width() as f64,
            self.height() as f64,
            state.scaled_width(),
            state.scaled_height(),
        );

        let new_pos = state.calculate_drag_position(start_pos, offset_x, offset_y, metrics.display_scale);
        if new_pos != state.position {
            state.position = new_pos;
            drop(state_guard);
            self.notify_position_changed();
            self.queue_draw();
        }
    }

    fn on_drag_end(&self) {
        let imp = self.imp();
        *imp.is_dragging.borrow_mut() = false;
        *imp.drag_start_pos.borrow_mut() = None;
        self.set_cursor_from_name(Some("grab"));
    }

    fn on_scroll(&self, dy: f64) -> glib::Propagation {
        // dy < 0: 上スクロール（拡大・範囲狭く）、dy > 0: 下スクロール（縮小・範囲広く）
        if dy < 0.0 {
            self.zoom_by(1.08);
        } else if dy > 0.0 {
            self.zoom_by(1.0 / 1.08);
        }
        glib::Propagation::Stop
    }

    fn on_mouse_motion(&self, x: f64, y: f64) {
        let imp = self.imp();
        if *imp.is_dragging.borrow() {
            return;
        }

        let state = match imp.crop_state.borrow().as_ref() {
            Some(s) => *s,
            None => {
                self.set_cursor_from_name(None);
                return;
            }
        };

        let metrics = CanvasMetrics::compute(
            self.width() as f64,
            self.height() as f64,
            state.scaled_width(),
            state.scaled_height(),
        );

        if metrics.is_point_inside_crop_box(x, y, state.position, state.crop_width, state.crop_height) {
            self.set_cursor_from_name(Some("grab"));
        } else {
            self.set_cursor_from_name(None);
        }
    }

    fn on_key_pressed(&self, keyval: gdk4::Key, state: gdk4::ModifierType) -> glib::Propagation {
        // PageUp: 拡大 (枠内範囲が狭くなる)
        if keyval == gdk4::Key::Page_Up {
            self.zoom_by(1.10);
            return glib::Propagation::Stop;
        }

        // PageDown: 縮小 (枠内範囲が広くなる)
        if keyval == gdk4::Key::Page_Down {
            self.zoom_by(1.0 / 1.10);
            return glib::Propagation::Stop;
        }

        let imp = self.imp();
        let mut state_guard = imp.crop_state.borrow_mut();
        let crop_state = match state_guard.as_mut() {
            Some(s) => s,
            None => return glib::Propagation::Proceed,
        };

        let step = if state.contains(gdk4::ModifierType::SHIFT_MASK) {
            10
        } else {
            1
        };

        let (dx, dy) = match keyval {
            gdk4::Key::Up => (0, -step),
            gdk4::Key::Down => (0, step),
            gdk4::Key::Left => (-step, 0),
            gdk4::Key::Right => (step, 0),
            _ => return glib::Propagation::Proceed,
        };

        crop_state.nudge(dx, dy);
        drop(state_guard);

        self.notify_position_changed();
        self.queue_draw();

        glib::Propagation::Stop
    }
}
