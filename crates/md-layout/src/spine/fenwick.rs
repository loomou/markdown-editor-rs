use md_core::Px;

const CHUNK_CAPACITY: usize = 256;

struct Chunk {
    tree: Vec<Px>,
}

impl Chunk {
    fn build(len: usize, height: impl Fn(usize) -> Px) -> Self {
        let mut tree = vec![0.0; len + 1];
        for i in 0..len {
            let index = i + 1;
            tree[index] += height(i);
            let parent = index + (index & index.wrapping_neg());
            if parent < tree.len() {
                tree[parent] += tree[index];
            }
        }
        Chunk { tree }
    }

    fn len(&self) -> usize {
        self.tree.len() - 1
    }

    fn add(&mut self, mut index: usize, delta: Px) {
        index += 1;
        while index < self.tree.len() {
            self.tree[index] += delta;
            index += index & index.wrapping_neg();
        }
    }

    fn prefix(&self, mut count: usize) -> Px {
        let mut sum = 0.0;
        while count > 0 {
            sum += self.tree[count];
            count -= count & count.wrapping_neg();
        }
        sum
    }

    fn total(&self) -> Px {
        self.prefix(self.len())
    }
}

pub(super) struct Fenwick {
    chunks: Vec<Chunk>,
    ends: Vec<usize>,
    totals: Vec<Px>,
}

impl Fenwick {
    pub(super) fn empty() -> Self {
        let mut index = Fenwick {
            chunks: Vec::new(),
            ends: Vec::new(),
            totals: Vec::new(),
        };
        index.rebuild_outer();
        index
    }

    pub(super) fn from_heights(heights: &[Px]) -> Self {
        crate::hot_path::add_height_index_build();
        let mut index = Fenwick::empty();
        index.chunks = Self::build_chunks(0, heights.len(), &|i| heights[i]);
        index.rebuild_outer();
        index
    }

    fn len(&self) -> usize {
        self.ends.last().copied().unwrap_or(0)
    }

    fn rebuild_outer(&mut self) {
        self.ends.clear();
        self.ends.reserve(self.chunks.len());
        let mut end = 0;
        for chunk in &self.chunks {
            end += chunk.len();
            self.ends.push(end);
        }
        self.totals = vec![0.0; self.chunks.len() + 1];
        for i in 0..self.chunks.len() {
            let index = i + 1;
            self.totals[index] += self.chunks[i].total();
            let parent = index + (index & index.wrapping_neg());
            if parent < self.totals.len() {
                self.totals[parent] += self.totals[index];
            }
        }
    }

    fn outer_add(&mut self, mut index: usize, delta: Px) {
        index += 1;
        while index < self.totals.len() {
            self.totals[index] += delta;
            index += index & index.wrapping_neg();
        }
    }

    fn outer_prefix(&self, mut count: usize) -> Px {
        let mut sum = 0.0;
        while count > 0 {
            sum += self.totals[count];
            count -= count & count.wrapping_neg();
        }
        sum
    }

    fn chunk_start(&self, chunk: usize) -> usize {
        if chunk == 0 { 0 } else { self.ends[chunk - 1] }
    }

    pub(super) fn add(&mut self, index: usize, delta: Px) {
        let chunk = self.ends.partition_point(|&end| end <= index);
        if chunk >= self.chunks.len() {
            return;
        }
        let local = index - self.chunk_start(chunk);
        self.chunks[chunk].add(local, delta);
        self.outer_add(chunk, delta);
    }

    pub(super) fn prefix(&self, count: usize) -> Px {
        let count = count.min(self.len());
        if count == 0 {
            return 0.0;
        }
        let chunk = self.ends.partition_point(|&end| end < count);
        let start = self.chunk_start(chunk);
        self.outer_prefix(chunk) + self.chunks[chunk].prefix(count - start)
    }

    pub(super) fn total(&self) -> Px {
        self.outer_prefix(self.chunks.len())
    }

    pub(super) fn splice(
        &mut self,
        at: usize,
        delete: usize,
        insert: usize,
        height: &dyn Fn(usize) -> Px,
    ) {
        let len = self.len();
        assert!(at + delete <= len);
        if self.chunks.is_empty() {
            self.chunks = Self::build_chunks(0, insert, height);
            self.rebuild_outer();
            return;
        }
        let first = if at == len {
            self.chunks.len() - 1
        } else {
            self.ends.partition_point(|&end| end <= at)
        };
        let mut last = if delete == 0 {
            first
        } else {
            self.ends
                .partition_point(|&end| end < at + delete)
                .min(self.chunks.len() - 1)
        };
        let base = self.chunk_start(first);
        let window = |last: usize, ends: &[usize]| ends[last] - base - delete + insert;
        while window(last, &self.ends) < CHUNK_CAPACITY / 2 && last + 1 < self.chunks.len() {
            last += 1;
        }
        let touched = window(last, &self.ends);
        self.chunks.drain(first..=last);
        let replacements = Self::build_chunks(base, touched, height);
        self.chunks.splice(first..first, replacements);
        self.rebuild_outer();
    }

    fn build_chunks(base: usize, len: usize, height: &dyn Fn(usize) -> Px) -> Vec<Chunk> {
        if len == 0 {
            return Vec::new();
        }
        let count = len.div_ceil(CHUNK_CAPACITY);
        let mut chunks = Vec::with_capacity(count);
        let mut offset = 0;
        for i in 0..count {
            let span = (len - offset) / (count - i);
            chunks.push(Chunk::build(span, |j| height(base + offset + j)));
            offset += span;
        }
        debug_assert_eq!(offset, len);
        chunks
    }

    #[cfg(test)]
    pub(super) fn chunk_sizes(&self) -> Vec<usize> {
        self.chunks.iter().map(Chunk::len).collect()
    }

    #[cfg(test)]
    pub(super) const MIN_CHUNK: usize = CHUNK_CAPACITY / 2;
}
