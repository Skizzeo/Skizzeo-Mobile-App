//! 08-gpu und 09-render: Umgebung, Achsen, Pfade und Maßzahlen als Dreieckslisten.
//!
//! Alle Koordinaten werden wie im Prototyp auf der CPU projiziert; die Dreieckslisten
//! entsprechen den WebGL-Zeichenaufrufen (Umgebung, Achsenkegel, Bildschirmpfad, Glyphen).
//! Das Ergebnis ist unabhängig von einer Grafikbibliothek ([`DrawList`]); die
//! Plattformschicht überträgt es in ihre eigenen Netze.

use skizzeo_kern::Point3;

use crate::camera::{clip, project, Matrix};
use crate::dimension_geometry::DimensionStroke;
use crate::glyphs::glyph;
use crate::style::{rgb_color, Appearance, Rgb8};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.left && x <= self.right && y >= self.top && y <= self.bottom
    }
}

/// Abgeleitetes Layout einer Maßzahl im letzten Frame.
#[derive(Clone, Debug)]
pub struct DimensionLayout {
    pub slot: usize,
    pub box_: Rect,
    pub hit_box: Rect,
}

#[derive(Clone, Copy, Debug)]
pub struct PreviewBounds {
    pub x: f64,
    pub y: f64,
    pub half_width: f64,
}

/// Einfarbige Dreiecke in Bildschirmkoordinaten (CSS-Pixel relativ zur Zeichenfläche),
/// je drei Ecken hintereinander.
#[derive(Clone, Debug, Default)]
pub struct TriangleBatch {
    pub vertices: Vec<([f64; 2], Rgb8)>,
}

/// Zeichenaufrufe eines Frames in Zeichenreihenfolge.
#[derive(Clone, Debug, Default)]
pub struct DrawList {
    pub batches: Vec<TriangleBatch>,
}

#[derive(Clone, Debug)]
pub struct FrameDimension {
    pub slot: usize,
    pub axis: usize,
    pub line_start: Point3,
    pub line_end: Point3,
    pub length: f64,
    pub text: String,
    pub affected: bool,
    pub preview: bool,
    pub hidden: bool,
    pub highlighted: bool,
    pub strokes: Vec<DimensionStroke>,
}

#[derive(Clone, Copy, Debug)]
pub struct FrameSelected {
    pub a: Point3,
    pub b: Point3,
    pub hover: bool,
}

pub struct RenderFrame {
    pub width: f64,
    pub height: f64,
    pub matrix: Matrix,
    pub style: Appearance,
    pub elevation: f64,
    pub axes_visible: bool,
    pub polygons: Vec<Vec<Point3>>,
    pub active: Vec<Point3>,
    pub live_point: Option<Point3>,
    pub close_tail: Vec<Point3>,
    pub selected: Option<FrameSelected>,
    pub dimensions: Vec<FrameDimension>,
}

pub struct RenderOutput {
    pub dimension_layouts: Vec<DimensionLayout>,
    pub preview_bounds: Option<PreviewBounds>,
    /// Bildschirmlage der Achsbeschriftungen; `None` = verborgen.
    pub axis_labels: Vec<Option<(f64, f64)>>,
}

#[derive(Clone, Copy, Debug)]
pub struct TextMetrics {
    pub width: f64,
    pub bottom: f64,
    pub top: f64,
}

type Rgb = [f64; 3];

const DIMENSION_TEXT: Rgb = [0.94, 0.95, 0.87];
const DIMENSION_LINE: Rgb = [0.82, 0.83, 0.77];

fn point(axis: usize, value: f64) -> Point3 {
    let mut p = [0.0; 3];
    p[axis] = value;
    p
}

fn shade(color: Rgb, factor: f64, offset: f64) -> Rgb {
    color.map(|v| v * factor + offset)
}

fn glyph_width(appearance: &Appearance) -> f64 {
    if appearance.text("font") == "narrow" {
        0.78
    } else {
        1.0
    }
}

