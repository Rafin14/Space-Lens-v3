use crate::scanner::Entry;

#[derive(Clone, Copy, Debug, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x <= self.x + self.w && y <= self.y + self.h
    }
    pub fn intersect(self, other: Self) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        Self {
            x,
            y,
            w: (self.x + self.w).min(other.x + other.w) - x,
            h: (self.y + self.h).min(other.y + other.h) - y,
        }
    }
}

pub fn split_boxes(entries: &[Entry], rect: Rect, first: usize, out: &mut Vec<(Rect, usize)>) {
    if entries.is_empty() || rect.w <= 0.0 || rect.h <= 0.0 {
        return;
    }
    if entries.len() == 1 {
        out.push((rect, first));
        return;
    }
    let total: f64 = entries.iter().map(|e| e.size.unwrap_or(0) as f64).sum();
    let mut running = 0.0;
    let mut cut = 1;
    for (i, e) in entries[..entries.len() - 1].iter().enumerate() {
        running += e.size.unwrap_or(0) as f64;
        cut = i + 1;
        if running >= total / 2.0 {
            break;
        }
    }
    let fraction = running / total;
    let (a, b) = if rect.w >= rect.h {
        (
            Rect {
                w: rect.w * fraction,
                ..rect
            },
            Rect {
                x: rect.x + rect.w * fraction,
                w: rect.w * (1.0 - fraction),
                ..rect
            },
        )
    } else {
        (
            Rect {
                h: rect.h * fraction,
                ..rect
            },
            Rect {
                y: rect.y + rect.h * fraction,
                h: rect.h * (1.0 - fraction),
                ..rect
            },
        )
    };
    split_boxes(&entries[..cut], a, first, out);
    split_boxes(&entries[cut..], b, first + cut, out);
}

pub fn vertical_label(rect: Rect, name_width: f64, scale: f64, was_vertical: bool) -> bool {
    let aspect = if was_vertical { 1.35 } else { 1.7 };
    let limit = (name_width + 16.0 * scale).max(116.0 * scale)
        + if was_vertical { 12.0 * scale } else { 0.0 };
    rect.h > rect.w * aspect && rect.w < limit
}

