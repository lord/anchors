use super::{Anchor, AnchorInner, Engine};
use std::panic::Location;

pub mod cutoff;
pub mod map;
pub mod map_mut;
pub mod refmap;
pub mod then;

/// A trait automatically implemented for tuples of Anchors.
///
/// You'll likely want to `use` this trait in most of your programs, since it can create many
/// useful Anchors that derive their output incrementally from some other Anchors.
///
/// Methods here mirror the non-tuple implementations listed in [Anchor]; check that out if you're
/// curious what these methods do.
pub trait MultiAnchor<E: Engine>: Sized {
    type Target;

    fn map<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map::Map<Self::Target, F, Out>: AnchorInner<E, Output = Out>;

    fn map_mut<F, Out>(self, initial: Out, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map_mut::MapMut<Self::Target, F, Out>: AnchorInner<E, Output = Out>;

    fn then<F, Out>(self, f: F) -> Anchor<Out, E>
    where
        F: 'static,
        Out: 'static,
        then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output = Out>;

    fn cutoff<F, Out>(self, _f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        cutoff::Cutoff<Self::Target, F>: AnchorInner<E, Output = Out>;

    fn refmap<F, Out>(self, _f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>;
}

impl<O1, E> Anchor<O1, E>
where
    O1: 'static,
    E: Engine,
{
    /// Creates an Anchor that maps a number of incremental input values to some output value.
    /// The function `f` accepts inputs as references, and must return an owned value.
    /// `f` will always be recalled any time any input value changes.
    ///
    /// This method is mirrored by [MultiAnchor::map].
    ///
    /// ```
    /// use anchors::singlethread::*;
    /// let mut engine = Engine::new();
    /// let a = Anchor::constant(1);
    /// let b = Anchor::constant(2);
    ///
    /// // add the two numbers together; types have been added for clarity but are optional:
    /// let res: Anchor<usize> = (&a, &b).map(|a_val: &usize, b_val: &usize| -> usize {
    ///    *a_val+*b_val
    /// });
    ///
    /// assert_eq!(3, engine.get(&res));
    /// ```
    #[track_caller]
    pub fn map<F, Out>(&self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map::Map<(Anchor<O1, E>,), F, Out>: AnchorInner<E, Output = Out>,
    {
        E::mount(map::Map {
            anchors: (self.clone(),),
            f,
            output: None,
            output_stale: true,
            location: Location::caller(),
        })
    }

    #[track_caller]
    pub fn map_mut<F, Out>(&self, initial: Out, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        map_mut::MapMut<(Anchor<O1, E>,), F, Out>: AnchorInner<E, Output = Out>,
    {
        E::mount(map_mut::MapMut {
            anchors: (self.clone(),),
            f,
            output: initial,
            output_stale: true,
            location: Location::caller(),
        })
    }

    /// Creates an Anchor that maps a number of incremental input values to some output Anchor.
    /// With `then`, your computation graph can dynamically select an Anchor to recalculate based
    /// on some other incremental computation.
    /// The function `f` accepts inputs as references, and must return an owned `Anchor`.
    /// `f` will always be recalled any time any input value changes.
    ///
    /// This method is mirrored by [MultiAnchor::then].
    ///
    /// ```
    /// use anchors::singlethread::*;
    /// let mut engine = Engine::new();
    /// let decision = Anchor::constant(true);
    /// let num = Anchor::constant(1);
    ///
    /// // because of how we're using the `then` below, only one of these two
    /// // additions will actually be run
    /// let a = num.map(|num| *num + 1);
    /// let b = num.map(|num| *num + 2);
    ///
    /// // types have been added for clarity but are optional:
    /// let res: Anchor<usize> = decision.then(move |decision: &bool| {
    ///     if *decision {
    ///         a.clone()
    ///     } else {
    ///         b.clone()
    ///     }
    /// });
    ///
    /// assert_eq!(2, engine.get(&res));
    /// ```
    #[track_caller]
    pub fn then<F, Out>(&self, f: F) -> Anchor<Out, E>
    where
        F: 'static,
        Out: 'static,
        then::Then<(Anchor<O1, E>,), Out, F, E>: AnchorInner<E, Output = Out>,
    {
        E::mount(then::Then {
            anchors: (self.clone(),),
            f,
            f_anchor: None,
            location: Location::caller(),
            lhs_stale: true,
        })
    }

    /// Creates an Anchor that maps some input reference to some output reference.
    /// Performance is critical here: `f` will always be recalled any time any downstream node
    /// requests the value of this Anchor, *not* just when an input value changes.
    /// It's also critical to note that due to constraints
    /// with Rust's lifetime system, these output references can not be owned values, and must
    /// live exactly as long as the input reference.
    ///
    /// This method is mirrored by [MultiAnchor::refmap].
    ///
    /// ```
    /// use anchors::singlethread::*;
    /// struct CantClone {val: usize};
    /// let mut engine = Engine::new();
    /// let tuple = Anchor::constant((CantClone{val: 1}, CantClone{val: 2}));
    ///
    /// // lookup the first value inside the tuple; types have been added for clarity but are optional:
    /// let res: Anchor<CantClone> = tuple.refmap(|tuple: &(CantClone, CantClone)| -> &CantClone {
    ///    &tuple.0
    /// });
    ///
    /// // check if the cantclone value is correct:
    /// let is_one = res.map(|tuple: &CantClone| -> bool {
    ///    tuple.val == 1
    /// });
    ///
    /// assert_eq!(true, engine.get(&is_one));
    /// ```
    #[track_caller]
    pub fn refmap<F, Out>(&self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        refmap::RefMap<(Anchor<O1, E>,), F>: AnchorInner<E, Output = Out>,
    {
        E::mount(refmap::RefMap {
            anchors: (self.clone(),),
            f,
            location: Location::caller(),
        })
    }

    /// Creates an Anchor that outputs its input. However, even if a value changes
    /// you may not want to recompute downstream nodes unless the value changes substantially.
    /// The function `f` accepts inputs as references, and must return true if Anchors that derive
    /// values from this cutoff should recalculate, or false if derivative Anchors should not recalculate.
    /// If this is the first calculation, `f` will be called, but return values of `false` will be ignored.
    /// `f` will always be recalled any time the input value changes.
    ///
    /// This method is mirrored by [MultiAnchor::cutoff].
    ///
    /// ```
    /// use anchors::singlethread::*;
    /// let mut engine = Engine::new();
    /// let num = Var::new(1i32);
    /// let cutoff = {
    ///     let mut old_num_opt: Option<i32> = None;
    ///     num.watch().cutoff(move |num| {
    ///         if let Some(old_num) = old_num_opt {
    ///             if (old_num - *num).abs() < 10 {
    ///                 return false;
    ///             }
    ///         }
    ///         old_num_opt = Some(*num);
    ///         true
    ///     })
    /// };
    /// let res = cutoff.map(|cutoff| *cutoff + 1);
    ///
    /// assert_eq!(2, engine.get(&res));
    ///
    /// // small changes don't cause recalculations
    /// num.set(5);
    /// assert_eq!(2, engine.get(&res));
    ///
    /// // but big changes do
    /// num.set(11);
    /// assert_eq!(12, engine.get(&res));
    /// ```
    #[track_caller]
    pub fn cutoff<F, Out>(&self, f: F) -> Anchor<Out, E>
    where
        Out: 'static,
        F: 'static,
        cutoff::Cutoff<(Anchor<O1, E>,), F>: AnchorInner<E, Output = Out>,
    {
        E::mount(cutoff::Cutoff {
            anchors: (self.clone(),),
            f,
            location: Location::caller(),
        })
    }
}

macro_rules! impl_tuple_ext {
    ($([$output_type:ident, $num:tt])+) => {
        impl <$($output_type,)+ E> Anchor<($($output_type,)+), E>
        where
            $(
                $output_type: Clone + PartialEq + 'static,
            )+
            E: Engine,
        {
            pub fn split(&self) -> ($(Anchor<$output_type, E>,)+) {
                ($(
                    self.refmap(|v| &v.$num),
                )+)
            }
        }

        impl<$($output_type,)+ E> MultiAnchor<E> for ($(&Anchor<$output_type, E>,)+)
        where
            $(
                $output_type: 'static,
            )+
            E: Engine,
        {
            type Target = ($(Anchor<$output_type, E>,)+);

            #[track_caller]
            fn map<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                map::Map<Self::Target, F, Out>: AnchorInner<E, Output=Out>,
            {
                E::mount(map::Map {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    output: None,
                    output_stale: true,
                    location: Location::caller(),
                })
            }

            #[track_caller]
            fn map_mut<F, Out>(self, initial: Out, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                map_mut::MapMut<Self::Target, F, Out>: AnchorInner<E, Output=Out>,
            {
                E::mount(map_mut::MapMut {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    output: initial,
                    output_stale: true,
                    location: Location::caller(),
                })
            }

            #[track_caller]
            fn then<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                F: 'static,
                Out: 'static,
                then::Then<Self::Target, Out, F, E>: AnchorInner<E, Output=Out>,
            {
                E::mount(then::Then {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    f_anchor: None,
                    location: Location::caller(),
                    lhs_stale: true,
                })
            }

            #[track_caller]
            fn refmap<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                refmap::RefMap<Self::Target, F>: AnchorInner<E, Output = Out>,
            {
                E::mount(refmap::RefMap {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    location: Location::caller(),
                })
            }

            #[track_caller]
            fn cutoff<F, Out>(self, f: F) -> Anchor<Out, E>
            where
                Out: 'static,
                F: 'static,
                cutoff::Cutoff<Self::Target, F>: AnchorInner<E, Output = Out>,
            {
                E::mount(cutoff::Cutoff {
                    anchors: ($(self.$num.clone(),)+),
                    f,
                    location: Location::caller(),
                })
            }
        }
    }
}

impl_tuple_ext! {
    [O0, 0]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
}

impl_tuple_ext! {
    [O0, 0]
    [O1, 1]
    [O2, 2]
    [O3, 3]
    [O4, 4]
    [O5, 5]
    [O6, 6]
    [O7, 7]
    [O8, 8]
}
