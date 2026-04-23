//! Viewport scaling utilities.
//!
//! GPU hardware imposes a maximum texture dimension. When the physical surface
//! size exceeds this limit (common on high-DPI tablets), the surface must be
//! downscaled. [`fit_surface`] computes effective dimensions and scale factor
//! that keep the logical size correct while respecting GPU limits.

/// Compute a surface size that fits within the GPU's max texture dimension.
///
/// Instead of clamping dimensions (which would misalign the surface with the
/// native view), this function reduces the effective scale factor so the
/// logical size stays correct.
///
/// Returns `(width, height, effective_scale_factor)`.
///
/// # Examples
///
/// ```
/// use iced_wgpu_embed::fit_surface;
///
/// // Within limits — no change
/// let (w, h, s) = fit_surface(1920, 1080, 2.0, 8192);
/// assert_eq!((w, h), (1920, 1080));
///
/// // Exceeds limit — scale reduced to fit
/// let (w, h, s) = fit_surface(12288, 6144, 3.0, 8192);
/// assert!(w <= 8192 && h <= 8192);
/// assert!(s < 3.0);
/// ```
pub fn fit_surface(
    width: u32,
    height: u32,
    scale_factor: f32,
    max_texture: u32,
) -> (u32, u32, f32) {
    if width <= max_texture && height <= max_texture {
        return (width, height, scale_factor);
    }
    let logical_w = width as f32 / scale_factor;
    let logical_h = height as f32 / scale_factor;
    let max_scale_w = max_texture as f32 / logical_w;
    let max_scale_h = max_texture as f32 / logical_h;
    let effective = scale_factor.min(max_scale_w).min(max_scale_h);
    let sw = (logical_w * effective).round() as u32;
    let sh = (logical_h * effective).round() as u32;
    (sw, sh, effective)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_change_when_within_limits() {
        let (w, h, s) = fit_surface(1920, 1080, 2.0, 8192);
        assert_eq!(w, 1920);
        assert_eq!(h, 1080);
        assert!((s - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn exact_limit() {
        let (w, h, s) = fit_surface(8192, 4096, 2.0, 8192);
        assert_eq!(w, 8192);
        assert_eq!(h, 4096);
        assert!((s - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reduces_scale_for_oversized_width() {
        // 4096 logical @ 3x = 12288 physical, exceeds 8192
        let (w, h, s) = fit_surface(12288, 6144, 3.0, 8192);
        assert!(w <= 8192, "width {w} should be <= 8192");
        assert!(h <= 8192, "height {h} should be <= 8192");
        assert!(s < 3.0, "scale {s} should be reduced from 3.0");
        // Logical size preserved: w/s ~= 12288/3 = 4096
        assert!(((w as f32 / s) - 4096.0).abs() < 2.0);
    }

    #[test]
    fn reduces_scale_for_oversized_height() {
        // 3000 logical @ 3x = 9000 physical height, exceeds 8192
        let (w, h, s) = fit_surface(6000, 9000, 3.0, 8192);
        assert!(w <= 8192);
        assert!(h <= 8192);
        assert!(s < 3.0);
    }

    #[test]
    fn both_dimensions_oversized() {
        let (w, h, s) = fit_surface(10000, 10000, 3.0, 8192);
        assert!(w <= 8192);
        assert!(h <= 8192);
        assert!(s < 3.0);
        // Both should be equal since input was square
        assert_eq!(w, h);
    }

    #[test]
    fn scale_factor_1() {
        let (w, h, s) = fit_surface(1000, 800, 1.0, 8192);
        assert_eq!(w, 1000);
        assert_eq!(h, 800);
        assert!((s - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn very_small_max_texture() {
        // Simulate a very constrained GPU
        let (w, h, s) = fit_surface(2000, 1000, 2.0, 512);
        assert!(w <= 512);
        assert!(h <= 512);
        assert!(s < 2.0);
    }
}