fn glyph_advance(c: char, appearance: &Appearance) -> f64 {
    if appearance.text("font") == "mono" {
        0.92
    } else {
        glyph(c).map_or(0.0, |g| g.advance)
    }
}

pub fn text_metrics(text: &str, appearance: &Appearance) -> TextMetrics {
    let width: f64 = text.chars().map(|c| glyph_advance(c, appearance)).sum();
    TextMetrics {
        width: width * glyph_width(appearance),
        bottom: -0.25,
        top: 1.1,
    }
}

pub fn label_point(anchor: &Point3, axis: usize, x: f64, y: f64, direction: f64) -> Point3 {
    let x = x * direction;
    let y = y * direction;
    if axis == 0 {
        [anchor[0] - x, anchor[1] - y, 0.0]
    } else {
        [anchor[0] - y, anchor[1] + x, 0.0]
    }
}

fn boxes_overlap(a: &Rect, b: &Rect, gap: f64) -> bool {
    a.left < b.right + gap && a.right + gap > b.left && a.top < b.bottom + gap && a.bottom + gap > b.top
}

/// Sammelt Dreiecke in Bildschirmkoordinaten (CSS-Pixel relativ zur Zeichenfläche).
#[derive(Default)]
struct Triangles {
    batch: TriangleBatch,
}

impl Triangles {
    fn new() -> Self {
        Self::default()
    }

    fn triangle(&mut self, a: [f64; 2], b: [f64; 2], c: [f64; 2], color: Rgb8) {
        self.batch.vertices.extend([(a, color), (b, color), (c, color)]);
    }

    fn finish(self, list: &mut DrawList) {
        if !self.batch.vertices.is_empty() {
            list.batches.push(self.batch);
        }
    }
}

/// Clip-Raum-Dreieck gegen die nahe und ferne Ebene beschneiden (wie die GPU)
/// und in Bildschirmkoordinaten übertragen.
fn clipped_triangle(
    out: &mut Triangles,
    corners: [[f64; 4]; 3],
    width: f64,
    height: f64,
    color: Rgb8,
) {
    let mut polygon: Vec<[f64; 4]> = corners.to_vec();
    for plane in [1.0, -1.0] {
        // nah: z + w >= 0, fern: w - z >= 0
        let distance = |c: &[f64; 4]| c[3] + plane * c[2];
        let mut next = Vec::with_capacity(polygon.len() + 2);
        for i in 0..polygon.len() {
            let a = polygon[i];
            let b = polygon[(i + 1) % polygon.len()];
            let (da, db) = (distance(&a), distance(&b));
            if da >= 0.0 {
                next.push(a);
            }
            if (da >= 0.0) != (db >= 0.0) {
                let t = da / (da - db);
                let mut c = [0.0; 4];
                for k in 0..4 {
                    c[k] = a[k] + (b[k] - a[k]) * t;
                }
                next.push(c);
            }
        }
        polygon = next;
        if polygon.len() < 3 {
            return;
        }
    }
    if polygon.iter().any(|c| c[3] <= 0.0) {
        return;
    }
    let screen: Vec<[f64; 2]> = polygon
        .iter()
        .map(|c| {
            [
                (c[0] / c[3] * 0.5 + 0.5) * width,
                (-c[1] / c[3] * 0.5 + 0.5) * height,
            ]
        })
        .collect();
    for i in 1..screen.len() - 1 {
        out.triangle(screen[0], screen[i], screen[i + 1], color);
    }
}

