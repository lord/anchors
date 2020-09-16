use std::marker::PhantomData;
use std::panic::Location;
use std::task::Poll;

mod ext;
pub use ext::AnchorExt;
mod constant;
mod fakeheap;
mod graph;
mod refcounter;
pub mod singlethread;
mod var;
pub use constant::Constant;
pub use var::{Var, VarSetter};

pub struct Anchor<O, E: Engine + ?Sized> {
    pub data: E::AnchorData,
    phantom: PhantomData<O>,
}

impl<O, E: Engine> Anchor<O, E> {
    fn new(data: E::AnchorData) -> Self {
        Self {
            data,
            phantom: PhantomData,
        }
    }
}

impl<O, E: Engine> Clone for Anchor<O, E> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            phantom: PhantomData,
        }
    }
}

impl<O, E: Engine> PartialEq for Anchor<O, E> {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}
impl<O, E: Engine> Eq for Anchor<O, E> {}

pub trait AnchorData: Sized + Clone + PartialEq + Eq + std::hash::Hash {}

pub trait Engine: 'static {
    type AnchorData: AnchorData;
    type DirtyHandle: DirtyHandle;

    fn mount<I: AnchorInner<Self> + 'static>(inner: I) -> Anchor<I::Output, Self>;
}

pub trait DirtyHandle {
    fn mark_dirty(&self);
}

pub trait OutputContext<'eng> {
    type Engine: Engine + ?Sized;

    fn get<'out, O: 'static>(&self, anchor: &Anchor<O, Self::Engine>) -> &'out O
    where
        'eng: 'out;
}

pub trait UpdateContext {
    type Engine: Engine + ?Sized;

    fn get<'out, 'slf, O: 'static>(&'slf self, anchor: &Anchor<O, Self::Engine>) -> &'out O
    where
        'slf: 'out;
    fn request<'out, O: 'static>(
        &mut self,
        anchor: &Anchor<O, Self::Engine>,
        necessary: bool,
    ) -> Poll<bool>;
    fn dirty_handle(&mut self) -> <Self::Engine as Engine>::DirtyHandle;
}

pub trait AnchorInner<E: Engine + ?Sized> {
    type Output: 'static;
    fn dirty(&mut self, child: &E::AnchorData);
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll<bool>;
    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out;

    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        None
    }
}

#[cfg(test)]
mod test {
    use crate::ext::{AnchorExt, AnchorSplit};
    #[test]
    fn test_cutoff_simple() {
        let mut engine = crate::singlethread::Engine::new();
        let (v, v_setter) = crate::var::Var::new(100i32);
        let mut old_val = 0i32;
        let post_cutoff = v.cutoff(move |new_val| {
            if (old_val-*new_val).abs() < 50 {
                println!("old!");
                false
            } else {
                println!("new!");
                old_val = *new_val;
                true
            }
        }).map(|v| {
            println!("recalc map");
            *v + 10
        });
        engine.mark_observed(&post_cutoff);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(101);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(200);
        assert_eq!(engine.get(&post_cutoff), 210);
    }

    #[test]
    fn test_refmap_simple() {
        #[derive(PartialEq, Debug)]
        struct NoClone(usize);

        let mut engine = crate::singlethread::Engine::new();
        let (v, _) = crate::var::Var::new((NoClone(1), NoClone(2)));
        let a = v.refmap(|(a, _)| a);
        let b = v.refmap(|(_, b)| b);
        let a_correct = a.map(|a| a == &NoClone(1));
        let b_correct = b.map(|b| b == &NoClone(2));
        assert!(engine.get(&a_correct));
        assert!(engine.get(&b_correct));
    }

    #[test]
    fn test_split_simple() {
        let mut engine = crate::singlethread::Engine::new();
        let (v, _) = crate::var::Var::new((1usize, 2usize, 3usize));
        let (a, b, c) = v.split();
        assert_eq!(engine.get(&a), 1);
        assert_eq!(engine.get(&b), 2);
        assert_eq!(engine.get(&c), 3);
    }

