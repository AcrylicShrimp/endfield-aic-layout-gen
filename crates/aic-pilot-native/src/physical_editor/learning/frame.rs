//! Checked occupied frame used to normalize translation-equivalent geometry.

use aic_layout::geometry::Point;

use super::{Result, error};

#[derive(Clone, Copy, Debug)]
pub(super) struct PhysicalFrame {
    pub origin: [i128; 2],
    pub width: i64,
    pub height: i64,
}

impl PhysicalFrame {
    pub fn from_points(points: impl IntoIterator<Item = Point>) -> Result<Self> {
        let mut bounds: Option<([i128; 2], [i128; 2])> = None;
        for point in points {
            let values = [i128::from(point.x), i128::from(point.y)];
            let (low, high) = bounds.get_or_insert((values, values));
            for axis in 0..2 {
                low[axis] = low[axis].min(values[axis]);
                high[axis] = high[axis].max(values[axis]);
            }
        }
        let (origin, high) = bounds.ok_or_else(|| {
            error(
                "learning-frame",
                "frame",
                "physical learning frame is empty",
            )
        })?;
        let [width, height] = std::array::from_fn(|axis| high[axis] - origin[axis] + 1);
        if width <= 0
            || height <= 0
            || width > i128::from(i64::MAX)
            || height > i128::from(i64::MAX)
        {
            return Err(error(
                "learning-frame",
                "frame",
                "physical learning frame exceeds integer representation",
            ));
        }
        Ok(Self {
            origin,
            width: width as i64,
            height: height as i64,
        })
    }

    pub fn scale(self) -> f64 {
        self.width.max(self.height).max(1) as f64
    }
}
