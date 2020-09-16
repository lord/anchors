use slotmap::{SecondaryMap, Key};

pub struct FakeHeap<T: Key> {
    min_height: usize,
    lists: Vec<Vec<T>>,
    contained_keys: SecondaryMap<T, usize>,
}

impl<T: Key> FakeHeap<T> {
    pub fn new(max_height: usize) -> Self {
        let mut lists = Vec::with_capacity(max_height);
        for _ in 0..max_height {
            lists.push(vec![])
        }
        Self {
            lists,
            min_height: max_height,
            contained_keys: SecondaryMap::new(),
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
        if let Some(count) = self.contained_keys.get_mut(item.clone()) {
            *count += 1;
        } else {
            self.contained_keys.insert(item.clone(), 1);
        }
        self.lists[height].push(item);
        self.min_height = self.min_height.min(height);
    }

    pub fn pop_min(&mut self) -> Option<(usize, T)> {
        while self.min_height < self.lists.len() {
            if let Some(v) = self.lists[self.min_height].pop() {
                let old_count = self.contained_keys.get_mut(v.clone()).unwrap();
                *old_count = *old_count - 1;
                return Some((self.min_height, v));
            } else {
                self.min_height += 1;
            }
        }
        None
    }

    pub fn contains(&self, item: T) -> bool {
        if let Some(count) = self.contained_keys.get(item) {
            *count > 0
        } else {
            false
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use slotmap::{SlotMap, DefaultKey};

    #[test]
    fn test_insert_pop_and_contains() {
        let (a, b, c, d, e) = {
            let mut map = SlotMap::new();
            (map.insert(()), map.insert(()), map.insert(()), map.insert(()), map.insert(()))
        };
        let mut heap: FakeHeap<DefaultKey> = FakeHeap::new(10);

        assert_eq!(false, heap.contains(a));
        assert_eq!(false, heap.contains(b));
        assert_eq!(false, heap.contains(c));
        assert_eq!(false, heap.contains(d));
        assert_eq!(false, heap.contains(e));

        heap.insert(0, a);
        heap.insert(0, a);
        heap.insert(0, a);
        heap.insert(5, b);
        heap.insert(3, c);
        heap.insert(4, d);

        assert_eq!(true, heap.contains(a));
        assert_eq!(true, heap.contains(b));
        assert_eq!(true, heap.contains(c));
        assert_eq!(true, heap.contains(d));
        assert_eq!(false, heap.contains(e));

        assert_eq!(Some(a), heap.pop_min().map(|(_, v)| v));
        assert_eq!(true, heap.contains(a));
        assert_eq!(Some(a), heap.pop_min().map(|(_, v)| v));
        assert_eq!(true, heap.contains(a));
        assert_eq!(Some(a), heap.pop_min().map(|(_, v)| v));
        assert_eq!(false, heap.contains(a));
        assert_eq!(Some(c), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(d), heap.pop_min().map(|(_, v)| v));

        assert_eq!(false, heap.contains(a));
        assert_eq!(true, heap.contains(b));
        assert_eq!(false, heap.contains(c));
        assert_eq!(false, heap.contains(d));
        assert_eq!(false, heap.contains(e));

        heap.insert(1, e);
        assert_eq!(true, heap.contains(e));

        assert_eq!(Some(e), heap.pop_min().map(|(_, v)| v));
        assert_eq!(Some(b), heap.pop_min().map(|(_, v)| v));

        assert_eq!(false, heap.contains(a));
        assert_eq!(false, heap.contains(b));
        assert_eq!(false, heap.contains(c));
        assert_eq!(false, heap.contains(d));
        assert_eq!(false, heap.contains(e));

        assert_eq!(None, heap.pop_min().map(|(_, v)| v));
    }

    #[test]
    #[should_panic]
    fn test_insert_above_max_height() {
        let mut heap: FakeHeap<DefaultKey> = FakeHeap::new(10);
        heap.insert(10, DefaultKey::null());
    }
}
