//! Fonts and type: fonts are found by name (their programs aren't run) and type becomes point
//! type in the app's font of that name, sized and turned as the font matrix and the current
//! transform say.

use std::rc::Rc;

use vectorcraft_color::Paint;
use vectorcraft_doc::{CharStyle, Node, NodeId, NodeKind, TextObject};
use vectorcraft_geom::{Affine, Point, Vec2};
use vectorcraft_text::TextLayout;

use super::interp::{Interp, matrix_obj, matrix_of};
use super::obj::{Dict, DictRef, Key, Obj, Op, PsError, Res, ps_err};

/// The font a program uses before it sets one.
const DEFAULT_FONT: &str = "Helvetica";

/// A font's family and style from its PostScript name: `Helvetica-BoldOblique` → (`Helvetica`,
/// `Bold Oblique`), `TimesNewRomanPS-BoldMT` → (`Times New Roman`, `Bold`), without a subset
/// prefix (`ABCDEF+`).
pub fn family_style(name: &str) -> (String, String) {
    let name = match name.split_once('+') {
        Some((prefix, rest)) if prefix.len() == 6 && prefix.bytes().all(|b| b.is_ascii_uppercase()) => rest,
        _ => name,
    };
    let (family, style) = name.split_once('-').unwrap_or((name, ""));
    let trim = |s: &str| -> String {
        let s = ["PSMT", "MT", "PS"].iter().find_map(|suffix| s.strip_suffix(suffix)).unwrap_or(s);
        // Words: a capital after a small letter starts one.
        let mut out = String::with_capacity(s.len() + 4);
        let mut prev_lower = false;
        for ch in s.chars() {
            if ch.is_ascii_uppercase() && prev_lower {
                out.push(' ');
            }
            prev_lower = ch.is_ascii_lowercase();
            out.push(ch);
        }
        out
    };
    let style = match trim(style).as_str() {
        "" | "Roman" | "Regular" | "Book" | "Normal" => "Regular".to_string(),
        s => s.to_string(),
    };
    (trim(family), style)
}