/// Fragment-Shader der Umgebung, zeilenweise ausgewertet (hängt nur von der Höhe ab).
fn draw_environment(frame: &RenderFrame, pixels_per_point: f64, list: &mut DrawList) {
    let style = &frame.style;
    let e = frame.elevation;
    let ratio = pixels_per_point.min(2.0);
    let pixel_height = (frame.height * ratio).round().max(1.0);
    let ground_color = style.rgb("groundColor");
    let ground_near = style.rgb("groundNear");
    let horizon_color = style.rgb("horizonColor");
    let sky_color = style.rgb("skyColor");
    let softness = style.num("horizonSoftness");
    let horizon_visible = if style.text("horizonVisible") == "on" { 1.0 } else { 0.0 };
    let gradient = if style.text("groundMode") == "gradient" { 1.0 } else { 0.0 };
    let mix = |a: Rgb, b: Rgb, t: f64| -> Rgb { [0, 1, 2].map(|i| a[i] * (1.0 - t) + b[i] * t) };
    let edge = (0.41421356237 * e.cos().abs() / pixel_height * softness).max(0.00001);
    // Zeilen in Gerätepixeln der tatsächlichen Darstellung
    let rows = (frame.height * pixels_per_point).round().max(1.0) as usize;
    let row_height = frame.height / rows as f64;
    let mut out = Triangles::new();
    let mut previous: Option<(Rgb8, f64)> = None;
    let push = |out: &mut Triangles, color: Rgb8, top: f64, bottom: f64| {
        let (a, b, c, d) = ([0.0, top], [frame.width, top], [frame.width, bottom], [0.0, bottom]);
        out.triangle(a, b, c, color);
        out.triangle(a, c, d, color);
    };
    for row in 0..rows {
        let top = row as f64 * row_height;
        let center_y = 1.0 - 2.0 * (row as f64 + 0.5) / rows as f64;
        let ray_z = -e.sin() + center_y * 0.41421356237 * e.cos();
        let ground = mix(ground_color, ground_near, (-ray_z).clamp(0.0, 1.0) * gradient);
        let sky = mix(horizon_color, sky_color, (ray_z / 0.32).clamp(0.0, 1.0));
        let t = ((ray_z + edge) / (2.0 * edge)).clamp(0.0, 1.0);
        let smooth = t * t * (3.0 - 2.0 * t);
        let color = rgb_color(mix(ground, sky, smooth * horizon_visible));
        match previous {
            Some((current, start)) if current == color => {
                previous = Some((current, start));
            }
            Some((current, start)) => {
                push(&mut out, current, start, top);
                previous = Some((color, top));
            }
            None => previous = Some((color, top)),
        }
    }
    if let Some((color, start)) = previous {
        push(&mut out, color, start, frame.height);
    }
    out.finish(list);
}

/// Achsenkegel (Tiefentest durch Sortierung von hinten nach vorn).
fn draw_axis_cones(frame: &RenderFrame, list: &mut DrawList) {
    let style = &frame.style;
    let scale = style.num("axisSize") / 2.5;
    let mut triangles: Vec<([[f64; 4]; 3], Rgb8, f64)> = Vec::new();
    for axis in 0..3 {
        let color = rgb_color(style.rgb(["axisX", "axisY", "axisZ"][axis]));
        let scaled = |p: Point3| clip(&frame.matrix, &p.map(|v| v * scale));
        let rim = |angle: f64| {
            let mut p = point(axis, 2.39);
            p[(axis + 1) % 3] = angle.cos() * 0.038;
            p[(axis + 2) % 3] = angle.sin() * 0.038;
            p
        };
        for segment in 0..16 {
            let corners = [
                scaled(point(axis, 2.53)),
                scaled(rim(segment as f64 * std::f64::consts::PI / 8.0)),
                scaled(rim((segment + 1) as f64 * std::f64::consts::PI / 8.0)),
            ];
            let depth = corners
                .iter()
                .map(|c| if c[3].abs() > 1e-12 { c[2] / c[3] } else { 1.0 })
                .sum::<f64>();
            triangles.push((corners, color, depth));
        }
    }
    triangles.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = Triangles::new();
    for (corners, color, _) in triangles {
        clipped_triangle(&mut out, corners, frame.width, frame.height, color);
    }
    out.finish(list);
}

