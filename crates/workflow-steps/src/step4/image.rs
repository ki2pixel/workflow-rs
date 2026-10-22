use std::sync::OnceLock;

/// Image RGB 8 bits, canaux entrelacés.
#[derive(Debug, Clone)]
pub struct RgbImage {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl RgbImage {
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Self {
        assert_eq!(data.len(), width * height * 3);
        Self {
            width,
            height,
            data,
        }
    }

    pub fn from_raw(width: usize, height: usize, data: Vec<u8>) -> Self {
        Self::new(width, height, data)
    }

    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        let offset = (y * self.width + x) * 3;
        [
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
        ]
    }

    fn pixel_or_zero(&self, x: i64, y: i64) -> [f32; 3] {
        if x < 0 || y < 0 || x as usize >= self.width || y as usize >= self.height {
            return [0.0; 3];
        }
        let offset = (y as usize * self.width + x as usize) * 3;
        [
            self.data[offset] as f32,
            self.data[offset + 1] as f32,
            self.data[offset + 2] as f32,
        ]
    }
}

fn round_ties_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}

fn saturate_short(value: f32) -> i16 {
    let rounded = value.round_ties_even();
    rounded.clamp(i16::MIN as f32, i16::MAX as f32) as i16
}

/// Redimensionnement linéaire compatible `cv2.resize(..., INTER_LINEAR)` 8u.
///
/// Le chemin OpenCV 8u combine des coefficients fixes 11 bits (HResize) puis
/// des coefficients fixes 11 bits (VResize). Ce code reproduit cette chaîne
/// plutôt qu'un bilinéaire flottant, afin que les entrées modèles restent
/// identiques au pipeline Python.
pub fn resize_linear(src: &RgbImage, dst_width: usize, dst_height: usize) -> RgbImage {
    if src.width == dst_width && src.height == dst_height {
        return src.clone();
    }
    if dst_width == 0 || dst_height == 0 {
        return RgbImage::new(dst_width, dst_height, vec![0; dst_width * dst_height * 3]);
    }
    if src.width == dst_width * 2 && src.height == dst_height * 2 {
        return resize_area_2x(src, dst_width, dst_height);
    }

    // OpenCV calcule scale = 1 / (dst/src), pas src/dst : l'arrondi
    // flottant diffère aux coordonnées entières exactes.
    let scale_x = 1.0 / (dst_width as f64 / src.width as f64);
    let scale_y = 1.0 / (dst_height as f64 / src.height as f64);
    let scale_x_f = scale_x as f32;
    let scale_y_f = scale_y as f32;

    let mut xofs = vec![0usize; dst_width];
    let mut xalpha = vec![[0i16; 2]; dst_width];
    for dx in 0..dst_width {
        let fx = (dx as f32 + 0.5) * scale_x_f - 0.5;
        let mut sx = fx.floor() as i32;
        let mut frac = fx - sx as f32;
        if sx < 0 {
            sx = 0;
            frac = 0.0;
        }
        if sx >= src.width as i32 - 1 {
            sx = src.width as i32 - 1;
            frac = 0.0;
        }
        xofs[dx] = sx as usize;
        let a0 = saturate_short((1.0 - frac) * 2048.0);
        let a1 = saturate_short(frac * 2048.0);
        xalpha[dx] = [a0, a1];
    }

    let mut out = vec![0u8; dst_width * dst_height * 3];
    let mut row0 = vec![0i32; dst_width * 3];
    let mut row1 = vec![0i32; dst_width * 3];

    for dy in 0..dst_height {
        let fy = (dy as f32 + 0.5) * scale_y_f - 0.5;
        let mut sy = fy.floor() as i32;
        let mut frac = fy - sy as f32;
        if sy < 0 {
            sy = 0;
            frac = 0.0;
        }
        if sy >= src.height as i32 - 1 {
            sy = src.height as i32 - 1;
            frac = 0.0;
        }
        let b0 = saturate_short((1.0 - frac) * 2048.0);
        let b1 = saturate_short(frac * 2048.0);

        hresize_row(src, sy as usize, &xofs, &xalpha, &mut row0);
        let second = if sy >= src.height as i32 - 1 {
            sy as usize
        } else {
            (sy + 1) as usize
        };
        hresize_row(src, second, &xofs, &xalpha, &mut row1);

        let out_row = dy * dst_width * 3;
        for index in 0..dst_width * 3 {
            let s0 = row0[index] >> 4;
            let s1 = row1[index] >> 4;
            let value = ((i32::from(b0) * s0) >> 16) + ((i32::from(b1) * s1) >> 16) + 2;
            out[out_row + index] = (value >> 2).clamp(0, 255) as u8;
        }
    }

    RgbImage::new(dst_width, dst_height, out)
}

