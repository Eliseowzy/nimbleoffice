//! OMML box layout: an equation becomes positioned glyphs and rules.
//!
//! Metrics are approximations of Cambria Math (advances, axis, script
//! shifts) calibrated against Word output; where the MATH table constants
//! are unavailable the TeX ratios are used. Sizes follow TeX's
//! display/text/script/scriptscript styles with Word's scale factors.

use serde::{Deserialize, Serialize};

const PX_PER_PT: f32 = 96.0 / 72.0;
const SCRIPT_SCALE: f32 = 0.7;
const SCRIPT_SCRIPT_SCALE: f32 = 0.58;
const AXIS: f32 = 0.25;
const RULE: f32 = 0.05;
const TEXT_ASCENT: f32 = 0.8;
const TEXT_DESCENT: f32 = 0.2;
/// Height of a bracket glyph's centre above its baseline; centred on the axis,
/// an unstretched bracket sits 0.06em above the content baseline (Word).
const DELIMITER_CENTER: f32 = 0.19;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MathItem {
    /// `x` is the pen origin and `y` the baseline, relative to the equation's
    /// left edge and baseline (y grows downward); all in px.
    Glyph {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        width: f32,
        italic: bool,
        bold: bool,
    },
    Rule {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MathLayout {
    pub width: f32,
    pub ascent: f32,
    pub depth: f32,
    pub items: Vec<MathItem>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Style {
    Display,
    Text,
    Script,
    ScriptScript,
}

impl Style {
    fn scripted(self) -> Style {
        match self {
            Style::Display | Style::Text => Style::Script,
            _ => Style::ScriptScript,
        }
    }
    fn fraction(self) -> Style {
        match self {
            Style::Display => Style::Text,
            Style::Text => Style::Script,
            _ => Style::ScriptScript,
        }
    }
    fn factor(self) -> f32 {
        match self {
            Style::Display | Style::Text => 1.0,
            Style::Script => SCRIPT_SCALE,
            Style::ScriptScript => SCRIPT_SCRIPT_SCALE,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Bx {
    w: f32,
    a: f32,
    d: f32,
    simple: bool,
    items: Vec<Raw>,
}

#[derive(Debug, Clone)]
enum Raw {
    Glyph {
        text: String,
        x: f32,
        y: f32,
        size: f32,
        italic: bool,
        bold: bool,
    },
    Rule {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
}

impl Bx {
    fn put(&mut self, other: Bx, dx: f32, dy: f32) {
        for item in other.items {
            self.items.push(match item {
                Raw::Glyph {
                    text,
                    x,
                    y,
                    size,
                    italic,
                    bold,
                } => Raw::Glyph {
                    text,
                    x: x + dx,
                    y: y + dy,
                    size,
                    italic,
                    bold,
                },
                Raw::Rule { x, y, w, h } => Raw::Rule {
                    x: x + dx,
                    y: y + dy,
                    w,
                    h,
                },
            });
        }
    }
    fn hcat(parts: Vec<Bx>) -> Bx {
        let mut out = Bx::default();
        let single = parts.len() == 1;
        let mut x = 0.0;
        for part in parts {
            out.a = out.a.max(part.a);
            out.d = out.d.max(part.d);
            let w = part.w;
            out.simple = single && part.simple;
            out.put(part, x, 0.0);
            x += w;
        }
        out.w = x;
        out
    }
}

#[derive(Debug, Default)]
struct Xml {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Xml>,
    text: String,
}

impl Xml {
    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v.as_str())
    }
    fn child(&self, name: &str) -> Option<&Xml> {
        self.children.iter().find(|c| c.name == name)
    }
    fn props_attr(&self, props: &str, attr: &str) -> Option<&str> {
        self.child(props)?.child(attr)?.attr("val")
    }
}

fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

fn unescape(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_owned();
    }
    let mut out = String::new();
    let mut rest = raw;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else { break };
        let entity = &rest[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            _ => {
                let code = entity.strip_prefix('#').and_then(|n| {
                    n.strip_prefix(['x', 'X'])
                        .map_or_else(|| n.parse().ok(), |h| u32::from_str_radix(h, 16).ok())
                });
                if let Some(c) = code.and_then(char::from_u32) {
                    out.push(c);
                }
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

fn parse_xml(src: &str) -> Option<Xml> {
    let mut stack: Vec<Xml> = vec![Xml::default()];
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(body) = rest.strip_prefix("<!--") {
            rest = &body[body.find("-->")? + 3..];
        } else if let Some(body) = rest.strip_prefix("<?") {
            rest = &body[body.find("?>")? + 2..];
        } else if let Some(body) = rest.strip_prefix("</") {
            let node = stack.pop()?;
            stack.last_mut()?.children.push(node);
            rest = &body[body.find('>')? + 1..];
        } else if let Some(body) = rest.strip_prefix('<') {
            let end = body.find('>')?;
            let tag = &body[..end];
            let self_closing = tag.ends_with('/');
            let tag = tag.trim_end_matches('/');
            let mut parts = tag.splitn(2, char::is_whitespace);
            let name = local(parts.next()?).to_owned();
            let mut attrs = Vec::new();
            let mut attr_src = parts.next().unwrap_or("");
            while let Some(eq) = attr_src.find('=') {
                let key = local(attr_src[..eq].trim()).to_owned();
                let after = attr_src[eq + 1..].trim_start();
                let quote = after.chars().next()?;
                let close = after[1..].find(quote)?;
                attrs.push((key, unescape(&after[1..1 + close])));
                attr_src = &after[close + 2..];
            }
            let node = Xml {
                name,
                attrs,
                ..Xml::default()
            };
            if self_closing {
                stack.last_mut()?.children.push(node);
            } else {
                stack.push(node);
            }
            rest = &body[end + 1..];
        } else {
            let end = rest.find('<').unwrap_or(rest.len());
            stack.last_mut()?.text.push_str(&unescape(&rest[..end]));
            rest = &rest[end..];
        }
    }
    let mut root = stack.pop()?;
    if !stack.is_empty() {
        return None;
    }
    if root.children.len() == 1 {
        root.children.pop()
    } else {
        Some(root)
    }
}

fn advance(c: char, italic: bool) -> f32 {
    match c {
        '(' | ')' => 0.415,
        '[' | ']' => 0.35,
        '{' | '}' => 0.45,
        '|' => 0.26,
        ',' | ';' | ':' => 0.26,
        '.' => 0.25,
        '⋅' | '·' => 0.245,
        '…' => 0.9,
        '=' | '+' | '−' | '-' | '×' | '÷' | '±' | '<' | '>' | '≤' | '≥' | '≠' | '≈' => {
            0.7475
        }
        '∑' | '∏' => 1.1,
        '∫' => 0.55,
        '√' => 0.6,
        '∞' => 0.9,
        '→' | '←' => 1.0,
        'm' | 'M' if italic => 0.85,
        'w' => 0.72,
        'i' | 'j' | 'l' => 0.33,
        'n' | 'h' | 'u' => 0.6,
        'x' => 0.56,
        'y' => 0.53,
        c if c.is_ascii_lowercase() => 0.52,
        c if c.is_ascii_uppercase() => 0.65,
        c if c.is_ascii_digit() => 0.55,
        c if (c as u32) >= 0x2E80 => 1.0,
        _ => 0.55,
    }
}

fn is_relation(c: char) -> bool {
    matches!(c, '=' | '<' | '>' | '≤' | '≥' | '≠' | '≈' | '→' | '←')
}

fn is_binary(c: char) -> bool {
    matches!(c, '+' | '−' | '-' | '×' | '÷' | '±')
}

fn has_descender(text: &str) -> bool {
    text.chars().any(|c| "()[]{}|,;gjpqy∑∫∏".contains(c))
}

struct Layouter {
    base: f32,
}

#[derive(Clone, Copy)]
struct Ctx {
    style: Style,
    plain: bool,
    bold: bool,
}

impl Layouter {
    fn size(&self, style: Style) -> f32 {
        self.base * style.factor()
    }

    fn glyph(&self, text: &str, size: f32, italic: bool, bold: bool) -> Bx {
        let w: f32 = text.chars().map(|c| advance(c, italic) * size).sum();
        Bx {
            w,
            a: TEXT_ASCENT * size,
            d: if has_descender(text) {
                TEXT_DESCENT * size
            } else {
                0.05 * size
            },
            simple: true,
            items: vec![Raw::Glyph {
                text: text.to_owned(),
                x: 0.0,
                y: 0.0,
                size,
                italic,
                bold,
            }],
        }
    }

    fn run(&self, node: &Xml, ctx: Ctx) -> Bx {
        let sty = node.props_attr("rPr", "sty");
        let normal = node
            .child("rPr")
            .and_then(|p| p.child("nor"))
            .is_some_and(|n| n.attr("val") != Some("0"));
        let plain = ctx.plain || normal || matches!(sty, Some("p" | "b"));
        let bold = ctx.bold || matches!(sty, Some("b" | "bi"));
        let size = self.size(ctx.style);
        let spaced = matches!(ctx.style, Style::Display | Style::Text);
        let text: String = node
            .children
            .iter()
            .filter(|c| c.name == "t")
            .map(|c| c.text.as_str())
            .collect();
        let mut parts: Vec<Bx> = Vec::new();
        let mut pending = String::new();
        let mut pending_italic = false;
        let flush = |parts: &mut Vec<Bx>, pending: &mut String, italic: bool, me: &Layouter| {
            if !pending.is_empty() {
                parts.push(me.glyph(pending, size, italic, bold));
                pending.clear();
            }
        };
        for c in text.chars() {
            let italic = !plain && (c.is_ascii_alphabetic() || ('α'..='ω').contains(&c));
            let gap = if !spaced {
                0.0
            } else if is_relation(c) {
                0.278
            } else if is_binary(c) {
                0.222
            } else {
                0.0
            };
            if gap > 0.0 {
                flush(&mut parts, &mut pending, pending_italic, self);
                parts.push(Bx {
                    w: gap * size,
                    ..Bx::default()
                });
                parts.push(self.glyph(&c.to_string(), size, false, bold));
                parts.push(Bx {
                    w: gap * size,
                    ..Bx::default()
                });
                continue;
            }
            if italic != pending_italic {
                flush(&mut parts, &mut pending, pending_italic, self);
                pending_italic = italic;
            }
            pending.push(c);
        }
        flush(&mut parts, &mut pending, pending_italic, self);
        let single = parts.len() == 1;
        let mut bx = Bx::hcat(parts);
        bx.simple = single || bx.items.len() <= 1;
        bx
    }

    fn kids(&self, node: &Xml, ctx: Ctx) -> Bx {
        let parts = node
            .children
            .iter()
            .filter(|c| !c.name.ends_with("Pr") && c.name != "argPr")
            .map(|c| self.node(c, ctx))
            .collect();
        Bx::hcat(parts)
    }

    fn arg(&self, node: &Xml, name: &str, ctx: Ctx) -> Bx {
        node.child(name)
            .map_or_else(Bx::default, |child| self.kids(child, ctx))
    }

    fn node(&self, node: &Xml, ctx: Ctx) -> Bx {
        match node.name.as_str() {
            "r" => self.run(node, ctx),
            "f" => self.fraction(node, ctx),
            "sSub" | "sSup" | "sSubSup" | "sPre" => self.scripts(node, ctx),
            "d" => self.delimited(node, ctx),
            "rad" => self.radical(node, ctx),
            "nary" => self.nary(node, ctx),
            "func" => self.func(node, ctx),
            "acc" => self.accent(node, ctx),
            "bar" => self.bar(node, ctx),
            "limLow" | "limUpp" => self.limit(node, ctx),
            "m" => self.matrix(node, ctx),
            "eqArr" => self.rows(node, ctx),
            "oMath" | "e" | "box" | "phant" | "groupChr" | "borderBox" | "oMathPara" | "num"
            | "den" | "sub" | "sup" | "lim" | "fName" | "deg" => self.kids(node, ctx),
            _ => self.kids(node, ctx),
        }
    }

    fn fraction(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let inner = Ctx {
            style: ctx.style.fraction(),
            ..ctx
        };
        let n = self.arg(node, "num", inner);
        let d = self.arg(node, "den", inner);
        let display = ctx.style == Style::Display;
        let t = RULE * size;
        let axis = AXIS * size;
        let gap = if display { 3.0 * t } else { t };
        let mut up = size * if display { 0.70 } else { 0.58 };
        let mut down = size * if display { 0.70 } else { 0.50 };
        up = up.max(axis + t / 2.0 + gap + n.d);
        down = down.max(d.a + gap + t / 2.0 - axis);
        let w = n.w.max(d.w);
        let (nw, dw) = (n.w, d.w);
        let mut out = Bx {
            w,
            a: (up + n.a).max(axis + t / 2.0),
            d: (down + d.d).max(t / 2.0 - axis),
            ..Bx::default()
        };
        out.items.push(Raw::Rule {
            x: 0.0,
            y: -axis - t / 2.0,
            w,
            h: t,
        });
        out.put(n, (w - nw) / 2.0, -up);
        out.put(d, (w - dw) / 2.0, down);
        out
    }

    fn scripts(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let script = Ctx {
            style: ctx.style.scripted(),
            ..ctx
        };
        let base = self.arg(node, "e", ctx);
        let sub = node.child("sub").map(|_| self.arg(node, "sub", script));
        let sup = node.child("sup").map(|_| self.arg(node, "sup", script));
        let both = sub.is_some() && sup.is_some();
        let mut drop = if base.simple {
            0.2 * size
        } else {
            (0.2 * size).max(base.d + 0.1 * size)
        };
        if both {
            drop = drop.max(0.25 * size);
        }
        let mut raise = if base.simple {
            0.363 * size
        } else {
            (0.363 * size).max(base.a - 0.3 * size)
        };
        if let Some(sup) = &sup {
            raise = raise.max(sup.d + 0.25 * size);
        }
        if let (Some(sub), Some(sup)) = (&sub, &sup) {
            let gap = (raise - sup.d) - (sub.a - drop);
            if gap < 0.15 * size {
                drop += 0.15 * size - gap;
            }
        }
        let space = 0.1 * size;
        let pre = node.name == "sPre";
        let script_w = sub
            .as_ref()
            .map_or(0.0, |b| b.w)
            .max(sup.as_ref().map_or(0.0, |b| b.w))
            + space;
        let mut out = Bx {
            w: base.w + script_w,
            a: base.a,
            d: base.d,
            ..Bx::default()
        };
        let (base_x, script_x) = if pre { (script_w, 0.0) } else { (0.0, base.w) };
        out.put(base, base_x, 0.0);
        if let Some(sub) = sub {
            out.d = out.d.max(drop + sub.d);
            out.a = out.a.max(sub.a - drop);
            out.put(sub, script_x, drop);
        }
        if let Some(sup) = sup {
            out.a = out.a.max(raise + sup.a);
            out.put(sup, script_x, -raise);
        }
        out
    }

    fn delim_glyph(&self, ch: &str, size: f32, content_half: f32, bold: bool) -> Bx {
        if ch.is_empty() {
            return Bx::default();
        }
        let scale = (content_half * 2.0 * 0.901 / size).max(1.0);
        let gsize = size * scale;
        let base_up = AXIS * size - DELIMITER_CENTER * gsize;
        let mut bx = self.glyph(ch, gsize, false, bold);
        bx.w = ch
            .chars()
            .map(|c| advance(c, false) * size * scale.sqrt())
            .sum();
        bx.simple = false;
        bx.a = (DELIMITER_CENTER + 0.5) * gsize + base_up;
        bx.d = (0.5 - DELIMITER_CENTER) * gsize - base_up;
        for item in &mut bx.items {
            if let Raw::Glyph { y, .. } = item {
                *y = -base_up;
            }
        }
        bx
    }

    fn delimited(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let chr = |name: &str, default: &str| {
            node.props_attr("dPr", name)
                .map(str::to_owned)
                .unwrap_or_else(|| default.to_owned())
        };
        let (open, close, sep) = (chr("begChr", "("), chr("endChr", ")"), chr("sepChr", "|"));
        let mut parts = Vec::new();
        let mut first = true;
        let mut content_probe = Vec::new();
        for e in node.children.iter().filter(|c| c.name == "e") {
            if !first {
                content_probe.push(None);
            }
            first = false;
            content_probe.push(Some(self.kids(e, ctx)));
        }
        let half = content_probe
            .iter()
            .flatten()
            .map(|b| (b.a - AXIS * size).max(b.d + AXIS * size))
            .fold(0.0, f32::max);
        parts.push(self.delim_glyph(&open, size, half, ctx.bold));
        for item in content_probe {
            parts.push(match item {
                Some(b) => b,
                None => self.delim_glyph(&sep, size, half, ctx.bold),
            });
        }
        parts.push(self.delim_glyph(&close, size, half, ctx.bold));
        let mut out = Bx::hcat(parts);
        out.simple = false;
        out
    }

    fn radical(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let body = self.arg(node, "e", ctx);
        let hide = node
            .props_attr("radPr", "degHide")
            .is_some_and(|v| v != "0" && v != "false");
        let deg = (!hide).then(|| {
            self.arg(
                node,
                "deg",
                Ctx {
                    style: Style::ScriptScript,
                    ..ctx
                },
            )
        });
        let t = RULE * size;
        let gap = 0.1 * size;
        let total = body.a + body.d + gap + t;
        let scale = (total / size).max(1.0);
        let sign_w = 0.55 * size * scale.sqrt();
        let deg_w = deg.as_ref().map_or(0.0, |d| d.w);
        let lead = (deg_w - 0.2 * size).max(0.0);
        let mut out = Bx {
            w: lead + sign_w + body.w,
            a: body.a + gap + t,
            d: body.d,
            ..Bx::default()
        };
        let sign_size = size * scale;
        let sign_up = body.a + gap + t - TEXT_ASCENT * sign_size;
        let mut sign = self.glyph("√", sign_size, false, false);
        sign.w = sign_w;
        for item in &mut sign.items {
            if let Raw::Glyph { y, .. } = item {
                *y = -sign_up;
            }
        }
        out.put(sign, lead, 0.0);
        out.items.push(Raw::Rule {
            x: lead + sign_w,
            y: -(body.a + gap + t),
            w: body.w,
            h: t,
        });
        let body_w = body.w;
        let _ = body_w;
        out.put(body, lead + sign_w, 0.0);
        if let Some(deg) = deg {
            let raise = 0.55 * total;
            out.a = out.a.max(raise + deg.a);
            out.put(deg, 0.0, -raise);
        }
        out
    }

    fn nary(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let chr = node.props_attr("naryPr", "chr").unwrap_or("∫").to_owned();
        let flag = |name: &str| {
            node.props_attr("naryPr", name)
                .is_some_and(|v| v != "0" && v != "false")
        };
        let display = ctx.style == Style::Display;
        let integral = chr.contains('∫') || chr.contains('∮');
        let under_over = match node.props_attr("naryPr", "limLoc") {
            Some("undOvr") => true,
            Some(_) => false,
            None => !integral,
        } && display;
        let script = Ctx {
            style: ctx.style.scripted(),
            ..ctx
        };
        let sub = (!flag("subHide")).then(|| self.arg(node, "sub", script));
        let sup = (!flag("supHide")).then(|| self.arg(node, "sup", script));
        let op_size = size * if display { 1.4 } else { 1.0 };
        let mut op = self.glyph(&chr, op_size, false, false);
        let op_up = AXIS * size - 0.3 * op_size;
        for item in &mut op.items {
            if let Raw::Glyph { y, .. } = item {
                *y = -op_up;
            }
        }
        op.a = TEXT_ASCENT * op_size + op_up;
        op.d = TEXT_DESCENT * op_size - op_up;
        let gap = 0.1 * size;
        let mut head = Bx::default();
        if under_over {
            let w =
                op.w.max(sub.as_ref().map_or(0.0, |b| b.w))
                    .max(sup.as_ref().map_or(0.0, |b| b.w));
            head.w = w;
            head.a = op.a;
            head.d = op.d;
            let op_w = op.w;
            if let Some(sup) = sup {
                let up = op.a + gap + sup.d;
                head.a = up + sup.a;
                let sw = sup.w;
                head.put(sup, (w - sw) / 2.0, -up);
            }
            if let Some(sub) = sub {
                let down = op.d + gap + sub.a;
                head.d = down + sub.d;
                let sw = sub.w;
                head.put(sub, (w - sw) / 2.0, down);
            }
            head.put(op, (w - op_w) / 2.0, 0.0);
        } else {
            let sw = sub
                .as_ref()
                .map_or(0.0, |b| b.w)
                .max(sup.as_ref().map_or(0.0, |b| b.w));
            head.w = op.w + sw;
            head.a = op.a;
            head.d = op.d;
            let op_w = op.w;
            if let Some(sup) = sup {
                let up = (op.a - 0.3 * size).max(0.363 * size + sup.d);
                head.a = head.a.max(up + sup.a);
                head.put(sup, op_w, -up);
            }
            if let Some(sub) = sub {
                let down = (op.d + 0.1 * size).max(0.25 * size);
                head.d = head.d.max(down + sub.d);
                head.put(sub, op_w, down);
            }
            head.put(op, 0.0, 0.0);
        }
        let body = self.arg(node, "e", ctx);
        let gap_w = Bx {
            w: 0.17 * size,
            ..Bx::default()
        };
        Bx::hcat(vec![head, gap_w, body])
    }

    fn func(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let name = self.arg(node, "fName", Ctx { plain: true, ..ctx });
        let space = Bx {
            w: 0.167 * size,
            ..Bx::default()
        };
        let body = self.arg(node, "e", ctx);
        Bx::hcat(vec![name, space, body])
    }

    fn accent(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let body = self.arg(node, "e", ctx);
        let mark = match node.props_attr("accPr", "chr").unwrap_or("\u{302}") {
            "\u{302}" | "^" => "^",
            "\u{303}" | "~" => "~",
            "\u{307}" => "˙",
            "\u{304}" | "¯" => "¯",
            other => other,
        }
        .to_owned();
        let mut out = body.clone();
        let mut glyph = self.glyph(&mark, size * 0.9, false, false);
        let up = body.a - 0.5 * size;
        for item in &mut glyph.items {
            if let Raw::Glyph { y, .. } = item {
                *y = -up;
            }
        }
        let dx = (body.w - glyph.w) / 2.0;
        out.items.clear();
        out.a = body.a + 0.3 * size;
        out.simple = false;
        let bw = body.w;
        out.put(body, 0.0, 0.0);
        out.put(glyph, dx.max(0.0), 0.0);
        out.w = bw;
        out
    }

    fn bar(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let body = self.arg(node, "e", ctx);
        let top = node.props_attr("barPr", "pos") == Some("top");
        let t = RULE * size;
        let (w, a, d) = (body.w, body.a, body.d);
        let mut out = Bx {
            w,
            a,
            d,
            ..Bx::default()
        };
        if top {
            out.items.push(Raw::Rule {
                x: 0.0,
                y: -(a + 0.1 * size + t),
                w,
                h: t,
            });
            out.a = a + 0.1 * size + t;
        } else {
            out.items.push(Raw::Rule {
                x: 0.0,
                y: d + 0.1 * size,
                w,
                h: t,
            });
            out.d = d + 0.1 * size + t;
        }
        out.put(body, 0.0, 0.0);
        out
    }

    fn limit(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let body = self.arg(node, "e", ctx);
        let lim = self.arg(
            node,
            "lim",
            Ctx {
                style: ctx.style.scripted(),
                ..ctx
            },
        );
        let w = body.w.max(lim.w);
        let (bw, lw) = (body.w, lim.w);
        let gap = 0.1 * size;
        let mut out = Bx {
            w,
            a: body.a,
            d: body.d,
            ..Bx::default()
        };
        if node.name == "limUpp" {
            let up = body.a + gap + lim.d;
            out.a = up + lim.a;
            out.put(lim, (w - lw) / 2.0, -up);
        } else {
            let down = body.d + gap + lim.a;
            out.d = down + lim.d;
            out.put(lim, (w - lw) / 2.0, down);
        }
        out.put(body, (w - bw) / 2.0, 0.0);
        out
    }

    fn stack(&self, cells: Vec<Vec<Bx>>, size: f32, center: bool) -> Bx {
        let cols = cells.iter().map(Vec::len).max().unwrap_or(0);
        let mut widths = vec![0.0f32; cols];
        let mut heights = Vec::new();
        for row in &cells {
            let mut a = TEXT_ASCENT * size;
            let mut d = TEXT_DESCENT * size;
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.w);
                a = a.max(cell.a);
                d = d.max(cell.d);
            }
            heights.push((a, d));
        }
        let row_gap = 0.25 * size;
        let col_gap = 0.8 * size;
        let total_h: f32 = heights.iter().map(|(a, d)| a + d).sum::<f32>()
            + row_gap * heights.len().saturating_sub(1) as f32;
        let total_w: f32 = widths.iter().sum::<f32>() + col_gap * cols.saturating_sub(1) as f32;
        let top = -(total_h / 2.0 - AXIS * size);
        let mut out = Bx {
            w: total_w,
            a: total_h / 2.0 + AXIS * size,
            d: total_h / 2.0 - AXIS * size,
            ..Bx::default()
        };
        let mut y = top;
        for (row, (a, d)) in cells.into_iter().zip(heights) {
            let baseline = y + a;
            let mut x = 0.0;
            for (i, cell) in row.into_iter().enumerate() {
                let off = if center {
                    (widths[i] - cell.w) / 2.0
                } else {
                    0.0
                };
                out.put(cell, x + off, baseline);
                x += widths[i] + col_gap;
            }
            y += a + d + row_gap;
        }
        out
    }

    fn matrix(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let cells = node
            .children
            .iter()
            .filter(|r| r.name == "mr")
            .map(|row| {
                row.children
                    .iter()
                    .filter(|e| e.name == "e")
                    .map(|e| self.kids(e, ctx))
                    .collect()
            })
            .collect();
        self.stack(cells, size, true)
    }

    fn rows(&self, node: &Xml, ctx: Ctx) -> Bx {
        let size = self.size(ctx.style);
        let cells = node
            .children
            .iter()
            .filter(|e| e.name == "e")
            .map(|e| vec![self.kids(e, ctx)])
            .collect();
        self.stack(cells, size, false)
    }
}

/// Lays out an `m:oMath`/`m:oMathPara` fragment at `font_pt`; `display`
/// selects TeX display style, otherwise text style.
pub fn layout_omml(omml: &str, font_pt: f32, display: bool) -> Option<MathLayout> {
    let root = parse_xml(omml)?;
    let layouter = Layouter { base: font_pt };
    let ctx = Ctx {
        style: if display { Style::Display } else { Style::Text },
        plain: false,
        bold: false,
    };
    let mut bx = layouter.node(&root, ctx);
    let floor_a = TEXT_ASCENT * font_pt;
    bx.a = bx.a.max(floor_a);
    let k = PX_PER_PT;
    Some(MathLayout {
        width: bx.w * k,
        ascent: bx.a * k,
        depth: bx.d * k,
        items: bx
            .items
            .into_iter()
            .map(|item| match item {
                Raw::Glyph {
                    text,
                    x,
                    y,
                    size,
                    italic,
                    bold,
                } => MathItem::Glyph {
                    width: text.chars().map(|c| advance(c, italic) * size).sum::<f32>() * k,
                    text,
                    x: x * k,
                    y: y * k,
                    size: size * k,
                    italic,
                    bold,
                },
                Raw::Rule { x, y, w, h } => MathItem::Rule {
                    x: x * k,
                    y: y * k,
                    w: w * k,
                    h: h * k,
                },
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NS: &str = r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#;

    fn glyphs(layout: &MathLayout) -> Vec<(&str, f32, f32, f32)> {
        layout
            .items
            .iter()
            .filter_map(|i| match i {
                MathItem::Glyph {
                    text, x, y, size, ..
                } => Some((text.as_str(), *x, *y / PX_PER_PT, *size / PX_PER_PT)),
                _ => None,
            })
            .collect()
    }

    fn run(text: &str) -> String {
        format!("<m:r><m:t>{text}</m:t></m:r>")
    }

    #[test]
    fn fraction_stacks_at_script_size() {
        let xml = format!(
            "<m:oMath {NS}><m:f><m:num>{}</m:num><m:den>{}</m:den></m:f></m:oMath>",
            run("a"),
            run("b")
        );
        let layout = layout_omml(&xml, 12.0, false).unwrap();
        let g = glyphs(&layout);
        assert_eq!(g.len(), 2);
        assert!((g[0].3 - 8.4).abs() < 0.01);
        assert!((g[0].2 + 6.96).abs() < 0.01, "num baseline {}", g[0].2);
        assert!((g[1].2 - 6.0).abs() < 0.01, "den baseline {}", g[1].2);
        assert!(
            layout
                .items
                .iter()
                .any(|i| matches!(i, MathItem::Rule { .. }))
        );
        assert!(layout.ascent / PX_PER_PT > 12.0 * 0.8);
        assert!(layout.depth / PX_PER_PT > 6.0);
    }

    #[test]
    fn subscript_drops_and_shrinks() {
        let xml = format!(
            "<m:oMath {NS}><m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub></m:oMath>",
            run("x"),
            run("1")
        );
        let layout = layout_omml(&xml, 12.0, false).unwrap();
        let g = glyphs(&layout);
        assert!((g[1].2 - 2.4).abs() < 0.01);
        assert!((g[1].3 - 8.4).abs() < 0.01);
        assert!(g[1].1 > 0.0);
    }

    #[test]
    fn superscript_rises() {
        let xml = format!(
            "<m:oMath {NS}><m:sSup><m:e>{}</m:e><m:sup>{}</m:sup></m:sSup></m:oMath>",
            run("x"),
            run("2")
        );
        let layout = layout_omml(&xml, 12.0, false).unwrap();
        assert!(glyphs(&layout)[1].2 < -4.0);
    }

    #[test]
    fn sub_in_fraction_uses_script_script_size() {
        let sub = format!(
            "<m:sSub><m:e>{}</m:e><m:sub>{}</m:sub></m:sSub>",
            run("x"),
            run("1")
        );
        let xml = format!(
            "<m:oMath {NS}><m:f><m:num>{sub}</m:num><m:den>{}</m:den></m:f></m:oMath>",
            run("y")
        );
        let layout = layout_omml(&xml, 12.0, false).unwrap();
        assert!((glyphs(&layout)[1].3 - 6.96).abs() < 0.01);
    }

    #[test]
    fn delimiters_stretch_around_tall_content() {
        let frac = format!(
            "<m:f><m:num>{}</m:num><m:den>{}</m:den></m:f>",
            run("a"),
            run("b")
        );
        let tall = format!("<m:oMath {NS}><m:d><m:e>{frac}</m:e></m:d></m:oMath>");
        let flat = format!("<m:oMath {NS}><m:d><m:e>{}</m:e></m:d></m:oMath>", run("a"));
        let tall = layout_omml(&tall, 12.0, false).unwrap();
        let flat = layout_omml(&flat, 12.0, false).unwrap();
        assert!((glyphs(&flat)[0].3 - 12.0).abs() < 0.01);
        assert!(
            (glyphs(&flat)[0].2 + 0.72).abs() < 0.01,
            "bracket baseline {}",
            glyphs(&flat)[0].2
        );
        assert!(glyphs(&tall)[0].3 > 14.0);
        assert!(tall.ascent > flat.ascent);
    }

    #[test]
    fn display_nary_stacks_limits() {
        let xml = format!(
            "<m:oMathPara {NS}><m:oMath><m:nary><m:naryPr><m:chr m:val=\"∑\"/></m:naryPr><m:sub>{}</m:sub><m:sup>{}</m:sup><m:e>{}</m:e></m:nary></m:oMath></m:oMathPara>",
            run("i"),
            run("n"),
            run("a")
        );
        let layout = layout_omml(&xml, 12.0, true).unwrap();
        let g = glyphs(&layout);
        let sup = g.iter().find(|g| g.0 == "n").unwrap();
        let sub = g.iter().find(|g| g.0 == "i").unwrap();
        assert!(sup.2 < 0.0 && sub.2 > 0.0);
    }

    #[test]
    fn unknown_markup_still_lays_out_children() {
        let xml = format!("<m:oMath {NS}><m:weird>{}</m:weird></m:oMath>", run("z"));
        assert_eq!(glyphs(&layout_omml(&xml, 12.0, false).unwrap()).len(), 1);
    }
}
