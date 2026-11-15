//! DrawingML line ends (`a:headEnd` / `a:tailEnd`): arrowhead geometry and path shortening.

use serde_json::Value;
use tiny_skia::{Path, PathBuilder, PathSegment, Point};

const MIN_BASE_WIDTH: f32 = 2.0 * 96.0 / 72.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndKind {
    Triangle,
    Stealth,
    Diamond,
    Oval,
    Arrow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LineEnd {
    pub(crate) kind: EndKind,
    pub(crate) width: f32,
    pub(crate) length: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct LineEnds {
    pub(crate) head: Option<LineEnd>,
    pub(crate) tail: Option<LineEnd>,
}

pub(crate) struct LineEndPieces {
    pub(crate) line: Path,
    pub(crate) fills: Vec<Path>,
    pub(crate) strokes: Vec<Path>,
}

fn size_factor(name: Option<&str>) -> Result<f32, String> {
    match name {
        None | Some("med") => Ok(3.0),
        Some("sm") => Ok(2.0),
        Some("lg") => Ok(5.0),
        Some(other) => Err(format!("unsupported line end size: {other}")),
    }
}

/// Reads one `headEnd`/`tailEnd` object for a line `line_width` px wide.
pub(crate) fn parse_line_end(value: &Value, line_width: f32) -> Result<Option<LineEnd>, String> {
    if value.is_null() {
        return Ok(None);
    }
    let object = value
        .as_object()
        .ok_or_else(|| "line end must be an object".to_string())?;
    let kind = match object.get("type").and_then(Value::as_str) {
        None | Some("none") => return Ok(None),
        Some("triangle") => EndKind::Triangle,
        Some("stealth") => EndKind::Stealth,
        Some("diamond") => EndKind::Diamond,
        Some("oval") => EndKind::Oval,
        Some("arrow") => EndKind::Arrow,
        Some(other) => return Err(format!("unsupported line end type: {other}")),
    };
    let base = line_width.max(MIN_BASE_WIDTH);
    Ok(Some(LineEnd {
        kind,
        width: base
            * size_factor(
                object
                    .get("w")
                    .or(object.get("width"))
                    .and_then(Value::as_str),
            )?,
        length: base
            * size_factor(
                object
                    .get("len")
                    .or(object.get("length"))
                    .and_then(Value::as_str),
            )?,
    }))
}

#[derive(Clone, Copy)]
enum Seg {
    Move(Point),
    Line(Point),
    Quad(Point, Point),
    Cubic(Point, Point, Point),
}

impl Seg {
    fn end(self) -> Point {
        match self {
            Seg::Move(p) | Seg::Line(p) | Seg::Quad(_, p) | Seg::Cubic(_, _, p) => p,
        }
    }

    fn with_end(self, end: Point) -> Seg {
        match self {
            Seg::Move(_) => Seg::Move(end),
            Seg::Line(_) => Seg::Line(end),
            Seg::Quad(c, _) => Seg::Quad(c, end),
            Seg::Cubic(a, b, _) => Seg::Cubic(a, b, end),
        }
    }
}

struct Subpath {
    segs: Vec<Seg>,
    closed: bool,
}

fn split(path: &Path) -> Vec<Subpath> {
    let mut out: Vec<Subpath> = Vec::new();
    for segment in path.segments() {
        match segment {
            PathSegment::MoveTo(p) => out.push(Subpath {
                segs: vec![Seg::Move(p)],
                closed: false,
            }),
            PathSegment::LineTo(p) => push(&mut out, Seg::Line(p)),
            PathSegment::QuadTo(c, p) => push(&mut out, Seg::Quad(c, p)),
            PathSegment::CubicTo(a, b, p) => push(&mut out, Seg::Cubic(a, b, p)),
            PathSegment::Close => {
                if let Some(last) = out.last_mut() {
                    last.closed = true;
                }
            }
        }
    }
    out
}

fn push(out: &mut [Subpath], seg: Seg) {
    if let Some(last) = out.last_mut() {
        last.segs.push(seg);
    }
}

fn dist(a: Point, b: Point) -> f32 {
    ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt()
}

/// Unit vector pointing out of the path at its end, from the nearest distinct earlier point.
fn outward(points: &[Point]) -> Option<(f32, f32)> {
    let (&tip, rest) = points.split_last()?;
    rest.iter().rev().find_map(|&p| {
        let d = dist(tip, p);
        (d > 1e-4).then(|| ((tip.x - p.x) / d, (tip.y - p.y) / d))
    })
}

fn tail_points(segs: &[Seg]) -> Vec<Point> {
    let mut pts = Vec::new();
    for seg in segs {
        match *seg {
            Seg::Move(p) | Seg::Line(p) => pts.push(p),
            Seg::Quad(c, p) => pts.extend([c, p]),
            Seg::Cubic(a, b, p) => pts.extend([a, b, p]),
        }
    }
    pts
}

fn head_points(segs: &[Seg]) -> Vec<Point> {
    let mut pts = tail_points(segs);
    pts.reverse();
    pts
}

struct Head {
    shorten: f32,
    fill: Option<Vec<Point>>,
    ellipse: Option<(Point, f32, f32, (f32, f32))>,
    open: Option<Vec<Point>>,
}

fn build_head(end: LineEnd, tip: Point, dir: (f32, f32)) -> Head {
    let (dx, dy) = dir;
    let (nx, ny) = (-dy, dx);
    let at = |back: f32, side: f32| {
        Point::from_xy(tip.x - dx * back + nx * side, tip.y - dy * back + ny * side)
    };
    let (l, hw) = (end.length, end.width / 2.0);
    match end.kind {
        EndKind::Triangle => Head {
            shorten: l,
            fill: Some(vec![tip, at(l, hw), at(l, -hw)]),
            ellipse: None,
            open: None,
        },
        EndKind::Stealth => Head {
            shorten: l * 0.7,
            fill: Some(vec![tip, at(l, hw), at(l * 0.7, 0.0), at(l, -hw)]),
            ellipse: None,
            open: None,
        },
        EndKind::Diamond => Head {
            shorten: l / 2.0,
            fill: Some(vec![
                at(-l / 2.0, 0.0),
                at(0.0, hw),
                at(l / 2.0, 0.0),
                at(0.0, -hw),
            ]),
            ellipse: None,
            open: None,
        },
        EndKind::Oval => Head {
            shorten: l / 2.0,
            fill: None,
            ellipse: Some((tip, l / 2.0, hw, dir)),
            open: None,
        },
        EndKind::Arrow => Head {
            shorten: 0.0,
            fill: None,
            ellipse: None,
            open: Some(vec![at(l, hw), tip, at(l, -hw)]),
        },
    }
}

fn polygon(points: &[Point]) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        builder.line_to(p.x, p.y);
    }
    builder.close();
    builder.finish()
}