fn hresize_row(src: &RgbImage, sy: usize, xofs: &[usize], xalpha: &[[i16; 2]], out: &mut [i32]) {
    let row = sy * src.width * 3;
    let dst_width = xofs.len();
    for dx in 0..dst_width {
        let sx = xofs[dx] * 3;
        let [a0, a1] = xalpha[dx];
        let p0 = &src.data[row + sx..row + sx + 3];
        if sx + 3 >= src.width * 3 || a1 == 0 {
            for channel in 0..3 {
                out[dx * 3 + channel] = i32::from(a0) * i32::from(p0[channel]);
            }
        } else {
            let p1 = &src.data[row + sx + 3..row + sx + 6];
            for channel in 0..3 {
                out[dx * 3 + channel] =
                    i32::from(a0) * i32::from(p0[channel]) + i32::from(a1) * i32::from(p1[channel]);
            }
        }
    }
}

fn resize_area_2x(src: &RgbImage, dst_width: usize, dst_height: usize) -> RgbImage {
    let mut out = vec![0u8; dst_width * dst_height * 3];
    for dy in 0..dst_height {
        for dx in 0..dst_width {
            let sx = dx * 2;
            let sy = dy * 2;
            for channel in 0..3 {
                let p00 = src.data[((sy) * src.width + sx) * 3 + channel] as i32;
                let p01 = src.data[((sy) * src.width + sx + 1) * 3 + channel] as i32;
                let p10 = src.data[((sy + 1) * src.width + sx) * 3 + channel] as i32;
                let p11 = src.data[((sy + 1) * src.width + sx + 1) * 3 + channel] as i32;
                out[(dy * dst_width + dx) * 3 + channel] = ((p00 + p01 + p10 + p11 + 2) >> 2) as u8;
            }
        }
    }
    RgbImage::new(dst_width, dst_height, out)
}

/// Redimensionnement au plus proche voisin, sémantique `cv2.resize(INTER_NEAREST)`.
pub fn resize_nearest(src: &RgbImage, dst_width: usize, dst_height: usize) -> RgbImage {
    if src.width == dst_width && src.height == dst_height {
        return src.clone();
    }
    let ifx = 1.0 / (dst_width as f64 / src.width as f64);
    let ify = 1.0 / (dst_height as f64 / src.height as f64);
    let mut out = vec![0u8; dst_width * dst_height * 3];
    for dy in 0..dst_height {
        let sy = ((dy as f64 * ify).floor() as usize).min(src.height - 1);
        for dx in 0..dst_width {
            let sx = ((dx as f64 * ifx).floor() as usize).min(src.width - 1);
            let src_offset = (sy * src.width + sx) * 3;
            let dst_offset = (dy * dst_width + dx) * 3;
            out[dst_offset..dst_offset + 3].copy_from_slice(&src.data[src_offset..src_offset + 3]);
        }
    }
    RgbImage::new(dst_width, dst_height, out)
}

