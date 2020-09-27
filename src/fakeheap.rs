pub struct FakeHeap<T> {
    current_min_height: usize,
    current_max_height: usize,
    lists: Vec<Vec<T>>,
}

impl<T> FakeHeap<T> {
    pub fn new(max_height: usize) -> Self {
        let mut lists = Vec::with_capacity(max_height);
        for _ in 0..max_height {
            lists.push(vec![])
        }
        Self {
            lists,
            current_min_height: max_height,
            current_max_height: 0,
        }
    }

    pub fn insert(&mut self, height: usize, item: T) {
        if height >= self.lists.len() {
            panic!(
                "attempted to insert item into FakeHeap at height {}, when max height was {}",
                height,
                self.lists.len() - 1
            );
        }
        self.lists[height].push(item);
        self.current_min_height = self.current_min_height.min(height);
        self.current_max_height = self.current_max_height.max(height);
    }

    pub fn pop_min(&mut self) -> Option<(usize, T)> {
        while self.current_min_height <= self.current_max_height {
            if let Some(v) = self.lists[self.current_min_height].pop() {
                return Some((self.current_min_height, v));
            } else {
                self.current_min_height += 1;
            }
        }
        self.current_max_height = 0;
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_insert_pop() {
        let mut heap: FakeHeap<i32> = FakeHeap::new(10);

        heap.insert(0, 1);
        heap.insert(0, 1);
        heap.insert(0, 1);
        heap.insert(5, 2);
        heap.insert(3, 3);
        heap.insert(4, 4);

        assert_eq!(Some(1), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(1), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(1), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(3), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(4), heap.pop_min().map(|(_, v)| v));

        heap.insert(1, 5);

        assert_eq!(Some(5), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(2), heap.pop_min().map(|(_, v)| v));

        assert_eq!(None, heap.pop_min().map(|(_, v)| v));
    }

    #[test]
    #[should_panic]
    fn test_insert_above_max_height() {
        let mut heap: FakeHeap<i32> = FakeHeap::new(10);
        heap.insert(10, 1);
    }
}