fn polyline(points: &[Point]) -> Option<Path> {
    let mut builder = PathBuilder::new();
    builder.move_to(points[0].x, points[0].y);
    for p in &points[1..] {
        builder.line_to(p.x, p.y);
    }
    builder.finish()
}

fn ellipse(center: Point, rx: f32, ry: f32, dir: (f32, f32)) -> Option<Path> {
    let (dx, dy) = dir;
    let mut points = Vec::with_capacity(24);
    for i in 0..24 {
        let a = i as f32 / 24.0 * std::f32::consts::TAU;
        let (u, v) = (a.cos() * rx, a.sin() * ry);
        points.push(Point::from_xy(
            center.x + dx * u - dy * v,
            center.y + dy * u + dx * v,
        ));
    }
    polygon(&points)
}

fn shorten_end(segs: &mut [Seg], dir: (f32, f32), amount: f32) {
    let Some(last) = segs.last().copied() else {
        return;
    };
    let end = last.end();
    let prev = if segs.len() >= 2 {
        segs[segs.len() - 2].end()
    } else {
        end
    };
    let available = match last {
        Seg::Line(_) => dist(prev, end),
        _ => f32::MAX,
    };
    let s = amount.min(available);
    let n = segs.len();
    segs[n - 1] = last.with_end(Point::from_xy(end.x - dir.0 * s, end.y - dir.1 * s));
}

fn reverse(sub: &Subpath) -> Vec<Seg> {
    let points: Vec<Point> = sub.segs.iter().map(|seg| seg.end()).collect();
    let mut result = vec![Seg::Move(points[points.len() - 1])];
    for i in (1..sub.segs.len()).rev() {
        let target = points[i - 1];
        result.push(match sub.segs[i] {
            Seg::Move(_) | Seg::Line(_) => Seg::Line(target),
            Seg::Quad(c, _) => Seg::Quad(c, target),
            Seg::Cubic(a, b, _) => Seg::Cubic(b, a, target),
        });
    }
    result
}

