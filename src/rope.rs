use std::{
    collections::VecDeque,
    ops::{Bound, RangeBounds},
    sync::Arc,
};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Debug)]
pub enum Rope {
    Branch {
        length: usize,
        depth: usize,
        newlines: Vec<usize>,
        left: Option<Arc<Rope>>,
        right: Option<Arc<Rope>>,
    },
    Leaf {
        length: usize,
        newlines: Vec<usize>,
        text: String,
    },
}

impl Default for Rope {
    fn default() -> Self {
        Self::Leaf {
            length: 0,
            newlines: Vec::new(),
            text: "".to_owned(),
        }
    }
}

impl Rope {
    const LEAF_UNIT_COUNT: usize = 32;
    const NEWLINE: &'static str = "\n";

    pub fn from_str(s: &str) -> Arc<Self> {
        // assemble leaves
        let mut gs = s.graphemes(true).peekable();
        let mut leaves = Vec::new();
        // split input string into chunks
        while let Some(_) = gs.peek() {
            let mut length = Self::LEAF_UNIT_COUNT;
            let mut newlines = Vec::new();
            let mut text = String::new();
            for i in 0..Self::LEAF_UNIT_COUNT {
                if let Some(g) = gs.next() {
                    if g == Self::NEWLINE { newlines.push(i); }
                    text.push_str(g);
                } else {
                    length = i;
                    break;
                }
            }
            // since this string will never be modified again...
            text.shrink_to_fit();
            leaves.push(Arc::new(Self::Leaf { length, newlines, text }));
        }
        Self::from_leaves(&leaves)
    }

    fn from_leaves(leaves: &[Arc<Self>]) -> Arc<Self> {
        match leaves.len() {
            0 => Arc::new(Self::default()),
            1 => Arc::clone(&leaves[0]),
            n => {
                let half = n / 2;
                let root = Self::concat(
                    &Self::from_leaves(&leaves[0..half]),
                    &Self::from_leaves(&leaves[half..n]),
                );
                root
            },
        }
    }

    pub fn concat(left: &Arc<Self>, right: &Arc<Self>) -> Arc<Self> {
        // short leaf optimization
        if let Self::Leaf {
            length: ref rlen,
            newlines: ref r_newlines,
            text: ref rtext
        } = **right {
            let mut lhandle = left;
            while let Self::Branch { right: Some(ref r), .. } = **lhandle {
                lhandle = &r;
            }
            if let Self::Leaf {
                length: ref llen,
                newlines: ref l_newlines,
                text: ref ltext
            } = **lhandle {
                if *llen + *rlen <= Self::LEAF_UNIT_COUNT {
                    // build new tree
                    let mut newlines = l_newlines.clone();
                    newlines.extend(r_newlines.iter().map(|i| i + llen));
                    return Self::concat_slo_helper(left, *rlen, Arc::new(Self::Leaf {
                        length: *llen + *rlen,
                        newlines,
                        text: ltext.graphemes(true).chain(rtext.graphemes(true)).collect(),
                    }));
                }
            }
        }

        // general case
        Arc::new(Self::Branch {
            length: left.len() + right.len(),
            depth: left.depth().max(right.depth()) + 1,
            newlines: left.newlines().iter().map(|x| *x).chain(
                right.newlines().iter().map(|i| i + left.len())
            ).collect(),
            left: Some(Arc::clone(left)),
            right: Some(Arc::clone(right)),
        })
    }

    fn concat_slo_helper(root: &Arc<Self>, lmod: usize, leaf: Arc<Self>) -> Arc<Self> {
        match **root {
            Self::Branch {
                length,
                depth,
                ref newlines,
                ref left,
                right: Some(ref r)
            } => {
                let new_right = Self::concat_slo_helper(r, lmod, leaf);
                let newlines = newlines.iter().map(|x| *x).chain(
                    new_right.newlines().iter()
                        .map(|i| i + left.as_ref().map(|l| l.len()).unwrap_or(0))
                ).collect();
                Arc::new(Self::Branch { 
                    length: length + lmod,
                    depth: depth,
                    newlines,
                    left: left.clone(),
                    right: Some(new_right),
                })
            },
            _ => leaf,
        }
    }

    pub fn split(&self, i: usize) -> (Arc<Self>, Arc<Self>) {
        todo!();
    }

    pub fn balance(&self) -> Arc<Self> {
        todo!();
    }

    pub fn is_balanced(&self) -> bool {
        let depth = self.depth() as i64;
        let expected_depth = (self.len() as f64).log2() as i64;
        let diff = depth - expected_depth;
        diff <= 2 && diff >= 0
    }

