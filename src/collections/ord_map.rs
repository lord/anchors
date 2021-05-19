use crate::expert::{Anchor, Engine, MultiAnchor};
use im::ordmap::DiffItem;
use im::OrdMap;

pub type Dict<K, V> = OrdMap<K, V>;

impl<E: Engine, K: Ord + Clone + PartialEq + 'static, V: Clone + PartialEq + 'static>
    Anchor<Dict<K, V>, E>
{
    #[track_caller]
    pub fn filter<F: FnMut(&K, &V) -> bool + 'static>(&self, mut f: F) -> Anchor<Dict<K, V>, E> {
        self.filter_map(move |k, v| if f(k, v) { Some(v.clone()) } else { None })
    }

    // TODO rlord: fix this name god

    #[track_caller]

    pub fn map_<F: FnMut(&K, &V) -> T + 'static, T: Clone + PartialEq + 'static>(
        &self,
        mut f: F,
    ) -> Anchor<Dict<K, T>, E> {
        self.filter_map(move |k, v| Some(f(k, v)))
    }

    /// FOOBAR
    #[track_caller]
    pub fn filter_map<F: FnMut(&K, &V) -> Option<T> + 'static, T: Clone + PartialEq + 'static>(
        &self,
        mut f: F,
    ) -> Anchor<Dict<K, T>, E> {
        self.unordered_fold(Dict::new(), move |out, diff_item| {
            match diff_item {
                DiffItem::Add(k, v) => {
                    if let Some(new) = f(k, v) {
                        out.insert(k.clone(), new);
                        return true;
                    }
                }
                DiffItem::Update {
                    new: (k, v),
                    old: _,
                } => {
                    if let Some(new) = f(k, v) {
                        out.insert(k.clone(), new);
                        return true;
                    } else if out.contains_key(k) {
                        out.remove(k);
                        return true;
                    }
                }
                DiffItem::Remove(k, _v) => {
                    out.remove(k);
                    return true;
                }
            }
            false
        })
    }
    #[track_caller]
    pub fn unordered_fold<
        T: PartialEq + Clone + 'static,
        F: for<'a> FnMut(&mut T, DiffItem<'a, K, V>) -> bool + 'static,
    >(
        &self,
        initial_state: T,
        mut f: F,
    ) -> Anchor<T, E> {
        let mut last_observation = Dict::new();
        self.map_mut(initial_state, move |mut out, this| {
            let mut did_update = false;
            for item in last_observation.diff(this) {
                if f(&mut out, item) {
                    did_update = true;
                }
            }
            last_observation = this.clone();
            did_update
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn test_filter() {
        let mut engine = crate::singlethread::Engine::new();
        let mut dict = Dict::new();
        let a = crate::expert::Var::new(dict.clone());
        let b = a.watch().filter(|_, n| *n > 10);
        let b_out = engine.get(&b);
        assert_eq!(0, b_out.len());

        dict.insert("a".to_string(), 1);
        dict.insert("b".to_string(), 23);
        dict.insert("c".to_string(), 5);
        dict.insert("d".to_string(), 24);
        a.set(dict.clone());
        let b_out = engine.get(&b);
        assert_eq!(2, b_out.len());
        assert_eq!(Some(&23), b_out.get("b"));
        assert_eq!(Some(&24), b_out.get("d"));

        dict.insert("a".to_string(), 25);
        dict.insert("b".to_string(), 5);
        dict.remove("d");
        dict.insert("e".to_string(), 50);
        a.set(dict.clone());
        let b_out = engine.get(&b);
        assert_eq!(2, b_out.len());
        assert_eq!(Some(&25), b_out.get("a"));
        assert_eq!(Some(&50), b_out.get("e"));
    }

    #[test]
    fn test_map() {
        let mut engine = crate::singlethread::Engine::new();
        let mut dict = Dict::new();
        let a = crate::expert::Var::new(dict.clone());
        let b = a.watch().map_(|_, n| *n + 1);
        let b_out = engine.get(&b);
        assert_eq!(0, b_out.len());

        dict.insert("a".to_string(), 1);
        dict.insert("b".to_string(), 2);
        dict.insert("c".to_string(), 3);
        dict.insert("d".to_string(), 4);
        a.set(dict.clone());
        let b_out = engine.get(&b);
        assert_eq!(4, b_out.len());
        assert_eq!(Some(&2), b_out.get("a"));
        assert_eq!(Some(&3), b_out.get("b"));
        assert_eq!(Some(&4), b_out.get("c"));
        assert_eq!(Some(&5), b_out.get("d"));

        dict.insert("a".to_string(), 10);
        dict.insert("b".to_string(), 11);
        dict.remove("d");
        dict.insert("e".to_string(), 12);
        a.set(dict.clone());
        let b_out = engine.get(&b);
        assert_eq!(4, b_out.len());
        assert_eq!(Some(&11), b_out.get("a"));
        assert_eq!(Some(&12), b_out.get("b"));
        assert_eq!(Some(&4), b_out.get("c"));
        assert_eq!(Some(&13), b_out.get("e"));
    }
}
