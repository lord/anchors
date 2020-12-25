#![feature(negative_impls)]

use std::marker::PhantomData;
use std::panic::Location;

mod ext;
pub use ext::AnchorExt;
mod constant;
pub mod singlethread;
mod var;
pub use constant::Constant;
pub use var::{Var, VarSetter};
// mod dict;

/// Indicates whether a value is ready for reading, and if it is, whether it's changed
/// since the last read.
#[derive(Debug, PartialEq, Eq)]
pub enum Poll {
    /// Indicates the polled value is ready for reading. Either this is the first read,
    /// or the value has changed since the last read.
    Updated,

    /// Indicates the polled value is ready for reading. This is not the first read, and
    /// the value is unchanged since the previous read.
    Unchanged,

    /// Indicates the polled value is not ready for reading, but has been queued for recalculation.
    /// The output value will eventually switch to Updated or Unchanged.
    Pending,
}

/// The main struct of the Anchors library. Represents a single value on the recomputation graph.
///
/// This doesn't contain the particular Anchor implementation directly, but instead contains an
/// engine-specific `AnchorHandle` which allows the recalculation engine to identify which
/// internal recomputation graph node this corresponds to. You should rarely create Anchors yourself;
/// instead use one of the built-in functions like `Var::new` to create one, or create derivative Anchors
/// with one of the `AnchorExt` methods.
pub struct Anchor<I, E: Engine + ?Sized> {
    data: E::AnchorHandle,
    phantom: PhantomData<I>,
}

impl<I, E: Engine + ?Sized> Anchor<I, E> {
    fn new(data: E::AnchorHandle) -> Self {
        Self {
            data,
            phantom: PhantomData,
        }
    }

    /// Returns the immutable, copyable, hashable, comparable engine-specific ID for this Anchor.
    pub fn token(&self) -> <E::AnchorHandle as AnchorHandle>::Token {
        self.data.token()
    }

    pub fn into_handle(self) -> E::AnchorHandle {
        self.data
    }

    pub fn handle(&self) -> &E::AnchorHandle {
        &self.data
    }
}

impl<I, E: Engine> Clone for Anchor<I, E> {
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            phantom: PhantomData,
        }
    }
}

impl<I, E: Engine> PartialEq for Anchor<I, E> {
    fn eq(&self, other: &Self) -> bool {
        self.token() == other.token()
    }
}
impl<I, E: Engine> Eq for Anchor<I, E> {}

/// A reference to a particular `AnchorInner`. Each engine implements its own.
pub trait AnchorHandle: Sized + Clone {
    type Token: Sized + Clone + Copy + PartialEq + Eq + std::hash::Hash;

    /// Returns a Copyable, comparable, hashable ID corresponding to this AnchorHandle.
    /// Some engines may garbage collect an AnchorInner when no more AnchorHandles pointing
    /// to it exist, which means it's possible to have a Token pointing to a since-deleted
    /// Anchor.
    fn token(&self) -> Self::Token;
}

/// The core engine trait implemented by each recalculation engine. Allows mounting an `AnchorInner`
/// into an actual `Anchor`, although this mounting should usually be done by each `AnchorInner`
/// implementation directly.
pub trait Engine: 'static {
    type AnchorHandle: AnchorHandle;
    type DirtyHandle: DirtyHandle;

    fn mount<I: AnchorInner<Self> + 'static>(inner: I) -> Anchor<I, Self>;
}

/// Allows a node with non-Anchors inputs to manually mark itself as dirty. Each engine implements its own.
pub trait DirtyHandle {
    /// Indicates that the Anchor associated with this `DirtyHandle` may have a changed its output, and should
    /// be repolled.
    fn mark_dirty(&self);
}

