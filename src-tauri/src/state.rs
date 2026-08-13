//! Trạng thái chạy, dùng chung giữa phím tắt, khay hệ thống và hai cửa sổ.

use std::sync::Mutex;

use writa_win::context::WindowId;

use crate::config::Settings;
use crate::flow::Anchor;
use crate::model::ReviewPayload;

/// Một lượt xem lại đang mở.
///
/// Chỉ tồn tại một lượt tại một thời điểm: popup là cửa sổ duy nhất, và bấm phím
/// tắt lần nữa nghĩa là user muốn xem đoạn mới chứ không phải mở thêm cửa sổ.
pub struct ActiveReview {
    pub payload: ReviewPayload,
    /// Cửa sổ sẽ nhận text đã sửa. `None` ở chế độ clipboard.
    pub target: Option<WindowId>,
    pub anchor: Anchor,
}

#[derive(Default)]
pub struct AppState {
    pub settings: Mutex<Settings>,
    pub review: Mutex<Option<ActiveReview>>,
}