    pub fn range<'a, B>(&'a self, range: B) -> RopeIterator<'a>
    where
        B: RangeBounds<usize>,
    {
        use Bound::*;
        let lo = match range.start_bound() {
            Included(i) => *i,
            Excluded(i) => *i + 1, // can ranges even give this as a start bound??
            Unbounded => 0,
        };
        let hi = match range.end_bound() {
            Included(i) => *i + 1,
            Excluded(i) => *i,
            Unbounded => usize::MAX,
        };
        let mut queue = VecDeque::new();
        self.range_helper(&mut queue, lo, hi);
        queue.shrink_to_fit();
        RopeIterator(queue)
    }

    fn range_helper<'a, 'b>(&'a self, queue: &'b mut VecDeque<&'a str>, lo: usize, hi: usize) {
        let weight = self.weight();
        match *self {
            Rope::Branch { ref left, ref right, .. } => {
                if let Some(ref l) = left {
                    if lo < weight {
                        l.range_helper(queue, lo, hi);
                    }
                }
                if let Some(ref r) = right {
                    if weight < hi {
                        // subtract but don't go below zero
                        let rh_lo = lo.checked_sub(weight).unwrap_or(0);
                        let rh_hi = hi.checked_sub(weight).unwrap_or(0);
                        r.range_helper(queue, rh_lo, rh_hi);
                    }
                }
            },
            Rope::Leaf { ref text, .. } => text.graphemes(true)
                .take(hi)
                .skip(lo)
                .for_each(|g| queue.push_back(g)),
        }
    }

    /// Traverses a rope in order, calling the provided function on each node.
    #[allow(dead_code)] // TODO delete me
    pub fn inorder<F: Fn(&Self)>(&self, func: &F) {
        match self {
            Self::Branch { left, right, .. } => {
                if let Some(l) = left { l.inorder(func); }
                func(self);
                if let Some(r) = right { r.inorder(func); }
            },
            Self::Leaf { .. } => { func(self); },
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Self::Leaf { .. } => 0,
            Self::Branch { depth, .. } => *depth,
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Leaf { length, .. } => *length,
            Self::Branch { length, .. } => *length,
        }
    }

    pub fn newlines(&self) -> &Vec<usize> {
        match self {
            Self::Leaf { ref newlines, .. } => newlines,
            Self::Branch { ref newlines, .. } => newlines,
        }
    }

    pub fn weight(&self) -> usize {
        match self {
            Self::Leaf { length, .. } => *length,
            Self::Branch { left: Some(rope), .. } => rope.len(),
            Self::Branch { left: None, .. } => 0,
        }
    }
}

pub struct RopeIterator<'a>(VecDeque<&'a str>);

impl<'a> Iterator for RopeIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.pop_front()
    }
}

#[test]
fn builds_without_crashing() {
    let corpus = "abcdefghijklmnopqrstuvwxyz ".chars().cycle().take(100).collect::<String>();
    let _ = Rope::from_str(&corpus);
}

#[test]
fn iterates_full_range() {
    let corpus = "abcdefghijklmnopqrstuvwxyz ".chars().cycle().take(100).collect::<String>();
    let root = Rope::from_str(&corpus);
    let mut corpus2 = String::new();
    root.range(..).for_each(|ch| corpus2.push_str(ch));
    assert_eq!(corpus, corpus2);
}

#[test]
fn iterates_partial_range() {
    let (lo, hi) = (38, 77);
    let corpus = "abcdefghijklmnopqrstuvwxyz ".chars().cycle();
    let input = corpus.clone().take(100).collect::<String>();
    let expected_output = corpus.take(hi).skip(lo).collect::<String>();
    let root = Rope::from_str(&input);
    let mut output = String::new();
    root.range(lo..hi).for_each(|ch| output.push_str(ch));
    println!("Incorrect output: {}", output);
    println!(" Expected output: {}", expected_output);
    assert_eq!(output, expected_output);
}

#[test]
fn newlines_are_correct() {
    let text = "abcdefghijklmnopqrstuvwxyz\n";
    let corpus = text.chars().cycle().take(text.len() * 100).collect::<String>();
    let root = Rope::from_str(&corpus);
    let expected = (1..=100).map(|i| i * text.len() - 1).collect::<Vec<usize>>();
    let found = root.newlines();

    println!("expected: {:?}", expected);
    println!("   found: {:?}", *found);
    assert_eq!(expected, *found);
}

#[test]
fn concat_short_leaf_optimization_flat() {
    use std::mem::discriminant;

    let flat_left = Arc::new(Rope::Leaf {
        length: "a".graphemes(true).count(),
        newlines: Vec::new(),
        text: "a".to_owned(),
    });
    let flat_right = Arc::new(Rope::Leaf {
        length: "b".graphemes(true).count(),
        newlines: Vec::new(),
        text: "b".to_owned(),
    });

    let short_cat = Rope::concat(&flat_left, &flat_right);
    assert_eq!(short_cat.range(..).collect::<String>(), "ab");
    assert_eq!(
        discriminant(&*short_cat),
        discriminant(&Rope::default()),
    );
}

#[test]
fn concat_short_leaf_optimization_tall() {
    use std::mem::discriminant;

    let tall_left = Arc::new(Rope::Branch {
        length: "a".graphemes(true).count(),
        depth: 2,
        newlines: Vec::new(),
        left: None,
        right: Some(Arc::new(Rope::Branch {
            length: "a".graphemes(true).count(),
            depth: 1,
            newlines: Vec::new(),
            left: None,
            right: Some(Arc::new(Rope::Leaf {
                length: "a".graphemes(true).count(),
                newlines: Vec::new(),
                text: "a".to_owned(),
            })),
        })),
    });
    let flat_right = Arc::new(Rope::Leaf {
        length: "b".graphemes(true).count(),
        newlines: Vec::new(),
        text: "b".to_owned(),
    });

    let tall_cat = Rope::concat(&tall_left, &flat_right);
    assert_eq!(tall_cat.range(..).collect::<String>(), "ab");
    assert_eq!(
        discriminant(&*tall_cat),
        discriminant(&Rope::Branch {
            length: 0,
            depth: 1,
            newlines: Vec::new(),
            left: None,
            right: None
        }),
    );
}
