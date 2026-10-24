use crop_picture::crop_state::{CanvasMetrics, CropPosition, CropState};

#[test]
fn test_crop_state_initial_centered() {
    // 3840 x 2160, crop 1000x1000 の場合: (3840 - 1000) / 2 = 1420, (2160 - 1000) / 2 = 580
    let state = CropState::new_centered(3840, 2160, 1000, 1000).unwrap();
    assert_eq!(state.position, CropPosition { x: 1420, y: 580 });
    assert_eq!(state.original_width, 3840);
    assert_eq!(state.original_height, 2160);
    assert_eq!(state.crop_width, 1000);
    assert_eq!(state.crop_height, 1000);
    assert_eq!(state.image_scale, 1.0);

    // ちょうど 1000 x 1000 の場合: x=0, y=0
    let state_exact = CropState::new_centered(1000, 1000, 1000, 1000).unwrap();
    assert_eq!(state_exact.position, CropPosition { x: 0, y: 0 });
}

#[test]
fn test_crop_state_rectangular() {
    // 1920 x 1080 画像で 1200 x 630 (OGP比率長方形)
    // (1920 - 1200) / 2 = 360, (1080 - 630) / 2 = 225
    let state = CropState::new_centered(1920, 1080, 1200, 630).unwrap();
    assert_eq!(state.position, CropPosition { x: 360, y: 225 });
    assert_eq!(state.crop_width, 1200);
    assert_eq!(state.crop_height, 630);
}

#[test]
fn test_crop_state_set_crop_size() {
    let mut state = CropState::new_centered(2000, 2000, 1000, 1000).unwrap();
    assert_eq!(state.position, CropPosition { x: 500, y: 500 });

    // 1200 x 800 に変更
    state.set_crop_size(1200, 800);
    assert_eq!(state.crop_width, 1200);
    assert_eq!(state.crop_height, 800);
    // 元画像サイズ 2000x2000 に対する中心: (2000 - 1200)/2 = 400, (2000 - 800)/2 = 600
    assert_eq!(state.position, CropPosition { x: 400, y: 600 });
}

#[test]
fn test_crop_state_rejects_small_image() {
    // 画像サイズが最小許容解像度 (100px) 未満の場合は Err
    assert!(CropState::new_centered(99, 100, 1000, 1000).is_err());
    assert!(CropState::new_centered(100, 99, 1000, 1000).is_err());
    assert!(CropState::new_centered(50, 50, 1000, 1000).is_err());
}

#[test]
fn test_zoom_by_scaling_and_limits() {
    // 2000 x 2000 の場合、min_scale = 1000 / 2000 = 0.5
    let mut state = CropState::new_centered(2000, 2000, 1000, 1000).unwrap();
    assert_eq!(state.min_scale(), 0.5);

    // 拡大テスト (x1.5 -> scale 1.5)
    state.zoom_by(1.5);
    assert!((state.image_scale - 1.5).abs() < 1e-4);
    assert_eq!(state.scaled_width(), 3000);
    assert_eq!(state.scaled_height(), 3000);

    // 縮小テスト (極端に縮小しても min_scale = 0.5 でクランプ)
    state.zoom_by(0.1);
    assert!((state.image_scale - 0.5).abs() < 1e-4);
    assert_eq!(state.scaled_width(), 1000);
    assert_eq!(state.scaled_height(), 1000);
    // 1000x1000 画像空間では位置は必ず (0, 0)
    assert_eq!(state.position, CropPosition { x: 0, y: 0 });
}

#[test]
fn test_drag_position_clamping() {
    let state = CropState::new_centered(2000, 2000, 1000, 1000).unwrap();
    let start_pos = state.position; // x=500, y=500
    let display_scale = 0.5;

    // 右下にドラッグ (+100px 論理移動 -> 実画像 +200px)
    let pos1 = state.calculate_drag_position(start_pos, 100.0, 100.0, display_scale);
    assert_eq!(pos1, CropPosition { x: 700, y: 700 });

    // 大きく左上にドラッグして境界(0)でクランプ
    let pos_min = state.calculate_drag_position(start_pos, -500.0, -500.0, display_scale);
    assert_eq!(pos_min, CropPosition { x: 0, y: 0 });

    // 大きく右下にドラッグして最大値 (2000 - 1000 = 1000) でクランプ
    let pos_max = state.calculate_drag_position(start_pos, 500.0, 500.0, display_scale);
    assert_eq!(pos_max, CropPosition { x: 1000, y: 1000 });
}

#[test]
fn test_keyboard_nudge_and_clamping() {
    let mut state = CropState::new_centered(1200, 1200, 1000, 1000).unwrap();
    // 初期位置: x=100, y=100 (max=200)

    state.nudge(1, 0);
    assert_eq!(state.position, CropPosition { x: 101, y: 100 });

    state.nudge(-10, -20);
    assert_eq!(state.position, CropPosition { x: 91, y: 80 });

    // クランプ下限
    state.nudge(-200, -200);
    assert_eq!(state.position, CropPosition { x: 0, y: 0 });

    // クランプ上限
    state.nudge(500, 500);
    assert_eq!(state.position, CropPosition { x: 200, y: 200 });
}

#[test]
fn test_canvas_metrics_and_hit_test() {
    let canvas_w = 800.0;
    let canvas_h = 600.0;
    let scaled_w = 2000;
    let scaled_h = 1000;

    let metrics = CanvasMetrics::compute(canvas_w, canvas_h, scaled_w, scaled_h);
    assert!(metrics.display_scale > 0.0);
    assert!(metrics.display_scale <= 1.0);

    let crop_pos = CropPosition { x: 500, y: 0 };
    let crop_w = 1000;
    let crop_h = 800;
    let (box_x, box_y, _box_w, box_h) = metrics.crop_rect_on_canvas(crop_pos, crop_w, crop_h);

    // ボックス内のヒットテスト
    assert!(metrics.is_point_inside_crop_box(box_x + 10.0, box_y + 10.0, crop_pos, crop_w, crop_h));

    // ボックス外のヒットテスト
    assert!(!metrics.is_point_inside_crop_box(box_x - 10.0, box_y, crop_pos, crop_w, crop_h));
    assert!(!metrics.is_point_inside_crop_box(box_x, box_y + box_h + 10.0, crop_pos, crop_w, crop_h));
}