fn axis_labels(frame: &RenderFrame) -> Vec<Option<(f64, f64)>> {
    let style = &frame.style;
    (0..3)
        .map(|axis| {
            let c = clip(&frame.matrix, &point(axis, style.num("axisSize") * 2.72 / 2.5));
            let hidden = style.text("axisLabels") == "off"
                || c[3] <= 0.0
                || c[..3].iter().any(|v| v.abs() > c[3]);
            (!hidden).then(|| {
                (
                    (c[0] / c[3] * 0.5 + 0.5) * frame.width,
                    (-c[1] / c[3] * 0.5 + 0.5) * frame.height,
                )
            })
        })
        .collect()
}

struct PathPainter<'a> {
    out: Triangles,
    width: f64,
    height: f64,
    style: &'a Appearance,
}

impl PathPainter<'_> {
    fn line(&mut self, a: Option<[f64; 2]>, b: Option<[f64; 2]>, color: Rgb, dashed: bool, weight: f64) {
        let (Some(a), Some(b)) = (a, b) else { return };
        let color = rgb_color(color);
        let (vx, vy) = (b[0] - a[0], b[1] - a[1]);
        let (mut enter, mut exit) = (0.0f64, 1.0f64);
        for (p, q) in [
            (-vx, a[0] + 8.0),
            (vx, self.width + 8.0 - a[0]),
            (-vy, a[1] + 8.0),
            (vy, self.height + 8.0 - a[1]),
        ] {
            if p.abs() < 1e-12 {
                if q < 0.0 {
                    return;
                }
                continue;
            }
            let t = q / p;
            if p < 0.0 {
                enter = enter.max(t);
            } else {
                exit = exit.min(t);
            }
            if enter > exit {
                return;
            }
        }
        let b = [a[0] + vx * exit, a[1] + vy * exit];
        let a = [a[0] + vx * enter, a[1] + vy * enter];
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let length = dx.hypot(dy);
        if length < 0.01 {
            return;
        }
        let nx = -dy / length * weight / 2.0;
        let ny = dx / length * weight / 2.0;
        let dash_length = self.style.num("dashLength");
        let step = if dashed {
            dash_length + self.style.num("dashGap")
        } else {
            length
        };
        let mut start = 0.0;
        while start < length {
            let end = length.min(if dashed { start + dash_length } else { length });
            let p = [a[0] + dx * start / length, a[1] + dy * start / length];
            let q = [a[0] + dx * end / length, a[1] + dy * end / length];
            let corners = [
                [p[0] + nx, p[1] + ny],
                [p[0] - nx, p[1] - ny],
                [q[0] - nx, q[1] - ny],
                [q[0] + nx, q[1] + ny],
            ];
            self.out.triangle(corners[0], corners[1], corners[2], color);
            self.out.triangle(corners[0], corners[2], corners[3], color);
            start += step;
        }
    }

    fn disk(&mut self, p: Option<[f64; 2]>, radius: f64, color: Rgb) {
        let Some(p) = p else { return };
        if p[0] < -radius || p[0] > self.width + radius || p[1] < -radius || p[1] > self.height + radius {
            return;
        }
        let color = rgb_color(color);
        for i in 0..20 {
            let a0 = i as f64 * std::f64::consts::PI / 10.0;
            let a1 = (i + 1) as f64 * std::f64::consts::PI / 10.0;
            self.out.triangle(
                p,
                [p[0] + a0.cos() * radius, p[1] + a0.sin() * radius],
                [p[0] + a1.cos() * radius, p[1] + a1.sin() * radius],
                color,
            );
        }
    }
}