/// The context passed to an `AnchorInner` when its `output` method is called.
pub trait OutputContext<'eng> {
    type Engine: Engine + ?Sized;

    /// If another Anchor during polling indicated its value was ready, the previously
    /// calculated value can be accessed with this method. Its implementation is virtually
    /// identical to `UpdateContext`'s `get`. This is mostly used by AnchorInner implementations
    /// that want to return a reference to some other Anchor's output without cloning.
    fn get<'out, I: AnchorInner<Self::Engine> + 'static>(
        &self,
        anchor: &Anchor<I, Self::Engine>,
    ) -> &'out I::Output
    where
        'eng: 'out,
    {
        let output: &I::Output = unsafe { self.get_untyped(anchor.handle()) }
            .downcast_ref()
            .unwrap();
        output
    }

    unsafe fn get_untyped<'out>(
        &self,
        anchor_handle: &<Self::Engine as Engine>::AnchorHandle,
    ) -> &'out dyn std::any::Any
    where
        'eng: 'out;
}

/// The context passed to an `AnchorInner` when its `poll_updated` method is called.
pub trait UpdateContext {
    type Engine: Engine + ?Sized;

    /// If `request` indicates another Anchor's value is ready, the previously
    /// calculated value can be accessed with this method.
    fn get<'out, 'slf, I: AnchorInner<Self::Engine> + 'static>(
        &'slf self,
        anchor: &Anchor<I, Self::Engine>,
    ) -> &'out I::Output
    where
        'slf: 'out,
    {
        let output: &I::Output = unsafe { self.get_untyped(anchor.handle()) }
            .downcast_ref()
            .unwrap();
        output
    }

    unsafe fn get_untyped<'out, 'slf>(
        &'slf self,
        anchor_handle: &<Self::Engine as Engine>::AnchorHandle,
    ) -> &'out dyn std::any::Any
    where
        'slf: 'out;

    /// If `anchor`'s output is ready, indicates whether the output has changed since this `AnchorInner`
    /// last called `request` on it. If `anchor`'s output is not ready, it is queued for recalculation and
    /// this returns Poll::Pending.
    ///
    /// `necessary` is a bit that indicates if we are necessary, `anchor` should be marked as necessary
    /// as well. If you don't know what this bit should be set to, you probably want a value of `true`.
    fn request<'out>(
        &mut self,
        anchor_handle: &<Self::Engine as Engine>::AnchorHandle,
        necessary: bool,
    ) -> Poll;

    /// If `anchor` was previously passed to `request` and you no longer care about its output, you can
    /// pass it to `unrequest` so the engine will stop calling your `dirty` method when `anchor` changes.
    /// If `self` is necessary, this is also critical for ensuring `anchor` is no longer marked as necessary.
    fn unrequest<'out>(&mut self, anchor_handle: &<Self::Engine as Engine>::AnchorHandle);

    /// Returns a new dirty handle, used for marking that `self`'s output may have changed through some
    /// non incremental means. For instance, perhaps this `AnchorInner`s value represents the current time, or
    /// it's a `Var` that has a setter function.
    fn dirty_handle(&mut self) -> <Self::Engine as Engine>::DirtyHandle;
}

/// The engine-agnostic implementation of each type of Anchor. You likely don't need to implement your own
/// `AnchorInner`; instead use one of the built-in implementations.
pub trait AnchorInner<E: Engine + ?Sized> {
    type Output;

    /// Called by the engine to indicate some input may have changed.
    /// If this `AnchorInner` still cares about `child`'s value, it should re-request
    /// it next time `poll_updated` is called.
    fn dirty(&mut self, child: &<E::AnchorHandle as AnchorHandle>::Token);

    /// Called by the engine when it wants to know if this value has changed or
    /// not. If some requested value from `ctx` is `Pending`, this method should
    /// return `Poll::Pending`; otherwise it must finish recalculation and report
    /// either `Poll::Updated` or `Poll::Unchanged`.
    fn poll_updated<G: UpdateContext<Engine = E>>(&mut self, ctx: &mut G) -> Poll;