    #[test]
    fn test_map_simple() {
        let mut engine = crate::singlethread::Engine::new();
        let (v1, _v1_setter) = crate::var::Var::new(1usize);
        let (v2, _v2_setter) = crate::var::Var::new(123usize);
        let _a2 = v1.map(|num1| {
            println!("a: adding to {:?}", num1);
            *num1
        });
        let a = AnchorExt::map((&v1, &v2), |num1, num2| num1 + num2);

        let b = AnchorExt::map((&v1, &a, &v2), |num1, num2, num3| num1 + num2 + num3);
        engine.mark_observed(&b);
        engine.stabilize();
        assert_eq!(engine.get(&b), 248);
    }

    #[test]
    fn test_then_simple() {
        let mut engine = crate::singlethread::Engine::new();
        let (v1, v1_setter) = crate::var::Var::new(true);
        let (v2, _v2_setter) = crate::var::Var::new(10usize);
        let (v3, _v3_setter) = crate::var::Var::new(20usize);
        let a = v1.then(move |val| if *val { v2.clone() } else { v3.clone() });
        engine.mark_observed(&a);
        engine.stabilize();
        assert_eq!(engine.get(&a), 10);

        v1_setter.set(false);
        engine.stabilize();
        assert_eq!(engine.get(&a), 20);
    }

    #[test]
    fn test_garbage_collection_wont_panic() {
        let mut engine = crate::singlethread::Engine::new();
        let (v1, _v1_setter) = crate::var::Var::new(1usize);
        engine.get(&v1);
        std::mem::drop(v1);
        engine.stabilize();
    }

    #[test]
    fn test_readme_example() {
        // example
        use crate::{singlethread::Engine, AnchorExt, Var};
        let mut engine = Engine::new();

        // create a couple `Var`s
        let (my_name, my_name_updater) = Var::new("Bob".to_string());
        let (my_unread, my_unread_updater) = Var::new(999usize);

        // `my_name` is a `Var`, our first type of `Anchor`. we can pull an `Anchor`'s value out with our `engine`:
        assert_eq!(&engine.get(&my_name), "Bob");
        assert_eq!(engine.get(&my_unread), 999);

        // we can create a new `Anchor` from another one using `map`. The function won't actually run until absolutely necessary.
        // also feel free to clone an `Anchor` — the clones will all refer to the same inner state
        let my_greeting = my_name.clone().map(|name| {
            println!("calculating name!");
            format!("Hello, {}!", name)
        });
        assert_eq!(engine.get(&my_greeting), "Hello, Bob!"); // prints "calculating name!"

        // we can update a `Var` with its updater. values are cached unless one of its dependencies changes
        assert_eq!(engine.get(&my_greeting), "Hello, Bob!"); // doesn't print anything
        my_name_updater.set("Robo".to_string());
        assert_eq!(engine.get(&my_greeting), "Hello, Robo!"); // prints "calculating name!"

        // a `map` can take values from multiple `Anchor`s. just use tuples:
        let header = (&my_greeting, &my_unread)
            .map(|greeting, unread| format!("{} You have {} new messages.", greeting, unread));
        assert_eq!(
            engine.get(&header),
            "Hello, Robo! You have 999 new messages."
        );

        // just like a future, you can dynamically decide which `Anchor` to use with `then`:
        let (insulting_name, _) = Var::new("Lazybum".to_string());
        let dynamic_name = my_unread.then(move |unread| {
            // only use the user's real name if the have less than 100 messages in their inbox
            if *unread < 100 {
                my_name.clone()
            } else {
                insulting_name.clone()
            }
        });
        assert_eq!(engine.get(&dynamic_name), "Lazybum");
        my_unread_updater.set(50);
        assert_eq!(engine.get(&dynamic_name), "Robo");
    }
}