fn draw_paths(frame: &RenderFrame, list: &mut DrawList) {
    let style = &frame.style;
    let (width, height, matrix) = (frame.width, frame.height, frame.matrix);
    let screen = |p: &Point3| project(p, &matrix, width, height);
    let amber = style.rgb("polygonColor");
    let violet = style.rgb("editColor");
    let vertex_color = style.rgb("vertexColor");
    let polygon_width = style.num("polygonWidth");
    let vertex_radius = style.num("vertexRadius");
    let mut pen = PathPainter {
        out: Triangles::new(),
        width,
        height,
        style,
    };
    if frame.axes_visible {
        let axis_width = style.num("axisWidth");
        let size = style.num("axisSize");
        for axis in 0..3 {
            let color = style.rgb(["axisX", "axisY", "axisZ"][axis]);
            pen.line(screen(&point(axis, 0.0)), screen(&point(axis, size)), color, false, axis_width);
            if style.text("axesMode") == "both" {
                pen.line(
                    screen(&point(axis, -size)),
                    screen(&point(axis, 0.0)),
                    shade(color, 0.55, 0.1),
                    false,
                    axis_width,
                );
            }
        }
    }
    for path in &frame.polygons {
        let points: Vec<Option<[f64; 2]>> = path.iter().map(screen).collect();
        for (i, p) in points.iter().enumerate() {
            pen.line(*p, points[(i + 1) % points.len()], amber, false, polygon_width);
            pen.disk(*p, vertex_radius, vertex_color);
        }
    }
    if let Some(selected) = frame.selected {
        let a = screen(&selected.a);
        let b = screen(&selected.b);
        let color = style.rgb(if selected.hover { "hoverColor" } else { "editColor" });
        let radius = style.num("highlightRadius");
        pen.line(a, b, color, false, style.num("highlightWidth"));
        pen.disk(a, radius, color);
        pen.disk(b, radius, color);
    }
    if !frame.active.is_empty() {
        let points: Vec<Option<[f64; 2]>> = frame.active.iter().map(screen).collect();
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                pen.line(points[i - 1], *p, amber, false, polygon_width);
            }
            pen.disk(*p, vertex_radius, vertex_color);
        }
        let start_radius = style.num("startRadius");
        pen.disk(points[0], start_radius, style.rgb("startColor"));
        pen.disk(points[0], start_radius / 2.0, style.rgb("groundColor"));
        let preview_width = style.num("previewWidth");
        for pair in frame.close_tail.windows(2) {
            pen.line(screen(&pair[0]), screen(&pair[1]), violet, true, preview_width);
        }
        if let Some(live) = frame.live_point {
            let live = screen(&live);
            let preview = style.rgb("previewColor");
            pen.line(*points.last().expect("Zug"), live, preview, true, preview_width);
            pen.disk(live, vertex_radius + 1.0, preview);
        }
    }
    let mut drawn: Vec<([u64; 6], bool)> = Vec::new();
    let line_minimum = style.num("lineMinimumPixels");
    let line_width = style.num("dimensionLineWidth");
    let symbol_weight = style.num("symbolWeight");
    for record in &frame.dimensions {
        let color = if record.affected { violet } else { DIMENSION_LINE };
        let (Some(a), Some(b)) = (screen(&record.line_start), screen(&record.line_end)) else {
            continue;
        };
        let scale = (b[0] - a[0]).hypot(b[1] - a[1]) / record.length;
        for item in &record.strokes {
            let bits = [
                item.a[0], item.a[1], item.a[2], item.b[0], item.b[1], item.b[2],
            ]
            .map(f64::to_bits);
            let key = (bits, record.affected);
            if drawn.contains(&key) {
                continue;
            }
            drawn.push(key);
            pen.line(
                screen(&item.a),
                screen(&item.b),
                color,
                false,
                line_minimum.max(scale * line_width) * if item.tick { symbol_weight } else { 1.0 },
            );
        }
    }
    pen.out.finish(list);
}

struct Placed {
    index: usize,
    direction: f64,
    anchor: Point3,
    size: f64,
    metrics: TextMetrics,
    box_: Option<Rect>,
    ink_box: Option<Rect>,
    hit_box: Option<Rect>,
}

