//! The "dark green" palette shared by every surface.

use super::paint::Col;

pub const PANEL_TOP: Col = Col(60, 64, 46, 252);
pub const PANEL_BOTTOM: Col = Col(44, 48, 34, 252);
pub const PANEL_EDGE: Col = Col(255, 255, 255, 22);
pub const CONTENT_SHADE: Col = Col(0, 0, 0, 34);

pub const TILE: Col = Col(255, 255, 255, 17);
pub const TILE_HOVER: Col = Col(255, 255, 255, 27);
pub const TILE_ACTIVE: Col = Col(14, 18, 9, 120);
pub const TILE_EDGE: Col = Col(255, 255, 255, 14);
pub const ROW: Col = Col(255, 255, 255, 12);

pub const CREAM: Col = Col(255, 242, 201, 255);
pub const TEXT: Col = Col(246, 240, 221, 255);
pub const MUTED: Col = Col(246, 240, 221, 165);
pub const FAINT: Col = Col(246, 240, 221, 105);

pub const GREEN: Col = Col(123, 168, 118, 255);
pub const DEEP_GREEN: Col = Col(34, 46, 27, 255);
pub const TRACK: Col = Col(0, 0, 0, 85);
pub const SLIDER_FILL: Col = Col(233, 221, 186, 255);
pub const WARN: Col = Col(242, 192, 104, 255);
pub const DANGER: Col = Col(232, 128, 104, 255);