/// The path with open ends shortened under solid heads, plus the head shapes.
pub(crate) fn apply_line_ends(path: &Path, ends: &LineEnds) -> Option<LineEndPieces> {
    let mut subs = split(path);
    let mut fills = Vec::new();
    let mut strokes = Vec::new();
    let open: Vec<usize> = subs
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.closed && s.segs.len() >= 2)
        .map(|(i, _)| i)
        .collect();
    let (first, last) = (open.first().copied(), open.last().copied());
    let place = |head: Head, fills: &mut Vec<Path>, strokes: &mut Vec<Path>| {
        if let Some(points) = &head.fill {
            fills.extend(polygon(points));
        }
        if let Some((c, rx, ry, dir)) = head.ellipse {
            fills.extend(ellipse(c, rx, ry, dir));
        }
        if let Some(points) = &head.open {
            strokes.extend(polyline(points));
        }
    };
    if let (Some(end), Some(i)) = (ends.tail, last) {
        let pts = tail_points(&subs[i].segs);
        if let Some(dir) = outward(&pts) {
            let head = build_head(end, *pts.last()?, dir);
            shorten_end(&mut subs[i].segs, dir, head.shorten);
            place(head, &mut fills, &mut strokes);
        }
    }
    if let (Some(end), Some(i)) = (ends.head, first) {
        let pts = head_points(&subs[i].segs);
        if let Some(dir) = outward(&pts) {
            let head = build_head(end, *pts.last()?, dir);
            let mut reversed = reverse(&subs[i]);
            shorten_end(&mut reversed, dir, head.shorten);
            subs[i].segs = reverse(&Subpath {
                segs: reversed,
                closed: false,
            });
            place(head, &mut fills, &mut strokes);
        }
    }
    let mut builder = PathBuilder::new();
    for sub in &subs {
        for seg in &sub.segs {
            match *seg {
                Seg::Move(p) => builder.move_to(p.x, p.y),
                Seg::Line(p) => builder.line_to(p.x, p.y),
                Seg::Quad(c, p) => builder.quad_to(c.x, c.y, p.x, p.y),
                Seg::Cubic(a, b, p) => builder.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y),
            }
        }
        if sub.closed {
            builder.close();
        }
    }
    Some(LineEndPieces {
        line: builder.finish()?,
        fills,
        strokes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Path {
        let mut b = PathBuilder::new();
        b.move_to(x0, y0);
        b.line_to(x1, y1);
        b.finish().unwrap()
    }

    fn end_point(path: &Path) -> Point {
        path.points()[path.points().len() - 1]
    }

    #[test]
    fn sizes_follow_line_width_with_minimum() {
        let e = parse_line_end(&json!({"type":"triangle","w":"lg","len":"sm"}), 4.0)
            .unwrap()
            .unwrap();
        assert_eq!((e.width, e.length), (20.0, 8.0));
        let thin = parse_line_end(&json!({"type":"oval"}), 0.5)
            .unwrap()
            .unwrap();
        assert!((thin.width - 3.0 * MIN_BASE_WIDTH).abs() < 1e-4);
        assert!(
            parse_line_end(&json!({"type":"none"}), 1.0)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn triangle_tail_shortens_line_and_points_along_tangent() {
        let ends = LineEnds {
            tail: parse_line_end(&json!({"type":"triangle"}), 4.0).unwrap(),
            head: None,
        };
        let pieces = apply_line_ends(&line(0.0, 0.0, 0.0, 100.0), &ends).unwrap();
        assert!((end_point(&pieces.line).y - 88.0).abs() < 1e-3);
        let tri = pieces.fills[0].points();
        assert_eq!((tri[0].x, tri[0].y), (0.0, 100.0));
        assert!((tri[1].y - 88.0).abs() < 1e-3 && (tri[1].x.abs() - 6.0).abs() < 1e-3);
    }

    #[test]
    fn head_end_shortens_start_of_zero_width_line() {
        let ends = LineEnds {
            head: parse_line_end(&json!({"type":"triangle"}), 1.0).unwrap(),
            tail: None,
        };
        let pieces = apply_line_ends(&line(0.0, 0.0, 0.0, 50.0), &ends).unwrap();
        let start = pieces.line.points()[0];
        assert!((start.y - 2.0 * 3.0 * MIN_BASE_WIDTH / 2.0).abs() < 1e-3 || start.y > 0.0);
        let tri = pieces.fills[0].points();
        assert_eq!((tri[0].x, tri[0].y), (0.0, 0.0));
        assert!(tri[1].y > 0.0);
    }

    #[test]
    fn oval_diamond_arrow_and_stealth_geometry() {
        let mk = |t: &str| LineEnds {
            tail: parse_line_end(&json!({"type": t}), 2.0).unwrap(),
            head: None,
        };
        let l = 3.0 * MIN_BASE_WIDTH;
        let oval = apply_line_ends(&line(0.0, 0.0, 100.0, 0.0), &mk("oval")).unwrap();
        assert!((end_point(&oval.line).x - (100.0 - l / 2.0)).abs() < 1e-3);
        assert_eq!(oval.fills.len(), 1);
        let diamond = apply_line_ends(&line(0.0, 0.0, 100.0, 0.0), &mk("diamond")).unwrap();
        assert!((diamond.fills[0].bounds().right() - (100.0 + l / 2.0)).abs() < 1e-3);
        let arrow = apply_line_ends(&line(0.0, 0.0, 100.0, 0.0), &mk("arrow")).unwrap();
        assert_eq!(end_point(&arrow.line).x, 100.0);
        assert_eq!((arrow.fills.len(), arrow.strokes.len()), (0, 1));
        let stealth = apply_line_ends(&line(0.0, 0.0, 100.0, 0.0), &mk("stealth")).unwrap();
        assert!((end_point(&stealth.line).x - (100.0 - l * 0.7)).abs() < 1e-3);
        assert_eq!(stealth.fills[0].points().len(), 4);
    }

    #[test]
    fn closed_paths_get_no_ends() {
        let mut b = PathBuilder::new();
        b.move_to(0.0, 0.0);
        b.line_to(10.0, 0.0);
        b.line_to(10.0, 10.0);
        b.close();
        let ends = LineEnds {
            tail: parse_line_end(&json!({"type":"triangle"}), 1.0).unwrap(),
            head: None,
        };
        assert!(
            apply_line_ends(&b.finish().unwrap(), &ends)
                .unwrap()
                .fills
                .is_empty()
        );
    }
}