impl Interp<'_> {
    pub fn text_op(&mut self, op: Op) -> Res {
        use Op::*;
        match op {
            FindFont => {
                let k = self.pop()?;
                let font = self.find_font(&k)?;
                self.push(Obj::Dict(font))?;
            }
            DefineFont => {
                let font = self.pop_dict()?;
                let k = self.pop()?.key().ok_or(PsError::Ps("typecheck", "definefont".into()))?;
                font.borrow_mut().entry(Key::name("FontName")).or_insert_with(|| k.obj());
                self.fonts.borrow_mut().insert(k, Obj::Dict(font.clone()));
                self.push(Obj::Dict(font))?;
            }
            UndefineFont => {
                let k = self.pop()?.key().ok_or(PsError::Ps("typecheck", "undefinefont".into()))?;
                self.fonts.borrow_mut().remove(&k);
            }
            ScaleFont | MakeFont => {
                let m = if op == ScaleFont {
                    let s = self.pop_num()?;
                    Affine::scale(s)
                } else {
                    self.pop_matrix()?
                };
                let font = self.pop_dict()?;
                self.push(Obj::Dict(transformed(&font, m)))?;
            }
            SetFont => {
                let font = self.pop_dict()?;
                self.g.font = Some(font);
            }
            SelectFont => {
                let m = match self.pop()? {
                    Obj::Array { items, .. } => matrix_of(&items.borrow()).ok_or(PsError::Ps("typecheck", "selectfont".into()))?,
                    o => Affine::scale(o.as_num().ok_or(PsError::Ps("typecheck", "selectfont".into()))?),
                };
                let k = self.pop()?;
                let font = self.find_font(&k)?;
                self.g.font = Some(transformed(&font, m));
            }
            CurrentFont => {
                let font = match self.g.font.clone() {
                    Some(f) => f,
                    None => self.find_font(&Obj::name(DEFAULT_FONT))?,
                };
                self.push(Obj::Dict(font))?;
            }
            FindResource => {
                let category = self.pop()?;
                let k = self.pop()?;
                if category.text().as_deref() == Some("Font") {
                    let font = self.find_font(&k)?;
                    return self.push(Obj::Dict(font));
                }
                let key = k.key().ok_or(PsError::Ps("typecheck", "findresource".into()))?;
                let v = self.category(&category)?.borrow().get(&key).cloned();
                self.push(v.ok_or_else(|| PsError::Ps("undefinedresource", k.text().map(|t| t.to_string()).unwrap_or_default()))?)?;
            }
            DefineResource => {
                let category = self.pop()?;
                let v = self.pop()?;
                let k = self.pop()?.key().ok_or(PsError::Ps("typecheck", "defineresource".into()))?;
                // A category is defined by its implementation dictionary.
                if category.text().as_deref() == Some("Category") && !matches!(v, Obj::Dict(_)) {
                    return ps_err("typecheck", "defineresource");
                }
                self.instances(&category)?.borrow_mut().insert(k, v.clone());
                self.push(v)?;
            }
            UndefineResource => {
                let category = self.pop()?;
                let k = self.pop()?.key().ok_or(PsError::Ps("typecheck", "undefineresource".into()))?;
                self.instances(&category)?.borrow_mut().remove(&k);
            }
            ResourceStatus => {
                let category = self.pop()?;
                let k = self.pop()?.key().ok_or(PsError::Ps("typecheck", "resourcestatus".into()))?;
                let known = self.instances(&category)?.borrow().contains_key(&k);
                if known {
                    self.push(Obj::Int(1))?;
                    self.push(Obj::Int(0))?;
                }
                self.push(Obj::Bool(known))?;
            }
            ResourceForAll => self.resource_for_all()?,
            Show => {
                let s = self.pop_str()?.to_vec();
                self.show(&s)?;
            }
            AShow => {
                let s = self.pop_str()?.to_vec();
                self.nums::<2>()?;
                self.show(&s)?;
            }
            WidthShow => {
                let s = self.pop_str()?.to_vec();
                self.pop()?;
                self.nums::<2>()?;
                self.show(&s)?;
            }
            AWidthShow => {
                let s = self.pop_str()?.to_vec();
                self.nums::<2>()?;
                self.pop()?;
                self.nums::<2>()?;
                self.show(&s)?;
            }
            XShow | YShow | XYShow => {
                self.pop()?;
                let s = self.pop_str()?.to_vec();
                self.show(&s)?;
            }
            KShow => {
                let s = self.pop_str()?.to_vec();
                self.pop()?;
                self.show(&s)?;
            }
            CShow => {
                // The procedure shows or places each character: it gets its code and width.
                let s = self.pop_str()?.to_vec();
                let proc = self.pop_proc()?;
                for b in s {
                    let (_, _, v) = self.set_type(&[b], Point::ZERO)?;
                    self.push(Obj::Int(i64::from(b)))?;
                    self.push_num(v.x)?;
                    self.push_num(v.y)?;
                    if !self.body(&proc)? {
                        break;
                    }
                }
            }
            GlyphShow => {
                let name = self.pop()?.text().unwrap_or_else(|| Rc::from(""));
                let ch = glyph_char(&name);
                self.show(ch.encode_utf8(&mut [0; 4]).as_bytes())?;
            }
            StringWidth => {
                let s = self.pop_str()?.to_vec();
                let (_, _, v) = self.set_type(&s, Point::ZERO)?;
                self.push_num(v.x)?;
                self.push_num(v.y)?;
            }
            CharPath => {
                self.pop_bool()?;
                let s = self.pop_str()?.to_vec();
                self.char_path(&s)?;
            }
            SetCacheDevice => {
                self.nums::<6>()?;
            }
            SetCharWidth => {
                self.nums::<2>()?;
            }
            _ => return ps_err("undefined", op.name()),
        }
        Ok(())
    }

    /// The font named `k`: one the program defined, else a font of that name.
    fn find_font(&mut self, k: &Obj) -> Res<DictRef> {
        let key = k.key().ok_or(PsError::Ps("typecheck", "findfont".into()))?;
        if let Some(Obj::Dict(d)) = self.fonts.borrow().get(&key) {
            return Ok(d.clone());
        }
        let mut d = Dict::new();
        d.insert(Key::name("FontName"), key.obj());
        d.insert(Key::name("FontType"), Obj::Int(1));
        d.insert(Key::name("FontMatrix"), matrix_obj(Affine::scale(0.001)));
        d.insert(Key::name("FontBBox"), Obj::array(vec![Obj::Int(0), Obj::Int(-250), Obj::Int(1000), Obj::Int(1000)]));
        d.insert(Key::name("Encoding"), Obj::array(vec![Obj::name(".notdef"); 256]));
        let font = Rc::new(std::cell::RefCell::new(d));
        self.fonts.borrow_mut().insert(key, Obj::Dict(font.clone()));
        Ok(font)
    }

    /// The current font's name and its matrix for a font of one unit (user space).
    fn font(&mut self) -> Res<(String, Affine)> {
        let font = match self.g.font.clone() {
            Some(f) => f,
            None => self.find_font(&Obj::name(DEFAULT_FONT))?,
        };
        let f = font.borrow();
        let name = f.get(&Key::name("FontName")).and_then(Obj::text).map_or_else(|| DEFAULT_FONT.to_string(), |n| n.to_string());
        let m = f.get(&Key::name("FontMatrix")).and_then(|o| o.items().and_then(|i| matrix_of(&i.borrow()))).unwrap_or(Affine::scale(0.001));
        Ok((name, m * Affine::scale(1000.0)))
    }

    /// `s` (read as Latin-1) set in the current font at user point `at`: point type placed there,
    /// its layout in text space, and how far it moves the current point (user space).
    fn set_type(&mut self, s: &[u8], at: Point) -> Res<(TextObject, TextLayout, Vec2)> {
        // Laying out type costs about as much as 300 operations, and 64 more a character
        // (measured), so `{ (a) stringwidth pop pop } loop` or `cshow` on a long string ends
        // within the budget like any other loop instead of running for minutes.
        self.spend((s.len() as u64).saturating_mul(64).saturating_add(300))?;
        let (name, f1) = self.font()?;
        // Text space (y down, `size` points an em) onto the document.
        let m = self.xf() * Affine::translate(at.to_vec2()) * f1;
        let size = m.determinant().abs().sqrt();
        if !(size.is_finite() && size > 1e-3 && size < 1e5) {
            return ps_err("undefinedresult", "a font size");
        }
        let (family, font_style) = family_style(&name);
        // The available family's own spelling: `MicrosoftYaHei` is Microsoft YaHei, not "Microsoft Ya Hei".
        let font_family = vectorcraft_text::FontDb::global().find_family(&family).unwrap_or(family);
        let text: String = s.iter().map(|b| char::from(*b)).filter(|c| !c.is_control()).collect();
        let fill = if self.g.paint.is_none() { Paint::solid(vectorcraft_color::Color::BLACK) } else { self.g.paint.clone() };
        let style = CharStyle { font_family, font_style, size, fill, stroke: Paint::None, ..CharStyle::default() };
        let mut t = TextObject::point(Point::ZERO, &text, style);
        let layout = vectorcraft_text::layout(vectorcraft_text::FontDb::global(), &t);
        t.xf = m * Affine::scale_non_uniform(1.0 / size, -1.0 / size);
        t.cached_bounds = Some(layout.bounds);
        let adv: f64 = layout.glyphs.iter().map(|g| g.advance).sum();
        Ok((t, layout, f1 * Point::new(adv / size, 0.0) - f1 * Point::ZERO))
    }

    /// `show`: point type at the current point, which moves past it.
    fn show(&mut self, s: &[u8]) -> Res {
        let at = self.user_point()?;
        let (t, _, adv) = self.set_type(s, at)?;
        if !self.g.paint.is_none() && !t.plain_text().trim().is_empty() {
            let clips = self.g.clips.clone();
            self.out.push(Node::new(NodeId(0), NodeKind::Text(Box::new(t))), &clips);
        }
        self.g.cur = Some(self.xf() * (at + adv));
        Ok(())
    }

    /// `charpath`: the glyph outlines of `s` added to the path.
    fn char_path(&mut self, s: &[u8]) -> Res {
        let at = self.user_point()?;
        let (t, layout, adv) = self.set_type(s, at)?;
        for g in &layout.glyphs {
            self.grow()?;
            let mut o = g.outline.clone();
            o.apply_affine(t.xf);
            self.g.path.extend(o);
        }
        self.g.cur = Some(self.xf() * (at + adv));
        Ok(())
    }
}

/// A font scaled or transformed by `m` (`scalefont`, `makefont`).
fn transformed(font: &DictRef, m: Affine) -> DictRef {
    let mut d = font.borrow().clone();
    let fm = d.get(&Key::name("FontMatrix")).and_then(|o| o.items().and_then(|i| matrix_of(&i.borrow()))).unwrap_or(Affine::scale(0.001));
    d.insert(Key::name("FontMatrix"), matrix_obj(m * fm));
    Rc::new(std::cell::RefCell::new(d))
}

/// The character a glyph name stands for (`A`, `space`, `uni20AC`), else a bullet.
fn glyph_char(name: &str) -> char {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return c;
    }
    if let Some(hex) = name.strip_prefix("uni").or_else(|| name.strip_prefix('u'))
        && let Some(c) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
    {
        return c;
    }
    match name {
        "space" => ' ',
        _ => '\u{2022}',
    }
}
