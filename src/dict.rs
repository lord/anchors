use crate::{Anchor, Engine};
use im::OrdMap;
use im::ordmap::DiffItem;

pub type Dict<K, V> = OrdMap<K, V>;

impl <E: Engine, K, V> Anchor<Dict<K, V>, E> {
    fn filter<F: Fn(K, V) -> bool>(&self, f: F) -> Anchor<Dict<K, V>, E> {
        unimplemented!()
    }

    fn map<F: Fn(K, V) -> T, T>(&self, f: F) -> Anchor<Dict<K, T>, E> {
        unimplemented!()
    }

    fn filter_map<F: Fn(K, V) -> Option<T>, T>(&self, f: F) -> Anchor<Dict<K, T>, E> {
        unimplemented!()
    }

    fn unordered_fold<T, F: for<'a> Fn(&mut T, DiffItem<'a, K, V>)>(&self, initial_state: T, f: F) -> Anchor<T, E> {
        unimplemented!()
    }

    fn merge<F: Fn()>(&self, other: Anchor<Dict<K, V>, E>) -> Anchor<Dict<K, V>, E> {
        unimplemented!()
    }
}
