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
                        w: r.w * (1. - fraction),
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
                        h: r.h * (1. - fraction),
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
}
