//! クロップ枠の幾何計算・座標・ズーム・クランプ処理を行う純粋ロジックモジュール。
//! 長方形を含む任意の切り抜きサイズ（幅×高さ）に対応する。

pub const DEFAULT_CROP_SIZE: u32 = 1000;
pub const MIN_CROP_DIM: u32 = 100;
pub const MAX_CROP_DIM: u32 = 10000;
pub const PADDING: f64 = 24.0;
pub const MAX_IMAGE_SCALE: f64 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropPosition {
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasMetrics {
    /// 画面描画用の表示縮小率 (Fit-to-Window)
    pub display_scale: f64,
    pub offset_x: f64,
    pub offset_y: f64,
    pub scaled_image_width: u32,
    pub scaled_image_height: u32,
    pub canvas_width: f64,
    pub canvas_height: f64,
}

impl CanvasMetrics {
    /// キャンバスサイズ (W_c, H_c) とスケーリング後の画像サイズ (W_s, H_s) から表示スケールとオフセットを計算
    pub fn compute(
        canvas_width: f64,
        canvas_height: f64,
        scaled_image_width: u32,
        scaled_image_height: u32,
    ) -> Self {
        if scaled_image_width == 0
            || scaled_image_height == 0
            || canvas_width <= 0.0
            || canvas_height <= 0.0
        {
            return Self {
                display_scale: 1.0,
                offset_x: 0.0,
                offset_y: 0.0,
                scaled_image_width,
                scaled_image_height,
                canvas_width,
                canvas_height,
            };
        }

        let avail_w = (canvas_width - 2.0 * PADDING).max(0.0);
        let avail_h = (canvas_height - 2.0 * PADDING).max(0.0);

        let scale_w = avail_w / scaled_image_width as f64;
        let scale_h = avail_h / scaled_image_height as f64;

        // 最大でも等倍 (1.0) に抑える (Fit-to-Window)
        let display_scale = scale_w.min(scale_h).min(1.0);

        let rendered_w = scaled_image_width as f64 * display_scale;
        let rendered_h = scaled_image_height as f64 * display_scale;

        let offset_x = (canvas_width - rendered_w) / 2.0;
        let offset_y = (canvas_height - rendered_h) / 2.0;

        Self {
            display_scale,
            offset_x,
            offset_y,
            scaled_image_width,
            scaled_image_height,
            canvas_width,
            canvas_height,
        }
    }

    /// キャンバス上の論理座標 (cx, cy) がクロップ枠の内部にあるかを判定
    pub fn is_point_inside_crop_box(
        &self,
        cx: f64,
        cy: f64,
        crop_pos: CropPosition,
        crop_width: u32,
        crop_height: u32,
    ) -> bool {
        let box_x = self.offset_x + crop_pos.x as f64 * self.display_scale;
        let box_y = self.offset_y + crop_pos.y as f64 * self.display_scale;
        let box_w = crop_width as f64 * self.display_scale;
        let box_h = crop_height as f64 * self.display_scale;

        cx >= box_x && cx <= box_x + box_w && cy >= box_y && cy <= box_y + box_h
    }

    /// クロップ枠のキャンバス上の描画矩形 (x, y, width, height) を取得
    pub fn crop_rect_on_canvas(
        &self,
        crop_pos: CropPosition,
        crop_width: u32,
        crop_height: u32,
    ) -> (f64, f64, f64, f64) {
        let x = self.offset_x + crop_pos.x as f64 * self.display_scale;
        let y = self.offset_y + crop_pos.y as f64 * self.display_scale;
        let w = crop_width as f64 * self.display_scale;
        let h = crop_height as f64 * self.display_scale;
        (x, y, w, h)
    }

    /// 画像のキャンバス上の描画矩形 (x, y, width, height) を取得
    pub fn image_rect_on_canvas(&self) -> (f64, f64, f64, f64) {
        (
            self.offset_x,
            self.offset_y,
            self.scaled_image_width as f64 * self.display_scale,
            self.scaled_image_height as f64 * self.display_scale,
        )
    }
}

/// クロップ状態の管理構造体
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropState {
    pub original_width: u32,
    pub original_height: u32,
    /// 切り抜き枠の指定幅 (ピクセル)
    pub crop_width: u32,
    /// 切り抜き枠の指定高さ (ピクセル)
    pub crop_height: u32,
    /// 画像自体の拡大縮小倍率 (初期値: 1.0)
    pub image_scale: f64,
    /// スケーリングされた画像空間におけるクロップ枠の左上座標
    pub position: CropPosition,
}

impl CropState {
    /// 新規画像が読み込まれた際の初期状態を生成（画像中央に指定サイズの枠を配置）
    pub fn new_centered(
        image_width: u32,
        image_height: u32,
        crop_width: u32,
        crop_height: u32,
    ) -> Result<Self, &'static str> {
        let cw = crop_width.clamp(MIN_CROP_DIM, MAX_CROP_DIM);
        let ch = crop_height.clamp(MIN_CROP_DIM, MAX_CROP_DIM);

        if image_width < MIN_CROP_DIM || image_height < MIN_CROP_DIM {
            return Err("画像の解像度が不足しています (100px 未満)");
        }

        // 初期画像倍率 (画像が枠より小さい場合は自動で拡大して枠を満たす)
        let min_w = cw as f64 / image_width as f64;
        let min_h = ch as f64 / image_height as f64;
        let init_scale = min_w.max(min_h).max(1.0);

        let sw = ((image_width as f64 * init_scale).round() as u32).max(cw);
        let sh = ((image_height as f64 * init_scale).round() as u32).max(ch);

        let x = (sw - cw) / 2;
        let y = (sh - ch) / 2;

        Ok(Self {
            original_width: image_width,
            original_height: image_height,
            crop_width: cw,
            crop_height: ch,
            image_scale: init_scale,
            position: CropPosition { x, y },
        })
    }

    /// 切り抜き枠が画像外にはみ出さないための最小画像倍率
    pub fn min_scale(&self) -> f64 {
        let min_w = self.crop_width as f64 / self.original_width as f64;
        let min_h = self.crop_height as f64 / self.original_height as f64;
        min_w.max(min_h)
    }

    /// 最大画像倍率
    pub fn max_scale(&self) -> f64 {
        MAX_IMAGE_SCALE.max(self.min_scale())
    }

    /// スケーリング後の画像幅
    pub fn scaled_width(&self) -> u32 {
        ((self.original_width as f64 * self.image_scale).round() as u32).max(self.crop_width)
    }

    /// スケーリング後の画像高さ
    pub fn scaled_height(&self) -> u32 {
        ((self.original_height as f64 * self.image_scale).round() as u32).max(self.crop_height)
    }

    /// クロップサイズ（幅・高さ）を更新し、枠の中心を維持してクランプ
    pub fn set_crop_size(&mut self, new_width: u32, new_height: u32) {
        let nw = new_width.clamp(MIN_CROP_DIM, MAX_CROP_DIM);
        let nh = new_height.clamp(MIN_CROP_DIM, MAX_CROP_DIM);

        // 現在の中心座標 (スケール後空間)
        let cx = self.position.x as f64 + (self.crop_width as f64 / 2.0);
        let cy = self.position.y as f64 + (self.crop_height as f64 / 2.0);

        self.crop_width = nw;
        self.crop_height = nh;

        // 必要に応じて倍率を引き上げ（画像が枠を下回らないようにする）
        let min_s = self.min_scale();
        if self.image_scale < min_s {
            self.image_scale = min_s;
        }

        let sw = self.scaled_width();
        let sh = self.scaled_height();

        let max_x = (sw - nw) as f64;
        let max_y = (sh - nh) as f64;

        let new_x = (cx - (nw as f64 / 2.0)).round().clamp(0.0, max_x) as u32;
        let new_y = (cy - (nh as f64 / 2.0)).round().clamp(0.0, max_y) as u32;

        self.position = CropPosition { x: new_x, y: new_y };
    }

    /// 画像スケールを変更（枠の中心被写体を基準に拡大・縮小）
    pub fn zoom_by(&mut self, factor: f64) {
        if factor <= 0.0 {
            return;
        }

        let old_scale = self.image_scale;
        let new_scale = (old_scale * factor).clamp(self.min_scale(), self.max_scale());
        if (new_scale - old_scale).abs() < 1e-6 {
            return;
        }

        // 現在の枠中心座標 (スケール後空間)
        let cx_old = self.position.x as f64 + (self.crop_width as f64 / 2.0);
        let cy_old = self.position.y as f64 + (self.crop_height as f64 / 2.0);

        // 元画像空間における中心座標
        let orig_cx = cx_old / old_scale;
        let orig_cy = cy_old / old_scale;

        self.image_scale = new_scale;
        let sw = self.scaled_width();
        let sh = self.scaled_height();

        // 新しいスケール後空間での中心座標
        let cx_new = orig_cx * new_scale;
        let cy_new = orig_cy * new_scale;

        let max_x = (sw - self.crop_width) as f64;
        let max_y = (sh - self.crop_height) as f64;

        let new_x = (cx_new - (self.crop_width as f64 / 2.0)).round().clamp(0.0, max_x) as u32;
        let new_y = (cy_new - (self.crop_height as f64 / 2.0)).round().clamp(0.0, max_y) as u32;

        self.position = CropPosition { x: new_x, y: new_y };
    }

    /// ドラッグ開始点からの移動量 (delta_x, delta_y) に基づき、新しいクランプされた位置を計算
    pub fn calculate_drag_position(
        &self,
        start_pos: CropPosition,
        delta_x: f64,
        delta_y: f64,
        display_scale: f64,
    ) -> CropPosition {
        if display_scale <= 0.0 {
            return start_pos;
        }

        let sw = self.scaled_width();
        let sh = self.scaled_height();

        let max_x = (sw - self.crop_width) as f64;
        let max_y = (sh - self.crop_height) as f64;

        let raw_x = start_pos.x as f64 + (delta_x / display_scale);
        let raw_y = start_pos.y as f64 + (delta_y / display_scale);

        let clamped_x = raw_x.round().clamp(0.0, max_x) as u32;
        let clamped_y = raw_y.round().clamp(0.0, max_y) as u32;

        CropPosition {
            x: clamped_x,
            y: clamped_y,
        }
    }

    /// キーボード等での差分移動 (dx, dy) を適用し、クランプ
    pub fn nudge(&mut self, dx: i32, dy: i32) {
        let max_x = (self.scaled_width() - self.crop_width) as i64;
        let max_y = (self.scaled_height() - self.crop_height) as i64;

        let cur_x = self.position.x as i64 + dx as i64;
        let cur_y = self.position.y as i64 + dy as i64;

        self.position.x = cur_x.clamp(0, max_x) as u32;
        self.position.y = cur_y.clamp(0, max_y) as u32;
    }
}