    /// Called by the engine to get the current output value of this `AnchorInner`. This
    /// is *only* called after this `AnchorInner` reported in the return value from
    /// `poll_updated` the value was ready. If `dirty` is called, this function will not
    /// be called until `poll_updated` returns a non-Pending value.
    fn output<'slf, 'out, G: OutputContext<'out, Engine = E>>(
        &'slf self,
        ctx: &mut G,
    ) -> &'out Self::Output
    where
        'slf: 'out;

    /// An optional function to report the track_caller-derived callsite where
    /// this Anchor was created. Useful for debugging purposes.
    fn debug_location(&self) -> Option<(&'static str, &'static Location<'static>)> {
        None
    }
}

#[cfg(test)]
mod test {
    use crate::ext::{AnchorExt, AnchorSplit};
    #[test]
    fn test_cutoff_simple_observed() {
        let mut engine = crate::singlethread::Engine::new();
        let (v, v_setter) = crate::var::Var::new(100i32);
        let mut old_val = 0i32;
        let post_cutoff = v
            .cutoff(move |new_val| {
                if (old_val - *new_val).abs() < 50 {
                    false
                } else {
                    old_val = *new_val;
                    true
                }
            })
            .map(|v| *v + 10);
        engine.mark_observed(&post_cutoff);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(125);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(151);
        assert_eq!(engine.get(&post_cutoff), 161);
        v_setter.set(125);
        assert_eq!(engine.get(&post_cutoff), 161);
    }

    #[test]
    fn test_cutoff_simple_unobserved() {
        let mut engine = crate::singlethread::Engine::new();
        let (v, v_setter) = crate::var::Var::new(100i32);
        let mut old_val = 0i32;
        let post_cutoff = v
            .cutoff(move |new_val| {
                if (old_val - *new_val).abs() < 50 {
                    false
                } else {
                    old_val = *new_val;
                    true
                }
            })
            .map(|v| *v + 10);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(125);
        assert_eq!(engine.get(&post_cutoff), 110);
        v_setter.set(151);
        assert_eq!(engine.get(&post_cutoff), 161);
        v_setter.set(125);
        assert_eq!(engine.get(&post_cutoff), 161);
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

    // #[test]
    // fn test_split_simple() {
    //     let mut engine = crate::singlethread::Engine::new();
    //     let (v, _) = crate::var::Var::new((1usize, 2usize, 3usize));
    //     let (a, b, c) = v.split();
    //     assert_eq!(engine.get(&a), 1);
    //     assert_eq!(engine.get(&b), 2);
    //     assert_eq!(engine.get(&c), 3);
    // }

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
    fn test_observed_marking() {
        use crate::singlethread::ObservedState;

        let mut engine = crate::singlethread::Engine::new();
        let (v1, _v1_setter) = crate::var::Var::new(1usize);
        let a = v1.map(|num1| *num1 + 1);
        let b = a.map(|num1| *num1 + 2);
        let c = b.map(|num1| *num1 + 3);
        engine.mark_observed(&a);
        engine.mark_observed(&c);

        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&v1));
        assert_eq!(ObservedState::Observed, engine.check_observed(&a));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&b));
        assert_eq!(ObservedState::Observed, engine.check_observed(&c));

        engine.stabilize();

        assert_eq!(ObservedState::Necessary, engine.check_observed(&v1));
        assert_eq!(ObservedState::Observed, engine.check_observed(&a));
        assert_eq!(ObservedState::Necessary, engine.check_observed(&b));
        assert_eq!(ObservedState::Observed, engine.check_observed(&c));

        engine.mark_unobserved(&c);

        assert_eq!(ObservedState::Necessary, engine.check_observed(&v1));
        assert_eq!(ObservedState::Observed, engine.check_observed(&a));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&b));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&c));

        engine.mark_unobserved(&a);

        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&v1));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&a));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&b));
        assert_eq!(ObservedState::Unnecessary, engine.check_observed(&c));
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
