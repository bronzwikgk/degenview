//! The four price-scale modes of a chart and the maths that tells them apart. Pure: no egui, so
//! it is tested without a window..
//! 
//! * **Regular**: the axis shows prices, equal price differences take equal heights.
//! * **Percent**: the axis shows the change from a reference price, `100 * (price - ref) / |ref|`.
//! * **Indexed to 100**: the same, plus 100, so the reference reads `100`.
//! * **Logarithmic**: the axis shows prices, equal ratios take equal heights.
//! 
//! The reference is the close of the first bar the chart shows, so it follows a pan. This is what
//! TradingView's docs say ("the first value on the chart is the base"); that the first bar may be
//! cut by the plot's edge and still count is inferred, and no live TradingView chart was compared.
//! 
//! The four price-scale modes of a chart and the maths that tells them apart. Pure: no egui, so
//! it is tested without a window.
//! 
//! * **Regular**: the axis shows prices, equal price differences take equal heights.
//! * **Percent**: the axis shows the change from a reference price, `100 * (price - ref) / |ref|`.
//! * **Indexed to 100**: the same, plus 100, so the reference reads `100`.
//! * **Logarithmic**: the axis shows prices, equal ratios take equal heights.
//! 
//! The reference is the close of the first bar the chart shows, so it follows a pan. This is what
//! TradingView's docs say ("the first value on the chart is the base"); that the first bar may be
//! cut by the plot's edge and still count is inferred, and no live TradingView chart was compared.
//! 
//! 
//! The four price-scale modes of a chart and the maths that tells them apart. Pure: no egui, so
//! it is tested without a window.
//! 
//! * **Regular**: the axis shows prices, equal price differences take equal heights.
//! * **Percent**: the axis shows the change from a reference price, `100 * (price - ref) / |ref|`.
//! * **Indexed to 100**: the same, plus 100, so the reference reads `100`.
//! * **Logarithmic**: the axis shows prices, equal ratios take equal heights.
//! 
//! The reference is the close of the first bar the chart shows, so it follows a pan. This is what
//! TradingView's docs say ("the first value on the chart is the base"); that the first bar may be
//! cut by the plot's edge and still count is inferred, and no live TradingView chart was compared.
//! 

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A chart's price-scale mode. Exactly one is active, so the combinations that would contradict
/// (percent and logarithmic) cannot be written.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PriceScaleMode {
    #[default]
    Regular,
    Percent,
    IndexedTo100,
    Logarithmic,
}

/// The y-axis side position of the price scale.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YAxisSide {
    /// Y-axis on the left side of the chart
    #[default]
    Left,
    /// Y-axis on the right side of the chart
    Right,
}

/// The x-axis date format for time labels.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum XAxisFormat {
    /// Default format based on timeframe
    #[default] Default,
    /// dd/mm/yyyy format (e.g., 25/12/2023)
    DdMmYyyy,
    /// mm/dd/yyyy format (e.g., 12/25/2023)
    MmDdYyyy,
    /// yyyy-mm-dd format (e.g., 2023-12-25)
    YyyyMmDd,
    /// Custom format string
    Custom(String),
}

/// Per-chart colors, price-axis precision, chart type and canvas. The defaults match SwiftUI's
/// dark system colors.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct ChartAppearance {
    pub bullish: [u8; 3],
    pub bearish: [u8; 3],
    /// None chooses precision from the price magnitude; Some(0..=12) fixes the axis precision.
    pub axis_decimals: Option<u8>,
    /// How a price under 0.001 reads: its zeros counted (`0.0₅278`) or written out.
    pub axis_small_prices: SmallPrices,
    /// How the bars are drawn, and each chart type's own settings; old rows read as candles.
    pub style: ChartStyle,
    /// The chart's background, grid, crosshair, watermark, scales, buttons and margins; old rows
    /// read as the default canvas.
    pub canvas: ChartCanvas,
    /// Position of the y-axis: Left or Right
    pub y_axis_side: YAxisSide,
    /// Format for x-axis date labels
    pub x_axis_format: XAxisFormat,
}