pub fn spring_step(position: f64, velocity: f64, target: f64, dt: f64) -> (f64, f64) {
    let omega = 4.6 / 0.24;
    let displacement = position - target;
    let decay = (-omega * dt).exp();
    let temporary = (velocity + omega * displacement) * dt;
    (
        target + (displacement + temporary) * decay,
        (velocity - omega * temporary) * decay,
    )
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub rect: Rect,
    pub item: usize,
    children: usize,
}
#[derive(Default)]
pub struct Layout {
    pub nodes: Vec<Node>,
}
impl Layout {
    pub fn clear(&mut self) {
        self.nodes.clear();
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn build(&mut self, weights: impl ExactSizeIterator<Item = u64>, rect: Rect) {
        self.clear();
        let mut sums = Vec::<u128>::with_capacity(weights.len() + 1);
        sums.push(0);
        for weight in weights {
            sums.push(sums.last().unwrap() + weight as u128);
        }
        let count = sums.len() - 1;
        if count == 0 || sums[count] == 0 || rect.w <= 0. || rect.h <= 0. {
            return;
        }
        self.nodes.reserve(count * 2 - 1);
        self.nodes.push(Node {
            rect,
            item: 0,
            children: usize::MAX,
        });
        let mut work = vec![(0, 0, count)];
        while let Some((node, start, end)) = work.pop() {
            if end - start == 1 {
                self.nodes[node].item = start;
                continue;
            }
            let total = sums[end] - sums[start];
            let target = sums[start] + total.div_ceil(2);
            let mut lo = start + 1;
            let mut hi = end - 1;
            while lo < hi {
                let mid = (lo + hi) / 2;
                if sums[mid] < target {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            let cut = lo;
            let fraction = (sums[cut] - sums[start]) as f64 / total as f64;
            let r = self.nodes[node].rect;
            let (a, b) = if r.w >= r.h {
                (
                    Rect {
                        w: r.w * fraction,
                        ..r
                    },
                    Rect {
                        x: r.x + r.w * fraction,
                        w: r.w * ((sums[end] - sums[cut]) as f64 / total as f64),
                        ..r
                    },
                )
            } else {
                (
                    Rect {
                        h: r.h * fraction,
                        ..r
                    },
                    Rect {
                        y: r.y + r.h * fraction,
                        h: r.h * ((sums[end] - sums[cut]) as f64 / total as f64),
                        ..r
                    },
                )
            };
            let child = self.nodes.len();
            self.nodes[node].children = child;
            self.nodes.push(Node {
                rect: a,
                item: start,
                children: usize::MAX,
            });
            self.nodes.push(Node {
                rect: b,
                item: cut,
                children: usize::MAX,
            });
            work.push((child + 1, cut, end));
            work.push((child, start, cut));
        }
    }
    pub fn visible(&self, rect: Rect, out: &mut Vec<usize>) {
        out.clear();
        if self.nodes.is_empty() {
            return;
        }
        let mut work = vec![0];
        while let Some(index) = work.pop() {
            let node = self.nodes[index];
            let clip = node.rect.intersect(rect);
            if clip.w <= 0. || clip.h <= 0. {
                continue;
            }
            if node.children == usize::MAX {
                out.push(index);
            } else {
                work.push(node.children + 1);
                work.push(node.children);
            }
        }
    }
    pub fn hit(&self, x: f64, y: f64) -> Option<(usize, Rect)> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut work = vec![0];
        while let Some(index) = work.pop() {
            let node = self.nodes[index];
            if !node.rect.contains(x, y) {
                continue;
            }
            if node.children == usize::MAX {
                return Some((node.item, node.rect));
            }
            work.push(node.children + 1);
            work.push(node.children);
        }
        None
    }

    pub fn zoom_limit(&self, width: f64, height: f64) -> f64 {
        self.nodes
            .iter()
            .filter(|n| n.children == usize::MAX)
            .fold(12_f64, |limit, n| {
                limit.max(4. * width / n.rect.w).max(4. * height / n.rect.h)
            })
            .min(1e12)
    }

    // Search only the neighborhood through the existing spatial tree, including narrow strips.
    pub fn nearby_tiny(
        &self,
        point: (f64, f64),
        zoom: f64,
        offset: (f64, f64),
        radius: f64,
        viewport: Rect,
    ) -> Option<Rect> {
        let mut candidates = Vec::new();
        self.visible(
            Rect {
                x: (point.0 - radius - offset.0) / zoom,
                y: (point.1 - radius - offset.1) / zoom,
                w: 2. * radius / zoom,
                h: 2. * radius / zoom,
            },
            &mut candidates,
        );
        let mut best = None;
        let mut score = (f64::INFINITY, f64::INFINITY);
        for index in candidates {
            let r = self.nodes[index].rect;
            if r.w.min(r.h) * zoom > radius {
                continue;
            }
            let screen = Rect {
                x: r.x * zoom + offset.0,
                y: r.y * zoom + offset.1,
                w: r.w * zoom,
                h: r.h * zoom,
            };
            let clip = screen.intersect(viewport);
            if clip.w <= 0. || clip.h <= 0. {
                continue;
            }
            let dx = (screen.x - point.0)
                .max(0.)
                .max(point.0 - screen.x - screen.w);
            let dy = (screen.y - point.1)
                .max(0.)
                .max(point.1 - screen.y - screen.h);
            let next = (dx * dx + dy * dy, r.w * r.h);
            if next.0 <= radius * radius && next < score {
                score = next;
                best = Some(r);
            }
        }
        best
    }
}

pub fn zoom_text(zoom: f64) -> String {
    if zoom >= 1e9 {
        format!("{:.1}B×", zoom / 1e9)
    } else if zoom >= 1e6 {
        format!("{:.1}M×", zoom / 1e6)
    } else if zoom >= 1e3 {
        format!("{:.1}K×", zoom / 1e3)
    } else {
        format!("{:.0}%", zoom * 100.)
    }
}

#[cfg(test)]
mod zoom_tests {
    use super::*;
    #[test]
    fn nearby_miss_finds_tiny_and_does_not_hijack_distant_zoom() {
        let mut layout = Layout::default();
        layout.build(
            [999_999_u64, 1].into_iter(),
            Rect {
                x: 0.,
                y: 0.,
                w: 400.,
                h: 800.,
            },
        );
        let viewport = Rect {
            x: 0.,
            y: 0.,
            w: 400.,
            h: 800.,
        };
        let tiny = layout
            .nearby_tiny((390., 785.), 1., (0., 0.), 36., viewport)
            .unwrap();
        assert!(tiny.w.min(tiny.h) < 1.);
        assert!(
            layout
                .nearby_tiny((200., 400.), 1., (0., 0.), 36., viewport)
                .is_none()
        );
        assert!(layout.zoom_limit(400., 800.) > 1e6);
    }
    #[test]
    fn extreme_positive_weights_keep_real_area_and_finite_limit() {
        let mut layout = Layout::default();
        layout.build(
            [u64::MAX - 1, 1].into_iter(),
            Rect {
                x: 0.,
                y: 0.,
                w: 400.,
                h: 800.,
            },
        );
        for n in &layout.nodes {
            assert!(n.rect.w > 0. && n.rect.h > 0.);
        }
        assert_eq!(layout.zoom_limit(400., 800.), 1e12);
        assert!(zoom_text(1e12).len() < 12);
    }
}
