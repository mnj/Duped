use image::GenericImageView;
use std::path::Path;

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif"];

pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn compute_phash(path: &Path) -> Result<i64, String> {
    let img = image::open(path).map_err(|e| format!("Failed to open image: {}", e))?;

    let small = img.resize_exact(32, 32, image::imageops::FilterType::Lanczos3);
    let gray = small.grayscale();

    let mut pixels = [[0.0f64; 32]; 32];
    for (x, y, pixel) in gray.pixels() {
        let luma = pixel[0] as f64;
        pixels[x as usize][y as usize] = luma;
    }

    let dct = dct2d_32x32(&pixels);

    let mut top_left = [0.0f64; 64];
    let mut idx = 0;
    for y in 0..8 {
        for x in 0..8 {
            if x == 0 && y == 0 {
                top_left[idx] = dct[x][y + 1];
            } else if y == 0 {
                top_left[idx] = dct[x + 1][y];
            } else {
                top_left[idx] = dct[x][y];
            }
            idx += 1;
        }
    }

    let mut sorted = top_left;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let median = sorted[32];

    let mut hash: i64 = 0;
    for (i, &val) in top_left.iter().enumerate() {
        if val > median {
            hash |= 1 << (63 - i);
        }
    }

    Ok(hash)
}

pub fn hamming_distance(a: i64, b: i64) -> u32 {
    (a ^ b).count_ones()
}

pub fn similarity_pct(a: i64, b: i64) -> f64 {
    let dist = hamming_distance(a, b);
    ((64 - dist) as f64 / 64.0) * 100.0
}

fn dct2d_32x32(input: &[[f64; 32]; 32]) -> [[f64; 32]; 32] {
    let mut temp = [[0.0f64; 32]; 32];
    let mut output = [[0.0f64; 32]; 32];

    let n: f64 = 32.0;
    let pi = std::f64::consts::PI;

    for u in 0..32 {
        let au = if u == 0 { (1.0 / n).sqrt() } else { (2.0 / n).sqrt() };
        for j in 0..32 {
            let mut sum = 0.0;
            for i in 0..32 {
                sum += input[i][j] * ((pi * (i as f64 + 0.5) * u as f64) / n).cos();
            }
            temp[u][j] = au * sum;
        }
    }

    for v in 0..32 {
        let av = if v == 0 { (1.0 / n).sqrt() } else { (2.0 / n).sqrt() };
        for u in 0..32 {
            let mut sum = 0.0;
            for j in 0..32 {
                sum += temp[u][j] * ((pi * (j as f64 + 0.5) * v as f64) / n).cos();
            }
            output[u][v] = av * sum;
        }
    }

    output
}