/// Paramètres de `get_affine_transform` du pipeline Python.
#[derive(Debug, Clone, Copy)]
pub struct AffineTransform {
    pub center_x: f64,
    pub center_y: f64,
    pub angle_deg: f64,
    pub distance: f64,
    pub output_size: usize,
}

impl AffineTransform {
    pub fn from_eyes(right_eye: (f32, f32), left_eye: (f32, f32), output_size: usize) -> Self {
        // Les yeux viennent de sorties float32. NumPy effectue la somme et la
        // différence en float32 avant la promotion en float64 par math.* ; on
        // conserve cette chaîne pour que la matrice reste identique.
        let center_x = (right_eye.0 + left_eye.0) as f64 / 2.0;
        let center_y = (right_eye.1 + left_eye.1) as f64 / 2.0;
        let dx = right_eye.0 - left_eye.0;
        let dy = right_eye.1 - left_eye.1;
        let angle_deg = (dy as f64).atan2(dx as f64).to_degrees();
        let distance = (dx as f64).hypot(dy as f64);
        Self {
            center_x,
            center_y,
            angle_deg,
            distance,
            output_size,
        }
    }

    pub fn matrix(&self, width: usize, height: usize) -> [[f64; 3]; 2] {
        // cv2.getRotationMatrix2D reçoit un Point2f : Python convertit le
        // centre en float32 avant le calcul double d'OpenCV.
        let cx_exact = self.center_x * width as f64;
        let cy_exact = self.center_y * height as f64;
        // Le centre passé à cv2.getRotationMatrix2D est un Point2f, mais la
        // translation ajoutée ensuite par le code Python utilise le centre
        // float64 d'origine.
        let cx = cx_exact as f32 as f64;
        let cy = cy_exact as f32 as f64;
        let box_size = (self.distance * 2.5 * width.max(height) as f64).max(1e-5);
        let scale = self.output_size as f64 / box_size;
        let angle = self.angle_deg.to_radians();
        let alpha = scale * angle.cos();
        let beta = scale * angle.sin();
        let half = self.output_size as f64 / 2.0;
        [
            [
                alpha,
                beta,
                (1.0 - alpha) * cx - beta * cy + half - cx_exact * scale,
            ],
            [
                -beta,
                alpha,
                beta * cx + (1.0 - alpha) * cy + half - cy_exact * scale,
            ],
        ]
    }
}

fn invert_affine(m: [[f64; 3]; 2]) -> [[f64; 3]; 2] {
    let determinant = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    if determinant.abs() < f64::EPSILON {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    }
    let inverse = 1.0 / determinant;
    [
        [
            m[1][1] * inverse,
            -m[0][1] * inverse,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inverse,
        ],
        [
            -m[1][0] * inverse,
            m[0][0] * inverse,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inverse,
        ],
    ]
}

/// Table de poids bilinéaire 15 bits d'OpenCV (`BilinearTab_i`).
fn bilinear_table() -> &'static [[i16; 4]; 1024] {
    static TABLE: OnceLock<[[i16; 4]; 1024]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [[0i16; 4]; 1024];
        for fy_index in 0..32 {
            for fx_index in 0..32 {
                let fx = fx_index as f32 / 32.0;
                let fy = fy_index as f32 / 32.0;
                let values = [
                    (1.0 - fy) * (1.0 - fx),
                    (1.0 - fy) * fx,
                    fy * (1.0 - fx),
                    fy * fx,
                ];
                let mut weights = [0i16; 4];
                let mut sum = 0i32;
                for index in 0..4 {
                    weights[index] = saturate_short(values[index] * 32768.0);
                    sum += i32::from(weights[index]);
                }
                if sum != 32768 {
                    // OpenCV (ksize=2) ne corrige que le dernier coefficient
                    // du bloc 2x2, pas le maximum global. Reproduire cette
                    // particularité est nécessaire aux coordonnées entières.
                    let diff = sum - 32768;
                    weights[3] = (i32::from(weights[3]) - diff) as i16;
                }
                table[(fy_index * 32 + fx_index) as usize] = weights;
            }
        }
        table
    })
}