fn place_dimension(index: usize, record: &FrameDimension, frame: &RenderFrame) -> Placed {
    let style = &frame.style;
    let (matrix, width, height) = (frame.matrix, frame.width, frame.height);
    let metrics = text_metrics(&record.text, style);
    let middle: Point3 = [0, 1, 2].map(|i| (record.line_start[i] + record.line_end[i]) / 2.0);
    let p = project(&middle, &matrix, width, height);
    let u = project(&label_point(&middle, record.axis, 1.0, 0.0, 1.0), &matrix, width, height);
    let (ux, uy) = match (p, u) {
        (Some(p), Some(u)) => (u[0] - p[0], u[1] - p[1]),
        _ => (0.0, 0.0),
    };
    let direction = if uy.abs() > ux.abs() && uy > 0.0 { -1.0 } else { 1.0 };
    let size = style.num("dimensionTextHeight");
    let center = label_point(
        &middle,
        record.axis,
        0.0,
        style.num("textGap") + (metrics.top - metrics.bottom) * size / 2.0,
        direction,
    );
    let anchor = label_point(&center, record.axis, 0.0, -(metrics.top + metrics.bottom) * size / 2.0, direction);
    let corners: Vec<Option<[f64; 2]>> = [
        [-metrics.width / 2.0, metrics.bottom],
        [metrics.width / 2.0, metrics.bottom],
        [metrics.width / 2.0, metrics.top],
        [-metrics.width / 2.0, metrics.top],
    ]
    .iter()
    .map(|[x, y]| {
        project(
            &label_point(&anchor, record.axis, x * size, y * size, direction),
            &matrix,
            width,
            height,
        )
    })
    .collect();
    let mut placed = Placed {
        index,
        direction,
        anchor,
        size,
        metrics,
        box_: None,
        ink_box: None,
        hit_box: None,
    };
    if corners.iter().any(Option::is_none) {
        return placed;
    }
    let xs: Vec<f64> = corners.iter().map(|c| c.expect("Ecke")[0]).collect();
    let ys: Vec<f64> = corners.iter().map(|c| c.expect("Ecke")[1]).collect();
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let max = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let cx = (min(&xs) + max(&xs)) / 2.0;
    let cy = (min(&ys) + max(&ys)) / 2.0;
    let hit_size = style.num("hitSize");
    let half_width = (hit_size / 2.0).max((max(&xs) - min(&xs)) / 2.0 + 3.0);
    let half_height = (hit_size / 2.0).max((max(&ys) - min(&ys)) / 2.0 + 3.0);
    placed.box_ = Some(Rect {
        left: cx - half_width,
        right: cx + half_width,
        top: cy - half_height,
        bottom: cy + half_height,
    });
    placed.ink_box = Some(Rect {
        left: min(&xs) - 2.0,
        right: max(&xs) + 2.0,
        top: min(&ys) - 2.0,
        bottom: max(&ys) + 2.0,
    });
    placed
}

