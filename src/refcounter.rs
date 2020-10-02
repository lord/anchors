use std::cell::RefCell;
use std::fmt::Debug;
use std::hash::Hash;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct RefCounter<T: Hash + Eq + Debug> {
    inner: Rc<RefCell<RefCounterState<T>>>,
}

#[derive(Clone, Debug)]
struct RefCounterState<T: Hash + Eq + Debug> {
    // counts: SecondaryMap<T, usize>,
    // deleted: Vec<T>,
    f: std::marker::PhantomData<T>,
}

impl<T: Hash + Eq + Debug> RefCounter<T> {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RefCell::new(RefCounterState {
                // counts: SecondaryMap::new(),
                // deleted: Vec::new(),
                f: std::marker::PhantomData,
            })),
        }
    }

    pub fn create(&self, _item: T) {
        // self.inner.borrow_mut().counts.insert(item, 1);
    }

    pub fn increment(&self, _item: T) {
        // *self
        //     .inner
        //     .borrow_mut()
        //     .counts
        //     .get_mut(item)
        //     .expect("item did not exist when incrementing") += 1;
    }

    pub fn decrement(&self, _item: T) {
        // let mut inner = self.inner.borrow_mut();
        // let count = inner
        //     .counts
        //     .get_mut(item.clone())
        //     .expect("item did not exist when decrementing");
        // *count -= 1;
        // if *count == 0 {
        //     inner.counts.remove(item.clone()).unwrap();
        //     inner.deleted.push(item);
        // }
    }

    pub fn drain<F: FnMut(T)>(&self, _f: F) {
        // while self.inner.borrow_mut().deleted.len() > 0 {
        //     let deleted: Vec<_> = self.inner.borrow_mut().deleted.drain(..).collect();
        //     deleted.into_iter().for_each(&mut f);
        // }
    }
}
