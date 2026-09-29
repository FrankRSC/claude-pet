/// Rectángulo en píxeles físicos de escritorio (left, top, right, bottom).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub l: f32,
    pub t: f32,
    pub r: f32,
    pub b: f32,
}

impl Rect {
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.l && x < self.r && y >= self.t && y < self.b
    }

    fn dist2(&self, x: f32, y: f32) -> f32 {
        let dx = (self.l - x).max(0.0).max(x - self.r);
        let dy = (self.t - y).max(0.0).max(y - self.b);
        dx * dx + dy * dy
    }
}

/// Área de trabajo que contiene el punto, o la más cercana.
pub fn area_at(areas: &[Rect], x: f32, y: f32) -> Rect {
    areas
        .iter()
        .find(|a| a.contains(x, y))
        .or_else(|| {
            areas
                .iter()
                .min_by(|a, b| a.dist2(x, y).total_cmp(&b.dist2(x, y)))
        })
        .copied()
        .unwrap_or(Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 })
}

pub fn in_any(areas: &[Rect], x: f32, y: f32) -> bool {
    areas.iter().any(|a| a.contains(x, y))
}

/// Tramo visible del borde superior de una ventana, donde un monito puede pararse.
#[derive(Clone, Copy, Debug)]
pub struct Floor {
    pub id: u64,
    pub y: f32,
    pub x0: f32,
    pub x1: f32,
    /// Borde izquierdo de la ventana, para arrastrar al monito si la ventana se mueve.
    pub left: f32,
}

/// `windows` va de la ventana de más adelante a la de más atrás. Del borde superior
/// de cada una se restan los pedazos tapados por las que tiene enfrente.
pub fn floors(windows: &[(u64, Rect)], areas: &[Rect], min_width: f32) -> Vec<Floor> {
    let mut out = Vec::new();
    for (i, (id, w)) in windows.iter().enumerate() {
        let y = w.t;
        // Maximizadas o pegadas al techo: su borde es el techo, no una plataforma.
        let mid = (w.l + w.r) / 2.0;
        if !in_any(areas, mid, y + 1.0) || areas.iter().any(|a| a.contains(mid, y + 1.0) && y <= a.t + 8.0) {
            continue;
        }
        let mut segs = vec![(w.l, w.r)];
        for (_, o) in &windows[..i] {
            if o.t <= y + 2.0 && o.b > y {
                segs = segs
                    .into_iter()
                    .flat_map(|(a, b)| {
                        let mut parts = Vec::with_capacity(2);
                        if o.l > a {
                            parts.push((a, o.l.min(b)));
                        }
                        if o.r < b {
                            parts.push((o.r.max(a), b));
                        }
                        parts
                    })
                    .filter(|(a, b)| b > a)
                    .collect();
            }
        }
        for (x0, x1) in segs {
            if x1 - x0 >= min_width {
                out.push(Floor { id: *id, y, x0, x1, left: w.l });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_parts_are_removed() {
        let screen = [Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 }];
        let back = Rect { l: 100.0, t: 300.0, r: 900.0, b: 800.0 };
        let front = Rect { l: 400.0, t: 200.0, r: 600.0, b: 700.0 };
        let fs = floors(&[(1, front), (2, back)], &screen, 50.0);
        let back_segs: Vec<_> = fs.iter().filter(|f| f.id == 2).map(|f| (f.x0, f.x1)).collect();
        assert_eq!(back_segs, vec![(100.0, 400.0), (600.0, 900.0)]);
        assert!(fs.iter().any(|f| f.id == 1 && f.y == 200.0));
    }

    #[test]
    fn maximized_windows_are_not_floors() {
        let screen = [Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 }];
        let max = Rect { l: 0.0, t: 0.0, r: 1920.0, b: 1040.0 };
        assert!(floors(&[(1, max)], &screen, 50.0).is_empty());
    }
}