/// `cv2.warpAffine` linéaire, bord constant noir, pour une image 8u RGB.
pub fn warp_affine_linear(src: &RgbImage, matrix: [[f64; 3]; 2], output_size: usize) -> RgbImage {
    let inverse = invert_affine(matrix);
    let table = bilinear_table();
    let mut out = vec![0u8; output_size * output_size * 3];

    for y in 0..output_size {
        for x in 0..output_size {
            let adelta = round_ties_even(inverse[0][0] * x as f64 * 1024.0) as i32;
            let bdelta = round_ties_even(inverse[1][0] * x as f64 * 1024.0) as i32;
            let x0 =
                round_ties_even((inverse[0][1] * y as f64 + inverse[0][2]) * 1024.0) as i32 + 16;
            let y0 =
                round_ties_even((inverse[1][1] * y as f64 + inverse[1][2]) * 1024.0) as i32 + 16;
            let xq = (x0 + adelta) >> 5;
            let yq = (y0 + bdelta) >> 5;
            let sx = xq >> 5;
            let sy = yq >> 5;
            let fx = (xq & 31) as usize;
            let fy = (yq & 31) as usize;
            let weights = table[fy * 32 + fx];

            let p00 = src.pixel_or_zero(sx as i64, sy as i64);
            let p01 = src.pixel_or_zero(sx as i64 + 1, sy as i64);
            let p10 = src.pixel_or_zero(sx as i64, sy as i64 + 1);
            let p11 = src.pixel_or_zero(sx as i64 + 1, sy as i64 + 1);
            let offset = (y * output_size + x) * 3;
            for channel in 0..3 {
                let sum = p00[channel] * f32::from(weights[0])
                    + p01[channel] * f32::from(weights[1])
                    + p10[channel] * f32::from(weights[2])
                    + p11[channel] * f32::from(weights[3]);
                out[offset + channel] = (((sum + 16384.0) as i64 >> 15).clamp(0, 255)) as u8;
            }
        }
    }

    RgbImage::new(output_size, output_size, out)
}

/// Normalisation `(pixel / 127.5) - 1` vers un tenseur float32 little-endian.
pub fn normalize_float_input(image: &RgbImage, output: &mut Vec<u8>) {
    output.clear();
    output.reserve(image.data.len() * 4);
    for value in &image.data {
        let normalized = (*value as f32 / 127.5) - 1.0;
        output.extend_from_slice(&normalized.to_le_bytes());
    }
}

pub fn image_to_uint8_input(image: &RgbImage, output: &mut Vec<u8>) {
    output.clear();
    output.extend_from_slice(&image.data);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_uses_floor_mapping() {
        let image = RgbImage::new(2, 2, vec![0, 0, 0, 10, 10, 10, 20, 20, 20, 30, 30, 30]);
        let resized = resize_nearest(&image, 4, 4);
        assert_eq!(resized.pixel(0, 0), [0, 0, 0]);
        assert_eq!(resized.pixel(2, 0), [10, 10, 10]);
        assert_eq!(resized.pixel(0, 2), [20, 20, 20]);
        assert_eq!(resized.pixel(2, 2), [30, 30, 30]);
    }

    #[test]
    fn exact_2x_is_area_average() {
        let image = RgbImage::new(2, 2, vec![0, 0, 0, 4, 4, 4, 8, 8, 8, 16, 16, 16]);
        let resized = resize_linear(&image, 1, 1);
        assert_eq!(resized.pixel(0, 0), [7, 7, 7]);
    }

    #[test]
    fn affine_identity_is_identity() {
        let image = RgbImage::new(2, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        let matrix = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let warped = warp_affine_linear(&image, matrix, 2);
        assert_eq!(warped.data, image.data);
    }
}
