//! 05-camera: Kamera, Projektion und Bodentreffer (reine Rechenteile).

use skizzeo_kern::Point3;

/// `Math.tan(Math.PI / 8)`: halber vertikaler Öffnungswinkel von 45°.
pub fn view_slope() -> f64 {
    (std::f64::consts::PI / 8.0).tan()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub azimuth: f64,
    pub elevation: f64,
    pub distance: f64,
    pub target: Point3,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            azimuth: std::f64::consts::PI / 4.0,
            elevation: 0.093,
            distance: 9.0,
            target: [0.0, 0.0, 0.0],
        }
    }
}

/// Kamerazustand samt CSS-Größe für eingefrorenes Picking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSnapshot {
    pub camera: Camera,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Basis {
    pub backward: Point3,
    pub right: Point3,
    pub up: Point3,
}

pub fn cross(a: &Point3, b: &Point3) -> Point3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: &Point3, b: &Point3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn camera_basis(azimuth: f64, elevation: f64) -> Basis {
    let (a, e) = (azimuth, elevation);
    let backward = [e.cos() * a.cos(), e.cos() * a.sin(), e.sin()];
    let right = [-a.sin(), a.cos(), 0.0];
    let up = cross(&backward, &right);
    Basis { backward, right, up }
}

/// Spaltenweise 4x4-Matrix wie das `Float32Array` des Prototyps (f32-gerundet).
pub type Matrix = [f32; 16];

pub fn view_projection(camera: &Camera, width: f64, height: f64) -> Matrix {
    let d = camera.distance;
    let Basis {
        backward: z,
        right: x,
        up: y,
    } = camera_basis(camera.azimuth, camera.elevation);
    let eye = [
        camera.target[0] + d * z[0],
        camera.target[1] + d * z[1],
        camera.target[2] + d * z[2],
    ];
    let view = [
        x[0],
        y[0],
        z[0],
        0.0,
        x[1],
        y[1],
        z[1],
        0.0,
        x[2],
        y[2],
        z[2],
        0.0,
        -dot(&x, &eye),
        -dot(&y, &eye),
        -dot(&z, &eye),
        1.0,
    ];
    let f = 1.0 / view_slope();
    let near = 0.01;
    let far = 1000f64.max(d * 4.0);
    let projection = [
        f / (width / height),
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) / (near - far),
        -1.0,
        0.0,
        0.0,
        (2.0 * far * near) / (near - far),
        0.0,
    ];
    let mut matrix = [0f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            for k in 0..4 {
                let sum = f64::from(matrix[col * 4 + row]) + projection[k * 4 + row] * view[col * 4 + k];
                matrix[col * 4 + row] = sum as f32;
            }
        }
    }
    matrix
}

pub fn clip(matrix: &Matrix, p: &Point3) -> [f64; 4] {
    let mut out = [0.0; 4];
    for (row, value) in out.iter_mut().enumerate() {
        *value = f64::from(matrix[row]) * p[0]
            + f64::from(matrix[4 + row]) * p[1]
            + f64::from(matrix[8 + row]) * p[2]
            + f64::from(matrix[12 + row]);
    }
    out
}

/// CSS-Pixelposition oder `None` bei ungültiger Tiefe.
pub fn project(p: &Point3, matrix: &Matrix, width: f64, height: f64) -> Option<[f64; 2]> {
    let c = clip(matrix, p);
    if c[3] <= 0.01 || c[2].abs() > c[3] {
        return None;
    }
    Some([
        (c[0] / c[3] * 0.5 + 0.5) * width,
        (-c[1] / c[3] * 0.5 + 0.5) * height,
    ])
}

/// Bildschirmstrahl auf Z=0.
pub fn ground_point(x: f64, y: f64, view: &ViewSnapshot) -> Option<Point3> {
    let (width, height) = (view.width, view.height);
    if width == 0.0 || height == 0.0 || !width.is_finite() || !height.is_finite() {
        return None;
    }
    let source = &view.camera;
    let d = source.distance;
    let Basis {
        backward,
        right,
        up,
    } = camera_basis(source.azimuth, source.elevation);
    let slope = view_slope();
    let sx = (2.0 * x / width - 1.0) * (width / height) * slope;
    let sy = (1.0 - 2.0 * y / height) * slope;
    let ray = [
        -backward[0] + right[0] * sx + up[0] * sy,
        -backward[1] + right[1] * sx + up[1] * sy,
        -backward[2] + right[2] * sx + up[2] * sy,
    ];
    if ray[2].abs() < 1e-7 {
        return None;
    }
    let t = -(source.target[2] + backward[2] * d) / ray[2];
    let ray_length = (ray[0] * ray[0] + ray[1] * ray[1] + ray[2] * ray[2]).sqrt();
    if t <= 0.0 || t * ray_length > d * 4.0 {
        return None;
    }
    Some([
        source.target[0] + backward[0] * d + ray[0] * t,
        source.target[1] + backward[1] * d + ray[1] * t,
        0.0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn target_projects_to_center() {
        let camera = Camera::default();
        let m = view_projection(&camera, 800.0, 600.0);
        let p = project(&[0.0, 0.0, 0.0], &m, 800.0, 600.0).unwrap();
        assert!((p[0] - 400.0).abs() < 1e-3 && (p[1] - 300.0).abs() < 1e-3);
    }

    #[test]
    fn ground_point_round_trip() {
        let camera = Camera {
            elevation: 0.7,
            ..Camera::default()
        };
        let view = ViewSnapshot {
            camera,
            width: 390.0,
            height: 823.0,
        };
        let m = view_projection(&camera, 390.0, 823.0);
        let world = [1.5, -0.5, 0.0];
        let s = project(&world, &m, 390.0, 823.0).unwrap();
        let back = ground_point(s[0], s[1], &view).unwrap();
        assert!((back[0] - world[0]).abs() < 1e-4 && (back[1] - world[1]).abs() < 1e-4);
    }
}