fn layout_dimensions(frame: &RenderFrame) -> Vec<Placed> {
    let mut placed: Vec<Placed> = frame
        .dimensions
        .iter()
        .enumerate()
        .filter(|(_, record)| !record.hidden)
        .map(|(index, record)| place_dimension(index, record, frame))
        .collect();
    let boxed: Vec<usize> = (0..placed.len()).filter(|&i| placed[i].box_.is_some()).collect();
    for &i in &boxed {
        placed[i].hit_box = placed[i].box_;
    }
    for (bi, &i) in boxed.iter().enumerate() {
        for &j in &boxed[..bi] {
            let (a_hit, b_hit) = (placed[i].hit_box.expect("Box"), placed[j].hit_box.expect("Box"));
            if !boxes_overlap(&a_hit, &b_hit, 0.0) {
                continue;
            }
            let (a_ink, b_ink) = (placed[i].ink_box.expect("Box"), placed[j].ink_box.expect("Box"));
            // [low, high]: links/rechts, dann oben/unten
            for horizontal in [true, false] {
                let low = |r: &Rect| if horizontal { r.left } else { r.top };
                let high = |r: &Rect| if horizontal { r.right } else { r.bottom };
                let (first, last) = if low(&a_ink) < low(&b_ink) { (i, j) } else { (j, i) };
                let (first_ink, last_ink) = (placed[first].ink_box.expect("Box"), placed[last].ink_box.expect("Box"));
                if high(&first_ink) > low(&last_ink) {
                    continue;
                }
                let split = (high(&first_ink) + low(&last_ink)) / 2.0;
                let first_hit = placed[first].hit_box.as_mut().expect("Box");
                if horizontal {
                    first_hit.right = first_hit.right.min(split);
                } else {
                    first_hit.bottom = first_hit.bottom.min(split);
                }
                let last_hit = placed[last].hit_box.as_mut().expect("Box");
                if horizontal {
                    last_hit.left = last_hit.left.max(split);
                } else {
                    last_hit.top = last_hit.top.max(split);
                }
                break;
            }
        }
    }
    placed
}

fn draw_dimensions(frame: &RenderFrame, list: &mut DrawList) -> (Vec<DimensionLayout>, Option<PreviewBounds>) {
    let style = &frame.style;
    let placed = layout_dimensions(frame);
    let mut layouts = Vec::new();
    let mut preview_bounds = None;
    let mut out = Triangles::new();
    let narrow = glyph_width(style);
    for layout in &placed {
        let record = &frame.dimensions[layout.index];
        if let (Some(box_), Some(hit_box)) = (layout.box_, layout.hit_box) {
            layouts.push(DimensionLayout {
                slot: record.slot,
                box_,
                hit_box,
            });
            if record.preview {
                preview_bounds = Some(PreviewBounds {
                    x: (box_.left + box_.right) / 2.0,
                    y: (box_.top + box_.bottom) / 2.0,
                    half_width: (box_.right - box_.left) / 2.0,
                });
            }
        }
        let color = rgb_color(if record.affected {
            style.rgb("editColor")
        } else if record.preview {
            style.rgb("previewColor")
        } else if record.highlighted {
            style.rgb("hoverColor")
        } else {
            DIMENSION_TEXT
        });
        let mut pen_x = -layout.metrics.width / (2.0 * narrow);
        for c in record.text.chars() {
            if let Some(g) = glyph(c) {
                for tri in g.triangles.chunks_exact(6) {
                    let corner = |k: usize| {
                        clip(
                            &frame.matrix,
                            &label_point(
                                &layout.anchor,
                                record.axis,
                                (pen_x + tri[2 * k]) * layout.size * narrow,
                                tri[2 * k + 1] * layout.size,
                                layout.direction,
                            ),
                        )
                    };
                    clipped_triangle(&mut out, [corner(0), corner(1), corner(2)], frame.width, frame.height, color);
                }
            }
            pen_x += glyph_advance(c, style);
        }
    }
    out.finish(list);
    (layouts, preview_bounds)
}

/// Einen Frame in Zeichenaufrufe übersetzen. `pixels_per_point` ist die Pixeldichte
/// der Ausgabe (Zeilen der Umgebung in Gerätepixeln).
pub fn render(frame: &RenderFrame, pixels_per_point: f64) -> (DrawList, RenderOutput) {
    let mut list = DrawList::default();
    draw_environment(frame, pixels_per_point, &mut list);
    let mut labels = Vec::new();
    if frame.axes_visible {
        draw_axis_cones(frame, &mut list);
        labels = axis_labels(frame);
    }
    draw_paths(frame, &mut list);
    let (dimension_layouts, preview_bounds) = draw_dimensions(frame, &mut list);
    let output = RenderOutput {
        dimension_layouts,
        preview_bounds,
        axis_labels: labels,
    };
    (list, output)
}
